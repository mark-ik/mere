/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat-shell: the on-screen serval host for Mere's chrome.
//!
//! A winit window that runs the reused chrome ([`meerkat::chrome_view`] over a
//! [`meerkat::Chrome`] wrapping the graphshell `ToolbarState`) through serval and
//! presents via netrender — pelt-live-shaped. It reuses pelt-live's lib for the
//! cascade → layout → paint → `Scene` builder ([`scene_from_scripted_dom`]) and
//! the point→node hit-test ([`hit_test_node`]), so this file is the window +
//! present + input-dispatch harness, not a second engine.
//!
//! ## Two roots, one window
//!
//! The window composites **two authorities**: the chrome root (the reused toolbar
//! / omnibar, diffed by the runner) in a top band, and the content root — an
//! [`Orrery`], the graph's spatial presentation — filling the rest. The chrome
//! runs through serval into a `Scene`; the orrery produces its own composited
//! `Scene` from the graph + physics. Each is rasterized and composited at its
//! band, neither root seeing the other's tree. Input routes by region: the chrome
//! band hit-tests the chrome root, the content band drives the orrery (pan / zoom
//! / drag / select), and keyboard modifiers feed both.
//!
//! The orrery is the graph-rooted content surface (modular integration plan, S1).
//! Next (S2): navigating a location adds a node and projects its media as a tile,
//! so the omnibar drives the graph rather than a synthesized page.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use eidetic_fjall::FjallStore;
use inker::EngineRegistry;
use layout_dom_api::LayoutDom;
use meerkat::{chrome_view, submit_omnibar, Chrome, ChromeLogic, ChromeView};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use orrery_host::{CameraView, Orrery, PointerButton, WHEEL_PAN_SCALE};
use pelt_live::{fragments_from_scripted_dom, hit_test_node, scene_from_scripted_dom, TextCursor};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::{key_event_from_winit, modifiers_from_winit, SurfaceHost};
use session_runtime::{
    content_store, session_graph_store, view_intent_store, CameraSnapshot, ViewIntent,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};
use xilem_serval::{Modifiers, PointerClick, ServalAppRunner};

mod card;
mod fetch;
mod resources;
mod sync;

/// Author CSS for the **chrome** root. The toolbar is a flex row (back / forward
/// buttons + a growing omnibar); serval lays it out via taffy's flexbox. The
/// `.chrome` container has no background — the host composites it over the
/// content root, so only the toolbar and the (opaque) suggestions dropdown paint
/// over the page; everything else stays transparent.
const CHROME_SHEET: &[&str] = &[
    "div, button, input { display: block; }",
    ".toolbar { display: flex; background-color: rgb(236, 238, 243); padding: 8px; }",
    "button { font-size: 22px; color: rgb(30, 30, 40); \
        background-color: rgb(220, 224, 232); padding: 8px 14px; margin: 4px; }",
    ".disabled { color: rgb(170, 174, 184); background-color: rgb(228, 230, 236); }",
    "input { font-size: 22px; color: rgb(20, 20, 20); \
        background-color: rgb(255, 255, 255); padding: 8px; margin: 4px; flex-grow: 1; }",
    // The p2p sync chip: small + muted, no flex-grow, so the omnibar pushes it to
    // the toolbar's right edge.
    ".sync-chip { font-size: 14px; color: rgb(96, 102, 116); \
        background-color: rgb(226, 230, 238); padding: 8px 12px; margin: 4px; }",
    ".suggestions { background-color: rgb(255, 255, 255); padding-bottom: 6px; }",
    ".suggestion { font-size: 18px; color: rgb(40, 44, 54); \
        background-color: rgb(255, 255, 255); padding: 8px 16px; }",
    ".suggestion-active { font-size: 18px; color: rgb(20, 24, 34); \
        background-color: rgb(216, 226, 244); padding: 8px 16px; }",
    // Command palette: a centered panel floated over the page (flex centering;
    // serval maps justify-content through stylo_taffy).
    ".palette-overlay { display: flex; justify-content: center; padding-top: 56px; }",
    ".palette { width: 540px; background-color: rgb(244, 246, 250); padding: 10px; }",
    ".cmd-list { background-color: rgb(244, 246, 250); }",
    ".cmd-row { font-size: 18px; color: rgb(40, 44, 54); \
        background-color: rgb(244, 246, 250); padding: 8px 12px; }",
    ".cmd-row-active { font-size: 18px; color: rgb(20, 24, 34); \
        background-color: rgb(210, 222, 242); padding: 8px 12px; }",
];

