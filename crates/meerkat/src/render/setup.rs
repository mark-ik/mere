/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Setup, measurement, and per-frame sync helpers for [`render`](super). Split
//! from `render.rs` to keep files under the workspace 600-LOC ceiling. The four
//! free fns and the small `WindowCtx` measurement methods live here; the new
//! frame-setup helpers (`setup_and_sync_chrome`, `compute_layout_rects`,
//! `gather_chrome_css`) extracted from `render()` join them.

use super::*;

/// The per-frame frame-tree layout outputs: the laid-out leaves + dividers, the orrery
/// pane's rect / graph / size, the optional pane rects (workbench / roster / comms / the
/// five folded list panes), and the focused-field text cursor. Produced once at the top
/// of `render()` by [`WindowCtx::compute_layout_rects`] and destructured at the call site.
/// (Extracted from `render()` — frame layout.)
pub(super) struct FrameRects {
    pub leaves: Vec<frame_view::LaidLeaf>,
    pub orrery_rect: [f32; 4],
    pub orrery_gid: frame::GraphId,
    pub workbench_rect: Option<[f32; 4]>,
    pub roster_rect: Option<[f32; 4]>,
    pub gloss_rect: Option<[f32; 4]>,
    pub comms_rect: Option<[f32; 4]>,
    pub list_pane_rects: [Option<[f32; 4]>; 5],
    pub dividers: Vec<frame_view::LaidDivider>,
    pub orrery_w: u32,
    pub orrery_h: u32,
    pub cursor: Option<TextCursor>,
}

/// The inline `style` for a host-positioned overlay surface: an absolute box at
/// `(x, y)` sized `w`×`h`, optionally with a `flex-direction`. Mirrors the
/// `xilem_serval::overlay_rect` geometry for the surfaces whose rect is a **layout
/// output** (the comms pane fills its frame leaf; the shellbar docks to a window
/// edge) — render patches these post-layout rather than building them with the
/// rect at view time, so this is the one spot that formats their geometry. (Overlay
/// adoption P3.)
pub(crate) fn overlay_geometry_style(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    flex_dir: Option<&str>,
) -> String {
    let mut style =
        format!("position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px;");
    if let Some(dir) = flex_dir {
        style.push_str(&format!(" flex-direction: {dir};"));
    }
    style
}

