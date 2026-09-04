// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Headed browser presenter for the Graphshell reference host.
//!
//! The whole crate is browser-only: it presents onto an
//! `HtmlCanvasElement` through WebGPU and stores through IndexedDB, neither
//! of which exists off wasm. The crate-level `cfg` below makes it compile to
//! nothing on a native host, so `cargo check --workspace` — the gate that
//! covers every other member — is not permanently red for a target this
//! crate was never meant to build for. It stays a workspace member rather
//! than an `exclude`d one so it keeps sharing the workspace lock and the
//! root `[patch]` table; excluding it would mean maintaining a second copy
//! of both.
//!
//! Check it for the target it is for:
//! `cargo check -p graphshell-web --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

mod web_events;
mod web_gpu;
mod web_product;
mod web_remote;
mod web_scenario;
mod web_view;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use graphshell::browser_storage::{StoragePersistence, decide, status_line};
use graphshell::client::{ActionDraft, ActionDraftSemantics, ActionDraftTarget};
use graphshell::endpoint::{IntentSink, ProjectionSource};
use graphshell::protocol::{IntentResult, ProjectionSession};
use mere::canvas::{Canvas, PhysicsBoard, PointerButton, project_canvas_strategy};
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::NodeKey;
use genet_render::TextSystem;
use netrender::Scene;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Document, Element, HtmlCanvasElement, HtmlInputElement, HtmlTextAreaElement, Window,
};

use graphshell::access::{AccessRecord, AccessRecordFilter, query_access_records};
use graphshell::app::GraphshellApp;
use graphshell::canary::FixtureEndpoint;
use graphshell::capture::{
    BROWSER_HISTORY_HANDLER_PREFIX, BrowserHistoryCapture, BrowserVisit, CaptureOutcome,
    ForgetMode, HistoryCapturePolicy,
};
use graphshell::mere_host::{
    FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef,
};
use graphshell::product::{ProjectionClock, RelationFamilyFilter, SavedSceneV1};
use graphshell::projection_editor::{
    Appearance, Channel, EditorAction, Encoding, Interaction, ProjectionDefinition,
    ProjectionDefinitionSink, ProjectionDraft, ProjectionEditor, ProjectionPanel, Provenance,
    Reading, SelectionMode, SourceBinding,
};
use graphshell::view::ProjectionLayoutView;
use graphshell_client::frozen::Satisfaction;
use muniment::IndexedDbBackend;
use uuid::Uuid;
use web_events::{install_events, schedule_frames};
use web_gpu::{GpuPresenter, PendingCapture};
use web_remote::RemoteLink;
use web_scenario::{DomAction, ScenarioRun};
use web_product::update_product_semantics;
use web_view::{ChromeModel, build_chrome_scene};

const REMOTE_LABEL: &str = "Remote projection · 2 objects";
const CAPTURE_POLICY_GLOBAL: &str = "graphshellCapturePolicyJson";
const CAPTURE_VISITS_GLOBAL: &str = "graphshellInitialVisitsJson";
const HISTORY_FILTER_GLOBAL: &str = "graphshellHistoryFilterJson";
const HISTORY_FORGET_GLOBAL: &str = "graphshellHistoryForgetJson";
const PROJECTION_EDITOR_STORAGE_KEY: &str = "graphshellProjectionDefinitionV1";

