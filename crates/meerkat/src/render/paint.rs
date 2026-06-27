/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The frame paint/compose pass: rasterize the panes, acquire the surface
//! frame, composite every layer, and present. Extracted verbatim from
//! render() (the frame-acquire early-return is the last thing render does, so
//! returning here is equivalent to returning from render). [`super`]

use super::*;

/// All the per-frame build-up outputs the paint pass consumes, bundled so
/// the call site is one argument rather than twenty positionals.
pub(super) struct PaintInputs {
    pub chrome_scene: netrender::Scene,
    pub orrery_scene: netrender::Scene,
    pub orrery_redraw: bool,
    pub orrery_w: u32,
    pub orrery_h: u32,
    pub secondary_orreries: Vec<(netrender::Scene, [f32; 4], u32, u32)>,
    pub workbench_scene: Option<(netrender::Scene, u32, u32)>,
    pub workbench_ghost: Option<((f32, f32, f32, f32), netrender::Scene)>,
    pub workbench_rect: Option<[f32; 4]>,
    pub cards: Vec<(GraphMemberId, [f32; 4], (u32, u32))>,
    pub scrying_surfaces: Vec<(GraphMemberId, [f32; 4])>,
    pub snapshot_card: Option<(GraphMemberId, String, [f32; 4], Option<(netrender::Scene, u32)>)>,
    pub external_texture_placements: Vec<(u64, [f32; 4])>,
    pub dividers: Vec<frame_view::LaidDivider>,
    pub w: u32,
    pub h: u32,
    pub toolbar_h: u32,
    pub dpr: f32,
    pub chrome_us: u128,
    pub frame_t: std::time::Instant,
}

