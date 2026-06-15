/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render, resize, and toolbar-measurement for [`Shell`](super::Shell). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use forme::GraphMemberId;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use netrender::ColorLoad;
use netrender::external_texture::ExternalTexturePlacement;
use crate::serval_render::TextCursor;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;

use std::cell::RefCell;
use std::time::Instant;

use super::fetch::{ContentState, Fetched};
use super::resources::{ResourceLoader, ResourceStore};
use frame::{PaneContent, SessionId};
use session_runtime::SwitcherThumbnail;

use super::{
    CARD_BG, FALLBACK_TOOLBAR_H, WindowCtx, all_with_class, first_with_class, frame_view, shellbar,
    measure_class_bottom, member_attr,
};
use meerkat::ShellbarPaneStates;
use crate::pane_session::PaneSession;

impl WindowCtx<'_> {
    /// The toolbar-band height (px), measuring + caching it on first use. The
    /// toolbar is a single flex row, so its border-box height is independent of
    /// the available width/height; measuring once suffices. Used to place the
    /// content root directly below the toolbar.
    pub(super) fn toolbar_height(&mut self) -> u32 {
        if self.view.toolbar_h == 0 {
            let sheet = self.shared.presentation.chrome_sheet_refs();
            let measured = measure_class_bottom(
                &self.view.dom.borrow(),
                &sheet,
                self.view.width,
                self.view.height,
                "toolbar",
            )
            .unwrap_or(FALLBACK_TOOLBAR_H);
            self.view.toolbar_h = measured;
        }
        self.view.toolbar_h
    }

    /// Reconfigure the surface for `(width, height)` and request a redraw.
    pub(super) fn resize(&mut self, width: u32, height: u32) {
        self.view.width = width.max(1);
        self.view.height = height.max(1);
        if let (Some(surface), Some(core)) = (self.view.surface.as_mut(), self.render_core) {
            surface.resize(core, self.view.width, self.view.height);
        }
        self.refresh_a11y_summary();
        self.view.request_redraw();
    }

    /// Render the two authorities and present them. The orrery content root fills
    /// everything below the toolbar; the chrome root is rendered over the full
    /// window with a *transparent* clear, so its toolbar band and any open
    /// dropdown float above the content while the rest lets the orrery show
    /// through. Composite order is content first, then chrome on top.
    pub(super) fn render(&mut self) {
        if self.view.surface.is_none() || self.render_core.is_none() {
            return;
        }
        // C0 baseline (cheap-path plan): time the whole frame + the always-present
        // chrome pipeline. Enable with `RUST_LOG=meerkat::profile=debug` and drive a
        // representative interaction. Per-pane granularity (roster/apparatus/utility,
        // each conditional) is the documented C0 refinement on top of this headline.
        let frame_t = Instant::now();
        let chrome_us: u128;
        let (w, h) = (self.view.width.max(1), self.view.height.max(1));
        let toolbar_h = self.toolbar_height().min(h);

        // Reserve / drop the Comms frame leaf to match the chrome's comms-open state
        // before laying the panes out, so the other panes make room for it. (Comms.)
        self.sync_comms_pane();

        // Sync shellbar pane-open states + edge into Chrome before the runner so
        // the view rebuilds with current active states. (Shellbar F2.1.)
        let sb_panes = ShellbarPaneStates {
            workbench: self.pane_of_content(&PaneContent::Workbench).is_some(),
            roster: self.pane_of_content(&PaneContent::Roster).is_some(),
            gloss: self.pane_of_content(&PaneContent::Gloss).is_some(),
            apparatus: self.pane_of_content(&PaneContent::Apparatus).is_some(),
            inspector: self.pane_of_content(&PaneContent::Inspector).is_some(),
            steward: self.pane_of_content(&PaneContent::Steward).is_some(),
            comms: self.pane_of_content(&PaneContent::Comms).is_some(),
        };
        let sb_edge = self.shared.presentation.shellbar_edge;
        if self.view.runner.state().shellbar_panes != sb_panes
            || self.view.runner.state().shellbar_edge != sb_edge
        {
            self.view.runner.update(move |c| {
                c.shellbar_panes = sb_panes;
                c.shellbar_edge = sb_edge;
            });
        }

        // Frame tree: the content band (below the toolbar) split into pane rects.
        // The shellbar strip is carved out of the band first; the frame tree fills
        // the remainder. A slim (leaf) window has no shellbar, so the band is the
        // whole area below the toolbar. (Shellbar F2.1; MW3 step 4.)
        let band = if self.view.kind.is_slim() {
            [0.0, toolbar_h as f32, w as f32, h as f32]
        } else {
            shellbar::band_after_shellbar(
                self.shared.presentation.shellbar_edge,
                w as f32,
                h as f32,
                toolbar_h as f32,
            )
        };
        let leaves = frame_view::leaf_rects(&self.view.frame_layout, band, self.view.maximized_pane);
        // The orrery is the always-present graph pane; the tiled workbench is its
        // summonable sibling. Each renders into its own leaf. (Workbench-as-pane.)
        // The *focused* Orrery leaf (bound to focused_graph) is the primary one — it
        // gets the full drive (node colouring, cards, centring); the rest render as
        // secondaries. Falls back to the first Orrery leaf. (Pane-as-unit.)
        let focused_gid = self.view.focused_graph;
        let orrery_leaf = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Orrery) && l.graph_id == focused_gid)
            .or_else(|| leaves.iter().find(|l| matches!(l.content, PaneContent::Orrery)));
        let orrery_rect = orrery_leaf.map(|l| l.rect).unwrap_or(band);
        // The graph this Orrery pane resolves to (its leaf's graph_id) — render
        // drives *that* pooled orrery, not the window-global one, so a second
        // Orrery pane of another graph would drive its own. (Window composition P2.)
        let orrery_gid = orrery_leaf.map(|l| l.graph_id).unwrap_or(self.view.focused_graph);
        let workbench_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Workbench))
            .map(|l| l.rect);
        let roster_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Roster))
            .map(|l| l.rect);
        let comms_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Comms))
            .map(|l| l.rect);
        let dividers = frame_view::divider_rects(&self.view.frame_layout, band, self.view.maximized_pane);
        let orrery_w = (orrery_rect[2] - orrery_rect[0]).round().max(1.0) as u32;
        let orrery_h = (orrery_rect[3] - orrery_rect[1]).round().max(1.0) as u32;

        // Chrome scene over the full window. Paint the caret / selection of the
        // focused field — the palette query when open, else the omnibar (byte
        // offsets from the field's char model).
        let cursor = self.view.runner.focus().map(|node| {
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
            }
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        // The chrome's own scroll (the command palette follows its selection); kept
        // separate from the content `scroll` so a cmd-list offset never bleeds into
        // another pane's DOM.
        let mut chrome_scroll = ScrollOffsets::<NodeId>::default();
        if self.view.runner.state().palette_open {
            // Bound the list to the window so a long palette can't overflow it. The
            // overlay floats the panel ~56px down with an input + paddings above the
            // list, so leave generous headroom + a bottom margin — otherwise a small
            // window pushes the last rows past its edge even when scrolled.
            let max_h = (h as f32 - 200.0).max(120.0);
            {
                let mut dom = self.view.dom.borrow_mut();
                let root = dom.document();
                if let Some(node) = first_with_class(&dom, root, "cmd-list") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(node, attr, &format!("overflow: scroll; max-height: {max_h}px;"));
                }
            }
            // Follow the selection: centre the active row in the bounded viewport,
            // from the prior frame's layout (one-frame lag, like the roster clamp).
            if let Some(session) = &self.view.chrome_session {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                if let (Some(list), Some(active)) = (
                    first_with_class(&dom, root, "cmd-list"),
                    first_with_class(&dom, root, "cmd-row-active"),
                ) {
                    if let (Some(lr), Some(ar)) = (frags.rect_of(list), frags.rect_of(active)) {
                        let viewport_h = lr.size.height;
                        let content_h = lr.content_size.height;
                        let target = (ar.location.y + ar.size.height / 2.0 - viewport_h / 2.0)
                            .clamp(0.0, (content_h - viewport_h).max(0.0));
                        chrome_scroll.insert(list, (0.0, target));
                    }
                }
            }
        }
        // Position the shellbar strip at its docked edge. The flex-direction
        // follows the edge so buttons stack vertically (Left/Right) or
        // horizontally (Top/Bottom). (Shellbar F2.1.)
        {
            let sr = shellbar::shellbar_rect(self.shared.presentation.shellbar_edge, w as f32, h as f32, toolbar_h as f32);
            let flex_dir = match self.shared.presentation.shellbar_edge {
                session_runtime::ShellbarEdge::Left | session_runtime::ShellbarEdge::Right => {
                    "column"
                }
                session_runtime::ShellbarEdge::Top | session_runtime::ShellbarEdge::Bottom => {
                    "row"
                }
            };
            let mut dom = self.view.dom.borrow_mut();
            let root = dom.document();
            if let Some(node) = first_with_class(&dom, root, "shellbar") {
                let style = format!(
                    "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; flex-direction: {};",
                    sr[0],
                    sr[1],
                    (sr[2] - sr[0]).max(0.0),
                    (sr[3] - sr[1]).max(0.0),
                    flex_dir,
                );
                let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                dom.set_attribute(node, attr, &style);
            }
        }
        // Position the chrome's comms overlay into its frame leaf (it's chrome-
        // rendered but laid out by the frame tree): set the geometry inline so it
        // fills the reserved Comms leaf rect. (Comms pane.)
        if let Some(cr) = comms_rect {
            let mut dom = self.view.dom.borrow_mut();
            let root = dom.document();
            if let Some(node) = first_with_class(&dom, root, "comms-pane") {
                let style = format!(
                    "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px;",
                    cr[0],
                    cr[1],
                    (cr[2] - cr[0]).max(0.0),
                    (cr[3] - cr[1]).max(0.0),
                );
                let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                dom.set_attribute(node, attr, &style);
            }
        }
        let chrome_sheet = self.shared.presentation.chrome_sheet_refs();
        let chrome_t = Instant::now();
        // C3 (cheap-path): render the chrome through its persistent
        // `IncrementalLayout` session — the session drains this frame's mutations,
        // rebuilds only on a structural / resize / theme frame, and otherwise
        // restyles incrementally (RepaintOnly, no relayout). Same Scene for a given
        // DOM as the old per-frame `scene_from_scripted_dom`.
        let chrome_scene = PaneSession::scene(
            &mut self.view.chrome_session,
            &self.view.dom,
            &chrome_sheet,
            w,
            h,
            cursor,
            &chrome_scroll,
        );
        chrome_us = chrome_t.elapsed().as_micros();

        // Color the orrery's nodes by activation state (green open / red closed /
        // blue new) so the graph shows at a glance what's live. (Visible in
        // Cartography; the orrery is hidden in the tiled view.)
        let states = self.node_states();
        self.pane_orrery_mut(orrery_gid).set_node_states(states);
        // Shape each node by its content type (square document / rounded menu /
        // circle feed), the same per-node-hint path as the color states.
        let shapes = self.node_shapes();
        self.pane_orrery_mut(orrery_gid).set_node_shapes(shapes);

        // The orrery always composites its own scene into its leaf (kept in sync,
        // centered once). The tiled workbench, when its pane is open, composites a
        // separate scene into its own leaf — the two coexist now, no longer toggled.
        self.pane_orrery_mut(orrery_gid).resize(orrery_w, orrery_h);
        if !self.view.centered {
            self.pane_orrery_mut(orrery_gid).recenter();
            self.view.centered = true;
        } else if !self.view.healed && self.pane_orrery(orrery_gid).has_nodes() {
            // One-shot self-heal: a restored camera that frames nothing (a
            // degenerate saved pan/zoom) snaps back to the graph. Gated on
            // has_nodes() so it waits for the async session load — firing against
            // the still-empty graph would spend the one shot while graph_visible()
            // is trivially true, leaving the restored degenerate camera in place
            // once the nodes actually arrive. Checked once, so it never fights an
            // intentional pan into empty space.
            self.view.healed = true;
            if !self.pane_orrery(orrery_gid).graph_visible() {
                self.pane_orrery_mut(orrery_gid).recenter();
            }
        }
        let (orrery_scene, orrery_redraw) = self.pane_orrery_mut(orrery_gid).frame(orrery_w, orrery_h);
        // P2 per-pane render: a second graph-pane (Shift+click a switcher tile)
        // drives its own pooled orrery into its own leaf, beside the focused one,
        // so two graphs show at once. Node coloring + the focused-node card stay
        // on the primary pane for now; this draws each extra graph live. (Window
        // composition P2 — second graph-pane.)
        let secondary_orreries: Vec<(netrender::Scene, [f32; 4], u32, u32)> = leaves
            .iter()
            .filter(|l| matches!(l.content, PaneContent::Orrery) && l.graph_id != orrery_gid)
            .map(|l| {
                let sw = (l.rect[2] - l.rect[0]).round().max(1.0) as u32;
                let sh = (l.rect[3] - l.rect[1]).round().max(1.0) as u32;
                let orrery = self.pane_orrery_mut(l.graph_id);
                orrery.resize(sw, sh);
                if !orrery.graph_visible() {
                    orrery.recenter();
                }
                let (scene, _) = orrery.frame(sw, sh);
                (scene, l.rect, sw, sh)
            })
            .collect();
        // The workbench pane renders through the pelt `TileSurface` (V6): meerkat owns
        // the `Workbench` (the authority), projects it onto pelt's tile-tree contract
        // each frame, drives the surface, and composites each member's actor texture
        // into the surface's reported tile rects below. `workbench_scene` is the
        // surface's frame (tab bars + dividers); `None` when the pane isn't open.
        let mut workbench_scene: Option<(netrender::Scene, u32, u32)> = None;
        // The surface's external-texture tile rects this frame, `(tile, (x,y,w,h), key)`
        // in surface-local px — carried to the placement step below.
        let mut workbench_external: Vec<(
            pelt_core::tile::TileId,
            (f32, f32, f32, f32),
            pelt_core::tile::TextureKey,
        )> = Vec::new();
        if let Some(wr) = workbench_rect {
            let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
            let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
            // Each open member projects to an external-texture tile, keyed by its UUID
            // low 64 bits so the surface's reported key maps back to the member; titles
            // come from the graph node's URL.
            let titles: std::collections::HashMap<GraphMemberId, String> = self
                .view
                .workbench
                .open_members()
                .iter()
                .filter_map(|&m| {
                    self.orrery().graph().get_node_by_id(m).map(|(_, n)| (m, n.url().to_string()))
                })
                .collect();
            let tree = self.view.workbench.to_tile_tree(|m| {
                let key = m.as_u128() as u64;
                pelt_core::tile::Tile {
                    id: pelt_core::tile::TileId(key),
                    title: titles.get(&m).cloned().unwrap_or_default(),
                    content: pelt_core::tile::ContentSource::ExternalTexture(
                        pelt_core::tile::TextureKey(key),
                    ),
                }
            });
            if let Some(tree) = tree {
                // Host-authority: set the projected tree (the surface is a driven view),
                // then render its frame. Created lazily on the first tiled frame.
                match self.view.pelt_surface.as_mut() {
                    Some(s) => s.set_tree(tree),
                    None => self.view.pelt_surface = Some(pelt_desktop::TileSurface::new(tree)),
                }
                let frame = self.view.pelt_surface.as_mut().unwrap().frame(ww, wh);
                workbench_external = frame.external_tiles;
                workbench_scene = Some((frame.frame_scene, ww, wh));
            }
        }

        // Reconcile the active-node pool to what this frame shows — the open tiles
        // (Tree) or the focused node (Cartography). Needed-but-dormant nodes spawn
        // an actor; active nodes no longer shown are reaped, unless backgrounded.
        let gid = self.view.focused_graph;
        let needed: Vec<_> = self.needed_members().into_iter().map(|m| (m, gid)).collect();
        self.shared.content.constellation.reconcile(&needed);

        // Content cards floating over the band: one per laid-out tile in Tree, the
        // focused-node card at `card_rect` in Cartography. Each entry is
        // `(member, window dest rect, raster size)`; the scene comes from that
        // node's activation at composite time. Driving an activation re-renders it
        // off the UI thread only when its document or size changed.
        let mut cards: Vec<(GraphMemberId, [f32; 4], (u32, u32))> = Vec::new();
        // The "unvisited" placeholder card (focused node, no snapshot yet): it has
        // no constellation scene, so it composites on its own path below.
        let mut unvisited_card: Option<(GraphMemberId, [f32; 4])> = None;
        // The "last visit" snapshot card (focused, visited, not live): rendered
        // host-side from the durable cache / synthesis (Card #4), so it also
        // composites on its own path below. `(member, url, dest rect, scene)` —
        // the scene is `Some` only when it must be (re)rasterized this frame; once
        // its texture is cached by url, later frames carry `None`.
        let mut snapshot_card: Option<(
            GraphMemberId,
            String,
            [f32; 4],
            Option<(netrender::Scene, u32)>,
        )> = None;
        let mut live_card: Option<(GraphMemberId, [f32; 4])> = None;
        // The focused card when its node is pinned to the compatibility view:
        // the system WebView's imported texture composites at this rect instead
        // of a constellation scene. (Scrying tile plan, X1.)
        let mut scrying_card: Option<(GraphMemberId, [f32; 4])> = None;
        if let Some(wr) = workbench_rect {
            // Read each content placeholder's laid-out rect + member out of the
            // workbench DOM (taffy laid it out above), then drive that tile's actor
            // and queue it to composite at that rect. taffy layouts are
            // *parent-relative*, so sum the workbench > slot > content chain for an
            // absolute rect — otherwise every slot's content reports the same
            // slot-local origin and the tiles stack on each other. The collect
            // releases the DOM borrow before we mutate self. (member, content rect,
            // full slot rect) in window coords, offset by the workbench leaf origin.
            let placements: Vec<(GraphMemberId, [f32; 4], [f32; 4])> = {
                let (ox, oy) = (wr[0], wr[1]);
                // Map the surface's external-texture tile rects (surface-local px) to
                // window coords + their members: the surface reports each tile's content
                // rect + the host's key, and the key is the member's UUID low 64 bits.
                let members = self.view.workbench.open_members();
                workbench_external
                    .iter()
                    .filter_map(|(_tile, rect, key)| {
                        let member = members.iter().copied().find(|m| m.as_u128() as u64 == key.0)?;
                        let r = [ox + rect.0, oy + rect.1, ox + rect.0 + rect.2, oy + rect.1 + rect.3];
                        // The surface gives one content rect per tile (below the tab
                        // bar); use it as both the content rect (actor composite) and
                        // the slot rect (drag target).
                        Some((member, r, r))
                    })
                    .collect()
            };
            let mut slot_rects = Vec::with_capacity(placements.len());
            for (member, content, slot) in placements {
                slot_rects.push((member, slot));
                let Some(url) = self
                    .orrery()
                    .graph()
                    .get_node_by_id(member)
                    .map(|(_, n)| n.url().to_string())
                else {
                    continue;
                };
                self.ensure_content(&url);
                let cw = (content[2] - content[0]).round().max(1.0) as u32;
                let ch = (content[3] - content[1]).round().max(1.0) as u32;
                let state = self.shared.content.pages.get(&url).cloned();
                self.shared.content.constellation.drive(member, &url, state, cw, ch);
                cards.push((member, content, (cw, ch)));
            }
            self.view.tile_rects = slot_rects;
        } else {
            self.view.tile_rects.clear(); // no tile drag targets when the pane is closed
        }
        // Reap every compat tile except the one about to be shown (the focused, pinned,
        // live-preview node). A deselected / unpinned compat node is torn down here
        // (reap-on-deselect, the X3 lifecycle Mark chose), so its WebView can't freeze
        // on screen and the window's single composition target is freed for the next. (X2.)
        let shown_compat = self.focused_member().filter(|m| {
            self.view.live_previews.contains(m) && self.shared.content.compat_pins.contains(m)
        });
        self.view.scrying.reap_except(shown_compat);
        // The orrery's focused-node card (always, alongside any workbench pane).
        if let (Some(member), Some(url)) = (
            self.focused_member(),
            self.orrery().focused_url().map(str::to_string),
        ) {
            // Float the card next to the focused node (fall back to the fixed
            // top-right rect when the node's screen pos is unknown). A live preview
            // is a medium card the actor renders into; a snapshot is a shorter
            // peek at the retained scene, no actor. A node with neither (never
            // visited this session) shows no card yet. (Card system P2/P3.)
            // The orrery reports the node in its own (leaf-local) viewport; offset
            // by the orrery leaf's origin for window coords, and anchor the card
            // within the orrery leaf rect (so it stays in the orrery pane when split).
            let node = self
                .orrery()
                .focused_node_screen()
                .map(|(nx, ny)| (orrery_rect[0] + nx, orrery_rect[1] + ny));
            if self.view.live_previews.contains(&member) {
                const LIVE_W: u32 = 300;
                const LIVE_H: u32 = 400;
                let rect = node
                    .and_then(|(nx, ny)| {
                        super::card::anchored_card_rect(nx, ny, LIVE_W, LIVE_H, orrery_rect)
                    })
                    .or_else(|| super::card::card_rect(orrery_rect));
                if let Some((x0, y0, x1, y1, cw, ch)) = rect {
                    if self.shared.content.compat_pins.contains(&member) {
                        // Compatibility view: the system WebView renders this
                        // node; drive the UI-thread scrying pool (spawn /
                        // resize / navigate + non-blocking frame import)
                        // instead of a content actor.
                        if let (Some(window), Some(core)) =
                            (self.view.window.as_ref(), self.render_core)
                        {
                            let window = window.clone();
                            let device = core.device().clone();
                            let queue = core.queue().clone();
                            let session_dir = self.shared.session.session_dir.clone();
                            self.view.scrying.drive(
                                member,
                                &url,
                                cw,
                                ch,
                                (x0, y0),
                                &window,
                                &device,
                                &queue,
                                &session_dir,
                            );
                        }
                        scrying_card = Some((member, [x0, y0, x1, y1]));
                        live_card = Some((member, [x0, y0, x1, y1]));
                        // The WebView paints on its own schedule; keep frames
                        // coming while the card is visible.
                        self.view.request_redraw();
                    } else {
                        self.ensure_content(&url);
                        let state = self.shared.content.pages.get(&url).cloned();
                        self.shared.content.constellation.drive(member, &url, state, cw, ch);
                        cards.push((member, [x0, y0, x1, y1], (cw, ch)));
                        live_card = Some((member, [x0, y0, x1, y1]));
                    }
                }
            } else if self.orrery().member_visited(member) {
                // "Last visit" snapshot: a small fixed-size card, rendered host-side
                // from the durable cache / synthesis below (no actor), so it survives
                // a restart. Composited uniform-scaled into a scrollable thumbnail.
                const SNAP_W: u32 = 200;
                const SNAP_H: u32 = 260;
                let rect = node
                    .and_then(|(nx, ny)| {
                        super::card::anchored_card_rect(nx, ny, SNAP_W, SNAP_H, orrery_rect)
                    })
                    .or_else(|| super::card::card_rect(orrery_rect));
                if let Some((x0, y0, x1, y1, _, _)) = rect {
                    // Render the snapshot scene from cache / synthesis once per url;
                    // `None` means its texture is already cached (composited below).
                    let built = if self.view.snapshot_textures.contains_key(&url) {
                        None
                    } else {
                        const RENDER_W: u32 = 300;
                        const RENDER_H: u32 = 600;
                        let state = self.load_cached(&url).map(|c| {
                            ContentState::Ready(Fetched {
                                content_type: c.content_type,
                                body: String::from_utf8_lossy(&c.body).into_owned(),
                            })
                        });
                        let store = RefCell::new(ResourceStore::default());
                        let wanted = RefCell::new(Vec::new());
                        let loader = ResourceLoader::new(&store, &url, &wanted);
                        // A snapshot is a non-interactive peek (no actor, no
                        // content_rects entry), so its link map is dropped here —
                        // link nav rides the live actor cards. (Inline-link nav.)
                        let (scene, content_height, _links) = super::card::render_content_scene(
                            &url,
                            state.as_ref(),
                            &self.shared.content.engine_registry,
                            &loader,
                            RENDER_W,
                            RENDER_H,
                        );
                        Some((scene, content_height))
                    };
                    snapshot_card = Some((member, url, [x0, y0, x1, y1], built));
                }
            } else {
                // Never visited this session: a small dashed "Double-click to load"
                // placeholder, anchored like the other cards. Double-clicking it
                // promotes to a live preview (same as a snapshot). Composited on its
                // own path below (no constellation scene).
                const UNVIS_W: u32 = 200;
                const UNVIS_H: u32 = 120;
                unvisited_card = node
                    .and_then(|(nx, ny)| {
                        super::card::anchored_card_rect(nx, ny, UNVIS_W, UNVIS_H, orrery_rect)
                    })
                    .map(|(x0, y0, x1, y1, _, _)| (member, [x0, y0, x1, y1]));
            }
        }

        // Record each card's on-screen content rect so a wheel over it scrolls the
        // card (resolved in the wheel handler) rather than panning the orrery. The
        // unvisited placeholder counts as a card too, so a double-click over it
        // promotes (and a click on it doesn't deselect the node).
        self.view.content_rects = cards
            .iter()
            .map(|(member, dest, _)| (*member, *dest))
            .collect();
        if let Some((member, rect)) = unvisited_card {
            self.view.content_rects.push((member, rect));
        }
        if let Some((member, _, rect, _)) = &snapshot_card {
            self.view.content_rects.push((*member, *rect));
        }
        if let Some((member, rect)) = scrying_card {
            self.view.content_rects.push((member, rect));
        }

        // The omnibar follows focus: point it at the focused tile / node when that
        // changed (next frame, like the chrome strips were — the scene above is
        // already built).
        self.sync_location();
        // Back/forward enabled-state tracks the focused node's own history.
        self.sync_nav_buttons();
        self.drain_portable_diagnostics();

        let apparatus_data = if self.apparatus_leaf_rect().is_some() {
            Some((self.apparatus_system_rows(), self.apparatus_observability()))
        } else {
            None
        };

        // The shared core (rasterize / compose) + this window's surface (acquire /
        // format); both checked present at the method entry. (MW3: one device, N surfaces.)
        let core = self.render_core.expect("render core present");
        let surface = self.view.surface.as_ref().expect("window surface present");
        let (_chrome_tex, chrome_view) = core.rasterize(
            &chrome_scene,
            w,
            h,
            ColorLoad::Clear(wgpu::Color::TRANSPARENT),
        );
        // The orrery paints its own opaque backdrop, but clear to the same dark
        // tone so a resize frame cannot flash white before the backdrop lands.
        let backdrop = wgpu::Color {
            r: 0.067,
            g: 0.078,
            b: 0.100,
            a: 1.0,
        };
        let (_orrery_tex, orrery_view) = core.rasterize(
            &orrery_scene,
            orrery_w,
            orrery_h,
            ColorLoad::Clear(backdrop),
        );
        // Rasterize each secondary graph-pane's scene. The textures must outlive
        // the composite below (they back the views), so they are held in this Vec
        // until the command buffer is submitted. (Window composition P2.)
        let secondary_textures: Vec<(wgpu::Texture, wgpu::TextureView, [f32; 4])> =
            secondary_orreries
                .iter()
                .map(|(scene, rect, sw, sh)| {
                    let (tex, view) = core.rasterize(scene, *sw, *sh, ColorLoad::Clear(backdrop));
                    (tex, view, *rect)
                })
                .collect();
        // Rasterize the workbench pane scene too, when its pane is open. The tex is
        // bound to `_workbench_tex` so it outlives the composite below.
        let (_workbench_tex, workbench_view) = match workbench_scene.as_ref() {
            Some((scene, ww, wh)) => {
                let (tex, view) = core.rasterize(scene, *ww, *wh, ColorLoad::Clear(backdrop));
                (Some(tex), Some(view))
            }
            None => (None, None),
        };
        // Rasterize each tile's scene to an offscreen texture only when its version
        // or size changed; reuse the cached texture otherwise, so an unchanged tile
        // is not re-rasterized every frame (the cost that scaled with tile count).
        // The cache (self.view.tile_textures) keeps the textures alive across frames; evict
        // closed tiles first so theirs free. `composite` is what to draw, in order.
        self.view.tile_textures
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        // Rasterize each card at its FULL content height (clamped to the cap), so
        // scrolling is a GPU UV-window shift over the cached tall texture rather
        // than a re-layout per tick (the gpui-flavored path).
        const MAX_CARD_TEX_H: u32 = 8192;
        let mut composite: Vec<([f32; 4], GraphMemberId)> = Vec::with_capacity(cards.len());
        for (member, dest, (cw, ch)) in &cards {
            // A live tile/preview bumps scene_version each scene; a static snapshot
            // has version 0, so its texture rasterizes once and then stays cached.
            let version = self.shared.content.constellation.scene_version(*member);
            let tex_h = self.shared.content.constellation
                .content_height(*member)
                .max(*ch)
                .min(MAX_CARD_TEX_H);
            let fresh = self
                .view
                .tile_textures
                .get(member)
                .is_some_and(|c| c.version == version && c.size == (*cw, tex_h));
            if !fresh {
                if let Some(scene) = self.shared.content.constellation.scene(*member) {
                    let (tex, view) = core.rasterize(scene, *cw, tex_h, ColorLoad::Clear(CARD_BG));
                    self.view.tile_textures.insert(
                        *member,
                        super::CachedTile {
                            version,
                            size: (*cw, tex_h),
                            tex,
                            view,
                        },
                    );
                }
            }
            if self.view.tile_textures.contains_key(member) {
                composite.push((*dest, *member));
            }
        }

        let Some(frame) = surface.acquire(core) else { return };
        let target_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let format = surface.format();
        // Content fills [toolbar_h, h] (dest_rect is [x0, y0, x1, y1] corners;
        // viewport is the full surface). Then each content card floats over it, and
        // the transparent-cleared chrome composites over the whole window — toolbar
        // + dropdown on top, the rest letting the content through.
        core.renderer().compose_external_texture(
            &orrery_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new(orrery_rect),
        );
        // Composite each secondary graph-pane's orrery into its own leaf. (Window
        // composition P2 — two graphs side by side.)
        for (_tex, view, rect) in &secondary_textures {
            core.renderer().compose_external_texture(
                view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*rect),
            );
        }
        if let (Some(wb_view), Some(wr)) = (&workbench_view, workbench_rect) {
            core.renderer().compose_external_texture(
                wb_view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(wr),
            );
        }
        for (dest, member) in &composite {
            let Some(cached) = self.view.tile_textures.get(member) else {
                continue;
            };
            // Scroll is a vertical UV window over the tall cached texture — a GPU
            // sample shift, no re-raster. Clamp the offset to the scrollable range.
            let tex_w = cached.size.0 as f32;
            let tex_h = cached.size.1 as f32;
            let dest_w = (dest[2] - dest[0]).max(1.0);
            let dest_h = (dest[3] - dest[1]).max(1.0);
            // Height of the texture slice shown, sized so the vertical scale equals
            // the horizontal one (tex_w -> dest_w): a uniform downscale for snapshot
            // thumbnails, and a no-op (= dest_h) for 1:1 live cards / tiles.
            let visible_h = dest_h * tex_w / dest_w;
            let max_scroll = (tex_h - visible_h).max(0.0);
            let scroll = self
                .view
                .scroll
                .get(member)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, max_scroll);
            let uv = [0.0, scroll / tex_h, 1.0, (scroll + visible_h) / tex_h];
            core.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*dest).with_uv(uv),
            );
        }
        // The compatibility-view card: the imported WebView texture at the card
        // rect. No UV window — the WebView scrolls itself. The rect is recorded so
        // the input path can forward mouse / wheel / keys into the WebView. (X2.)
        self.view.scrying_rect = scrying_card;
        if let Some((member, dest)) = scrying_card {
            if let Some(view) = self.view.scrying.texture_view(member) {
                core.renderer().compose_external_texture(
                    view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new(dest),
                );
            }
        }
        // Live cards carry a close (X) button at their top-right corner. Rasterize
        // the shared button texture once, composite it on each live card, and
        // record its rect so a press there reaps the live preview. (Card system.)
        // The X sits on the orrery's live-preview card only (`live_card`); tiles in
        // the workbench pane have their own tab close, so the button never lands on
        // a tile even when the same node is both previewed and tiled.
        self.view.close_button_rects.clear();
        if let Some((member, dest)) = live_card {
            let btn = super::card::CLOSE_BTN;
            let inset = super::card::CLOSE_BTN_INSET;
            let size = btn.round().max(1.0) as u32;
            if self.view.close_button_tex.as_ref().map(|c| c.size) != Some((size, size)) {
                let scene = super::card::close_button_scene(size);
                let (tex, view) = core.rasterize(
                    &scene,
                    size,
                    size,
                    ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                );
                self.view.close_button_tex = Some(super::CachedTile {
                    version: 0,
                    size: (size, size),
                    tex,
                    view,
                });
            }
            if let Some(cached) = &self.view.close_button_tex {
                let bx1 = dest[2] - inset;
                let bx0 = bx1 - btn;
                let by0 = dest[1] + inset;
                let by1 = by0 + btn;
                core.renderer().compose_external_texture(
                    &cached.view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new([bx0, by0, bx1, by1]),
                );
                self.view.close_button_rects.push((member, [bx0, by0, bx1, by1]));
            }
        }
        // The "last visit" snapshot card: rasterize the host-rendered scene once
        // per url (cached), then composite uniform-scaled into the small dest with
        // the same vertical-scroll UV window as the other cards.
        if let Some((member, url, rect, built)) = snapshot_card {
            if let Some((scene, content_h)) = built {
                let tex_h = content_h.max(1).min(MAX_CARD_TEX_H);
                let (tex, view) = core.rasterize(&scene, 300, tex_h, ColorLoad::Clear(CARD_BG));
                self.view.snapshot_textures.insert(
                    url.clone(),
                    super::CachedTile {
                        version: 0,
                        size: (300, tex_h),
                        tex,
                        view,
                    },
                );
            }
            if let Some(cached) = self.view.snapshot_textures.get(&url) {
                let tex_w = cached.size.0 as f32;
                let tex_h = cached.size.1 as f32;
                let dest_w = (rect[2] - rect[0]).max(1.0);
                let dest_h = (rect[3] - rect[1]).max(1.0);
                let visible_h = dest_h * tex_w / dest_w;
                let max_scroll = (tex_h - visible_h).max(0.0);
                let scroll = self
                    .view
                    .scroll
                    .get(&member)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, max_scroll);
                let uv = [0.0, scroll / tex_h, 1.0, (scroll + visible_h) / tex_h];
                core.renderer().compose_external_texture(
                    &cached.view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new(rect).with_uv(uv),
                );
            }
        }
        // The unvisited placeholder card: rasterize its (static) scene once per
        // size and composite it at the anchored rect.
        if let Some((_, rect)) = unvisited_card {
            let uw = (rect[2] - rect[0]).round().max(1.0) as u32;
            let uh = (rect[3] - rect[1]).round().max(1.0) as u32;
            if self.view.unvisited_tex.as_ref().map(|c| c.size) != Some((uw, uh)) {
                let scene = super::card::unvisited_card_scene(uw, uh);
                let (tex, view) = core.rasterize(&scene, uw, uh, ColorLoad::Clear(CARD_BG));
                self.view.unvisited_tex = Some(super::CachedTile {
                    version: 0,
                    size: (uw, uh),
                    tex,
                    view,
                });
            }
            if let Some(cached) = &self.view.unvisited_tex {
                core.renderer().compose_external_texture(
                    &cached.view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new(rect),
                );
            }
        }
        // Frame dividers: fill each split gutter with a dark seam (so the gutter is
        // not stale pixels and reads as a divider). (Frame tree, F1.)
        if !dividers.is_empty() {
            if self.view.divider_tex.as_ref().map(|c| c.size) != Some((1, 1)) {
                let mut scene = netrender::Scene::new(1, 1);
                scene.push_rect(0.0, 0.0, 1.0, 1.0, [0.04, 0.05, 0.07, 1.0]);
                let (tex, view) =
                    core.rasterize(&scene, 1, 1, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                self.view.divider_tex = Some(super::CachedTile {
                    version: 0,
                    size: (1, 1),
                    tex,
                    view,
                });
            }
            if let Some(cached) = &self.view.divider_tex {
                for d in &dividers {
                    core.renderer().compose_external_texture(
                        &cached.view,
                        &target_view,
                        format,
                        w,
                        h,
                        ExternalTexturePlacement::new(d.rect),
                    );
                }
            }
        }
        // The roster pane renders through its view-driven `RosterPane` bundle: set the
        // rows, clamp the stored scroll to the last frame's content height, frame, and
        // composite. Row clicks dispatch through the runner DOM and the a11y projection
        // reads bounds off the same cached layout, so there is no rect cache.
        // (Window composition P2 companion — list-pane view-ification.)
        if let Some(rrect) = roster_rect {
            let rw = (rrect[2] - rrect[0]).round().max(1.0) as u32;
            let rh = (rrect[3] - rrect[1]).round().max(1.0) as u32;
            let rows = self.roster_rows();
            let field_rows = self.roster_field_rows();
            self.view.roster_pane.set_rows(&self.shared.presentation.chrome_theme, rows, field_rows);
            let max_scroll = self.view.roster_pane.max_scroll();
            self.view.roster_scroll = self.view.roster_scroll.clamp(0.0, max_scroll);
            let scene = self.view.roster_pane.frame(rw, rh, self.view.roster_scroll);
            let pb = self.shared.presentation.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = core.rasterize(&scene, rw, rh, ColorLoad::Clear(clear));
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(rrect),
            );
        }
        // The apparatus pane renders through its view-driven `ListPane`: build the
        // items (theme buttons + host diagnostics), set them on the pane, frame, and
        // composite. Theme-button clicks dispatch through the runner DOM, so there is
        // no button rect cache. (Apparatus; window composition P2 companion.)
        if let Some(arect) = self.apparatus_leaf_rect() {
            let aw = (arect[2] - arect[0]).round().max(1.0) as u32;
            let ah = (arect[3] - arect[1]).round().max(1.0) as u32;
            let themes = self.theme_options();
            let (system_rows, observability) = apparatus_data
                .as_ref()
                .expect("apparatus data was prepared when the pane was open");
            let items = super::apparatus::apparatus_items(
                &themes,
                self.physics_damping(),
                system_rows,
                observability,
            );
            let sheet = super::apparatus::apparatus_sheet(&self.shared.presentation.chrome_theme);
            self.view.apparatus_pane.set(sheet, "apparatus", items);
            let max_scroll = self.view.apparatus_pane.max_scroll();
            self.view.apparatus_scroll = self.view.apparatus_scroll.clamp(0.0, max_scroll);
            let scene = self.view.apparatus_pane.frame(aw, ah, self.view.apparatus_scroll);
            let pb = self.shared.presentation.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = core.rasterize(&scene, aw, ah, ColorLoad::Clear(clear));
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(arect),
            );
        }
        // The steward + inspector utility panes render through their view-driven
        // `ListPane`s (display-only): set the rows as inert items, frame, composite.
        // Each content type has its own bundle, so both can be open at once without
        // thrashing one cached layout. (Window composition P2 companion.)
        for leaf in self
            .laid_leaves()
            .into_iter()
            .filter(|leaf| matches!(leaf.content, PaneContent::Inspector | PaneContent::Steward))
        {
            let rect = leaf.rect;
            let pw = (rect[2] - rect[0]).round().max(1.0) as u32;
            let ph = (rect[3] - rect[1]).round().max(1.0) as u32;
            let rows = self.utility_pane_rows(&leaf.content);
            let items = super::utility_panes::utility_pane_items(&leaf.content, &rows);
            let sheet = super::utility_panes::utility_pane_sheet(&self.shared.presentation.chrome_theme);
            let pb = self.shared.presentation.chrome_theme.panel_bg.to_array();
            let (pane, scroll) = match &leaf.content {
                PaneContent::Inspector => (&mut self.view.inspector_pane, &mut self.view.inspector_scroll),
                PaneContent::Steward => (&mut self.view.steward_pane, &mut self.view.steward_scroll),
                _ => continue,
            };
            pane.set(sheet, "utility-pane", items);
            let max_scroll = pane.max_scroll();
            *scroll = scroll.clamp(0.0, max_scroll);
            let scene = pane.frame(pw, ph, *scroll);
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = core.rasterize(&scene, pw, ph, ColorLoad::Clear(clear));
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(rect),
            );
        }
        // The gloss pane: a whole-graph minimap swatch, with node hit-rects for
        // click-to-focus. (Gloss; the Navigator's graph-scope swatch cell.)
        self.view.gloss_node_rects.clear();
        if let Some(grect) = self.gloss_leaf_rect() {
            let gw = (grect[2] - grect[0]).round().max(1.0) as u32;
            let gh = (grect[3] - grect[1]).round().max(1.0) as u32;
            let (nodes, edges) = self.orrery().minimap_geometry();
            let (scene, local) =
                super::gloss::minimap_scene(&nodes, &edges, gw, gh, &self.shared.presentation.chrome_theme);
            let pb = self.shared.presentation.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = core.rasterize(&scene, gw, gh, ColorLoad::Clear(clear));
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(grect),
            );
            for (id, r) in local {
                self.view.gloss_node_rects.push((
                    id,
                    [
                        grect[0] + r[0],
                        grect[1] + r[1],
                        grect[0] + r[2],
                        grect[1] + r[3],
                    ],
                ));
            }
        }
        core.renderer().compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        // Window controls (borderless titlebar): the min / max / close strip drawn
        // over the chrome at the toolbar's top-right. Cached by band size; the input
        // path hit-tests the same geometry, so nothing is recorded here.
        let band_h = toolbar_h.max(1);
        let strip_w = super::titlebar::CONTROLS_W.round().max(1.0) as u32;
        if self.view.window_controls_tex.as_ref().map(|c| c.size) != Some((strip_w, band_h)) {
            let scene = super::titlebar::controls_scene(band_h, &self.shared.presentation.chrome_theme);
            let (tex, view) = core.rasterize(
                &scene,
                strip_w,
                band_h,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            );
            self.view.window_controls_tex = Some(super::CachedTile {
                version: 0,
                size: (strip_w, band_h),
                tex,
                view,
            });
        }
        if let Some(cached) = &self.view.window_controls_tex {
            let x0 = w as f32 - super::titlebar::CONTROLS_W;
            core.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([x0, 0.0, w as f32, band_h as f32]),
            );
        }
        // The F2.3 shellbar session switcher: a bottom-anchored strip of per-graph
        // thumbnail tiles drawn over the shellbar chrome. Left/Right edges only for
        // now — the Top/Bottom strips are too thin for the vertical tile stack.
        // Mirrors the gloss host-draw + hit-rect pattern, so clicks route through the
        // same shellbar region the input path already knows. (Multi-graph MG4.)
        self.view.session_row_rects.clear();
        self.view.session_close_rects.clear();
        self.view.session_add_rect = None;
        if !self.view.kind.is_slim()
            && matches!(
                self.shared.presentation.shellbar_edge,
                session_runtime::ShellbarEdge::Left | session_runtime::ShellbarEdge::Right
            )
            && !self.shared.session.session_thumbnails.is_empty()
        {
            let strip =
                shellbar::shellbar_rect(self.shared.presentation.shellbar_edge, w as f32, h as f32, toolbar_h as f32);
            // Order tiles by session id, matching `cycle_session`'s row order.
            let mut ids: Vec<SessionId> = self.shared.session.session_thumbnails.keys().copied().collect();
            ids.sort_by_key(|id| *id.as_uuid());
            // The highlighted tile is the *focused pane's* session (pane-as-unit),
            // resolved from focused_graph — equal to the active session today.
            let focused_session = self.session_for_graph(self.view.focused_graph).map(|(id, _)| id);
            let entries: Vec<(SessionId, &SwitcherThumbnail, &str, bool)> = ids
                .iter()
                .filter_map(|id| {
                    let thumb = self.shared.session.session_thumbnails.get(id)?;
                    let label = self.shared.session.session_labels.get(id).map(String::as_str).unwrap_or("");
                    Some((*id, thumb, label, Some(*id) == focused_session))
                })
                .collect();
            let region_w = (strip[2] - strip[0]).round().max(1.0) as u32;
            let region_h_f = super::switcher::switcher_height(entries.len()).min(strip[3] - strip[1]);
            let region_h = region_h_f.round().max(1.0) as u32;
            let origin_x = strip[0];
            let origin_y = strip[3] - region_h_f; // anchored at the strip's bottom
            let renaming = self.view.renaming.as_ref().map(|(id, buf)| (*id, buf.as_str()));
            let (scene, hits) = super::switcher::switcher_scene(
                &entries,
                region_w,
                region_h,
                &self.shared.presentation.chrome_theme,
                renaming,
                &mut self.shared.session.host_text,
            );
            let (_t, view) = core.rasterize(
                &scene,
                region_w,
                region_h,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            );
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([origin_x, origin_y, strip[2], strip[3]]),
            );
            let offset = |r: [f32; 4]| {
                [
                    origin_x + r[0],
                    origin_y + r[1],
                    origin_x + r[2],
                    origin_y + r[3],
                ]
            };
            for (id, r) in hits.rows {
                self.view.session_row_rects.push((id, offset(r)));
            }
            for (id, r) in hits.closes {
                self.view.session_close_rects.push((id, offset(r)));
            }
            self.view.session_add_rect = hits.add.map(offset);
        }
        frame.present();
        self.refresh_a11y_summary();

        // C0 baseline: one line per rendered frame (gated by the `meerkat::profile`
        // target). `total_us` is the whole `render()`; `chrome_us` is the chrome
        // cascade+layout+paint pipeline inside it. (cheap-path plan C0.)
        tracing::debug!(
            target: "meerkat::profile",
            total_us = frame_t.elapsed().as_micros(),
            chrome_us,
            "frame render profile"
        );

        // Keep animating while the orrery is settling / gliding / dragging.
        if orrery_redraw {
            self.view.request_redraw();
        }
    }

}
