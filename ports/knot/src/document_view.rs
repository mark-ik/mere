//! The reusable Cambium view for one Knot document session.
//!
//! Knot owns the document authority and the component owns only its product
//! presentation.  In particular, the body is the session's retained
//! [`cambium::TextInput`], reached through [`KnotDocumentSession::input_mut`].
//! There is no second `String` or editor buffer in this module.

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, RunnerSurfaceSession, TextInput,
    button, div, el, lens, span, textarea_typed,
};
use genet_host_api::{
    CapabilityId, PlacementHint, ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor,
    SurfaceId, SurfaceMultiplicity, SurfaceRole, SurfaceSourceShape,
};

use crate::document_surface::{
    KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentSession, KnotDocumentSnapshotV1,
    KnotDocumentSourceKindV1, KnotDocumentWritePostureV1,
};
use crate::writer::DocumentFormat;

/// CSS selectors used by the Knot document component.
///
/// The host may concatenate this sheet with its product/theme sheet.  Layout
/// remains a host concern; these rules only establish the compact status/header
/// and the document body's ordinary editable surface.
pub const KNOT_DOCUMENT_CSS: &str = r#"
.knot-document { display: flex; flex-direction: column; gap: 8px; }
.knot-document-status { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.knot-document-status-item { white-space: nowrap; }
.knot-document-save { margin-left: auto; }
.knot-document-body { min-height: 240px; white-space: pre-wrap; }
.knot-document-save-outcome { white-space: nowrap; }
"#;

/// The concrete product state retained by the Knot document surface.
pub struct KnotDocumentSurfaceState {
    session: KnotDocumentSession,
}

impl KnotDocumentSurfaceState {
    pub fn new(session: KnotDocumentSession) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &KnotDocumentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut KnotDocumentSession {
        &mut self.session
    }

    pub fn snapshot(&self) -> KnotDocumentSnapshotV1 {
        self.session.snapshot()
    }

    /// Apply a product intent and retain any refusal/failure for the next
    /// render.  UI actions use this method rather than exposing the intent to
    /// the host.
    pub fn apply(
        &mut self,
        intent: KnotDocumentIntentV1,
    ) -> Result<KnotDocumentSnapshotV1, KnotDocumentIntentErrorV1> {
        self.session.apply(intent)
    }
}

/// The view's erased concrete type, useful to standalone hosts that retain the
/// product runner themselves.
pub type KnotDocumentView = Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>>;

/// Build the shared Knot document component.
pub fn knot_document_view(state: &KnotDocumentSurfaceState) -> KnotDocumentView {
    let snapshot = state.snapshot();
    let status = div((
        span(format!("Source: {}", snapshot.display_label))
            .attr("class", "knot-document-status-item"),
        span(format!("Format: {}", format_label(snapshot.format)))
            .attr("class", "knot-document-status-item"),
        span(if snapshot.dirty { "Dirty" } else { "Clean" })
            .attr("class", "knot-document-status-item"),
        span(format!(
            "Posture: {}",
            posture_label(snapshot.write_posture)
        ))
        .attr("class", "knot-document-status-item"),
        span(save_outcome_label(&snapshot)).attr(
            "class",
            "knot-document-status-item knot-document-save-outcome",
        ),
        button("Save", |state: &mut KnotDocumentSurfaceState, _| {
            // Save is a normal product action.  The host sees only the
            // resulting redraw, never KnotDocumentIntentV1.
            let _ = state.apply(KnotDocumentIntentV1::Save);
        })
        .attr("class", "knot-document-save"),
    ))
    .attr("class", "knot-document-status");

    let body = el(
        "div",
        lens(
            |input: &mut TextInput| textarea_typed(input),
            |state: &mut KnotDocumentSurfaceState| state.session.input_mut(),
        ),
    )
    .attr("class", "knot-document-body")
    .attr("role", "textbox")
    .attr("aria-label", "Document text");

    Box::new(
        el("section", (status, body))
            .attr("class", "knot-document")
            .attr("data-surface", "knot.document.v1"),
    )
}

/// Stable, data-only description of the shared Knot document surface.
pub fn knot_document_descriptor() -> SurfaceDescriptor {
    SurfaceDescriptor {
        provider_id: ProviderId::from("knot"),
        surface_id: SurfaceId::from("knot.document.v1"),
        label: "Knot document".to_owned(),
        accepted_source: SurfaceSourceShape::One(SourceKindId::from("knot.document.v1")),
        roles: vec![SurfaceRole::from("document"), SurfaceRole::from("editor")],
        multiplicity: SurfaceMultiplicity::PerSource,
        placement_hint: PlacementHint::from("main"),
        potential_capabilities: vec![CapabilityId::from("edit"), CapabilityId::from("save")],
    }
}

/// Construct the real retained Genet runner for a Knot document.
///
/// The caller supplies the host-owned DOM handle.  This keeps DOM allocation
/// and lifetime with the host while Knot still owns the concrete runner and
/// product state inside the erased session.
pub fn knot_document_surface(
    dom: DomHandle,
    state: KnotDocumentSurfaceState,
) -> Box<dyn cambium::RetainedSurfaceSession> {
    let runner = GenetAppRunner::new(dom, knot_document_view, state);
    Box::new(RunnerSurfaceSession::new(
        knot_document_descriptor(),
        runner,
        |state: &KnotDocumentSurfaceState| {
            // Both admitted file and scratch documents are editable and
            // available.  Admission failures happen before construction.
            match state.snapshot().source.kind {
                KnotDocumentSourceKindV1::File | KnotDocumentSourceKindV1::Scratch => {
                    SurfaceAvailability::Available
                }
            }
        },
        |_state: &mut KnotDocumentSurfaceState, _viewport| {},
        |_action: ()| Vec::new(),
    ))
}

fn format_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Djot => "Djot",
        DocumentFormat::Knot => "legacy .knot",
        DocumentFormat::Markdown => "Markdown",
        DocumentFormat::Json => "JSON",
    }
}