impl crate::WindowCtx<'_> {
    pub(crate) fn toolbar_fallback_height(&self) -> u32 {
        let scaled = FALLBACK_TOOLBAR_H as f32 * self.shared.presentation.ui_scale();
        scaled.ceil().max(1.0) as u32
    }

    pub(crate) fn current_toolbar_height(&self) -> u32 {
        if self.view.toolbar_h == 0 {
            self.toolbar_fallback_height()
        } else {
            self.view.toolbar_h
        }
    }

    /// The toolbar-band height (px), measured against the current chrome sheet,
    /// window width, and chrome DOM state. High zoom and crowded chips make the
    /// toolbar effectively width-sensitive, so this refreshes the stored inset
    /// instead of treating it as a once-per-window constant.
    pub(crate) fn toolbar_height(&mut self) -> u32 {
        let fallback = self.toolbar_fallback_height();
        let previous = self.view.toolbar_h;
        let sheet = self.shared.presentation.chrome_sheet_refs();
        let measured = measure_class_bottom(
            &self.view.dom.borrow(),
            &sheet,
            self.view.width,
            self.view.height,
            "toolbar",
        )
        .unwrap_or(fallback);
        self.view.toolbar_h = measured.max(1);
        if previous != self.view.toolbar_h {
            tracing::trace!(
                target: "meerkat::profile",
                previous,
                next = self.view.toolbar_h,
                width = self.view.width,
                height = self.view.height,
                "toolbar height changed"
            );
        }
        self.view.toolbar_h
    }

    /// Reconfigure the surface for `(width, height)` and request a redraw.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        let next_w = width.max(1);
        let next_h = height.max(1);
        let resized = self.view.width != next_w || self.view.height != next_h;
        self.view.width = next_w;
        self.view.height = next_h;
        if resized {
            self.view.toolbar_h = 0;
            self.view.window_controls_tex = None;
        }
        if let (Some(surface), Some(core)) = (self.view.surface.as_mut(), self.render_core) {
            surface.resize(core, self.view.width, self.view.height);
        }
        self.refresh_a11y_summary();
        self.view.request_redraw();
    }

    /// Enumerate the chrome document's `<external-texture>` elements as `(key, [x0, y0, x1, y1])`
    /// in viewport px, from the chrome session's retained layout. The host composites each
    /// element's registered texture (resolved by `key`) at its laid-out rect, so an external
    /// surface's placement comes from the document, not a hardcoded host rect. The
    /// external-texture-element compose. (cond 5.)
    pub(super) fn external_texture_placements(&self) -> Vec<(u64, [f32; 4])> {
        let Some(session) = self.view.chrome_session.as_ref() else {
            return Vec::new();
        };
        let dom = self.view.dom.borrow();
        let fragments = session.fragments();
        let origins = crate::genet_render::accumulate_origins(&dom, fragments);
        let mut out = Vec::new();
        let mut stack = vec![dom.document()];
        while let Some(node) = stack.pop() {
            if dom.element_name(node).map(|q| q.local.as_ref()) == Some("external-texture") {
                if let (Some(&(x, y)), Some(rect)) = (origins.get(&node), fragments.rect_of(node)) {
                    if let Some(key) = dom
                        .attributes(node)
                        .find(|a| a.name.local.as_ref() == "key")
                        .and_then(|a| a.value.parse::<u64>().ok())
                    {
                        out.push((key, [x, y, x + rect.size.width, y + rect.size.height]));
                    }
                }
            }
            for child in dom.dom_children(node) {
                stack.push(child);
            }
        }
        out
    }

    /// Snapshot the four folded list panes into the shell document for this frame: build
    /// each open pane's items and set its slot (closing a pane that just went away). The
    /// per-frame call before the shell render, the ListPane analogue of `set_roster` —
    /// each inner root takes a unique-id class (`apparatus`, `utility-pane steward`, …) so
    /// the shared `.utility-pane` styling still applies while `has_class` finds each pane
    /// distinctly for scroll + hit-test. (Phase 1, step 2.)
    pub(super) fn snapshot_list_panes(&mut self, rects: [Option<[f32; 4]>; 5]) {
        use crate::window_view::ShellListPane::{Alembic, Apparatus, Inspector, Steward, Trail};
        // Apparatus: theme + engine + physics buttons over the host observability rows.
        if let Some(rect) = rects[0] {
            // The apparatus is read-only diagnostics now; its settings sections moved to the
            // pelt settings lane (Settings lane P2).
            let system_rows = self.apparatus_system_rows();
            let table_stats = self.apparatus_table_stats();
            let sync_rows = self.apparatus_sync_rows();
            let obs = self.apparatus_observability();
            let graph_metrics = mere::glossary::graph_metrics(self.orrery().graph());
            let items = crate::apparatus::apparatus_items(
                &system_rows,
                &table_stats,
                &sync_rows,
                &obs,
                &graph_metrics,
            );
            self.set_list_pane(Apparatus, "apparatus", items, Some(rect));
        } else if self.list_pane_open(Apparatus) {
            self.set_list_pane(Apparatus, "apparatus", Vec::new(), None);
        }
        // Steward + Inspector: display-only `label: value` rows under a unique utility root.
        if let Some(rect) = rects[1] {
            // Steward builds its own items (status rows + clickable action buttons),
            // mirroring Alembic, so the focused-op verbs are reachable by click. (A2.)
            let items = self.steward_items();
            self.set_list_pane(Steward, "utility-pane steward", items, Some(rect));
        } else if self.list_pane_open(Steward) {
            self.set_list_pane(Steward, "utility-pane steward", Vec::new(), None);
        }
        if let Some(rect) = rects[2] {
            let rows = self.utility_pane_rows(&PaneContent::Inspector);
            let items = crate::utility_panes::utility_pane_items(&PaneContent::Inspector, &rows);
            self.set_list_pane(Inspector, "utility-pane inspector", items, Some(rect));
        } else if self.list_pane_open(Inspector) {
            self.set_list_pane(Inspector, "utility-pane inspector", Vec::new(), None);
        }
        // Trail: its own sectioned items (history / recent / removed); a Removed row recovers.
        if let Some(rect) = rects[3] {
            let items = self.trail_items();
            self.set_list_pane(Trail, "utility-pane trail", items, Some(rect));
        } else if self.list_pane_open(Trail) {
            self.set_list_pane(Trail, "utility-pane trail", Vec::new(), None);
        }
        // Alembic: memory — Recent / Saved / Engrams sections (the Engrams list reads eidetic).
        if let Some(rect) = rects[4] {
            let items = self.alembic_items();
            self.set_list_pane(Alembic, "utility-pane alembic", items, Some(rect));
        } else if self.list_pane_open(Alembic) {
            self.set_list_pane(Alembic, "utility-pane alembic", Vec::new(), None);
        }
    }

    /// The focused node's content-card descriptor for the shell document: its rect (local
    /// to the orrery element) and kind. A visited node gets a snapshot preview (the host
    /// shows the cached member/url preview image); an unvisited node gets a dashed
    /// placeholder. `None` when no node is focused, or the focused node is an open
    /// workbench tile (the tile is the view; a card would contend for its content actor).
    /// The card is placed *after* the gnodes in document order, so it paints over them
    /// while the chrome overlays still paint over it. (Layering fix — card over nodes.)
    pub(super) fn compute_focus_card(
        &self,
        orrery_rect: [f32; 4],
        workbench_rect: Option<[f32; 4]>,
    ) -> Option<crate::window_view::FocusCard> {
        use crate::window_view::{FocusCard, FocusCardKind};
        // A multi-node selection summons the connections swatch (the selected nodes + their
        // inter-edges) in place of a single-node card. (Swatch primitive — P2, scope=Selection.)
        if self.orrery().selected_members().len() > 1 {
            return self.compute_connections_card(orrery_rect);
        }
        let member = self.focused_member().filter(|m| {
            workbench_rect.is_none() || !self.view.workbench.open_members().contains(m)
        })?;
        // The node's pane-local screen position (same camera the gnodes use).
        let (nx, ny) = self.orrery().focused_node_screen()?;
        let (pw, ph) = (
            orrery_rect[2] - orrery_rect[0],
            orrery_rect[3] - orrery_rect[1],
        );
        let local = [0.0, 0.0, pw, ph];
        let (kind, cw, ch) = if self.view.object_card == Some(member) {
            // The object card replaces the preview when summoned (the context action set the
            // card's member). The node preset (P1): the size-tier stepper + the representation
            // toggle, resolved from the node's current settings. (Object card — P1.)
            use crate::window_view::CardWidget;
            let widgets = self
                .orrery()
                .focused_key()
                .map(|k| {
                    vec![
                        CardWidget::SizeTier {
                            tier: self.orrery().node_size_tier(k),
                        },
                        CardWidget::Face {
                            is_favicon: self.orrery().node_face(k) == mere::canvas::Face::Favicon,
                        },
                    ]
                })
                .unwrap_or_default();
            let ch = crate::card::object_card_height(widgets.len());
            (
                FocusCardKind::ObjectCard { widgets },
                crate::card::OBJCARD_W,
                ch,
            )
        } else if self.orrery().member_visited(member) {
            // Show only the *cached* snapshot image — no placeholder while it is still
            // building. A fresh focus otherwise flashed an empty card (and claimed a hit-rect
            // over the node, making the double-click-to-open flaky). `None` → no focus card
            // this frame; the readback below still builds + caches it for the next focus.
            // (Snapshot — no placeholder flash.)
            let current_url = self.orrery().focused_url()?;
            let data_uri = self
                .view
                .snapshot_data_uris
                .get(&member)
                .filter(|snapshot| snapshot.url == current_url)
                .map(|snapshot| snapshot.data_uri.clone())?;
            (
                FocusCardKind::Snapshot { data_uri },
                crate::card::SNAP_W,
                crate::card::SNAP_H,
            )
        } else {
            (
                FocusCardKind::Unvisited,
                crate::card::UNVIS_W,
                crate::card::UNVIS_H,
            )
        };
        let (x0, y0, x1, y1, _, _) = crate::card::anchored_card_rect(nx, ny, cw, ch, local)?;
        Some(FocusCard {
            rect: [x0, y0, x1, y1],
            kind,
        })
    }

    /// Start the frame clock and sync the always-present per-frame chrome state into the
    /// shell view: the device-pixel-ratio push, the comms-pane reserve, the shellbar
    /// pane/edge state, the session chips, and the find-match count. Returns the frame
    /// start instant plus the frame geometry `(w, h, toolbar_h, dpr)` the rest of
    /// `render()` is laid out against. (Extracted from `render()` — frame setup.)
    pub(super) fn setup_and_sync_chrome(&mut self) -> (std::time::Instant, u32, u32, u32, f32) {
        // C0 baseline (cheap-path plan): time the whole frame + the always-present
        // chrome pipeline. Enable with `RUST_LOG=meerkat::profile=debug` and drive a
        // representative interaction. Per-pane granularity (roster/apparatus/utility,
        // each conditional) is the documented C0 refinement on top of this headline.
        let frame_t = Instant::now();
        let (w, h) = (self.view.width.max(1), self.view.height.max(1));
        // This window's display dpi (D3 — per-window). Re-bake the shared chrome sheet to
        // it when it differs from the current bake, so a window on a different-density
        // monitor renders at its own scale. A no-op on a single monitor (or co-density
        // windows): the sheet stays baked at this dpi after the first sync. (Auto-DPI D3.)
        let dpr = self.view.dpi_scale;
        if (self.shared.presentation.dpi_scale - dpr).abs() > 1e-3 {
            self.shared.presentation.dpi_scale = dpr;
            self.shared.presentation.rebuild_chrome_sheet();
            self.view.toolbar_h = 0; // re-measure the band from the re-scaled sheet
        }
        // Push this window's device-pixel-ratio to the content pool: actors lay out
        // logical, the host rasterizes their scenes at physical via `rasterize_scaled`
        // below, so content text is crisp + correctly-sized on a HiDPI display. (Auto-DPI D2/D3.)
        self.shared
            .content
            .constellation
            .set_device_pixel_ratio(dpr);

        // Reserve / drop the Comms frame leaf to match the chrome's comms-open state
        // before laying the panes out, so the other panes make room for it. (Comms.)
        self.sync_comms_pane();

        // Sync shellbar pane-open states + hidden flag into Chrome before the runner so
        // the view rebuilds with current active states. (Shellbar F2.1.) The dock edge is
        // applied host-side from `presentation` each render, so it is not mirrored here.
        let sb_panes = ShellbarPaneStates {
            workbench: self.pane_of_content(&PaneContent::Workbench).is_some(),
            roster: self.pane_of_content(&PaneContent::Roster).is_some(),
            gloss: self.pane_of_content(&PaneContent::Gloss).is_some(),
            trail: self.pane_of_content(&PaneContent::Trail).is_some(),
            alembic: self.pane_of_content(&PaneContent::Alembic).is_some(),
            apparatus: self.pane_of_content(&PaneContent::Apparatus).is_some(),
            inspector: self.pane_of_content(&PaneContent::Inspector).is_some(),
            steward: self.pane_of_content(&PaneContent::Steward).is_some(),
            comms: self.pane_of_content(&PaneContent::Comms).is_some(),
        };
        let sb_hidden = self.shared.presentation.shellbar_hidden;
        if self.chrome().shellbar_panes != sb_panes || self.chrome().shellbar_hidden != sb_hidden {
            self.chrome_update(move |c| {
                c.shellbar_panes = sb_panes;
                c.shellbar_hidden = sb_hidden;
            });
        }

        // Sync the open sessions into Chrome as toolbar chips (Chrome bar P4 — the
        // switcher moved out of the shellbar). Ordered by id like `cycle_session`; the
        // focused pane's session is the active chip. A slim (leaf) window shows none.
        let chips: Vec<meerkat::SessionChip> = if self.view.kind.is_slim() {
            Vec::new()
        } else {
            let focused = self
                .session_for_graph(self.view.focused_graph)
                .map(|(id, _)| id);
            // While a session is being renamed (F2 / context rename), show the live edit
            // buffer in its chip — the chip is the rename surface now the switcher is gone.
            let renaming = self.view.renaming.clone();
            // Source the session list from the canonical manifest store (what
            // `cycle_session` enumerates), not the retired switcher-thumbnail map.
            // (Chrome bar P4 cleanup.)
            let mut ids: Vec<SessionId> = self
                .shared
                .session
                .manifests
                .iter()
                .map(|(id, _)| id)
                .collect();
            ids.sort_by_key(|id| *id.as_uuid());
            ids.iter()
                .map(|id| {
                    let label = match &renaming {
                        // The live rename buffer (with a caret bar) — shown unclipped.
                        Some((rid, buf)) if rid == id => format!("{buf}\u{2502}"),
                        _ => {
                            let raw = self
                                .shared
                                .session
                                .session_labels
                                .get(id)
                                .filter(|l| !l.is_empty())
                                .cloned()
                                .unwrap_or_else(|| {
                                    let s = id.as_uuid().to_string();
                                    format!("graph {}", &s[..s.len().min(4)])
                                });
                            // Clip long labels (a session named after a URL) so a chip
                            // stays compact; the full name is one F2/rename away.
                            if raw.chars().count() > 22 {
                                format!("{}\u{2026}", raw.chars().take(21).collect::<String>())
                            } else {
                                raw
                            }
                        }
                    };
                    meerkat::SessionChip {
                        id: *id,
                        label,
                        active: Some(*id) == focused,
                        thumb: self.shared.session.session_thumbs.get(id).cloned(),
                    }
                })
                .collect()
        };
        if self.chrome().sessions != chips {
            self.chrome_update(move |c| c.sessions = chips);
        }

        // Sync the focused node's live find-match count into Chrome so the find bar
        // shows "active/total" before the chrome is rasterized this frame. (Find S2.)
        if self.chrome().find_open {
            let find_count = self
                .focused_member()
                .map(|m| self.find_matches_for(m).len())
                .unwrap_or(0);
            if self.chrome().find_count != find_count {
                self.chrome_update(move |c| c.find_count = find_count);
            }
        }
        let toolbar_h = self.toolbar_height().min(h);
        (frame_t, w, h, toolbar_h, dpr)
    }

    /// Build the owned per-pane chrome stylesheets for this frame, pre-merged into one
    /// sheet: the roster, the folded list panes (apparatus + utility), and the gloss
    /// outline + recent lenses. Owned so it outlives the deferred `chrome_sheet`
    /// assembly below. Rules are inert when no matching pane element is in the
    /// document, so every pane's sheet can be unconditional. (Extracted from
    /// `render()`; gloss sheets added by the gloss-outline plan P1 / Scene-to-DOM P1.)
    pub(super) fn gather_chrome_css(&self) -> Vec<String> {
        // Built from the light/dark chrome-token PAIR (not the active mode's
        // tokens) and pair-baked, so these strings — appended to the chrome
        // sheet each frame — stay identical across a light/dark mode flip and
        // the chrome session keeps its cheap `set_prefers_color_scheme` path.
        // (Theme-modes T2.)
        let side = |theme: &register_theme::chrome::ChromeTheme| {
            let mut css = crate::roster::roster_sheet(theme);
            css.extend(crate::apparatus::apparatus_sheet(theme));
            css.extend(crate::utility_panes::utility_pane_sheet(theme));
            css.extend(crate::gloss_outline_view::gloss_outline_sheet(theme));
            css.extend(crate::gloss_view::gloss_recent_sheet(theme));
            css.extend(crate::gloss_view::gloss_minimap_sheet(theme));
            css
        };
        crate::bake_scheme_pair(
            side(&self.shared.presentation.chrome_theme_light),
            side(&self.shared.presentation.chrome_theme_dark),
        )
    }

    /// Lay this frame's content band out into pane rects: carve the shellbar strip, lay
    /// the frame tree's leaves + dividers, resolve the focused Orrery pane (rect, graph,
    /// size), pick out the workbench / roster / comms / folded-list-pane rects, and build
    /// the focused-field text cursor. Returns them bundled in [`FrameRects`]. (Extracted
    /// from `render()` — frame layout.)
    pub(super) fn compute_layout_rects(&self, w: u32, h: u32, toolbar_h: u32) -> FrameRects {
        // Frame tree: the content band (below the toolbar) split into pane rects.
        // The shellbar strip is carved out of the band first; the frame tree fills
        // the remainder. A slim (leaf) window has no shellbar, and a hidden shellbar
        // carves nothing either, so the band is the whole area below the toolbar.
        // (Shellbar F2.1; MW3 step 4; hide-shellbar.)
        let band = if self.view.kind.is_slim() || self.shared.presentation.shellbar_hidden {
            [0.0, toolbar_h as f32, w as f32, h as f32]
        } else {
            shellbar::band_after_shellbar(
                self.shared.presentation.shellbar_edge,
                w as f32,
                h as f32,
                toolbar_h as f32,
                self.shared.presentation.ui_scale(),
            )
        };
        let leaves =
            frame_view::leaf_rects(&self.view.frame_layout, band, self.view.maximized_pane);
        // The orrery is the always-present graph pane; the tiled workbench is its
        // summonable sibling. Each renders into its own leaf. (Workbench-as-pane.)
        // The *focused* Orrery leaf (bound to focused_graph) is the primary one — it
        // gets the full drive (node colouring, cards, centring); the rest render as
        // secondaries. Falls back to the first Orrery leaf. (Pane-as-unit.)
        let focused_gid = self.view.focused_graph;
        let orrery_leaf = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Orrery) && l.graph_id == focused_gid)
            .or_else(|| {
                leaves
                    .iter()
                    .find(|l| matches!(l.content, PaneContent::Orrery))
            });
        let orrery_rect = orrery_leaf.map(|l| l.rect).unwrap_or(band);
        // The graph this Orrery pane resolves to (its leaf's graph_id) — render
        // drives *that* pooled orrery, not the window-global one, so a second
        // Orrery pane of another graph would drive its own. (Window composition P2.)
        let orrery_gid = orrery_leaf
            .map(|l| l.graph_id)
            .unwrap_or(self.view.focused_graph);
        let workbench_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Workbench))
            .map(|l| l.rect);
        let roster_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Roster))
            .map(|l| l.rect);
        let gloss_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Gloss))
            .map(|l| l.rect);
        let comms_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Comms))
            .map(|l| l.rect);
        // The five folded list panes' rects, in `ShellListPane` order (apparatus, steward,
        // inspector, trail, alembic), for the per-frame snapshot into the shell document. (Phase 1.)
        let list_pane_rects: [Option<[f32; 4]>; 5] = [
            leaves
                .iter()
                .find(|l| matches!(l.content, PaneContent::Apparatus))
                .map(|l| l.rect),
            leaves
                .iter()
                .find(|l| matches!(l.content, PaneContent::Steward))
                .map(|l| l.rect),
            leaves
                .iter()
                .find(|l| matches!(l.content, PaneContent::Inspector))
                .map(|l| l.rect),
            leaves
                .iter()
                .find(|l| matches!(l.content, PaneContent::Trail))
                .map(|l| l.rect),
            leaves
                .iter()
                .find(|l| matches!(l.content, PaneContent::Alembic))
                .map(|l| l.rect),
        ];
        let dividers =
            frame_view::divider_rects(&self.view.frame_layout, band, self.view.maximized_pane);
        let orrery_w = (orrery_rect[2] - orrery_rect[0]).round().max(1.0) as u32;
        let orrery_h = (orrery_rect[3] - orrery_rect[1]).round().max(1.0) as u32;

        // Chrome scene over the full window. Paint the caret / selection of the
        // focused field — the palette query when open, else the omnibar (byte
        // offsets from the field's char model).
        let cursor = self.multi.focus(self.view.projection_id).map(|node| {
            let field = self.caret_field(node);
            let byte_of = |i: usize| {
                field
                    .text()
                    .char_indices()
                    .nth(i)
                    .map(|(b, _)| b)
                    .unwrap_or(field.text().len())
            };
            let selection = field.has_selection().then(|| {
                let (s, e) = field.selection();
                (byte_of(s), byte_of(e))
            });
            TextCursor {
                node,
                caret: field.caret_byte_in_render(),
                selection,
                editable: self.is_text_input(node),
            }
        });
        FrameRects {
            leaves,
            orrery_rect,
            orrery_gid,
            workbench_rect,
            roster_rect,
            gloss_rect,
            comms_rect,
            list_pane_rects,
            dividers,
            orrery_w,
            orrery_h,
            cursor,
        }
    }
}
