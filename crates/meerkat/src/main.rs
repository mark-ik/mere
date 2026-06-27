/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat-shell: the on-screen serval host for Mere's chrome.
//!
//! A winit window that runs the reused chrome ([`meerkat::chrome_view`] over a
//! [`meerkat::Chrome`] wrapping the graphshell `ToolbarState`) through serval and
//! presents via netrender. Its `ScriptedDom → Scene` render glue (the
//! `serval_render` module: [`crate::serval_render::scene_from_session`] and
//! the point→node [`crate::serval_render::hit_test_node`]) calls serval-layout +
//! paint_list_render directly, so this file is the window + present +
//! input-dispatch harness, not a second engine.
//!
//! ## One shell document, one window
//!
//! The window draws one **shell document**: a single `ScriptedDom` under one
//! [`ServalAppRunner`], holding the chrome (toolbar, omnibar, palette, overlays),
//! the folded panes (roster, apparatus, steward, inspector, trail) as lensed
//! subtrees, and the orrery's node-card chips as transform-positioned DOM. That
//! document runs through serval-layout into one chrome `Scene`. Around and beneath
//! it the host composites separate surfaces that are not serval documents: the
//! orrery graph scene ([`Orrery`]'s own `Scene` of gnodes / edges / physics from
//! `gyre`), the pelt workbench tile surface, and the focused node's content card.
//! Each rasterizes to its own texture and composites back to front: the orrery
//! scene underneath, the chrome on top. The capability-separation discipline holds
//! (neither the shell document nor a content surface sees the other's tree).
//!
//! Input routes top-down through the shell hit-test first: a press resolves
//! against the one document (chrome control, folded-pane row, or orrery card), and
//! an orrery-area miss falls through to `gyre`'s pan / zoom / drag / select.
//! Keyboard goes through the runner's `dispatch_key` (Tab traversal, Enter / Space
//! activation) for focusables, then to the graph handler. This is the
//! unified-document-host shape (Phase 1 complete); it replaced the earlier
//! two-root, route-by-Y-band composition.
//!
//! The orrery-as-element work lives in the unified-document-host plan: Phase 2a
//! landed (node cards select + focus through the shell hit-test); retiring the
//! standalone orrery `Scene` into a scene underlay (cond 5) remains.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use accesskit::NodeId as AccessNodeId;
use eidetic_fjall::FjallStore;
use forme::GraphMemberId;
use frame::{
    FrameId, FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SessionId,
};
use inker::EngineRegistry;
use layout_dom_api::LayoutDom;
use meerkat::{Chrome, ChromeLogic, chrome_view};
use orrery::{CameraView, Orrery};
use crate::serval_render::fragments_from_scripted_dom;
use serval_layout::FragmentPlane;
use platen::Workbench;
use register_diagnostics::{DiagnosticEvent, install_global_sender};
use register_theme::chrome::{ChromeTheme, Color32};
use register_theme::theme::ThemeRegistry;
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::RenderCore;
use session_runtime::{
    ManifestStore, SwitcherThumbnail, frame_layout_store, manifest::GraphSessionManifest,
    session_graph_store, settings_store, view_intent_store,
};
use tracing_subscriber::prelude::*;
use winit::window::{ResizeDirection, WindowId};
use xilem_serval::ServalAppRunner;

mod card;
mod doc_style;
mod comms_host;
mod constellation;
mod browse_capture;
mod content;
mod crawl;
mod fetch;
mod resources;
mod sync;

mod a11y_bridge;
#[cfg(any(test, feature = "agent-harness"))]
mod agent_harness;
mod app_handler;
mod apparatus;
mod command_drain;
mod engine_activation;
mod export;
mod find;
mod find_worker;
mod frame_a11y;
mod frame_a11y_panes;
mod frame_ops;
mod frame_view;
mod viewport;
mod gloss;
mod graphlets;
mod ime;
mod input;
mod menus;
mod nav_sync;
mod node_ops;
mod inspector;
mod list_pane;
mod observability;
mod pane_data;
mod pane_geom;
mod pane_session;
mod render;
mod roster;
mod roster_view;
mod scene_settings;
mod settings_lane;
mod settings_node;
mod settings_pane_view;
mod sprite_import;
mod swatch;
// `ViewPane` is the shared base for the `RosterPane` / `ListPane` test harnesses only;
// every product pane now folds into the shell document, so the module is test-gated.
// (Phase 1, step 2.)
#[cfg(test)]
mod view_pane;
mod scrying_host;
mod serval_a11y;
mod serval_render;
mod session_ops;
mod shellbar;
mod steward;
mod switcher;
mod tags;
mod theme_edit;
mod theme_store;
mod text;
mod titlebar;
mod tracing_layer;
mod window_view;
mod utility_panes;

use constellation::Constellation;
use observability::HostObservability;


mod theme_sheets;
pub(crate) use theme_sheets::*;

/// Single-pane view-intent identity for the default session (one frame, one
/// pane). Per-frame / per-pane ids arrive with the tiled workbench (S4) and
/// session manifests (S3.2b).
const DEFAULT_FRAME: &str = "00000000-0000-0000-0000-0000000f1a3e";
const DEFAULT_PANE: u64 = 0;