/// Fallback chrome-band height (px) if the toolbar can't be measured.
const FALLBACK_TOOLBAR_H: u32 = 64;

/// Background of the floating content card — a light panel (the chrome palette's
/// surface tint), so the card reads as a card over the white orrery band.
const CARD_BG: wgpu::Color = wgpu::Color { r: 0.957, g: 0.965, b: 0.980, a: 1.0 };

/// Single-pane view-intent identity for the default session (one frame, one
/// pane). Per-frame / per-pane ids arrive with the tiled workbench (S4) and
/// session manifests (S3.2b).
const DEFAULT_FRAME: &str = "00000000-0000-0000-0000-0000000f1a3e";
const DEFAULT_PANE: u64 = 0;

/// The meerkat shell application: the shared chrome DOM, the runner that diffs
/// the chrome view tree into it, the orrery content-root, the window + GPU, and
/// input bookkeeping.
struct App {
    /// The chrome DOM the runner mutates and the render path reads.
    dom: Rc<RefCell<ScriptedDom>>,
    runner: ServalAppRunner<Chrome, ChromeLogic, ChromeView>,
    /// The content root: the [`Orrery`] — the graph's spatial presentation,
    /// rendered into the band below the chrome and driven by content-band input.
    orrery: Orrery,
    /// The navigation target last synced into the orrery via `visit`; guards
    /// re-visiting. Mirrors the chrome's `content_location`.
    content_location: String,
    /// Whether the orrery has been centered on its content band yet (done once,
    /// the first render after the toolbar height is known).
    centered: bool,
    /// The focused node's content card, laid out lazily and cached by
    /// `(url, card_w, card_h, content_tag)` so it re-renders only when the focus,
    /// card size, or fetch state changes. `None` when no single node is focused.
    card_scene: Option<netrender::Scene>,
    card_key: Option<(String, u32, u32, u8)>,
    /// Off-UI-thread fetch worker + the channels its outcomes arrive on: page
    /// documents (`fetch_rx`) and subresources (`sub_rx`).
    fetcher: fetch::Fetcher,
    fetch_rx: Receiver<fetch::FetchOutcome>,
    sub_rx: Receiver<fetch::SubresourceOutcome>,
    /// The p2p sync subsystem (S5.0): owns the transport + the tessera lane on
    /// its own runtime, held for the app's lifetime (its drop stops sync). Status
    /// changes arrive on `sync_rx` and fold into the chrome sync chip. `_`-named
    /// because it is a lifetime guard today; S5.1 gives it called methods.
    _sync: sync::SyncHost,
    sync_rx: Receiver<sync::SyncUpdate>,
    /// Per-URL fetched content state, keyed by the node's URL (URL identity).
    content: HashMap<String, fetch::ContentState>,
    /// Subresource bytes (page `<link>` CSS, `<img>` media) the HTML card pulls
    /// on demand while rendering, keyed by absolute URL. `RefCell` so the
    /// render-time loader can read it through a shared borrow of `self`.
    resources: RefCell<resources::ResourceStore>,
    /// Content engines for the card: nematic's document engines, routed by
    /// content-type. (HTML rides the serval lane instead, in `card`.)
    engines: EngineRegistry,
    /// The session's data directory (`<data_dir>/mere`): holds `graph.json` and
    /// the `views/` view-intent sidecars.
    session_dir: PathBuf,
    /// Durable content cache (S3.2c) under the session dir, persisting fetched
    /// pages + subresources by URL. `None` if the store could not be opened
    /// (caching disabled; the shell still runs).
    store: Option<FjallStore>,
    /// Cached measured height (px) of the chrome band; `0` until first measured.
    toolbar_h: u32,
    window: Option<Arc<Window>>,
    /// The shared serval-on-winit present stack, built once a window exists.
    host: Option<SurfaceHost>,
    /// Tracked keyboard modifiers, folded into each dispatched `KeyEvent`.
    modifiers: Modifiers,
    /// Last cursor position in physical pixels (window space == content space).
    cursor: (f32, f32),
    width: u32,
    height: u32,
}

