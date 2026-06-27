/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Surface compositing: paint the graph + content layers (orrery, secondary
//! panes, workbench, content cards, find overlay, scrying surfaces) onto the
//! frame target. The growth point — each new surface that registers a view
//! composites here. Read-only on `self`, so it borrows compatibly with the
//! live `target_view`. [`super::paint`]

use super::*;
use serval_winit_host::RenderCore;

impl crate::WindowCtx<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn compose_surfaces(
        &self,
        core: &RenderCore,
        target_view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
        dpr: f32,
        orrery_view: wgpu::TextureView,
        secondary_textures: Vec<(wgpu::Texture, wgpu::TextureView, [f32; 4])>,
        workbench_view: Option<wgpu::TextureView>,
        workbench_rect: Option<[f32; 4]>,
        composite: Vec<([f32; 4], GraphMemberId)>,
        external_texture_placements: Vec<(u64, [f32; 4])>,
        scrying_surfaces: &[(GraphMemberId, [f32; 4])],
    ) {
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
                    target_view,
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
                target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*rect),
            );
        }
        if let (Some(wb_view), Some(wr)) = (&workbench_view, workbench_rect) {
            core.renderer().compose_external_texture(
                wb_view,
                target_view,
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
                target_view,
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
                            target_view,
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
        for (member, dest) in scrying_surfaces {
            if let Some(view) = self.view.scrying.texture_view(*member) {
                core.renderer().compose_external_texture(
                    view,
                    target_view,
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
    }
}