/// The frame tree's graph pane — the always-present leaf hosting the orrery. The
/// tiled workbench is a separate summonable pane that coexists beside it (no
/// longer a projection toggle inside one leaf). Summoned sibling panes (roster,
/// workbench, …) get fresh ids from `next_pane_id`. (Frame tree, F1 / W.)
const GRAPH_PANE: PaneId = PaneId(0);

/// Which content pane navigation acts on — the **last-interacted** one. The
/// orrery and the tiled workbench coexist as panes; this disambiguates the single
/// nav target (omnibar / Ctrl+Enter / Back-Forward) between them. (Workbench-as-
/// pane: focus follows the last-clicked content pane.)
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ContentPane {
    #[default]
    Orrery,
    Workbench,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum A11yHostAction {
    SelectNodeByUrl(String),
    /// A chrome control, by its DOM node in the chrome runner. A screen reader's
    /// `Focus` sets the runner's focus to it; a `Click` dispatches to its handler
    /// — the same activation paths a pointer drives. The whole `NodeId` is stored
    /// (keyed by the node's salted a11y id) rather than reversed from that id,
    /// because on 64-bit debug builds `NodeId::raw()` packs a doc-tag into the same
    /// high bits the salt uses, so the salted id cannot be inverted. (G2.4.)
    ChromeNode(NodeId),
}

/// The meerkat shell application: the shared chrome DOM, the runner that diffs
/// the chrome view tree into it, the orrery content-root, the window + GPU, and
/// input bookkeeping.
/// Session + app state shared across every window. A second window is a second
/// [`WindowView`](window_view::WindowView) over this same `SharedState`. Subdivided
/// into subsystems so a per-window handler can take a narrow borrow of just the
/// subsystem it touches — the seam the `ShellCommand` path leans on. Multi-member
/// groups nest (`content` / `session` / `presentation` / `inbox`); single-member
/// ones stay flat (`comms_handle` / `sync_handle` / `observability`). (Multi-window
/// MW2.)
struct SharedState {
    /// Active-node pool + the fetched-page cache that feeds it.
    content: Content,
    /// The session registry + the active session's identity / paths / switcher caches.
    session: Session,
    /// Theming + the persisted chrome settings every window's chrome renders from.
    presentation: Presentation,
    /// The comms actor's command handle (P6c). The actor owns the live `Comms`
    /// (misfin + murm adapters) on its own tokio runtime; conversation lists +
    /// threads arrive on `inbox.comms`, and load / send verbs are `CommsCommand`s.
    comms_handle: armillary::ActorHandle<comms_host::CommsCommand>,
    /// The p2p sync actor's command handle (S5.0 / S5.1). The actor owns the
    /// transport + tessera lane on its own tokio runtime; status arrives on
    /// `inbox.sync`, and the "connect to peer" verb is a `SyncCommand`.
    sync_handle: armillary::ActorHandle<sync::SyncCommand>,
    /// The kernel inbox: the typed receivers the I/O actors deliver on, behind the
    /// one winit wake. `user_event` is the single documented place that reads them.
    inbox: KernelInbox,
    /// Bounded observation cache backing the Apparatus diagnostics pane.
    observability: HostObservability,
}

/// The `content` subsystem: the active-node pool and the page-content cache that
/// backs it. Shared across windows — one activation lifecycle, one cache.
struct Content {
    /// The constellation: the pool of active nodes (their content actors). The
    /// focused card (Cartography) and the workbench tiles (Tree) both draw their
    /// scenes from here — one activation lifecycle, not two. Reconciled to the
    /// needed set each frame; backgrounded nodes outlive the view.
    constellation: Constellation,
    /// Per-URL fetched content state, keyed by the node's URL (URL identity).
    pages: HashMap<String, fetch::ContentState>,
    /// Durable content cache (S3.2c) under the session dir, persisting fetched
    /// pages + subresources by URL. `None` if the store could not be opened
    /// (caching disabled; the shell still runs).
    store: Option<FjallStore>,
    /// The fetch actor's command handle (the kernel commands it over this; its
    /// outcomes arrive on `inbox.fetch`).
    fetch_handle: armillary::ActorHandle<fetch::FetchCommand>,
    /// The find-in-page worker's command handle: the kernel ships it the focused page +
    /// query off the UI thread, and its match rects arrive on `inbox.find`. (Find.)
    find_worker: armillary::ActorHandle<find_worker::FindCommand>,
    /// The nematic engine registry, for rendering "last visit" snapshot cards
    /// host-side from the durable content cache (no actor). (Card #4.)
    engine_registry: EngineRegistry,
    /// Per-node engine pins (member → engine id). The compatibility view is a pin
    /// to `scrying.web`; the picker (engine-picker plan) writes other ids here.
    /// Session state, shared across windows: the pin is the *intent*; for a
    /// surface-engine pin, each window's per-`WindowView` producer pool spawns the
    /// HWND-bound WebView that serves it. A torn-out compat tile (MW4) carries the
    /// pin, the recipient spawns a fresh WebView. (Replaces the `compat_pins` bool;
    /// engine-picker Phase 0. The durable per-node graph field takes over later.)
    engine_pins: HashMap<GraphMemberId, String>,
    /// The engine routing policy: scheme / content-type / per-host / pin → engine
    /// id. Consulted at nav time (scheme + pin) to choose the tier (surface engine
    /// vs the document/constellation lane); the document-engine re-route by
    /// content-type is the actor's second pass. (engine-picker Phase 0.)
    route_policy: inker::routing::EngineRoutePolicy,
    /// Which present engines are active this session (global default from settings +
    /// per-session overrides). `engine_available` gates routing on this, so a
    /// deactivated engine is never picked and spawns no actors. (engine-picker Phase 1.)
    engine_activation: engine_activation::EngineActivation,
    /// The crawl actor's owner (relational-browse V2): a `>crawl` on a focused page
    /// seeds a bounded crawl whose harvested link + metadata contributions drain back
    /// each frame and apply to the focused graph. One crawl at a time.
    crawl: crawl::CrawlSession,
    /// Whether the live trail recorder writes a `BrowsingTrace` per navigation
    /// (C1). Default on; the consent layer (plan C4) drives this and a
    /// per-session incognito exclusion. Recorded traces are always `LocalOnly` —
    /// sharing is a separate, explicit act.
    capture_enabled: bool,
}

/// The `session` subsystem: the session registry plus the active session's
/// identity, on-disk paths, and switcher caches.
struct Session {
    /// The on-disk session registry, loaded from `<mere_root>/sessions/`. (MG1.)
    manifests: ManifestStore,
    /// The session whose graph + frame + views are loaded right now; its dir is
    /// `session_dir`. (Multi-graph MG1.)
    active_session_id: SessionId,
    /// The active session's persona — the identity boundary its persona-scoped data
    /// (engine UDFs, the configurable menu, future vaults) is filed under. v0 has one
    /// default persona; threading the manifest's id keeps the wiring persona-ready.
    active_persona: session_runtime::PersonaId,
    /// The active session's per-session data dir (`<mere_root>/sessions/<id>/`):
    /// holds `graph.json`, `frame.json`, and the `views/` sidecars. (Multi-graph.)
    session_dir: PathBuf,
    /// The shared per-user data root (`<data_dir>/mere`): settings, the content
    /// cache, and comms live here, above the per-session dirs. (Multi-graph MG1.)
    mere_root: PathBuf,
    /// Cached switcher thumbnails per session (the F2.3 shellbar switcher rows);
    /// rebuilt on session/graph change, the active one from the live orrery. (MG4.)
    session_thumbnails: HashMap<SessionId, SwitcherThumbnail>,
    /// Cached switcher label per session (display name, else derived from the
    /// graph), refreshed in lockstep with `session_thumbnails`. (Host text path.)
    session_labels: HashMap<SessionId, String>,
    /// Host text shaping for host-drawn labels (the switcher tile names). Holds the
    /// parley contexts so they aren't rebuilt per frame. (Host text path.)
    host_text: text::HostText,
}

/// The `presentation` subsystem: the resolved theme + the persisted chrome
/// settings every window's chrome renders from.
struct Presentation {
    /// The theme registry, kept so the apparatus pane can switch themes at runtime
    /// (re-resolve → rebuild the chrome sheet + tokens). (Theme switcher.)
    theme: ThemeRegistry,
    /// The active theme's chrome tokens — kept beside the baked `chrome_sheet` for
    /// the host-drawn surfaces the CSS can't reach (the window-control glyphs).
    chrome_theme: ChromeTheme,
    /// The active theme's chrome CSS (built from a resolved [`ChromeTheme`] at
    /// startup). The render / measure / hit-test paths read it instead of a const,
    /// so a theme switch rebuilds it and the whole shell re-themes. (Theming pass.)
    chrome_sheet: Vec<String>,
    /// The active theme's id (e.g. `theme:dark`), persisted in settings.
    active_theme_id: String,
    /// The active-tab cap last written to the settings sidecar. Guards the persist
    /// path so an unchanged value isn't re-written on every chrome click.
    saved_tab_cap: usize,
    /// Which window edge the shellbar is docked to. Persisted in settings.json.
    shellbar_edge: session_runtime::ShellbarEdge,
    /// Whether the shellbar is hidden (the user's explicit hide toggle, distinct from a
    /// leaf window's slim chrome). Persisted in settings.json; revealed from the palette /
    /// `>shellbar`. (Hide-shellbar.)
    shellbar_hidden: bool,
    /// Linear damping for orrery node bodies — the "inertia" physics setting,
    /// adjusted in the apparatus pane and persisted. The host owns the value and
    /// pushes it to each orrery via `set_physics_damping`. (Physics settings.)
    physics_damping: f32,
    /// The user's chrome zoom multiplier (Ctrl +/-/0), persisted. Composed with the
    /// display's [`dpi_scale`](Self::dpi_scale) into the effective
    /// [`ui_scale`](Self::ui_scale). Default 1.1, the baseline "a point or two larger"
    /// bump. Shared across this session's windows. (UI scale.)
    user_zoom: f32,
    /// The display's DPI factor (winit `scale_factor()`), folded into `ui_scale` now
    /// the window is sized in **logical** px so the chrome tracks the OS scale. 1.0 at
    /// 100%. Shared today; goes per-window in the auto-DPI plan's D3 (multi-monitor).
    /// (Auto-DPI D1.)
    dpi_scale: f32,
    /// The active theme's document-lane palette (content cards: smolweb /
    /// markdown / feed text). Threaded into content actors so baked glyph colors
    /// follow the theme; also read by the host for rule / image colors at lower
    /// time. Rebuilt on theme switch. (Document theming, P3.)
    document_palette: document_canvas::ColorVocabulary,
    /// The user's document **typography** (base size, line spacing, fonts, link
    /// adornment). Composed with `document_palette` into the sheet the content
    /// actors lay out with; edited in the `pelt/reading` page and persisted.
    /// Its own `colors` field is ignored (the palette overwrites it at compose
    /// time). (Document typography surface.)
    document_sheet: document_canvas::DocumentStyleSheet,
    /// The persona-curated context-menu command list (command registry P4): the registry ids
    /// shown in the right-click menu, in order. Loaded from the persona settings store at boot
    /// (or the registry default when unset), persisted on change; the menu builder resolves +
    /// applicability-filters each id for the current selection.
    menu_actions: Vec<String>,
    /// How many times each registry command has run — the frequency behind the context menu's
    /// auto-suggestions (command registry S3). Keyed by registry id; loaded from / persisted to
    /// the persona settings store, incremented at the command-invocation hook.
    command_usage: std::collections::BTreeMap<String, u32>,
    /// The short-term memory eviction policy (the Alembic Recent header). Loaded from the persona
    /// settings store at boot, cycled by the header control, persisted on change; read by the
    /// Recent-header display and `run_forgetting_pass`. (Editable eviction policy, B4.)
    eviction_policy: session_runtime::memory_levels::EvictionPolicy,
}

impl Presentation {
    /// The active theme's chrome CSS as `&[&str]`, the shape the serval layout /
    /// paint / hit-test entry points take. Borrows the baked `chrome_sheet`. A read
    /// of shared presentation state, so it lives on the subsystem that owns it; every
    /// window's chrome renders from the same sheet. (MW2 (c).)
    fn chrome_sheet_refs(&self) -> Vec<&str> {
        self.chrome_sheet.iter().map(String::as_str).collect()
    }

    /// The effective chrome scale: the display's DPI factor times the user's zoom,
    /// clamped to a sane band. Now that the window is sized in **logical** px (so a 2×
    /// display gives a 2×-physical window), folding `scale_factor` in makes the chrome
    /// fill it at the right density instead of overflowing a physically-small window
    /// (the earlier-attempt bug). (Auto-DPI D1.)
    fn ui_scale(&self) -> f32 {
        (self.dpi_scale * self.user_zoom).clamp(0.5, 4.0)
    }

    /// Rebuild the chrome sheet from the active theme tokens at the current
    /// [`ui_scale`](Self::ui_scale), re-adding the syntax-highlight rules. Called
    /// after a zoom (Ctrl +/-/0) or a display DPI change; the theme switcher has its
    /// own rebuild in `theme_edit`. (UI scale.)
    fn rebuild_chrome_sheet(&mut self) {
        let seeds = self
            .theme
            .theme_def(&self.active_theme_id)
            .map(|d| d.seeds)
            .unwrap_or_else(meerkat::knot_highlight::fallback_seeds);
        let mut sheet = scale_px(chrome_sheet(&self.chrome_theme), self.ui_scale());
        sheet.extend(meerkat::knot_highlight::syntax_css(&seeds));
        self.chrome_sheet = sheet;
    }

    /// The composed document style sheet the content actors lay out with: the
    /// user's typography with the active theme's document colours overlaid. The
    /// one place typography ⊕ palette meet; `drive` / `set_theme` / the snapshot
    /// path all send this. (Document typography surface.)
    fn document_sheet_composed(&self) -> document_canvas::DocumentStyleSheet {
        document_canvas::DocumentStyleSheet {
            colors: self.document_palette,
            ..self.document_sheet.clone()
        }
    }
}

/// How many graph orreries stay warm in the pool before the least-recently-
/// focused non-focused one is evicted. Each live orrery costs memory + its own
/// physics actor thread, so the pool is bounded; "a handful" keeps several
/// sessions warm (instant switch-back) without unbounded growth. A configurable
/// setting later (per the configurability rule). (Window composition P1, OQ2.)
const MAX_POOLED_ORRERIES: usize = 8;

struct Shell {
    /// Session + app state shared across every window. (Multi-window MW2.)
    shared: SharedState,
    /// The pooled orrery authorities, keyed by graph. Each is a whole [`Orrery`]
    /// (graph + physics + camera) — the source of every pane's content for that graph.
    /// Panes resolve to one by `graph_id`; the ctx bundles the window's focused-graph
    /// orrery as `self.orrery`. A sibling `Shell` field (not in `SharedState`) so it
    /// borrows disjointly from `shared` / `view`, as the single `orrery` did before.
    /// (Window composition P1; was the single `orrery: Orrery`.)
    orreries: HashMap<GraphId, Orrery>,
    /// Pooled graphs in least-recently-focused order (front = stalest). A graph
    /// moves to the back when focused; over [`MAX_POOLED_ORRERIES`] the stalest
    /// non-focused one is evicted (dropped, ending its physics thread). The graph
    /// was already saved when it was last switched away from, so eviction needs no
    /// save; switching back to it reloads from disk. (Window composition P1, OQ2 —
    /// the unload half of pool eviction.)
    orrery_lru: Vec<GraphId>,
    /// Per-session graphlet indices, keyed by graph (a sibling pool to `orreries`).
    /// Each holds the named sub-structures over that graph's nodes — today a tear-out
    /// **branch** graphlet, later document-groups / relational-browse neighborhoods.
    /// Loaded lazily from the `graphlets.json` sidecar; mutated + persisted on branch.
    /// (Graphlet-wiring Phase 1; reuses forme's `GraphletRef`, not its `GraphTree`.)
    graphlets: HashMap<GraphId, graphlets::SessionGraphlets>,
    /// All live windows, keyed by OS `WindowId` — the registry. Every per-window
    /// handler is dispatched by resolving the event's id to its view here. At N=1
    /// it holds just the primary; tear-out (MW3+) inserts more. (Multi-window MW2 (d).)
    windows: HashMap<WindowId, window_view::WindowView>,
    /// Which window is primary (owns the orrery + save-on-close). `None` until the
    /// first window is created in `resumed`. (MW2 (d).)
    primary: Option<WindowId>,
    /// The primary view, built in `new()` and consumed by `resumed` once the OS
    /// window (and thus its `WindowId`) exists. winit splits construction from window
    /// creation, so the view outlives its registry key for exactly one step. (MW2 (d).)
    pending_view: Option<window_view::WindowView>,
    /// The shared present core: one wgpu device + netrender `Renderer`, booted once on
    /// the first `resumed`. Every window's `WindowSurface` is created from it, so N
    /// windows present through one device. `None` until the first window boots it.
    /// Shared infra, so it sits on `Shell` (like `clipboard`), not in `SharedState`.
    /// (MW3: one device, N surfaces.)
    render_core: Option<RenderCore>,
    /// System clipboard for the omnibar / palette Ctrl(Cmd)+C/X/V. `None` if the
    /// platform clipboard could not be opened (the shortcuts then no-op). System-
    /// global, so it stays on `Shell`, not in `SharedState`.
    clipboard: Option<arboard::Clipboard>,
    /// The **primary** window's platform AccessKit bridge, fed by the same host-local
    /// uxtree snapshot as Apparatus. Unsupported platforms keep this as an explicit
    /// degraded bridge. Secondary (leaf) windows get their own in
    /// [`secondary_a11y_bridges`](Self::secondary_a11y_bridges); `ctx()` (always the
    /// primary) uses this one, `window_ctx` forks by id. (Per-window a11y, MW3 step 6.)
    a11y_bridge: a11y_bridge::AccessKitBridge,
    /// Per-secondary-window AccessKit bridges (MW3 step 6). The primary's bridge is
    /// `a11y_bridge` above; each spawned leaf gets its own here, keyed by its
    /// `WindowId` and installed against its own window. `window_ctx` resolves the right
    /// one (primary field vs this map); `close_window` drops it.
    secondary_a11y_bridges: HashMap<WindowId, a11y_bridge::AccessKitBridge>,
    /// The event-loop proxy that wakes the kernel from any window's AccessKit adapter,
    /// kept so a spawned window can mint its own bridge with the same wake. (MW3 step 6.)
    a11y_proxy: winit::event_loop::EventLoopProxy<()>,
    /// Host-owned routes for actionable AccessKit nodes in the current snapshot.
    /// The bridge only queues raw AccessKit requests; the kernel thread resolves
    /// ids through this table and applies semantic host actions.
    a11y_action_routes: HashMap<AccessNodeId, A11yHostAction>,
    /// The cross-window command queue: per-window handlers push [`ShellCommand`]s
    /// here (spawn / close a window) and the event loop drains them through
    /// [`Shell::apply`] in `about_to_wait`, once the borrowing ctx has ended. (MW3.)
    commands: Vec<ShellCommand>,
    /// The wake every pooled orrery's physics actor pokes (the winit proxy, host-
    /// neutral). Held so a graph minted into the pool at session-switch time gets
    /// its own offloaded physics, like the seed orrery did at boot. (Window
    /// composition P1, multi-graph.)
    physics_wake: armillary::Wake,
    /// Marks this struct as the kernel-thread context: `!Send` by construction
    /// (armillary's typed boundary), so kernel authority cannot be moved onto an
    /// actor thread — the attempt is a compile error, not a review catch.
    _kernel: armillary::KernelThread,
}

/// The borrow bundle for handling **one window's** events. The bulk of the
/// event-handling logic hangs off `impl WindowCtx` rather than `impl Shell`, so a
/// handler operates on exactly one window's [`WindowView`] plus the shared state
/// and the shell singletons the active window legitimately drives (the orrery,
/// the clipboard, the a11y bridge). The registry picks *which* window by building
/// the ctx over `windows[&id]`; that construction is the seam — a ctx method
/// cannot reach another window or the window map, so cross-window work (spawn /
/// close / move-tile) goes through `ShellCommand` instead. Bodies are unchanged
/// from when these were `&mut self` on `Shell`: `self.view` / `self.shared`
/// resolve to these fields; `self.orrery()` / `self.orrery_mut()` resolve the
/// focused pane's orrery out of the pool. (Multi-window MW2 (c); Window
/// composition P2.)
struct WindowCtx<'a> {
    view: &'a mut window_view::WindowView,
    shared: &'a mut SharedState,
    /// The orrery pool (every live graph's authority), borrowed whole so render
    /// and input can resolve *any* pane's orrery by `graph_id`, not just the
    /// window-focused one. The focused-bucket sites reach it through
    /// [`WindowCtx::orrery`] / [`WindowCtx::orrery_mut`]; per-pane paths resolve a
    /// specific `graph_id`. Was the single bundled `orrery: &mut Orrery` (P1).
    /// (Window composition P2.)
    orreries: &'a mut HashMap<GraphId, Orrery>,
    /// The per-session graphlet pool (read-only here), so a **branch** window's render
    /// can scope its orrery to its graphlet's live roster. (Graphlet wiring Phase 2
    /// slice 3.)
    graphlets: &'a HashMap<GraphId, graphlets::SessionGraphlets>,
    clipboard: &'a mut Option<arboard::Clipboard>,
    a11y_bridge: &'a mut a11y_bridge::AccessKitBridge,
    a11y_action_routes: &'a mut HashMap<AccessNodeId, A11yHostAction>,
    /// The shared present core (device + renderer). `None` before the first window
    /// boots it, and in the headless harness; the render path early-returns then.
    render_core: Option<&'a RenderCore>,
    /// The shell command queue. A per-window handler reaches exactly one window, so
    /// work that touches the registry or a second window (spawn / close) can't run
    /// here — it's pushed as a [`ShellCommand`] and applied by `Shell` after the ctx
    /// borrow ends. (Multi-window MW3, the deferred MW2 (e).)
    commands: &'a mut Vec<ShellCommand>,
    /// How many graphs are live in the orrery pool right now (a plain count, not a
    /// borrow — resolved at ctx build). Surfaced in Steward as the tripwire for the
    /// pool's bound (live / cap). (Window composition P1, OQ2.)
    orrery_pool_count: usize,
    /// While a **branch** window's pass has the orrery scoped to its graphlet, this holds
    /// the orrery's prior scope, restored on ctx `Drop` so the override is transient and
    /// never leaks to the next window's pass. `None` when this pass set no branch scope.
    /// (Graphlet wiring Phase 2 slice 3.)
    branch_scope_restore: Option<Option<Vec<uuid::Uuid>>>,
}