impl App {
    fn new(proxy: EventLoopProxy<()>) -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = ServalAppRunner::new(
            dom.clone(),
            chrome_view as ChromeLogic,
            Chrome::new("mere://welcome"),
        );
        let content_location = runner.state().content_location().to_string();
        // The session lives under the per-user data dir; restore the graph + the
        // camera (view-intent) on launch, else seed fresh from the initial location.
        let session_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("mere");
        let _ = std::fs::create_dir_all(&session_dir);
        // Durable content cache (S3.2c) in a fjall keyspace under the session dir;
        // `None` simply disables caching (the shell runs without it).
        let store = match FjallStore::open(session_dir.join("content")) {
            Ok(store) => Some(store),
            Err(err) => {
                tracing::warn!(%err, "content cache unavailable; running without it");
                None
            },
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        let restored = match session_graph_store::load(&graph_file) {
            Ok(Some(graph)) => {
                tracing::info!(path = ?graph_file, "restored the session graph");
                Some(graph)
            },
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(%err, path = ?graph_file, "session graph load failed; starting fresh");
                None
            },
        };
        let mut orrery = match restored {
            Some(graph) => Orrery::with_graph(graph),
            None => {
                // The orrery opens on one node and grows from there as the user
                // navigates (the graph-rooted browse loop).
                let mut orrery = Orrery::new();
                if !content_location.is_empty() {
                    orrery.visit(&content_location);
                }
                orrery
            },
        };
        // Restore the view-intent (camera + focused node) so the spatial view and
        // the open card persist across restarts. A restored camera suppresses the
        // first-frame recenter; the focused node re-selects (if it still exists).
        let restored_view =
            view_intent_store::load_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE)
                .ok()
                .flatten();
        let restored_camera = restored_view.as_ref().and_then(|v| v.camera);
        if let Some(snapshot) = &restored_camera {
            orrery.set_camera(snapshot_to_camera(snapshot));
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            orrery.select_by_url(url);
        }
        let (fetcher, fetch_rx, sub_rx) = fetch::Fetcher::new(proxy.clone());
        // The p2p sync subsystem: binds the transport + joins the tessera demo
        // moot on its own runtime, delivering status changes through `proxy` (the
        // same wake the fetcher uses). Setup failure disables p2p, not the shell.
        let (sync, sync_rx) = sync::SyncHost::new(proxy, sync::DEMO_MOOT, sync::DEMO_SEED);
        let mut engines = EngineRegistry::new();
        for engine in nematic::engines() {
            engines.register(engine);
        }
        Self {
            dom,
            runner,
            orrery,
            content_location,
            centered: restored_camera.is_some(),
            card_scene: None,
            card_key: None,
            fetcher,
            fetch_rx,
            sub_rx,
            _sync: sync,
            sync_rx,
            content: HashMap::new(),
            resources: RefCell::new(resources::ResourceStore::default()),
            engines,
            session_dir,
            store,
            toolbar_h: 0,
            window: None,
            host: None,
            modifiers: Modifiers::default(),
            cursor: (0.0, 0.0),
            width: 1024,
            height: 600,
        }
    }

    /// The toolbar-band height (px), measuring + caching it on first use. The
    /// toolbar is a single flex row, so its border-box height is independent of
    /// the available width/height; measuring once suffices. Used to place the
    /// content root directly below the toolbar.
    fn toolbar_height(&mut self) -> u32 {
        if self.toolbar_h == 0 {
            self.toolbar_h = measure_class_bottom(&self.dom.borrow(), self.width, self.height, "toolbar")
                .unwrap_or(FALLBACK_TOOLBAR_H);
        }
        self.toolbar_h
    }

    /// Reconfigure the surface for `(width, height)` and request a redraw.
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(host) = self.host.as_mut() {
            host.resize(self.width, self.height);
        }
        self.request_redraw();
    }

    /// Render the two authorities and present them. The orrery content root fills
    /// everything below the toolbar; the chrome root is rendered over the full
    /// window with a *transparent* clear, so its toolbar band and any open
    /// dropdown float above the content while the rest lets the orrery show
    /// through. Composite order is content first, then chrome on top.
    fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let toolbar_h = self.toolbar_height().min(h);
        let content_h = h.saturating_sub(toolbar_h).max(1);

        // Chrome scene over the full window. Paint the caret / selection of the
        // focused field — the palette query when open, else the omnibar (byte
        // offsets from the field's char model).
        let cursor = self.runner.focus().map(|node| {
            let field = self.runner.state().active_field();
            let byte_of = |i: usize| {
                field.text().char_indices().nth(i).map(|(b, _)| b).unwrap_or(field.text().len())
            };
            let selection = field.has_selection().then(|| {
                let (s, e) = field.selection();
                (byte_of(s), byte_of(e))
            });
            TextCursor { node, caret: field.caret_byte_in_render(), selection }
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        let chrome_scene =
            scene_from_scripted_dom(&self.dom.borrow(), CHROME_SHEET, w, h, cursor, &scroll);

        // The content root: the orrery's own composited scene over the content
        // band. Keep its viewport in sync each frame; center it once, the first
        // time the band height is known.
        self.orrery.resize(w, content_h);
        if !self.centered {
            self.orrery.recenter();
            self.centered = true;
        }
        let (content_scene, orrery_redraw) = self.orrery.frame(w, content_h);

        // Focused-node content card (S2.2a): the selected node's synthesized media
        // as a floating card. Lay it out once per (url, card size) and cache; this
        // skips parley layout on every settle frame.
        let focus_url = self.orrery.focused_url().map(str::to_string);
        let card_geom = focus_url.as_ref().and_then(|_| card::card_rect(w, toolbar_h, h));
        if let (Some(url), Some((.., cw, ch))) = (focus_url.as_deref(), card_geom) {
            let state = self.content.get(url);
            let key = (url.to_string(), cw, ch, fetch::ContentState::tag(state));
            if self.card_key.as_ref() != Some(&key) {
                // The HTML lane pulls subresources (`<link>` CSS, `<img>`) through
                // a demand loader: a miss records the absolute URL in `wanted` and
                // renders without it. After the render we fetch each wanted URL we
                // have not already requested; its arrival re-renders the card.
                let wanted = RefCell::new(Vec::new());
                let scene = {
                    let loader = resources::ResourceLoader::new(&self.resources, url, &wanted);
                    card::render_content_scene(url, state, &self.engines, &loader, cw, ch)
                };
                self.card_scene = Some(scene);
                self.card_key = Some(key);
                let mut refilled = false;
                for res_url in wanted.into_inner() {
                    let newly = self.resources.borrow_mut().request(res_url.clone());
                    if newly {
                        // A durable cache hit fills the resource without a fetch;
                        // a miss spawns one (its arrival re-renders the card).
                        if let Some(stored) = self.load_cached(&res_url) {
                            self.resources.borrow_mut().insert(res_url, stored.body);
                            refilled = true;
                        } else {
                            self.fetcher.spawn_subresource(res_url);
                        }
                    }
                }
                if refilled {
                    // Re-render so the demand loader picks up the cache-filled resources.
                    self.card_key = None;
                    self.request_redraw();
                }
            }
        } else {
            self.card_key = None;
            self.card_scene = None;
        }

        let host = self.host.as_ref().unwrap();
        let (_chrome_tex, chrome_view) =
            host.rasterize(&chrome_scene, w, h, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
        let (_content_tex, content_view) =
            host.rasterize(&content_scene, w, content_h, ColorLoad::Clear(wgpu::Color::WHITE));
        // The focused-node card to its own offscreen target (kept alive — the view
        // borrows nothing, but mirror the chrome/content pattern of holding both).
        let card_raster = match (card_geom, self.card_scene.as_ref()) {
            (Some((.., cw, ch)), Some(scene)) => {
                Some(host.rasterize(scene, cw, ch, ColorLoad::Clear(CARD_BG)))
            },
            _ => None,
        };

        let Some(frame) = host.acquire() else { return };
        let target_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let format = host.format();
        // Content fills [toolbar_h, h] (dest_rect is [x0, y0, x1, y1] corners;
        // viewport is the full surface). Then the transparent-cleared chrome is
        // composited over the whole window — toolbar + dropdown on top, the rest
        // letting the orrery through.
        host.renderer().compose_external_texture(
            &content_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, toolbar_h as f32, w as f32, h as f32]),
        );
        // The focused-node card floats over the orrery, under the chrome.
        if let (Some((x0, y0, x1, y1, ..)), Some((_card_tex, card_view))) =
            (card_geom, card_raster.as_ref())
        {
            host.renderer().compose_external_texture(
                card_view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([x0, y0, x1, y1]),
            );
        }
        host.renderer().compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        frame.present();

        // Keep animating while the orrery is settling / gliding / dragging.
        if orrery_redraw {
            self.request_redraw();
        }
    }

    /// Request a redraw if a window exists.
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Sync the orrery to the chrome's current navigation target: when the
    /// location changes, `visit` it — adding a node and a browse-trail edge, or
    /// selecting the existing node (URL identity). Called after any input that can
    /// navigate (omnibar submit, suggestion / back / forward clicks, palette).
    fn sync_orrery(&mut self) {
        let loc = self.runner.state().content_location().to_string();
        if loc != self.content_location {
            self.orrery.visit(&loc);
            self.ensure_content(&loc);
            self.content_location = loc;
            self.save_session();
            self.request_redraw();
        }
    }

    /// Persist the session (graph + camera view-intent) under the session dir.
    /// Best-effort: a write failure is logged, not fatal. Called after each
    /// navigation and on window close.
    fn save_session(&self) {
        let graph_file = self.session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery.graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            camera: Some(camera_to_snapshot(self.orrery.camera())),
            focus: self.orrery.focused_url().map(str::to_string),
            ..Default::default()
        };
        if let Err(err) =
            view_intent_store::save_view_intent(&self.session_dir, DEFAULT_FRAME, DEFAULT_PANE, &intent)
        {
            tracing::warn!(%err, dir = ?self.session_dir, "failed to persist the view intent");
        }
    }

    /// Make the focused node's content available. A network address already in
    /// this session's content map is left as-is; otherwise a durable cache hit is
    /// shown without re-fetching (so a reload need not hit the network), and a
    /// miss marks it `Loading` and spawns a fetch.
    fn ensure_content(&mut self, url: &str) {
        if !fetch::is_fetchable(url) || self.content.contains_key(url) {
            return;
        }
        if let Some(stored) = self.load_cached(url) {
            self.content.insert(url.to_string(), fetch::ContentState::Ready(fetched_from(stored)));
            return;
        }
        self.content.insert(url.to_string(), fetch::ContentState::Loading);
        self.fetcher.spawn(url.to_string());
    }

    /// Load durably-cached content for `url` (page or subresource), or `None`.
    /// The fjall store's futures are ready, so `block_on` does not stall the UI.
    fn load_cached(&mut self, url: &str) -> Option<content_store::StoredContent> {
        let store = self.store.as_mut()?;
        pollster::block_on(content_store::load_content(store, url)).ok().flatten()
    }

    /// Persist `body` (+ its content-type) for `url` to the durable content cache,
    /// so a reload need not re-fetch it. Best-effort; a write failure is logged.
    fn save_cached(&mut self, url: &str, content_type: Option<String>, body: &[u8]) {
        let Some(store) = self.store.as_mut() else {
            return;
        };
        let stored = content_store::StoredContent { content_type, body: body.to_vec() };
        if let Err(err) = pollster::block_on(content_store::save_content(store, url, &stored)) {
            tracing::warn!(%err, url, "failed to cache content");
        }
    }

    /// Route a mouse button press/release by region. A left press in the chrome
    /// band (toolbar + any open dropdown) hit-tests + dispatches the chrome; any
    /// other press in the content band, and every release, goes to the orrery in
    /// content-band coordinates (its viewport top sits at the toolbar bottom).
    fn on_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        let orrery_button = match button {
            MouseButton::Left => Some(PointerButton::Left),
            MouseButton::Middle => Some(PointerButton::Middle),
            MouseButton::Right => Some(PointerButton::Right),
            _ => None,
        };
        let (x, y) = self.cursor;
        let th = self.toolbar_height() as f32;
        match state {
            ElementState::Pressed => {
                // The chrome's interactive area is the toolbar plus any open
                // dropdown (its `.chrome` border-box). A left press there dispatches
                // the chrome; below it (the content band) goes to the orrery.
                let chrome_h = {
                    let dom = self.dom.borrow();
                    measure_class_bottom(&dom, self.width, self.height, "chrome")
                        .unwrap_or(self.toolbar_h.max(FALLBACK_TOOLBAR_H))
                };
                if y < chrome_h as f32 {
                    if button == MouseButton::Left {
                        self.chrome_click(x, y);
                    }
                } else if let Some(b) = orrery_button {
                    if self.orrery.pointer_down(b, x, y - th) {
                        self.request_redraw();
                    }
                }
            },
            ElementState::Released => {
                // Releases always reach the orrery: it acts only if it owns an
                // in-progress pan / drag / marquee, so a chrome-band release is a
                // harmless no-op.
                if let Some(b) = orrery_button {
                    if self.orrery.pointer_up(b, x, y - th) {
                        self.request_redraw();
                    }
                }
            },
        }
    }

    /// Hit-test the chrome root at `(x, y)` and dispatch the click (buttons +
    /// suggestion / palette rows). A row / backdrop click that closes the palette
    /// restores focus so the caret doesn't dangle on the removed field.
    fn chrome_click(&mut self, x: f32, y: f32) {
        let offsets = ScrollOffsets::<NodeId>::default();
        let hit = {
            let dom = self.dom.borrow();
            hit_test_node(&dom, CHROME_SHEET, self.width, self.height, x, y, &offsets)
        };
        if let Some(node) = hit {
            let palette_was_open = self.runner.state().palette_open;
            self.runner.dispatch_click(node, PointerClick::at((x, y)));
            self.sync_orrery();
            if palette_was_open && !self.runner.state().palette_open {
                self.focus_after_palette_close();
            }
            self.request_redraw();
        }
    }

    /// Handle a pressed key. Ctrl+K toggles the command palette; while the
    /// palette is open all keys route to it. Otherwise Enter submits the omnibar,
    /// Arrow Up/Down and Escape drive the suggestions dropdown, and every other
    /// key edits the omnibar and regenerates suggestions.
    fn on_key_pressed(&mut self, key: &WinitKey) {
        if self.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("k"))
        {
            self.toggle_palette();
            return;
        }
        if self.runner.state().palette_open {
            self.on_palette_key(key);
            return;
        }
        let suggestions_open = !self.runner.state().suggest.is_empty();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) if self.runner.focus().is_some() => {
                self.runner.update(submit_omnibar);
                tracing::info!(
                    location = %self.runner.state().toolbar.editable.location,
                    "omnibar submit"
                );
                self.sync_orrery();
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowDown) if suggestions_open => {
                self.runner.update(|c| c.step_suggestion(1));
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowUp) if suggestions_open => {
                self.runner.update(|c| c.step_suggestion(-1));
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::Escape) if suggestions_open => {
                self.runner.update(Chrome::close_suggestions);
                self.request_redraw();
            },
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.modifiers) {
                    self.runner.dispatch_key(key_event);
                    self.runner.update(Chrome::refresh_suggestions);
                    self.request_redraw();
                }
            },
        }
    }

    /// Route a key to the open command palette: Enter runs the selection, Arrow
    /// Up/Down step it, Escape closes, anything else edits the query.
    fn on_palette_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.runner.update(Chrome::run_palette_selection);
                self.sync_orrery();
                self.focus_after_palette_close();
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.runner.update(Chrome::close_palette);
                self.focus_after_palette_close();
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.runner.update(|c| c.step_palette(1));
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.runner.update(|c| c.step_palette(-1));
                self.request_redraw();
            },
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.modifiers) {
                    self.runner.dispatch_key(key_event);
                    self.runner.update(Chrome::sync_palette_query);
                    self.request_redraw();
                }
            },
        }
    }

    /// Toggle the palette and move focus to match: into the palette query when
    /// it opens, back to the omnibar when it closes.
    fn toggle_palette(&mut self) {
        self.runner.update(Chrome::toggle_palette);
        if self.runner.state().palette_open {
            if let Some(node) = self.input_under_class("palette") {
                self.runner.set_focus(Some(node));
            }
        } else {
            self.focus_after_palette_close();
        }
        self.request_redraw();
    }

    /// Restore focus to the omnibar after the palette closes (so keyboard use
    /// continues there).
    fn focus_after_palette_close(&mut self) {
        let omnibar = self.input_under_class("toolbar");
        self.runner.set_focus(omnibar);
    }

    /// The first `<input>` under the first element carrying CSS class `class`
    /// (the omnibar under `.toolbar`, the query field under `.palette`).
    fn input_under_class(&self, class: &str) -> Option<NodeId> {
        let dom = self.dom.borrow();
        let container = first_with_class(&dom, dom.document(), class)?;
        first_tag(&dom, container, "input")
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Meerkat — Mere chrome on serval")
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create meerkat window"),
        );
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);

        // The shared serval-on-winit present stack: wgpu + netrender boot, surface
        // configured at the window size.
        let options =
            NetrenderOptions { tile_cache_size: Some(64), enable_vello: true, ..Default::default() };
        match SurfaceHost::boot(window.clone(), self.width, self.height, options) {
            Ok(host) => self.host = Some(host),
            Err(err) => {
                eprintln!("[meerkat] {err}");
                event_loop.exit();
                return;
            },
        }
        window.request_redraw();
        self.window = Some(window);

        // Show the restored focused node's content from the durable cache (so a
        // reload re-opens its card without a navigation). A fresh `mere://welcome`
        // focus is not fetchable, so this is a no-op there.
        if let Some(url) = self.orrery.focused_url().map(str::to_string) {
            self.ensure_content(&url);
        }
    }

    /// Drain completed fetches (delivery model 2): a worker woke us via the proxy;
    /// fold each outcome into the content cache and re-render the card.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        let mut card_changed = false;
        let mut graph_changed = false;
        while let Ok(outcome) = self.fetch_rx.try_recv() {
            let state = match outcome.result {
                Ok(fetched) => {
                    // A fetched document also contributes any linked data it
                    // carries (JSON-LD, or JSON-LD embedded in HTML) into the
                    // graph, reconciled into the spatial field via `ingest_graph`.
                    graph_changed |= self.orrery.ingest_graph(|g| {
                        meerkat::ingest::harvest(g, fetched.content_type.as_deref(), &fetched.body)
                    });
                    // Persist so a reload shows this page without re-fetching.
                    self.save_cached(
                        &outcome.url,
                        fetched.content_type.clone(),
                        fetched.body.as_bytes(),
                    );
                    fetch::ContentState::Ready(fetched)
                },
                Err(reason) => fetch::ContentState::Failed(reason),
            };
            self.content.insert(outcome.url, state);
            card_changed = true;
        }
        // Subresources (page CSS / media) on their own channel: fill the cache so
        // the next render's demand loader hits instead of missing.
        while let Ok(sub) = self.sub_rx.try_recv() {
            // Persist the subresource (its content-type is unknown here) so a
            // page's CSS / media survive restart alongside the page itself.
            self.save_cached(&sub.url, None, &sub.bytes);
            self.resources.borrow_mut().insert(sub.url, sub.bytes);
            card_changed = true;
        }
        // P2P sync status (S5.0): the same wake also carries lane-status changes.
        // Fold the latest into the chrome chip (the host owns the mutation).
        let mut latest_sync = None;
        while let Ok(update) = self.sync_rx.try_recv() {
            latest_sync = Some(sync::to_indicator(&update.status, sync::LANE_LABEL));
        }
        if let Some(indicator) = latest_sync {
            self.runner.update(|c| c.sync = indicator.clone());
            self.request_redraw();
        }
        if graph_changed {
            self.save_session();
        }
        if card_changed || graph_changed {
            self.card_key = None; // force the card to re-render from the new state
            self.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_session();
                event_loop.exit();
            },
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                // Forward to the orrery in content-band coordinates, so an
                // in-progress pan / drag / marquee tracks even when the pointer
                // strays over the chrome.
                let th = self.toolbar_height() as f32;
                if self.orrery.cursor_moved(self.cursor.0, self.cursor.1 - th) {
                    self.request_redraw();
                }
            },
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = modifiers_from_winit(mods.state());
                self.orrery.set_ctrl(self.modifiers.ctrl);
            },
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel over the content band drives the orrery (pan, or zoom under
                // Ctrl). LineDelta is scaled to device px the way the orrery
                // expects; PixelDelta passes through.
                let th = self.toolbar_height() as f32;
                if self.cursor.1 >= th {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE)
                        },
                        MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                    };
                    if self.orrery.wheel(dx, dy) {
                        self.request_redraw();
                    }
                }
            },
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_input(state, button),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.on_key_pressed(&event.logical_key);
                }
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

