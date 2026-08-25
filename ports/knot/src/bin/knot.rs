//! Thin standalone desktop host for the shared Knot document surface.
//!
//! The product owns the document session and view.  Genet's Cambium desktop
//! host owns the window, event loop, layout, paint, input, IME, and a11y path.

use std::ffi::OsString;
use std::path::PathBuf;

use cambium_genet_winit_host::{
    CloseDisposition, FocusedTextSlot, HostHooks, HostOptions, Init, Key, KeyPress, Runner,
    inert_hooks, run,
};
use knot_editor::{
    KNOT_DOCUMENT_CSS, KnotDocumentIntentV1, KnotDocumentSession, KnotDocumentSurfaceState,
    KnotDocumentView, knot_document_view,
};
use layout_dom_api::LayoutDom;

const SCRATCH_ADDRESS: &str = "scratch:untitled";

#[derive(Debug, PartialEq, Eq)]
enum DocumentSelection {
    Scratch,
    File(PathBuf),
}

fn select_document<I>(args: I) -> Result<DocumentSelection, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    let path = args.next();
    if args.next().is_some() {
        return Err("expected zero or one document path".to_owned());
    }
    Ok(match path {
        Some(path) => DocumentSelection::File(PathBuf::from(path)),
        None => DocumentSelection::Scratch,
    })
}

fn open_selection(selection: DocumentSelection) -> Result<KnotDocumentSession, String> {
    match selection {
        DocumentSelection::Scratch => Ok(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "")),
        DocumentSelection::File(path) => KnotDocumentSession::open(path),
    }
}

fn is_save_chord(press: &KeyPress) -> bool {
    press.modifiers.is_command_chord()
        && matches!(&press.key, Key::Character(key) if key.eq_ignore_ascii_case("s"))
}

fn focused_text(
    runner: &Runner<
        KnotDocumentSurfaceState,
        fn(&KnotDocumentSurfaceState) -> KnotDocumentView,
        KnotDocumentView,
    >,
) -> Option<FocusedTextSlot<KnotDocumentSurfaceState>> {
    let focused = runner.focus()?;
    let dom = runner.dom();
    let dom_ref = dom.borrow();
    let is_document_textarea = LayoutDom::element_name(&*dom_ref, focused)
        .is_some_and(|name| name.local.as_ref() == "textarea");
    drop(dom_ref);
    if !is_document_textarea {
        return None;
    }
    Some(FocusedTextSlot {
        node: focused,
        get: Box::new(|state: &KnotDocumentSurfaceState| state.session().input()),
        get_mut: Box::new(|state: &mut KnotDocumentSurfaceState| state.session_mut().input_mut()),
    })
}

fn host_hooks() -> HostHooks<
    KnotDocumentSurfaceState,
    fn(&KnotDocumentSurfaceState) -> KnotDocumentView,
    KnotDocumentView,
> {
    let mut hooks = inert_hooks();
    hooks.close_request = Box::new(|_, _| CloseDisposition::Exit);
    hooks.focused_text = Box::new(focused_text);
    hooks.key_intercept = Box::new(|runner, press| {
        if !is_save_chord(press) {
            return false;
        }
        runner.update(|state| {
            let _ = state.apply(KnotDocumentIntentV1::Save);
        });
        true
    });
    hooks
}

fn run_standalone(session: KnotDocumentSession) -> Result<(), String> {
    let options = HostOptions {
        title: "Knot".to_owned(),
        initial_logical_size: (1_100.0, 700.0),
        ..HostOptions::default()
    };
    run(
        options,
        move |_, _, _| Init {
            state: KnotDocumentSurfaceState::new(session),
            logic: knot_document_view,
            sheet: KNOT_DOCUMENT_CSS.to_owned(),
        },
        host_hooks(),
    )
    .map_err(|error| error.to_string())
}