/// Stable fallback paint for an open backdrop kind. Product hosts can replace
/// this with native art; an unfamiliar remote scene still gets a distinct,
/// deterministic face from its wire data.
fn remote_backdrop_color(kind: &str) -> [f32; 4] {
    const PALETTE: [[f32; 4]; 5] = [
        [0.10, 0.19, 0.22, 1.0],
        [0.16, 0.17, 0.25, 1.0],
        [0.18, 0.14, 0.20, 1.0],
        [0.13, 0.21, 0.17, 1.0],
        [0.22, 0.18, 0.12, 1.0],
    ];
    let hash = kind.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    PALETTE[hash as usize % PALETTE.len()]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InitialCaptureSummary {
    active: bool,
    accepted: usize,
    dropped: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct HistoryFilterInput {
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    persona: Option<String>,
    device: Option<String>,
}

impl From<HistoryFilterInput> for AccessRecordFilter {
    fn from(value: HistoryFilterInput) -> Self {
        Self {
            start_ms: value.start_ms,
            end_ms: value.end_ms,
            persona: value.persona,
            device: value.device,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryForgetInput {
    url: String,
    #[serde(default)]
    remove_object: bool,
}

#[derive(Clone, Debug, Default)]
struct HistoryControlSummary {
    active: bool,
    records: Vec<AccessRecord>,
    forget_attempted: bool,
    forgotten: usize,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveSession {
    Local,
    Remote,
}

/// One opt-in playback of an analytic arrangement change. Scenotime owns the
/// schedule; this browser host owns the frame clock and the NodeKey binding.
struct CanvasTransition {
    schedule: scenotime::TransitionSchedule,
    clock: ProjectionClock,
    node_of: HashMap<sceno::InstanceId, NodeKey>,
    start_positions: Vec<(NodeKey, PortablePoint)>,
    final_positions: Vec<(NodeKey, PortablePoint)>,
}

impl CanvasTransition {
    fn between(
        canvas: &Canvas,
        final_positions: &[(NodeKey, PortablePoint)],
    ) -> Result<Option<Self>, String> {
        let final_of = final_positions.iter().copied().collect::<HashMap<_, _>>();
        let old_geometry = canvas.cartography_geometry();
        let old_of = old_geometry.iter().collect::<HashMap<_, _>>();
        let extents = canvas.strategy_extents();
        let mut nodes = canvas.graph().nodes().collect::<Vec<_>>();
        nodes.sort_by_key(|(_, node)| node.id);

        let mut scene = sceno::Scene::new();
        scene.generation = canvas.graph().revision();
        let mut node_of = HashMap::new();
        let mut start_positions = Vec::with_capacity(nodes.len());
        let mut operations = Vec::new();
        for (key, node) in nodes {
            let Some(target) = final_of.get(&key).copied() else {
                continue;
            };
            let current = old_of
                .get(&node.id)
                .map(|(x, y)| PortablePoint::new(*x, *y))
                .unwrap_or(target);
            let source = scene.intern_source(sceno::SourceRef::new(
                mere::canvas::MERE_GRAPH_ADAPTER,
                node.id.to_string(),
            ));
            let (width, height) = extents.get(&key).copied().unwrap_or((36.0, 36.0));
            let item = sceno::ProjectedItem {
                source,
                space: sceno::Scene::WORLD,
                transform: sceno::Transform2::translation(current.x, current.y),
                footprint: sceno::Footprint::Rect {
                    size: sceno::Size2::new(width, height),
                },
                representation: canvas
                    .projection_representation(key)
                    .cloned()
                    .unwrap_or(sceno::Representation::Card),
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            };
            let instance = sceno::InstanceId(scene.items.len() as u32);
            node_of.insert(instance, key);
            start_positions.push((key, current));
            scene.items.push(item.clone());
            if current != target {
                let mut target_item = item;
                target_item.transform = sceno::Transform2::translation(target.x, target.y);
                operations.push(scenotime::SceneOp::UpdateItem {
                    index: instance,
                    value: target_item,
                });
            }
        }

        if operations.is_empty() {
            return Ok(None);
        }
        let before = scenotime::SceneSnapshot::from_dense(
            scenotime::SceneEpoch(1),
            scenotime::Revision(1),
            scene,
        )
        .map_err(|error| format!("could not build transition start: {error:?}"))?;
        let diff = scenotime::SceneDiff {
            epoch: before.epoch,
            base: before.revision,
            revision: scenotime::Revision(before.revision.0 + 1),
            operations,
        };
        let schedule = scenotime::TransitionSchedule::from_diff(
            &before,
            &diff,
            &scenotime::TransitionSpec::default(),
        )
        .map_err(|error| format!("could not schedule arrangement transition: {error:?}"))?;
        Ok(Some(Self {
            schedule,
            clock: ProjectionClock::default(),
            node_of,
            start_positions,
            final_positions: final_positions.to_vec(),
        }))
    }

    fn advance(&mut self, host_ms: f64) -> (Vec<(NodeKey, PortablePoint)>, bool) {
        let frame = self.schedule.sample_at(self.clock.observe(host_ms));
        if frame.complete {
            return (self.final_positions.clone(), true);
        }
        let mut positions = self
            .start_positions
            .iter()
            .copied()
            .collect::<HashMap<_, _>>();
        for sample in frame.items {
            let Some(key) = self.node_of.get(&sample.instance).copied() else {
                continue;
            };
            positions.insert(
                key,
                PortablePoint::new(
                    sample.value.transform.translate.x,
                    sample.value.transform.translate.y,
                ),
            );
        }
        let positions = self
            .start_positions
            .iter()
            .filter_map(|(key, _)| positions.get(key).copied().map(|position| (*key, position)))
            .collect();
        (positions, false)
    }
}

struct BrowserHost {
    app: GraphshellApp<IndexedDbBackend>,
    /// Where the remote projection lives (`web_remote`).
    remote: RemoteLink,
    /// The remote board's physics: the mounted scene's items as bodies, the
    /// score's positions as anchor slots, the local canvas's law mirrored.
    /// Synced whenever the acknowledged revision moves. (Physics catalog — P3.)
    remote_board: PhysicsBoard,
    remote_board_revision: Option<u64>,
    /// Whether the semantics mirror must recompute the layout statistics — a
    /// pairwise pass over every node — before it publishes them. Set while
    /// the local canvas reports motion, on the frame after it stops (the
    /// final tick moves bodies and reports rest in the same call), and on
    /// any chrome change; cleared when the mirror recomputes. Every frame
    /// paid for it before.
    layout_stats_stale: bool,
    /// The canvas's motion answer from the previous local frame.
    layout_moved: bool,
    /// The last layout statistics the mirror published.
    layout_stats: mere::canvas::LayoutStats,
    remote_session: Option<ProjectionSession>,
    remote_status: String,
    remote_joining: bool,
    remote_last_resume: String,
    active: ActiveSession,
    canvas: Canvas,
    canvas_element: HtmlCanvasElement,
    gpu: GpuPresenter,
    chrome_scene: Scene,
    /// Retained shaping state and the shipped font (see `run`).
    chrome_text: TextSystem,
    chrome_dirty: bool,
    detail_open: bool,
    action_count: u32,
    action_status: String,
    action_draft: Option<ActionDraft>,
    action_draft_target: Option<ActionDraftTarget>,
    rendered_action_draft: Option<ActionDraftSemantics>,
    action_draft_semantics_ready: bool,
    width: u32,
    height: u32,
    product_status: String,
    storage_status: String,
    storage_persistence: StoragePersistence,
    layout_id: String,
    physics_paused: bool,
    physics_damping: f32,
    handler_id: String,
    relation_family: RelationFamilyFilter,
    filter_count: usize,
    face: String,
    last_export: String,
    export_bytes: usize,
    imported_nodes: usize,
    saved_scene: Option<SavedSceneV1>,
    arrangement_transition: Option<CanvasTransition>,
    primary_member: Option<Uuid>,
    last_detail_member: Option<Uuid>,
    projection_editor: ProjectionEditor,
    projection_editor_open: bool,
    projection_editor_status: String,
    projection_editor_save_count: u32,
    /// The scenario lane (`web_scenario`): a script in flight, the semantic
    /// events it asserts against, and a capture armed or landing.
    scenario: Option<ScenarioRun>,
    scenario_frames: u32,
    probe_events: Vec<String>,
    /// DOM events a scenario step asked for, dispatched by the frame pump
    /// once it has let go of the host (`web_scenario::DomAction`).
    deferred_dom: Vec<DomAction>,
    capture_request: Option<String>,
    capture_pending: Option<(String, PendingCapture)>,
    capture_count: u32,
}

struct BrowserProjectionSink;

impl ProjectionDefinitionSink for BrowserProjectionSink {
    type Error = String;

    fn save(&mut self, definition: &ProjectionDefinition) -> Result<(), Self::Error> {
        let bytes = definition
            .to_json_bytes()
            .map_err(|error| format!("could not serialize projection definition: {error}"))?;
        let storage = window()?
            .local_storage()
            .map_err(|_| "could not access browser local storage".to_string())?
            .ok_or_else(|| "browser local storage is unavailable".to_string())?;
        let value = String::from_utf8(bytes)
            .map_err(|error| format!("projection definition was not UTF-8: {error}"))?;
        storage
            .set_item(PROJECTION_EDITOR_STORAGE_KEY, &value)
            .map_err(|_| "could not save projection definition".to_string())
    }
}

fn initial_projection_draft() -> ProjectionDraft {
    ProjectionDraft {
        version: graphshell::projection_editor::PROJECTION_DEFINITION_VERSION,
        id: "graphshell-reference".to_string(),
        label: "Graphshell reference projection".to_string(),
        source: SourceBinding {
            authority: "graphshell-reference-host".to_string(),
            domain: "mere.graph".to_string(),
            resource: "fixture.graphshell/reference".to_string(),
        },
        reading: Reading {
            kind: "nodes".to_string(),
            key: "title".to_string(),
            value: None,
        },
        encoding: Encoding {
            x: Channel::Field("x".to_string()),
            y: Channel::Field("y".to_string()),
            color: Some(Channel::Field("kind".to_string())),
            label: Some(Channel::Field("title".to_string())),
        },
        arrangement: graphshell::projection_editor::Arrangement {
            kind: "phyllotaxis.default".to_string(),
            direction: "horizontal".to_string(),
            spacing: 16,
        },
        interaction: Interaction {
            selection: SelectionMode::Single,
            pan: true,
            zoom: true,
        },
        appearance: Appearance {
            realization: "canvas".to_string(),
            title: "Graphshell reference projection".to_string(),
            theme: "dark".to_string(),
        },
        provenance: Provenance {
            author: "Graphshell reference host".to_string(),
            source_revision: "fixture-v1".to_string(),
            note: "Host-owned editor fixture".to_string(),
        },
    }
}

fn draft_from_definition(definition: &ProjectionDefinition) -> ProjectionDraft {
    ProjectionDraft {
        version: definition.version,
        id: definition.id.clone(),
        label: definition.label.clone(),
        source: definition.source.clone(),
        reading: definition.reading.clone(),
        encoding: definition.encoding.clone(),
        arrangement: definition.arrangement.clone(),
        interaction: definition.interaction.clone(),
        appearance: definition.appearance.clone(),
        provenance: definition.provenance.clone(),
    }
}

impl BrowserHost {
    fn current_primary_member(&self) -> Option<Uuid> {
        self.canvas.focused_member().or(self.primary_member)
    }

    fn chrome_model(&self) -> ChromeModel {
        let (selection, detail_address) = match self.active {
            ActiveSession::Local => self
                .current_primary_member()
                .and_then(|id| self.app.host.graph().get_node_by_id(id))
                .map(|(_, node)| {
                    let title = if node.title.trim().is_empty() {
                        node.url().to_string()
                    } else {
                        node.title.clone()
                    };
                    (title, node.url().to_string())
                })
                .unwrap_or_else(|| {
                    (
                        "No object selected".to_string(),
                        "Select an object".to_string(),
                    )
                }),
            ActiveSession::Remote => self.remote_selection(),
        };
        let (product_status, arrangement, physics_law, physics_paused) = self.product_chrome();
        // Satisfaction belongs to the remote scene, so it is only spoken when
        // one is mounted. A local canvas has no holds to report on.
        let satisfaction = self
            .remote_mounted()
            .and_then(|mounted| Satisfaction::of(&mounted.scene.tables).line())
            .unwrap_or_default();
        ChromeModel {
            active_session: match self.active {
                ActiveSession::Local => format!(
                    "Local Mere · {} objects",
                    self.app.host.graph().node_count()
                ),
                ActiveSession::Remote => self.remote_label(),
            },
            local_active: self.active == ActiveSession::Local,
            detail_open: self.detail_open,
            detail_address,
            selection,
            action_status: self.action_status.clone(),
            viewport_label: format!(
                "{} × {} · {}",
                self.width,
                self.height,
                if self.width < 720 { "narrow" } else { "wide" }
            ),
            product_status,
            satisfaction,
            arrangement,
            physics_law,
            physics_paused,
            action_draft: self.action_draft.as_ref().map(ActionDraft::semantics),
        }
    }

    fn resize_if_needed(&mut self) {
        let width = self.canvas_element.client_width().max(1) as u32;
        let height = self.canvas_element.client_height().max(1) as u32;
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.canvas_element.set_width(width);
        self.canvas_element.set_height(height);
        self.gpu.resize(width, height);
        self.canvas.resize(width, height);
        self.chrome_dirty = true;
    }

    fn render(&mut self, host_ms: f64) -> Result<(), String> {
        self.scenario_frames = self.scenario_frames.wrapping_add(1);
        self.finish_capture()?;
        self.resize_if_needed();
        self.advance_arrangement_transition(host_ms);
        if self.chrome_dirty {
            // A chrome change follows every host command (a law, a profile, an
            // arrangement), which may move bodies without a settle.
            self.layout_stats_stale = true;
            self.chrome_scene = build_chrome_scene(
                self.chrome_model(),
                self.width,
                self.height,
                &mut self.chrome_text,
            )?;
            self.chrome_dirty = false;
        }
        let content = match self.active {
            ActiveSession::Local => {
                let (scene, moving) = self.canvas.frame(self.width, self.height);
                self.layout_stats_stale |= moving || self.layout_moved;
                self.layout_moved = moving;
                scene
            }
            ActiveSession::Remote => {
                // The board's physics runs only while the board is shown: sync
                // to the acknowledged scene, tick, then draw from the bodies.
                self.sync_remote_board();
                self.remote_board.tick();
                self.remote_scene()
            }
        };
        if let Some(name) = self.capture_request.take() {
            // The same two scenes this frame presents, composed into an owned
            // target: a capture is the frame, not a re-rendering of it.
            let pending = self
                .gpu
                .capture(&content, &self.chrome_scene, self.width, self.height);
            self.capture_pending = Some((name, pending));
        }
        self.gpu
            .present(&content, &self.chrome_scene, self.width, self.height)
    }

    /// Publish a capture whose readback has landed, if any.
    fn finish_capture(&mut self) -> Result<(), String> {
        let landed = self
            .capture_pending
            .as_ref()
            .is_some_and(|(_, pending)| pending.ready());
        if !landed {
            return Ok(());
        }
        let Some((name, pending)) = self.capture_pending.take() else {
            return Ok(());
        };
        let (width, height, rgba) = pending.take()?;
        web_scenario::publish_capture(&name, width, height, &rgba)?;
        self.capture_count += 1;
        self.probe_events.push(format!("capture-done {name} {width}x{height}"));
        Ok(())
    }

    fn begin_arrangement_transition(
        &mut self,
        final_positions: &[(NodeKey, PortablePoint)],
    ) -> Result<bool, String> {
        self.arrangement_transition = CanvasTransition::between(&self.canvas, final_positions)?;
        Ok(self.arrangement_transition.is_some())
    }

    fn advance_arrangement_transition(&mut self, host_ms: f64) {
        let Some((positions, complete)) = self
            .arrangement_transition
            .as_mut()
            .map(|transition| transition.advance(host_ms))
        else {
            return;
        };
        if complete {
            self.canvas.apply_strategy_positions(&positions);
            self.arrangement_transition = None;
            self.product_status = format!("Arrangement set to {}", self.layout_id);
            self.chrome_dirty = true;
        } else {
            self.canvas.preview_strategy_positions(&positions);
        }
    }

    fn remote_scene(&self) -> Scene {
        let mut scene = Scene::new(self.width, self.height);
        scene.push_rect(
            0.0,
            0.0,
            self.width as f32,
            self.height as f32,
            [0.025, 0.045, 0.057, 1.0],
        );
        let Some(mounted) = self.remote_mounted() else {
            return scene;
        };
        let bounds = mounted.scene.tables.bounds;
        let scale = ((self.width as f32 - 100.0) / bounds.size.w.max(1.0))
            .min((self.height as f32 - 180.0) / bounds.size.h.max(1.0))
            .min(1.0);
        let origin_x = (self.width as f32 - bounds.size.w * scale) * 0.5 - bounds.origin.x * scale;
        let origin_y = 116.0 - bounds.origin.y * scale;
        let layout = ProjectionLayoutView::from_scene(&mounted.scene);
        for backdrop in &layout.backdrops {
            let x0 = origin_x + backdrop.x * scale;
            let y0 = origin_y + backdrop.y * scale;
            let x1 = x0 + backdrop.width * scale;
            let y1 = y0 + backdrop.height * scale;
            let color = remote_backdrop_color(&backdrop.kind);
            scene.push_rect(x0, y0, x1, y1, color);
            if backdrop.collidable {
                let stroke = 2.0;
                let edge = [0.78, 0.61, 0.31, 0.9];
                scene.push_rect(x0, y0, x1, y0 + stroke, edge);
                scene.push_rect(x0, y1 - stroke, x1, y1, edge);
                scene.push_rect(x0, y0, x0 + stroke, y1, edge);
                scene.push_rect(x1 - stroke, y0, x1, y1, edge);
            }
        }
        for (instance, item) in mounted.scene.active_items_in_order() {
            // Where the physics put the card, falling back to the score's
            // own position for an item the board has not seen yet.
            let (x, y) = self
                .remote_board
                .position(&instance.0.to_string())
                .unwrap_or((item.transform.translate.x, item.transform.translate.y));
            let center_x = origin_x + x * scale;
            let center_y = origin_y + y * scale;
            let (card_w, card_h) = match item.footprint {
                sceno::Footprint::Rect { size } => (size.w * scale, size.h * scale),
                _ => (120.0, 80.0),
            };
            // An item sitting where a person put it should not look identical
            // to one the arrangement happened to place there. Read from the
            // snapshot rather than inferred, which is why A1 puts the honored
            // half on the wire beside the unmet half.
            let pinned = Satisfaction::is_pinned(&mounted.scene.tables, instance);
            let color = if instance.0 == 0 {
                [0.16, 0.31, 0.35, 1.0]
            } else {
                [0.23, 0.28, 0.38, 1.0]
            };
            if pinned {
                // A held edge, drawn outside the card so it reads as something
                // done to the item rather than part of it.
                scene.push_rect(
                    center_x - card_w * 0.5 - 3.0,
                    center_y - card_h * 0.5 - 3.0,
                    center_x + card_w * 0.5 + 3.0,
                    center_y + card_h * 0.5 + 3.0,
                    [0.85, 0.72, 0.35, 1.0],
                );
            }
            scene.push_rect(
                center_x - card_w * 0.5 - 5.0,
                center_y - card_h * 0.5 + 6.0,
                center_x + card_w * 0.5 + 5.0,
                center_y + card_h * 0.5 + 11.0,
                [0.01, 0.02, 0.025, 0.45],
            );
            scene.push_rect(
                center_x - card_w * 0.5,
                center_y - card_h * 0.5,
                center_x + card_w * 0.5,
                center_y + card_h * 0.5,
                color,
            );
        }
        scene
    }

    /// Run one named command. `false` when no such command exists, so a
    /// scenario's `act` fails loudly rather than silently doing nothing.
    fn run_command(&mut self, command: &str) -> bool {
        self.probe_events.push(format!("command {command}"));
        match command {
            "open-projection-editor" => {
                self.projection_editor_open = true;
                self.projection_editor_status = "Draft ready · unsaved".to_string();
            }
            "close-projection-editor" => {
                self.projection_editor_open = false;
            }
            "save-projection" => self.save_projection(),
            "reload-projection" => self.reload_projection(),
            "session-local" => {
                self.active = ActiveSession::Local;
                self.detail_open = false;
            }
            "session-remote" => {
                self.active = ActiveSession::Remote;
                self.detail_open = false;
            }
            "select-web" => {
                self.active = ActiveSession::Local;
                self.canvas.select_by_url(FIXTURE_WEB_ADDRESS);
                self.primary_member = self.canvas.focused_member();
                self.detail_open = false;
            }
            "open-detail" => {
                if self.active == ActiveSession::Local && self.current_primary_member().is_none() {
                    self.canvas.select_by_url(FIXTURE_WEB_ADDRESS);
                    self.primary_member = self.canvas.focused_member();
                }
                self.detail_open = true;
            }
            "close-detail" => self.detail_open = false,
            "invoke-action" => self.invoke_action(),
            "submit-action-draft" => self.submit_action_draft(),
            "zoom-in" => self.zoom(40.0),
            "zoom-out" => self.zoom(-40.0),
            "pan-left" => self.pan(-42.0, 0.0),
            "pan-right" => self.pan(42.0, 0.0),
            "pan-up" => self.pan(0.0, -42.0),
            "pan-down" => self.pan(0.0, 42.0),
            "remote-disconnect" => self.remote_disconnect(),
            "remote-reconnect" => self.remote_reconnect(),
            "remote-nudge" => self.remote_nudge(),
            other if other.starts_with("remote-action-") => {
                match other["remote-action-".len()..].parse::<usize>() {
                    Ok(index) => self.invoke_remote_action(index),
                    Err(_) => {
                        self.probe_events.push(format!("command-unknown {command}"));
                        return false;
                    }
                }
            }
            _ => {
                if !self.run_product_command(command) {
                    self.probe_events.push(format!("command-unknown {command}"));
                    return false;
                }
            }
        }
        if self.active == ActiveSession::Local {
            self.refresh_representation_score();
        }
        self.chrome_dirty = true;
        true
    }

    fn select_projection_panel(&mut self, panel: &str) {
        let Some(panel) = projection_panel(panel) else {
            self.projection_editor_status = format!("Unknown editor panel: {panel}");
            return;
        };
        self.projection_editor
            .reduce(EditorAction::SelectPanel(panel));
        self.projection_editor_open = true;
        self.projection_editor_status = format!("Editing {}", panel.label());
        self.chrome_dirty = true;
    }

    fn update_projection_field(&mut self, field: &str, value: &str) {
        let mut draft = self.projection_editor.draft().clone();
        let action = match field {
            "source.authority" => {
                draft.source.authority = value.to_string();
                EditorAction::SetSource(draft.source)
            }
            "source.domain" => {
                draft.source.domain = value.to_string();
                EditorAction::SetSource(draft.source)
            }
            "source.resource" => {
                draft.source.resource = value.to_string();
                EditorAction::SetSource(draft.source)
            }
            "reading.key" => {
                draft.reading.key = value.to_string();
                EditorAction::SetReading(draft.reading)
            }
            "encoding.x" => {
                draft.encoding.x = Channel::Field(value.to_string());
                EditorAction::SetEncoding(draft.encoding)
            }
            "encoding.y" => {
                draft.encoding.y = Channel::Field(value.to_string());
                EditorAction::SetEncoding(draft.encoding)
            }
            "arrangement.kind" => {
                draft.arrangement.kind = value.to_string();
                EditorAction::SetArrangement(draft.arrangement)
            }
            "arrangement.direction" => {
                draft.arrangement.direction = value.to_string();
                EditorAction::SetArrangement(draft.arrangement)
            }
            "arrangement.spacing" => match value.parse::<u32>() {
                Ok(spacing) => {
                    draft.arrangement.spacing = spacing;
                    EditorAction::SetArrangement(draft.arrangement)
                }
                Err(_) => {
                    self.projection_editor_status =
                        "Invalid · arrangement.spacing must be a number".to_string();
                    self.projection_editor_open = true;
                    self.chrome_dirty = true;
                    return;
                }
            },
            "appearance.realization" => {
                draft.appearance.realization = value.to_string();
                EditorAction::SetAppearance(draft.appearance)
            }
            "appearance.title" => {
                draft.appearance.title = value.to_string();
                EditorAction::SetAppearance(draft.appearance)
            }
            "provenance.author" => {
                draft.provenance.author = value.to_string();
                EditorAction::SetProvenance(draft.provenance)
            }
            "provenance.source_revision" => {
                draft.provenance.source_revision = value.to_string();
                EditorAction::SetProvenance(draft.provenance)
            }
            "provenance.note" => {
                draft.provenance.note = value.to_string();
                EditorAction::SetProvenance(draft.provenance)
            }
            _ => return,
        };
        self.projection_editor.reduce(action);
        self.projection_editor_status = format!("Edited · {field}");
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }

    fn save_projection(&mut self) {
        let mut sink = BrowserProjectionSink;
        match self.projection_editor.save(&mut sink) {
            Ok(()) => {
                self.projection_editor_save_count =
                    self.projection_editor_save_count.saturating_add(1);
                self.projection_editor_status = format!(
                    "Saved · {} · {} save(s)",
                    self.projection_editor.draft().provenance.source_revision,
                    self.projection_editor_save_count
                );
            }
            Err(graphshell::projection_editor::SaveError::Invalid(issues)) => {
                let summary = issues
                    .first()
                    .map(|issue| format!("{}: {}", issue.field, issue.message))
                    .unwrap_or_else(|| "invalid projection draft".to_string());
                self.projection_editor_status = format!("Invalid · {summary}");
            }
            Err(graphshell::projection_editor::SaveError::Sink(error)) => {
                self.projection_editor_status = format!("Save failed · {error}");
            }
        }
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }

    fn reload_projection(&mut self) {
        let result = (|| -> Result<Option<ProjectionDefinition>, String> {
            let storage = window()?
                .local_storage()
                .map_err(|_| "could not access browser local storage".to_string())?;
            let Some(storage) = storage else {
                return Ok(None);
            };
            let Some(value) = storage
                .get_item(PROJECTION_EDITOR_STORAGE_KEY)
                .map_err(|_| "could not read saved projection definition".to_string())?
            else {
                return Ok(None);
            };
            serde_json::from_str(&value)
                .map(Some)
                .map_err(|error| format!("could not decode saved projection definition: {error}"))
        })();
        match result {
            Ok(Some(definition)) => {
                let draft = draft_from_definition(&definition);
                match draft.to_definition() {
                    Ok(_) => {
                        self.projection_editor = ProjectionEditor::new(draft);
                        self.projection_editor_status = format!(
                            "Reloaded · {} · {}",
                            definition.id, definition.provenance.source_revision
                        );
                    }
                    Err(issues) => {
                        self.projection_editor_status = format!(
                            "Reload failed · {}",
                            issues
                                .first()
                                .map(|issue| issue.message.as_str())
                                .unwrap_or("invalid saved definition")
                        );
                    }
                }
            }
            Ok(None) => {
                self.projection_editor_status = "Reload skipped · no saved definition".to_string();
            }
            Err(error) => self.projection_editor_status = format!("Reload failed · {error}"),
        }
        self.projection_editor_open = true;
        self.chrome_dirty = true;
    }

    fn zoom(&mut self, delta: f32) {
        if self.active != ActiveSession::Local {
            return;
        }
        self.canvas
            .cursor_moved(self.width as f32 * 0.5, self.height as f32 * 0.5);
        self.canvas.set_ctrl(true);
        self.canvas.wheel(0.0, delta);
        self.canvas.set_ctrl(false);
    }

    fn pan(&mut self, dx: f32, dy: f32) {
        if self.active == ActiveSession::Local {
            self.canvas.wheel(dx, dy);
        }
    }

    fn invoke_action(&mut self) {
        if self.active == ActiveSession::Remote {
            self.open_remote_action_draft();
            self.detail_open = true;
            return;
        }
        self.handler_id =
            web_product::selected_handler().unwrap_or_else(|_| self.handler_id.clone());
        let address = self
            .current_primary_member()
            .and_then(|id| self.app.host.graph().get_node_by_id(id))
            .map(|(_, node)| node.url().to_string())
            .unwrap_or_else(|| FIXTURE_WEB_ADDRESS.to_string());
        let result = self
            .app
            .open_address(&address, &self.handler_id)
            .map_err(|error| error.to_string());
        self.action_count = self.action_count.saturating_add(1);
        self.action_status = match result {
            Ok(IntentResult::Accepted)
                if self.active == ActiveSession::Local && self.handler_id == "system.default" =>
            {
                match window().and_then(|window| {
                    window
                        .open_with_url_and_target(&address, "_blank")
                        .map_err(|_| "host-browser open failed".to_string())
                }) {
                    Ok(Some(_)) => format!(
                        "Accepted · opened in host browser · {} invocation(s)",
                        self.action_count
                    ),
                    Ok(None) => "Failed · host browser blocked the external open".to_string(),
                    Err(error) => format!("Failed · {error}"),
                }
            }
            Ok(IntentResult::Accepted) => format!("Accepted · {} invocation(s)", self.action_count),
            Ok(other) => format!("{other:?}"),
            Err(error) => format!("Failed · {error}"),
        };
        self.detail_open = true;
    }

    fn open_remote_action_draft(&mut self) {
        let Some(session) = self.remote_session.clone() else {
            self.action_status = "Failed · remote projection is not mounted".to_string();
            return;
        };
        let Some((observed_epoch, observed_revision)) = self
            .remote_mounted()
            .map(|mounted| (mounted.scene.epoch, mounted.scene.revision))
        else {
            self.action_status = "Failed · remote projection is not mounted".to_string();
            return;
        };
        let tree = match self
            .remote_client()
            .ok_or("remote link is not discovered".to_string())
            .and_then(|client| {
                client
                    .accessibility_tree(&session, &web_remote::remote_profile())
                    .map_err(|error| format!("{error:?}"))
            }) {
            Ok(tree) => tree,
            Err(error) => {
                self.action_status = format!("Failed · remote accessibility tree: {error}");
                return;
            }
        };
        // A bounded form opens as a draft. Plain actions are offered as
        // buttons by `update_remote_semantics`; opening the detail is enough.
        let Some((target, action)) = tree.children.iter().find_map(|item| {
            item.actions
                .iter()
                .find(|action| action.input_form.is_some())
                .cloned()
                .map(|action| (item.instance, action))
        }) else {
            let count = tree
                .children
                .iter()
                .map(|item| item.actions.len())
                .sum::<usize>();
            self.action_status = format!("{count} remote action(s) advertised");
            return;
        };
        self.action_status = format!("Choose values · {}", action.label);
        self.action_draft = Some(ActionDraft::new(action));
        self.action_draft_target = Some(ActionDraftTarget {
            session,
            target,
            observed_epoch,
            observed_revision,
        });
    }

    fn choose_action_draft(&mut self, field: &str, value: &str) {
        let Some(draft) = self.action_draft.as_mut() else {
            self.action_status = "Failed · no remote action draft is open".to_string();
            return;
        };
        self.action_status = match draft.choose(field, value) {
            Ok(()) => format!("Selected {field}"),
            Err(error) => format!("Choose values · {error}"),
        };
        self.chrome_dirty = true;
    }

    fn submit_action_draft(&mut self) {
        if matches!(self.remote, RemoteLink::WebRtc(_)) {
            self.submit_remote_draft();
            return;
        }
        let Some(target) = self.action_draft_target.clone() else {
            self.action_status = "Failed · no remote action draft target is open".to_string();
            return;
        };
        let Some(draft) = self.action_draft.as_mut() else {
            self.action_status = "Failed · no remote action draft is open".to_string();
            return;
        };
        let invocation = match draft.invocation(&target) {
            Ok(invocation) => invocation,
            Err(error) => {
                self.action_status = format!("Choose required values · {error}");
                self.detail_open = true;
                return;
            }
        };
        self.action_count = self.action_count.saturating_add(1);
        let RemoteLink::Fixture(fixture) = &mut self.remote else {
            return;
        };
        match fixture.invoke(invocation) {
            Ok(IntentResult::Accepted) => match fixture.snapshot(fixture.request()) {
                Ok(snapshot) => match self.app.mount_remote(snapshot) {
                    Ok(session) => {
                        let revision = self
                            .app
                            .client
                            .mounted(&session)
                            .map(|mounted| mounted.scene.revision.0)
                            .unwrap_or_default();
                        self.remote_session = Some(session);
                        self.action_status = format!(
                            "Accepted · resnapshotted revision {revision} · {} invocation(s)",
                            self.action_count
                        );
                        self.action_draft = None;
                        self.action_draft_target = None;
                    }
                    Err(error) => {
                        self.action_status =
                            format!("Accepted · failed to mount resnapshot: {error}");
                    }
                },
                Err(error) => {
                    self.action_status =
                        format!("Accepted · failed to request resnapshot: {error}");
                }
            },
            Ok(IntentResult::Stale { .. }) => {
                self.action_status = "Stale · reopen the remote action form".to_string();
                self.action_draft = None;
                self.action_draft_target = None;
            }
            Ok(IntentResult::Rejected { reason }) => {
                self.action_status = format!("Rejected · {reason}");
            }
            Err(error) => self.action_status = format!("Failed · {error}"),
        }
        self.detail_open = true;
    }

    fn pointer_position(&self, x: i32, y: i32) -> (f32, f32) {
        let bounds = self.canvas_element.get_bounding_client_rect();
        (
            x as f32 - bounds.left() as f32,
            y as f32 - bounds.top() as f32,
        )
    }

    fn pointer_button(button: i16) -> Option<PointerButton> {
        match button {
            0 => Some(PointerButton::Left),
            1 => Some(PointerButton::Middle),
            2 => Some(PointerButton::Right),
            _ => None,
        }
    }
}

/// Ask the browser whether this origin's storage is kept, requesting it when
/// it is not.
///
/// Every failure path lands on `Unknown` with its reason rather than on
/// `Refused`. An insecure context and a browser that declined are different
/// facts, and only one of them changes if the person installs the resident
/// host.
async fn resolve_storage_persistence() -> StoragePersistence {
    let Ok(window) = window() else {
        return StoragePersistence::Unknown("browser window is unavailable".to_string());
    };
    let manager = window.navigator().storage();
    let persisted = match manager.persisted() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(value) => value
                .as_bool()
                .ok_or_else(|| "persisted() did not answer with a boolean".to_string()),
            Err(error) => Err(format!("persisted() failed: {error:?}")),
        },
        Err(error) => Err(format!("storage persistence is unavailable: {error:?}")),
    };
    // `decide` takes the request as a closure so it is never made when the
    // answer is already yes; awaiting inside one needs the future built first.
    let requested = if matches!(persisted, Ok(false)) {
        match manager.persist() {
            Ok(promise) => match JsFuture::from(promise).await {
                Ok(value) => value
                    .as_bool()
                    .ok_or_else(|| "persist() did not answer with a boolean".to_string()),
                Err(error) => Err(format!("persist() failed: {error:?}")),
            },
            Err(error) => Err(format!("persist() is unavailable: {error:?}")),
        }
    } else {
        Ok(false)
    };
    decide(persisted, move || requested)
}

fn window() -> Result<Window, String> {
    web_sys::window().ok_or_else(|| "browser window is unavailable".to_string())
}

fn document() -> Result<Document, String> {
    window()?
        .document()
        .ok_or_else(|| "browser document is unavailable".to_string())
}

fn projection_panel(value: &str) -> Option<ProjectionPanel> {
    match value {
        "source" => Some(ProjectionPanel::Source),
        "reading" => Some(ProjectionPanel::Reading),
        "encoding" => Some(ProjectionPanel::Encoding),
        "arrangement" => Some(ProjectionPanel::Arrangement),
        "interaction" => Some(ProjectionPanel::Interaction),
        "preview" => Some(ProjectionPanel::Preview),
        "provenance" => Some(ProjectionPanel::Provenance),
        _ => None,
    }
}

thread_local! {
    /// The element `mount` was given: the component's whole world.
    static ROOT: RefCell<Option<Element>> = const { RefCell::new(None) };
}

/// The mount root. Every lookup, token and listener of the component goes
/// through it, never through the document, so a host page's own ids and
/// keys are never in play.
fn root() -> Result<Element, String> {
    ROOT.with(|slot| slot.borrow().clone())
        .ok_or_else(|| "the component is not mounted".to_string())
}

/// One of the component's parts, by its unprefixed name (`detail-surface`
/// for `#gs-detail-surface`), found under the root.
fn element(part: &str) -> Result<Element, String> {
    root()?
        .query_selector(&format!("#gs-{part}"))
        .map_err(|_| format!("bad part name {part}"))?
        .ok_or_else(|| format!("missing part {part}"))
}

fn set_text(part: &str, value: &str) {
    if let Ok(element) = element(part) {
        element.set_text_content(Some(value));
    }
}

fn set_attr(element: &Element, name: &str, value: &str) -> Result<(), String> {
    element
        .set_attribute(name, value)
        .map_err(|_| format!("could not set {name} on #{}", element.id()))
}

/// Rebuild the browser's semantic controls from the same renderer-neutral
/// draft that Cambium paints. Endpoint fields and choices stay opaque values;
/// this bridge only gives their advertised labels a native HTML control.
fn update_action_draft_semantics(
    document: &Document,
    draft: Option<&ActionDraftSemantics>,
) -> Result<(), String> {
    let surface = element("action-draft-surface")?;
    surface.set_text_content(None);
    let body = root()?;
    let Some(draft) = draft else {
        surface
            .set_attribute("hidden", "")
            .map_err(|_| "could not hide action draft surface")?;
        surface
            .set_attribute("aria-hidden", "true")
            .map_err(|_| "could not hide action draft semantics")?;
        body.set_attribute("data-action-draft-open", "false")
            .map_err(|_| "could not expose closed action draft")?;
        body.remove_attribute("data-action-draft-fields")
            .map_err(|_| "could not clear action draft fields")?;
        body.remove_attribute("data-action-draft-error")
            .map_err(|_| "could not clear action draft error")?;
        return Ok(());
    };
    surface
        .remove_attribute("hidden")
        .map_err(|_| "could not show action draft surface")?;
    surface
        .set_attribute("aria-hidden", "false")
        .map_err(|_| "could not expose action draft semantics")?;
    let title = document
        .create_element("h2")
        .map_err(|_| "could not create action draft title")?;
    title.set_text_content(Some(&draft.label));
    surface
        .append_child(&title)
        .map_err(|_| "could not append action draft title")?;
    let explanation = document
        .create_element("p")
        .map_err(|_| "could not create action draft explanation")?;
    explanation.set_text_content(Some(&draft.explanation));
    surface
        .append_child(&explanation)
        .map_err(|_| "could not append action draft explanation")?;

    for (field_index, field) in draft.fields.iter().enumerate() {
        let fieldset = document
            .create_element("fieldset")
            .map_err(|_| "could not create action field")?;
        let legend = document
            .create_element("legend")
            .map_err(|_| "could not create action field label")?;
        let requirement = if field.required {
            "required"
        } else {
            "optional"
        };
        legend.set_text_content(Some(&format!("{} ({requirement})", field.label)));
        fieldset
            .append_child(&legend)
            .map_err(|_| "could not append action field label")?;
        let description_id = format!("gs-action-draft-help-{field_index}");
        let description = document
            .create_element("p")
            .map_err(|_| "could not create action field description")?;
        description
            .set_attribute("id", &description_id)
            .map_err(|_| "could not name action field description")?;
        description.set_text_content(Some(&field.description));
        fieldset
            .append_child(&description)
            .map_err(|_| "could not append action field description")?;
        let select = document
            .create_element("select")
            .map_err(|_| "could not create action field select")?;
        select
            .set_attribute("data-action-draft-field", &field.name)
            .map_err(|_| "could not name action field select")?;
        select
            .set_attribute("aria-label", &field.label)
            .map_err(|_| "could not label action field select")?;
        select
            .set_attribute("aria-describedby", &description_id)
            .map_err(|_| "could not describe action field select")?;
        if field.required {
            select
                .set_attribute("required", "")
                .map_err(|_| "could not require action field select")?;
        }
        if !field.choices.iter().any(|choice| choice.selected) {
            let placeholder = document
                .create_element("option")
                .map_err(|_| "could not create action choice placeholder")?;
            placeholder
                .set_attribute("value", "")
                .map_err(|_| "could not set action choice placeholder")?;
            placeholder
                .set_attribute("disabled", "")
                .map_err(|_| "could not disable action choice placeholder")?;
            placeholder
                .set_attribute("selected", "")
                .map_err(|_| "could not select action choice placeholder")?;
            placeholder.set_text_content(Some("Choose an advertised value"));
            select
                .append_child(&placeholder)
                .map_err(|_| "could not append action choice placeholder")?;
        }
        for choice in &field.choices {
            let option = document
                .create_element("option")
                .map_err(|_| "could not create action choice")?;
            option
                .set_attribute("value", &choice.value)
                .map_err(|_| "could not set action choice value")?;
            if choice.selected {
                option
                    .set_attribute("selected", "")
                    .map_err(|_| "could not select action choice")?;
            }
            option.set_text_content(Some(&choice.label));
            select
                .append_child(&option)
                .map_err(|_| "could not append action choice")?;
        }
        fieldset
            .append_child(&select)
            .map_err(|_| "could not append action field select")?;
        surface
            .append_child(&fieldset)
            .map_err(|_| "could not append action field")?;
    }
    if let Some(error) = &draft.error {
        let error_node = document
            .create_element("p")
            .map_err(|_| "could not create action draft error")?;
        error_node
            .set_attribute("role", "alert")
            .map_err(|_| "could not identify action draft error")?;
        error_node.set_text_content(Some(error));
        surface
            .append_child(&error_node)
            .map_err(|_| "could not append action draft error")?;
    }
    let submit = document
        .create_element("button")
        .map_err(|_| "could not create action draft submit")?;
    submit
        .set_attribute("type", "button")
        .map_err(|_| "could not set action draft submit type")?;
    submit
        .set_attribute("data-action-draft-submit", "")
        .map_err(|_| "could not identify action draft submit")?;
    submit.set_text_content(Some(&draft.submit_label));
    surface
        .append_child(&submit)
        .map_err(|_| "could not append action draft submit")?;

    body.set_attribute("data-action-draft-open", "true")
        .map_err(|_| "could not expose open action draft")?;
    body.set_attribute("data-action-draft-fields", &draft.fields.len().to_string())
        .map_err(|_| "could not expose action draft fields")?;
    if let Some(error) = &draft.error {
        body.set_attribute("data-action-draft-error", error)
            .map_err(|_| "could not expose action draft error")?;
    } else {
        body.remove_attribute("data-action-draft-error")
            .map_err(|_| "could not clear action draft error")?;
    }
    Ok(())
}

fn projection_panel_key(panel: ProjectionPanel) -> &'static str {
    match panel {
        ProjectionPanel::Source => "source",
        ProjectionPanel::Reading => "reading",
        ProjectionPanel::Encoding => "encoding",
        ProjectionPanel::Arrangement => "arrangement",
        ProjectionPanel::Interaction => "interaction",
        ProjectionPanel::Preview => "preview",
        ProjectionPanel::Provenance => "provenance",
    }
}

fn set_projection_input_value(id: &str, value: &str) -> Result<(), String> {
    let node = element(id)?;
    if let Ok(input) = node.clone().dyn_into::<HtmlInputElement>() {
        input.set_value(value);
        return Ok(());
    }
    if let Ok(textarea) = node.dyn_into::<HtmlTextAreaElement>() {
        textarea.set_value(value);
        return Ok(());
    }
    Err(format!("projection editor field #{id} is not an input"))
}

fn projection_preview(draft: &ProjectionDraft) -> String {
    let x = match &draft.encoding.x {
        Channel::Field(value) => value.as_str(),
        Channel::Constant(value) => value.as_str(),
    };
    let y = match &draft.encoding.y {
        Channel::Field(value) => value.as_str(),
        Channel::Constant(value) => value.as_str(),
    };
    format!(
        "{} · read {} by {} · {} · x={} y={} · {}",
        draft.appearance.title,
        draft.reading.kind,
        draft.reading.key,
        draft.arrangement.kind,
        x,
        y,
        draft.appearance.realization
    )
}

fn update_projection_editor_semantics(host: &BrowserHost) -> Result<(), String> {
    let surface = element("projection-editor")?;
    if host.projection_editor_open {
        surface
            .remove_attribute("hidden")
            .map_err(|_| "could not show projection editor")?;
        surface
            .set_attribute("aria-hidden", "false")
            .map_err(|_| "could not expose projection editor")?;
    } else {
        surface
            .set_attribute("hidden", "")
            .map_err(|_| "could not hide projection editor")?;
        surface
            .set_attribute("aria-hidden", "true")
            .map_err(|_| "could not hide projection editor semantics")?;
    }
    let draft = host.projection_editor.draft();
    let validation = host.projection_editor.validate();
    let (validation_token, error_count) = match &validation {
        Ok(()) => ("valid", 0),
        Err(issues) => ("invalid", issues.len()),
    };
    let panel = projection_panel_key(host.projection_editor.panel());
    let content_id = host.projection_editor.panel().content_id();
    let preview = projection_preview(draft);
    set_projection_input_value("projection-source-authority",
        &draft.source.authority,
    )?;
    set_projection_input_value("projection-source-domain", &draft.source.domain)?;
    set_projection_input_value("projection-source-resource",
        &draft.source.resource,
    )?;
    set_projection_input_value("projection-reading-key", &draft.reading.key)?;
    set_projection_input_value("projection-encoding-x",
        match &draft.encoding.x {
            Channel::Field(value) | Channel::Constant(value) => value,
        },
    )?;
    set_projection_input_value("projection-encoding-y",
        match &draft.encoding.y {
            Channel::Field(value) | Channel::Constant(value) => value,
        },
    )?;
    set_projection_input_value("projection-arrangement-kind",
        &draft.arrangement.kind,
    )?;
    set_projection_input_value("projection-arrangement-direction",
        &draft.arrangement.direction,
    )?;
    set_projection_input_value("projection-arrangement-spacing",
        &draft.arrangement.spacing.to_string(),
    )?;
    set_projection_input_value("projection-appearance-realization",
        &draft.appearance.realization,
    )?;
    set_projection_input_value("projection-appearance-title",
        &draft.appearance.title,
    )?;
    set_projection_input_value("projection-provenance-author",
        &draft.provenance.author,
    )?;
    set_projection_input_value("projection-provenance-revision",
        &draft.provenance.source_revision,
    )?;
    set_projection_input_value("projection-provenance-note",
        &draft.provenance.note,
    )?;
    set_text(
        "projection-editor-status",
        &host.projection_editor_status,
    );
    set_text(
        "projection-editor-source",
        &format!(
            "{} / {} / {}",
            draft.source.authority, draft.source.domain, draft.source.resource
        ),
    );
    set_text(
        "projection-editor-provenance",
        &format!(
            "{} · source {} · {}",
            draft.provenance.author, draft.provenance.source_revision, draft.provenance.note
        ),
    );
    set_text(
        "projection-editor-validation",
        &match &validation {
            Ok(()) => "Valid draft".to_string(),
            Err(_) => format!("{error_count} validation issue(s)"),
        },
    );
    set_text(
        "projection-editor-lane",
        &format!("ContentSource::Open · graphshell.projection-editor.panel · {content_id}"),
    );
    set_text(
        "projection-editor-preview", &preview);
    set_attr(
        &element("projection-editor-preview")?,
        "data-preview-value",
        &preview,
    )?;
    for candidate in ProjectionPanel::ALL {
        let button = element(
            &format!("projection-panel-{}", projection_panel_key(candidate)),
        )?;
        button
            .set_attribute(
                "aria-selected",
                (candidate == host.projection_editor.panel())
                    .then_some("true")
                    .unwrap_or("false"),
            )
            .map_err(|_| "could not expose selected projection panel")?;
        let group = element(
            &format!("projection-fields-{}", projection_panel_key(candidate)),
        )?;
        if candidate == host.projection_editor.panel() {
            group
                .remove_attribute("hidden")
                .map_err(|_| "could not show projection editor panel")?;
        } else {
            group
                .set_attribute("hidden", "")
                .map_err(|_| "could not hide projection editor panel")?;
        }
    }
    let body = root()?;
    body.set_attribute(
        "data-projection-editor-open",
        &host.projection_editor_open.to_string(),
    )
    .map_err(|_| "could not expose projection editor state")?;
    body.set_attribute("data-projection-editor-panel", panel)
        .map_err(|_| "could not expose projection editor panel")?;
    body.set_attribute(
        "data-projection-editor-content",
        &format!("open:graphshell.projection-editor.panel:{content_id}"),
    )
    .map_err(|_| "could not expose projection editor content lane")?;
    body.set_attribute("data-projection-editor-preview", &preview)
        .map_err(|_| "could not expose projection editor preview")?;
    body.set_attribute("data-projection-editor-validation", validation_token)
        .map_err(|_| "could not expose projection editor validation")?;
    body.set_attribute("data-projection-editor-errors", &error_count.to_string())
        .map_err(|_| "could not expose projection editor errors")?;
    body.set_attribute(
        "data-projection-editor-save-count",
        &host.projection_editor_save_count.to_string(),
    )
    .map_err(|_| "could not expose projection editor save count")?;
    Ok(())
}

fn global_json(window: &Window, name: &str) -> Result<Option<String>, String> {
    let value = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str(name))
        .map_err(|_| format!("could not read browser global {name}"))?;
    Ok(value.as_string())
}

fn initial_capture_input(
    window: &Window,
) -> Result<Option<(HistoryCapturePolicy, Vec<BrowserVisit>)>, String> {
    let Some(policy_json) = global_json(window, CAPTURE_POLICY_GLOBAL)? else {
        return Ok(None);
    };
    let policy = serde_json::from_str(&policy_json)
        .map_err(|error| format!("invalid browser capture policy: {error}"))?;
    let visits_json =
        global_json(window, CAPTURE_VISITS_GLOBAL)?.unwrap_or_else(|| "[]".to_string());
    let visits = serde_json::from_str(&visits_json)
        .map_err(|error| format!("invalid browser visit batch: {error}"))?;
    Ok(Some((policy, visits)))
}

async fn apply_initial_capture(
    app: &mut GraphshellApp<IndexedDbBackend>,
    store: &mut IndexedDbBackend,
    input: Option<(HistoryCapturePolicy, Vec<BrowserVisit>)>,
    persona: &str,
    now_secs: u64,
) -> Result<InitialCaptureSummary, String> {
    let Some((policy, visits)) = input else {
        return Ok(InitialCaptureSummary::default());
    };
    let mut capture = BrowserHistoryCapture::load(store, policy)
        .await
        .map_err(|error| error.to_string())?;
    if visits.is_empty() {
        return Ok(InitialCaptureSummary {
            active: capture.policy().enabled,
            ..InitialCaptureSummary::default()
        });
    }
    let outcomes = capture
        .ingest_batch(
            &mut app.host,
            store,
            visits,
            persona,
            FIXTURE_DEVICE_TWO_ADDRESS,
            now_secs,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(InitialCaptureSummary {
        active: capture.policy().enabled,
        accepted: outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CaptureOutcome::Accepted { .. }))
            .count(),
        dropped: outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CaptureOutcome::Dropped(_)))
            .count(),
    })
}