/// Lay out the chrome root and return the border-box bottom (px, rounded up) of
/// the first element carrying CSS class `class` — `"toolbar"` for the content
/// split, `"chrome"` for the click-region gate (toolbar + open dropdown).
/// `None` if no such element is laid out.
fn measure_class_bottom(dom: &ScriptedDom, w: u32, h: u32, class: &str) -> Option<u32> {
    let frags = fragments_from_scripted_dom(dom, CHROME_SHEET, w, h);
    first_with_class(dom, dom.document(), class)
        .and_then(|node| frags.rect_of(node))
        .map(|layout| (layout.location.y + layout.size.height).ceil() as u32)
        .filter(|&measured| measured > 0)
}

/// The first element carrying CSS class `class` in pre-order under `id`.
fn first_with_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Option<NodeId> {
    if has_class(dom, id, class) {
        return Some(id);
    }
    dom.dom_children(id).find_map(|c| first_with_class(dom, c, class))
}

/// The first element with local tag `local` in pre-order under `id`.
fn first_tag(dom: &ScriptedDom, id: NodeId, local: &str) -> Option<NodeId> {
    if dom.element_name(id).is_some_and(|q| q.local.as_ref() == local) {
        return Some(id);
    }
    dom.dom_children(id).find_map(|c| first_tag(dom, c, local))
}

