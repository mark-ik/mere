/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The frame paint/compose pass: rasterize the panes, acquire the surface
//! frame, composite every layer, and present. Extracted verbatim from
//! render() (the frame-acquire early-return is the last thing render does, so
//! returning here is equivalent to returning from render). [`super`]

use super::*;

/// A chrome scene plus the box-shadow mask requests it references. The masks must
/// be built (`build_box_shadow_mask`) immediately before this scene rasterizes, or
/// its blurred shadows reference unbuilt mask images and paint black. (Box-shadow.)
type ChromeScene = (netrender::Scene, Vec<paint_list_render::BoxShadowMaskRequest>);

pub(super) enum ChromeRasterPlan {
    Full(ChromeScene),
    Partitioned {
        base_scene: Option<ChromeScene>,
        orrery_scene: Option<ChromeScene>,
        orrery_rect: [f32; 4],
        base_sig: u64,
    },
}

/// Build every box-shadow mask a chrome scene references, immediately before it
/// rasterizes. Chrome scenes rasterize 1:1 (`rasterize_for`, not the content
/// lane's `rasterize_scaled_for`), so the mask geometry is used verbatim — no DPR
/// scaling, unlike the per-card build. Masks for the base and orrery sub-scenes
/// reuse the same key range, so each must be built right before its own rasterize
/// (they rasterize sequentially, so the registry holds the right masks each time).
fn build_chrome_masks(
    core: &serval_winit_host::RenderCore,
    masks: &[paint_list_render::BoxShadowMaskRequest],
) {
    for m in masks {
        core.renderer().build_box_shadow_mask(
            m.key,
            m.dim,
            m.bounds,
            m.corner_radius,
            m.blur_radius_px,
            m.invert,
        );
    }
}