/// A deferred shell-level operation a per-window handler requests but cannot perform
/// itself: it needs full `&mut Shell` (to mutate the window registry) or the
/// `ActiveEventLoop` (to create an OS window), neither reachable from a [`WindowCtx`]
/// (which borrows exactly one window + the shared state). Handlers push onto
/// `Shell.commands`; the event loop drains them through `Shell::apply` after the ctx
/// borrow ends. This is the cross-window seam — spawning or closing a window is a
/// registry op no single-view ctx can express. (Multi-window MW3, the deferred MW2 (e).)
enum ShellCommand {
    /// Open a new OS window over the shared session — a second [`WindowView`].
    /// (Cmd/Ctrl+Shift+N; MW3 step 3. Step 4 differentiates its kind + chrome.)
    SpawnWindow,
    /// Tear a node out into a new leaf window (the tear-out drag, G1). Carries the
    /// torn node's stable id so the leaf can resolve it (G2 will show its tile). Runs
    /// on `Shell` after the ctx borrow ends, like `SpawnWindow`. (Tear-out gestures G1.)
    TearOut { node: uuid::Uuid },
    /// Cross-graph copy (G5): a node dragged from graph `from`'s pane onto graph `to`'s
    /// pane mints a copy in `to` (via `Graph::copy_node_from`, with `CopiedFrom`
    /// provenance back to the source). A two-orrery pool op, so it runs on `Shell` after
    /// the ctx borrow ends. (Tear-out gestures G5.)
    CopyNodeAcross {
        node: uuid::Uuid,
        from: GraphId,
        to: GraphId,
    },
    /// Fork (Ctrl+Shift tear, G4): mint an independent session + graph holding a copy of
    /// the dragged node's connected component (with a weak parent ref to the donor), then
    /// open a new window onto it. Needs both `&mut Shell` (mint + pool) and the event
    /// loop (the window), so it defers here. The donor session is untouched. (Tear-out
    /// gestures G4.)
    ForkNode { node: uuid::Uuid, from: GraphId },
    /// Branch (Shift tear, G3): mint a `Branched` graphlet anchored on the dragged node
    /// in the donor's session graphlet index (sharing the donor's `GraphId` + kernel
    /// nodes — no copy), then open a new window scoped to it. Needs `&mut Shell` (the
    /// graphlet pool + persistence) and the event loop (the window). The donor is
    /// untouched. (Tear-out gestures G3; graphlet wiring Phase 1.)
    BranchNode { node: uuid::Uuid, from: GraphId },
    /// Grow a branch graphlet's roster (Phase 2 slice 2): the branch window navigated to
    /// `node`, so it joins the branch's lineage, diverging from the donor while sharing
    /// kernel nodes. Pushed from `sync_orrery` when the window carries a `branch_graphlet`;
    /// handled on `Shell` (the graphlet pool + persistence). (Tear-out gestures G3.)
    RecordBranchMember {
        graph: GraphId,
        graphlet: forme::GraphletId,
        node: uuid::Uuid,
    },
    /// Open the focused node's connected component as a **Linked** graphlet in a scoped
    /// window (Phase 3 slice 2, the manual Linked consumer): mint a `Linked { Component }`
    /// graphlet derived from the graph, then open a window scoped to it (reusing the
    /// branch-window scope path). Needs `&mut Shell` + the event loop, so it defers here.
    /// (Graphlet wiring Phase 3.)
    OpenLinkedGraphlet { node: uuid::Uuid, from: GraphId },
    /// Reconcile graph `graph`'s **Linked** graphlets against the (just-changed) graph and
    /// persist any that drifted (Phase 3 slice 2+ — data-level drift). Queued by
    /// `save_session` after a graph mutation; runs on `Shell` (needs `&mut graphlets`).
    /// Cheap + idempotent (a no-op when nothing drifted). The scoped windows already track
    /// drift live via `install_scope`'s re-derive; this keeps the persisted roster current.
    ReconcileGraphlets { graph: GraphId },
    /// Close window `id` and drop its view. The primary is exempt — its close saves
    /// the session and exits the app; a secondary just releases its surface. (MW3.)
    #[allow(dead_code)] // queued by the close fork once leaf windows can self-close (MW4)
    CloseWindow(WindowId),
    /// Mint a fresh session + graph and make it active. (Cmd-N.) A session op
    /// re-keys the orrery pool, which a per-window `WindowCtx` cannot do (it holds
    /// one orrery borrowed out of the pool), so it runs on `Shell` after the ctx
    /// borrow ends — like spawn/close. (Window composition P1, multi-graph.)
    CreateSession,
    /// Switch the active session to `id` (load its graph into the pool, focus it).
    SwitchSession(SessionId),
    /// Cycle to the next (`true`) / previous session in id order, wrapping.
    CycleSession(bool),
    /// Close (trash) session `id`, switching to a survivor first if it was active.
    CloseSession(SessionId),
    /// Open session `id`'s graph in a second Orrery pane beside the current one,
    /// without switching focus (the per-pane render path shows two graphs at
    /// once). (Window composition P2 — second graph-pane.)
    OpenGraphBeside(SessionId),
    /// Thaw the graph engram with this manifest-id string into a fresh ephemeral Orrery
    /// pane beside the current one, read-only. The Alembic Engrams row queues this; `Shell`
    /// thaws it off the private store after the `WindowCtx` borrow ends. (Alembic B2.)
    OpenEngramBeside(String),
}

