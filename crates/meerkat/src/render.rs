/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render, resize, and toolbar-measurement for [`App`](super::App). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use forme::GraphMemberId;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use netrender::ColorLoad;
use netrender::external_texture::ExternalTexturePlacement;
use pelt_live::{TextCursor, fragments_from_scripted_dom, scene_from_scripted_dom};
use platen_view::{WORKBENCH_SHEET, WorkbenchScene};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;

use std::cell::RefCell;

use super::fetch::{ContentState, Fetched};
use super::resources::{ResourceLoader, ResourceStore};
use frame::PaneContent;

use super::{
    App, CARD_BG, FALLBACK_TOOLBAR_H, all_with_class, first_with_class, frame_view,
    measure_class_bottom, member_attr,
};

impl App {
    /// The toolbar-band height (px), measuring + caching it on first use. The
    /// toolbar is a single flex row, so its border-box height is independent of
    /// the available width/height; measuring once suffices. Used to place the
    /// content root directly below the toolbar.
    pub(super) fn toolbar_height(&mut self) -> u32 {
        if self.toolbar_h == 0 {
            let sheet = self.chrome_sheet_refs();
            let measured = measure_class_bottom(
                &self.dom.borrow(),
                &sheet,
                self.width,
                self.height,
                "toolbar",
            )
            .unwrap_or(FALLBACK_TOOLBAR_H);
            self.toolbar_h = measured;
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
        self.refresh_a11y_summary();
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

        // Reserve / drop the Comms frame leaf to match the chrome's comms-open state
        // before laying the panes out, so the other panes make room for it. (Comms.)
        self.sync_comms_pane();

        // Frame tree: the content band (below the toolbar) split into pane rects.
        // The orrery (always) renders into its leaf; the workbench + summoned panes
        // render into theirs. With a single leaf, `orrery_rect` is the whole band.
        let band = [0.0, toolbar_h as f32, w as f32, h as f32];
        let leaves = frame_view::leaf_rects(&self.frame_layout, band, self.maximized_pane);
        // The orrery is the always-present graph pane; the tiled workbench is its
        // summonable sibling. Each renders into its own leaf. (Workbench-as-pane.)
        let orrery_rect = leaves
            .iter()
            .find(|l| matches!(l.content, PaneContent::Orrery))
            .map(|l| l.rect)
            .unwrap_or(band);
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
        let dividers = frame_view::divider_rects(&self.frame_layout, band, self.maximized_pane);
        let orrery_w = (orrery_rect[2] - orrery_rect[0]).round().max(1.0) as u32;
        let orrery_h = (orrery_rect[3] - orrery_rect[1]).round().max(1.0) as u32;

        // Chrome scene over the full window. Paint the caret / selection of the
        // focused field — the palette query when open, else the omnibar (byte
        // offsets from the field's char model).
        let cursor = self.runner.focus().map(|node| {
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
        // Position the chrome's comms overlay into its frame leaf (it's chrome-
        // rendered but laid out by the frame tree): set the geometry inline so it
        // fills the reserved Comms leaf rect. (Comms pane.)
        if let Some(cr) = comms_rect {
            let mut dom = self.dom.borrow_mut();
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
        let chrome_sheet = self.chrome_sheet_refs();
        let chrome_scene =
            scene_from_scripted_dom(&self.dom.borrow(), &chrome_sheet, w, h, cursor, &scroll);

        // Color the orrery's nodes by activation state (green open / red closed /
        // blue new) so the graph shows at a glance what's live. (Visible in
        // Cartography; the orrery is hidden in the tiled view.)
        let states = self.node_states();
        self.orrery.set_node_states(states);
        // Shape each node by its content type (square document / rounded menu /
        // circle feed), the same per-node-hint path as the color states.
        let shapes = self.node_shapes();
        self.orrery.set_node_shapes(shapes);

        // The orrery always composites its own scene into its leaf (kept in sync,
        // centered once). The tiled workbench, when its pane is open, composites a
        // separate scene into its own leaf — the two coexist now, no longer toggled.
        self.orrery.resize(orrery_w, orrery_h);
        if !self.centered {
            self.orrery.recenter();
            self.centered = true;
        }
        let (orrery_scene, orrery_redraw) = self.orrery.frame(orrery_w, orrery_h);
        // The workbench root (a serval flex-DOM document) for the Workbench pane;
        // taffy lays the tiles out. `(scene, w, h)` so the composite can rasterize
        // it at the pane size. `None` when the workbench pane isn't open.
        let mut workbench_scene: Option<(netrender::Scene, u32, u32)> = None;
        if let Some(wr) = workbench_rect {
            let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
            let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
            let mut scene = WorkbenchScene::from_workbench(
                &self.workbench,
                self.orrery.graph(),
                (ww as f32, wh as f32),
                |m| self.constellation.is_background(m),
                |m| self.constellation.is_recovering(m),
            );
            scene.drag_target = self.drag_target_member();
            if self.workbench_runner.state() != &scene {
                self.workbench_runner.update(move |s| *s = scene);
            }
            let wb = scene_from_scripted_dom(
                &self.workbench_dom.borrow(),
                WORKBENCH_SHEET,
                ww,
                wh,
                None,
                &scroll,
            );
            workbench_scene = Some((wb, ww, wh));
        }

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
        let mut snapshot_card: Option<(
            GraphMemberId,
            String,
            [f32; 4],
            Option<(netrender::Scene, u32)>,
        )> = None;
        let mut live_card: Option<(GraphMemberId, [f32; 4])> = None;
        if let Some(wr) = workbench_rect {
            // Read each content placeholder's laid-out rect + member out of the
            // workbench DOM (taffy laid it out above), then drive that tile's actor
            // and queue it to composite at that rect. taffy layouts are
            // *parent-relative*, so sum the workbench > slot > content chain for an
            // absolute rect — otherwise every slot's content reports the same
            // slot-local origin and the tiles stack on each other. The collect
            // releases the DOM borrow before we mutate self. (member, content rect,
            // full slot rect) in window coords, offset by the workbench leaf origin.
            let ww = (wr[2] - wr[0]).round().max(1.0) as u32;
            let wh = (wr[3] - wr[1]).round().max(1.0) as u32;
            let placements: Vec<(GraphMemberId, [f32; 4], [f32; 4])> = {
                let (ox, oy) = (wr[0], wr[1]);
                let dom = self.workbench_dom.borrow();
                let frags = fragments_from_scripted_dom(&dom, WORKBENCH_SHEET, ww, wh);
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
                        let cx = ox + wx + sl.location.x + cl.location.x;
                        let cy = oy + wy + sl.location.y + cl.location.y;
                        let content_rect = [cx, cy, cx + cl.size.width, cy + cl.size.height];
                        let sx = ox + wx + sl.location.x;
                        let sy = oy + wy + sl.location.y;
                        let slot_rect = [sx, sy, sx + sl.size.width, sy + sl.size.height];
                        Some((member, content_rect, slot_rect))
                    })
                    .collect()
            };
            let mut slot_rects = Vec::with_capacity(placements.len());
            for (member, content, slot) in placements {
                slot_rects.push((member, slot));
                let Some(url) = self
                    .orrery
                    .graph()
                    .get_node_by_id(member)
                    .map(|(_, n)| n.url().to_string())
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
        } else {
            self.tile_rects.clear(); // no tile drag targets when the pane is closed
        }
        // The orrery's focused-node card (always, alongside any workbench pane).
        if let (Some(member), Some(url)) = (
            self.focused_member(),
            self.orrery.focused_url().map(str::to_string),
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
                .orrery
                .focused_node_screen()
                .map(|(nx, ny)| (orrery_rect[0] + nx, orrery_rect[1] + ny));
            if self.live_previews.contains(&member) {
                const LIVE_W: u32 = 300;
                const LIVE_H: u32 = 400;
                let rect = node
                    .and_then(|(nx, ny)| {
                        super::card::anchored_card_rect(nx, ny, LIVE_W, LIVE_H, orrery_rect)
                    })
                    .or_else(|| super::card::card_rect(orrery_rect));
                if let Some((x0, y0, x1, y1, cw, ch)) = rect {
                    self.ensure_content(&url);
                    let state = self.content.get(&url).cloned();
                    self.constellation.drive(member, &url, state, cw, ch);
                    cards.push((member, [x0, y0, x1, y1], (cw, ch)));
                    live_card = Some((member, [x0, y0, x1, y1]));
                }
            } else if self.orrery.member_visited(member) {
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
                        super::card::anchored_card_rect(nx, ny, UNVIS_W, UNVIS_H, orrery_rect)
                    })
                    .map(|(x0, y0, x1, y1, _, _)| (member, [x0, y0, x1, y1]));
            }
        }

        // Record each card's on-screen content rect so a wheel over it scrolls the
        // card (resolved in the wheel handler) rather than panning the orrery. The
        // unvisited placeholder counts as a card too, so a double-click over it
        // promotes (and a click on it doesn't deselect the node).
        self.content_rects = cards
            .iter()
            .map(|(member, dest, _)| (*member, *dest))
            .collect();
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
        self.drain_portable_diagnostics();

        let apparatus_data = if self.apparatus_leaf_rect().is_some() {
            Some((self.apparatus_system_rows(), self.apparatus_observability()))
        } else {
            None
        };

        let host = self.host.as_ref().unwrap();
        let (_chrome_tex, chrome_view) = host.rasterize(
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
        let (_orrery_tex, orrery_view) = host.rasterize(
            &orrery_scene,
            orrery_w,
            orrery_h,
            ColorLoad::Clear(backdrop),
        );
        // Rasterize the workbench pane scene too, when its pane is open. The tex is
        // bound to `_workbench_tex` so it outlives the composite below.
        let (_workbench_tex, workbench_view) = match workbench_scene.as_ref() {
            Some((scene, ww, wh)) => {
                let (tex, view) = host.rasterize(scene, *ww, *wh, ColorLoad::Clear(backdrop));
                (Some(tex), Some(view))
            }
            None => (None, None),
        };
        // Rasterize each tile's scene to an offscreen texture only when its version
        // or size changed; reuse the cached texture otherwise, so an unchanged tile
        // is not re-rasterized every frame (the cost that scaled with tile count).
        // The cache (self.tile_textures) keeps the textures alive across frames; evict
        // closed tiles first so theirs free. `composite` is what to draw, in order.
        self.tile_textures
            .retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
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
                        super::CachedTile {
                            version,
                            size: (*cw, tex_h),
                            tex,
                            view,
                        },
                    );
                }
            }
            if self.tile_textures.contains_key(member) {
                composite.push((*dest, *member));
            }
        }

        let Some(frame) = host.acquire() else { return };
        let target_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let format = host.format();
        // Content fills [toolbar_h, h] (dest_rect is [x0, y0, x1, y1] corners;
        // viewport is the full surface). Then each content card floats over it, and
        // the transparent-cleared chrome composites over the whole window — toolbar
        // + dropdown on top, the rest letting the content through.
        host.renderer().compose_external_texture(
            &orrery_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new(orrery_rect),
        );
        if let (Some(wb_view), Some(wr)) = (&workbench_view, workbench_rect) {
            host.renderer().compose_external_texture(
                wb_view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(wr),
            );
        }
        for (dest, member) in &composite {
            let Some(cached) = self.tile_textures.get(member) else {
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
                .scroll
                .get(member)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, max_scroll);
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
        // The X sits on the orrery's live-preview card only (`live_card`); tiles in
        // the workbench pane have their own tab close, so the button never lands on
        // a tile even when the same node is both previewed and tiled.
        self.close_button_rects.clear();
        if let Some((member, dest)) = live_card {
            let btn = super::card::CLOSE_BTN;
            let inset = super::card::CLOSE_BTN_INSET;
            let size = btn.round().max(1.0) as u32;
            if self.close_button_tex.as_ref().map(|c| c.size) != Some((size, size)) {
                let scene = super::card::close_button_scene(size);
                let (tex, view) = host.rasterize(
                    &scene,
                    size,
                    size,
                    ColorLoad::Clear(wgpu::Color::TRANSPARENT),
                );
                self.close_button_tex = Some(super::CachedTile {
                    version: 0,
                    size: (size, size),
                    tex,
                    view,
                });
            }
            if let Some(cached) = &self.close_button_tex {
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
                self.close_button_rects.push((member, [bx0, by0, bx1, by1]));
            }
        }
        // The "last visit" snapshot card: rasterize the host-rendered scene once
        // per url (cached), then composite uniform-scaled into the small dest with
        // the same vertical-scroll UV window as the other cards.
        if let Some((member, url, rect, built)) = snapshot_card {
            if let Some((scene, content_h)) = built {
                let tex_h = content_h.max(1).min(MAX_CARD_TEX_H);
                let (tex, view) = host.rasterize(&scene, 300, tex_h, ColorLoad::Clear(CARD_BG));
                self.snapshot_textures.insert(
                    url.clone(),
                    super::CachedTile {
                        version: 0,
                        size: (300, tex_h),
                        tex,
                        view,
                    },
                );
            }
            if let Some(cached) = self.snapshot_textures.get(&url) {
                let tex_w = cached.size.0 as f32;
                let tex_h = cached.size.1 as f32;
                let dest_w = (rect[2] - rect[0]).max(1.0);
                let dest_h = (rect[3] - rect[1]).max(1.0);
                let visible_h = dest_h * tex_w / dest_w;
                let max_scroll = (tex_h - visible_h).max(0.0);
                let scroll = self
                    .scroll
                    .get(&member)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, max_scroll);
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
                self.unvisited_tex = Some(super::CachedTile {
                    version: 0,
                    size: (uw, uh),
                    tex,
                    view,
                });
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
        // Frame dividers: fill each split gutter with a dark seam (so the gutter is
        // not stale pixels and reads as a divider). (Frame tree, F1.)
        if !dividers.is_empty() {
            if self.divider_tex.as_ref().map(|c| c.size) != Some((1, 1)) {
                let mut scene = netrender::Scene::new(1, 1);
                scene.push_rect(0.0, 0.0, 1.0, 1.0, [0.04, 0.05, 0.07, 1.0]);
                let (tex, view) =
                    host.rasterize(&scene, 1, 1, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
                self.divider_tex = Some(super::CachedTile {
                    version: 0,
                    size: (1, 1),
                    tex,
                    view,
                });
            }
            if let Some(cached) = &self.divider_tex {
                for d in &dividers {
                    host.renderer().compose_external_texture(
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
        // The roster pane: render the node list into its leaf, composite it, and
        // record each row's window rect for click-to-focus. (Frame tree, F1 roster.)
        self.roster_row_rects.clear();
        if let Some(rrect) = roster_rect {
            let rw = (rrect[2] - rrect[0]).round().max(1.0) as u32;
            let rh = (rrect[3] - rrect[1]).round().max(1.0) as u32;
            let rows = self.roster_rows();
            let dom = super::roster::build_roster_dom(&rows);
            let sheet_strings = super::roster::roster_sheet(&self.chrome_theme);
            let sheet: Vec<&str> = sheet_strings.iter().map(String::as_str).collect();
            let roster_scroll = ScrollOffsets::<NodeId>::default();
            let scene = scene_from_scripted_dom(&dom, &sheet, rw, rh, None, &roster_scroll);
            let pb = self.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = host.rasterize(&scene, rw, rh, ColorLoad::Clear(clear));
            host.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(rrect),
            );
            // Row rects for hit-testing (window coords = roster origin + leaf-local).
            let frags = fragments_from_scripted_dom(&dom, &sheet, rw, rh);
            let root = dom.document();
            let mut row_nodes = all_with_class(&dom, root, "roster-row");
            row_nodes.extend(all_with_class(&dom, root, "roster-row-selected"));
            for node in row_nodes {
                if let (Some(member), Some(l)) = (member_attr(&dom, node), frags.rect_of(node)) {
                    let x0 = rrect[0] + l.location.x;
                    let y0 = rrect[1] + l.location.y;
                    self.roster_row_rects
                        .push((member, [x0, y0, x0 + l.size.width, y0 + l.size.height]));
                }
            }
        }
        // The apparatus pane: theme buttons + system diagnostics, rendered into
        // its leaf with button hit-rects recorded for theme switching. (A1.)
        self.apparatus_button_rects.clear();
        if let Some(arect) = self.apparatus_leaf_rect() {
            let aw = (arect[2] - arect[0]).round().max(1.0) as u32;
            let ah = (arect[3] - arect[1]).round().max(1.0) as u32;
            let themes = self.theme_options();
            let (system_rows, observability) = apparatus_data
                .as_ref()
                .expect("apparatus data was prepared when the pane was open");
            let dom = super::apparatus::build_apparatus_dom(&themes, &system_rows, &observability);
            let sheet_strings = super::apparatus::apparatus_sheet(&self.chrome_theme);
            let sheet: Vec<&str> = sheet_strings.iter().map(String::as_str).collect();
            let app_scroll = ScrollOffsets::<NodeId>::default();
            let scene = scene_from_scripted_dom(&dom, &sheet, aw, ah, None, &app_scroll);
            let pb = self.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = host.rasterize(&scene, aw, ah, ColorLoad::Clear(clear));
            host.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(arect),
            );
            let frags = fragments_from_scripted_dom(&dom, &sheet, aw, ah);
            let root = dom.document();
            let mut buttons = all_with_class(&dom, root, "app-btn");
            buttons.extend(all_with_class(&dom, root, "app-btn-active"));
            for node in buttons {
                if let (Some(id), Some(l)) = (
                    super::string_attr(&dom, node, "data-theme"),
                    frags.rect_of(node),
                ) {
                    let x0 = arect[0] + l.location.x;
                    let y0 = arect[1] + l.location.y;
                    self.apparatus_button_rects
                        .push((id, [x0, y0, x0 + l.size.width, y0 + l.size.height]));
                }
            }
        }
        for leaf in self
            .laid_leaves()
            .into_iter()
            .filter(|leaf| matches!(leaf.content, PaneContent::Inspector | PaneContent::Steward))
        {
            let rect = leaf.rect;
            let pw = (rect[2] - rect[0]).round().max(1.0) as u32;
            let ph = (rect[3] - rect[1]).round().max(1.0) as u32;
            let rows = self.utility_pane_rows(&leaf.content);
            let dom = super::utility_panes::build_utility_pane_dom(&leaf.content, &rows);
            let sheet_strings = super::utility_panes::utility_pane_sheet(&self.chrome_theme);
            let sheet: Vec<&str> = sheet_strings.iter().map(String::as_str).collect();
            let pane_scroll = ScrollOffsets::<NodeId>::default();
            let scene = scene_from_scripted_dom(&dom, &sheet, pw, ph, None, &pane_scroll);
            let pb = self.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = host.rasterize(&scene, pw, ph, ColorLoad::Clear(clear));
            host.renderer().compose_external_texture(
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
        self.gloss_node_rects.clear();
        if let Some(grect) = self.gloss_leaf_rect() {
            let gw = (grect[2] - grect[0]).round().max(1.0) as u32;
            let gh = (grect[3] - grect[1]).round().max(1.0) as u32;
            let (nodes, edges) = self.orrery.minimap_geometry();
            let (scene, local) =
                super::gloss::minimap_scene(&nodes, &edges, gw, gh, &self.chrome_theme);
            let pb = self.chrome_theme.panel_bg.to_array();
            let clear = wgpu::Color {
                r: pb[0] as f64 / 255.0,
                g: pb[1] as f64 / 255.0,
                b: pb[2] as f64 / 255.0,
                a: 1.0,
            };
            let (_t, view) = host.rasterize(&scene, gw, gh, ColorLoad::Clear(clear));
            host.renderer().compose_external_texture(
                &view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(grect),
            );
            for (id, r) in local {
                self.gloss_node_rects.push((
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
        host.renderer().compose_external_texture(
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
        if self.window_controls_tex.as_ref().map(|c| c.size) != Some((strip_w, band_h)) {
            let scene = super::titlebar::controls_scene(band_h, &self.chrome_theme);
            let (tex, view) = host.rasterize(
                &scene,
                strip_w,
                band_h,
                ColorLoad::Clear(wgpu::Color::TRANSPARENT),
            );
            self.window_controls_tex = Some(super::CachedTile {
                version: 0,
                size: (strip_w, band_h),
                tex,
                view,
            });
        }
        if let Some(cached) = &self.window_controls_tex {
            let x0 = w as f32 - super::titlebar::CONTROLS_W;
            host.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new([x0, 0.0, w as f32, band_h as f32]),
            );
        }
        frame.present();
        self.refresh_a11y_summary();

        // Keep animating while the orrery is settling / gliding / dragging.
        if orrery_redraw {
            self.request_redraw();
        }
    }
}
