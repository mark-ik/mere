//! Headed browser presenter for the Graphshell reference host.

mod web_events;
mod web_gpu;
mod web_product;
mod web_view;

use std::cell::RefCell;
use std::rc::Rc;

use graphshell::endpoint::{IntentSink, ProjectionSource};
use graphshell::protocol::{
    CapabilityProfile, IntentInvocation, IntentResult, PresentationCapability, ProjectionSession,
};
use mere::canvas::{Canvas, PointerButton, project_canvas_strategy};
use netrender::Scene;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlCanvasElement, Window};

use graphshell::app::GraphshellApp;
use graphshell::canary::FixtureEndpoint;
use graphshell::capture::{
    BrowserHistoryCapture, BrowserVisit, CaptureOutcome, HistoryCapturePolicy,
};
use graphshell::indexeddb_backend::IndexedDbBackend;
use graphshell::mere_host::{
    FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef,
};
use graphshell::product::{RelationFamilyFilter, SavedSceneV1};
use uuid::Uuid;
use web_events::{install_events, schedule_frames};
use web_gpu::GpuPresenter;
use web_product::update_product_semantics;
use web_view::{ChromeModel, build_chrome_scene};

const REMOTE_LABEL: &str = "Remote projection · 2 objects";
const CAPTURE_POLICY_GLOBAL: &str = "graphshellCapturePolicyJson";
const CAPTURE_VISITS_GLOBAL: &str = "graphshellInitialVisitsJson";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InitialCaptureSummary {
    active: bool,
    accepted: usize,
    dropped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveSession {
    Local,
    Remote,
}

struct BrowserHost {
    app: GraphshellApp<IndexedDbBackend>,
    remote: FixtureEndpoint,
    remote_session: ProjectionSession,
    active: ActiveSession,
    canvas: Canvas,
    canvas_element: HtmlCanvasElement,
    gpu: GpuPresenter,
    chrome_scene: Scene,
    chrome_dirty: bool,
    detail_open: bool,
    action_count: u32,
    action_status: String,
    width: u32,
    height: u32,
    product_status: String,
    storage_status: String,
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
    primary_member: Option<Uuid>,
    last_detail_member: Option<Uuid>,
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
            ActiveSession::Remote => (
                "Projection boundary card".to_string(),
                "fixture.graphshell/note:recent".to_string(),
            ),
        };
        let (product_status, arrangement, physics_paused) = self.product_chrome();
        ChromeModel {
            active_session: match self.active {
                ActiveSession::Local => format!(
                    "Local Mere · {} objects",
                    self.app.host.graph().node_count()
                ),
                ActiveSession::Remote => REMOTE_LABEL.to_string(),
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
            arrangement,
            physics_paused,
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

    fn render(&mut self) -> Result<(), String> {
        self.resize_if_needed();
        if self.chrome_dirty {
            self.chrome_scene = build_chrome_scene(self.chrome_model(), self.width, self.height)?;
            self.chrome_dirty = false;
        }
        let content = match self.active {
            ActiveSession::Local => self.canvas.frame(self.width, self.height).0,
            ActiveSession::Remote => self.remote_scene(),
        };
        self.gpu
            .present(&content, &self.chrome_scene, self.width, self.height)
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
        let Some(mounted) = self.app.client.mounted(&self.remote_session) else {
            return scene;
        };
        let bounds = mounted.scene.tables.bounds;
        let scale = ((self.width as f32 - 100.0) / bounds.size.w.max(1.0))
            .min((self.height as f32 - 180.0) / bounds.size.h.max(1.0))
            .min(1.0);
        let origin_x = (self.width as f32 - bounds.size.w * scale) * 0.5 - bounds.origin.x * scale;
        let origin_y = 116.0 - bounds.origin.y * scale;
        for (instance, item) in mounted.scene.active_items_in_order() {
            let center_x = origin_x + item.transform.translate.x * scale;
            let center_y = origin_y + item.transform.translate.y * scale;
            let (card_w, card_h) = match item.footprint {
                sceno::Footprint::Rect { size } => (size.w * scale, size.h * scale),
                _ => (120.0, 80.0),
            };
            let color = if instance.0 == 0 {
                [0.16, 0.31, 0.35, 1.0]
            } else {
                [0.23, 0.28, 0.38, 1.0]
            };
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

    fn run_command(&mut self, command: &str) {
        match command {
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
            "zoom-in" => self.zoom(40.0),
            "zoom-out" => self.zoom(-40.0),
            "pan-left" => self.pan(-42.0, 0.0),
            "pan-right" => self.pan(42.0, 0.0),
            "pan-up" => self.pan(0.0, -42.0),
            "pan-down" => self.pan(0.0, 42.0),
            _ => {
                if !self.run_product_command(command) {
                    return;
                }
            }
        }
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
        self.handler_id =
            web_product::selected_handler().unwrap_or_else(|_| self.handler_id.clone());
        let address = self
            .current_primary_member()
            .and_then(|id| self.app.host.graph().get_node_by_id(id))
            .map(|(_, node)| node.url().to_string())
            .unwrap_or_else(|| FIXTURE_WEB_ADDRESS.to_string());
        let result = match self.active {
            ActiveSession::Local => self
                .app
                .open_address(&address, &self.handler_id)
                .map_err(|error| error.to_string()),
            ActiveSession::Remote => self.invoke_remote_action(),
        };
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

    fn invoke_remote_action(&mut self) -> Result<IntentResult, String> {
        let mounted = self
            .app
            .client
            .mounted(&self.remote_session)
            .ok_or("remote projection is not mounted")?;
        let tree = self
            .app
            .client
            .accessibility_tree(
                &self.remote_session,
                &CapabilityProfile::new([
                    PresentationCapability::PortableCard,
                    PresentationCapability::Image,
                ]),
            )
            .map_err(|error| format!("{error:?}"))?;
        let item = tree.children.first().ok_or("remote tree is empty")?;
        let action = item.actions.first().ok_or("remote item has no action")?;
        self.remote
            .invoke(IntentInvocation {
                session: self.remote_session.clone(),
                target: item.instance,
                observed_epoch: mounted.scene.epoch,
                observed_revision: mounted.scene.revision,
                intent: action.intent.0.clone(),
                payload: Vec::new(),
            })
            .map_err(|error| error.to_string())
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

fn window() -> Result<Window, String> {
    web_sys::window().ok_or_else(|| "browser window is unavailable".to_string())
}

fn document() -> Result<Document, String> {
    window()?
        .document()
        .ok_or_else(|| "browser document is unavailable".to_string())
}

fn element(document: &Document, id: &str) -> Result<Element, String> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| format!("missing #{id}"))
}

fn set_text(document: &Document, id: &str, value: &str) {
    if let Some(element) = document.get_element_by_id(id) {
        element.set_text_content(Some(value));
    }
}

fn set_attr(element: &Element, name: &str, value: &str) -> Result<(), String> {
    element
        .set_attribute(name, value)
        .map_err(|_| format!("could not set {name} on #{}", element.id()))
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
    persona: &str,
    now_secs: u64,
) -> Result<InitialCaptureSummary, String> {
    let Some((policy, visits)) = initial_capture_input(&window()?)? else {
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

fn publish_capture_receipt(
    document: &Document,
    summary: InitialCaptureSummary,
) -> Result<(), String> {
    if !summary.active {
        return Ok(());
    }
    let body = document.body().ok_or("document has no body")?;
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

fn update_semantics(host: &mut BrowserHost) -> Result<(), String> {
    let document = document()?;
    let model = host.chrome_model();
    set_text(&document, "active-session", &model.active_session);
    set_text(&document, "selection-status", &model.selection);
    set_text(&document, "detail-title", &model.selection);
    set_text(&document, "detail-address", &model.detail_address);
    set_text(&document, "action-status", &host.action_status);
    set_text(
        &document,
        "viewport-status",
        &format!("{} by {}", host.width, host.height),
    );
    update_product_semantics(host, &model)?;
    set_attr(
        &element(&document, "detail-surface")?,
        "aria-hidden",
        if host.detail_open { "false" } else { "true" },
    )?;
    set_attr(
        &element(&document, "session-local")?,
        "aria-pressed",
        if host.active == ActiveSession::Local {
            "true"
        } else {
            "false"
        },
    )?;
    set_attr(
        &element(&document, "session-remote")?,
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
    let body = document.body().ok_or("document has no body")?;
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
    document.set_title("GRAPHSHELL H3 READY");
    Ok(())
}

async fn run() -> Result<(), String> {
    let document = document()?;
    document.set_title("Graphshell H3 · booting");
    genet_layout::register_host_font(include_bytes!("../web/GraphshellSans.ttf").to_vec());
    let canvas: HtmlCanvasElement = element(&document, "graphshell-canvas")?
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
    let storage_status = if app.host.was_reopened() {
        "IndexedDB reopened"
    } else {
        "IndexedDB seeded"
    }
    .to_string();
    let now_secs = (js_sys::Date::now() / 1_000.0) as u64;
    let capture_summary =
        apply_initial_capture(&mut app, &mut capture_store, &capture_persona, now_secs).await?;
    if capture_summary.accepted == 0 {
        app.host
            .persist(now_secs)
            .await
            .map_err(|error| error.to_string())?;
    }
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
        arrangement: "phyllotaxis.default".to_string(),
        physics_paused: false,
    };
    let chrome_scene = build_chrome_scene(initial_model, width, height)?;
    let state = Rc::new(RefCell::new(BrowserHost {
        app,
        remote,
        remote_session,
        active: ActiveSession::Local,
        canvas: graph_canvas,
        canvas_element: canvas,
        gpu,
        chrome_scene,
        chrome_dirty: false,
        detail_open: false,
        action_count: 0,
        action_status: "Ready".to_string(),
        width,
        height,
        product_status,
        storage_status,
        layout_id: "phyllotaxis.default".to_string(),
        physics_paused: false,
        physics_damping: 0.82,
        handler_id: "graphshell.inspect".to_string(),
        relation_family: RelationFamilyFilter::All,
        filter_count: node_count,
        face: "favicon".to_string(),
        last_export: String::new(),
        export_bytes: 0,
        imported_nodes: 0,
        saved_scene: None,
        primary_member,
        last_detail_member: None,
    }));
    install_events(&state)?;
    web_product::install_product_events(&state)?;
    update_semantics(&mut state.borrow_mut())?;
    publish_capture_receipt(&document, capture_summary)?;
    schedule_frames(state)?;
    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = run().await {
            web_sys::console::error_1(&error.clone().into());
            if let Ok(document) = document() {
                document.set_title(&format!("GRAPHSHELL H3 FAIL: {error}"));
            }
        }
    });
}
