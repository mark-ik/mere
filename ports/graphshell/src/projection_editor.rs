// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-neutral projection authoring state.
//!
//! The editor owns a draft and the validation needed to turn it into a
//! definition. It deliberately has no graph, endpoint, or authority handle.
//! A host supplies [`ProjectionDefinitionSink`] when it elects to persist a
//! valid definition.

use serde::{Deserialize, Serialize};

/// The stable schema version for definitions emitted by this editor.
pub const PROJECTION_DEFINITION_VERSION: u16 = 1;

/// The plain panels a host may expose for projection authoring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPanel {
    #[default]
    Source,
    Reading,
    Encoding,
    Arrangement,
    Interaction,
    Preview,
    Provenance,
}

impl ProjectionPanel {
    pub const ALL: [Self; 7] = [
        Self::Source,
        Self::Reading,
        Self::Encoding,
        Self::Arrangement,
        Self::Interaction,
        Self::Preview,
        Self::Provenance,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Reading => "Reading",
            Self::Encoding => "Encoding",
            Self::Arrangement => "Arrangement",
            Self::Interaction => "Interaction",
            Self::Preview => "Preview",
            Self::Provenance => "Provenance",
        }
    }
}

/// A source and domain binding selected by the author.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceBinding {
    /// The authority or owner identifier, not an authority handle.
    pub authority: String,
    /// The domain or namespace in which the source is interpreted.
    pub domain: String,
    /// A source-local resource, dataset, or graph identifier.
    pub resource: String,
}

/// The facts and grain an arrangement reads from its source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// A registered reading id resolved by the host/catalog.
    pub kind: String,
    /// The identity field used to address records.
    pub key: String,
    /// The value field for a value-oriented reading.
    pub value: Option<String>,
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            kind: "nodes".into(),
            key: String::new(),
            value: None,
        }
    }
}

/// A field or literal assigned to a visual channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Channel {
    Field(String),
    Constant(String),
}

/// Encodings map reading fields to arrangement channels.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Encoding {
    pub x: Channel,
    pub y: Channel,
    pub color: Option<Channel>,
    pub label: Option<Channel>,
}

impl Default for Encoding {
    fn default() -> Self {
        Self {
            x: Channel::Field(String::new()),
            y: Channel::Field(String::new()),
            color: None,
            label: None,
        }
    }
}

/// The spatial or tabular arrangement requested by the author.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Arrangement {
    /// A registered arrangement id resolved by the host/catalog.
    pub kind: String,
    /// An open direction or coordinate parameter understood by that id.
    pub direction: String,
    /// Positive spacing in the arrangement's own units.
    pub spacing: u32,
}

impl Default for Arrangement {
    fn default() -> Self {
        Self {
            kind: "grid".into(),
            direction: "horizontal".into(),
            spacing: 16,
        }
    }
}

/// Interaction affordances offered by a realization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    pub selection: SelectionMode,
    pub pan: bool,
    pub zoom: bool,
}

impl Default for Interaction {
    fn default() -> Self {
        Self {
            selection: SelectionMode::Single,
            pan: true,
            zoom: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    None,
    Single,
    Multiple,
}

/// Appearance and realization choices. Rendering remains host-owned.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    /// A registered realization id. Rendering and resolution stay host-owned.
    pub realization: String,
    pub title: String,
    pub theme: String,
}

/// Human and machine provenance attached to a saved definition.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub author: String,
    pub source_revision: String,
    pub note: String,
}

/// The editable form. It intentionally retains incomplete values so a host
/// can show useful field-level validation instead of rejecting keystrokes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDraft {
    pub version: u16,
    pub id: String,
    pub label: String,
    pub source: SourceBinding,
    pub reading: Reading,
    pub encoding: Encoding,
    pub arrangement: Arrangement,
    pub interaction: Interaction,
    pub appearance: Appearance,
    pub provenance: Provenance,
}

/// The complete, validated, durable projection definition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDefinition {
    pub version: u16,
    pub id: String,
    pub label: String,
    pub source: SourceBinding,
    pub reading: Reading,
    pub encoding: Encoding,
    pub arrangement: Arrangement,
    pub interaction: Interaction,
    pub appearance: Appearance,
    pub provenance: Provenance,
}

impl ProjectionDraft {
    /// Start an empty draft with safe enum and layout defaults.
    pub fn new() -> Self {
        Self {
            version: PROJECTION_DEFINITION_VERSION,
            id: String::new(),
            label: String::new(),
            source: SourceBinding::default(),
            reading: Reading::default(),
            encoding: Encoding::default(),
            arrangement: Arrangement::default(),
            interaction: Interaction::default(),
            appearance: Appearance::default(),
            provenance: Provenance::default(),
        }
    }

