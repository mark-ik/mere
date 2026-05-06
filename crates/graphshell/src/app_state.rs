/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable app-state and service-boundary contracts for Graphshell.
//!
//! This module is the first Mere-side home for the reducer-owned
//! `GraphWorkspace` target shape. It deliberately contains pure state,
//! typed pending effects, and traits for side-effecting services. Concrete
//! persistence stores, engine instances, task runtimes, and host adapters live
//! outside this module and consume these contracts.

use std::collections::HashMap;

use crate::mnem;
use graph_tree::{GraphTree, LayoutMode, ProjectionLens};
use graphshell_core::graph::{Graph, GraphViewId, NodeKey};
use graphshell_core::pane::PaneId;
use graphshell_core::persistence::GraphSnapshot;
use serde::{Deserialize, Serialize};

pub mod app_ux;
pub mod composition;
pub mod graph_runtime;
pub mod intent_system;
pub mod persistence;
pub mod workspace_routing;

/// Result type used by portable app-state service traits.
pub type WorkspaceServiceResult<T> = Result<T, WorkspaceServiceError>;

/// Opaque workspace identity used by repositories and local memory stores.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub String);

impl WorkspaceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque host-surface identity. Hosts mint these; reducers only route by key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfaceHostId(pub String);

impl SurfaceHostId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque frame/workbench snapshot identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reducer-owned workspace state.
///
/// `GraphWorkspace` is the durable app-state destination for the migration. It
/// may emit pending effects, but it does not own concrete stores, task queues,
/// engine instances, renderer handles, or host widgets.
#[derive(Clone)]
pub struct GraphWorkspace {
    pub domain: WorkspaceDomainState,
    pub views: WorkspaceViewState,
    pub workbench: WorkbenchState,
    pub chrome: ChromeState,
    pub pending_effects: PendingEffects,
}

impl GraphWorkspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_graph(graph: Graph) -> Self {
        Self {
            domain: WorkspaceDomainState {
                graph,
                ..WorkspaceDomainState::default()
            },
            ..Self::default()
        }
    }

    /// Convert the live graph into a persistence-friendly snapshot envelope.
    pub fn to_snapshot(&self) -> GraphWorkspaceSnapshot {
        GraphWorkspaceSnapshot {
            graph: self.domain.graph.to_snapshot(),
            next_mutation_sequence: self.domain.next_mutation_sequence,
            views: self.views.to_snapshot(),
            workbench: self.workbench.clone(),
            chrome: self.chrome.snapshot(),
        }
    }

    /// Restore the live reducer state from a snapshot envelope.
    pub fn from_snapshot(snapshot: &GraphWorkspaceSnapshot) -> Self {
        Self {
            domain: WorkspaceDomainState {
                graph: Graph::from_snapshot(&snapshot.graph),
                next_mutation_sequence: snapshot.next_mutation_sequence,
            },
            views: WorkspaceViewState::from_snapshot(snapshot.views.clone()),
            workbench: snapshot.workbench.clone(),
            chrome: ChromeState::from_snapshot(snapshot.chrome.clone()),
            pending_effects: PendingEffects::default(),
        }
    }

    pub fn push_effect(&mut self, effect: WorkspaceEffect) {
        self.pending_effects.effects.push(effect);
    }

    pub fn drain_effects(&mut self) -> Vec<WorkspaceEffect> {
        std::mem::take(&mut self.pending_effects.effects)
    }
}

impl Default for GraphWorkspace {
    fn default() -> Self {
        Self {
            domain: WorkspaceDomainState::default(),
            views: WorkspaceViewState::default(),
            workbench: WorkbenchState::default(),
            chrome: ChromeState::default(),
            pending_effects: PendingEffects::default(),
        }
    }
}

/// Reducer-owned graph/domain state.
#[derive(Clone)]
pub struct WorkspaceDomainState {
    pub graph: Graph,
    pub next_mutation_sequence: u64,
}

impl Default for WorkspaceDomainState {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            next_mutation_sequence: 0,
        }
    }
}

impl WorkspaceDomainState {
    pub fn next_mutation_sequence(&mut self) -> u64 {
        let sequence = self.next_mutation_sequence;
        self.next_mutation_sequence = self.next_mutation_sequence.saturating_add(1);
        sequence
    }
}

/// View/session state keyed by portable graph-view IDs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorkspaceViewState {
    pub graph_views: HashMap<GraphViewId, GraphViewState>,
    pub focused_view: Option<GraphViewId>,
}

impl WorkspaceViewState {
    pub fn ensure_view(&mut self, view_id: GraphViewId) -> &mut GraphViewState {
        self.graph_views.entry(view_id).or_default()
    }

