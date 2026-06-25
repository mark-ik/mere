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

/// Build the chrome root's author CSS from a resolved [`ChromeTheme`] (theming
/// pass). The toolbar is a flex row (back / forward buttons + a growing omnibar)
/// that serval lays out via taffy's flexbox; the `.chrome` container itself has
/// no background, so the host composites it over the content root and only the
/// toolbar + the (opaque) dropdowns paint over the page. The toolbar band sits a
/// step above the graph backdrop, fields/buttons a step above that, dropdowns +
/// floated panels their own tiers; all colors come from the active theme so the
/// chrome theme-switches alongside the graph. Surfaces, not classes, are the
/// unit: the toolbar, palette, settings, and comms pane reuse the same dozen
/// tokens, so a theme reads as one coherent shell.
fn chrome_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div, button, input { display: block; }".to_string(),
        // Shell z-stack: the chrome is the top layer. Making `.chrome` a stacking context
        // (position + z-index) above the orrery means every chrome surface — toolbar,
        // omnibar dropdown, context menu, palette, find, settings, shellbar — paints over
        // the orrery's node/content cards and wins their hit-test, whether it is in normal
        // flow or positioned. Without it, the node cards (each its own stacking context via
        // `transform`) paint above normal-flow chrome content (the omnibar dropdown was the
        // symptom). The orrery sits at the base layer (z-index:0, below). (Shell z-stack.)
        ".chrome { position: relative; z-index: 10; }".to_string(),
        // The toolbar reserves right padding the width of the window-control strip
        // (the borderless titlebar's min / max / close), so the omnibar + sync chip
        // stop short of it and the host composites the controls into that gap.
        format!(
            ".toolbar {{ display: flex; background-color: {}; padding: 8px {}px 8px 8px; }}",
            rgb(c.toolbar_bg),
            titlebar::CONTROLS_W as u32
        ),
        format!(
            "button {{ font-size: 22px; color: {}; background-color: {}; padding: 8px 14px; margin: 4px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".disabled {{ color: {}; background-color: {}; }}",
            rgb(c.disabled_text),
            rgb(c.disabled_bg)
        ),
        format!(
            "input {{ font-size: 22px; color: {}; background-color: {}; padding: 8px; margin: 4px; flex-grow: 1; }}",
            rgb(c.field_text),
            rgb(c.field_bg)
        ),
        // The p2p sync chip: small + muted, no flex-grow, so the omnibar pushes it
        // to the toolbar's right edge.
        format!(
            ".sync-chip {{ font-size: 14px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px; border-radius: 14px; }}",
            rgb(c.muted_text),
            rgb(c.menu_bg)
        ),
        // The crawl-progress chip (relational-browse V2), matching the sync chip's pill;
        // hidden when empty (no crawl has run) via `:empty`.
        format!(
            ".crawl-chip {{ font-size: 14px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px; border-radius: 14px; }} .crawl-chip-hidden {{ display: none; }}",
            rgb(c.muted_text),
            rgb(c.menu_bg)
        ),
        // The add pill — the primary create affordance, an accent pill (matching the
        // sync-chip's pill shape) with a "+" that opens the Add node/tile/session menu.
        format!(
            ".add-pill {{ font-size: 18px; color: {}; background-color: {}; padding: 6px 16px; margin: 4px; border-radius: 14px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        format!(
            ".suggestions {{ background-color: {}; padding-bottom: 6px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".suggestion {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 16px; }}",
            rgb(c.body_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".suggestion-active {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 16px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // Command palette: a centered panel floated over the page (flex centering;
        // serval maps justify-content through stylo_taffy).
        ".palette-overlay { display: flex; justify-content: center; padding-top: 56px; }"
            .to_string(),
        format!(
            ".palette {{ width: 540px; background-color: {}; padding: 10px; }}",
            rgb(c.surface_bg)
        ),
        // The command list scrolls (the host bounds its height per-window inline
        // and offsets it to follow the selection, so a long list stays reachable in
        // a small window).
        format!(".cmd-list {{ overflow: scroll; background-color: {}; }}", rgb(c.surface_bg)),
        format!(
            ".cmd-row {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
        format!(
            ".cmd-row-active {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // Find-in-page bar: docked top-right under the toolbar (flex end), a small
        // panel with a label + the query field.
        ".find-overlay { display: flex; justify-content: flex-end; padding-top: 56px; padding-right: 12px; }"
            .to_string(),
        format!(
            ".find-bar {{ display: flex; align-items: center; background-color: {}; padding: 6px 10px; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".find-label {{ font-size: 16px; color: {}; padding: 6px 8px; }}",
            rgb(c.body_text)
        ),
        format!(
            ".find-count {{ font-size: 14px; color: {}; padding: 6px 8px; }}",
            rgb(c.muted_text)
        ),
        // Right-click context menu: a small panel of action rows floated at the cursor.
        format!(
            ".context-menu {{ background-color: {}; padding: 4px; }}",
            rgb(c.menu_bg)
        ),
        format!(
            ".context-item {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.body_text),
            rgb(c.menu_bg)
        ),
        // The keyboard-highlighted context-menu row (arrow nav), matching the palette's
        // `cmd-row-active` accent.
        format!(
            ".context-item-active {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The context menu's search field (the cursor palette): an input-styled row at the top.
        // `-empty` dims the placeholder. Typing edits it (the menu owns the keyboard).
        format!(
            ".context-search {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.field_text),
            rgb(c.field_bg)
        ),
        format!(
            ".context-search-empty {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.muted_text),
            rgb(c.field_bg)
        ),
        // The pin toggle on a search-result row: "+" to pin (muted), "✓" once pinned (accent).
        format!(
            ".context-pin {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.muted_text),
            rgb(c.menu_bg)
        ),
        format!(
            ".context-pin-on {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // Comms pane: an absolutely-positioned panel whose geometry the host sets
        // inline each frame from the Comms frame leaf's rect (so it splits beside
        // the orrery like the other panes, rather than floating docked).
        format!(
            ".comms-pane {{ position: absolute; background-color: {}; padding: 10px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-title {{ display: flex; background-color: {}; padding: 4px 4px 10px 4px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-title-text {{ font-size: 20px; color: {}; background-color: {}; flex-grow: 1; padding: 4px 8px; }}",
            rgb(c.strong_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-btn {{ font-size: 18px; color: {}; background-color: {}; padding: 4px 12px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".comms-failure {{ font-size: 14px; color: {}; background-color: {}; padding: 6px 10px; margin-bottom: 6px; }}",
            rgb(c.error_text),
            rgb(c.error_bg)
        ),
        format!(
            ".comms-row {{ font-size: 17px; color: {}; background-color: {}; padding: 10px 12px; margin: 3px 0; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
        format!(
            ".comms-empty {{ font-size: 15px; color: {}; background-color: {}; padding: 10px 12px; }}",
            rgb(c.muted_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-back {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 12px; margin-bottom: 6px; }}",
            rgb(c.body_text),
            rgb(c.control_bg)
        ),
        format!(
            ".comms-thread-title {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 4px; }}",
            rgb(c.strong_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-msg-in {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 24px 4px 0; }}",
            rgb(c.body_text),
            rgb(c.menu_bg)
        ),
        format!(
            ".comms-msg-out {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 0 4px 24px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        format!(
            ".comms-compose {{ display: flex; background-color: {}; padding-top: 8px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-send {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 16px; margin: 4px; }}",
            rgb(c.control_text),
            rgb(c.active_bg)
        ),
        format!(
            ".comms-status {{ font-size: 14px; color: {}; background-color: {}; padding: 6px 12px; }}",
            rgb(c.muted_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-new-btn {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 0; }}",
            rgb(c.control_text),
            rgb(c.active_bg)
        ),
        format!(".comms-new {{ background-color: {}; }}", rgb(c.panel_bg)),
        format!(
            ".comms-proto-row {{ display: flex; background-color: {}; padding: 4px 0; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-proto {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 14px; margin: 0 6px 0 0; }}",
            rgb(c.body_text),
            rgb(c.control_bg)
        ),
        format!(
            ".comms-proto-active {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 14px; margin: 0 6px 0 0; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        format!(
            ".comms-new-to {{ background-color: {}; padding-top: 2px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-new-body {{ display: flex; background-color: {}; padding-top: 2px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-field-label {{ font-size: 13px; color: {}; background-color: {}; padding: 8px 4px 2px 4px; }}",
            rgb(c.muted_text),
            rgb(c.panel_bg)
        ),
        // Shellbar: an absolutely-positioned chrome strip whose geometry the host
        // sets inline each frame from the shellbar_rect() helper. Contains toggle
        // buttons for each pane (F2.1).
        format!(
            ".shellbar {{ position: absolute; background-color: {}; display: flex; \
                align-items: center; justify-content: flex-start; }}",
            rgb(c.toolbar_bg)
        ),
        // Uniform square buttons: every glyph occupies an identical 44x44 box (the
        // strip is 48px thick) and is flex-centred within it, so differing glyph
        // widths read even instead of ragged. The two rules differ only in colour.
        format!(
            ".shellbar-btn {{ display: flex; align-items: center; justify-content: center; \
                width: 44px; height: 44px; font-size: 17px; padding: 0; margin: 2px 0; \
                color: {}; background-color: {}; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".shellbar-btn-active {{ display: flex; align-items: center; justify-content: center; \
                width: 44px; height: 44px; font-size: 17px; padding: 0; margin: 2px 0; \
                color: {}; background-color: {}; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
    ]
}

/// Build the pelt tile-surface theme sheet from the resolved [`ChromeTheme`], so the
/// workbench tiles read as the same shell as the chrome. The surface layers this over
/// its structural default CSS (`TileSurface::set_theme`), so it only restates colors,
/// not layout. Roles mirror [`chrome_sheet`] and the Strophos shared-palette model
/// (woodshed's `audio-widgets::theme` — surface ladder + selection fill + text
/// weights): the tab bar is the toolbar band, an inactive tab a control surface, the
/// active tab the selection fill, the content area the band tone (matching [`CARD_BG`]
/// so the gap below a short card is seamless, replacing pelt's white page default),
/// and the gutter a darkened seam.
fn tile_sheet(c: &ChromeTheme) -> String {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    // A darker step off a token, for the slot gutter (no "darkest" theme token; the
    // chrome's frame dividers use a near-black seam, so halve the band tone here).
    let darken = |color: Color32, f: f32| {
        let [r, g, b, _] = color.to_array();
        let s = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
        format!("rgb({}, {}, {})", s(r), s(g), s(b))
    };
    // A glyph/text resting on a fill picks near-white or near-black by WCAG
    // contrast, so it stays legible whatever hue the fill becomes. The active
    // tab's close × sits on the (theme-accented) active-tab background, so it is
    // contrast-picked rather than a fixed muted tone. (Readable-on-accent.)
    let on = |bg: Color32| {
        let [r, g, b, _] = bg.to_array();
        let o = tincture::best_on(tincture::Srgb::rgb(r, g, b));
        format!("rgb({}, {}, {})", o.r, o.g, o.b)
    };
    format!(
        ".tile-tabbar {{ background: {tabbar}; }} \
         .tile-tab {{ color: {tab_text}; background: {tab_bg}; }} \
         .tile-tab.active {{ color: {active_text}; background: {active_bg}; }} \
         .tile-close {{ color: {close}; }} \
         .tile-tab.active .tile-close {{ color: {active_close}; }} \
         .tile-content {{ background: {content}; }} \
         .tile-divider {{ background: {divider}; }} \
         .tile-ghost {{ color: {active_text}; background: {active_bg}; border: 1px solid {ghost_border}; }}",
        tabbar = rgb(c.toolbar_bg),
        tab_text = rgb(c.muted_text),
        tab_bg = rgb(c.control_bg),
        active_text = rgb(c.strong_text),
        active_bg = rgb(c.active_bg),
        close = rgb(c.muted_text),
        active_close = on(c.active_bg),
        content = rgb(c.toolbar_bg),
        divider = darken(c.toolbar_bg, 0.5),
        ghost_border = rgb(c.muted_text),
    )
}

/// Fallback chrome-band height (px) if the toolbar can't be measured.
const FALLBACK_TOOLBAR_H: u32 = 64;

/// Background of the floating content card — a panel a step above the orrery
/// backdrop, so the card reads as a raised surface over the dark orrery band.
const CARD_BG: wgpu::Color = wgpu::Color {
    r: 0.110,
    g: 0.122,
    b: 0.145,
    a: 1.0,
};

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
}

impl Presentation {
    /// The active theme's chrome CSS as `&[&str]`, the shape the serval layout /
    /// paint / hit-test entry points take. Borrows the baked `chrome_sheet`. A read
    /// of shared presentation state, so it lives on the subsystem that owns it; every
    /// window's chrome renders from the same sheet. (MW2 (c).)
    fn chrome_sheet_refs(&self) -> Vec<&str> {
        self.chrome_sheet.iter().map(String::as_str).collect()
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

impl Shell {
    fn new(
        proxy: winit::event_loop::EventLoopProxy<()>,
        diagnostics_rx: Receiver<DiagnosticEvent>,
    ) -> Self {
        Self::new_with_session_dir(proxy, diagnostics_rx, default_mere_root())
    }

    fn new_with_session_dir(
        proxy: winit::event_loop::EventLoopProxy<()>,
        diagnostics_rx: Receiver<DiagnosticEvent>,
        mere_root: PathBuf,
    ) -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        // Shared per-user root (`<data_dir>/mere`): settings, the content cache, and
        // comms live here; per-session graph/frame/views live under sessions/<id>/.
        let _ = std::fs::create_dir_all(&mere_root);
        // Bring up the session registry: scan sessions/, migrate a pre-MG1 flat
        // graph in, or seed one default session. The active session's dir is where
        // the graph + frame + views load from. (Multi-graph MG1.)
        let (manifests, active_session_id) = bootstrap_sessions(&mere_root);
        let session_dir = mere_root
            .join("sessions")
            .join(active_session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        // Restore persisted settings (active-tab cap, theme, shellbar edge) from the
        // shared root so they apply across sessions, not per-graph.
        let saved_settings = settings_store::load_settings(&mere_root)
            .ok()
            .flatten()
            .unwrap_or_default();
        // The active session's persona (v0: the single default persona) — the boundary its
        // persona-scoped files are filed under. Resolved from the active manifest so the wiring
        // is persona-ready when personas become user-managed.
        let active_persona = manifests
            .get(active_session_id)
            .map(|m| m.persona_id)
            .unwrap_or_else(session_runtime::PersonaId::default_persona);
        // The persona's UI settings (command registry P4/S3): the curated context menu + the
        // command-usage frequencies behind auto-suggest. Loaded before `mere_root` is moved into
        // the session struct below.
        let persona_ui = session_runtime::load_persona_settings(&mere_root, active_persona)
            .ok()
            .flatten()
            .unwrap_or_default();
        let menu_actions = persona_ui.menu_actions.unwrap_or_else(default_menu_actions);
        let command_usage = persona_ui.command_usage;
        let mut chrome = Chrome::new("mere://welcome");
        chrome.settings.tab_cap = saved_settings.tab_cap;
        let runner = window_view::shell_runner(dom.clone(), chrome);
        let content_location = runner.state().chrome.content_location().to_string();
        // Durable content cache (S3.2c), shared (persona-scoped) under the mere root
        // so sessions don't re-fetch each other's pages; `None` disables caching.
        let store = match FjallStore::open(mere_root.join("content")) {
            Ok(mut store) => {
                // Restore this persona's persisted HTTP session, so a login survives
                // an app restart (native session store; durability thread).
                fetch::load_cookies(&mut store, active_persona);
                Some(store)
            }
            Err(err) => {
                tracing::warn!(%err, "content cache unavailable; running without it");
                None
            }
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        let restored = match session_graph_store::load(&graph_file) {
            Ok(Some(graph)) => {
                tracing::info!(path = ?graph_file, "restored the session graph");
                Some(graph)
            }
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(%err, path = ?graph_file, "session graph load failed; starting fresh");
                None
            }
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
            }
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
            let (yaw, tilt) = snapshot_yaw_tilt(snapshot);
            orrery.set_yaw(yaw);
            orrery.set_tilt(tilt);
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            orrery.select_by_url(url);
        }
        // Restore the orrery pane's layout strategy at boot (None = force-directed),
        // recomputed on the first frame from the node set. (Layout picker.)
        orrery.set_layout_strategy(restored_view.as_ref().and_then(|v| v.strategy.clone()));
        // Always-offload physics (P6): move the orrery's gyre simulation onto its
        // own armillary actor thread, so a heavy settle never blocks compositing or
        // input. It wakes the loop through the same winit proxy as the other
        // actors; the host folds each layout snapshot into the orrery's read model
        // on the next frame.
        let physics_proxy = proxy.clone();
        let physics_wake: armillary::Wake = Arc::new(move || {
            let _ = physics_proxy.send_event(());
        });
        orrery.offload_physics(physics_wake.clone());
        // The fetch actor wakes the loop through the winit proxy; armillary takes
        // the wake as a host-neutral callback.
        let fetch_proxy = proxy.clone();
        let fetch_wake: armillary::Wake = Arc::new(move || {
            let _ = fetch_proxy.send_event(());
        });
        let (fetch_handle, fetch_rx) = fetch::spawn_fetcher(fetch_wake);
        // The find-in-page worker lays out the focused page off the UI thread (a full
        // serval layout costs ~1-2s, far too slow per keystroke) and ships back match
        // rects, woken through the same proxy.
        let find_proxy = proxy.clone();
        let find_wake: armillary::Wake = Arc::new(move || {
            let _ = find_proxy.send_event(());
        });
        let (find_worker, find_rx) = find_worker::spawn_find_worker(find_wake);
        // The content actor renders the focused card off the UI thread (it owns the
        // serval cascade + nematic engines + a per-tile subresource cache on its own
        // thread) and ships scenes / wanted subresources / harvested linked data
        // back through the same wake.
        let content_proxy = proxy.clone();
        let content_wake: armillary::Wake = Arc::new(move || {
            let _ = content_proxy.send_event(());
        });
        // The crawl actor shares the content wake: its updates schedule the same frame
        // drain that picks up content-actor updates. (Relational-browse V2.)
        let mut crawl = crawl::CrawlSession::new(content_wake.clone());
        // Restore the crawl scope / depth the settings lane last persisted.
        if let Some(scope) =
            saved_settings.crawl_scope.as_deref().and_then(crawl::HostScope::from_key)
        {
            crawl.set_scope(scope);
        }
        if let Some(depth) = saved_settings.crawl_depth {
            crawl.set_max_depth(depth);
        }
        if let Some(whole_site) = saved_settings.crawl_sitemap {
            crawl.set_seed_sitemap(whole_site);
        }
        if let Some(pages) = saved_settings.crawl_max_pages {
            crawl.set_max_pages(pages);
        }
        let mut constellation = Constellation::new(content_wake);
        constellation.set_cap(saved_settings.tab_cap);
        // Seed the actor pool's deactivated-engine set so a globally-disabled
        // document engine renders the fallback off-thread too. (engine-picker Phase 1b.)
        constellation.set_disabled_engines(saved_settings.disabled_engines.iter().cloned().collect());
        // Seed the installed DocumentScript origin bindings (§11.4 follow-on #2):
        // resolved from `script-bindings.json` (user form) + installed mod manifests
        // under `<mere_root>/mods/` ("installed extension" form), both against the
        // session script-permissions, so a fresh navigation to a bound origin
        // auto-attaches its script (the App-default Allow narrowed by any
        // session-scope opinion). User bindings take precedence on origin overlap
        // (first match wins in `binding_for`), so they lead the merged list.
        let mut script_bindings =
            crate::content::script::load_resolved_bindings(&mere_root, &saved_settings.script_permissions);
        script_bindings.extend(crate::content::script::load_mod_bindings(
            &mere_root,
            &saved_settings.script_permissions,
        ));
        constellation.set_script_bindings(script_bindings);
        // The p2p sync actor: an armillary actor whose run closure owns a tokio
        // runtime (built on its thread) that binds the transport + joins the tessera
        // demo moot, polling status back through the same wake shape as fetch/content.
        // Setup failure disables p2p, not the shell.
        let sync_proxy = proxy.clone();
        let sync_wake: armillary::Wake = Arc::new(move || {
            let _ = sync_proxy.send_event(());
        });
        let (sync_handle, sync_rx) = sync::spawn_sync(sync_wake, sync::DEMO_MOOT);
        // The comms actor: owns the live `Comms` (misfin + murm adapters over local
        // stores under the session dir) on its own tokio runtime, waking the loop
        // through the same winit proxy. Setup failure disables comms, not the shell.
        let comms_proxy = proxy.clone();
        let comms_wake: armillary::Wake = Arc::new(move || {
            let _ = comms_proxy.send_event(());
        });
        let (comms_handle, comms_rx) = comms_host::spawn_comms(comms_wake, mere_root.clone());
        // The host's own nematic engine registry, for rendering snapshot cards
        // from the durable cache without a live actor (Card #4).
        let mut engine_registry = EngineRegistry::new();
        for engine in nematic::engines() {
            engine_registry.register(engine);
        }
        // Resolve the active theme's chrome tokens once and bake the chrome CSS
        // from them (theming pass). A runtime theme switch (settings / apparatus)
        // rebuilds this from the registry; today it opens on the default theme.
        let mut theme = ThemeRegistry::default();
        // Load user / mod theme files (`<mere_root>/themes/*.json`) so a saved
        // active user theme resolves and they appear in the picker. A malformed
        // file is skipped + logged, never fatal. (Seed-palette themes T3/T4.)
        for def in theme_store::load_user_themes(&mere_root) {
            let id = def.id.clone();
            if let Err(e) = theme.add_user_theme(def) {
                tracing::warn!(theme = %id, error = %e, "skipping invalid user theme");
            }
        }
        // Honor the saved theme (falls back to the registry default), and keep the
        // registry so the apparatus pane can switch at runtime. (Theme switcher.)
        let active_theme_id = saved_settings
            .theme_id
            .clone()
            .unwrap_or_else(|| theme.active_theme().resolved_id);
        let resolution = theme.set_active_theme(&active_theme_id);
        let active_theme_id = resolution.resolved_id;
        let chrome_theme = resolution.tokens.chrome;
        let chrome_sheet = chrome_sheet(&chrome_theme);
        // Theme the orrery's backdrop + edges from the same resolved theme. (A2.)
        let (orrery_backdrop, orrery_edge) = orrery_palette(&resolution.tokens);
        orrery.set_palette(orrery_backdrop, orrery_edge);
        // The document-lane palette for content cards, from the same theme. (P3.)
        let document_palette = document_palette(&resolution.tokens);
        // The user's persisted document typography (embedded JSON in settings),
        // or the built-in look. Composed with the palette per render. (Typography.)
        let document_sheet = saved_settings
            .document_typography
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        // The content region opens as a single graph pane (orrery / tiled
        // workbench); summoning the roster splits it. (Frame tree, F1.)
        let active_graph = manifests
            .get(active_session_id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        let mut frame_layout = default_content_frame(active_graph);
        // The frame is **window-scoped** (Model B, MG5): load it from the shared root,
        // not the session. A pre-MG5 install saved it per-session, so carry the active
        // session's layout up once if the shared one is absent.
        let mut next_pane_id = 1u64;
        let restored_frame = frame_layout_store::load_frame_layout(&mere_root)
            .ok()
            .flatten()
            .or_else(|| {
                frame_layout_store::load_frame_layout(&session_dir)
                    .ok()
                    .flatten()
            });
        if let Some(mut restored) = restored_frame {
            // Keep the restored layout only if it carries the graph (Orrery) pane;
            // a pre-coexistence layout (graph pane saved as Workbench) is stale, so
            // fall back to the default single Orrery pane. (Workbench-as-pane.)
            if restored
                .iter_leaves()
                .any(|(_, c, _)| matches!(c, PaneContent::Orrery))
            {
                next_pane_id = restored
                    .iter_leaves()
                    .map(|(id, _, _)| id.0)
                    .max()
                    .unwrap_or(0)
                    + 1;
                // Reattach only the graph-bound leaves whose graph_id is nil / stale
                // (no live session) to the active graph; a leaf pinned to a *valid*
                // graph (a second graph-pane restored from a prior run) stays put, so
                // it reloads instead of being clobbered onto the active graph. (MG5;
                // pane-as-unit restore.)
                let valid_graphs: HashSet<GraphId> =
                    manifests.iter().map(|(_, m)| m.root_graph_id).collect();
                restored.retag_graph_bound_invalid(&valid_graphs, active_graph);
                frame_layout = restored;
            }
        }
        let a11y_proxy = proxy.clone();
        let mut view = window_view::WindowView::new(
            window_view::WindowKind::Primary,
            active_graph,
            dom,
            runner,
            Workbench::new(),
        );
        view.centered = restored_camera.is_some();
        view.content_location = content_location;
        view.frame_layout = frame_layout;
        view.next_pane_id = next_pane_id;
        // Collapse any duplicate Orrery panes a persisted layout accumulated (two
        // panes on one graph render the extra blank); keep one per graph. (Pane-as-unit.)
        view.frame_layout.dedupe_graph_panes();
        // Restore this session's persisted workbench tiling at boot (not just on a
        // later session switch), so split shape / tabs / active tab survive a restart;
        // pruned to the loaded graph's members. (A3 persistence.)
        let present_members: HashSet<forme::GraphMemberId> =
            orrery.graph().nodes().map(|(_, node)| node.id).collect();
        view.workbench = session_ops::load_workbench(&session_dir, &present_members);
        // Restore live workbench-mirror mode at boot; the render loop re-scopes the
        // orrery to the just-restored open tiles. (Workbench mirror.)
        view.mirror_tiles = restored_view.as_ref().is_some_and(|v| v.mirror_tiles);
        // Restore the orrery's settled layout from the cartography sidecar at boot,
        // overriding the graph's load-time seed (the live layout is never committed to
        // graph.json). (Position sidecar.)
        if let Some(geom) = session_ops::load_cartography(&session_dir, &present_members) {
            orrery.seed_cartography(geom.iter());
            // Restore the importance metric first, so the sizing restore below recomputes with it.
            orrery.apply_cartography_importance_metric(geom.importance_metric());
            // Restore the per-node sizes + the size-by-degree / size-by-importance scene flags
            // alongside the positions. (Node-rep / graph signals.)
            orrery.apply_cartography_sizing(
                geom.size_iter(),
                geom.size_by_degree(),
                geom.size_by_importance(),
            );
            // Restore the custom sprite faces, so a textured node re-opens textured. (Node-rep.)
            orrery.apply_cartography_sprites(geom.sprite_iter());
            // ...and their collider hulls, so the traced-to-image collider survives too. (Node-rep.)
            orrery.apply_cartography_sprite_hulls(geom.sprite_hull_iter());
            // ...and the per-node physical materials, so a tuned node re-opens tuned. (Body & face.)
            orrery.apply_cartography_materials(geom.material_iter());
            // ...and the face overrides LAST, so a node switched off its sprite face re-opens on
            // the chosen face, not back on Sprite from the sprite restore above. (Body & face.)
            orrery.apply_cartography_faces(geom.face_iter());
        }
        // Pool every graph a restored pane resolves to, not just the active one, so a
        // second graph-pane (persisted from a prior run) loads instead of leaving a
        // blank pane the user can't dismiss. Each cold-loads its graph from its
        // session dir and offloads its own physics, like the active orrery above; the
        // render then centres it on first frame. (Window composition — pane-as-unit
        // restore.)
        let mut orreries: HashMap<GraphId, Orrery> = HashMap::from([(active_graph, orrery)]);
        let mut orrery_lru: Vec<GraphId> = vec![active_graph];
        let extra_graphs: HashSet<GraphId> = view
            .frame_layout
            .iter_leaves()
            .filter(|(_, c, gid)| matches!(c, PaneContent::Orrery) && *gid != active_graph)
            .map(|(_, _, gid)| gid)
            .collect();
        for gid in extra_graphs {
            let dir = manifests.iter().find(|(_, m)| m.root_graph_id == gid).map(|(id, m)| {
                m.storage_path
                    .clone()
                    .unwrap_or_else(|| mere_root.join("sessions").join(id.as_uuid().to_string()))
            });
            let graph = dir.and_then(|d| {
                session_graph_store::load(&d.join(session_graph_store::GRAPH_FILE))
                    .ok()
                    .flatten()
            });
            let mut extra = match graph {
                Some(g) => Orrery::with_graph(g),
                None => Orrery::new(),
            };
            extra.offload_physics(physics_wake.clone());
            orreries.insert(gid, extra);
            orrery_lru.push(gid);
        }
        // Apply the persisted "inertia" (linear damping) to every pooled orrery, so a
        // restart honors the saved physics setting. (Physics settings.)
        for orrery in orreries.values_mut() {
            orrery.set_physics_damping(saved_settings.physics_damping);
        }
        let mut app = Self {
            shared: SharedState {
                content: Content {
                    constellation,
                    pages: HashMap::new(),
                    store,
                    fetch_handle,
                    find_worker,
                    engine_registry,
                    engine_pins: HashMap::new(),
                    route_policy: inker::routing::EngineRoutePolicy::default(),
                    engine_activation: engine_activation::EngineActivation::new(
                        saved_settings.disabled_engines.clone(),
                    ),
                    crawl,
                },
                session: Session {
                    manifests,
                    active_session_id,
                    active_persona,
                    session_dir,
                    mere_root,
                    session_thumbnails: HashMap::new(),
                    session_labels: HashMap::new(),
                    host_text: text::HostText::new(),
                },
                presentation: Presentation {
                    theme,
                    chrome_theme,
                    chrome_sheet,
                    active_theme_id,
                    saved_tab_cap: saved_settings.tab_cap,
                    shellbar_edge: saved_settings.shellbar_edge,
                    shellbar_hidden: saved_settings.shellbar_hidden,
                    physics_damping: saved_settings.physics_damping,
                    document_palette,
                    document_sheet,
                    menu_actions,
                    command_usage,
                },
                comms_handle,
                sync_handle,
                inbox: KernelInbox {
                    fetch: fetch_rx,
                    find: find_rx,
                    sync: sync_rx,
                    comms: comms_rx,
                    diagnostics: diagnostics_rx,
                },
                observability: HostObservability::new(),
            },
            orreries,
            orrery_lru,
            windows: HashMap::new(),
            primary: None,
            pending_view: Some(view),
            render_core: None,
            clipboard: arboard::Clipboard::new().ok(),
            a11y_bridge: a11y_bridge::AccessKitBridge::new({
                let proxy = a11y_proxy.clone();
                move || {
                    let _ = proxy.send_event(());
                }
            }),
            secondary_a11y_bridges: HashMap::new(),
            a11y_proxy,
            a11y_action_routes: HashMap::new(),
            commands: Vec::new(),
            physics_wake,
            _kernel: armillary::KernelThread::new(),
        };
        let pane_count = app
            .pending_view
            .as_ref()
            .expect("pending primary view")
            .frame_layout
            .iter_leaves()
            .count();
        app.shared
            .observability
            .record_startup(&app.shared.presentation.active_theme_id, pane_count);
        // The initial switcher-thumbnail + a11y refresh run in `resumed`, once the
        // primary view is keyed into the registry (a ctx needs a window id). (MW2 (d).)
        app
    }

    /// Borrow the **primary** window as a handling context. Before `resumed` keys the
    /// primary in, it falls back to the `pending_view` — so the headless test harness
    /// (which never resumes) and the `new()`-time bootstrap both still resolve a view.
    /// (MW2 (d).)
    fn ctx(&mut self) -> WindowCtx<'_> {
        let view = match self.primary {
            Some(id) => self
                .windows
                .get_mut(&id)
                .expect("primary window missing from registry"),
            None => self
                .pending_view
                .as_mut()
                .expect("a primary or pending view"),
        };
        // Pass the whole pool; the ctx resolves the focused (or a per-pane) orrery
        // by `graph_id`. (Window composition P2; was a single bundled orrery, P1.)
        let pool_count = self.orreries.len();
        let mut wc = WindowCtx {
            view,
            shared: &mut self.shared,
            orreries: &mut self.orreries,
            clipboard: &mut self.clipboard,
            a11y_bridge: &mut self.a11y_bridge,
            a11y_action_routes: &mut self.a11y_action_routes,
            render_core: self.render_core.as_ref(),
            commands: &mut self.commands,
            orrery_pool_count: pool_count,
        };
        // Install this window's per-pane cameras into the shared orreries for the
        // pass; the ctx's `Drop` reads them back. (Camera on the view.)
        wc.install_viewports();
        wc
    }

    /// Read-only borrow of the primary window's view (registry entry, else the
    /// pending bootstrap view). For read paths that don't need the full ctx — the
    /// agent harness's `&Shell` closures. (MW2 (d).)
    #[cfg(any(test, feature = "agent-harness"))]
    fn view(&self) -> &window_view::WindowView {
        match self.primary {
            Some(id) => &self.windows[&id],
            None => self.pending_view.as_ref().expect("a primary or pending view"),
        }
    }

    /// The focused window's orrery resolved from the pool — for the read/write paths
    /// that reach the orrery off `Shell` directly (the agent harness; the per-window
    /// `WindowCtx` bundles it as `self.orrery`). (Window composition P1.)
    #[cfg(any(test, feature = "agent-harness"))]
    fn orrery(&self) -> &Orrery {
        let gid = self.view().focused_graph;
        self.orreries.get(&gid).expect("focused orrery is pooled")
    }

    #[cfg(any(test, feature = "agent-harness"))]
    fn orrery_mut(&mut self) -> &mut Orrery {
        let gid = self.view().focused_graph;
        self.orreries.get_mut(&gid).expect("focused orrery is pooled")
    }

    /// Borrow window `id` as a handling context: its view from the registry plus the
    /// shared state and shell singletons the active window drives. `None` if no such
    /// window. The construction is the per-window seam — a ctx reaches exactly one
    /// view, never the registry or its siblings. (MW2 (d).)
    fn window_ctx(&mut self, id: WindowId) -> Option<WindowCtx<'_>> {
        let view = self.windows.get_mut(&id)?;
        let pool_count = self.orreries.len();
        // Resolve this window's AccessKit bridge: the primary keeps the long-standing
        // `a11y_bridge` field (so its path and the harness are unchanged); a secondary
        // (leaf) gets its own, minted on first access with the shared wake proxy. (MW3
        // step 6 — per-window a11y.)
        let a11y_bridge = if Some(id) == self.primary {
            &mut self.a11y_bridge
        } else {
            let proxy = self.a11y_proxy.clone();
            self.secondary_a11y_bridges
                .entry(id)
                .or_insert_with(move || {
                    a11y_bridge::AccessKitBridge::new(move || {
                        let _ = proxy.send_event(());
                    })
                })
        };
        let mut wc = WindowCtx {
            view,
            shared: &mut self.shared,
            orreries: &mut self.orreries,
            clipboard: &mut self.clipboard,
            a11y_bridge,
            a11y_action_routes: &mut self.a11y_action_routes,
            render_core: self.render_core.as_ref(),
            commands: &mut self.commands,
            orrery_pool_count: pool_count,
        };
        // Install this window's per-pane cameras for the pass; `Drop` reads them back.
        // (Camera on the view.)
        wc.install_viewports();
        Some(wc)
    }

    /// The focused window's view (primary, or the pending bootstrap view) — the same
    /// primary-or-pending resolution `ctx()` uses, for the Shell-level session ops.
    /// Unlike `ctx()` it touches only the view, so it is valid mid-re-key when the
    /// focused orrery is not yet pooled under the new graph id. (Window composition
    /// P1, multi-graph.)
    fn focused_view_mut(&mut self) -> &mut window_view::WindowView {
        match self.primary {
            Some(id) => self.windows.get_mut(&id).expect("primary window missing from registry"),
            None => self.pending_view.as_mut().expect("a primary or pending view"),
        }
    }

    /// Read-only twin of [`Self::focused_view_mut`] — reads the focused graph id
    /// before a re-key without holding a mutable borrow.
    fn focused_view(&self) -> &window_view::WindowView {
        match self.primary {
            Some(id) => &self.windows[&id],
            None => self.pending_view.as_ref().expect("a primary or pending view"),
        }
    }

    /// Mint a fresh per-window view over the shared session: its own chrome +
    /// workbench runners (a second pair of serval document authorities) and a default
    /// single-orrery content frame bound to the active graph. The view-session bits
    /// start at rest (no restored camera / frame); a spawned window opens on the
    /// shared graph the way the primary first did. The caller (`SpawnWindow`) creates
    /// the OS window + surface around it. (Multi-window MW3.)
    fn build_window_view(&self) -> window_view::WindowView {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        let mut chrome = Chrome::new("mere://welcome");
        chrome.settings.tab_cap = self.shared.presentation.saved_tab_cap;
        chrome.slim = true; // a spawned window is a leaf: slim chrome (no shellbar / switcher)
        // Seed the leaf's sync chip from the primary's current state so it shows real
        // standing immediately, not a stale "p2p off" until the next status change. The
        // fan-out keeps it current after. (MW3 step 5; real-sync-feedback.)
        chrome.sync = self.focused_view().chrome().sync.clone();
        // Likewise seed the crawl chip: one crawl is shared kernel state, so a new leaf
        // should show the same "crawling: N pages" immediately. (Crawl controls; MW3.)
        chrome.crawl = self.focused_view().chrome().crawl.clone();
        let runner = window_view::shell_runner(dom.clone(), chrome);
        let content_location = runner.state().chrome.content_location().to_string();
        let active_graph = self
            .shared
            .session
            .manifests
            .get(self.shared.session.active_session_id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        let mut view = window_view::WindowView::new(
            window_view::WindowKind::Leaf,
            active_graph,
            dom,
            runner,
            Workbench::new(),
        );
        view.content_location = content_location;
        view.frame_layout = default_content_frame(active_graph);
        view.next_pane_id = 1;
        view
    }
}

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
mod multi_graph_tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mere-mg-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn seeds_one_session_on_a_fresh_root() {
        let root = temp_root("seed");
        let (store, active) = bootstrap_sessions(&root);
        assert_eq!(store.len(), 1);
        assert!(store.get(active).is_some());
        let manifest = root
            .join("sessions")
            .join(active.as_uuid().to_string())
            .join(session_runtime::MANIFEST_FILE);
        assert!(manifest.exists(), "seeded session manifest written to disk");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn migrates_a_flat_graph_into_a_session() {
        let root = temp_root("migrate");
        // A pre-MG1 flat layout: graph.json + frame.json + views/ at the root.
        std::fs::write(
            root.join(session_graph_store::GRAPH_FILE),
            br#"{"flat":true}"#,
        )
        .unwrap();
        std::fs::write(root.join(frame_layout_store::FRAME_FILE), b"{}").unwrap();
        let flat_views = root.join(view_intent_store::VIEW_INTENT_DIR);
        std::fs::create_dir_all(&flat_views).unwrap();
        std::fs::write(flat_views.join("pane.json"), b"{}").unwrap();

        let (store, active) = bootstrap_sessions(&root);
        assert_eq!(store.len(), 1);
        let session_dir = root.join("sessions").join(active.as_uuid().to_string());
        // The session-scoped artefacts (graph + views) moved into the session dir,
        // and the bytes survived.
        assert!(session_dir.join(session_graph_store::GRAPH_FILE).exists());
        assert!(
            session_dir
                .join(view_intent_store::VIEW_INTENT_DIR)
                .join("pane.json")
                .exists()
        );
        assert!(
            !root.join(session_graph_store::GRAPH_FILE).exists(),
            "the flat graph was moved, not copied"
        );
        // The frame layout is window-scoped (MG5): it stays at the root, not the
        // session dir.
        assert!(
            root.join(frame_layout_store::FRAME_FILE).exists(),
            "the window-scoped frame stays at the root"
        );
        assert!(
            !session_dir.join(frame_layout_store::FRAME_FILE).exists(),
            "the frame is not pulled into the session"
        );
        let moved =
            std::fs::read_to_string(session_dir.join(session_graph_store::GRAPH_FILE)).unwrap();
        assert!(moved.contains("flat"), "no graph lost in the migration");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reuses_an_existing_session_instead_of_seeding() {
        let root = temp_root("reuse");
        let (_first, first) = bootstrap_sessions(&root);
        let (store, active) = bootstrap_sessions(&root);
        assert_eq!(store.len(), 1, "no duplicate session seeded");
        assert_eq!(active, first, "the existing session is reopened as active");
        std::fs::remove_dir_all(&root).ok();
    }
}

/// Lay out the chrome root and return the border-box bottom (px, rounded up) of
/// the first element carrying CSS class `class` — `"toolbar"` for the content
/// split, `"chrome"` for the click-region gate (toolbar + open dropdown).
/// `None` if no such element is laid out.
fn measure_class_bottom(
    dom: &ScriptedDom,
    sheet: &[&str],
    w: u32,
    h: u32,
    class: &str,
) -> Option<u32> {
    let frags = fragments_from_scripted_dom(dom, sheet, w, h);
    class_bottom_in(dom, &frags, class)
}

/// The border-box bottom (px, rounded up) of the first element carrying CSS
/// `class`, read from an **already-computed** fragment plane. Lets a caller that
/// holds a session's retained fragments (`ChromeSession::fragments`) measure the
/// chrome-region gate off the rendered layout instead of re-laying-out (C4).
/// `None` if no such element is laid out, or its bottom is non-positive.
fn class_bottom_in(dom: &ScriptedDom, frags: &FragmentPlane<NodeId>, class: &str) -> Option<u32> {
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
    dom.dom_children(id)
        .find_map(|c| first_with_class(dom, c, class))
}

/// Every element carrying CSS class `class` in pre-order under `id`. Used to find
/// the workbench root's content placeholders, one per tile.
fn all_with_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Vec<NodeId> {
    let mut out = Vec::new();
    if has_class(dom, id, class) {
        out.push(id);
    }
    for child in dom.dom_children(id) {
        out.extend(all_with_class(dom, child, class));
    }
    out
}

/// The `data-member` attribute of element `id`, parsed as a graph member id — the
/// tile whose content composites at this placeholder's rect.
fn member_attr(dom: &ScriptedDom, id: NodeId) -> Option<GraphMemberId> {
    dom.attributes(id)
        .find(|a| a.name.local.as_ref() == "data-member")
        .and_then(|a| a.value.parse::<GraphMemberId>().ok())
}

/// The orrery's themed palette — `(backdrop, edge)` as straight `[r, g, b, a]`
/// (0..1) — from a resolved theme: the backdrop is the theme background, the edge
/// a translucent default stroke that contrasts with it per theme. (Theming A2.)
fn orrery_palette(tokens: &register_theme::theme::ThemeTokenSet) -> ([f32; 4], [f32; 4]) {
    let (br, bg, bb) = tokens.theme_data.background_rgb;
    let backdrop = [br as f32 / 255.0, bg as f32 / 255.0, bb as f32 / 255.0, 1.0];
    let [er, eg, eb, _] = tokens.graph_node_chrome.default_stroke.to_array();
    // A higher alpha than the old translucent edges, so the stroke reads on a
    // light backdrop instead of washing out.
    let edge = [
        er as f32 / 255.0,
        eg as f32 / 255.0,
        eb as f32 / 255.0,
        0.85,
    ];
    (backdrop, edge)
}

/// Straight RGBA (0..1) for a chrome `Color32` — the format the document
/// [`document_canvas::ColorVocabulary`] + paint list consume.
fn vocab_color(c: register_theme::theme::Color32) -> [f32; 4] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        c.a() as f32 / 255.0,
    ]
}

/// The document-lane color palette for the focused-content card, derived from
/// the active theme: the chrome text tiers (`body` / `strong` / `muted`) plus
/// the theme accent for links, so smolweb / markdown / feed cards re-theme with
/// the shell instead of a fixed light-on-dark palette. Code has no dedicated
/// token — it takes `body_text` and leans on the monospace font for the
/// distinction. (Document theming, P3.)
fn document_palette(
    tokens: &register_theme::theme::ThemeTokenSet,
) -> document_canvas::ColorVocabulary {
    let ch = &tokens.chrome;
    let (ar, ag, ab) = tokens.theme_data.accent_rgb;
    let muted = ch.muted_text;
    let muted_rgb = [
        muted.r() as f32 / 255.0,
        muted.g() as f32 / 255.0,
        muted.b() as f32 / 255.0,
    ];
    document_canvas::ColorVocabulary {
        body_text: vocab_color(ch.body_text),
        heading_text: vocab_color(ch.strong_text),
        link_text: [ar as f32 / 255.0, ag as f32 / 255.0, ab as f32 / 255.0, 1.0],
        code_text: vocab_color(ch.body_text),
        badge_text: vocab_color(ch.muted_text),
        rule: vocab_color(ch.muted_text),
        placeholder_text: [muted_rgb[0], muted_rgb[1], muted_rgb[2], 0.12],
        placeholder_image: [muted_rgb[0], muted_rgb[1], muted_rgb[2], 0.20],
    }
}

/// A `wgpu::Color` (opaque) for a chrome `Color32` — the host-cleared content
/// card background, taken from the theme's floated-panel surface so the card
/// reads as a raised surface in every theme. (Document theming, P3.)
fn chrome_to_wgpu(c: register_theme::theme::Color32) -> wgpu::Color {
    wgpu::Color {
        r: c.r() as f64 / 255.0,
        g: c.g() as f64 / 255.0,
        b: c.b() as f64 / 255.0,
        a: 1.0,
    }
}

/// The first element with local tag `local` in pre-order under `id`.
fn first_tag(dom: &ScriptedDom, id: NodeId, local: &str) -> Option<NodeId> {
    if dom
        .element_name(id)
        .is_some_and(|q| q.local.as_ref() == local)
    {
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

/// Map the orrery camera to a serialized [`CameraSnapshot`] — the kurbo `Affine`
/// coefficient order `[a, b, c, d, e, f]` (a point maps to `(a*x + c*y + e,
/// b*x + d*y + f)`). The orrery camera is rotation(`yaw`) . non-uniform-scale(`zoom`,
/// `tilt*zoom`) . translate(`offset`), which the six coefficients carry exactly; a
/// top-down camera (`yaw 0`, `tilt 1`) reduces to `a = d = zoom, b = c = 0` (the prior
/// form), so old snapshots load unchanged. (Isometric camera — persist yaw/tilt.)
fn camera_to_snapshot(camera: CameraView, yaw: f32, tilt: f32) -> session_runtime::CameraSnapshot {
    let (sn, cs) = (yaw.sin() as f64, yaw.cos() as f64);
    let z = camera.zoom as f64;
    let tz = (tilt * camera.zoom) as f64;
    session_runtime::CameraSnapshot {
        coefficients: [
            cs * z,
            sn * tz,
            -sn * z,
            cs * tz,
            camera.offset.0 as f64,
            camera.offset.1 as f64,
        ],
    }
}

/// Recover pan + zoom from the affine coefficients: `offset` from `(e, f)`, `zoom`
/// from the first row's magnitude (`sqrt(a^2 + c^2)`, which is `zoom` for the orrery's
/// rotation+scale affine). The yaw/tilt half is [`snapshot_yaw_tilt`].
fn snapshot_to_camera(snapshot: &session_runtime::CameraSnapshot) -> CameraView {
    let m = snapshot.coefficients;
    let zoom = (m[0] * m[0] + m[2] * m[2]).sqrt();
    CameraView {
        offset: (m[4] as f32, m[5] as f32),
        zoom: zoom as f32,
    }
}

/// Recover the isometric orbit (`yaw`, radians) and vertical foreshorten (`tilt`) from
/// the affine coefficients: `yaw = atan2(-c, a)`, `tilt = |row2| / |row1|`. An old
/// top-down snapshot (`b = c = 0`, `a = d = zoom`) yields `(0, 1)`. (Isometric camera.)
fn snapshot_yaw_tilt(snapshot: &session_runtime::CameraSnapshot) -> (f32, f32) {
    let m = snapshot.coefficients;
    let row1 = (m[0] * m[0] + m[2] * m[2]).sqrt();
    let row2 = (m[1] * m[1] + m[3] * m[3]).sqrt();
    let yaw = (-m[2]).atan2(m[0]);
    let tilt = if row1 > 1e-6 { row2 / row1 } else { 1.0 };
    (yaw as f32, tilt as f32)
}

/// A durably-cached entry as a [`fetch::Fetched`], decoding the stored body as
/// text (lossily). Binary subresources are served from the resource cache as
/// bytes; this text view is for the page-document lane.
fn fetched_from(stored: session_runtime::content_store::StoredContent) -> fetch::Fetched {
    fetch::Fetched {
        content_type: stored.content_type,
        body: String::from_utf8_lossy(&stored.body).into_owned(),
    }
}

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
