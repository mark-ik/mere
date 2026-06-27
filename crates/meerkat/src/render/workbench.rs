/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Workbench-surface rendering for [`render`](super): project the workbench's open tiles
//! onto pelt's tile-tree contract, drive the host-authoritative tile shell, and read its
//! frame (scene + external-texture tile rects + drag ghost); plus the per-tile content
//! card collection. Split from `render.rs` to keep files under the workspace 600-LOC ceiling.

use super::*;

impl crate::WindowCtx<'_> {
    /// Render the workbench pane through the pelt `TileSurface` when open: build the tinted
    /// tile tree from the workbench's open members, drive the host-authoritative tile shell
    /// (lazily created, themed to the chrome), and read its frame. Returns
    /// `(workbench_scene, workbench_external, workbench_ghost)` — all empty / `None` when
    /// the pane is closed. (Extracted from `render()`.)
    pub(super) fn render_workbench_surface(
        &mut self,
        workbench_rect: Option<[f32; 4]>,
    ) -> (
        Option<(netrender::Scene, u32, u32)>,
        Vec<(
            pelt_core::tile::TileId,
            (f32, f32, f32, f32),
            pelt_core::tile::TextureKey,
        )>,
        Option<((f32, f32, f32, f32), netrender::Scene)>,
    ) {
        let mut workbench_scene: Option<(netrender::Scene, u32, u32)> = None;
        // The surface's external-texture tile rects this frame, `(tile, (x,y,w,h), key)`
        // in surface-local px — carried to the placement step below.
        let mut workbench_external: Vec<(
            pelt_core::tile::TileId,
            (f32, f32, f32, f32),
            pelt_core::tile::TextureKey,
        )> = Vec::new();
        // The dragged-tab ghost (pane-local rect + its scene), composited over the
        // workbench leaf while a tab drag is in flight. (Drag via pelt TileEvents.)
        let mut workbench_ghost: Option<((f32, f32, f32, f32), netrender::Scene)> = None;
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
                    self.orrery().graph().get_node_by_id(m).map(|(_, n)| {
                        let url = n.url();
                        // A settings tile shows a friendly page title, not its `settings://` url.
                        let title = crate::settings_lane::settings_tab_title(url)
                            .unwrap_or_else(|| url.to_string());
                        (m, title)
                    })
                })
                .collect();
            // Each tab is tinted to match its graph node, so a tab reads as its node:
            // the orrery's gnode coloring (NODE_SHEET) — selection wins (amber, dark
            // label), else the activation state (open green / closed red / idle blue),
            // each with that gnode's own label color. Recomputed per frame, so selecting
            // a node recolors its tab (the yellow highlight) live.
            let states = self.node_states();
            let selected: std::collections::HashSet<GraphMemberId> =
                self.orrery().selected_members().into_iter().collect();
            let tree = self.view.workbench.to_tile_tree(|m| {
                let key = m.as_u128() as u64;
                let accent = if selected.contains(&m) {
                    pelt_core::tile::TabAccent { background: [232, 150, 40], foreground: [28, 22, 10] }
                } else {
                    match states.get(&m) {
                        Some(orrery::NodeState::Open) => {
                            pelt_core::tile::TabAccent { background: [58, 140, 94], foreground: [238, 250, 243] }
                        }
                        Some(orrery::NodeState::Closed) => {
                            pelt_core::tile::TabAccent { background: [166, 72, 72], foreground: [250, 240, 240] }
                        }
                        _ => pelt_core::tile::TabAccent { background: [54, 92, 156], foreground: [245, 247, 252] },
                    }
                };
                pelt_core::tile::Tile {
                    id: pelt_core::tile::TileId(key),
                    title: titles.get(&m).cloned().unwrap_or_default(),
                    content: pelt_core::tile::ContentSource::ExternalTexture(
                        pelt_core::tile::TextureKey(key),
                    ),
                    accent: Some(accent),
                }
            });
            if let Some(tree) = tree {
                // Host-authority: set the projected tree (the shell is a driven view),
                // then render its frame. Created lazily on the first tiled frame. The
                // shell (vs the bare surface) also owns the pointer state machine that
                // turns drag / divider gestures into TileEvents the host applies.
                match self.view.pelt_shell.as_mut() {
                    Some(s) => s.set_tree(tree),
                    None => {
                        self.view.pelt_shell =
                            Some(pelt_desktop::TileShell::new_host_authoritative(tree))
                    }
                }
                // Theme the tiles to match the chrome: layer the chrome-theme-derived
                // tile CSS over the shell's structural default, rebuilt only when the
                // active theme changed (the shell persists across frames).
                let theme = self.shared.presentation.chrome_theme;
                if self.view.pelt_theme != Some(theme) {
                    if let Some(s) = self.view.pelt_shell.as_mut() {
                        s.set_theme(crate::tile_sheet(&theme));
                    }
                    self.view.pelt_theme = Some(theme);
                }
                let shell = self.view.pelt_shell.as_mut().unwrap();
                shell.resize(ww, wh);
                let frame = shell.frame();
                workbench_external = frame.external_tiles;
                workbench_scene = Some((frame.frame_scene, ww, wh));
                // A tab drag carries a ghost of the dragged tab at the cursor; composite
                // it over the workbench leaf. (Replaces the host drop-target highlight.)
                workbench_ghost = frame.ghost.map(|g| (g.rect, g.scene));
            }
        }
        (workbench_scene, workbench_external, workbench_ghost)
    }

    /// Collect this frame's content cards from the workbench's laid-out tiles: map the
    /// surface's external-texture tile rects to window coords, drive each tile's content
    /// actor (or surface-tier scrying pool / settings pane), and return
    /// `(cards, scrying_surfaces)`. Also records the slot rects + folds the settings tiles.
    /// (Extracted from `render()`.)
    pub(super) fn collect_cards(
        &mut self,
        workbench_rect: Option<[f32; 4]>,
        workbench_external: Vec<(
            pelt_core::tile::TileId,
            (f32, f32, f32, f32),
            pelt_core::tile::TextureKey,
        )>,
    ) -> (
        Vec<(GraphMemberId, [f32; 4], (u32, u32))>,
        Vec<(GraphMemberId, [f32; 4])>,
    ) {
        // Content cards floating over the band: one per laid-out tile in Tree, the
        // focused-node card at `card_rect` in Cartography. Each entry is
        // `(member, window dest rect, raster size)`; the scene comes from that
        // node's activation at composite time. Driving an activation re-renders it
        // off the UI thread only when its document or size changed.
        let mut cards: Vec<(GraphMemberId, [f32; 4], (u32, u32))> = Vec::new();
        // Every compat-pinned surface shown this frame, as (member, window rect):
        // each open compat tile in Tree, plus the focused compat card in Cartography.
        // The panes share one per-HWND composition target (scry's `new_attached`), so
        // any number stay live at once; the imported textures composite at these
        // rects and the input path resolves a point against this list. (Multi-tile
        // scry; was a single `scrying_card` under the one-surface X1 model.)
        let mut scrying_surfaces: Vec<(GraphMemberId, [f32; 4])> = Vec::new();
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
            // Settings tiles recorded this frame: `(member, ref, body rect)`, resolved
            // through the provider seam into the shell document's settings panes after the
            // loop. They drive no content actor and composite no texture. (Settings lane P1.)
            let mut settings_tiles: Vec<(GraphMemberId, String, [f32; 4])> = Vec::new();
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
                // A settings tile (`settings://<ns>/<page>`) renders through the shell
                // document's settings pane, not a content actor: record its ref + body rect
                // and skip the actor / card / texture paths below. (Settings lane P1.)
                if let Some(reference) = url.strip_prefix("settings://") {
                    settings_tiles.push((member, reference.to_string(), content));
                    continue;
                }
                let cw = (content[2] - content[0]).round().max(1.0) as u32;
                let ch = (content[3] - content[1]).round().max(1.0) as u32;
                if self.is_surface_tier(member, &url) {
                    // Surface-tier tile: drive the UI-thread scrying pool into this tile's
                    // own pane on the shared composition root — park the WebView's
                    // visual at the tile's content origin and import its frame. Each
                    // compat tile is an independent pane, so several render at once.
                    // The tile gets no constellation actor; its imported texture
                    // composites at its rect below, and `scrying_rects` routes input
                    // into the WebView. (Multi-tile scry.)
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
                            (content[0], content[1]),
                            &window,
                            &device,
                            &queue,
                            &session_dir,
                        );
                    }
                    scrying_surfaces.push((member, content));
                    // The WebView paints on its own schedule; keep frames coming.
                    self.view.request_redraw();
                    continue;
                }
                self.ensure_content(&url);
                let state = self.shared.content.pages.get(&url).cloned();
                let sheet = self.shared.presentation.document_sheet_composed();
                // The routed engine (the node's pin or the policy decision) tells the
                // actor which render rung to take — static `serval.web` or the scripted
                // lane for `serval.scripted`. (Render ladder.)
                let engine = self.route_engine(member, &url).engine_id;
                self.shared
                    .content
                    .constellation
                    .drive(member, &url, state, cw, ch, sheet, &engine);
                cards.push((member, content, (cw, ch)));
            }
            self.view.tile_rects = slot_rects;
            // Fold the recorded settings tiles into the shell document (or clear them) for
            // this frame's render. (Settings lane P1.)
            self.snapshot_settings_panes(settings_tiles);
        } else {
            self.view.tile_rects.clear(); // no tile drag targets when the pane is closed
            self.snapshot_settings_panes(Vec::new()); // no settings tiles with the pane closed
        }
        (cards, scrying_surfaces)
    }
}
