/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render, resize, and toolbar-measurement for [`Shell`](super::Shell). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use forme::GraphMemberId;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use netrender::ColorLoad;
use netrender::external_texture::{ExternalTexturePlacement, SourceAlpha};
use image::ImageEncoder;
use crate::serval_render::TextCursor;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;

use std::cell::RefCell;
use std::time::Instant;

use super::fetch::{ContentState, Fetched};
use super::resources::{ResourceLoader, ResourceStore};
use frame::{PaneContent, SessionId};

use super::{
    CARD_BG, FALLBACK_TOOLBAR_H, WindowCtx, first_with_class, frame_view, shellbar,
    measure_class_bottom,
};
use meerkat::ShellbarPaneStates;
use crate::pane_session::PaneSession;
use crate::window_view::{OrreryCard, OrreryRender};

mod setup;
mod overlays;
mod orrery_scene;
mod workbench;
mod cards;
mod compose;
mod paint;
use paint::PaintInputs;
pub(crate) use setup::*;

impl WindowCtx<'_> {
    /// Render the two authorities and present them. The orrery content root fills
    /// everything below the toolbar; the chrome root is rendered over the full
    /// window with a *transparent* clear, so its toolbar band and any open
    /// dropdown float above the content while the rest lets the orrery show
    /// through. Composite order is content first, then chrome on top.
    pub(super) fn render(&mut self) {
        if self.view.surface.is_none() || self.render_core.is_none() {
            return;
        }
        let (frame_t, w, h, toolbar_h, dpr) = self.setup_and_sync_chrome();

        let FrameRects {
            leaves,
            orrery_rect,
            orrery_gid,
            workbench_rect,
            roster_rect,
            comms_rect,
            list_pane_rects,
            dividers,
            orrery_w,
            orrery_h,
            cursor,
        } = self.compute_layout_rects(w, h, toolbar_h);
        let chrome_scroll = self.position_chrome_overlays(w, h, toolbar_h, orrery_rect, comms_rect);
        let (orrery_scene, orrery_redraw) =
            self.render_orrery_scene(orrery_gid, orrery_w, orrery_h, orrery_rect, workbench_rect);
        // Fold the roster pane into the shell document: snapshot its rect + rows into the
        // shell state before the one render lays the document out, so the roster renders,
        // hit-tests, and projects a11y through the shell runner (its CSS rides the shell
        // stylesheet below). Replaces the separate RosterPane frame + composite. (Phase 1.)
        if roster_rect.is_some() {
            let rows = self.roster_rows();
            let field_rows = self.roster_field_rows();
            self.view.set_roster(rows, field_rows, roster_rect);
        } else if self.view.roster_open() {
            self.view.set_roster(Vec::new(), Vec::new(), None);
        }
        // Fold the four list panes (apparatus / steward / inspector / trail) into the same
        // shell document: snapshot each open pane's items + rect into its slot before the
        // render lays the document out. Replaces the separate ListPane frames + composites.
        // (Phase 1, step 2.)
        self.snapshot_list_panes(list_pane_rects);
        // The roster + folded list panes scroll their own inner containers via the engine's
        // retained `element_scroll` (the wheel drives `scroll_at`), which `emit_paint_list`
        // folds in through `merged_scroll`. The host no longer mirrors per-pane offsets into
        // `chrome_scroll` here; it carries only the scroll-into-view targets below. (Host-scroll P2.)
        let (roster_css, apparatus_css, utility_css) = self.gather_chrome_css();
        // The chrome (shell document) scene is built **after** the workbench block below,
        // not here: the folded settings panes are positioned at the workbench's tile rects,
        // which are only known once the tile surface has laid out. Rendering the shell after
        // `snapshot_settings_panes` lets a spine page-switch land this frame instead of one
        // frame late. `roster_css` / `apparatus_css` / `utility_css` are owned, so they stay
        // here and feed the deferred `chrome_sheet`. (Settings lane P1.)

        // The orrery's per-frame update (node state/shape, resize, recenter, mirror,
        // strategy) and the node-card snapshot now run *above* the chrome render, so the
        // cards read this-frame positions/colors and align with the scene (no one-frame
        // lag). See the "Orrery-as-element" block before the snapshot.
        // P2 per-pane render: a second graph-pane (Shift+click a switcher tile)
        // drives its own pooled orrery into its own leaf, beside the focused one,
        // so two graphs show at once. Node coloring + the focused-node card stay
        // on the primary pane for now; this draws each extra graph live. (Window
        // composition P2 — second graph-pane.)
        let secondary_orreries = self.render_secondary_orreries(&leaves, orrery_gid);
        // The workbench pane renders through the pelt `TileSurface` (V6): meerkat owns
        // the `Workbench` (the authority), projects it onto pelt's tile-tree contract
        // each frame, drives the surface, and composites each member's actor texture
        // into the surface's reported tile rects below. `workbench_scene` is the
        // surface's frame (tab bars + dividers); `None` when the pane isn't open.
        let (workbench_scene, workbench_external, workbench_ghost) =
            self.render_workbench_surface(workbench_rect);

        // Reconcile the active-node pool to what this frame shows — the open tiles
        // (Tree) or the focused node (Cartography). Needed-but-dormant nodes spawn
        // an actor; active nodes no longer shown are reaped, unless backgrounded.
        let gid = self.view.focused_graph;
        let needed: Vec<_> = self.needed_members().into_iter().map(|m| (m, gid)).collect();
        self.shared.content.constellation.reconcile(&needed);

        let (cards, scrying_surfaces) = self.collect_cards(workbench_rect, workbench_external);
        // An open settings tile scrolls its `.settings-pane-body` via the engine's retained
        // `element_scroll` (the wheel drives `scroll_at`); `emit_paint_list` folds it in, so the
        // host no longer mirrors the offset into `chrome_scroll`. (Host-scroll P2.)

        let (chrome_scene, chrome_us, external_texture_placements) = self.render_chrome_scene(
            w,
            h,
            cursor,
            &chrome_scroll,
            &roster_css,
            &apparatus_css,
            &utility_css,
        );
        // The "last visit" snapshot card (focused, visited, not live) + the "unvisited"
        // placeholder card (focused node, no snapshot yet): both composite on their own
        // path below. `snapshot_card` is `(member, url, dest rect, scene)` — the scene is
        // `Some` only when it must be (re)rasterized this frame; once its texture is cached
        // by url, later frames carry `None`.
        let (snapshot_card, unvisited_card) = self.compute_focus_cards(workbench_rect, orrery_rect);

        // Reap every compat WebView whose member isn't a surface shown this frame:
        // a tile that was closed / unpinned, or a card that lost focus, is torn down
        // here (reap-on-deselect) so its visual can't freeze on screen. The shared
        // composition target persists, so the surviving panes are untouched. (X3
        // lifecycle; multi-tile.)
        let shown: std::collections::HashSet<GraphMemberId> =
            scrying_surfaces.iter().map(|(m, _)| *m).collect();
        self.view.scrying.retain(&shown);

        self.finalize_content_rects(&cards, unvisited_card, &snapshot_card, &scrying_surfaces);

        // The omnibar follows focus: point it at the focused tile / node when that
        // changed (next frame, like the chrome strips were — the scene above is
        // already built).
        self.sync_location();
        // Back/forward enabled-state tracks the focused node's own history.
        self.sync_nav_buttons();
        self.drain_portable_diagnostics();

        // The shared core (rasterize / compose) + this window's surface (acquire /
        // format); both checked present at the method entry. (MW3: one device, N surfaces.)
        self.paint_frame(PaintInputs {
            chrome_scene,
            orrery_scene,
            orrery_redraw,
            orrery_w,
            orrery_h,
            secondary_orreries,
            workbench_scene,
            workbench_ghost,
            workbench_rect,
            cards,
            scrying_surfaces,
            snapshot_card,
            external_texture_placements,
            dividers,
            w,
            h,
            toolbar_h,
            dpr,
            chrome_us,
            frame_t,
        });
    }

}