    pub fn to_snapshot(&self) -> WorkspaceViewSnapshot {
        WorkspaceViewSnapshot {
            graph_views: self.graph_views.clone(),
            focused_view: self.focused_view,
        }
    }

    pub fn from_snapshot(snapshot: WorkspaceViewSnapshot) -> Self {
        Self {
            graph_views: snapshot.graph_views,
            focused_view: snapshot.focused_view,
        }
    }
}

/// State for one portable graph view.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphViewState {
    pub tree: GraphTree<NodeKey>,
    pub projection: ViewProjectionState,
    pub panes: Vec<PaneBinding>,
}

impl Default for GraphViewState {
    fn default() -> Self {
        Self {
            tree: GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal),
            projection: ViewProjectionState::default(),
            panes: Vec::new(),
        }
    }
}

/// Portable view projection settings. More donor fields migrate here as their
/// dependencies become host-neutral.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewProjectionState {
    pub active_lens: ProjectionLens,
    pub active_node: Option<NodeKey>,
}

/// Stable pane-to-graph binding consumed by hosts and frame projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneBinding {
    pub pane_id: PaneId,
    pub node: NodeKey,
    pub surface_host: Option<SurfaceHostId>,
}

/// Workbench/frame state that is independent of any concrete host widget tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchState {
    pub persistence: persistence::WorkbenchPersistenceState,
    pub frames: HashMap<FrameId, FrameState>,
    pub active_frame: Option<FrameId>,
    pub has_unsaved_changes: bool,
}

/// One named frame/workbench composition.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameState {
    pub id: FrameId,
    pub label: String,
    pub root_view: Option<GraphViewId>,
    pub panes: Vec<PaneBinding>,
}

/// Runtime chrome state plus durable user preferences.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeState {
    pub app_ux: app_ux::AppUxState,
    pub command_palette_open: bool,
    pub settings_route: Option<String>,
    pub preferences: WorkspacePreferences,
}

impl ChromeState {
    pub fn snapshot(&self) -> ChromeSnapshot {
        ChromeSnapshot {
            preferences: self.preferences.clone(),
        }
    }

    pub fn from_snapshot(snapshot: ChromeSnapshot) -> Self {
        Self {
            preferences: snapshot.preferences,
            ..Self::default()
        }
    }
}