fn history_control_input(
    window: &Window,
) -> Result<Option<(AccessRecordFilter, Option<HistoryForgetInput>)>, String> {
    let Some(filter_json) = global_json(window, HISTORY_FILTER_GLOBAL)? else {
        return Ok(None);
    };
    let filter: HistoryFilterInput = serde_json::from_str(&filter_json)
        .map_err(|error| format!("invalid history authority filter: {error}"))?;
    let forget = global_json(window, HISTORY_FORGET_GLOBAL)?
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|error| format!("invalid history forget request: {error}"))
        })
        .transpose()?;
    Ok(Some((filter.into(), forget)))
}

async fn apply_history_controls(
    app: &mut GraphshellApp<IndexedDbBackend>,
    store: &mut IndexedDbBackend,
    policy: HistoryCapturePolicy,
    now_secs: u64,
) -> HistoryControlSummary {
    let browser_window = match window() {
        Ok(window) => window,
        Err(error) => {
            return HistoryControlSummary {
                active: true,
                error: Some(error),
                ..HistoryControlSummary::default()
            };
        }
    };
    let input = match history_control_input(&browser_window) {
        Ok(input) => input,
        Err(error) => {
            return HistoryControlSummary {
                active: true,
                error: Some(error),
                ..HistoryControlSummary::default()
            };
        }
    };
    let Some((filter, forget)) = input else {
        return HistoryControlSummary::default();
    };
    let mut summary = HistoryControlSummary {
        active: true,
        forget_attempted: forget.is_some(),
        ..HistoryControlSummary::default()
    };
    if let Some(forget) = forget {
        let mode = if forget.remove_object {
            ForgetMode::RemoveCapturedObject
        } else {
            ForgetMode::HistoryOnly
        };
        let result = async {
            let mut capture = BrowserHistoryCapture::load(store, policy).await?;
            capture
                .forget_url(&mut app.host, store, &forget.url, mode, now_secs)
                .await
        }
        .await;
        match result {
            Ok(forgotten) => summary.forgotten = forgotten,
            Err(error) => summary.error = Some(error.to_string()),
        }
    }
    if summary.error.is_none() {
        match query_access_records(store, &filter).await {
            Ok(records) => {
                summary.records = records
                    .into_iter()
                    .filter(|record| record.handler.starts_with(BROWSER_HISTORY_HANDLER_PREFIX))
                    .collect();
            }
            Err(error) => summary.error = Some(error.to_string()),
        }
    }
    summary
}