/// All the per-frame build-up outputs the paint pass consumes, bundled so
/// the call site is one argument rather than twenty positionals.
pub(super) struct PaintInputs {
    pub chrome: ChromeRasterPlan,
    pub orrery_scene: netrender::Scene,
    pub orrery_redraw: bool,
    pub orrery_w: u32,
    pub orrery_h: u32,
    pub secondary_orreries: Vec<(netrender::Scene, [f32; 4], u32, u32)>,
    pub workbench_scene: Option<(netrender::Scene, u32, u32)>,
    pub workbench_ghost: Option<((f32, f32, f32, f32), netrender::Scene)>,
    pub workbench_rect: Option<[f32; 4]>,
    /// The gloss minimap's backdrop (edges + signal rings) Scene + its pixel size,
    /// built by `render_gloss_minimap` — `None` when the gloss pane is closed or
    /// empty. Rasterized here, then composited generically by `compose_surfaces`'s
    /// `external_texture_placements` loop at `GLOSS_MINIMAP_SCENE_KEY` (the DOM
    /// layout owns the placement rect now, like the orrery's own backdrop — no
    /// manual rect tracking). The minimap's node squares, plus the outline + recent
    /// lenses, are already folded into the chrome DOM textures above. (Scene-to-DOM
    /// migration P1 outline/recent, P2 minimap.)
    pub gloss_minimap_scene: Option<(netrender::Scene, u32, u32)>,
    pub cards: Vec<(GraphMemberId, [f32; 4], (u32, u32))>,
    pub scrying_surfaces: Vec<(GraphMemberId, [f32; 4])>,
    pub snapshot_card: Option<(
        GraphMemberId,
        String,
        [f32; 4],
        Option<(netrender::Scene, u32)>,
    )>,
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
            chrome,
            orrery_scene,
            orrery_redraw,
            orrery_w,
            orrery_h,
            secondary_orreries,
            workbench_scene,
            workbench_ghost,
            workbench_rect,
            gloss_minimap_scene,
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
        let secondary_count = secondary_orreries.len();
        let mut snapshot_cache_fill_us = 0;
        let chrome_raster_t = std::time::Instant::now();
        let (_chrome_tex, chrome_view, partitioned_orrery_dom): (
            Option<wgpu::Texture>,
            wgpu::TextureView,
            Option<(wgpu::TextureView, [f32; 4])>,
        ) = match chrome {
            ChromeRasterPlan::Full((chrome_scene, masks)) => {
                build_chrome_masks(core, &masks);
                let (tex, view) = core.rasterize_for(
                    super::surface_keys::CHROME_FULL,
                    &chrome_scene,
                    w,
                    h,
                    ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                );
                (Some(tex), view, None)
            }
            ChromeRasterPlan::Partitioned {
                base_scene,
                orrery_scene,
                orrery_rect,
                base_sig,
            } => {
                if let Some((scene, masks)) = base_scene.as_ref() {
                    build_chrome_masks(core, masks);
                    let (tex, view) = core.rasterize_for(
                        super::surface_keys::CHROME_BASE,
                        scene,
                        w,
                        h,
                        ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                    );
                    self.view.chrome_base_tex = Some(crate::CachedTile {
                        version: 0,
                        size: (w, h),
                        tex,
                        view,
                    });
                    self.view.chrome_base_sig = base_sig;
                }
                let cached = self
                    .view
                    .chrome_base_tex
                    .as_ref()
                    .expect("chrome base texture cached");
                let sw = (orrery_rect[2] - orrery_rect[0]).round().max(1.0) as u32;
                let sh = (orrery_rect[3] - orrery_rect[1]).round().max(1.0) as u32;
                if let Some((scene, masks)) = orrery_scene.as_ref() {
                    build_chrome_masks(core, masks);
                    let (subtree_tex, subtree_view) = core.rasterize_for(
                        super::surface_keys::CHROME_ORRERY,
                        scene,
                        sw,
                        sh,
                        ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                    );
                    self.view.chrome_orrery_tex = Some(crate::CachedTile {
                        version: 0,
                        size: (sw, sh),
                        tex: subtree_tex,
                        view: subtree_view,
                    });
                }
                let cached_subtree = self
                    .view
                    .chrome_orrery_tex
                    .as_ref()
                    .expect("chrome orrery texture cached");
                (
                    None,
                    cached.view.clone(),
                    Some((cached_subtree.view.clone(), orrery_rect)),
                )
            }
        };
        let chrome_raster_us = chrome_raster_t.elapsed().as_micros();
        if let Some(tex) = _chrome_tex.as_ref() {
            maybe_dump_chrome_capture(core.device(), core.queue(), tex, w, h);
        }
        // The orrery paints its own opaque backdrop, but clear to the same dark
        // tone so a resize frame cannot flash white before the backdrop lands.
        let backdrop = wgpu::Color {
            r: 0.067,
            g: 0.078,
            b: 0.100,
            a: 1.0,
        };
        let orrery_raster_t = std::time::Instant::now();
        let (_orrery_tex, orrery_view) = core.rasterize_for(
            super::surface_keys::ORRERY_CANVAS,
            &orrery_scene,
            orrery_w,
            orrery_h,
            ColorLoad::Clear(backdrop),
        );
        let orrery_raster_us = orrery_raster_t.elapsed().as_micros();
        // Rasterize each secondary graph-pane's scene. The textures must outlive
        // the composite below (they back the views), so they are held in this Vec
        // until the command buffer is submitted. (Window composition P2.)
        let secondary_raster_t = std::time::Instant::now();
        let secondary_textures: Vec<(wgpu::Texture, wgpu::TextureView, [f32; 4])> =
            secondary_orreries
                .iter()
                .enumerate()
                .map(|(i, (scene, rect, sw, sh))| {
                    let (tex, view) = core.rasterize_for(
                        super::surface_keys::secondary_orrery(i),
                        scene,
                        *sw,
                        *sh,
                        ColorLoad::Clear(backdrop),
                    );
                    (tex, view, *rect)
                })
                .collect();
        let secondary_raster_us = secondary_raster_t.elapsed().as_micros();
        // Rasterize the workbench pane scene too, when its pane is open. The tex is
        // bound to `_workbench_tex` so it outlives the composite below.
        let workbench_raster_t = std::time::Instant::now();
        let (_workbench_tex, workbench_view) = match workbench_scene.as_ref() {
            Some((scene, ww, wh)) => {
                let (tex, view) = core.rasterize_for(
                    super::surface_keys::WORKBENCH,
                    scene,
                    *ww,
                    *wh,
                    ColorLoad::Clear(backdrop),
                );
                (Some(tex), Some(view))
            }
            None => (None, None),
        };
        let workbench_raster_us = workbench_raster_t.elapsed().as_micros();
        // The gloss minimap's edges/rings backdrop, embedded via `<external-texture>`
        // inside the shell document (its node squares are DOM, already folded into
        // `chrome_scene`) — composited generically below by `compose_surfaces`'s
        // `external_texture_placements` loop, not a manual rect. (Scene-to-DOM
        // migration P2.)
        let pb = self.shared.presentation.chrome_theme.panel_bg.to_array();
        let gloss_clear = wgpu::Color {
            r: pb[0] as f64 / 255.0,
            g: pb[1] as f64 / 255.0,
            b: pb[2] as f64 / 255.0,
            a: 1.0,
        };
        let gloss_minimap_raster_t = std::time::Instant::now();
        let (_gloss_minimap_tex, gloss_minimap_view) = match gloss_minimap_scene.as_ref() {
            Some((scene, mw, mh)) => {
                let (tex, view) = core.rasterize_for(
                    super::surface_keys::GLOSS_MINIMAP,
                    scene,
                    *mw,
                    *mh,
                    ColorLoad::Clear(gloss_clear),
                );
                (Some(tex), Some(view))
            }
            None => (None, None),
        };
        let gloss_minimap_raster_us = gloss_minimap_raster_t.elapsed().as_micros();
        let cards_raster_t = std::time::Instant::now();
        let composite = self.rasterize_cards(core, dpr, &cards);
        let cards_raster_us = cards_raster_t.elapsed().as_micros();

