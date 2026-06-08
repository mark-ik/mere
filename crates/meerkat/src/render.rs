/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render, resize, and toolbar-measurement for [`App`](super::App). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use forme::GraphMemberId;
use layout_dom_api::LayoutDom;
use netrender::external_texture::ExternalTexturePlacement;
use netrender::ColorLoad;
use pelt_live::{fragments_from_scripted_dom, scene_from_scripted_dom, TextCursor};
use platen_view::{WorkbenchScene, WORKBENCH_SHEET};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;

use std::cell::RefCell;

use super::fetch::{ContentState, Fetched};
use super::resources::{ResourceLoader, ResourceStore};
use super::{
    all_with_class, first_with_class, measure_class_bottom, member_attr, App, CARD_BG,
    CHROME_SHEET, FALLBACK_TOOLBAR_H,
};

impl App {
    /// The toolbar-band height (px), measuring + caching it on first use. The
    /// toolbar is a single flex row, so its border-box height is independent of
    /// the available width/height; measuring once suffices. Used to place the
    /// content root directly below the toolbar.
    pub(super) fn toolbar_height(&mut self) -> u32 {
        if self.toolbar_h == 0 {
            self.toolbar_h =
                measure_class_bottom(&self.dom.borrow(), self.width, self.height, "toolbar")
                    .unwrap_or(FALLBACK_TOOLBAR_H);
        }
        self.toolbar_h
    }