fn publish_capture_receipt(
    document: &Document,
    summary: InitialCaptureSummary,
) -> Result<(), String> {
    if !summary.active {
        return Ok(());
    }
    let body = root()?;
    body.set_attribute("data-capture-accepted", &summary.accepted.to_string())
        .map_err(|_| "could not expose accepted capture count")?;
    body.set_attribute("data-capture-dropped", &summary.dropped.to_string())
        .map_err(|_| "could not expose dropped capture count")?;
    document
        .dispatch_event(
            &web_sys::Event::new("graphshell-capture-complete")
                .map_err(|_| "could not create capture completion event")?,
        )
        .map_err(|_| "could not dispatch capture completion event")?;
    Ok(())
}

fn publish_history_controls(
    document: &Document,
    summary: &HistoryControlSummary,
) -> Result<(), String> {
    if !summary.active {
        return Ok(());
    }
    let body = root()?;
    body.set_attribute(
        "data-history-result-count",
        &summary.records.len().to_string(),
    )
    .map_err(|_| "could not expose history result count")?;
    body.set_attribute(
        "data-history-forget-attempted",
        &summary.forget_attempted.to_string(),
    )
    .map_err(|_| "could not expose history forget state")?;
    body.set_attribute("data-history-forgotten", &summary.forgotten.to_string())
        .map_err(|_| "could not expose forgotten history count")?;
    if let Some(error) = &summary.error {
        body.set_attribute("data-history-error", error)
            .map_err(|_| "could not expose history action error")?;
    } else {
        body.remove_attribute("data-history-error")
            .map_err(|_| "could not clear history action error")?;
    }

    let results = element("history-results")?;
    results.set_text_content(None);
    for record in summary.records.iter().rev().take(100) {
        let item = document
            .create_element("li")
            .map_err(|_| "could not create history result")?;
        item.set_text_content(Some(&format!(
            "{} · {} · {} · {}",
            record.address, record.persona, record.device, record.at_ms
        )));
        results
            .append_child(&item)
            .map_err(|_| "could not append history result")?;
    }
    document
        .dispatch_event(
            &web_sys::Event::new("graphshell-history-controls-complete")
                .map_err(|_| "could not create history completion event")?,
        )
        .map_err(|_| "could not dispatch history completion event")?;
    Ok(())
}