    /// Validate and promote this draft to the durable definition.
    pub fn to_definition(&self) -> Result<ProjectionDefinition, Vec<ValidationIssue>> {
        self.validate()?;
        Ok(ProjectionDefinition {
            version: self.version,
            id: self.id.clone(),
            label: self.label.clone(),
            source: self.source.clone(),
            reading: self.reading.clone(),
            encoding: self.encoding.clone(),
            arrangement: self.arrangement.clone(),
            interaction: self.interaction.clone(),
            appearance: self.appearance.clone(),
            provenance: self.provenance.clone(),
        })
    }

    /// Check all required fields without contacting a host or authority.
    pub fn validate(&self) -> Result<(), Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        if self.version != PROJECTION_DEFINITION_VERSION {
            issues.push(ValidationIssue::error(
                "version",
                "unsupported projection definition version",
            ));
        }
        required(&mut issues, "id", &self.id, "an id is required");
        required(&mut issues, "label", &self.label, "a label is required");
        required(
            &mut issues,
            "source.authority",
            &self.source.authority,
            "an authority binding is required",
        );
        required(
            &mut issues,
            "source.domain",
            &self.source.domain,
            "a domain binding is required",
        );
        required(
            &mut issues,
            "source.resource",
            &self.source.resource,
            "a source resource is required",
        );
        required(
            &mut issues,
            "reading.kind",
            &self.reading.kind,
            "a registered reading id is required",
        );
        required(
            &mut issues,
            "reading.key",
            &self.reading.key,
            "a reading key is required",
        );
        if self.reading.kind == "values" {
            match self.reading.value.as_deref() {
                Some(value) if !value.trim().is_empty() => {}
                _ => issues.push(ValidationIssue::error(
                    "reading.value",
                    "a value field is required for a values reading",
                )),
            }
        }
        channel_required(&mut issues, "encoding.x", &self.encoding.x);
        channel_required(&mut issues, "encoding.y", &self.encoding.y);
        required(
            &mut issues,
            "arrangement.kind",
            &self.arrangement.kind,
            "a registered arrangement id is required",
        );
        required(
            &mut issues,
            "arrangement.direction",
            &self.arrangement.direction,
            "an arrangement direction or coordinate mode is required",
        );
        if self.arrangement.spacing == 0 {
            issues.push(ValidationIssue::error(
                "arrangement.spacing",
                "spacing must be greater than zero",
            ));
        }
        required(
            &mut issues,
            "appearance.realization",
            &self.appearance.realization,
            "a registered realization id is required",
        );
        required(
            &mut issues,
            "appearance.title",
            &self.appearance.title,
            "a preview title is required",
        );
        required(
            &mut issues,
            "provenance.author",
            &self.provenance.author,
            "an author is required",
        );
        required(
            &mut issues,
            "provenance.source_revision",
            &self.provenance.source_revision,
            "the source revision is required for reproducibility",
        );
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

impl Default for ProjectionDraft {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionDefinition {
    /// Serialize in declaration order using the existing serde JSON stack.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

fn required(issues: &mut Vec<ValidationIssue>, field: &str, value: &str, message: &str) {
    if value.trim().is_empty() {
        issues.push(ValidationIssue::error(field, message));
    }
}

fn channel_required(issues: &mut Vec<ValidationIssue>, field: &str, channel: &Channel) {
    let value = match channel {
        Channel::Field(value) | Channel::Constant(value) => value,
    };
    required(issues, field, value, "an encoding channel is required");
}

/// A field-specific validation problem suitable for a panel or summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub field: String,
    pub message: String,
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    fn error(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            severity: ValidationSeverity::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
}

/// Typed messages understood by the reducer. Hosts can map these to any UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorAction {
    SelectPanel(ProjectionPanel),
    SetId(String),
    SetLabel(String),
    SetSource(SourceBinding),
    SetReading(Reading),
    SetEncoding(Encoding),
    SetArrangement(Arrangement),
    SetInteraction(Interaction),
    SetAppearance(Appearance),
    SetProvenance(Provenance),
}

/// The result of reducing one editor action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReduceResult {
    Changed,
    PanelChanged,
}

/// Stateful editor boundary, independent of a widget toolkit or host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionEditor {
    draft: ProjectionDraft,
    panel: ProjectionPanel,
}

impl ProjectionEditor {
    pub fn new(draft: ProjectionDraft) -> Self {
        Self {
            draft,
            panel: ProjectionPanel::default(),
        }
    }

    pub fn draft(&self) -> &ProjectionDraft {
        &self.draft
    }

    pub fn panel(&self) -> ProjectionPanel {
        self.panel
    }

    pub fn reduce(&mut self, action: EditorAction) -> ReduceResult {
        match action {
            EditorAction::SelectPanel(panel) => {
                self.panel = panel;
                ReduceResult::PanelChanged
            }
            EditorAction::SetId(value) => {
                self.draft.id = value;
                ReduceResult::Changed
            }
            EditorAction::SetLabel(value) => {
                self.draft.label = value;
                ReduceResult::Changed
            }
            EditorAction::SetSource(value) => {
                self.draft.source = value;
                ReduceResult::Changed
            }
            EditorAction::SetReading(value) => {
                self.draft.reading = value;
                ReduceResult::Changed
            }
            EditorAction::SetEncoding(value) => {
                self.draft.encoding = value;
                ReduceResult::Changed
            }
            EditorAction::SetArrangement(value) => {
                self.draft.arrangement = value;
                ReduceResult::Changed
            }
            EditorAction::SetInteraction(value) => {
                self.draft.interaction = value;
                ReduceResult::Changed
            }
            EditorAction::SetAppearance(value) => {
                self.draft.appearance = value;
                ReduceResult::Changed
            }
            EditorAction::SetProvenance(value) => {
                self.draft.provenance = value;
                ReduceResult::Changed
            }
        }
    }

