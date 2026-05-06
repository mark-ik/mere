/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Trait-backed service container for `GraphWorkspace` pending effects.
//!
//! This module proves the app-state layer can execute reducer-emitted effects
//! without importing the donor `GraphBrowserApp`, concrete stores, host widget
//! handles, or engine implementations.

use crate::mnem::{MnemResponse, MnemStore, dispatch_mnem_request};

use super::{
    DiagnosticsSink, EngineRouteDecision, EngineRouter, GraphMutationJournal, GraphWorkspace,
    SettingsStore, SurfaceCommandOutcome, SurfaceCommandSchedule, SurfaceHost,
    SurfaceLifecycleState, TaskRuntime, WorkspaceEffect, WorkspaceRepository,
    WorkspaceServiceResult,
    persistence::{checkpoint_workspace, save_preferences},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceServiceDispatchReport {
    pub persisted_workspaces: usize,
    pub persisted_preferences: usize,
    pub appended_mutations: usize,
    pub mnem_responses: Vec<MnemResponse>,
    pub engine_route_decisions: Vec<EngineRouteDecision>,
    pub surface_outcomes: Vec<SurfaceCommandOutcome>,
    pub diagnostics_emitted: usize,
    pub tasks_spawned: usize,
}

pub struct WorkspaceServices<'a> {
    pub repository: &'a mut dyn WorkspaceRepository,
    pub settings: &'a mut dyn SettingsStore,
    pub journal: &'a mut dyn GraphMutationJournal,
    pub mnem: &'a mut dyn MnemStore,
    pub engine_router: &'a mut dyn EngineRouter,
    pub surface_host: &'a mut dyn SurfaceHost,
    pub diagnostics: &'a mut dyn DiagnosticsSink,
    pub task_runtime: &'a mut dyn TaskRuntime,
}

