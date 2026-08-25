//! Thin standalone host for the reusable Knot document surface.
use cambium_genet_winit_host::{
    CloseDisposition, FocusedTextSlot, HostHooks, HostOptions, Init, Key, KeyPress, Runner,
    inert_hooks, run,
};
use knot_document::{
    KNOT_DOCUMENT_CSS, KnotDocumentIntentV1, KnotDocumentSession, KnotDocumentSurfaceState,
    KnotDocumentView, knot_document_view,
};
use layout_dom_api::LayoutDom;
use std::ffi::OsString;
use std::path::PathBuf;
const SCRATCH_ADDRESS: &str = "scratch:untitled";
#[derive(Debug, PartialEq, Eq)]
enum DocumentSelection {
    Scratch,
    File(PathBuf),
}
fn select_document<I: IntoIterator<Item = OsString>>(args: I) -> Result<DocumentSelection, String> {
    let mut args = args.into_iter();
    let _ = args.next();
    let path = args.next();
    if args.next().is_some() {
        return Err("expected zero or one document path".into());
    }
    Ok(path
        .map(|path| DocumentSelection::File(PathBuf::from(path)))
        .unwrap_or(DocumentSelection::Scratch))
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
    let textarea = LayoutDom::element_name(&*dom_ref, focused)
        .is_some_and(|name| name.local.as_ref() == "textarea");
    drop(dom_ref);
    textarea.then(|| FocusedTextSlot {
        node: focused,
        get: Box::new(|state| state.session().input()),
        get_mut: Box::new(|state| state.session_mut().input_mut()),
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
    run(
        HostOptions {
            title: "Knot".into(),
            initial_logical_size: (1100.0, 700.0),
            ..HostOptions::default()
        },
        move |_, _, _| Init {
            state: KnotDocumentSurfaceState::new(session),
            logic: knot_document_view,
            sheet: KNOT_DOCUMENT_CSS.into(),
        },
        host_hooks(),
    )
    .map_err(|error| error.to_string())
}
fn main() {
    let session = select_document(std::env::args_os())
        .and_then(open_selection)
        .unwrap_or_else(|error| {
            eprintln!("knot: {error}");
            std::process::exit(1)
        });
    if let Err(error) = run_standalone(session) {
        eprintln!("knot: host failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::{CloseRequest, Harness, Modifiers};
    use genet_probe::Selector;
    use tempfile::tempdir;
    #[test]
    fn app_authored_open_edit_save_close_reopen_receipt() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("receipt.djot");
        std::fs::write(&path, "# Receipt\n").unwrap();
        let init = Init {
            state: KnotDocumentSurfaceState::new(KnotDocumentSession::open(&path).unwrap()),
            logic: knot_document_view,
            sheet: KNOT_DOCUMENT_CSS.into(),
        };
        let mut harness = Harness::with_hooks(init, host_hooks());
        harness.layout_at(900.0, 640.0);
        assert!(harness.click_on(&Selector::role("textbox").containing("Document text")));
        harness.key_injected("Body\n");
        assert!(harness.state().snapshot().dirty);
        harness.set_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        harness.key_char("s");
        assert!(!harness.state().snapshot().dirty);
        harness.request_close(CloseRequest::Native);
        drop(harness);
        assert!(
            KnotDocumentSession::open(&path)
                .unwrap()
                .snapshot()
                .text
                .contains("Body")
        );
    }
}
