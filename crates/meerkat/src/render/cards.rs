/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Content-card rendering for [`render`](super): the chrome scene build, the focused-node
//! snapshot / unvisited cards, the per-card on-screen content-rect recording, and the
//! per-card band rasterization. Split from `render.rs` to keep files under the workspace
//! 600-LOC ceiling.

use super::*;
use serval_winit_host::RenderCore;

impl crate::WindowCtx<'_> {
    /// Record each card's on-screen content rect into `self.view.content_rects` for this
    /// frame, so a wheel over a card scrolls the card rather than panning the orrery: the
    /// live tiles, the unvisited placeholder, the (cached-and-shown) snapshot card, and
    /// every scrying surface. (Extracted from `render()`.)
    pub(super) fn finalize_content_rects(
        &mut self,
        cards: &[(GraphMemberId, [f32; 4], (u32, u32))],
        unvisited_card: Option<(GraphMemberId, [f32; 4])>,
        snapshot_card: &Option<(GraphMemberId, String, [f32; 4], Option<(netrender::Scene, u32)>)>,
        scrying_surfaces: &[(GraphMemberId, [f32; 4])],
    ) {
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
        if let Some((member, _, rect, built)) = snapshot_card {
            // Claim the card's hit-rect only once its snapshot is cached + shown (`built` is
            // `None` = already cached). While it is still building there is no visible card,
            // so its rect must not intercept clicks on the node beneath it — that phantom
            // rect was making the first double-click-to-open flaky. (Snapshot — no phantom rect.)
            if built.is_none() {
                self.view.content_rects.push((*member, *rect));
            }
        }
        for (member, rect) in scrying_surfaces {
            self.view.content_rects.push((*member, *rect));
        }
    }

    /// Build the chrome (shell document) scene for this frame, now that every folded pane
    /// is set: assemble the chrome stylesheet (base + roster + apparatus + utility), render
    /// the shell through its persistent incremental session, time it, and enumerate the
    /// chrome's `<external-texture>` placements. Returns
    /// `(chrome_scene, chrome_us, external_texture_placements)`. (Extracted from `render()`.)
    pub(super) fn render_chrome_scene(
        &mut self,
        w: u32,
        h: u32,
        cursor: Option<TextCursor>,
        chrome_scroll: &ScrollOffsets<NodeId>,
        roster_css: &[String],
        apparatus_css: &[String],
        utility_css: &[String],
    ) -> (netrender::Scene, u128, Vec<(u64, [f32; 4])>) {
        // Build the chrome (shell document) scene now that every folded pane — roster, the
        // list panes, and the settings panes (positioned at this frame's tile rects) — is set,
        // so the one shell render reflects them this frame (no page-switch lag). Moved down
        // from above the workbench block for the settings panes' sake. (Settings lane P1.)
        let chrome_sheet: Vec<&str> = self
            .shared
            .presentation
            .chrome_sheet_refs()
            .into_iter()
            .chain(roster_css.iter().map(String::as_str))
            .chain(apparatus_css.iter().map(String::as_str))
            .chain(utility_css.iter().map(String::as_str))
            .collect();
        let chrome_t = Instant::now();
        // C3 (cheap-path): render the chrome through its persistent `IncrementalLayout`
        // session — drains this frame's mutations, rebuilds only on a structural / resize /
        // theme frame, else restyles incrementally (RepaintOnly, no relayout).
        let chrome_scene = PaneSession::scene(
            &mut self.view.chrome_session,
            &self.view.dom,
            &chrome_sheet,
            w,
            h,
            cursor,
            chrome_scroll,
        );
        let chrome_us = chrome_t.elapsed().as_micros();
        // The chrome document's `<external-texture>` elements + their laid-out rects, enumerated
        // now that the chrome session has been laid out, composited at the compositor pass below
        // so each external surface's placement comes from the document. (cond 5.)
        let external_texture_placements = self.external_texture_placements();
        (chrome_scene, chrome_us, external_texture_placements)
    }

    /// Compute the orrery's focused-node content card for this frame (when the focused node
    /// is not itself an open tile): a "last visit" snapshot card for a visited node
    /// (rendered host-side from the durable cache, once per url) or a dashed "double-click
    /// to load" placeholder for an unvisited one. Returns `(snapshot_card, unvisited_card)`.
    /// (Extracted from `render()`.)
    pub(super) fn compute_focus_cards(
        &mut self,
        workbench_rect: Option<[f32; 4]>,
        orrery_rect: [f32; 4],
    ) -> (
        Option<(GraphMemberId, String, [f32; 4], Option<(netrender::Scene, u32)>)>,
        Option<(GraphMemberId, [f32; 4])>,
    ) {
        let mut snapshot_card: Option<(
            GraphMemberId,
            String,
            [f32; 4],
            Option<(netrender::Scene, u32)>,
        )> = None;
        let mut unvisited_card: Option<(GraphMemberId, [f32; 4])> = None;
        // The orrery's focused-node card, alongside any workbench pane — but not when
        // the focused node is itself an open tile. The tile is the view; a second card
        // would drive the node's one content actor at a different viewport size and
        // contend with the tile (last-writer-per-frame thrash, so opening the card
        // visibly reflows the tile). The proper fix is per-surface scenes; until then
        // the tile wins. (Card/tile contention fix.)
        let card_member = self.focused_member().filter(|m| {
            workbench_rect.is_none() || !self.view.workbench.open_members().contains(m)
        });
        if let (Some(member), Some(url)) = (
            card_member,
            self.orrery().focused_url().map(str::to_string),
        ) {
            // The orrery's static card next to the focused node (fall back to the
            // fixed top-right rect when the node's screen pos is unknown): a visited
            // node shows its "last visit" snapshot (a short peek at the retained
            // scene, no actor), an un-visited node a "double-click to load"
            // placeholder. The live-preview card is retired — content opens in pelt.
            // The orrery reports the node in its own (leaf-local) viewport; offset
            // by the orrery leaf's origin for window coords, and anchor the card
            // within the orrery leaf rect (so it stays in the orrery pane when split).
            let node = self
                .orrery()
                .focused_node_screen()
                .map(|(nx, ny)| (orrery_rect[0] + nx, orrery_rect[1] + ny));
            if self.orrery().member_visited(member) {
                // "Last visit" snapshot: a small fixed-size card, rendered host-side
                // from the durable cache / synthesis below (no actor), so it survives
                // a restart. Composited uniform-scaled into a scrollable thumbnail.
                let rect = node
                    .and_then(|(nx, ny)| {
                        crate::card::anchored_card_rect(
                            nx,
                            ny,
                            crate::card::SNAP_W,
                            crate::card::SNAP_H,
                            orrery_rect,
                        )
                    })
                    .or_else(|| crate::card::card_rect(orrery_rect));
                if let Some((x0, y0, x1, y1, _, _)) = rect {
                    // Render the snapshot scene from cache / synthesis once per url;
                    // `None` means its data-URI image is already cached (encoded below).
                    let built = if self.view.snapshot_data_uris.contains_key(&url) {
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
                        // Document-family built-ins (mere:// synthesized pages) render
                        // the focused content *natively* through serval (note_view ->
                        // ScriptedDom -> netrender), the reframe's path; web / engine
                        // content stays on the document-canvas preview lane. knot://
                        // notes join this branch once their routing lands. (Slice 1b-A.)
                        if url.starts_with("mere://") {
                            let doc = crate::card::content_document(&url, state.as_ref());
                            let scene =
                                crate::note_surface::note_scene(&doc, RENDER_W, RENDER_H, &[]);
                            Some((scene, RENDER_H))
                        } else {
                            let (scene, content_height, _links) = crate::card::render_content_scene(
                                &url,
                                state.as_ref(),
                                &self.shared.content.engine_registry,
                                &self.shared.content.route_policy,
                                &loader,
                                RENDER_W,
                                RENDER_H,
                                &self.shared.presentation.document_sheet_composed(),
                            );
                            Some((scene, content_height))
                        }
                    };
                    snapshot_card = Some((member, url, [x0, y0, x1, y1], built));
                }
            } else {
                // Never visited this session: a small dashed "Double-click to load"
                // placeholder, anchored like the other cards. Double-clicking it
                // opens the node in a pelt (workbench) tile (same as a snapshot).
                // Composited on its own path below (no constellation scene).
                unvisited_card = node
                    .and_then(|(nx, ny)| {
                        crate::card::anchored_card_rect(
                            nx,
                            ny,
                            crate::card::UNVIS_W,
                            crate::card::UNVIS_H,
                            orrery_rect,
                        )
                    })
                    .map(|(x0, y0, x1, y1, _, _)| (member, [x0, y0, x1, y1]));
            }
        }
        (snapshot_card, unvisited_card)
    }

    /// Rasterize each content card as a vertical band of its content (centred on the
    /// scroll, with overscan), caching the texture per member and reusing it when the
    /// version + size + coverage are unchanged. The HTML/serval lane windows actor-side
    /// (request_scroll then composite the delivered band); the document lane windows the
    /// retained packet host-side. Returns `composite` — the dest rect + member to draw, in
    /// order. (Extracted from `render()`.)
    pub(super) fn rasterize_cards(
        &mut self,
        core: &RenderCore,
        dpr: f32,
        cards: &[(GraphMemberId, [f32; 4], (u32, u32))],
    ) -> Vec<([f32; 4], GraphMemberId)> {
        // Rasterize each tile's scene to an offscreen texture only when its version
        // or size changed; reuse the cached texture otherwise, so an unchanged tile
        // is not re-rasterized every frame (the cost that scaled with tile count).
        // The cache (self.view.tile_textures) keeps the textures alive across frames; evict
        // closed tiles first so theirs free. `composite` is what to draw, in order.
        self.view.tile_textures
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        self.view.tile_bands
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        // Rasterize each card as a vertical BAND of its content, not one giant
        // texture: a tall document (a 166 KB gemtext capsule lays out to ~19000 px)
        // overflows the GPU texture limits and fails to rasterize whole. The host
        // lowers + rasterizes only the band the scroll sits in (centred, with
        // overscan), UV-shifts within it for fine scroll, and re-rasters when the
        // scroll leaves the band. The full scroll range is the document's real height.
        // (Retained-text / tiled render; document lane. The HTML/serval lane still
        // rasterizes one capped texture until Phase 5 lane parity.)
        const BAND_CAP: u32 = 6144;
        // The HTML/serval lane re-emits its band actor-side, culled to the band
        // viewport, so the constraint is the band's *op density* (the whole dense
        // page's op count overflowed vello's encode budget even capped to a 6144
        // texture), not the texture size. Keep the band near twice the visible window
        // so density stays close to the single viewport that always rendered fine.
        // The document lane keeps the larger BAND_CAP — its packet windowing is cheap.
        const HTML_BAND_CAP: u32 = 2560;
        // Cap the texture *area* too: vello binds width*height*4 bytes against wgpu's
        // 128 MiB downlevel-minimum `max_buffer_binding_size`, so a wide+tall band
        // would overflow. ~30 MiB stays well under; the band height is reduced to fit.
        // (Render-target clamp.)
        const MAX_CARD_TEX_AREA: u32 = 30 * 1024 * 1024;
        let mut composite: Vec<([f32; 4], GraphMemberId)> = Vec::with_capacity(cards.len());
        for (member, dest, (cw, ch)) in cards {
            // A live tile/preview bumps scene_version each render; a static snapshot
            // has version 0, so its band rasterizes once and stays cached until scroll.
            let version = self.shared.content.constellation.scene_version(*member);
            let dest_w = (dest[2] - dest[0]).max(1.0);
            let dest_h = (dest[3] - dest[1]).max(1.0);
            // Document-px shown in the dest rect (= ch for a 1:1 live card; less for a
            // downscaled snapshot thumbnail).
            let visible_h = dest_h * (*cw as f32) / dest_w;
            let content_h = (self.shared.content.constellation.content_height(*member) as f32)
                .max(visible_h)
                .max(*ch as f32);
            let scroll = self
                .view
                .scroll
                .get(member)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, (content_h - visible_h).max(0.0));
            // The HTML/serval lane windows actor-side (the host cannot lower an
            // arbitrary band of a flat scene, so the actor re-emits the band it is
            // told); the document lane windows host-side (the host lowers any band of
            // the retained packet itself). Branch on which lane this node is.
            // The themed content-card background + document palette (P3); both Copy
            // reads of `presentation`, used by the rasterize clear + the document
            // lane's lower_window below.
            let card_bg = crate::chrome_to_wgpu(self.shared.presentation.chrome_theme.surface_bg);
            let doc_palette = self.shared.presentation.document_palette;
            if self.shared.content.constellation.scene(*member).is_some() {
                // HTML lane. Ask the actor for the band centred on the scroll — a
                // culled re-emit, so only the band's ops are encoded (the whole dense
                // page overflows vello). Keep the band near 2x the visible window so op
                // density stays close to the single viewport that always rendered.
                let overscan = visible_h * 0.5;
                let desired_h = (visible_h + 2.0 * overscan)
                    .min(content_h)
                    .min(HTML_BAND_CAP as f32)
                    .max(1.0);
                let desired_y = (scroll - overscan).clamp(0.0, (content_h - desired_h).max(0.0));
                self.shared.content.constellation.request_scroll(
                    *member,
                    desired_y.floor() as u32,
                    desired_h.ceil() as u32,
                );
                // Composite the band the actor has actually delivered (it may lag the
                // request by a frame). A fresh band bumps scene_version, so the cache
                // miss below re-rasterizes exactly when a new band lands; holding still
                // (request deduped, version unchanged) reuses the cached texture and
                // only UV-shifts at composite time.
                let (actor_band_y, actor_band_h) =
                    self.shared.content.constellation.scene_band(*member);
                let band_px = actor_band_h.max(1);
                let fresh = self
                    .view
                    .tile_textures
                    .get(member)
                    .is_some_and(|c| c.version == version && c.size == (*cw, band_px));
                if !fresh {
                    if let Some(scene) = self.shared.content.constellation.scene(*member) {
                        // Build + register the page's blurred box-shadow masks (GPU)
                        // before rasterizing, so the shadow image ops resolve to a
                        // texture instead of being skipped. Per member right before its
                        // own rasterize, so per-scene mask keys never collide across
                        // cards. (Box-shadow.)
                        for m in self.shared.content.constellation.scene_masks(*member) {
                            // The mask is in the scene's logical coords; build it at
                            // physical (×dpr) so it stays crisp under the scaled scene.
                            // The key is unchanged, so the scene's shadow op still
                            // resolves it. (Auto-DPI D2 tail.)
                            core.renderer().build_box_shadow_mask(
                                m.key,
                                ((m.dim as f32) * dpr).round().max(1.0) as u32,
                                [
                                    m.bounds[0] * dpr,
                                    m.bounds[1] * dpr,
                                    m.bounds[2] * dpr,
                                    m.bounds[3] * dpr,
                                ],
                                m.corner_radius * dpr,
                                m.blur_radius_px * dpr,
                                m.invert,
                            );
                        }
                        let (tex, view) =
                            core.rasterize_scaled(scene, *cw, band_px, ColorLoad::Clear(card_bg), dpr);
                        self.view.tile_textures.insert(
                            *member,
                            crate::CachedTile { version, size: (*cw, band_px), tex, view },
                        );
                        self.view.tile_bands.insert(*member, actor_band_y as f32);
                    }
                }
            } else {
                // Document lane: window the retained packet to a band centred on the
                // scroll, then lower it. Reuse the cached band if version + width match
                // and it still covers the visible window; otherwise re-pick.
                let max_h_for_width = (MAX_CARD_TEX_AREA / (*cw).max(1)) as f32;
                let band_h = content_h.min(BAND_CAP as f32).min(max_h_for_width).max(1.0);
                let band_px = band_h.ceil() as u32;
                let band_y = self.view.tile_bands.get(member).copied().unwrap_or(0.0);
                let covers = band_y <= scroll && scroll + visible_h <= band_y + band_h + 0.5;
                let fresh = self
                    .view
                    .tile_textures
                    .get(member)
                    .is_some_and(|c| c.version == version && c.size == (*cw, band_px))
                    && covers;
                if !fresh {
                    let new_band_y = (scroll - (band_h - visible_h) * 0.5)
                        .clamp(0.0, (content_h - band_h).max(0.0));
                    // The packet is in the actor's logical coords; window it with the band
                    // converted to logical (÷dpr), then rasterize the logical scene at
                    // physical via the scale. (Auto-DPI D2.)
                    let doc_scene = self
                        .shared
                        .content
                        .constellation
                        .packet(*member)
                        .map(|(packet, fonts)| {
                            crate::card::lower_window(
                                packet,
                                fonts,
                                new_band_y / dpr,
                                band_h / dpr,
                                doc_palette,
                            )
                        });
                    if let Some(scene) = doc_scene {
                        let (tex, view) =
                            core.rasterize_scaled(&scene, *cw, band_px, ColorLoad::Clear(card_bg), dpr);
                        self.view.tile_textures.insert(
                            *member,
                            crate::CachedTile { version, size: (*cw, band_px), tex, view },
                        );
                        self.view.tile_bands.insert(*member, new_band_y);
                    }
                }
            }
            if self.view.tile_textures.contains_key(member) {
                composite.push((*dest, *member));
            }
        }
        composite
    }
}