/// Durable chrome/workspace preferences. The first contract keeps values
/// intentionally sparse; donor settings move here as their types become pure.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePreferences {
    pub theme_mode: ThemeModePreference,
    pub navigator_sidebar: NavigatorSidebarPreference,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeModePreference {
    #[default]
    FollowSystem,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NavigatorSidebarPreference {
    #[default]
    Start,
    End,
    Hidden,
}

/// Non-persisted effects emitted by reducers and consumed by composition glue.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEffects {
    pub effects: Vec<WorkspaceEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceEffect {
    PersistWorkspace { workspace_id: WorkspaceId },
    PersistPreferences { preferences: WorkspacePreferences },
    AppendGraphMutation(GraphMutationRecord),
    RequestMnem(mnem::MnemRequest),
    RequestSurface(SurfaceEffect),
    RouteEngine(EngineRouteRequest),
    EmitDiagnostic(DiagnosticRecord),
    RunTask(TaskRequest),
}

/// Typed mutation record for the journal seam. Detailed graph-delta variants
/// migrate here after the donor `GraphMutation` enum is free of service types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMutationRecord {
    pub sequence: u64,
    pub label: String,
    pub payload: MutationPayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationPayload {
    TypedJson(String),
    OpaqueBytes(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceEffect {
    pub host: SurfaceHostId,
    pub view: Option<GraphViewId>,
    pub pane: Option<PaneId>,
    pub request: SurfaceRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceRequest {
    Present,
    Retire,
    Focus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRouteRequest {
    pub workspace_id: WorkspaceId,
    pub view: Option<GraphViewId>,
    pub node: Option<NodeKey>,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRouteDecision {
    pub engine_id: String,
    pub surface_contract: SurfaceContract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceContract {
    pub host: SurfaceHostId,
    pub mode: SurfaceContractMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceContractMode {
    CompositedTexture,
    NativeOverlay,
    EmbeddedHost,
    Headless,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    pub channel: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRequest {
    pub kind: String,
    pub payload: Vec<u8>,
}

/// Persistence-friendly workspace envelope.
#[derive(Clone, Debug)]
pub struct GraphWorkspaceSnapshot {
    pub graph: GraphSnapshot,
    pub next_mutation_sequence: u64,
    pub views: WorkspaceViewSnapshot,
    pub workbench: WorkbenchState,
    pub chrome: ChromeSnapshot,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorkspaceViewSnapshot {
    pub graph_views: HashMap<GraphViewId, GraphViewState>,
    pub focused_view: Option<GraphViewId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeSnapshot {
    pub preferences: WorkspacePreferences,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalCursor {
    pub next_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceServiceError {
    pub message: String,
}

impl WorkspaceServiceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WorkspaceServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WorkspaceServiceError {}

/// Durable workspace snapshot repository.
pub trait WorkspaceRepository {
    fn load_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> WorkspaceServiceResult<Option<GraphWorkspaceSnapshot>>;

    fn save_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
        snapshot: &GraphWorkspaceSnapshot,
    ) -> WorkspaceServiceResult<()>;

    fn load_or_create_workbench_view_id(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> WorkspaceServiceResult<GraphViewId>;

    fn load_graph_tree(
        &mut self,
        view_id: &GraphViewId,
    ) -> WorkspaceServiceResult<Option<persistence::GraphTreeDocument>>;

    fn save_graph_tree(
        &mut self,
        view_id: &GraphViewId,
        document: &persistence::GraphTreeDocument,
    ) -> WorkspaceServiceResult<()>;

    fn load_workspace_layout(
        &mut self,
        name: &persistence::WorkspaceLayoutName,
    ) -> WorkspaceServiceResult<Option<persistence::WorkspaceLayoutDocument>>;

    fn save_workspace_layout(
        &mut self,
        name: &persistence::WorkspaceLayoutName,
        document: &persistence::WorkspaceLayoutDocument,
    ) -> WorkspaceServiceResult<()>;

    fn list_workspace_layouts(
        &mut self,
    ) -> WorkspaceServiceResult<Vec<persistence::WorkspaceLayoutName>>;
}

/// Append/replay lane for typed graph mutations and traversal events.
pub trait GraphMutationJournal {
    fn append_mutation(&mut self, record: &GraphMutationRecord) -> WorkspaceServiceResult<()>;

    fn replay_from(
        &mut self,
        cursor: Option<JournalCursor>,
    ) -> WorkspaceServiceResult<Vec<GraphMutationRecord>>;
}

/// Typed settings persistence for chrome and workspace preferences.
pub trait SettingsStore {
    fn load_preferences(&mut self) -> WorkspaceServiceResult<WorkspacePreferences>;

    fn save_preferences(
        &mut self,
        preferences: &WorkspacePreferences,
    ) -> WorkspaceServiceResult<()>;
}

/// Inker-facing route decision contract.
pub trait EngineRouter {
    fn route(
        &mut self,
        request: &EngineRouteRequest,
    ) -> WorkspaceServiceResult<EngineRouteDecision>;
}

/// Host/adapter sink for surface effects emitted by reducers.
pub trait SurfaceHost {
    fn apply_surface_effect(&mut self, effect: &SurfaceEffect) -> WorkspaceServiceResult<()>;
}

/// Runtime diagnostics sink for app-state and reducer tests.
pub trait DiagnosticsSink {
    fn emit(&mut self, record: &DiagnosticRecord);
}

/// Deterministic clock seam. Hosts provide time; reducers do not call platform
/// clocks directly.
pub trait Clock {
    fn now_unix_ms(&self) -> u64;
}

/// Task runtime seam for side effects that must leave reducer-owned state.
pub trait TaskRuntime {
    fn spawn(&mut self, task: TaskRequest) -> WorkspaceServiceResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_starts_with_no_host_owned_effects() {
        let workspace = GraphWorkspace::new();

        assert_eq!(workspace.domain.graph.node_count(), 0);
        assert!(workspace.views.graph_views.is_empty());
        assert!(workspace.workbench.frames.is_empty());
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn pending_effects_are_drained_by_composition_glue() {
        let mut workspace = GraphWorkspace::new();
        workspace.push_effect(WorkspaceEffect::EmitDiagnostic(DiagnosticRecord {
            channel: "graphshell.app_state".to_string(),
            message: "contract test".to_string(),
        }));

        let effects = workspace.drain_effects();

        assert_eq!(effects.len(), 1);
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn fake_settings_store_round_trips_preferences() {
        #[derive(Default)]
        struct FakeSettingsStore {
            preferences: WorkspacePreferences,
        }

        impl SettingsStore for FakeSettingsStore {
            fn load_preferences(&mut self) -> WorkspaceServiceResult<WorkspacePreferences> {
                Ok(self.preferences.clone())
            }

            fn save_preferences(
                &mut self,
                preferences: &WorkspacePreferences,
            ) -> WorkspaceServiceResult<()> {
                self.preferences = preferences.clone();
                Ok(())
            }
        }

        let preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Dark,
            navigator_sidebar: NavigatorSidebarPreference::End,
        };
        let mut store = FakeSettingsStore::default();

        store.save_preferences(&preferences).unwrap();

        assert_eq!(store.load_preferences().unwrap(), preferences);
    }
}
