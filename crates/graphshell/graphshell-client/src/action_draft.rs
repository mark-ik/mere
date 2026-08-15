//! Host-owned editable state for one endpoint-advertised action.
//!
//! The portable protocol deliberately carries only an action descriptor and
//! bounded choices. A host needs a separate object for the user's selections,
//! validation result, and the snapshot position at which it may invoke. This
//! module supplies that object without choosing a renderer, persistence model,
//! or endpoint authority.

use std::collections::BTreeMap;

use chirograph::{
    ActionFormChoiceV1, ActionFormError, AdvertisedAction, IntentInvocation, ProjectionSession,
    Revision, SceneEpoch,
};
use sceno::InstanceId;

/// The snapshot-local target an action draft will invoke when it is submitted.
///
/// This is host state, not a portable form field: endpoints still validate the
/// session, target, epoch, revision, payload, and authority independently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionDraftTarget {
    pub session: ProjectionSession,
    pub target: InstanceId,
    pub observed_epoch: SceneEpoch,
    pub observed_revision: Revision,
}

/// One choice as a renderer may present it. `value` remains the exact opaque
/// endpoint value, while label and description are presentation data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionDraftChoice {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    pub selected: bool,
}

/// One bounded choice field as a renderer or accessibility bridge may present
/// it. The protocol only has choice fields in v1, so this stays concrete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionDraftField {
    pub name: String,
    pub label: String,
    pub description: String,
    pub required: bool,
    pub choices: Vec<ActionDraftChoice>,
}

/// Renderer-neutral semantic projection of a draft.
///
/// A browser may turn this into labelled choice controls; a native host may
/// expose the same labels, required state, and selected values through its
/// accessibility tree. Neither projection gets to reinterpret opaque values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionDraftSemantics {
    pub label: String,
    pub explanation: String,
    pub fields: Vec<ActionDraftField>,
    pub error: Option<String>,
    pub submit_label: String,
}

/// A user-editable action descriptor plus its chosen exact values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionDraft {
    action: AdvertisedAction,
    values: BTreeMap<String, String>,
    error: Option<String>,
}

impl ActionDraft {
    /// Open a draft for an action whose form has already been disclosed by an
    /// endpoint. Static form defects remain visible as a validation error at
    /// submit time, rather than becoming host-defined defaults.
    pub fn new(action: AdvertisedAction) -> Self {
        Self {
            action,
            values: BTreeMap::new(),
            error: None,
        }
    }