fn main() {
    let selection = match select_document(std::env::args_os()) {
        Ok(selection) => selection,
        Err(error) => {
            eprintln!("knot: {error}");
            std::process::exit(2);
        }
    };
    let session = match open_selection(selection) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("knot: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = run_standalone(session) {
        eprintln!("knot: host failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use cambium_genet_winit_host::{CloseRequest, Harness};
    use genet_probe::Selector;
    use layout_dom_api::LayoutDom;
    use tempfile::tempdir;

    use cambium_genet_winit_host::{Modifiers, NamedKey};

    use super::*;

    fn arg(value: &str) -> OsString {
        OsString::from(value)
    }

    #[test]
    fn zero_arguments_selects_scratch() {
        assert_eq!(
            select_document([arg("knot")]).unwrap(),
            DocumentSelection::Scratch
        );
        let session = open_selection(DocumentSelection::Scratch).unwrap();
        let snapshot = session.snapshot();
        assert_eq!(snapshot.source.address, SCRATCH_ADDRESS);
        assert!(snapshot.source.kind == knot_editor::KnotDocumentSourceKindV1::Scratch);
    }

    #[test]
    fn one_path_is_selected_and_extra_arguments_are_rejected() {
        assert_eq!(
            select_document([arg("knot"), arg("note.djot")]).unwrap(),
            DocumentSelection::File(PathBuf::from("note.djot"))
        );
        assert_eq!(
            select_document([arg("knot"), arg("a.djot"), arg("b.djot")]),
            Err("expected zero or one document path".to_owned())
        );
    }

    #[test]
    fn save_chord_consumes_control_and_command_s_only() {
        let ctrl = KeyPress::character("s").with_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        let command = KeyPress::character("S").with_modifiers(Modifiers {
            meta: true,
            ..Modifiers::NONE
        });
        let plain = KeyPress::character("s");
        let named = KeyPress::named(NamedKey::Other).with_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        assert!(is_save_chord(&ctrl));
        assert!(is_save_chord(&command));
        assert!(!is_save_chord(&plain));
        assert!(!is_save_chord(&named));
    }

    #[test]
    fn host_hook_construction_is_headless_and_deterministic() {
        let _hooks: HostHooks<
            KnotDocumentSurfaceState,
            fn(&KnotDocumentSurfaceState) -> KnotDocumentView,
            KnotDocumentView,
        > = host_hooks();
    }

    #[test]
    fn app_authored_open_edit_save_close_reopen_receipt() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("receipt.djot");
        fs::write(&path, "# Receipt\n").unwrap();

        let session = KnotDocumentSession::open(&path).unwrap();
        let init = Init {
            state: KnotDocumentSurfaceState::new(session),
            logic: knot_document_view,
            sheet: KNOT_DOCUMENT_CSS.to_owned(),
        };
        let mut harness = Harness::with_hooks(init, host_hooks());
        harness.layout_at(900.0, 640.0);

        let editor = Selector::role("textbox").containing("Document text");
        assert!(
            harness.click_on(&editor),
            "semantic editor selector resolves"
        );
        let focused = harness.focus().expect("semantic click focuses editor");
        harness.with_dom(|dom| {
            assert_eq!(
                LayoutDom::element_name(dom, focused)
                    .expect("focused editor has an element name")
                    .local
                    .as_ref(),
                "textarea"
            );
        });

        harness.key_injected("Body\n");
        assert!(harness.state().snapshot().dirty);

        harness.set_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        harness.key_char("s");
        harness.set_modifiers(Modifiers::NONE);

        let saved = harness.state().snapshot();
        assert!(!saved.dirty);
        assert_eq!(
            saved.last_save_outcome,
            Some(knot_editor::KnotDocumentSaveOutcomeV1::Written)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), saved.text);

        harness.request_close(CloseRequest::Native);
        assert!(!harness.hidden(), "close policy exits rather than hiding");
        drop(harness);

        let reopened = KnotDocumentSession::open(&path).unwrap();
        let reopened_init = Init {
            state: KnotDocumentSurfaceState::new(reopened),
            logic: knot_document_view,
            sheet: KNOT_DOCUMENT_CSS.to_owned(),
        };
        let reopened_harness = Harness::with_hooks(reopened_init, host_hooks());
        assert_eq!(reopened_harness.state().snapshot().text, saved.text);
        assert!(!reopened_harness.state().snapshot().dirty);
    }
}
