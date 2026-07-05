/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Content-card rendering for [`render`](super): the chrome scene build, the focused-node
//! snapshot / unvisited cards, the per-card on-screen content-rect recording, and the
//! per-card band rasterization. Split from `render.rs` to keep files under the workspace
//! 600-LOC ceiling.

use super::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use layout_dom_api::{DomMutation, LayoutDom};
use rustc_hash::FxHashSet;
use serval_winit_host::RenderCore;

impl crate::WindowCtx<'_> {
    /// Full scrollable height for a composited member in document px. The content
    /// actor reports this for web/document lanes; note tiles are host-rendered, so
    /// they read the height measured by `note_surface`.
    pub(crate) fn member_content_height(&self, member: GraphMemberId, visible_h: f32) -> f32 {
        let actor_h = self.shared.content.constellation.content_height(member) as f32;
        let note_h = self
            .view
            .note_content_heights
            .get(&member)
            .copied()
            .unwrap_or(0) as f32;
        actor_h.max(note_h).max(visible_h)
    }

    /// Record each card's on-screen content rect into `self.view.content_rects` for this
    /// frame, so a wheel over a card scrolls the card rather than panning the orrery: the
    /// live tiles, the unvisited placeholder, the (cached-and-shown) snapshot card, and
    /// every scrying surface. (Extracted from `render()`.)
    pub(super) fn finalize_content_rects(
        &mut self,
        cards: &[(GraphMemberId, [f32; 4], (u32, u32))],
        unvisited_card: Option<(GraphMemberId, [f32; 4])>,
        snapshot_card: &Option<(
            GraphMemberId,
            String,
            [f32; 4],
            Option<(netrender::Scene, u32)>,
        )>,
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
    /// is set: assemble the chrome stylesheet (base + the pre-merged pane CSS from
    /// `gather_chrome_css` — roster + apparatus + utility + gloss outline/recent), render
    /// the shell through its persistent incremental session, time it, and enumerate the
    /// chrome's `<external-texture>` placements. Returns the chrome raster plan,
    /// its build time, and the external-texture placements. (Extracted from `render()`.)
    pub(super) fn render_chrome_scene(
        &mut self,
        w: u32,
        h: u32,
        orrery_rect: [f32; 4],
        cursor: Option<TextCursor>,
        chrome_scroll: &ScrollOffsets<NodeId>,
        pane_css: &[String],
    ) -> (super::paint::ChromeRasterPlan, u128, Vec<(u64, [f32; 4])>) {
        // Build the chrome (shell document) scene now that every folded pane — roster, the
        // list panes, and the settings panes (positioned at this frame's tile rects) — is set,
        // so the one shell render reflects them this frame (no page-switch lag). Moved down
        // from above the workbench block for the settings panes' sake. (Settings lane P1.)
        let chrome_sheet: Vec<&str> = self
            .shared
            .presentation
            .chrome_sheet_refs()
            .into_iter()
            .chain(pane_css.iter().map(String::as_str))
            .collect();
        let chrome_t = Instant::now();
        let mut muts = Vec::new();
        self.view.dom.borrow_mut().drain_mutations(&mut muts);
        let dom = self.view.dom.borrow();
        let orrery_root = crate::first_with_class(&dom, dom.document(), "orrery");
        let orrery_only = orrery_root.is_some_and(|root| {
            !muts.is_empty() && muts.iter().all(|m| mutation_is_under_root(&dom, m, root))
        });
        let orrery_dirty = orrery_root
            .is_some_and(|root| muts.iter().any(|m| mutation_is_under_root(&dom, m, root)));
        let base_dirty = !muts.is_empty() && !orrery_only;
        let scheme_dark = self.shared.presentation.scheme_dark();
        let refresh = PaneSession::refresh(
            &mut self.view.chrome_session,
            &dom,
            &chrome_sheet,
            scheme_dark,
            w,
            h,
            &muts,
        );
        let session = self
            .view
            .chrome_session
            .as_ref()
            .expect("chrome session refreshed");
        let chrome = if let Some(root) = orrery_root {
            let base_sig = chrome_base_sig(&chrome_sheet, scheme_dark);
            let base_stale = self.view.chrome_base_tex.as_ref().map(|c| c.size) != Some((w, h))
                || self.view.chrome_base_sig != base_sig;
            let orrery_size = (
                (orrery_rect[2] - orrery_rect[0]).round().max(1.0) as u32,
                (orrery_rect[3] - orrery_rect[1]).round().max(1.0) as u32,
            );
            let orrery_stale =
                self.view.chrome_orrery_tex.as_ref().map(|c| c.size) != Some(orrery_size);
            let mut skipped = FxHashSet::default();
            skipped.insert(root);
            let base_scene = if !base_dirty && !base_stale {
                None
            } else {
                Some(crate::serval_render::scene_from_session_excluding_subtrees(
                    session.layout(),
                    &dom,
                    cursor,
                    chrome_scroll,
                    &skipped,
                    w,
                    h,
                ))
            };
            let orrery_scene = if !orrery_dirty && !orrery_stale {
                None
            } else {
                crate::serval_render::scene_from_session_subtree(
                    session.layout(),
                    &dom,
                    root,
                    cursor,
                    chrome_scroll,
                    orrery_size.0,
                    orrery_size.1,
                )
            };
            if base_scene.is_some() || orrery_scene.is_some() || (!base_stale && !orrery_stale) {
                super::paint::ChromeRasterPlan::Partitioned {
                    base_scene,
                    orrery_scene,
                    orrery_rect,
                    base_sig,
                }
            } else {
                super::paint::ChromeRasterPlan::Full(crate::serval_render::scene_from_session(
                    session.layout(),
                    &dom,
                    cursor,
                    chrome_scroll,
                    w,
                    h,
                ))
            }
        } else {
            super::paint::ChromeRasterPlan::Full(crate::serval_render::scene_from_session(
                session.layout(),
                &dom,
                cursor,
                chrome_scroll,
                w,
                h,
            ))
        };
        if matches!(chrome, super::paint::ChromeRasterPlan::Full(_)) {
            self.view.chrome_base_tex = None;
            self.view.chrome_orrery_tex = None;
            self.view.chrome_base_sig = 0;
        }
        let chrome_us = chrome_t.elapsed().as_micros();
        // The chrome document's `<external-texture>` elements + their laid-out rects, enumerated
        // now that the chrome session has been laid out, composited at the compositor pass below
        // so each external surface's placement comes from the document. (cond 5.)
        let external_texture_placements = self.external_texture_placements();
        tracing::trace!(
            target: "meerkat::profile",
            orrery_only,
            base_dirty,
            orrery_dirty,
            rebuild = refresh.rebuild,
            structural = refresh.structural,
            mut_count = refresh.mut_count,
            "chrome partition decision"
        );
        (chrome, chrome_us, external_texture_placements)
    }

    /// Compute the orrery's focused-node content card for this frame (when the focused node
    /// is not itself an open tile): a "last visit" snapshot card for a visited node
    /// (rendered host-side from the node thumbnail or durable cache) or a dashed
    /// "double-click to load" placeholder for an unvisited one. Returns
    /// `(snapshot_card, unvisited_card)`.
    /// (Extracted from `render()`.)
    pub(super) fn compute_focus_cards(
        &mut self,
        workbench_rect: Option<[f32; 4]>,
        orrery_rect: [f32; 4],
    ) -> (
        Option<(
            GraphMemberId,
            String,
            [f32; 4],
            Option<(netrender::Scene, u32)>,
        )>,
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
        // Single-node selection summons the snapshot card; nothing else does. The card
        // member is the focused node only when *exactly one* node is selected (and it is
        // not already an open tile — the tile is the view).
        //
        // A multi-node selection (`selected_members().len() > 1`) summons a *connections swatch*
        // instead — built in `render/connections.rs` (`compute_connections_card`) and rendered as
        // DOM by `swatch::connections_swatch_view`, off `compute_focus_card`, not this snapshot
        // path (which the `len == 1` gate below correctly suppresses for a multi-selection). The
        // cartography re-layout, the classifier strip, and per-cell hit-test are P2b/P3/P4.
        // (Swatch primitive — P2 built; see 2026-06-27_swatch_primitive_plan.md.)
        let card_member = self.focused_member().filter(|m| {
            self.orrery().selected_members().len() == 1
                && (workbench_rect.is_none() || !self.view.workbench.open_members().contains(m))
        });
        if let (Some(member), Some(url)) =
            (card_member, self.orrery().focused_url().map(str::to_string))
        {
            // The orrery's static card next to the focused node (fall back to the
            // fixed top-right rect when the node's screen pos is unknown): a visited
            // node shows its "last visit" snapshot (a short peek at the retained
            // scene, no actor), an un-visited node a "double-click to load"
            // placeholder. Double-clicking the node or its card opens it in pelt.
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
                    // Prefer the node's persisted thumbnail PNG when present; else synthesize
                    // the preview scene from the durable cache below. The window-local cache is
                    // member-scoped and carries the URL it was built for, so same-URL siblings do
                    // not alias and an in-place navigation invalidates the stale image.
                    let cached = self
                        .view
                        .snapshot_data_uris
                        .get(&member)
                        .is_some_and(|snapshot| snapshot.url == url);
                    if !cached {
                        let persisted = self
                            .orrery()
                            .graph()
                            .get_node_by_id(member)
                            .and_then(|(_, node)| node.thumbnail_png.as_deref())
                            .and_then(crate::render::png_data_uri);
                        if let Some(data_uri) = persisted {
                            if self.view.snapshot_data_uris.len() >= 256 {
                                self.view.snapshot_data_uris.clear();
                            }
                            self.view.snapshot_data_uris.insert(
                                member,
                                crate::window_view::SnapshotDataUri {
                                    url: url.clone(),
                                    data_uri,
                                },
                            );
                        }
                    }
                    let built = if self
                        .view
                        .snapshot_data_uris
                        .get(&member)
                        .is_some_and(|snapshot| snapshot.url == url)
                    {
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
                        // Reproduce the node's last-known scroll position (if it was
                        // ever an open tile this session) rather than always the page
                        // top — same "exact last viewport" fix as the workbench-tile
                        // fallback. (Node/card summoning design, §5.)
                        let scroll = self.view.scroll.get(&member).copied().unwrap_or(0.0);
                        let (scene, content_height, _links) = crate::card::render_content_scene(
                            &url,
                            state.as_ref(),
                            &self.shared.content.engine_registry,
                            &self.shared.content.route_policy,
                            &loader,
                            RENDER_W,
                            RENDER_H,
                            scroll,
                            &self.shared.presentation.document_sheet_composed(),
                        );
                        Some((scene, content_height))
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
        self.view
            .tile_textures
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        // Drop the no-texture latch for members no longer carded this frame (a
        // closed tile isn't a "recovered" card, so no recovery log for it).
        self.view
            .content_card_unhealthy
            .retain(|m| cards.iter().any(|(cm, _, _)| cm == m));
        self.view
            .tile_bands
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        self.view
            .note_content_heights
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        // Rasterize each card as a vertical BAND of its content, not one giant
        // texture: a tall document (a 166 KB gemtext capsule lays out to ~19000 px)
        // overflows the GPU texture limits and fails to rasterize whole. The host
        // lowers + rasterizes only the band the scroll sits in (centred, with
        // overscan), UV-shifts within it for fine scroll, and re-rasters when the
        // scroll leaves the band. The full scroll range is the document's real height.
        // (Retained-text / tiled render; document lane. The HTML/serval lane uses
        // actor-side band re-emit below.)
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
        // Per-card render inputs (content-card health). DEBUG, so it's off by
        // default and its field reads cost nothing until enabled
        // (`RUST_LOG=meerkat::content_card=debug` or the Apparatus ring); it names
        // exactly what the rasterizer sees per card — the scene/packet presence,
        // op count, and band — so a blank or missing card is attributable without
        // a bespoke trace session.
        for (member, dest, sz) in cards {
            tracing::debug!(
                target: "meerkat::content_card",
                %member,
                dest = ?dest,
                size = ?sz,
                scene = self.shared.content.constellation.scene(*member).is_some(),
                packet = self.shared.content.constellation.packet(*member).is_some(),
                active = self.shared.content.constellation.is_active(*member),
                version = self.shared.content.constellation.scene_version(*member),
                ops = self.shared.content.constellation.scene_stats(*member).map(|s| s.op_count),
                band = ?self.shared.content.constellation.scene_band(*member),
                content_h = self.shared.content.constellation.content_height(*member),
                "content card inputs",
            );
        }
        for (member, dest, (cw, ch)) in cards {
            // A live tile/preview bumps scene_version each render; a static snapshot
            // has version 0, so its band rasterizes once and stays cached until scroll.
            let version = self.shared.content.constellation.scene_version(*member);
            // Focused-card audit (shell paint plan P4 tail): when a settled
            // session re-rasters a card, name WHY (version churn vs size vs
            // band coverage) so the churn source is attributable from a log.
            let cached_tile = self
                .view
                .tile_textures
                .get(member)
                .map(|c| (c.version, c.size));
            let dest_w = (dest[2] - dest[0]).max(1.0);
            let dest_h = (dest[3] - dest[1]).max(1.0);
            // Document-px shown in the dest rect (= ch for a 1:1 live card; less for a
            // downscaled snapshot thumbnail).
            let visible_h = dest_h * (*cw as f32) / dest_w;
            let content_h = self
                .member_content_height(*member, visible_h)
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
            // B: a knot note tile renders through serval (note_view -> ScriptedDom ->
            // netrender), the reframe's native path — not document-canvas. Re-derive the
            // EngineDocument from the node's body and lay it out with note_scene. (Slice B.)
            let knot_url = self
                .orrery()
                .graph()
                .get_node_by_id(*member)
                .map(|(_, n)| n.url().to_string())
                .filter(|u| u.starts_with("knot://"));
            if let Some(url) = knot_url {
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
                    tracing::debug!(
                        target: "meerkat::profile",
                        lane = "knot",
                        member = %member,
                        version,
                        ?cached_tile,
                        covers,
                        "card re-raster"
                    );
                    let new_band_y = (scroll - (band_h - visible_h) * 0.5)
                        .clamp(0.0, (content_h - band_h).max(0.0));
                    let state = self.shared.content.pages.get(&url).cloned();
                    if let Some(doc) = crate::card::engine_document_for(
                        &url,
                        state.as_ref(),
                        &self.shared.content.engine_registry,
                        &self.shared.content.route_policy,
                    ) {
                        let sheet = crate::note_sheet(&self.shared.presentation.chrome_theme);
                        let band = crate::note_surface::note_scene_band(
                            &doc,
                            *cw,
                            visible_h.ceil().max(1.0) as u32,
                            new_band_y.floor().max(0.0) as u32,
                            band_px,
                            &sheet,
                        );
                        self.view
                            .note_content_heights
                            .insert(*member, band.content_height);
                        let note_bg =
                            crate::chrome_to_wgpu(self.shared.presentation.chrome_theme.surface_bg);
                        let (tex, view) = core.rasterize_scaled_for(
                            super::surface_keys::card(*member),
                            &band.scene,
                            *cw,
                            band_px,
                            ColorLoad::Clear(note_bg),
                            dpr,
                        );
                        self.view.tile_textures.insert(
                            *member,
                            crate::CachedTile {
                                version,
                                size: (*cw, band_px),
                                tex,
                                view,
                            },
                        );
                        self.view.tile_bands.insert(*member, new_band_y);
                    }
                }
            } else if self.shared.content.constellation.scene(*member).is_some() {
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
                    tracing::debug!(
                        target: "meerkat::profile",
                        lane = "html",
                        member = %member,
                        version,
                        ?cached_tile,
                        url = self
                            .orrery()
                            .graph()
                            .get_node_by_id(*member)
                            .map(|(_, n)| n.url().to_string())
                            .as_deref(),
                        "card re-raster"
                    );
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
                        let (tex, view) = core.rasterize_scaled_for(
                            super::surface_keys::card(*member),
                            scene,
                            *cw,
                            band_px,
                            ColorLoad::Clear(card_bg),
                            dpr,
                        );
                        self.view.tile_textures.insert(
                            *member,
                            crate::CachedTile {
                                version,
                                size: (*cw, band_px),
                                tex,
                                view,
                            },
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
                    tracing::debug!(
                        target: "meerkat::profile",
                        lane = "doc",
                        member = %member,
                        version,
                        ?cached_tile,
                        covers,
                        "card re-raster"
                    );
                    let new_band_y = (scroll - (band_h - visible_h) * 0.5)
                        .clamp(0.0, (content_h - band_h).max(0.0));
                    // The packet is in the actor's logical coords; window it with the band
                    // converted to logical (÷dpr), then rasterize the logical scene at
                    // physical via the scale. (Auto-DPI D2.)
                    let doc_scene =
                        self.shared
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
                        let (tex, view) = core.rasterize_scaled_for(
                            super::surface_keys::card(*member),
                            &scene,
                            *cw,
                            band_px,
                            ColorLoad::Clear(card_bg),
                            dpr,
                        );
                        self.view.tile_textures.insert(
                            *member,
                            crate::CachedTile {
                                version,
                                size: (*cw, band_px),
                                tex,
                                view,
                            },
                        );
                        self.view.tile_bands.insert(*member, new_band_y);
                    }
                }
            }
            if self.view.tile_textures.contains_key(member) {
                composite.push((*dest, *member));
            }
            // Content-card health invariant: a card in this frame's list must
            // result in *visible paint*. It fails two ways, both silent until now:
            // (1) **no texture** despite ready content (a scene/packet arrived but
            // nothing rasterized — missing lane / rasterize bailout), and (2) a
            // texture built from a **zero-op scene** (it composites, but paints only
            // the background clear — the "card is there but blank" case, exactly
            // what a `composite=1` yet empty card is). A member still loading (no
            // texture *and* no scene/packet yet) is healthy, not alarmed. The render
            // path measured re-raster *cost* but never this presence invariant,
            // which is how "cards don't show" stayed invisible to telemetry. Latched
            // per member so the alarm fires once on entry and once on recovery.
            // (Content-card health.)
            let has_texture = self.view.tile_textures.contains_key(member);
            let scene_present = self.shared.content.constellation.scene(*member).is_some();
            let ops = self
                .shared
                .content
                .constellation
                .scene_stats(*member)
                .map(|s| s.op_count);
            let ready = scene_present || self.shared.content.constellation.packet(*member).is_some();
            let blank_scene = scene_present && ops.unwrap_or(0) == 0;
            let unhealthy = if has_texture { blank_scene } else { ready };
            if unhealthy {
                if self.view.content_card_unhealthy.insert(*member) {
                    tracing::warn!(
                        target: "meerkat::content_card",
                        %member,
                        active = self.shared.content.constellation.is_active(*member),
                        has_texture,
                        scene = scene_present,
                        packet = self.shared.content.constellation.packet(*member).is_some(),
                        ops,
                        band = ?self.shared.content.constellation.scene_band(*member),
                        content_h = self.shared.content.constellation.content_height(*member),
                        reason = if has_texture {
                            "composited a zero-op scene (card paints only its background — blank)"
                        } else {
                            "content is ready but produced no tile texture (no render lane matched / rasterize bailed)"
                        },
                        "content card will not be visible",
                    );
                }
            } else if self.view.content_card_unhealthy.remove(member) {
                tracing::info!(
                    target: "meerkat::content_card",
                    %member,
                    "content card recovered: producing visible paint again",
                );
            }
        }
        // Page Visibility (W3C plan P1): what this frame drew is visible;
        // every other active member goes hidden (scripted timer pump clamps
        // to 1/s). Deduped inside, so a steady composition sends nothing.
        let presented: Vec<GraphMemberId> = cards.iter().map(|(m, _, _)| *m).collect();
        self.shared
            .content
            .constellation
            .apply_presentation(&presented);
        // Frame summary (content-card health): expected vs composited. A divergence
        // is already alarmed per-member above; this is the at-a-glance count.
        tracing::debug!(
            target: "meerkat::content_card",
            cards = cards.len(),
            composited = composite.len(),
            "content cards rasterized",
        );
        composite
    }
}