    pub fn validate(&self) -> Result<(), Vec<ValidationIssue>> {
        self.draft.validate()
    }

    /// Persist only a validated, immutable definition through host policy.
    pub fn save<S: ProjectionDefinitionSink>(
        &self,
        sink: &mut S,
    ) -> Result<(), SaveError<S::Error>> {
        let definition = self.draft.to_definition().map_err(SaveError::Invalid)?;
        sink.save(&definition).map_err(SaveError::Sink)
    }
}

/// Host persistence is the only effect exposed by the editor.
pub trait ProjectionDefinitionSink {
    type Error;

    fn save(&mut self, definition: &ProjectionDefinition) -> Result<(), Self::Error>;
}

#[derive(Debug, Eq, PartialEq)]
pub enum SaveError<E> {
    Invalid(Vec<ValidationIssue>),
    Sink(E),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_draft() -> ProjectionDraft {
        ProjectionDraft {
            version: PROJECTION_DEFINITION_VERSION,
            id: "notes-by-topic".into(),
            label: "Notes by topic".into(),
            source: SourceBinding {
                authority: "local-device".into(),
                domain: "notes".into(),
                resource: "graph:notes".into(),
            },
            reading: Reading {
                kind: "nodes".into(),
                key: "topic".into(),
                value: None,
            },
            encoding: Encoding {
                x: Channel::Field("topic_x".into()),
                y: Channel::Field("topic_y".into()),
                color: Some(Channel::Field("kind".into())),
                label: Some(Channel::Field("title".into())),
            },
            arrangement: Arrangement::default(),
            interaction: Interaction::default(),
            appearance: Appearance {
                realization: "canvas".into(),
                title: "Notes by topic".into(),
                theme: "light".into(),
            },
            provenance: Provenance {
                author: "mark".into(),
                source_revision: "rev-7".into(),
                note: "fixture".into(),
            },
        }
    }

    #[test]
    fn reducer_edits_typed_draft_and_panel() {
        let mut editor = ProjectionEditor::new(valid_draft());
        assert_eq!(
            editor.reduce(EditorAction::SetLabel("Edited".into())),
            ReduceResult::Changed
        );
        assert_eq!(
            editor.reduce(EditorAction::SelectPanel(ProjectionPanel::Preview)),
            ReduceResult::PanelChanged
        );
        assert_eq!(editor.draft().label, "Edited");
        assert_eq!(editor.panel(), ProjectionPanel::Preview);
    }

    #[test]
    fn validation_reports_useful_fields() {
        let mut draft = valid_draft();
        draft.source.domain.clear();
        draft.arrangement.spacing = 0;
        draft.provenance.source_revision.clear();
        let issues = draft.validate().expect_err("invalid draft");
        assert!(issues.iter().any(|issue| issue.field == "source.domain"));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "arrangement.spacing")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "provenance.source_revision")
        );
    }

    #[test]
    fn unsupported_version_cannot_be_promoted_or_saved() {
        let mut draft = valid_draft();
        draft.version += 1;
        let issues = draft.to_definition().expect_err("version is invalid");
        assert!(issues.iter().any(|issue| issue.field == "version"));
    }

    struct FixtureSink {
        saved: Vec<ProjectionDefinition>,
    }

    impl ProjectionDefinitionSink for FixtureSink {
        type Error = &'static str;

        fn save(&mut self, definition: &ProjectionDefinition) -> Result<(), Self::Error> {
            self.saved.push(definition.clone());
            Ok(())
        }
    }

    #[test]
    fn save_uses_fixture_sink_and_serialization_is_repeatable() {
        let editor = ProjectionEditor::new(valid_draft());
        let mut sink = FixtureSink { saved: Vec::new() };
        editor.save(&mut sink).expect("valid draft saves");
        assert_eq!(sink.saved[0].id, valid_draft().id);
        let definition = editor.draft().to_definition().expect("valid definition");
        assert_eq!(definition.to_json_bytes(), definition.to_json_bytes());
    }

    #[test]
    fn invalid_draft_never_reaches_sink() {
        let mut draft = valid_draft();
        draft.id.clear();
        let editor = ProjectionEditor::new(draft);
        let mut sink = FixtureSink { saved: Vec::new() };
        let error = editor
            .save(&mut sink)
            .expect_err("invalid draft is refused");
        assert!(matches!(error, SaveError::Invalid(_)));
        assert!(sink.saved.is_empty());
    }
}