fn posture_label(posture: KnotDocumentWritePostureV1) -> &'static str {
    match posture {
        KnotDocumentWritePostureV1::FileTarget => "file target",
        KnotDocumentWritePostureV1::Scratch => "scratch",
    }
}

fn save_outcome_label(snapshot: &KnotDocumentSnapshotV1) -> String {
    let outcome = match snapshot.last_save_outcome {
        None => "Save: not attempted".to_owned(),
        Some(crate::KnotDocumentSaveOutcomeV1::Written) => "Save: written".to_owned(),
        Some(crate::KnotDocumentSaveOutcomeV1::Unchanged) => "Save: unchanged".to_owned(),
        Some(crate::KnotDocumentSaveOutcomeV1::Refused) => "Save: refused".to_owned(),
        Some(crate::KnotDocumentSaveOutcomeV1::Failed) => "Save: failed".to_owned(),
    };
    if let Some(refusal) = snapshot.refusal {
        let reason = match refusal {
            crate::KnotDocumentRefusalV1::ScratchHasNoSaveTarget => {
                "scratch document has no file target"
            }
        };
        return format!("{outcome}: {reason}");
    }
    if let Some(failure) = &snapshot.last_save_failure {
        return format!("{outcome} ({})", failure.message);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use cambium::TextCommand;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn state_uses_the_session_input_as_the_component_buffer() {
        let session = KnotDocumentSession::scratch("memory:test", "hello");
        let mut state = KnotDocumentSurfaceState::new(session);
        let input_ptr = state.session().input() as *const TextInput;
        let borrowed_ptr = state.session_mut().input_mut() as *mut TextInput;
        assert_eq!(input_ptr, borrowed_ptr.cast_const());

        state
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                " world".into(),
            )))
            .unwrap();
        assert_eq!(state.snapshot().text, "hello world");
    }

    #[test]
    fn real_djot_edit_and_save_updates_visible_product_status() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("note.djot");
        std::fs::write(&path, "# Note\n").unwrap();
        let mut state = KnotDocumentSurfaceState::new(KnotDocumentSession::open(&path).unwrap());

        state
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Body\n".into(),
            )))
            .unwrap();
        assert!(state.snapshot().dirty);
        assert!(save_outcome_label(&state.snapshot()).contains("not attempted"));

        state.apply(KnotDocumentIntentV1::Save).unwrap();
        let saved = state.snapshot();
        assert!(!saved.dirty);
        assert_eq!(
            saved.last_save_outcome,
            Some(crate::KnotDocumentSaveOutcomeV1::Written)
        );
        assert_eq!(save_outcome_label(&saved), "Save: written");
    }

    #[test]
    fn scratch_is_available_but_save_refusal_is_visible() {
        let mut state = KnotDocumentSurfaceState::new(KnotDocumentSession::scratch(
            "memory:test",
            "# Scratch\n",
        ));
        let result = state.apply(KnotDocumentIntentV1::Save);
        assert!(result.is_err());
        assert!(matches!(
            state.snapshot().source.kind,
            KnotDocumentSourceKindV1::Scratch
        ));
        assert_eq!(
            save_outcome_label(&state.snapshot()),
            "Save: refused: scratch document has no file target"
        );
    }

    #[test]
    fn erased_surface_api_keeps_descriptor_dom_and_root_on_the_generic_trait() {
        let descriptor = knot_document_descriptor();
        assert_eq!(descriptor.surface_id.as_str(), "knot.document.v1");
        assert_eq!(descriptor.provider_id.as_str(), "knot");

        let constructor: fn(
            DomHandle,
            KnotDocumentSurfaceState,
        ) -> Box<dyn cambium::RetainedSurfaceSession> = knot_document_surface;
        let _ = constructor;

        fn generic_host_receipt(session: &dyn cambium::RetainedSurfaceSession) {
            let _ = session.descriptor();
            let _ = session.dom();
            let _ = session.root();
        }
        let _ = generic_host_receipt;
    }
}