    /// Reconfigure the surface for `(width, height)` and request a redraw.
    pub(super) fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(host) = self.host.as_mut() {
            host.resize(self.width, self.height);
        }
        self.request_redraw();
    }

    /// Render the two authorities and present them. The orrery content root fills
    /// everything below the toolbar; the chrome root is rendered over the full
    /// window with a *transparent* clear, so its toolbar band and any open
    /// dropdown float above the content while the rest lets the orrery show
    /// through. Composite order is content first, then chrome on top.
    pub(super) fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let toolbar_h = self.toolbar_height().min(h);
        let content_h = h.saturating_sub(toolbar_h).max(1);

        // Chrome scene over the full window. Paint the caret / selection of the
        // focused field — the palette query when open, else the omnibar (byte
        // offsets from the field's char model).
        let cursor = self.runner.focus().map(|node| {
            let field = self.runner.state().active_field();
            let byte_of = |i: usize| {
                field.text().char_indices().nth(i).map(|(b, _)| b).unwrap_or(field.text().len())
            };
            let selection = field.has_selection().then(|| {
                let (s, e) = field.selection();
                (byte_of(s), byte_of(e))
            });
            TextCursor { node, caret: field.caret_byte_in_render(), selection }
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        let chrome_scene =
            scene_from_scripted_dom(&self.dom.borrow(), CHROME_SHEET, w, h, cursor, &scroll);

        // Color the orrery's nodes by activation state (green open / red closed /
        // blue new) so the graph shows at a glance what's live. (Visible in
        // Cartography; the orrery is hidden in the tiled view.)
        let states = self.node_states();
        self.orrery.set_node_states(states);

        // The content root. In Cartography the orrery composites its own scene over
        // the band (kept in sync, centered once). In the tiled workbench the orrery
        // is hidden behind the tiles, so skip its physics + paint entirely and back
        // the band with an empty dark-cleared scene — the tiles composite over it,
        // the splitter gutters show the dark. Skipping it also stops the orrery's
        // settle / glide redraw loop, which would otherwise re-rasterize every tile
        // each frame behind the cover.
        let (content_scene, orrery_redraw) = if self.workbench.is_tiled() {
            // Tree: the workbench root (a serval flex-DOM document) is the band. Sync
            // it from the model + graph + pin state, then rasterize it — taffy lays
            // the tiles out (no morphorm). The orrery is hidden, so its physics +
            // paint are skipped.
            let mut scene = WorkbenchScene::from_workbench(
                &self.workbench,
                self.orrery.graph(),
                (w as f32, content_h as f32),
                |m| self.constellation.is_background(m),
                |m| self.constellation.is_recovering(m),
            );
            // Highlight the slot under the pointer while a tab is being dragged
            // (uses last frame's tile rects; the slots don't move, so the lag is
            // imperceptible).
            scene.drag_target = self.drag_target_member();
            if self.workbench_runner.state() != &scene {
                self.workbench_runner.update(move |s| *s = scene);
            }
            let wb = scene_from_scripted_dom(
                &self.workbench_dom.borrow(),
                WORKBENCH_SHEET,
                w,
                content_h,
                None,
                &scroll,
            );
            (wb, false)
        } else {
            self.orrery.resize(w, content_h);
            if !self.centered {
                self.orrery.recenter();
                self.centered = true;
            }
            self.orrery.frame(w, content_h)
        };

        // Reconcile the active-node pool to what this frame shows — the open tiles
        // (Tree) or the focused node (Cartography). Needed-but-dormant nodes spawn
        // an actor; active nodes no longer shown are reaped, unless backgrounded.
        let needed = self.needed_members();
        self.constellation.reconcile(&needed);

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
        let mut snapshot_card: Option<(GraphMemberId, String, [f32; 4], Option<(netrender::Scene, u32)>)> =
            None;
        if self.workbench.is_tiled() {
            // Read each content placeholder's laid-out rect + member out of the
            // workbench DOM (taffy laid it out above), then drive that tile's actor
            // and queue it to composite at that rect (window coords add `toolbar_h`).
            // taffy layouts are *parent-relative*, so sum the workbench > slot >
            // content chain for an absolute rect — otherwise every slot's content
            // reports the same slot-local origin and the tiles stack on each other.
            // The collect releases the DOM borrow before we mutate self.
            // (member, content rect, full slot rect) in window coords. The content
            // rect is where the tile's texture composites (below the strip); the slot
            // rect is the whole column (strip + content), used as the drag target so
            // dragging along the strip still resolves + highlights its slot.
            let placements: Vec<(GraphMemberId, [f32; 4], [f32; 4])> = {
                let th = toolbar_h as f32;
                let dom = self.workbench_dom.borrow();
                let frags = fragments_from_scripted_dom(&dom, WORKBENCH_SHEET, w, content_h);
                let root = dom.document();
                let (wx, wy) = first_with_class(&dom, root, "workbench")
                    .and_then(|n| frags.rect_of(n))
                    .map(|l| (l.location.x, l.location.y))
                    .unwrap_or((0.0, 0.0));
                all_with_class(&dom, root, "wb-slot")
                    .into_iter()
                    .filter_map(|slot| {
                        let sl = frags.rect_of(slot)?;
                        let content = first_with_class(&dom, slot, "wb-content")?;
                        let member = member_attr(&dom, content)?;
                        let cl = frags.rect_of(content)?;
                        let cx = wx + sl.location.x + cl.location.x;
                        let cy = th + wy + sl.location.y + cl.location.y;
                        let content_rect = [cx, cy, cx + cl.size.width, cy + cl.size.height];
                        let sx = wx + sl.location.x;
                        let sy = th + wy + sl.location.y;
                        let slot_rect = [sx, sy, sx + sl.size.width, sy + sl.size.height];
                        Some((member, content_rect, slot_rect))
                    })
                    .collect()
            };
            let mut slot_rects = Vec::with_capacity(placements.len());
            for (member, content, slot) in placements {
                slot_rects.push((member, slot));
                let Some(url) =
                    self.orrery.graph().get_node_by_id(member).map(|(_, n)| n.url().to_string())
                else {
                    continue;
                };
                self.ensure_content(&url);
                let cw = (content[2] - content[0]).round().max(1.0) as u32;
                let ch = (content[3] - content[1]).round().max(1.0) as u32;
                let state = self.content.get(&url).cloned();
                self.constellation.drive(member, &url, state, cw, ch);
                cards.push((member, content, (cw, ch)));
            }
            self.tile_rects = slot_rects;
        } else if let (Some(member), Some(url)) =
            (self.focused_member(), self.orrery.focused_url().map(str::to_string))
        {
            // Float the card next to the focused node (fall back to the fixed
            // top-right rect when the node's screen pos is unknown). A live preview
            // is a medium card the actor renders into; a snapshot is a shorter
            // peek at the retained scene, no actor. A node with neither (never
            // visited this session) shows no card yet. (Card system P2/P3.)
            let node = self
                .orrery
                .focused_node_screen()
                .map(|(nx, ny)| (nx, ny + toolbar_h as f32));
            if self.live_previews.contains(&member) {
                const LIVE_W: u32 = 300;
                const LIVE_H: u32 = 400;
                let rect = node
                    .and_then(|(nx, ny)| {
                        super::card::anchored_card_rect(nx, ny, LIVE_W, LIVE_H, w, toolbar_h, h)
                    })
                    .or_else(|| super::card::card_rect(w, toolbar_h, h));
                if let Some((x0, y0, x1, y1, cw, ch)) = rect {
                    self.ensure_content(&url);
                    let state = self.content.get(&url).cloned();
                    self.constellation.drive(member, &url, state, cw, ch);
                    cards.push((member, [x0, y0, x1, y1], (cw, ch)));
                }
            } else if self.orrery.member_visited(member) {
                // "Last visit" snapshot: a small fixed-size card, rendered host-side
                // from the durable cache / synthesis below (no actor), so it survives
                // a restart. Composited uniform-scaled into a scrollable thumbnail.
                const SNAP_W: u32 = 200;
                const SNAP_H: u32 = 260;
                let rect = node
                    .and_then(|(nx, ny)| {
                        super::card::anchored_card_rect(nx, ny, SNAP_W, SNAP_H, w, toolbar_h, h)
                    })
                    .or_else(|| super::card::card_rect(w, toolbar_h, h));
                if let Some((x0, y0, x1, y1, _, _)) = rect {
                    // Render the snapshot scene from cache / synthesis once per url;
                    // `None` means its texture is already cached (composited below).
                    let built = if self.snapshot_textures.contains_key(&url) {
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
                        Some(super::card::render_content_scene(
                            &url,
                            state.as_ref(),
                            &self.engine_registry,
                            &loader,
                            RENDER_W,
                            RENDER_H,
                        ))
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
                        super::card::anchored_card_rect(nx, ny, UNVIS_W, UNVIS_H, w, toolbar_h, h)
                    })
                    .map(|(x0, y0, x1, y1, _, _)| (member, [x0, y0, x1, y1]));
            }
            self.tile_rects.clear(); // no drag targets outside the tiled view
        } else {
            self.tile_rects.clear();
        }

        // Record each card's on-screen content rect so a wheel over it scrolls the
        // card (resolved in the wheel handler) rather than panning the orrery. The
        // unvisited placeholder counts as a card too, so a double-click over it
        // promotes (and a click on it doesn't deselect the node).
        self.content_rects = cards.iter().map(|(member, dest, _)| (*member, *dest)).collect();
        if let Some((member, rect)) = unvisited_card {
            self.content_rects.push((member, rect));
        }
        if let Some((member, _, rect, _)) = &snapshot_card {
            self.content_rects.push((*member, *rect));
        }

        // The omnibar follows focus: point it at the focused tile / node when that
        // changed (next frame, like the chrome strips were — the scene above is
        // already built).
        self.sync_location();
        // Back/forward enabled-state tracks the focused node's own history.
        self.sync_nav_buttons();

        let host = self.host.as_ref().unwrap();
        let (_chrome_tex, chrome_view) =
            host.rasterize(&chrome_scene, w, h, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
        // The orrery paints its own opaque backdrop, but clear to the same dark
        // tone so a resize frame cannot flash white before the backdrop lands.
        let (_content_tex, content_view) = host.rasterize(
            &content_scene,
            w,
            content_h,
            ColorLoad::Clear(wgpu::Color { r: 0.067, g: 0.078, b: 0.100, a: 1.0 }),
        );
        // Rasterize each tile's scene to an offscreen texture only when its version
        // or size changed; reuse the cached texture otherwise, so an unchanged tile
        // is not re-rasterized every frame (the cost that scaled with tile count).
        // The cache (self.tile_textures) keeps the textures alive across frames; evict
        // closed tiles first so theirs free. `composite` is what to draw, in order.
        self.tile_textures.retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        // Rasterize each card at its FULL content height (clamped to the cap), so
        // scrolling is a GPU UV-window shift over the cached tall texture rather
        // than a re-layout per tick (the gpui-flavored path).
        const MAX_CARD_TEX_H: u32 = 8192;
        let mut composite: Vec<([f32; 4], GraphMemberId)> = Vec::with_capacity(cards.len());
        for (member, dest, (cw, ch)) in &cards {
            // A live tile/preview bumps scene_version each scene; a static snapshot
            // has version 0, so its texture rasterizes once and then stays cached.
            let version = self.constellation.scene_version(*member);
            let tex_h = self
                .constellation
                .content_height(*member)
                .max(*ch)
                .min(MAX_CARD_TEX_H);
            let fresh = self
                .tile_textures
                .get(member)
                .is_some_and(|c| c.version == version && c.size == (*cw, tex_h));
            if !fresh {
                if let Some(scene) = self.constellation.scene(*member) {
                    let (tex, view) = host.rasterize(scene, *cw, tex_h, ColorLoad::Clear(CARD_BG));
                    self.tile_textures.insert(
                        *member,
                        super::CachedTile { version, size: (*cw, tex_h), tex, view },
                    );
                }
            }
            if self.tile_textures.contains_key(member) {
                composite.push((*dest, *member));
            }
        }

        let Some(frame) = host.acquire() else { return };
        let target_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let format = host.format();
        // Content fills [toolbar_h, h] (dest_rect is [x0, y0, x1, y1] corners;
        // viewport is the full surface). Then each content card floats over it, and
        // the transparent-cleared chrome composites over the whole window — toolbar
        // + dropdown on top, the rest letting the content through.
        host.renderer().compose_external_texture(
            &content_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, toolbar_h as f32, w as f32, h as f32]),
        );
        for (dest, member) in &composite {
            let Some(cached) = self.tile_textures.get(member) else { continue };
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
            let scroll = self.scroll.get(member).copied().unwrap_or(0.0).clamp(0.0, max_scroll);
            let uv = [0.0, scroll / tex_h, 1.0, (scroll + visible_h) / tex_h];
            host.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*dest).with_uv(uv),
            );
        }
        // Live cards carry a close (X) button at their top-right corner. Rasterize
        // the shared button texture once, composite it on each live card, and
        // record its rect so a press there reaps the live preview. (Card system.)
        self.close_button_rects.clear();
        if composite.iter().any(|(_, m)| self.live_previews.contains(m)) {
            let btn = super::card::CLOSE_BTN;
            let inset = super::card::CLOSE_BTN_INSET;
            let size = btn.round().max(1.0) as u32;
            if self.close_button_tex.as_ref().map(|c| c.size) != Some((size, size)) {
                let scene = super::card::close_button_scene(size);
                let (tex, view) =
                    host.rasterize(&scene, size, size, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                self.close_button_tex =
                    Some(super::CachedTile { version: 0, size: (size, size), tex, view });
            }
            if let Some(cached) = &self.close_button_tex {
                for (dest, member) in &composite {
                    if !self.live_previews.contains(member) {
                        continue;
                    }
                    let bx1 = dest[2] - inset;
                    let bx0 = bx1 - btn;
                    let by0 = dest[1] + inset;
                    let by1 = by0 + btn;
                    host.renderer().compose_external_texture(
                        &cached.view,
                        &target_view,
                        format,
                        w,
                        h,
                        ExternalTexturePlacement::new([bx0, by0, bx1, by1]),
                    );
                    self.close_button_rects.push((*member, [bx0, by0, bx1, by1]));
                }
            }
        }
        // The "last visit" snapshot card: rasterize the host-rendered scene once
        // per url (cached), then composite uniform-scaled into the small dest with
        // the same vertical-scroll UV window as the other cards.
        if let Some((member, url, rect, built)) = snapshot_card {
            if let Some((scene, content_h)) = built {
                let tex_h = content_h.max(1).min(MAX_CARD_TEX_H);
                let (tex, view) = host.rasterize(&scene, 300, tex_h, ColorLoad::Clear(CARD_BG));
                self.snapshot_textures
                    .insert(url.clone(), super::CachedTile { version: 0, size: (300, tex_h), tex, view });
            }
            if let Some(cached) = self.snapshot_textures.get(&url) {
                let tex_w = cached.size.0 as f32;
                let tex_h = cached.size.1 as f32;
                let dest_w = (rect[2] - rect[0]).max(1.0);
                let dest_h = (rect[3] - rect[1]).max(1.0);
                let visible_h = dest_h * tex_w / dest_w;
                let max_scroll = (tex_h - visible_h).max(0.0);
                let scroll = self.scroll.get(&member).copied().unwrap_or(0.0).clamp(0.0, max_scroll);
                let uv = [0.0, scroll / tex_h, 1.0, (scroll + visible_h) / tex_h];
                host.renderer().compose_external_texture(
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
            if self.unvisited_tex.as_ref().map(|c| c.size) != Some((uw, uh)) {
                let scene = super::card::unvisited_card_scene(uw, uh);
                let (tex, view) = host.rasterize(&scene, uw, uh, ColorLoad::Clear(CARD_BG));
                self.unvisited_tex =
                    Some(super::CachedTile { version: 0, size: (uw, uh), tex, view });
            }
            if let Some(cached) = &self.unvisited_tex {
                host.renderer().compose_external_texture(
                    &cached.view,
                    &target_view,
                    format,
                    w,
                    h,
                    ExternalTexturePlacement::new(rect),
                );
            }
        }
        host.renderer().compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        frame.present();

        // Keep animating while the orrery is settling / gliding / dragging.
        if orrery_redraw {
            self.request_redraw();
        }
    }
}