/// A tile's cached rasterized texture: the scene version + size it was rasterized
/// at, plus the GPU texture and its view. Reused across frames while the version +
/// size hold, so an idle tile is not re-rasterized.
pub(crate) struct CachedTile {
    pub(crate) version: u64,
    pub(crate) size: (u32, u32),
    #[allow(dead_code)] // owns the texture the `view` references; kept alive here
    pub(crate) tex: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
}

/// An in-progress manual window resize (custom titlebar). winit's
/// `drag_resize_window` is inert on frameless Windows — the non-client area is
/// removed via `WM_NCCALCSIZE`, so the OS has no edge frame to grab — so the host
/// resizes the window itself: it anchors the opposite edge(s) to the press-time
/// rect and tracks the cursor in screen space (the press-time screen cursor is the
/// origin, so there is no first-move jump). On Wayland `set_outer_position` is a
/// no-op, so left/top edges there can't move the origin; right/bottom still size.
#[derive(Clone, Copy)]
pub(crate) struct ResizeDrag {
    dir: ResizeDirection,
    /// Window outer top-left (physical px) at press.
    start_outer: (i32, i32),
    /// Window inner size (physical px) at press.
    start_size: (u32, u32),
    /// Cursor position in screen space (physical px) at press.
    start_cursor_screen: (f32, f32),
}

