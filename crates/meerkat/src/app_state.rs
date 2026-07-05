/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Core binary state types: ContentPane, SharedState, Content, Session, Presentation.

use super::*;

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
pub(crate) enum A11yHostAction {
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
pub(crate) struct SharedState {
    /// Active-node pool + the fetched-page cache that feeds it.
    pub(crate) content: Content,
    /// The session registry + the active session's identity / paths / switcher caches.
    pub(crate) session: Session,
    /// Theming + the persisted chrome settings every window's chrome renders from.
    pub(crate) presentation: Presentation,
    /// The comms actor's command handle (P6c). The actor owns the live `Comms`
    /// (misfin + murm adapters) on its own tokio runtime; conversation lists +
    /// threads arrive on `inbox.comms`, and load / send verbs are `CommsCommand`s.
    pub(crate) comms_handle: armillary::ActorHandle<comms_host::CommsCommand>,
    /// The p2p sync actor's command handle (S5.0 / S5.1). The actor owns the
    /// transport + tessera lane on its own tokio runtime; status arrives on
    /// `inbox.sync`, and the "connect to peer" verb is a `SyncCommand`.
    pub(crate) sync_handle: armillary::ActorHandle<sync::SyncCommand>,
    /// The kernel inbox: the typed receivers the I/O actors deliver on, behind the
    /// one winit wake. `user_event` is the single documented place that reads them.
    pub(crate) inbox: KernelInbox,
    /// Bounded observation cache backing the Apparatus diagnostics pane.
    pub(crate) observability: HostObservability,
}

/// The `content` subsystem: the active-node pool and the page-content cache that
/// backs it. Shared across windows — one activation lifecycle, one cache.
pub(crate) struct Content {
    /// The constellation: the pool of active nodes (their content actors). The
    /// focused card (Cartography) and the workbench tiles (Tree) both draw their
    /// scenes from here — one activation lifecycle, not two. Reconciled to the
    /// needed set each frame; backgrounded nodes outlive the view.
    pub(crate) constellation: Constellation,
    /// Per-URL fetched content state, keyed by the node's URL (URL identity).
    pub(crate) pages: HashMap<String, fetch::ContentState>,
    /// Durable content cache (S3.2c) under the session dir, persisting fetched
    /// pages + subresources by URL. `None` if the store could not be opened
    /// (caching disabled; the shell still runs).
    pub(crate) store: Option<FjallStore>,
    /// The fetch actor's command handle (the kernel commands it over this; its
    /// outcomes arrive on `inbox.fetch`).
    pub(crate) fetch_handle: armillary::ActorHandle<fetch::FetchCommand>,
    /// The find-in-page worker's command handle: the kernel ships it the focused page +
    /// query off the UI thread, and its match rects arrive on `inbox.find`. (Find.)
    pub(crate) find_worker: armillary::ActorHandle<find_worker::FindCommand>,
    /// The nematic engine registry, for rendering "last visit" snapshot cards
    /// host-side from the durable content cache (no actor). (Card #4.)
    pub(crate) engine_registry: EngineRegistry,
    /// Per-node engine pins (member → engine id). The compatibility view is a pin
    /// to `scrying.web`; the picker (engine-picker plan) writes other ids here.
    /// Session state, shared across windows: the pin is the *intent*; for a
    /// surface-engine pin, each window's per-`WindowView` producer pool spawns the
    /// HWND-bound WebView that serves it. A torn-out compat tile (MW4) carries the
    /// pin, the recipient spawns a fresh WebView. (Replaces the `compat_pins` bool;
    /// engine-picker Phase 0. The durable per-node graph field takes over later.)
    pub(crate) engine_pins: HashMap<GraphMemberId, String>,
    /// The engine routing policy: scheme / content-type / per-host / pin → engine
    /// id. Consulted at nav time (scheme + pin) to choose the tier (surface engine
    /// vs the document/constellation lane); the document-engine re-route by
    /// content-type is the actor's second pass. (engine-picker Phase 0.)
    pub(crate) route_policy: inker::routing::EngineRoutePolicy,
    /// Which present engines are active this session (global default from settings +
    /// per-session overrides). `engine_available` gates routing on this, so a
    /// deactivated engine is never picked and spawns no actors. (engine-picker Phase 1.)
    pub(crate) engine_activation: engine_activation::EngineActivation,
    /// The crawl actor's owner (relational-browse V2): a `>crawl` on a focused page
    /// seeds a bounded crawl whose harvested link + metadata contributions drain back
    /// each frame and apply to the focused graph. One crawl at a time.
    pub(crate) crawl: crawl::CrawlSession,
    /// The browse-capture consent level (C1 enforcement point; consent layer C4):
    /// whether the live trail recorder writes a `BrowsingTrace` per navigation, and
    /// at what granularity. Persisted in `settings.json` (`capture_consent`), changed
    /// at runtime by `>capture off|corridor|full`. Recorded traces are always
    /// `LocalOnly` — sharing is a separate, explicit act.
    pub(crate) capture_consent: crate::browse_capture::CaptureConsent,
}

/// The `session` subsystem: the session registry plus the active session's
/// identity, on-disk paths, and switcher caches.
pub(crate) struct Session {
    /// The on-disk session registry, loaded from `<mere_root>/sessions/`. (MG1.)
    pub(crate) manifests: ManifestStore,
    /// The session whose graph + frame + views are loaded right now; its dir is
    /// `session_dir`. (Multi-graph MG1.)
    pub(crate) active_session_id: SessionId,
    /// The active session's persona — the identity boundary its persona-scoped data
    /// (engine UDFs, the configurable menu, future vaults) is filed under. v0 has one
    /// default persona; threading the manifest's id keeps the wiring persona-ready.
    pub(crate) active_persona: session_runtime::PersonaId,
    /// The active session's per-session data dir (`<mere_root>/sessions/<id>/`):
    /// holds `graph.json`, `frame.json`, and the `views/` sidecars. (Multi-graph.)
    pub(crate) session_dir: PathBuf,
    /// The shared per-user data root (`<data_dir>/mere`): settings, the content
    /// cache, and comms live here, above the per-session dirs. (Multi-graph MG1.)
    pub(crate) mere_root: PathBuf,
    /// Cached display label per session — the user's display name, else one derived
    /// from the graph. Rebuilt on session/graph change by `refresh_session_labels`;
    /// read by the toolbar session chips. (The switcher thumbnails this once sat beside
    /// are retired — sessions are toolbar chips now. Chrome bar P4 cleanup.)
    pub(crate) session_labels: HashMap<SessionId, String>,
    /// Cached mini-graph thumbnail per session as a PNG data URI, painted beside the
    /// labels by `refresh_session_labels` (live orrery or cold sidecar positions) and
    /// shown in the toolbar chips. Event-driven, never per frame. (ui_polish S1.)
    pub(crate) session_thumbs: HashMap<SessionId, String>,
    /// Host text shaping for host-drawn labels (the switcher tile names). Holds the
    /// parley contexts so they aren't rebuilt per frame. (Host text path.)
    pub(crate) host_text: text::HostText,
    /// This persona's app-launch counter, incremented and persisted once at boot
    /// (`PersonaSettings::session_count`). Pushed into every pooled orrery via
    /// `Orrery::set_current_session` so in-place navigation stamps
    /// `last_session_visited`; `run_forgetting_pass` reads it back for
    /// `EvictionPolicy::KeepSessions`. (Alembic B5 — by-sessions eviction.)
    pub(crate) current_session_count: u64,
    /// The current host launch's structural graph-delta log, if capture is enabled
    /// via `MERE_GRAPH_DELTA_LOG`. One writer for the shell session, surfaced in
    /// Apparatus and wired into the kernel's single `apply_graph_delta` funnel.
    pub(crate) graph_delta_log: crate::graph_delta_log::GraphDeltaLog,
    /// The per-session cap, in bytes, on thumbnail bytes the idle-cadence snapshot
    /// refresh will write before it stops (`settings.json`'s `snapshot_byte_cap_mb`,
    /// resolved to bytes at boot; a host default when unset). Boundary-triggered
    /// deposits ignore this — only the idle pass checks it. (Node/card summoning
    /// design, §5 item 4.)
    pub(crate) thumbnail_byte_cap: usize,
    /// Running total of thumbnail bytes deposited this session, across every deposit
    /// path (boundary, on-demand card render, and idle refresh alike) — the idle
    /// pass reads this against `thumbnail_byte_cap` before running again. Resets to
    /// 0 each launch. (Node/card summoning design, §5 item 4.)
    pub(crate) thumbnail_bytes_this_session: usize,
}

/// The `presentation` subsystem: the resolved theme + the persisted chrome
/// settings every window's chrome renders from.
pub(crate) struct Presentation {
    /// The theme registry, kept so the apparatus pane can switch themes at runtime
    /// (re-resolve → rebuild the chrome sheet + tokens). (Theme switcher.)
    pub(crate) theme: ThemeRegistry,
    /// The active theme's chrome tokens — kept beside the baked `chrome_sheet` for
    /// the host-drawn surfaces the CSS can't reach (the window-control glyphs).
    pub(crate) chrome_theme: ChromeTheme,
    /// The active theme's chrome CSS (built from a resolved [`ChromeTheme`] at
    /// startup). The render / measure / hit-test paths read it instead of a const,
    /// so a theme switch rebuilds it and the whole shell re-themes. (Theming pass.)
    pub(crate) chrome_sheet: Vec<String>,
    /// The active theme's id (e.g. `theme:dark`), persisted in settings.
    pub(crate) active_theme_id: String,
    /// The active-tab cap last written to the settings sidecar. Guards the persist
    /// path so an unchanged value isn't re-written on every chrome click.
    pub(crate) saved_tab_cap: usize,
    /// Which window edge the shellbar is docked to. Persisted in settings.json.
    pub(crate) shellbar_edge: session_runtime::ShellbarEdge,
    /// Whether the shellbar is hidden (the user's explicit hide toggle, distinct from a
    /// leaf window's slim chrome). Persisted in settings.json; revealed from the palette /
    /// `>shellbar`. (Hide-shellbar.)
    pub(crate) shellbar_hidden: bool,
    /// Linear damping for orrery node bodies — the "inertia" physics setting,
    /// adjusted in the apparatus pane and persisted. The host owns the value and
    /// pushes it to each orrery via `set_physics_damping`. (Physics settings.)
    pub(crate) physics_damping: f32,
    /// The user's chrome zoom multiplier (Ctrl +/-/0), persisted. Composed with the
    /// display's [`dpi_scale`](Self::dpi_scale) into the effective
    /// [`ui_scale`](Self::ui_scale). Default 1.1, the baseline "a point or two larger"
    /// bump. Shared across this session's windows. (UI scale.)
    pub(crate) user_zoom: f32,
    /// The DPI factor the shared `chrome_sheet` is currently **baked at**. The
    /// authoritative per-window dpi lives on `WindowView::dpi_scale` (D3, multi-monitor);
    /// each window's render re-bakes this sheet to its own dpi when they differ. Folded
    /// with `user_zoom` into `ui_scale`. 1.0 at 100%. (Auto-DPI D1 → D3.)
    pub(crate) dpi_scale: f32,
    /// The active theme's document-lane palette (content cards: smolweb /
    /// markdown / feed text). Threaded into content actors so baked glyph colors
    /// follow the theme; also read by the host for rule / image colors at lower
    /// time. Rebuilt on theme switch. (Document theming, P3.)
    pub(crate) document_palette: document_canvas::ColorVocabulary,
    /// The user's document **typography** (base size, line spacing, fonts, link
    /// adornment). Composed with `document_palette` into the sheet the content
    /// actors lay out with; edited in the `pelt/reading` page and persisted.
    /// Its own `colors` field is ignored (the palette overwrites it at compose
    /// time). (Document typography surface.)
    pub(crate) document_sheet: document_canvas::DocumentStyleSheet,
    /// The persona-curated context-menu command list (command registry P4): the registry ids
    /// shown in the right-click menu, in order. Loaded from the persona settings store at boot
    /// (or the registry default when unset), persisted on change; the menu builder resolves +
    /// applicability-filters each id for the current selection.
    pub(crate) menu_actions: Vec<String>,
    /// How many times each registry command has run — the frequency behind the context menu's
    /// auto-suggestions (command registry S3). Keyed by registry id; loaded from / persisted to
    /// the persona settings store, incremented at the command-invocation hook.
    pub(crate) command_usage: std::collections::BTreeMap<String, u32>,
    /// The short-term memory eviction policy (the Alembic Recent header). Loaded from the persona
    /// settings store at boot, cycled by the header control, persisted on change; read by the
    /// Recent-header display and `run_forgetting_pass`. (Editable eviction policy, B4.)
    pub(crate) eviction_policy: session_runtime::memory_levels::EvictionPolicy,
    /// The Engrams two-select compose gesture's pending first pick (an engram id), or `None`
    /// between gestures. Ephemeral interaction state, not persisted — a fresh launch always
    /// starts with no pending selection. (Alembic B7-P3.)
    pub(crate) pending_compose_engram: Option<String>,
    /// Whether the idle-cadence pass redeposits open workbench-tile thumbnails while the
    /// app sits idle. Loaded from `settings.json` at boot; toggled by a future settings
    /// control (none exists yet — hand-edit the sidecar to turn it off, like
    /// `retention_keep_n`). Read by `maybe_run_idle_snapshot_refresh`. (Node/card summoning
    /// design, §5 item 4.)
    pub(crate) snapshot_idle_refresh: bool,
}

impl Presentation {
    /// The active theme's chrome CSS as `&[&str]`, the shape the serval layout /
    /// paint / hit-test entry points take. Borrows the baked `chrome_sheet`. A read
    /// of shared presentation state, so it lives on the subsystem that owns it; every
    /// window's chrome renders from the same sheet. (MW2 (c).)
    pub(crate) fn chrome_sheet_refs(&self) -> Vec<&str> {
        self.chrome_sheet.iter().map(String::as_str).collect()
    }

    /// The effective chrome scale: the display's DPI factor times the user's zoom,
    /// clamped to a sane band. Now that the window is sized in **logical** px (so a 2×
    /// display gives a 2×-physical window), folding `scale_factor` in makes the chrome
    /// fill it at the right density instead of overflowing a physically-small window
    /// (the earlier-attempt bug). (Auto-DPI D1.)
    pub(crate) fn ui_scale(&self) -> f32 {
        (self.dpi_scale * self.user_zoom).clamp(0.5, 4.0)
    }

    /// Rebuild the chrome sheet from the active theme tokens at the current
    /// [`ui_scale`](Self::ui_scale), re-adding the syntax-highlight rules. Called
    /// after a zoom (Ctrl +/-/0) or a display DPI change; the theme switcher has its
    /// own rebuild in `theme_edit`. (UI scale.)
    pub(crate) fn rebuild_chrome_sheet(&mut self) {
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
    pub(crate) fn document_sheet_composed(&self) -> document_canvas::DocumentStyleSheet {
        document_canvas::DocumentStyleSheet {
            colors: self.document_palette,
            ..self.document_sheet.clone()
        }
    }
}
