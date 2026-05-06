/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure persistence-boundary reducer intents.
//!
//! This module does not call concrete stores. It expresses workspace,
//! preference, and private local-memory requests as typed effects for service
//! glue to execute through `WorkspaceRepository`, `GraphMutationJournal`,
//! `SettingsStore`, and `MnemStore` implementations.

use graphshell_core::graph::GraphViewId;
use serde::{Deserialize, Serialize};

use crate::mnem::{MnemRequest, MnemResponse, MnemStore, dispatch_mnem_request};

use super::{
    GraphMutationJournal, GraphWorkspace, JournalCursor, MutationPayload, SettingsStore,
    WorkspaceEffect, WorkspaceId, WorkspacePreferences, WorkspaceRepository, WorkspaceServiceError,
    WorkspaceServiceResult,
    graph_runtime::{GraphRuntimeIntent, reduce_graph_runtime_intent},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistenceIntent {
    RequestWorkspaceSave { workspace_id: WorkspaceId },
    MarkWorkspaceSaved,
    RequestPreferencesSave,
    RequestMnemBlobLoad { key: String },
    RequestMnemBlobSave { key: String, value: Vec<u8> },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersistenceOutcome {
    pub state_changed: bool,
    pub effects_emitted: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersistenceDispatchReport {
    pub persisted_workspaces: usize,
    pub persisted_preferences: usize,
    pub appended_mutations: usize,
    pub mnem_responses: Vec<MnemResponse>,
    pub deferred_effects: usize,
}

#[derive(Clone)]
pub struct HydratedWorkspaceState {
    pub workspace: GraphWorkspace,
    pub journal_cursor: JournalCursor,
    pub replayed_records: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchPersistenceState {
    pub workbench_view_id: Option<GraphViewId>,
    pub active_layout: Option<WorkspaceLayoutName>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersistenceDocumentFormat {
    #[default]
    Json,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceLayoutName(pub String);

impl WorkspaceLayoutName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphTreeDocument {
    pub format: PersistenceDocumentFormat,
    pub body: String,
}

impl GraphTreeDocument {
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            format: PersistenceDocumentFormat::Json,
            body: body.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLayoutDocument {
    pub format: PersistenceDocumentFormat,
    pub body: String,
}

impl WorkspaceLayoutDocument {
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            format: PersistenceDocumentFormat::Json,
            body: body.into(),
        }
    }
}

pub fn reduce_persistence_intent(
    workspace: &mut GraphWorkspace,
    intent: PersistenceIntent,
) -> PersistenceOutcome {
    match intent {
        PersistenceIntent::RequestWorkspaceSave { workspace_id } => {
            workspace.push_effect(WorkspaceEffect::PersistWorkspace { workspace_id });
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
        PersistenceIntent::MarkWorkspaceSaved => {
            let changed = workspace.workbench.has_unsaved_changes;
            workspace.workbench.has_unsaved_changes = false;
            PersistenceOutcome {
                state_changed: changed,
                effects_emitted: 0,
            }
        }
        PersistenceIntent::RequestPreferencesSave => {
            workspace.push_effect(WorkspaceEffect::PersistPreferences {
                preferences: workspace.chrome.preferences.clone(),
            });
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
        PersistenceIntent::RequestMnemBlobLoad { key } => {
            workspace.push_effect(WorkspaceEffect::RequestMnem(MnemRequest::LoadBlob { key }));
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
        PersistenceIntent::RequestMnemBlobSave { key, value } => {
            workspace.push_effect(WorkspaceEffect::RequestMnem(MnemRequest::SaveBlob {
                key,
                value,
            }));
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
    }
}

pub fn hydrate_workspace(
    repository: &mut dyn WorkspaceRepository,
    settings: &mut dyn SettingsStore,
    workspace_id: &WorkspaceId,
) -> WorkspaceServiceResult<GraphWorkspace> {
    let mut workspace = repository
        .load_workspace(workspace_id)?
        .map(|snapshot| GraphWorkspace::from_snapshot(&snapshot))
        .unwrap_or_default();
    workspace.chrome.preferences = settings.load_preferences()?;
    Ok(workspace)
}

pub fn hydrate_workspace_with_replay(
    repository: &mut dyn WorkspaceRepository,
    settings: &mut dyn SettingsStore,
    journal: &mut dyn GraphMutationJournal,
    workspace_id: &WorkspaceId,
    cursor: Option<JournalCursor>,
) -> WorkspaceServiceResult<HydratedWorkspaceState> {
    let mut workspace = hydrate_workspace(repository, settings, workspace_id)?;
    let replayed_records = replay_graph_journal(journal, &mut workspace, cursor)?;
    Ok(HydratedWorkspaceState {
        journal_cursor: JournalCursor {
            next_sequence: workspace.domain.next_mutation_sequence,
        },
        workspace,
        replayed_records,
    })
}

pub fn checkpoint_workspace(
    repository: &mut dyn WorkspaceRepository,
    workspace_id: &WorkspaceId,
    workspace: &mut GraphWorkspace,
) -> WorkspaceServiceResult<()> {
    repository.save_workspace(workspace_id, &workspace.to_snapshot())?;
    workspace.workbench.has_unsaved_changes = false;
    Ok(())
}

pub fn save_preferences(
    settings: &mut dyn SettingsStore,
    preferences: &WorkspacePreferences,
) -> WorkspaceServiceResult<()> {
    settings.save_preferences(preferences)
}

pub fn ensure_workbench_view_id(
    repository: &mut dyn WorkspaceRepository,
    workspace_id: &WorkspaceId,
    workspace: &mut GraphWorkspace,
) -> WorkspaceServiceResult<GraphViewId> {
    if let Some(view_id) = workspace.workbench.persistence.workbench_view_id {
        return Ok(view_id);
    }

    let view_id = repository.load_or_create_workbench_view_id(workspace_id)?;
    workspace.workbench.persistence.workbench_view_id = Some(view_id);
    Ok(view_id)
}

pub fn load_workbench_graph_tree(
    repository: &mut dyn WorkspaceRepository,
    workspace_id: &WorkspaceId,
    workspace: &mut GraphWorkspace,
) -> WorkspaceServiceResult<Option<GraphTreeDocument>> {
    let view_id = ensure_workbench_view_id(repository, workspace_id, workspace)?;
    repository.load_graph_tree(&view_id)
}

pub fn save_workbench_graph_tree(
    repository: &mut dyn WorkspaceRepository,
    workspace_id: &WorkspaceId,
    workspace: &mut GraphWorkspace,
    document: &GraphTreeDocument,
) -> WorkspaceServiceResult<GraphViewId> {
    let view_id = ensure_workbench_view_id(repository, workspace_id, workspace)?;
    repository.save_graph_tree(&view_id, document)?;
    Ok(view_id)
}

pub fn load_named_workspace_layout(
    repository: &mut dyn WorkspaceRepository,
    workspace: &mut GraphWorkspace,
    name: &WorkspaceLayoutName,
) -> WorkspaceServiceResult<Option<WorkspaceLayoutDocument>> {
    let document = repository.load_workspace_layout(name)?;
    if document.is_some() {
        workspace.workbench.persistence.active_layout = Some(name.clone());
    }
    Ok(document)
}

pub fn save_named_workspace_layout(
    repository: &mut dyn WorkspaceRepository,
    workspace: &mut GraphWorkspace,
    name: &WorkspaceLayoutName,
    document: &WorkspaceLayoutDocument,
) -> WorkspaceServiceResult<()> {
    repository.save_workspace_layout(name, document)?;
    workspace.workbench.persistence.active_layout = Some(name.clone());
    workspace.workbench.has_unsaved_changes = false;
    Ok(())
}

pub fn list_named_workspace_layouts(
    repository: &mut dyn WorkspaceRepository,
) -> WorkspaceServiceResult<Vec<WorkspaceLayoutName>> {
    repository.list_workspace_layouts()
}

pub fn consume_persistence_effects(
    workspace: &mut GraphWorkspace,
    repository: &mut dyn WorkspaceRepository,
    settings: &mut dyn SettingsStore,
    journal: &mut dyn GraphMutationJournal,
    mnem: &mut dyn MnemStore,
) -> WorkspaceServiceResult<PersistenceDispatchReport> {
    let mut report = PersistenceDispatchReport::default();
    let mut deferred_effects = Vec::new();

    for effect in workspace.drain_effects() {
        match effect {
            WorkspaceEffect::PersistWorkspace { workspace_id } => {
                checkpoint_workspace(repository, &workspace_id, workspace)?;
                report.persisted_workspaces += 1;
            }
            WorkspaceEffect::PersistPreferences { preferences } => {
                save_preferences(settings, &preferences)?;
                report.persisted_preferences += 1;
            }
            WorkspaceEffect::AppendGraphMutation(record) => {
                journal.append_mutation(&record)?;
                report.appended_mutations += 1;
            }
            WorkspaceEffect::RequestMnem(request) => {
                report
                    .mnem_responses
                    .push(dispatch_mnem_request(mnem, &request)?);
            }
            other => deferred_effects.push(other),
        }
    }

    report.deferred_effects = deferred_effects.len();
    workspace.pending_effects.effects = deferred_effects;
    Ok(report)
}

pub fn replay_graph_journal(
    journal: &mut dyn GraphMutationJournal,
    workspace: &mut GraphWorkspace,
    cursor: Option<JournalCursor>,
) -> WorkspaceServiceResult<usize> {
    let records = journal.replay_from(cursor)?;
    for record in &records {
        let intent = decode_graph_runtime_intent(record)?;
        let prior_effect_len = workspace.pending_effects.effects.len();
        reduce_graph_runtime_intent(workspace, intent).map_err(|error| {
            WorkspaceServiceError::new(format!(
                "failed to replay graph mutation {}: {error:?}",
                record.sequence
            ))
        })?;

        let replay_effects = workspace
            .pending_effects
            .effects
            .split_off(prior_effect_len);
        if !replay_effects
            .iter()
            .all(|effect| matches!(effect, WorkspaceEffect::AppendGraphMutation(_)))
        {
            return Err(WorkspaceServiceError::new(format!(
                "graph mutation replay emitted non-journal effects for sequence {}",
                record.sequence
            )));
        }
    }

    Ok(records.len())
}

fn decode_graph_runtime_intent(
    record: &super::GraphMutationRecord,
) -> WorkspaceServiceResult<GraphRuntimeIntent> {
    match &record.payload {
        MutationPayload::TypedJson(json) => serde_json::from_str(json).map_err(|error| {
            WorkspaceServiceError::new(format!(
                "failed to decode graph mutation {}: {error}",
                record.sequence
            ))
        }),
        MutationPayload::OpaqueBytes(_) => Err(WorkspaceServiceError::new(format!(
            "opaque graph mutation payload is not replayable for sequence {}",
            record.sequence
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use graphshell_core::geometry::PortablePoint;
    use graphshell_core::graph::Graph;
    use uuid::Uuid;

    use super::*;
    use crate::app_state::{
        DiagnosticRecord, GraphMutationRecord, GraphWorkspaceSnapshot, NavigatorSidebarPreference,
        ThemeModePreference, WorkspaceServiceError,
    };

    #[derive(Default)]
    struct FakeWorkspaceRepository {
        snapshot: Option<GraphWorkspaceSnapshot>,
        saved_snapshot: Option<GraphWorkspaceSnapshot>,
        workbench_view_id: Option<GraphViewId>,
        graph_trees: HashMap<GraphViewId, GraphTreeDocument>,
        layouts: HashMap<WorkspaceLayoutName, WorkspaceLayoutDocument>,
    }

    impl WorkspaceRepository for FakeWorkspaceRepository {
        fn load_workspace(
            &mut self,
            _workspace_id: &WorkspaceId,
        ) -> WorkspaceServiceResult<Option<GraphWorkspaceSnapshot>> {
            Ok(self.snapshot.clone())
        }

        fn save_workspace(
            &mut self,
            _workspace_id: &WorkspaceId,
            snapshot: &GraphWorkspaceSnapshot,
        ) -> WorkspaceServiceResult<()> {
            self.saved_snapshot = Some(snapshot.clone());
            Ok(())
        }

        fn load_or_create_workbench_view_id(
            &mut self,
            _workspace_id: &WorkspaceId,
        ) -> WorkspaceServiceResult<GraphViewId> {
            let view_id = self
                .workbench_view_id
                .unwrap_or_else(|| GraphViewId::from_uuid(Uuid::from_u128(900)));
            self.workbench_view_id = Some(view_id);
            Ok(view_id)
        }

        fn load_graph_tree(
            &mut self,
            view_id: &GraphViewId,
        ) -> WorkspaceServiceResult<Option<GraphTreeDocument>> {
            Ok(self.graph_trees.get(view_id).cloned())
        }

        fn save_graph_tree(
            &mut self,
            view_id: &GraphViewId,
            document: &GraphTreeDocument,
        ) -> WorkspaceServiceResult<()> {
            self.graph_trees.insert(*view_id, document.clone());
            Ok(())
        }

        fn load_workspace_layout(
            &mut self,
            name: &WorkspaceLayoutName,
        ) -> WorkspaceServiceResult<Option<WorkspaceLayoutDocument>> {
            Ok(self.layouts.get(name).cloned())
        }

        fn save_workspace_layout(
            &mut self,
            name: &WorkspaceLayoutName,
            document: &WorkspaceLayoutDocument,
        ) -> WorkspaceServiceResult<()> {
            self.layouts.insert(name.clone(), document.clone());
            Ok(())
        }

        fn list_workspace_layouts(&mut self) -> WorkspaceServiceResult<Vec<WorkspaceLayoutName>> {
            let mut names = self.layouts.keys().cloned().collect::<Vec<_>>();
            names.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            Ok(names)
        }
    }

    #[derive(Default)]
    struct FakeSettingsStore {
        preferences: WorkspacePreferences,
        saved_preferences: Option<WorkspacePreferences>,
    }

    impl SettingsStore for FakeSettingsStore {
        fn load_preferences(&mut self) -> WorkspaceServiceResult<WorkspacePreferences> {
            Ok(self.preferences.clone())
        }

        fn save_preferences(
            &mut self,
            preferences: &WorkspacePreferences,
        ) -> WorkspaceServiceResult<()> {
            self.saved_preferences = Some(preferences.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeMnemStore {
        blobs: HashMap<String, Vec<u8>>,
    }

    #[derive(Default)]
    struct FakeGraphMutationJournal {
        appended: Vec<GraphMutationRecord>,
        replay: Vec<GraphMutationRecord>,
    }

    impl GraphMutationJournal for FakeGraphMutationJournal {
        fn append_mutation(&mut self, record: &GraphMutationRecord) -> WorkspaceServiceResult<()> {
            self.appended.push(record.clone());
            Ok(())
        }

        fn replay_from(
            &mut self,
            _cursor: Option<JournalCursor>,
        ) -> WorkspaceServiceResult<Vec<GraphMutationRecord>> {
            Ok(self.replay.clone())
        }
    }

    fn node_id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn add_node_record(sequence: u64, id: u128, url: &str) -> GraphMutationRecord {
        GraphMutationRecord {
            sequence,
            label: "graph.add_node".to_string(),
            payload: MutationPayload::TypedJson(
                serde_json::to_string(&GraphRuntimeIntent::AddNode {
                    id: node_id(id),
                    url: url.to_string(),
                    position: PortablePoint::new(3.0, 4.0),
                })
                .unwrap(),
            ),
        }
    }

    impl MnemStore for FakeMnemStore {
        fn load_blob(&mut self, key: &str) -> WorkspaceServiceResult<Option<Vec<u8>>> {
            Ok(self.blobs.get(key).cloned())
        }

        fn save_blob(&mut self, key: &str, value: &[u8]) -> WorkspaceServiceResult<()> {
            self.blobs.insert(key.to_string(), value.to_vec());
            Ok(())
        }
    }

    #[test]
    fn workspace_save_request_emits_effect_only() {
        let mut workspace = GraphWorkspace::new();
        let workspace_id = WorkspaceId::new("default");

        let outcome = reduce_persistence_intent(
            &mut workspace,
            PersistenceIntent::RequestWorkspaceSave {
                workspace_id: workspace_id.clone(),
            },
        );

        assert!(!outcome.state_changed);
        assert_eq!(outcome.effects_emitted, 1);
        assert_eq!(
            workspace.drain_effects(),
            vec![WorkspaceEffect::PersistWorkspace { workspace_id }]
        );
    }

    #[test]
    fn mark_workspace_saved_clears_dirty_state_without_effects() {
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.has_unsaved_changes = true;

        let outcome =
            reduce_persistence_intent(&mut workspace, PersistenceIntent::MarkWorkspaceSaved);

        assert!(outcome.state_changed);
        assert!(!workspace.workbench.has_unsaved_changes);
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn preferences_save_emits_current_preferences_snapshot() {
        let mut workspace = GraphWorkspace::new();
        workspace.chrome.preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Dark,
            navigator_sidebar: NavigatorSidebarPreference::Hidden,
        };

        reduce_persistence_intent(&mut workspace, PersistenceIntent::RequestPreferencesSave);

        assert_eq!(
            workspace.drain_effects(),
            vec![WorkspaceEffect::PersistPreferences {
                preferences: WorkspacePreferences {
                    theme_mode: ThemeModePreference::Dark,
                    navigator_sidebar: NavigatorSidebarPreference::Hidden,
                }
            }]
        );
    }

    #[test]
    fn mnem_blob_requests_emit_private_memory_effects() {
        let mut workspace = GraphWorkspace::new();

        reduce_persistence_intent(
            &mut workspace,
            PersistenceIntent::RequestMnemBlobSave {
                key: "clip-cache/example".to_string(),
                value: vec![1, 2, 3],
            },
        );
        reduce_persistence_intent(
            &mut workspace,
            PersistenceIntent::RequestMnemBlobLoad {
                key: "clip-cache/example".to_string(),
            },
        );

        assert_eq!(
            workspace.drain_effects(),
            vec![
                WorkspaceEffect::RequestMnem(MnemRequest::SaveBlob {
                    key: "clip-cache/example".to_string(),
                    value: vec![1, 2, 3],
                }),
                WorkspaceEffect::RequestMnem(MnemRequest::LoadBlob {
                    key: "clip-cache/example".to_string(),
                }),
            ]
        );
    }

    #[test]
    fn hydrate_workspace_loads_snapshot_and_preferences_from_traits() {
        let mut graph = Graph::new();
        graph.add_node_with_id(
            Uuid::from_u128(1),
            "https://example.test".to_string(),
            PortablePoint::new(1.0, 2.0),
        );
        let mut original = GraphWorkspace::from_graph(graph);
        original.domain.next_mutation_sequence = 7;
        original.workbench.has_unsaved_changes = true;

        let mut repository = FakeWorkspaceRepository {
            snapshot: Some(original.to_snapshot()),
            saved_snapshot: None,
            ..FakeWorkspaceRepository::default()
        };
        let mut settings = FakeSettingsStore {
            preferences: WorkspacePreferences {
                theme_mode: ThemeModePreference::Light,
                navigator_sidebar: NavigatorSidebarPreference::End,
            },
            saved_preferences: None,
        };

        let hydrated =
            hydrate_workspace(&mut repository, &mut settings, &WorkspaceId::new("main")).unwrap();

        assert_eq!(hydrated.domain.graph.node_count(), 1);
        assert_eq!(hydrated.domain.next_mutation_sequence, 7);
        assert!(hydrated.workbench.has_unsaved_changes);
        assert_eq!(
            hydrated.chrome.preferences,
            WorkspacePreferences {
                theme_mode: ThemeModePreference::Light,
                navigator_sidebar: NavigatorSidebarPreference::End,
            }
        );
        assert!(hydrated.pending_effects.effects.is_empty());
    }

    #[test]
    fn hydrate_workspace_uses_empty_workspace_when_snapshot_missing() {
        let mut repository = FakeWorkspaceRepository::default();
        let mut settings = FakeSettingsStore {
            preferences: WorkspacePreferences {
                theme_mode: ThemeModePreference::Dark,
                navigator_sidebar: NavigatorSidebarPreference::Start,
            },
            saved_preferences: None,
        };

        let hydrated =
            hydrate_workspace(&mut repository, &mut settings, &WorkspaceId::new("new")).unwrap();

        assert_eq!(hydrated.domain.graph.node_count(), 0);
        assert_eq!(
            hydrated.chrome.preferences.theme_mode,
            ThemeModePreference::Dark
        );
    }

    #[test]
    fn checkpoint_workspace_saves_snapshot_and_clears_dirty_state() {
        let mut repository = FakeWorkspaceRepository::default();
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.has_unsaved_changes = true;
        workspace.domain.next_mutation_sequence = 42;

        checkpoint_workspace(&mut repository, &WorkspaceId::new("main"), &mut workspace).unwrap();

        assert!(!workspace.workbench.has_unsaved_changes);
        assert_eq!(
            repository.saved_snapshot.unwrap().next_mutation_sequence,
            42
        );
    }

    #[test]
    fn save_preferences_writes_through_settings_trait() {
        let mut settings = FakeSettingsStore::default();
        let preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Light,
            navigator_sidebar: NavigatorSidebarPreference::Hidden,
        };

        save_preferences(&mut settings, &preferences).unwrap();

        assert_eq!(settings.saved_preferences, Some(preferences));
    }

    #[test]
    fn dispatch_mnem_request_round_trips_private_blob() {
        let mut store = FakeMnemStore::default();

        let saved = dispatch_mnem_request(
            &mut store,
            &MnemRequest::SaveBlob {
                key: "private/history".to_string(),
                value: vec![9, 8, 7],
            },
        )
        .unwrap();
        let loaded = dispatch_mnem_request(
            &mut store,
            &MnemRequest::LoadBlob {
                key: "private/history".to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            saved,
            MnemResponse::BlobSaved {
                key: "private/history".to_string(),
            }
        );
        assert_eq!(
            loaded,
            MnemResponse::BlobLoaded {
                key: "private/history".to_string(),
                value: Some(vec![9, 8, 7]),
            }
        );
    }

    #[test]
    fn mnem_dispatch_propagates_trait_errors() {
        struct BrokenMnemStore;

        impl MnemStore for BrokenMnemStore {
            fn load_blob(&mut self, _key: &str) -> WorkspaceServiceResult<Option<Vec<u8>>> {
                Err(WorkspaceServiceError::new("load failed"))
            }

            fn save_blob(&mut self, _key: &str, _value: &[u8]) -> WorkspaceServiceResult<()> {
                Err(WorkspaceServiceError::new("save failed"))
            }
        }

        let error = dispatch_mnem_request(
            &mut BrokenMnemStore,
            &MnemRequest::LoadBlob {
                key: "missing".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(error.message, "load failed");
    }

    #[test]
    fn consume_persistence_effects_dispatches_and_preserves_non_persistence_effects() {
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.has_unsaved_changes = true;
        workspace.chrome.preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Light,
            navigator_sidebar: NavigatorSidebarPreference::End,
        };
        workspace.push_effect(WorkspaceEffect::EmitDiagnostic(DiagnosticRecord {
            channel: "graphshell.test".to_string(),
            message: "keep me".to_string(),
        }));
        workspace.push_effect(WorkspaceEffect::PersistWorkspace {
            workspace_id: WorkspaceId::new("main"),
        });
        workspace.push_effect(WorkspaceEffect::PersistPreferences {
            preferences: workspace.chrome.preferences.clone(),
        });
        workspace.push_effect(WorkspaceEffect::AppendGraphMutation(add_node_record(
            0,
            11,
            "https://journal.example",
        )));
        workspace.push_effect(WorkspaceEffect::RequestMnem(MnemRequest::SaveBlob {
            key: "private/cache".to_string(),
            value: vec![5, 4, 3],
        }));

        let mut repository = FakeWorkspaceRepository::default();
        let mut settings = FakeSettingsStore::default();
        let mut journal = FakeGraphMutationJournal::default();
        let mut mnem = FakeMnemStore::default();

        let report = consume_persistence_effects(
            &mut workspace,
            &mut repository,
            &mut settings,
            &mut journal,
            &mut mnem,
        )
        .unwrap();

        assert_eq!(report.persisted_workspaces, 1);
        assert_eq!(report.persisted_preferences, 1);
        assert_eq!(report.appended_mutations, 1);
        assert_eq!(report.deferred_effects, 1);
        assert_eq!(
            report.mnem_responses,
            vec![MnemResponse::BlobSaved {
                key: "private/cache".to_string(),
            }]
        );
        assert!(!workspace.workbench.has_unsaved_changes);
        assert_eq!(journal.appended.len(), 1);
        assert_eq!(
            settings.saved_preferences,
            Some(WorkspacePreferences {
                theme_mode: ThemeModePreference::Light,
                navigator_sidebar: NavigatorSidebarPreference::End,
            })
        );
        assert_eq!(workspace.pending_effects.effects.len(), 1);
        assert!(matches!(
            workspace.pending_effects.effects[0],
            WorkspaceEffect::EmitDiagnostic(_)
        ));
    }

    #[test]
    fn hydrate_workspace_with_replay_applies_graph_mutations_without_requeueing() {
        let mut repository = FakeWorkspaceRepository::default();
        let mut settings = FakeSettingsStore {
            preferences: WorkspacePreferences {
                theme_mode: ThemeModePreference::Dark,
                navigator_sidebar: NavigatorSidebarPreference::Start,
            },
            saved_preferences: None,
        };
        let mut journal = FakeGraphMutationJournal {
            appended: Vec::new(),
            replay: vec![add_node_record(0, 22, "https://replay.example")],
        };

        let hydrated = hydrate_workspace_with_replay(
            &mut repository,
            &mut settings,
            &mut journal,
            &WorkspaceId::new("main"),
            Some(JournalCursor { next_sequence: 0 }),
        )
        .unwrap();

        assert_eq!(hydrated.replayed_records, 1);
        assert_eq!(hydrated.workspace.domain.graph.node_count(), 1);
        assert_eq!(hydrated.workspace.domain.next_mutation_sequence, 1);
        assert_eq!(hydrated.journal_cursor.next_sequence, 1);
        assert!(hydrated.workspace.pending_effects.effects.is_empty());
        assert_eq!(
            hydrated.workspace.chrome.preferences.theme_mode,
            ThemeModePreference::Dark
        );
    }

    #[test]
    fn replay_graph_journal_rejects_opaque_payloads() {
        let mut journal = FakeGraphMutationJournal {
            appended: Vec::new(),
            replay: vec![GraphMutationRecord {
                sequence: 4,
                label: "graph.opaque".to_string(),
                payload: MutationPayload::OpaqueBytes(vec![1, 2, 3]),
            }],
        };
        let mut workspace = GraphWorkspace::new();

        let error = replay_graph_journal(
            &mut journal,
            &mut workspace,
            Some(JournalCursor { next_sequence: 4 }),
        )
        .unwrap_err();

        assert!(error.message.contains("opaque graph mutation payload"));
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn ensure_workbench_view_id_caches_repository_identity_in_workspace_state() {
        let expected = GraphViewId::from_uuid(Uuid::from_u128(44));
        let mut repository = FakeWorkspaceRepository {
            workbench_view_id: Some(expected),
            ..FakeWorkspaceRepository::default()
        };
        let mut workspace = GraphWorkspace::new();

        let first =
            ensure_workbench_view_id(&mut repository, &WorkspaceId::new("main"), &mut workspace)
                .unwrap();
        repository.workbench_view_id = Some(GraphViewId::from_uuid(Uuid::from_u128(55)));
        let second =
            ensure_workbench_view_id(&mut repository, &WorkspaceId::new("main"), &mut workspace)
                .unwrap();

        assert_eq!(first, expected);
        assert_eq!(second, expected);
        assert_eq!(
            workspace.workbench.persistence.workbench_view_id,
            Some(expected)
        );
    }

    #[test]
    fn workbench_graph_tree_round_trips_through_repository_contract() {
        let view_id = GraphViewId::from_uuid(Uuid::from_u128(66));
        let mut repository = FakeWorkspaceRepository {
            workbench_view_id: Some(view_id),
            ..FakeWorkspaceRepository::default()
        };
        let mut workspace = GraphWorkspace::new();
        let tree = GraphTreeDocument::json("{\"tree\":\"tabs\"}");

        let saved_view = save_workbench_graph_tree(
            &mut repository,
            &WorkspaceId::new("main"),
            &mut workspace,
            &tree,
        )
        .unwrap();
        let loaded =
            load_workbench_graph_tree(&mut repository, &WorkspaceId::new("main"), &mut workspace)
                .unwrap();

        assert_eq!(saved_view, view_id);
        assert_eq!(loaded, Some(tree));
        assert_eq!(
            workspace.workbench.persistence.workbench_view_id,
            Some(view_id)
        );
    }

    #[test]
    fn named_workspace_layout_helpers_track_active_layout_and_clear_dirty_state() {
        let mut repository = FakeWorkspaceRepository::default();
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.has_unsaved_changes = true;
        let name = WorkspaceLayoutName::new("daily");
        let document = WorkspaceLayoutDocument::json("{\"layout\":1}");

        save_named_workspace_layout(&mut repository, &mut workspace, &name, &document).unwrap();
        let loaded = load_named_workspace_layout(&mut repository, &mut workspace, &name).unwrap();

        assert_eq!(loaded, Some(document));
        assert_eq!(workspace.workbench.persistence.active_layout, Some(name));
        assert!(!workspace.workbench.has_unsaved_changes);
    }

    #[test]
    fn list_named_workspace_layouts_returns_stable_names() {
        let mut repository = FakeWorkspaceRepository::default();
        repository.layouts.insert(
            WorkspaceLayoutName::new("zebra"),
            WorkspaceLayoutDocument::json("{}"),
        );
        repository.layouts.insert(
            WorkspaceLayoutName::new("alpha"),
            WorkspaceLayoutDocument::json("{}"),
        );

        let names = list_named_workspace_layouts(&mut repository).unwrap();

        assert_eq!(
            names,
            vec![
                WorkspaceLayoutName::new("alpha"),
                WorkspaceLayoutName::new("zebra")
            ]
        );
    }

    #[test]
    fn workspace_snapshot_roundtrips_workbench_persistence_state() {
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.persistence.workbench_view_id =
            Some(GraphViewId::from_uuid(Uuid::from_u128(77)));
        workspace.workbench.persistence.active_layout = Some(WorkspaceLayoutName::new("focus"));

        let restored = GraphWorkspace::from_snapshot(&workspace.to_snapshot());

        assert_eq!(
            restored.workbench.persistence.workbench_view_id,
            Some(GraphViewId::from_uuid(Uuid::from_u128(77)))
        );
        assert_eq!(
            restored.workbench.persistence.active_layout,
            Some(WorkspaceLayoutName::new("focus"))
        );
    }
}