/// The host kernel's inbox: the typed receivers each I/O actor delivers updates on,
/// all woken by the one bare `EventLoopProxy<()>`. Grouping them names the seam (the
/// kernel/actor boundary's inbound half) without collapsing the per-subsystem
/// channels into one mega enum, which would muddy ownership. The constellation's
/// per-tile content channels are drained separately (`Constellation::drain`); this
/// holds only the I/O streams.
struct KernelInbox {
    fetch: Receiver<fetch::FetchUpdate>,
    /// Find-in-page worker replies (match rects per query generation). (Find.)
    find: Receiver<find_worker::FindResult>,
    sync: Receiver<sync::SyncUpdate>,
    comms: Receiver<comms_host::CommsUpdate>,
    /// Portable diagnostics emitted through `register_diagnostics::emit`.
    diagnostics: Receiver<DiagnosticEvent>,
}


mod shell_access;
mod shell_new;
/// The shared per-user data root (`<data_dir>/mere`). Settings, the content
/// cache, and comms live directly here; per-session graph/frame/views live under
/// `<mere_root>/sessions/<session_id>/`. (Multi-graph MG1.)
fn default_mere_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mere")
}

/// The registry default context menu as owned strings (the seed when no persona curation
/// exists, and the target of "Reset to default"). (Command registry P4.)
fn default_menu_actions() -> Vec<String> {
    meerkat::command::DEFAULT_MENU_ACTIONS.iter().map(|s| s.to_string()).collect()
}