        let surface = self.view.surface.as_ref().expect("window surface present");
        let surface_acquire_t = std::time::Instant::now();
        let Some(frame) = surface.acquire(core) else {
            return;
        };
        let surface_acquire_us = surface_acquire_t.elapsed().as_micros();
        let target_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let format = surface.format();
        // Composite the graph + content surfaces (orrery / secondary / workbench /
        // content cards / find / scrying) onto the frame. See render::compose.
        let compose_surfaces_t = std::time::Instant::now();
        self.compose_surfaces(
            core,
            &target_view,
            format,
            w,
            h,
            dpr,
            orrery_view,
            secondary_textures,
            workbench_view,
            workbench_rect,
            gloss_minimap_view,
            composite,
            external_texture_placements,
            &scrying_surfaces,
        );
        let compose_surfaces_us = compose_surfaces_t.elapsed().as_micros();
        self.view.scrying_rects = scrying_surfaces;
        // The "last visit" snapshot card: rasterize the host-rendered scene once
        // per member+url cache entry, then composite uniform-scaled into the small dest with
        // the same vertical-scroll UV window as the other cards.
        if let Some((member, url, _, built)) = snapshot_card {
            // Never cache/persist a BLANK peek. A snapshot re-rendered before its body
            // is fetched (or with no cached body) produces a 0-op scene; caching it
            // sticks a blank thumbnail that later frames reuse (`built=None` on the
            // cache hit) even after the body arrives — the blank-card bug. Filtering
            // the empty scene out here means the next frame re-renders and caches only
            // once there is real content. (Blank-snapshot fix.)
            if let Some((scene, _content_h)) = built.filter(|(s, _)| !s.ops.is_empty()) {
                let snapshot_cache_fill_t = std::time::Instant::now();
                // Render the page's top peek, read it back, and encode a PNG data-URI for the
                // snapshot card's chrome `<img>`. The card shows only its top band, so a fixed
                // peek size suffices (the `<img>` scales it to the card). The readback blocks,
                // so it is gated on the cache miss above. The image is opaque chrome DOM after
                // the gnodes, so it paints over them and under the overlays — the layering an
                // external-texture could not give. (Layering fix.)
                const PEEK_W: u32 = 300;
                const PEEK_H: u32 = 390;
                // Clear the peek to a page-like light (the shared THUMBNAIL_BG), not
                // the dark chrome CARD_BG — see that const for why. (Card legibility.)
                let (tex, _view) = core.rasterize_for(
                    super::surface_keys::SNAPSHOT_PEEK,
                    &scene,
                    PEEK_W,
                    PEEK_H,
                    ColorLoad::Clear(crate::THUMBNAIL_BG),
                );
                let rgba = read_texture_rgba(core.device(), core.queue(), &tex, PEEK_W, PEEK_H);
                if let Some(png_bytes) = png_bytes_from_rgba(&rgba, PEEK_W, PEEK_H) {
                    if let Some(uri) = png_data_uri(&png_bytes) {
                        // Bound the snapshot cache: each entry is a base64 PNG peek
                        // (tens of KB), so without a cap it grows unbounded over a long
                        // session. Crude but bounded: drop the cache when it gets large; the
                        // few visible cards re-encode once on a later frame. (Cache cap.)
                        if self.view.snapshot_data_uris.len() >= 256 {
                            self.view.snapshot_data_uris.clear();
                        }
                        self.view.snapshot_data_uris.insert(
                            member,
                            crate::window_view::SnapshotDataUri {
                                url: url.clone(),
                                data_uri: uri,
                            },
                        );
                        // Tally against the per-session thumbnail byte budget (node/card
                        // summoning design, §5 item 4) — this on-demand render bypasses
                        // `persist_node_thumbnail_png`'s funnel, so it accounts separately.
                        self.shared.session.thumbnail_bytes_this_session += png_bytes.len();
                        self.orrery_mut()
                            .set_node_thumbnail(member, png_bytes, PEEK_W, PEEK_H);
                    }
                }
                snapshot_cache_fill_us += snapshot_cache_fill_t.elapsed().as_micros();
            }
        }
        // Tile decorations (workbench pane): a "Reloading…" placeholder over a tile
        // whose actor is recovering (respawned, no scene yet — so nothing composited
        // there), and a small amber "kept-warm" badge on a background-pinned tile (the
        // star the old strip carried). Both draw over the tile's content rect; the
        // click-to-pin toggle stays on the Ctrl+B / command path. State is collected
        // first so the composite loop holds no `self` borrow. (Decoration re-applied on
        // the pelt surface path.)
        let overlay_compose_t = std::time::Instant::now();
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
                let card_bg =
                    crate::chrome_to_wgpu(self.shared.presentation.chrome_theme.surface_bg);
                let scene = crate::card::recovering_card_scene(
                    rw,
                    rh,
                    self.shared.presentation.document_palette,
                );
                let (_t, view) = core.rasterize_for(
                    super::surface_keys::SNAPSHOT_CARD,
                    &scene,
                    rw,
                    rh,
                    ColorLoad::Clear(card_bg),
                );
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
                let (_t, view) = core.rasterize_for(
                    super::surface_keys::KEPT_WARM_BADGE,
                    &scene,
                    1,
                    1,
                    ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                );
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
        // "double-click to load" card, document-ordered over the gnodes), so there is no
        // host composite for it; `unvisited_card` survives only as a `content_rects` entry so
        // a double-click over it still opens the node in a pelt tile. (Layering fix.)
        // Frame dividers: fill each split gutter with a dark seam (so the gutter is
        // not stale pixels and reads as a divider). (Frame tree, F1.)
        if !dividers.is_empty() {
            if self.view.divider_tex.as_ref().map(|c| c.size) != Some((1, 1)) {
                let mut scene = netrender::Scene::new(1, 1);
                scene.push_rect(0.0, 0.0, 1.0, 1.0, [0.04, 0.05, 0.07, 1.0]);
                let (tex, view) = core.rasterize_for(
                    super::surface_keys::DIVIDER,
                    &scene,
                    1,
                    1,
                    ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                );
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
            let (_t, view) = core.rasterize_for(
                super::surface_keys::WORKBENCH_GHOST,
                scene,
                gw_px,
                gh_px,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            );
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
        // The gloss pane (the Navigator): all three sections — minimap, outline,
        // recent — are folded into the shell document now. The outline + recent +
        // minimap-node-squares are already part of the chrome DOM textures above; the minimap's
        // edges/rings backdrop was rasterized above (`gloss_minimap_view`) and
        // composites generically below via `compose_surfaces`'s
        // `external_texture_placements` loop, like the orrery's own backdrop. Nothing
        // left to do here. (gloss-outline plan P1; Scene-to-DOM migration P1/P2.)
        core.renderer().compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        if let Some((view, rect)) = partitioned_orrery_dom.as_ref() {
            core.renderer().compose_external_texture(
                view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*rect),
            );
        }
        // Window controls (borderless titlebar): the min / max / close strip drawn
        // over the chrome at the toolbar's top-right. Cached by band size; the input
        // path hit-tests the same geometry, so nothing is recorded here.
        let band_h = toolbar_h.max(1);
        // The controls scale with the chrome (the toolbar reserves a `ui_scale`-scaled
        // right gap), so the strip is `CONTROLS_W × ui_scale` wide and its glyphs scale
        // too — otherwise tiny controls sit in a big gap at zoom/HiDPI. (Auto-DPI.)
        let ctl_scale = self.shared.presentation.ui_scale();
        let strip_w = (crate::titlebar::CONTROLS_W * ctl_scale).round().max(1.0) as u32;
        if self.view.window_controls_tex.as_ref().map(|c| c.size) != Some((strip_w, band_h)) {
            let scene = crate::titlebar::controls_scene(
                band_h,
                &self.shared.presentation.chrome_theme,
                ctl_scale,
            );
            let (tex, view) = core.rasterize_for(
                super::surface_keys::WINDOW_CONTROLS,
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
            let x0 = w as f32 - crate::titlebar::CONTROLS_W * ctl_scale;
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
            let (_t, view) = core.rasterize_for(
                super::surface_keys::EMPTY_STATE_PANEL,
                &scene,
                pw,
                ph,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            );
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
        let overlay_compose_us = overlay_compose_t.elapsed().as_micros();
        let present_t = std::time::Instant::now();
        frame.present();
        let present_us = present_t.elapsed().as_micros();
        let a11y_refresh_t = std::time::Instant::now();
        self.refresh_a11y_summary();
        let a11y_refresh_us = a11y_refresh_t.elapsed().as_micros();

        // C0 baseline: one line per rendered frame (gated by the `meerkat::profile`
        // target). `total_us` is the whole `render()`; `chrome_us` is the chrome
        // cascade+layout+paint pipeline inside it. (cheap-path plan C0.)
        tracing::debug!(
            target: "meerkat::profile",
            total_us = frame_t.elapsed().as_micros(),
            chrome_us,
            chrome_raster_us,
            orrery_raster_us,
            secondary_raster_us,
            secondary_count,
            workbench_raster_us,
            gloss_minimap_raster_us,
            cards_raster_us,
            surface_acquire_us,
            compose_surfaces_us,
            snapshot_cache_fill_us,
            overlay_compose_us,
            present_us,
            a11y_refresh_us,
            "frame render profile"
        );

        // Keep animating while the orrery is settling / gliding / dragging.
        if orrery_redraw {
            self.view.request_redraw();
        }
    }
}
