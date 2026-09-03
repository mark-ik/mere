// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::{
    DocumentFormat, KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentSession,
    KnotDocumentSnapshotV1, KnotDocumentSourceKindV1, KnotDocumentWritePostureV1,
};
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, RunnerSurfaceSession, TextInput,
    button, div, el, lens, span, textarea_typed,
};
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};

pub const KNOT_DOCUMENT_CSS: &str = ".knot-document { display: flex; flex-direction: column; gap: 8px; } .knot-document-status { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; } .knot-document-status-item { white-space: nowrap; } .knot-document-save { margin-left: auto; } .knot-document-body { min-height: 240px; white-space: pre-wrap; } .knot-document-read-only { cursor: default; user-select: text; }";
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
    fn input_mut_for_editable_view(&mut self) -> &mut TextInput {
        self.session.input_mut_for_editable_view()
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
    let read_only = snapshot.write_posture == KnotDocumentWritePostureV1::ReadOnly;
    let save_affordance: Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>> =
        if read_only {
            Box::new(
                span("Save disabled: read-only")
                    .attr("class", "knot-document-status-item")
                    .attr("aria-live", "polite"),
            )
        } else {
            Box::new(
                button("Save", |state: &mut KnotDocumentSurfaceState, _| {
                    let _ = state.apply(KnotDocumentIntentV1::Save);
                })
                .attr("class", "knot-document-save"),
            )
        };
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
        save_affordance,
    ))
    .attr("class", "knot-document-status");
    let body: Box<dyn AnyView<KnotDocumentSurfaceState, (), GenetCtx, GenetElement>> = if read_only
    {
        Box::new(
            div(snapshot.text)
                .attr("class", "knot-document-body knot-document-read-only")
                .attr("role", "document")
                .attr("aria-label", "Read-only document text")
                .attr("aria-readonly", "true"),
        )
    } else {
        Box::new(
            el(
                "div",
                lens(
                    |input: &mut TextInput| textarea_typed(input),
                    |state: &mut KnotDocumentSurfaceState| state.input_mut_for_editable_view(),
                ),
            )
            .attr("class", "knot-document-body")
            .attr("role", "textbox")
            .attr("aria-label", "Document text"),
        )
    };
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
        KnotDocumentWritePostureV1::ReadOnly => "read-only",
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
    if let Some(refusal) = snapshot.refusal {
        format!("{outcome}: {}", refusal_label(refusal))
    } else if let Some(failure) = &snapshot.last_save_failure {
        format!("{outcome} ({})", failure.message)
    } else {
        outcome.to_owned()
    }
}

fn refusal_label(refusal: crate::KnotDocumentRefusalV1) -> &'static str {
    match refusal {
        crate::KnotDocumentRefusalV1::ScratchHasNoSaveTarget => {
            "scratch document has no file target"
        }
        crate::KnotDocumentRefusalV1::ReadOnly => "document is read-only",
    }
}
#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use genet_scripted_dom::ScriptedDom;
    use layout_dom_api::LayoutDom;

    use super::*;

    fn contains_element(dom: &ScriptedDom, node: genet_scripted_dom::NodeId, name: &str) -> bool {
        dom.element_name(node)
            .is_some_and(|qualified| qualified.local.as_ref() == name)
            || dom
                .dom_children(node)
                .any(|child| contains_element(dom, child, name))
    }
    #[test]
    fn state_uses_the_session_input_as_the_component_buffer() {
        let mut state =
            KnotDocumentSurfaceState::new(KnotDocumentSession::scratch("memory:test", "hello"));
        let first = state.session().input() as *const TextInput;
        let second = state.input_mut_for_editable_view() as *mut TextInput;
        assert_eq!(first, second.cast_const());
    }

    #[test]
    fn read_only_posture_has_explicit_visible_labels() {
        let snapshot = KnotDocumentSession::read_only("memory:test", "hello").snapshot();
        assert_eq!(posture_label(snapshot.write_posture), "read-only");
        assert_eq!(
            save_outcome_label(&KnotDocumentSnapshotV1 {
                refusal: Some(crate::KnotDocumentRefusalV1::ReadOnly),
                last_save_outcome: Some(crate::KnotDocumentSaveOutcomeV1::Refused),
                ..snapshot
            }),
            "Save: refused: document is read-only"
        );
    }

    #[test]
    fn read_only_view_has_no_editable_textbox_or_save_button() {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = KnotDocumentSurfaceState::new(KnotDocumentSession::read_only(
            "memory:field",
            "# Field\n",
        ));
        let runner = GenetAppRunner::new(dom.clone(), knot_document_view, state);
        let rendered = dom.borrow();
        let body = rendered
            .all_with_class(rendered.document(), "knot-document-read-only")
            .into_iter()
            .next()
            .expect("read-only body");
        assert_eq!(
            rendered
                .element_name(body)
                .map(|name| name.local.to_string()),
            Some("div".to_owned())
        );
        assert!(
            !contains_element(&rendered, runner.root(), "textarea"),
            "a read-only document must not retain an editable text control"
        );
        assert!(
            rendered
                .all_with_class(rendered.document(), "knot-document-save")
                .is_empty(),
            "a read-only document must not render a save button"
        );
    }
}