fn mutation_is_under_root(
    dom: &serval_scripted_dom::ScriptedDom,
    m: &DomMutation<NodeId>,
    root: NodeId,
) -> bool {
    let anchor = match *m {
        DomMutation::Inserted { parent, .. } => parent,
        DomMutation::Removed { former_parent, .. } => former_parent,
        DomMutation::AttributeChanged { node, .. } => node,
        DomMutation::CharacterDataChanged { node } => node,
        DomMutation::SubtreeReplaced { node } => node,
        // A move dirties both ends: the subtree left `from_parent` and arrived
        // under `to_parent`, so this mutation is "under" the root when either
        // side is — a cross-subtree move must invalidate the source partition
        // too, not only the destination. (moveBefore plan S1.)
        DomMutation::Moved {
            from_parent,
            to_parent,
            ..
        } => {
            return crate::serval_render::node_under_root(dom, to_parent, root)
                || crate::serval_render::node_under_root(dom, from_parent, root);
        }
    };
    crate::serval_render::node_under_root(dom, anchor, root)
}

/// Signature of the (sheet, scheme) pair the chrome base texture renders from.
/// The scheme folds in because the pair-baked sheet's strings do NOT change on
/// a light/dark mode flip — only the media evaluation does — and the cached
/// raster must still invalidate. (Theme-modes T2.)
fn chrome_base_sig(sheet: &[&str], scheme_dark: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    scheme_dark.hash(&mut hasher);
    for rule in sheet {
        rule.hash(&mut hasher);
    }
    hasher.finish()
}
