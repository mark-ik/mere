use crate::{
    DocumentFormat, KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentSession,
    KnotDocumentSnapshotV1, KnotDocumentSourceKindV1, KnotDocumentWritePostureV1,
};
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, RunnerSurfaceSession, TextInput,
    button, div, el, lens, span, textarea_typed,
};
use genet_host_api::{
    CapabilityId, PlacementHint, ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor,
    SurfaceId, SurfaceMultiplicity, SurfaceRole, SurfaceSourceShape,
};

pub const KNOT_DOCUMENT_CSS: &str = ".knot-document { display: flex; flex-direction: column; gap: 8px; } .knot-document-status { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; } .knot-document-status-item { white-space: nowrap; } .knot-document-save { margin-left: auto; } .knot-document-body { min-height: 240px; white-space: pre-wrap; }";
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
    pub fn apply(
        &mut self,
        intent: KnotDocumentIntentV1,
    ) -> Result<KnotDocumentSnapshotV1, KnotDocumentIntentErrorV1> {
        self.session.apply(intent)
    }
}
pub type KnotDocumentView = Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>>;
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
        span(save_outcome_label(&snapshot)).attr("class", "knot-document-status-item"),
        button("Save", |state: &mut KnotDocumentSurfaceState, _| {
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
pub fn knot_document_surface(
    dom: DomHandle,
    state: KnotDocumentSurfaceState,
) -> Box<dyn cambium::RetainedSurfaceSession> {
    let runner = GenetAppRunner::new(dom, knot_document_view, state);
    Box::new(RunnerSurfaceSession::new(
        knot_document_descriptor(),
        runner,
        |state: &KnotDocumentSurfaceState| match state.snapshot().source.kind {
            KnotDocumentSourceKindV1::File | KnotDocumentSourceKindV1::Scratch => {
                SurfaceAvailability::Available
            }
        },
        |_state, _viewport| {},
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
        None => "Save: not attempted",
        Some(crate::KnotDocumentSaveOutcomeV1::Written) => "Save: written",
        Some(crate::KnotDocumentSaveOutcomeV1::Unchanged) => "Save: unchanged",
        Some(crate::KnotDocumentSaveOutcomeV1::Refused) => "Save: refused",
        Some(crate::KnotDocumentSaveOutcomeV1::Failed) => "Save: failed",
    };
    if snapshot.refusal.is_some() {
        format!("{outcome}: scratch document has no file target")
    } else if let Some(failure) = &snapshot.last_save_failure {
        format!("{outcome} ({})", failure.message)
    } else {
        outcome.to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_uses_the_session_input_as_the_component_buffer() {
        let mut state =
            KnotDocumentSurfaceState::new(KnotDocumentSession::scratch("memory:test", "hello"));
        let first = state.session().input() as *const TextInput;
        let second = state.session_mut().input_mut() as *mut TextInput;
        assert_eq!(first, second.cast_const());
    }
}