fn update_semantics(host: &mut BrowserHost) -> Result<(), String> {
    let document = document()?;
    let model = host.chrome_model();
    set_text("active-session", &model.active_session);
    set_text("selection-status", &model.selection);
    set_text("detail-title", &model.selection);
    set_text("detail-address", &model.detail_address);
    set_text("action-status", &host.action_status);
    if !host.action_draft_semantics_ready || model.action_draft != host.rendered_action_draft {
        update_action_draft_semantics(&document, model.action_draft.as_ref())?;
        host.rendered_action_draft = model.action_draft.clone();
        host.action_draft_semantics_ready = true;
    }
    update_projection_editor_semantics(host)?;
    set_text(
        "capture-attribution",
        &format!(
            "Reference-host attribution · {} · {}",
            host.app.host.selected_persona().persona,
            FIXTURE_DEVICE_TWO_ADDRESS
        ),
    );
    set_text(
        "viewport-status",
        &format!("{} by {}", host.width, host.height),
    );
    update_product_semantics(host, &model)?;
    web_remote::update_remote_semantics(host, &document)?;
    set_attr(
        &element("detail-surface")?,
        "aria-hidden",
        if host.detail_open { "false" } else { "true" },
    )?;
    set_attr(
        &element("session-local")?,
        "aria-pressed",
        if host.active == ActiveSession::Local {
            "true"
        } else {
            "false"
        },
    )?;
    set_attr(
        &element("session-remote")?,
        "aria-pressed",
        if host.active == ActiveSession::Remote {
            "true"
        } else {
            "false"
        },
    )?;
    host.canvas_element
        .set_attribute(
            "data-camera",
            &format!(
                "{:.2},{:.2},{:.3}",
                host.canvas.camera().offset.0,
                host.canvas.camera().offset.1,
                host.canvas.camera().zoom
            ),
        )
        .map_err(|_| "could not expose camera state")?;
    if let Some((x, y)) = host.canvas.focused_node_screen() {
        host.canvas_element
            .set_attribute("data-focused-node", &format!("{x:.1},{y:.1}"))
            .map_err(|_| "could not expose focused node")?;
    } else {
        host.canvas_element
            .remove_attribute("data-focused-node")
            .map_err(|_| "could not clear focused node")?;
    }
    let body = root()?;
    body.set_attribute("data-ready", "true")
        .map_err(|_| "could not expose ready state")?;
    body.set_attribute(
        "data-session",
        if host.active == ActiveSession::Local {
            "local"
        } else {
            "remote"
        },
    )
    .map_err(|_| "could not expose active session")?;
    body.set_attribute("data-detail-open", &host.detail_open.to_string())
        .map_err(|_| "could not expose detail state")?;
    body.set_attribute("data-action-count", &host.action_count.to_string())
        .map_err(|_| "could not expose action count")?;
    body.set_attribute("data-storage", &host.storage_status)
        .map_err(|_| "could not expose storage state")?;
    // A stable token beside the sentence, so a scenario checks a state rather
    // than parsing prose that is allowed to change.
    body.set_attribute("data-storage-persistence", host.storage_persistence.token())
        .map_err(|_| "could not expose storage persistence")?;
    body.set_attribute("data-capture-count", &host.capture_count.to_string())
        .map_err(|_| "could not expose the capture count")?;
    if owns_title() {
        document.set_title("GRAPHSHELL H3 READY");
    }
    Ok(())
}