/// Whether element `id` carries CSS class `class` (whitespace-split `class` attr).
fn has_class(dom: &ScriptedDom, id: NodeId, class: &str) -> bool {
    dom.attributes(id).any(|attr| {
        attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
    })
}

/// Map the orrery camera (pan + zoom) to a serialized [`CameraSnapshot`] — the
/// kurbo `Affine` coefficient order `[a, b, c, d, e, f]`. The orrery camera is a
/// pure scale + translate (no rotation / skew), so `a = d = zoom`, `b = c = 0`,
/// and `(e, f)` is the offset.
fn camera_to_snapshot(camera: CameraView) -> CameraSnapshot {
    let zoom = camera.zoom as f64;
    CameraSnapshot {
        coefficients: [zoom, 0.0, 0.0, zoom, camera.offset.0 as f64, camera.offset.1 as f64],
    }
}

/// The inverse of [`camera_to_snapshot`]: recover pan + zoom from the affine
/// coefficients (scale from `a`, offset from `e` / `f`; rotation / skew are
/// ignored, as the orrery never sets them).
fn snapshot_to_camera(snapshot: &CameraSnapshot) -> CameraView {
    let c = snapshot.coefficients;
    CameraView { offset: (c[4] as f32, c[5] as f32), zoom: c[0] as f32 }
}

/// A durably-cached entry as a [`fetch::Fetched`], decoding the stored body as
/// text (lossily). Binary subresources are served from the resource cache as
/// bytes; this text view is for the page-document lane.
fn fetched_from(stored: content_store::StoredContent) -> fetch::Fetched {
    fetch::Fetched {
        content_type: stored.content_type,
        body: String::from_utf8_lossy(&stored.body).into_owned(),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("meerkat=info")),
        )
        .init();
    tracing::info!("meerkat-shell starting");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("event loop error");
}