    pub fn action(&self) -> &AdvertisedAction {
        &self.action
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Select one advertised value. Unknown field names and values are
    /// rejected immediately, before a host ever attempts an endpoint call.
    pub fn choose(
        &mut self,
        field_name: impl AsRef<str>,
        value: impl AsRef<str>,
    ) -> Result<(), ActionFormError> {
        let field_name = field_name.as_ref();
        let value = value.as_ref();
        let result = (|| {
            let form = self
                .action
                .input_form
                .as_ref()
                .ok_or(ActionFormError::NoInputForm)?;
            form.validate()?;
            let field = form
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .ok_or_else(|| ActionFormError::UnknownField(field_name.to_string()))?;
            if !field.choices.iter().any(|choice| choice.value == value) {
                return Err(ActionFormError::InvalidChoice {
                    field: field_name.to_string(),
                    value: value.to_string(),
                });
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.values
                    .insert(field_name.to_string(), value.to_string());
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Compose an invocation at the exact snapshot position where this draft
    /// was opened. A stale response is therefore ordinary endpoint truth, not
    /// a host guess about whether a selection remains valid.
    pub fn invocation(
        &mut self,
        target: &ActionDraftTarget,
    ) -> Result<IntentInvocation, ActionFormError> {
        match self.action.compose_payload(&self.values) {
            Ok(payload) => {
                self.error = None;
                Ok(IntentInvocation {
                    session: target.session.clone(),
                    target: target.target,
                    observed_epoch: target.observed_epoch,
                    observed_revision: target.observed_revision,
                    intent: self.action.intent.0.clone(),
                    payload,
                })
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Return the shared renderer/accessibility model for the current draft.
    pub fn semantics(&self) -> ActionDraftSemantics {
        let fields = self
            .action
            .input_form
            .as_ref()
            .map(|form| {
                form.fields
                    .iter()
                    .map(|field| ActionDraftField {
                        name: field.name.clone(),
                        label: field.label.clone(),
                        description: field.description.clone(),
                        required: field.required,
                        choices: field
                            .choices
                            .iter()
                            .map(|choice| self.choice_semantics(&field.name, choice))
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        ActionDraftSemantics {
            label: self.action.label.clone(),
            explanation: self.action.explanation.clone(),
            fields,
            error: self.error.clone(),
            submit_label: self.action.label.clone(),
        }
    }

    fn choice_semantics(&self, field_name: &str, choice: &ActionFormChoiceV1) -> ActionDraftChoice {
        ActionDraftChoice {
            value: choice.value.clone(),
            label: choice.label.clone(),
            description: choice.description.clone(),
            selected: self.values.get(field_name) == Some(&choice.value),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chirograph::{
        ActionFormChoiceV1, ActionFormFieldV1, ActionFormV1, IntentEffect, IntentReference,
    };

    use super::*;

    fn action() -> AdvertisedAction {
        AdvertisedAction {
            intent: IntentReference("fixture.choose".into()),
            label: "Inspect map tile".into(),
            explanation: "Choose an exact map detail.".into(),
            payload_schema: "fixture.choose/v1".into(),
            input_form: Some(ActionFormV1::new("fixture.choose/v1").with_field(
                ActionFormFieldV1::choice(
                    "inspection_scope",
                    "Inspect",
                    [
                        ActionFormChoiceV1::new("outline", "Coast outline"),
                        ActionFormChoiceV1::new("coordinates", "Field coordinates"),
                    ],
                ),
            )),
            effect: IntentEffect::Curation,
        }
    }

    fn target() -> ActionDraftTarget {
        ActionDraftTarget {
            session: ProjectionSession("fixture".into()),
            target: InstanceId(7),
            observed_epoch: SceneEpoch(3),
            observed_revision: Revision(9),
        }
    }

    #[test]
    fn draft_requires_an_advertised_selection_before_it_composes() {
        let mut draft = ActionDraft::new(action());
        assert_eq!(
            draft.invocation(&target()),
            Err(ActionFormError::MissingField("inspection_scope".into()))
        );
        assert_eq!(
            draft.error(),
            Some("action form requires field inspection_scope")
        );
    }

    #[test]
    fn draft_keeps_only_exact_advertised_choice_values() {
        let mut draft = ActionDraft::new(action());
        assert_eq!(
            draft.choose("inspection_scope", "invented"),
            Err(ActionFormError::InvalidChoice {
                field: "inspection_scope".into(),
                value: "invented".into(),
            })
        );
        draft
            .choose("inspection_scope", "coordinates")
            .expect("advertised choice");
        assert_eq!(
            draft.values(),
            &BTreeMap::from([("inspection_scope".into(), "coordinates".into())])
        );
    }

    #[test]
    fn semantics_and_invocation_share_the_same_selected_value() {
        let mut draft = ActionDraft::new(action());
        draft
            .choose("inspection_scope", "outline")
            .expect("advertised choice");
        let semantics = draft.semantics();
        assert_eq!(semantics.fields.len(), 1);
        assert!(semantics.fields[0].choices[0].selected);
        assert!(!semantics.fields[0].choices[1].selected);

        let invocation = draft.invocation(&target()).expect("complete draft");
        assert_eq!(invocation.session, ProjectionSession("fixture".into()));
        assert_eq!(invocation.target, InstanceId(7));
        assert_eq!(invocation.observed_epoch, SceneEpoch(3));
        assert_eq!(invocation.observed_revision, Revision(9));
        assert_eq!(invocation.intent, "fixture.choose");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&invocation.payload).unwrap(),
            serde_json::json!({
                "schema": "fixture.choose/v1",
                "inspection_scope": "outline",
            })
        );
    }
}