impl WindowCtx<'_> {
    pub(super) fn paint_frame(&mut self, inputs: PaintInputs) {
        let PaintInputs {
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
        } = inputs;
        let core = self.render_core.expect("render core present");
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
        let composite = self.rasterize_cards(core, dpr, &cards);

        let surface = self.view.surface.as_ref().expect("window surface present");
        let Some(frame) = surface.acquire(core) else { return };
        let target_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let format = surface.format();
        // Content fills [toolbar_h, h] (dest_rect is [x0, y0, x1, y1] corners;
        // viewport is the full surface). Then each content card floats over it, and
        // the transparent-cleared chrome composites over the whole window — toolbar
        // + dropdown on top, the rest letting the content through.
        // Composite each chrome-document `<external-texture>` element's registered texture at its
        // laid-out rect: the placement comes from the document, not a hardcoded host rect. The
        // orrery scene is the first such element (its key is `ORRERY_SCENE_KEY`); secondaries /
        // workbench / scrying join as they become document elements and register their views. (cond 5.)
        for &(key, rect) in &external_texture_placements {
            if key == crate::window_view::ORRERY_SCENE_KEY {
                core.renderer().compose_external_texture(
                    &orrery_view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new(rect),
                );
            }
        }
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
            // Scroll is a vertical UV window over the cached band — a GPU sample
            // shift, no re-raster within the band. The visible window
            // [scroll, scroll+visible_h] maps into the band [band_y, band_y+band_h].
            let band_y = self.view.tile_bands.get(member).copied().unwrap_or(0.0);
            let tex_w = cached.size.0 as f32;
            let band_h = cached.size.1 as f32;
            let dest_w = (dest[2] - dest[0]).max(1.0);
            let dest_h = (dest[3] - dest[1]).max(1.0);
            // Document-px shown, sized so the vertical scale equals the horizontal one
            // (tex_w -> dest_w): a uniform downscale for snapshot thumbnails, 1:1 for
            // live cards / tiles.
            let visible_h = dest_h * tex_w / dest_w;
            let content_h = (self.shared.content.constellation.content_height(*member) as f32)
                .max(visible_h);
            let scroll = self
                .view
                .scroll
                .get(member)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, (content_h - visible_h).max(0.0));
            let uv = [
                0.0,
                ((scroll - band_y) / band_h).clamp(0.0, 1.0),
                1.0,
                ((scroll + visible_h - band_y) / band_h).clamp(0.0, 1.0),
            ];
            core.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*dest).with_uv(uv),
            );
        }
        // Find-in-page highlights (S2): translucent rects over the focused node's
        // match rects, mapped content-local -> window with the same scale + scroll
        // the composite loop above used. The active match is tinted stronger. The
        // host owns this overlay: the actor only ships rects (full-document px); the
        // host knows the card's dest rect + scroll. HTML/serval lane only.
        if self.view.chrome().find_open {
            if let Some(focused) = self.focused_member() {
                let active = self.view.chrome().find_active;
                let mut overlays: Vec<([f32; 4], bool)> = Vec::new();
                for (dest, member) in &composite {
                    if *member != focused {
                        continue;
                    }
                    let Some(cached) = self.view.tile_textures.get(member) else {
                        continue;
                    };
                    let tex_w = cached.size.0 as f32;
                    let dest_w = (dest[2] - dest[0]).max(1.0);
                    let dest_h = (dest[3] - dest[1]).max(1.0);
                    // Window px per document px (1.0 for a 1:1 live card / tile).
                    let s = dest_w / tex_w;
                    let visible_h = dest_h / s;
                    let content_h = (self.shared.content.constellation.content_height(*member)
                        as f32)
                        .max(visible_h);
                    let scroll = self
                        .view
                        .scroll
                        .get(member)
                        .copied()
                        .unwrap_or(0.0)
                        .clamp(0.0, (content_h - visible_h).max(0.0));
                    for (mi, rects) in self.find_matches_for(*member).iter().enumerate() {
                        let is_active = mi == active;
                        for r in rects {
                            // Find rects come back in the actor's logical coords; scale to
                            // physical (×dpr) to match the physical scroll + texture. (Auto-DPI D2.)
                            let (r0, r1, r2, r3) =
                                (r[0] * dpr, r[1] * dpr, r[2] * dpr, r[3] * dpr);
                            let wy0 = dest[1] + (r1 - scroll) * s;
                            let wy1 = dest[1] + (r3 - scroll) * s;
                            // Cull a match scrolled out of the card's visible band.
                            if wy1 <= dest[1] || wy0 >= dest[3] {
                                continue;
                            }
                            let wx0 = (dest[0] + r0 * s).max(dest[0]);
                            let wx1 = (dest[0] + r2 * s).min(dest[2]);
                            let cy0 = wy0.max(dest[1]);
                            let cy1 = wy1.min(dest[3]);
                            if wx1 <= wx0 || cy1 <= cy0 {
                                continue;
                            }
                            overlays.push(([wx0, cy0, wx1, cy1], is_active));
                        }
                    }
                }
                if !overlays.is_empty() {
                    // Two 1x1 translucent textures (amber normal, stronger active),
                    // rasterized once and composited per rect — the drop-target /
                    // divider overlay idiom.
                    let mut normal = netrender::Scene::new(1, 1);
                    normal.push_rect(0.0, 0.0, 1.0, 1.0, [1.0, 0.82, 0.20, 0.38]);
                    let (_n, normal_view) =
                        core.rasterize(&normal, 1, 1, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                    let mut act = netrender::Scene::new(1, 1);
                    act.push_rect(0.0, 0.0, 1.0, 1.0, [1.0, 0.55, 0.10, 0.55]);
                    let (_a, active_view) =
                        core.rasterize(&act, 1, 1, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                    for (rect, is_active) in &overlays {
                        let view = if *is_active { &active_view } else { &normal_view };
                        core.renderer().compose_external_texture(
                            view,
                            &target_view,
                            format,
                            w,
                            h,
                            ExternalTexturePlacement::new(*rect),
                        );
                    }
                }
            }
        }
        // The compatibility-view surfaces: each pane's imported WebView texture at
        // its tile / card rect. No UV window — the WebView scrolls itself. The rects
        // are recorded so the input path can forward mouse / wheel / keys into the
        // pane under the cursor. (X2; multi-tile.)
        for (member, dest) in &scrying_surfaces {
            if let Some(view) = self.view.scrying.texture_view(*member) {
                core.renderer().compose_external_texture(
                    view,
                    &target_view,
                    format,
                    w,
                    h,
                    // The imported WebView texture (WebView2 composition capture)
                    // is premultiplied-alpha; blend it as such so transparent
                    // regions composite correctly (opaque pages are unaffected).
                    ExternalTexturePlacement::new(*dest).with_alpha(SourceAlpha::Premultiplied),
                );
            }
        }
        self.view.scrying_rects = scrying_surfaces;
        // The "last visit" snapshot card: rasterize the host-rendered scene once
        // per url (cached), then composite uniform-scaled into the small dest with
        // the same vertical-scroll UV window as the other cards.
        if let Some((_, url, _, built)) = snapshot_card {
            if let Some((scene, _content_h)) = built {
                // Render the page's top peek, read it back, and encode a PNG data-URI for the
                // snapshot card's chrome `<img>`. The card shows only its top band, so a fixed
                // peek size suffices (the `<img>` scales it to the card). Once per url — the
                // readback blocks, so it is gated on the data-uri cache miss above. The image
                // is opaque chrome DOM after the node cards, so it paints over them and under
                // the overlays — the layering an external-texture could not give. (Layering fix.)
                const PEEK_W: u32 = 300;
                const PEEK_H: u32 = 390;
                let (tex, _view) = core.rasterize(&scene, PEEK_W, PEEK_H, ColorLoad::Clear(CARD_BG));
                let rgba = read_texture_rgba(core.device(), core.queue(), &tex, PEEK_W, PEEK_H);
                if let Some(uri) = favicon_data_uri(&rgba, PEEK_W, PEEK_H) {
                    // Bound the per-url snapshot cache: each entry is a base64 PNG peek
                    // (tens of KB), so without a cap it grows unbounded over a long
                    // session. Crude but bounded: drop the cache when it gets large; the
                    // few visible cards re-encode once on a later frame. (Cache cap.)
                    if self.view.snapshot_data_uris.len() >= 256 {
                        self.view.snapshot_data_uris.clear();
                    }
                    self.view.snapshot_data_uris.insert(url.clone(), uri);
                }
            }
        }
        // Tile decorations (workbench pane): a "Reloading…" placeholder over a tile
        // whose actor is recovering (respawned, no scene yet — so nothing composited
        // there), and a small amber "kept-warm" badge on a background-pinned tile (the
        // star the old strip carried). Both draw over the tile's content rect; the
        // click-to-pin toggle stays on the Ctrl+B / command path. State is collected
        // first so the composite loop holds no `self` borrow. (Decoration re-applied on
        // the pelt surface path.)
        let tile_decos: Vec<([f32; 4], bool, bool)> = self
            .view
            .tile_rects
            .iter()
            .map(|(m, r)| {
                (
                    *r,
                    self.shared.content.constellation.is_recovering(*m),
                    self.shared.content.constellation.is_background(*m),
                )
            })
            .collect();
        for (rect, recovering, background) in &tile_decos {
            if *recovering {
                let rw = (rect[2] - rect[0]).round().max(1.0) as u32;
                let rh = (rect[3] - rect[1]).round().max(1.0) as u32;
                let card_bg = crate::chrome_to_wgpu(self.shared.presentation.chrome_theme.surface_bg);
                let scene =
                    crate::card::recovering_card_scene(rw, rh, self.shared.presentation.document_palette);
                let (_t, view) = core.rasterize(&scene, rw, rh, ColorLoad::Clear(card_bg));
                core.renderer().compose_external_texture(
                    &view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new(*rect),
                );
            }
            if *background {
                let mut scene = netrender::Scene::new(1, 1);
                scene.push_rect(0.0, 0.0, 1.0, 1.0, [0.88, 0.66, 0.27, 1.0]);
                let (_t, view) =
                    core.rasterize(&scene, 1, 1, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                let bs = 10.0;
                let bx1 = rect[2] - 6.0;
                let by0 = rect[1] + 6.0;
                core.renderer().compose_external_texture(
                    &view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new([bx1 - bs, by0, bx1, by0 + bs]),
                );
            }
        }
        // The unvisited placeholder card now renders as a pure-DOM shell element (a dashed
        // "double-click to load" card, document-ordered over the node cards), so there is no
        // host composite for it; `unvisited_card` survives only as a `content_rects` entry so
        // a double-click over it still opens the node in a pelt tile. (Layering fix.)
        // Frame dividers: fill each split gutter with a dark seam (so the gutter is
        // not stale pixels and reads as a divider). (Frame tree, F1.)
        if !dividers.is_empty() {
            if self.view.divider_tex.as_ref().map(|c| c.size) != Some((1, 1)) {
                let mut scene = netrender::Scene::new(1, 1);
                scene.push_rect(0.0, 0.0, 1.0, 1.0, [0.04, 0.05, 0.07, 1.0]);
                let (tex, view) =
                    core.rasterize(&scene, 1, 1, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                self.view.divider_tex = Some(crate::CachedTile {
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
        // Dragged-tab ghost: while a tab drag is in flight the shell carries a ghost of
        // the dragged tab at the cursor (pane-local); composite it on top of the tiles,
        // offset into the workbench leaf. The shell owns the drag, so this replaces the
        // host's old drop-target highlight. (Drag via pelt TileEvents.)
        if let (Some(((gx, gy, gw, gh), scene)), Some(wr)) =
            (workbench_ghost.as_ref(), workbench_rect)
        {
            let gw_px = gw.round().max(1.0) as u32;
            let gh_px = gh.round().max(1.0) as u32;
            let (_t, view) =
                core.rasterize(scene, gw_px, gh_px, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
            let (x0, y0) = (wr[0] + gx, wr[1] + gy);
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([x0, y0, x0 + gw, y0 + gh]),
            );
        }
        // The roster pane is folded into the shell document now: its rect + rows were
        // snapshotted into the shell state above (before the chrome render), so the one
        // render lays it out and the one hit-test routes its clicks. (Phase 1.)
        // The apparatus / steward / inspector / trail panes are folded into the shell
        // document now (like the roster): their items + rects were snapshotted into the
        // shell state before the chrome render, so the one render lays them out, scrolls
        // them, and routes their clicks. Replaces the per-pane ListPane frame + composite.
        // (Phase 1, step 2.)
        // The gloss pane (the Navigator): a whole-graph minimap swatch on top, the
        // recently-visited nodes listed below. Both carry node hit-rects for
        // click-to-focus; recent is the `SharedNavigationMemory` projection. (Gloss.)
        self.view.gloss_node_rects.clear();
        self.view.gloss_recent_rects.clear();
        if let Some(grect) = self.gloss_leaf_rect() {
            let pb = self.shared.presentation.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            // Split the pane: minimap (top ~58%), recent list (the rest).
            let minimap_h = ((grect[3] - grect[1]) * 0.58).max(1.0);
            let minimap_rect = [grect[0], grect[1], grect[2], grect[1] + minimap_h];
            let recent_rect = [grect[0], grect[1] + minimap_h, grect[2], grect[3]];

            // Minimap swatch. With a gloss lens set, the gloss shows its OWN arrangement (recomputed
            // only when its inputs change, since it may be an expensive layout); otherwise it mirrors
            // the main view. (Graph signals — P6, the independent gloss projection.)
            let mw = (minimap_rect[2] - minimap_rect[0]).round().max(1.0) as u32;
            let mh = (minimap_rect[3] - minimap_rect[1]).round().max(1.0) as u32;
            let (nodes, edges, rings) =
                if let Some(id) = self.orrery().gloss_strategy().map(str::to_string) {
                    if self.orrery().gloss_needs_recompute(mw, mh) {
                        let pane = self.orrery();
                        // Gate the lens's overlays on the same ring toggles as the main view: the
                        // gloss shows community / bridge rings exactly when those toggles are on. The
                        // overlays ride the projection (the overlay pipe), placed at the lens's own
                        // positions by `gloss_geometry`. (Graph signals — P6b.)
                        let clusters = pane.show_community_rings().then(|| pane.community()).flatten();
                        let bridges = pane.show_bridge_rings().then(|| pane.bridges()).flatten();
                        // Positions come from the whole graph or, when the gloss is scoped to the
                        // selection, the *induced subgraph* of those nodes — so the lens reflects the
                        // selection's own structure, not a crop of the whole-graph layout. The
                        // overlays are layout-independent (the same signal builder either way).
                        // (Graph signals — P6c, the gloss subgraph re-layout.)
                        let (positions, overlays): (Vec<_>, _) = match pane.gloss_scope_keys() {
                            Some(scope) => (
                                platen::project_orrery_subgraph(
                                    pane.graph(),
                                    &scope,
                                    &id,
                                    pane.focused_key(),
                                    mw,
                                    mh,
                                ),
                                platen::signal_overlays(clusters, bridges),
                            ),
                            None => {
                                let projection = platen::project_orrery_lens(
                                    &id,
                                    pane.graph(),
                                    pane.focused_key(),
                                    mw,
                                    mh,
                                    clusters,
                                    bridges,
                                );
                                let pos =
                                    projection.nodes.iter().map(|n| (n.node, n.position)).collect();
                                (pos, projection.overlays)
                            }
                        };
                        self.orrery_mut().set_gloss_positions(positions, overlays, mw, mh);
                    }
                    self.orrery().gloss_geometry_cached()
                } else {
                    let (n, e) = self.orrery().minimap_geometry();
                    (n, e, Vec::new())
                };
            let (scene, local) = crate::gloss::minimap_scene(
                &nodes,
                &edges,
                &rings,
                mw,
                mh,
                &self.shared.presentation.chrome_theme,
            );
            let (_t, view) = core.rasterize(&scene, mw, mh, ColorLoad::Clear(clear));
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(minimap_rect),
            );
            for (id, r) in local {
                self.view.gloss_node_rects.push((
                    id,
                    [
                        minimap_rect[0] + r[0],
                        minimap_rect[1] + r[1],
                        minimap_rect[0] + r[2],
                        minimap_rect[1] + r[3],
                    ],
                ));
            }

            // Recently-visited list.
            let recent: Vec<_> = self
                .orrery()
                .graph()
                .recent_visited(8)
                .into_iter()
                .map(|rv| (rv.node, rv.url))
                .collect();
            let rw = (recent_rect[2] - recent_rect[0]).round().max(1.0) as u32;
            let rh = (recent_rect[3] - recent_rect[1]).round().max(1.0) as u32;
            let (rscene, rlocal) = crate::gloss::recent_scene(
                &recent,
                rw,
                rh,
                &self.shared.presentation.chrome_theme,
                &mut self.shared.session.host_text,
            );
            let (_t2, rview) = core.rasterize(&rscene, rw, rh, ColorLoad::Clear(clear));
            core.renderer().compose_external_texture(
                &rview,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(recent_rect),
            );
            for (id, r) in rlocal {
                self.view.gloss_recent_rects.push((
                    id,
                    [
                        recent_rect[0] + r[0],
                        recent_rect[1] + r[1],
                        recent_rect[0] + r[2],
                        recent_rect[1] + r[3],
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
        let strip_w = crate::titlebar::CONTROLS_W.round().max(1.0) as u32;
        if self.view.window_controls_tex.as_ref().map(|c| c.size) != Some((strip_w, band_h)) {
            let scene = crate::titlebar::controls_scene(band_h, &self.shared.presentation.chrome_theme);
            let (tex, view) = core.rasterize(
                &scene,
                strip_w,
                band_h,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            );
            self.view.window_controls_tex = Some(crate::CachedTile {
                version: 0,
                size: (strip_w, band_h),
                tex,
                view,
            });
        }
        if let Some(cached) = &self.view.window_controls_tex {
            let x0 = w as f32 - crate::titlebar::CONTROLS_W;
            core.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([x0, 0.0, w as f32, band_h as f32]),
            );
        }
        // The add-tag prompt: a centered text-entry box over the content while the
        // host captures a tag for the selected node(s). Drawn last so it sits over
        // the orrery + panes. (Add-tag.)
        if let Some(buf) = self.view.tagging.clone() {
            let pw: u32 = 360;
            let ph: u32 = 40;
            let scene = crate::tags::tag_prompt_scene(
                &buf,
                pw,
                ph,
                &self.shared.presentation.chrome_theme,
                &mut self.shared.session.host_text,
            );
            let (_t, view) =
                core.rasterize(&scene, pw, ph, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
            let x0 = ((w as f32) - pw as f32) * 0.5;
            let y0 = toolbar_h as f32 + 16.0;
            core.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([x0, y0, x0 + pw as f32, y0 + ph as f32]),
            );
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