/// The default content frame: a single orrery pane filling the band, bound to the
/// active `graph_id`. Used at first launch and when no window layout is saved.
/// (Frame tree F1 / MG2; graph-bound leaf per MG5.)
fn default_content_frame(graph_id: GraphId) -> FrameLayout {
    FrameLayout {
        id: FrameId::new("content"),
        label: "content".to_string(),
        root: PaneNode::Leaf {
            pane_id: GRAPH_PANE,
            content: PaneContent::Orrery,
            graph_id,
        },
    }
}

/// Bring up the session registry under `<mere_root>/sessions/`: scan existing
/// session manifests, migrate a pre-MG1 flat graph in if one is found, or seed
/// one default session on a fresh install. Returns the registry plus the session
/// to open as active (the most-recently-updated). (Multi-graph MG1.)
fn bootstrap_sessions(mere_root: &Path) -> (ManifestStore, SessionId) {
    let sessions_root = mere_root.join("sessions");
    let mut manifests = ManifestStore::new();
    if let Err(err) = manifests.load_from_disk(&sessions_root) {
        tracing::warn!(%err, dir = ?sessions_root, "scanning sessions/ failed; starting fresh");
    }
    manifests.set_root(&sessions_root);

    // One-time migration: a flat `<mere_root>/graph.json` with no sessions/ is a
    // pre-MG1 single-session install. Mint a session and move its graph + views into
    // `sessions/<id>/`. The frame layout stays at the root (it is window-scoped per
    // MG5), and the content cache, settings, comms stay at the root too.
    let flat_graph = mere_root.join(session_graph_store::GRAPH_FILE);
    if manifests.is_empty() && flat_graph.exists() {
        let session_id = SessionId::new();
        let session_dir = sessions_root.join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let _ = std::fs::rename(
            &flat_graph,
            session_dir.join(session_graph_store::GRAPH_FILE),
        );
        let flat_views = mere_root.join(view_intent_store::VIEW_INTENT_DIR);
        if flat_views.is_dir() {
            let _ = std::fs::rename(
                &flat_views,
                session_dir.join(view_intent_store::VIEW_INTENT_DIR),
            );
        }
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir);
        manifests.insert(manifest);
        let _ = manifests.flush_dirty();
        tracing::info!(?session_id, "migrated the flat session into sessions/");
        return (manifests, session_id);
    }

    // Fresh install (or an empty sessions/): seed one default session.
    if manifests.is_empty() {
        let session_id = SessionId::new();
        let session_dir = sessions_root.join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir);
        manifests.insert(manifest);
        let _ = manifests.flush_dirty();
        return (manifests, session_id);
    }

    // Existing sessions: open the most-recently-updated one.
    let active = manifests
        .iter()
        .max_by_key(|(_, m)| m.updated_at)
        .map(|(id, _)| id)
        .expect("manifests is non-empty here");
    (manifests, active)
}


#[cfg(test)]
mod multi_graph_tests;

mod view_helpers;
pub(crate) use view_helpers::*;

fn main() {
    // The scrying compatibility tiles import a WebView2 D3D11 shared texture, which the
    // host wgpu device can only do on **D3D12** (the NT-handle interop is DX12-only). wgpu
    // on Windows otherwise picks Vulkan, where the import fails with "backend mismatch:
    // expected Dx12, found non-Dx12" and the tile stays blank — so pin the backend to DX12
    // before any wgpu instance is built. An explicit `WGPU_BACKEND` (e.g. for debugging)
    // still wins. (Scrying tile plan; scry-in-pelt.)
    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: the first statement in `main`, before any thread or wgpu instance
        // exists, so there is no concurrent environment access.
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    let (diagnostics_tx, diagnostics_rx) = mpsc::channel();
    install_global_sender(diagnostics_tx.clone());
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("meerkat=info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_layer::ApparatusTracingLayer::new(diagnostics_tx))
        .init();
    tracing::info!("meerkat-shell starting");

    let event_loop = winit::event_loop::EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = Shell::new(proxy, diagnostics_rx);
    event_loop.run_app(&mut app).expect("event loop error");
}