impl<'a> WorkspaceServices<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: &'a mut dyn WorkspaceRepository,
        settings: &'a mut dyn SettingsStore,
        journal: &'a mut dyn GraphMutationJournal,
        mnem: &'a mut dyn MnemStore,
        engine_router: &'a mut dyn EngineRouter,
        surface_host: &'a mut dyn SurfaceHost,
        diagnostics: &'a mut dyn DiagnosticsSink,
        task_runtime: &'a mut dyn TaskRuntime,
    ) -> Self {
        Self {
            repository,
            settings,
            journal,
            mnem,
            engine_router,
            surface_host,
            diagnostics,
            task_runtime,
        }
    }

    pub fn dispatch_pending_effects(
        &mut self,
        workspace: &mut GraphWorkspace,
    ) -> WorkspaceServiceResult<WorkspaceServiceDispatchReport> {
        let mut report = WorkspaceServiceDispatchReport::default();

        for effect in workspace.drain_effects() {
            match effect {
                WorkspaceEffect::PersistWorkspace { workspace_id } => {
                    checkpoint_workspace(self.repository, &workspace_id, workspace)?;
                    report.persisted_workspaces += 1;
                }
                WorkspaceEffect::PersistPreferences { preferences } => {
                    save_preferences(self.settings, &preferences)?;
                    report.persisted_preferences += 1;
                }
                WorkspaceEffect::AppendGraphMutation(record) => {
                    self.journal.append_mutation(&record)?;
                    report.appended_mutations += 1;
                }
                WorkspaceEffect::RequestMnem(request) => {
                    report
                        .mnem_responses
                        .push(dispatch_mnem_request(self.mnem, &request)?);
                }
                WorkspaceEffect::RouteEngine(request) => {
                    report
                        .engine_route_decisions
                        .push(self.engine_router.route(&request)?);
                }
                WorkspaceEffect::RequestSurface(command) => {
                    report
                        .surface_outcomes
                        .push(self.surface_host.apply_surface_command(&command)?);
                }
                WorkspaceEffect::EmitDiagnostic(record) => {
                    self.diagnostics.emit(&record);
                    report.diagnostics_emitted += 1;
                }
                WorkspaceEffect::RunTask(task) => {
                    self.task_runtime.spawn(task)?;
                    report.tasks_spawned += 1;
                }
            }
        }

        Ok(report)
    }

    pub fn dispatch_surface_schedule(
        &mut self,
        lifecycle: &mut SurfaceLifecycleState,
        schedule: &SurfaceCommandSchedule,
    ) -> WorkspaceServiceResult<Vec<SurfaceCommandOutcome>> {
        let mut outcomes = Vec::new();
        for command in schedule.commands() {
            let outcome = self.surface_host.apply_surface_command(command)?;
            lifecycle.record_outcome(command, &outcome);
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use graphshell_core::{graph::GraphViewId, pane::PaneId};
    use inker::routing::{SurfaceContract, SurfaceContractMode, SurfaceTargetId, WorkspaceRouteId};
    use uuid::Uuid;

    use crate::{
        app_state::{
            DiagnosticRecord, GraphMutationRecord, GraphWorkspaceSnapshot, JournalCursor,
            MutationPayload, NavigatorSidebarPreference, SurfaceCommand, SurfaceCommandSink,
            SurfaceCommandStatus, SurfaceHostId, SurfaceLifecycleState, SurfacePlacementPlan,
            SurfaceSlotPlacement, TaskRequest, ThemeModePreference, TileSlot, WorkspaceId,
            WorkspacePreferences, WorkspaceServiceError,
            persistence::{GraphTreeDocument, WorkspaceLayoutDocument, WorkspaceLayoutName},
        },
        mnem::MnemRequest,
    };

    use super::*;

    #[derive(Default)]
    struct FakeWorkspaceRepository {
        saved_snapshot: Option<GraphWorkspaceSnapshot>,
    }

    impl WorkspaceRepository for FakeWorkspaceRepository {
        fn load_workspace(
            &mut self,
            _workspace_id: &WorkspaceId,
        ) -> WorkspaceServiceResult<Option<GraphWorkspaceSnapshot>> {
            Ok(None)
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
            Ok(GraphViewId::from_uuid(Uuid::from_u128(1)))
        }

        fn load_graph_tree(
            &mut self,
            _view_id: &GraphViewId,
        ) -> WorkspaceServiceResult<Option<GraphTreeDocument>> {
            Ok(None)
        }

        fn save_graph_tree(
            &mut self,
            _view_id: &GraphViewId,
            _document: &GraphTreeDocument,
        ) -> WorkspaceServiceResult<()> {
            Ok(())
        }

        fn load_workspace_layout(
            &mut self,
            _name: &WorkspaceLayoutName,
        ) -> WorkspaceServiceResult<Option<WorkspaceLayoutDocument>> {
            Ok(None)
        }

        fn save_workspace_layout(
            &mut self,
            _name: &WorkspaceLayoutName,
            _document: &WorkspaceLayoutDocument,
        ) -> WorkspaceServiceResult<()> {
            Ok(())
        }

        fn list_workspace_layouts(&mut self) -> WorkspaceServiceResult<Vec<WorkspaceLayoutName>> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct FakeSettingsStore {
        saved_preferences: Option<WorkspacePreferences>,
    }

    impl SettingsStore for FakeSettingsStore {
        fn load_preferences(&mut self) -> WorkspaceServiceResult<WorkspacePreferences> {
            Ok(WorkspacePreferences::default())
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
    struct FakeGraphMutationJournal {
        appended: Vec<GraphMutationRecord>,
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
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct FakeMnemStore {
        blobs: HashMap<String, Vec<u8>>,
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

    #[derive(Default)]
    struct FakeEngineRouter {
        routed_addresses: Vec<String>,
    }

    impl EngineRouter for FakeEngineRouter {
        fn route(
            &mut self,
            request: &super::super::EngineRouteRequest,
        ) -> WorkspaceServiceResult<EngineRouteDecision> {
            self.routed_addresses.push(request.address.clone());
            Ok(EngineRouteDecision {
                engine_id: "nematic.markdown".to_string(),
                surface_contract: SurfaceContract {
                    target: SurfaceTargetId::new("tile/main"),
                    mode: SurfaceContractMode::CompositedTexture,
                },
            })
        }
    }

    #[derive(Default)]
    struct FakeSurfaceHost {
        commands: Vec<SurfaceCommand>,
        statuses: Vec<SurfaceCommandStatus>,
    }

    impl SurfaceCommandSink for FakeSurfaceHost {
        type Error = WorkspaceServiceError;

        fn apply_surface_command(
            &mut self,
            command: &SurfaceCommand,
        ) -> Result<SurfaceCommandOutcome, Self::Error> {
            self.commands.push(command.clone());
            let status = if self.statuses.is_empty() {
                SurfaceCommandStatus::AlreadySatisfied
            } else {
                self.statuses.remove(0)
            };
            Ok(command.outcome(status))
        }
    }

    #[derive(Default)]
    struct FakeDiagnosticsSink {
        records: Vec<DiagnosticRecord>,
    }

    impl DiagnosticsSink for FakeDiagnosticsSink {
        fn emit(&mut self, record: &DiagnosticRecord) {
            self.records.push(record.clone());
        }
    }

    #[derive(Default)]
    struct FakeTaskRuntime {
        tasks: Vec<TaskRequest>,
    }

    impl TaskRuntime for FakeTaskRuntime {
        fn spawn(&mut self, task: TaskRequest) -> WorkspaceServiceResult<()> {
            self.tasks.push(task);
            Ok(())
        }
    }

    #[test]
    fn service_container_dispatches_every_pending_effect_kind() {
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.has_unsaved_changes = true;
        workspace.chrome.preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Dark,
            navigator_sidebar: NavigatorSidebarPreference::Hidden,
        };
        workspace.push_effect(WorkspaceEffect::PersistWorkspace {
            workspace_id: WorkspaceId::new("main"),
        });
        workspace.push_effect(WorkspaceEffect::PersistPreferences {
            preferences: workspace.chrome.preferences.clone(),
        });
        workspace.push_effect(WorkspaceEffect::AppendGraphMutation(GraphMutationRecord {
            sequence: 0,
            label: "graph.test".to_string(),
            payload: MutationPayload::OpaqueBytes(vec![1, 2, 3]),
        }));
        workspace.push_effect(WorkspaceEffect::RequestMnem(MnemRequest::SaveBlob {
            key: "private/cache".to_string(),
            value: vec![4, 5, 6],
        }));
        workspace.push_effect(WorkspaceEffect::RouteEngine(
            super::super::EngineRouteRequest {
                workspace_id: WorkspaceRouteId::new("main"),
                view: None,
                node: None,
                address: "gemini://example.test".to_string(),
            },
        ));
        workspace.push_effect(WorkspaceEffect::RequestSurface(SurfaceCommand::focus(
            SurfaceHostId::new("desktop"),
            None,
            None,
        )));
        workspace.push_effect(WorkspaceEffect::EmitDiagnostic(DiagnosticRecord {
            channel: "graphshell.test".to_string(),
            message: "service dispatch".to_string(),
        }));
        workspace.push_effect(WorkspaceEffect::RunTask(TaskRequest {
            kind: "thumbnail.refresh".to_string(),
            payload: vec![7, 8, 9],
        }));

        let mut repository = FakeWorkspaceRepository::default();
        let mut settings = FakeSettingsStore::default();
        let mut journal = FakeGraphMutationJournal::default();
        let mut mnem = FakeMnemStore::default();
        let mut engine_router = FakeEngineRouter::default();
        let mut surface_host = FakeSurfaceHost::default();
        let mut diagnostics = FakeDiagnosticsSink::default();
        let mut task_runtime = FakeTaskRuntime::default();

        let report = {
            let mut services = WorkspaceServices::new(
                &mut repository,
                &mut settings,
                &mut journal,
                &mut mnem,
                &mut engine_router,
                &mut surface_host,
                &mut diagnostics,
                &mut task_runtime,
            );

            services.dispatch_pending_effects(&mut workspace).unwrap()
        };

        assert_eq!(report.persisted_workspaces, 1);
        assert_eq!(report.persisted_preferences, 1);
        assert_eq!(report.appended_mutations, 1);
        assert_eq!(report.mnem_responses.len(), 1);
        assert_eq!(
            report.engine_route_decisions[0].engine_id,
            "nematic.markdown"
        );
        assert_eq!(
            report.surface_outcomes[0].status,
            SurfaceCommandStatus::AlreadySatisfied
        );
        assert_eq!(report.diagnostics_emitted, 1);
        assert_eq!(report.tasks_spawned, 1);
        assert!(!workspace.workbench.has_unsaved_changes);
        assert!(workspace.pending_effects.effects.is_empty());
        assert!(repository.saved_snapshot.is_some());
        assert_eq!(
            settings.saved_preferences,
            Some(workspace.chrome.preferences)
        );
        assert_eq!(journal.appended.len(), 1);
        assert_eq!(
            engine_router.routed_addresses,
            vec!["gemini://example.test"]
        );
        assert_eq!(surface_host.commands.len(), 1);
        assert_eq!(diagnostics.records.len(), 1);
        assert_eq!(task_runtime.tasks.len(), 1);
    }

    #[test]
    fn surface_schedule_dispatch_records_deferred_outcomes_for_retry() {
        let view = GraphViewId::from_uuid(Uuid::from_u128(80));
        let host = SurfaceHostId::new("desktop");
        let first_pane = PaneId::from_uuid(Uuid::from_u128(81));
        let second_pane = PaneId::from_uuid(Uuid::from_u128(82));
        let mut plan = SurfacePlacementPlan::default();
        plan.push(SurfaceSlotPlacement::new(
            host.clone(),
            Some(view),
            first_pane,
            graphshell_core::graph::NodeKey::new(80),
            TileSlot::primary(),
        ));
        plan.push(SurfaceSlotPlacement::new(
            host,
            Some(view),
            second_pane,
            graphshell_core::graph::NodeKey::new(81),
            TileSlot::secondary(1),
        ));
        let mut lifecycle = SurfaceLifecycleState::default();
        let schedule = lifecycle.schedule_placements(plan);

        let mut repository = FakeWorkspaceRepository::default();
        let mut settings = FakeSettingsStore::default();
        let mut journal = FakeGraphMutationJournal::default();
        let mut mnem = FakeMnemStore::default();
        let mut engine_router = FakeEngineRouter::default();
        let mut surface_host = FakeSurfaceHost {
            statuses: vec![
                SurfaceCommandStatus::Deferred,
                SurfaceCommandStatus::AlreadySatisfied,
            ],
            ..FakeSurfaceHost::default()
        };
        let mut diagnostics = FakeDiagnosticsSink::default();
        let mut task_runtime = FakeTaskRuntime::default();

        let outcomes = {
            let mut services = WorkspaceServices::new(
                &mut repository,
                &mut settings,
                &mut journal,
                &mut mnem,
                &mut engine_router,
                &mut surface_host,
                &mut diagnostics,
                &mut task_runtime,
            );
            services
                .dispatch_surface_schedule(&mut lifecycle, &schedule)
                .unwrap()
        };

        assert_eq!(outcomes.len(), 2);
        assert_eq!(surface_host.commands.len(), 2);
        assert_eq!(lifecycle.backlog().len(), 1);
        let retry = lifecycle.schedule_retries();
        assert_eq!(retry.retries, 1);
        assert_eq!(retry.commands(), &[surface_host.commands[0].clone()]);
    }
}