/// The component's markup, shipped in the bundle.
const COMPONENT_MARKUP: &str = include_str!("../web/component.html");

/// Whether this root owns the page title (the full-page surface does; an
/// embed must not retitle its host).
fn owns_title() -> bool {
    root().is_ok_and(|root| root.has_attribute("data-owns-title"))
}

async fn run(root_element: Element) -> Result<(), String> {
    root_element.set_inner_html(COMPONENT_MARKUP);
    ROOT.with(|slot| *slot.borrow_mut() = Some(root_element));
    let document = document()?;
    if owns_title() {
        document.set_title("Graphshell H3 · booting");
    }
    let canvas: HtmlCanvasElement = element("graphshell-canvas")?
        .dyn_into()
        .map_err(|_| "#graphshell-canvas is not a canvas")?;
    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    let backend = IndexedDbBackend::open("graphshell-reference-host-h5", "muniment")
        .await
        .map_err(|error| error.to_string())?;
    let mut capture_store = backend.clone();
    let selected_persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.to_string(),
        profile: "profile:graphshell-h3".to_string(),
    };
    let capture_persona = selected_persona.persona.clone();
    let mut app = GraphshellApp::open_or_fixture(backend, selected_persona)
        .await
        .map_err(|error| error.to_string())?;
    let store_state = if app.host.was_reopened() {
        "IndexedDB reopened"
    } else {
        "IndexedDB seeded"
    };
    // Asked once, at open. A browser decides this on heuristics that change
    // with how established the profile looks, so the answer is recorded rather
    // than assumed, and a refusal is reported rather than hidden.
    let storage_persistence = resolve_storage_persistence().await;
    let storage_status = status_line(store_state, &storage_persistence);
    let now_secs = (js_sys::Date::now() / 1_000.0) as u64;
    let capture_input = initial_capture_input(&window()?)?;
    let capture_policy = capture_input
        .as_ref()
        .map(|(policy, _)| policy.clone())
        .unwrap_or_else(HistoryCapturePolicy::disabled);
    let capture_summary = apply_initial_capture(
        &mut app,
        &mut capture_store,
        capture_input,
        &capture_persona,
        now_secs,
    )
    .await?;
    if capture_summary.accepted == 0 {
        app.host
            .persist(now_secs)
            .await
            .map_err(|error| error.to_string())?;
    }
    let history_summary =
        apply_history_controls(&mut app, &mut capture_store, capture_policy, now_secs).await;
    app.mount_local().map_err(|error| error.to_string())?;
    let mut remote = FixtureEndpoint::new();
    let remote_snapshot = remote
        .snapshot(remote.request())
        .map_err(|error| error.to_string())?;
    let remote_session = app
        .mount_remote(remote_snapshot)
        .map_err(|error| error.to_string())?;

    let mut graph_canvas = Canvas::with_graph(app.host.graph().clone());
    graph_canvas.resize(width, height);
    graph_canvas.set_layout_strategy(Some("phyllotaxis.default".to_string()));
    let positions = project_canvas_strategy(
        "phyllotaxis.default",
        graph_canvas.graph(),
        None,
        width,
        height,
        None,
        None,
        true,
    );
    graph_canvas.apply_strategy_positions(&positions);
    graph_canvas.fit_to_content();
    graph_canvas.select_by_url(FIXTURE_WEB_ADDRESS);
    let primary_member = graph_canvas.focused_member();
    let initial_face = graph_canvas
        .graph()
        .get_node_by_url(FIXTURE_WEB_ADDRESS)
        .map(|(key, _)| graph_canvas.node_face(key).as_code().to_string())
        .unwrap_or_else(|| "derived".to_string());
    let node_count = app.host.graph().node_count();
    let product_status = if capture_summary.active {
        format!(
            "Daily graph operations ready · {storage_status} · {} captured",
            capture_summary.accepted
        )
    } else {
        format!("Daily graph operations ready · {storage_status}")
    };

    let gpu = GpuPresenter::boot(canvas.clone(), width, height).await?;
    let initial_model = ChromeModel {
        active_session: format!("Local Mere · {node_count} objects"),
        local_active: true,
        selection: FIXTURE_WEB_ADDRESS.to_string(),
        detail_open: false,
        detail_address: FIXTURE_WEB_ADDRESS.to_string(),
        action_status: "Ready".to_string(),
        viewport_label: format!("{width} × {height}"),
        product_status: product_status.clone(),
        // Nothing is mounted at construction, so there is nothing to report.
        satisfaction: String::new(),
        arrangement: "phyllotaxis.default".to_string(),
        physics_law: mere::canvas::PhysicsLaw::Springs.label().to_string(),
        physics_paused: false,
        action_draft: None,
    };
    // The chrome's font. A browser has no system fonts for fontique to find,
    // so the page ships one (Roboto Regular, as `GraphshellSans.ttf`) and
    // registers it once on a text system the host keeps; the chrome sheet
    // names the family. `genet_layout::register_host_font` did this until
    // the Livery migration retired that crate, and the glyphs went with it.
    let mut chrome_text = TextSystem::new();
    chrome_text.register_font_bytes(include_bytes!("../web/GraphshellSans.ttf").to_vec());
    let chrome_scene = build_chrome_scene(initial_model, width, height, &mut chrome_text)?;
    let state = Rc::new(RefCell::new(BrowserHost {
        app,
        remote: RemoteLink::Fixture(remote),
        remote_board: PhysicsBoard::new(),
        remote_board_revision: None,
        layout_stats_stale: true,
        layout_moved: false,
        layout_stats: mere::canvas::LayoutStats::default(),
        remote_session: Some(remote_session),
        remote_status: "fixture".to_string(),
        remote_joining: false,
        remote_last_resume: String::new(),
        active: ActiveSession::Local,
        canvas: graph_canvas,
        canvas_element: canvas,
        gpu,
        chrome_scene,
        chrome_text,
        chrome_dirty: false,
        detail_open: false,
        action_count: 0,
        action_status: "Ready".to_string(),
        action_draft: None,
        action_draft_target: None,
        rendered_action_draft: None,
        action_draft_semantics_ready: false,
        width,
        height,
        product_status,
        storage_status,
        storage_persistence,
        layout_id: "phyllotaxis.default".to_string(),
        physics_paused: false,
        physics_damping: 0.82,
        handler_id: "graphshell.inspect".to_string(),
        relation_family: RelationFamilyFilter::All,
        filter_count: node_count,
        face: initial_face,
        last_export: String::new(),
        export_bytes: 0,
        imported_nodes: 0,
        saved_scene: None,
        arrangement_transition: None,
        primary_member,
        last_detail_member: None,
        projection_editor: ProjectionEditor::new(initial_projection_draft()),
        projection_editor_open: false,
        projection_editor_status: "Draft ready · unsaved".to_string(),
        projection_editor_save_count: 0,
        scenario: None,
        scenario_frames: 0,
        probe_events: Vec::new(),
        deferred_dom: Vec::new(),
        capture_request: None,
        capture_pending: None,
        capture_count: 0,
    }));
    web_scenario::install(&state);
    install_events(&state)?;
    web_product::install_product_events(&state)?;
    update_semantics(&mut state.borrow_mut())?;
    publish_capture_receipt(&document, capture_summary)?;
    publish_history_controls(&document, &history_summary)?;
    schedule_frames(state)?;
    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Mount the component into `root`: the full page's body-filling element or
/// a box in someone else's page. One component, whichever surface.
#[wasm_bindgen]
pub fn mount(root: Element) -> Result<(), JsValue> {
    if ROOT.with(|slot| slot.borrow().is_some()) {
        return Err(JsValue::from_str("Graphshell is already mounted on this page"));
    }
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = run(root).await {
            web_sys::console::error_1(&error.clone().into());
            if let Ok(document) = document() {
                document.set_title(&format!("GRAPHSHELL H3 FAIL: {error}"));
            }
        }
    });
    Ok(())
}
