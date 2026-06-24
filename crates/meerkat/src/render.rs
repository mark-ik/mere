/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Render, resize, and toolbar-measurement for [`Shell`](super::Shell). Factored
//! from `main.rs` to keep files under the workspace 600-LOC ceiling.

use forme::GraphMemberId;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use netrender::ColorLoad;
use netrender::external_texture::{ExternalTexturePlacement, SourceAlpha};
use image::ImageEncoder;
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
    CARD_BG, FALLBACK_TOOLBAR_H, WindowCtx, first_with_class, frame_view, shellbar,
    measure_class_bottom,
};
use meerkat::ShellbarPaneStates;
use crate::pane_session::PaneSession;
use crate::window_view::{OrreryCard, OrreryRender};

/// The node's favicon RGBA (straight-alpha RGBA8) encoded as a `data:image/png;base64,`
/// URI, or `None` if it can't be encoded. The orrery card carries it as a leading
/// `<img>` that serval decodes. PNG (not BMP) keeps alpha and stays compact. (Phase 2.)
/// Also the sprite-import encoder (a downscaled dropped image → the face). (P2 — sprite.)
pub(crate) fn favicon_data_uri(rgba: &[u8], w: u32, h: u32) -> Option<String> {
    if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&png)))
}

/// Read an `Rgba8Unorm` texture back to tightly-packed CPU RGBA bytes (width*height*4),
/// stripping wgpu's per-row alignment padding. Blocks until the copy + map complete, so
/// it is only used off the hot path — once per url for a snapshot card preview, then
/// cached. Mirrors netrender_device's test-only `read_rgba8_texture`. (Layering fix.)
fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let row_bytes = width * 4;
    let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: padded as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("snapshot readback") });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    // A snapshot readback failing (GPU hang / lost device / OOM) must not crash the
    // app: the caller treats an empty result as "skip the snapshot" (favicon_data_uri
    // returns None on a short slice, then the plain stand-in card shows). (Layering fix.)
    if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
        tracing::warn!("snapshot readback poll failed; skipping snapshot");
        return Vec::new();
    }
    if !matches!(rx.recv(), Ok(Ok(()))) {
        tracing::warn!("snapshot readback map failed; skipping snapshot");
        return Vec::new();
    }
    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity((row_bytes * height) as usize);
    for row in 0..height as usize {
        let src = row * padded as usize;
        out.extend_from_slice(&mapped[src..src + row_bytes as usize]);
    }
    out
}

/// Minimal standard base64 (no line breaks), for the favicon data URI. (Phase 2.)
fn base64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | (c.get(2).copied().unwrap_or(0) as u32);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 { A[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    out
}

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

    /// Enumerate the chrome document's `<external-texture>` elements as `(key, [x0, y0, x1, y1])`
    /// in viewport px, from the chrome session's retained layout. The host composites each
    /// element's registered texture (resolved by `key`) at its laid-out rect, so an external
    /// surface's placement comes from the document, not a hardcoded host rect. The
    /// external-texture-element compose. (cond 5.)
    fn external_texture_placements(&self) -> Vec<(u64, [f32; 4])> {
        let Some(session) = self.view.chrome_session.as_ref() else {
            return Vec::new();
        };
        let dom = self.view.dom.borrow();
        let fragments = session.fragments();
        let mut origins = std::collections::HashMap::new();
        crate::serval_render::accumulate_origins(&dom, fragments, dom.document(), (0.0, 0.0), &mut origins);
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
    fn snapshot_list_panes(&mut self, rects: [Option<[f32; 4]>; 4]) {
        use crate::window_view::ShellListPane::{Apparatus, Inspector, Steward, Trail};
        // Apparatus: theme + engine + physics buttons over the host observability rows.
        if let Some(rect) = rects[0] {
            // The apparatus is read-only diagnostics now; its settings sections moved to the
            // pelt settings lane (Settings lane P2).
            let system_rows = self.apparatus_system_rows();
            let obs = self.apparatus_observability();
            let items = crate::apparatus::apparatus_items(&system_rows, &obs);
            self.view.set_list_pane(Apparatus, "apparatus", items, Some(rect));
        } else if self.view.list_pane_open(Apparatus) {
            self.view.set_list_pane(Apparatus, "apparatus", Vec::new(), None);
        }
        // Steward + Inspector: display-only `label: value` rows under a unique utility root.
        if let Some(rect) = rects[1] {
            let rows = self.utility_pane_rows(&PaneContent::Steward);
            let items = crate::utility_panes::utility_pane_items(&PaneContent::Steward, &rows);
            self.view.set_list_pane(Steward, "utility-pane steward", items, Some(rect));
        } else if self.view.list_pane_open(Steward) {
            self.view.set_list_pane(Steward, "utility-pane steward", Vec::new(), None);
        }
        if let Some(rect) = rects[2] {
            let rows = self.utility_pane_rows(&PaneContent::Inspector);
            let items = crate::utility_panes::utility_pane_items(&PaneContent::Inspector, &rows);
            self.view.set_list_pane(Inspector, "utility-pane inspector", items, Some(rect));
        } else if self.view.list_pane_open(Inspector) {
            self.view.set_list_pane(Inspector, "utility-pane inspector", Vec::new(), None);
        }
        // Trail: its own sectioned items (history / recent / removed); a Removed row recovers.
        if let Some(rect) = rects[3] {
            let items = self.trail_items();
            self.view.set_list_pane(Trail, "utility-pane trail", items, Some(rect));
        } else if self.view.list_pane_open(Trail) {
            self.view.set_list_pane(Trail, "utility-pane trail", Vec::new(), None);
        }
    }

    /// The focused node's content-card descriptor for the shell document: its rect (local
    /// to the orrery element) and kind. A visited node gets a snapshot preview (the host
    /// fills its external-texture from the url-cached scene); an unvisited node gets a
    /// dashed placeholder. `None` when no node is focused, or the focused node is an open
    /// workbench tile (the tile is the view; a card would contend for its content actor).
    /// The card is placed *after* the node cards in document order, so it paints over them
    /// while the chrome overlays still paint over it. (Layering fix — card over nodes.)
    fn compute_focus_card(
        &self,
        orrery_rect: [f32; 4],
        workbench_rect: Option<[f32; 4]>,
    ) -> Option<crate::window_view::FocusCard> {
        use crate::window_view::{FocusCard, FocusCardKind};
        let member = self.focused_member().filter(|m| {
            workbench_rect.is_none() || !self.view.workbench.open_members().contains(m)
        })?;
        // The node's pane-local screen position (same camera the node cards use).
        let (nx, ny) = self.orrery().focused_node_screen()?;
        let (pw, ph) = (orrery_rect[2] - orrery_rect[0], orrery_rect[3] - orrery_rect[1]);
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
                        CardWidget::SizeTier { tier: self.orrery().node_size_tier(k) },
                        CardWidget::Face {
                            is_favicon: self.orrery().node_face(k) == orrery::Face::Favicon,
                        },
                    ]
                })
                .unwrap_or_default();
            let ch = super::card::object_card_height(widgets.len());
            (FocusCardKind::ObjectCard { widgets }, super::card::OBJCARD_W, ch)
        } else if self.orrery().member_visited(member) {
            // Show only the *cached* snapshot image — no placeholder while it is still
            // building. A fresh focus otherwise flashed an empty card (and claimed a hit-rect
            // over the node, making the double-click-to-open flaky). `None` → no focus card
            // this frame; the readback below still builds + caches it for the next focus.
            // (Snapshot — no placeholder flash.)
            let data_uri = self
                .orrery()
                .focused_url()
                .and_then(|u| self.view.snapshot_data_uris.get(u).cloned())?;
            (FocusCardKind::Snapshot { data_uri }, super::card::SNAP_W, super::card::SNAP_H)
        } else {
            (FocusCardKind::Unvisited, super::card::UNVIS_W, super::card::UNVIS_H)
        };
        let (x0, y0, x1, y1, _, _) = super::card::anchored_card_rect(nx, ny, cw, ch, local)?;
        Some(FocusCard { rect: [x0, y0, x1, y1], kind })
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
        if self.view.chrome().shellbar_panes != sb_panes
            || self.view.chrome().shellbar_edge != sb_edge
        {
            self.view.chrome_update(move |c| {
                c.shellbar_panes = sb_panes;
                c.shellbar_edge = sb_edge;
            });
        }

        // Sync the focused node's live find-match count into Chrome so the find bar
        // shows "active/total" before the chrome is rasterized this frame. (Find S2.)
        if self.view.chrome().find_open {
            let find_count = self
                .focused_member()
                .map(|m| self.find_matches_for(m).len())
                .unwrap_or(0);
            if self.view.chrome().find_count != find_count {
                self.view.chrome_update(move |c| c.find_count = find_count);
            }
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
        // The four folded list panes' rects, in `ShellListPane` order (apparatus, steward,
        // inspector, trail), for the per-frame snapshot into the shell document. (Phase 1.)
        let list_pane_rects: [Option<[f32; 4]>; 4] = [
            leaves.iter().find(|l| matches!(l.content, PaneContent::Apparatus)).map(|l| l.rect),
            leaves.iter().find(|l| matches!(l.content, PaneContent::Steward)).map(|l| l.rect),
            leaves.iter().find(|l| matches!(l.content, PaneContent::Inspector)).map(|l| l.rect),
            leaves.iter().find(|l| matches!(l.content, PaneContent::Trail)).map(|l| l.rect),
        ];
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
                editable: self.is_text_input(node),
            }
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        // The chrome's own scroll (the command palette follows its selection); kept
        // separate from the content `scroll` so a cmd-list offset never bleeds into
        // another pane's DOM.
        let mut chrome_scroll = ScrollOffsets::<NodeId>::default();
        if self.view.chrome().palette_open {
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
                // Centre the palette over the orrery area, not the full window, so it does
                // not overlap the side panes: the overlay's flex centring is shifted by the
                // orrery rect's insets (which already shrink when a pane opens), and the panel
                // is capped to the orrery width for a narrow canvas. (Phase 1, step 3.)
                let pad_left = orrery_rect[0].max(0.0);
                let pad_right = (w as f32 - orrery_rect[2]).max(0.0);
                let panel_max = (orrery_rect[2] - orrery_rect[0] - 40.0).max(120.0);
                if let Some(node) = first_with_class(&dom, root, "palette-overlay") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(
                        node,
                        attr,
                        &format!(
                            "display: flex; justify-content: center; padding: 56px {pad_right}px 0 {pad_left}px;"
                        ),
                    );
                }
                if let Some(node) = first_with_class(&dom, root, "palette") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(node, attr, &format!("max-width: {panel_max}px;"));
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
        // The context menu follows its keyboard selection like the palette: bound the panel to the
        // window so a tall menu (the layout submenu) can't spill past the bottom edge, and scroll
        // the highlighted row into view. (Context-menu keyboard nav.)
        if let Some((mx, my)) = self.view.chrome().context_menu.as_ref().map(|m| (m.x, m.y)) {
            let max_h = (h as f32 - my - 16.0).max(120.0);
            {
                let mut dom = self.view.dom.borrow_mut();
                let root = dom.document();
                if let Some(node) = first_with_class(&dom, root, "context-menu") {
                    let attr = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                    dom.set_attribute(
                        node,
                        attr,
                        &format!(
                            "position: absolute; left: {mx}px; top: {my}px; overflow: scroll; max-height: {max_h}px;"
                        ),
                    );
                }
            }
            if let Some(session) = &self.view.chrome_session {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                if let (Some(list), Some(active)) = (
                    first_with_class(&dom, root, "context-menu"),
                    first_with_class(&dom, root, "context-item-active"),
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
        // The focused orrery renders its on-screen nodes as DOM cards in the shell (the
        // snapshot below), so drop its in-scene gnode layer. (Orrery-as-element.)
        self.pane_orrery_mut(orrery_gid).set_render_as_cards(true);
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
        // Live workbench mirror: re-scope the focused orrery to the workbench's open
        // tiles each frame, so the spatial map tracks the tile set as it changes (the
        // two surfaces stay in lockstep). (Curated orrery — workbench mirror.)
        if self.view.mirror_tiles {
            let members = self.view.workbench.open_members();
            self.pane_orrery_mut(orrery_gid).scope_to_members(members);
        }
        // Drive the pane's active layout strategy (if any): compute its node positions
        // through platen's cartography dispatch and push them in; the orrery overlays
        // them on the physics snapshot each frame. No-op under force-directed. (Layout picker.)
        if let Some(id) = self.pane_orrery(orrery_gid).layout_strategy().map(str::to_string) {
            // Focus-driven strategies (radial) center on the pane's single selection;
            // passing it each frame lets radial re-center live as the selection moves.
            // The graph-only strategies ignore it. (Layout picker.)
            let pane = self.pane_orrery(orrery_gid);
            let positions =
                platen::project_orrery_strategy(&id, pane.graph(), pane.focused_key(), orrery_w, orrery_h);
            self.pane_orrery_mut(orrery_gid).apply_strategy_positions(&positions);
        }
        let (orrery_scene, orrery_redraw) = self.pane_orrery_mut(orrery_gid).frame(orrery_w, orrery_h);

        // Orrery-as-element (i-2): snapshot the focused orrery's nodes through its
        // camera into the shell state, so the orrery element renders a DOM card per
        // node. The update + frame() above ran this frame, so the snapshot reads
        // this-frame positions/colors/scope and the cards align with the scene. (Phase 2.)
        // The cursor (window px) for the per-card hover test below, copied out so the card
        // closure does not re-borrow self while `orrery` is held. (P0 hover.)
        let hover_cursor = self.view.cursor;
        let orrery_cards = {
            let orrery = self.pane_orrery(orrery_gid);
            let cam = orrery.camera();
            // The focused pane box, for culling cards to it: serval does not clip
            // transformed overflow, so an off-screen node would otherwise escape the
            // orrery element up into the chrome (the toolbar-escape we saw).
            let (pw, ph) = (orrery_rect[2] - orrery_rect[0], orrery_rect[3] - orrery_rect[1]);
            let cards = orrery
                .graph()
                .nodes()
                .filter_map(|(key, node)| {
                    // Settings nodes render as normal nodes addressing their page (Mark,
                    // 2026-06-22): a `settings://` node is a first-class, visible graph node you
                    // can see / open / relate, not an invisible tile-only member. Opening it
                    // routes to the settings page like any node. (Settings lane — visible nodes.)
                    let w = orrery.node_position(key)?;
                    let x = w.x * cam.zoom + cam.offset.0;
                    let y = w.y * cam.zoom + cam.offset.1;
                    // Off-pane nodes ride the underlay demote-dots, not a card.
                    if !(0.0..=pw).contains(&x) || !(0.0..=ph).contains(&y) {
                        return None;
                    }
                    // Label: a real page title once the node has loaded one; otherwise the
                    // URL's last path segment (the readable slug, e.g. a wiki article name),
                    // which previews the eventual title and keeps same-site nodes distinct.
                    // node.title is seeded to the URL until a load completes, so a URL-shaped
                    // title is the un-loaded case. (node_display_label would collapse these to
                    // the bare host.) Capped with an ellipsis for the compact on-canvas card.
                    const CARD_LABEL_CAP: usize = 24;
                    let raw = node.title.trim_end_matches('/');
                    let base = if raw.contains("://") {
                        match raw.rsplit('/').next() {
                            Some(slug) if !slug.is_empty() => slug,
                            _ => raw,
                        }
                    } else {
                        raw
                    };
                    let label = if base.chars().count() <= CARD_LABEL_CAP {
                        base.to_string()
                    } else {
                        base.chars().take(CARD_LABEL_CAP - 1).chain(['\u{2026}']).collect()
                    };
                    // The node's footprint (per-node override / size-by-degree / default),
                    // used both as the card's face size and as the hover hit-box half. (P0.)
                    let node_size = orrery.node_size(key);
                    let face_half = node_size / 2.0;
                    Some(OrreryCard {
                        member: node.id,
                        label,
                        x,
                        y,
                        // State color only; selection shows as a ring + lift on the card face.
                        color: orrery.node_state_color(key).to_string(),
                        selected: orrery.node_selected(key),
                        // Hover: the cursor over this node's face box (window px). (P0 hover.)
                        hovered: {
                            let (wx, wy) = (orrery_rect[0] + x, orrery_rect[1] + y);
                            (hover_cursor.0 - wx).abs() <= face_half
                                && (hover_cursor.1 - wy).abs() <= face_half
                        },
                        size: node_size,
                        // Content-type silhouette as the face's border-radius.
                        radius: match orrery.node_shape(key) {
                            orrery::NodeShape::Square => "0",
                            orrery::NodeShape::Rounded => "9px",
                            orrery::NodeShape::Circle => "50%",
                        },
                        favicon: node.favicon_rgba.as_ref().and_then(|rgba| {
                            favicon_data_uri(rgba, node.favicon_width, node.favicon_height)
                        }),
                        // The custom sprite image (a data-URI), for a `Sprite` face. (P2.)
                        sprite: orrery.node_sprite(key).map(str::to_string),
                        face: orrery.node_face(key),
                        // The body hull, so the card face is clipped to the collider shape. (B&F.)
                        hull: orrery
                            .node_sprite_hull(key)
                            .map(<[(f32, f32)]>::to_vec)
                            .unwrap_or_default(),
                    })
                })
                .collect();
            cards
        };
        // The focused node's content card (snapshot / unvisited placeholder), placed after
        // the node cards in document order so it paints over them. (Layering fix.)
        let focus_card = self.compute_focus_card(orrery_rect, workbench_rect);
        let orrery_render = OrreryRender { rect: orrery_rect, cards: orrery_cards, focus_card };
        // Only rebuild the shell view when the snapshot actually changed: a settled
        // orrery (no motion, selection, or camera change) produces an identical snapshot
        // each frame, so this skips the per-frame view re-run + diff entirely. (Perf.)
        if &orrery_render != self.view.orrery_render() {
            self.view.set_orrery(orrery_render);
        }
        // Fold the roster pane into the shell document: snapshot its rect + rows into the
        // shell state before the one render lays the document out, so the roster renders,
        // hit-tests, and projects a11y through the shell runner (its CSS rides the shell
        // stylesheet below). Replaces the separate RosterPane frame + composite. (Phase 1.)
        if roster_rect.is_some() {
            let rows = self.roster_rows();
            let field_rows = self.roster_field_rows();
            self.view.set_roster(rows, field_rows, roster_rect);
        } else if self.view.roster_open() {
            self.view.set_roster(Vec::new(), Vec::new(), None);
        }
        // Fold the four list panes (apparatus / steward / inspector / trail) into the same
        // shell document: snapshot each open pane's items + rect into its slot before the
        // render lays the document out. Replaces the separate ListPane frames + composites.
        // (Phase 1, step 2.)
        self.snapshot_list_panes(list_pane_rects);
        // The roster scrolls its own `.roster` container; add its offset to the shell
        // ScrollOffsets so the one render scrolls it (chrome_click mirrors this for the
        // hit-test). (Phase 1.)
        if roster_rect.is_some() {
            let dom = self.view.dom.borrow();
            if let Some(node) = first_with_class(&dom, dom.document(), "roster") {
                chrome_scroll.insert(node, (0.0, self.view.roster_scroll));
            }
        }
        // Each open list pane scrolls its own inner root (`.apparatus` / `.steward` /
        // `.inspector` / `.trail`, the unique-id class); add its offset to the shell
        // ScrollOffsets so the one render scrolls it (chrome_click mirrors this). (Phase 1.)
        {
            let dom = self.view.dom.borrow();
            let root = dom.document();
            for (class, scroll) in [
                ("apparatus", self.view.apparatus_scroll),
                ("steward", self.view.steward_scroll),
                ("inspector", self.view.inspector_scroll),
                ("trail", self.view.trail_scroll),
            ] {
                if let Some(node) = first_with_class(&dom, root, class) {
                    chrome_scroll.insert(node, (0.0, scroll));
                }
            }
        }
        let roster_css = crate::roster::roster_sheet(&self.shared.presentation.chrome_theme);
        // The folded list panes' CSS rides the shell stylesheet too; rules are inert when
        // no matching pane element is in the document, so they can be unconditional. The
        // apparatus root is `.apparatus`, the others `.utility-pane`. (Phase 1, step 2.)
        let apparatus_css = crate::apparatus::apparatus_sheet(&self.shared.presentation.chrome_theme);
        let utility_css = crate::utility_panes::utility_pane_sheet(&self.shared.presentation.chrome_theme);
        // The chrome (shell document) scene is built **after** the workbench block below,
        // not here: the folded settings panes are positioned at the workbench's tile rects,
        // which are only known once the tile surface has laid out. Rendering the shell after
        // `snapshot_settings_panes` lets a spine page-switch land this frame instead of one
        // frame late. `roster_css` / `apparatus_css` / `utility_css` are owned, so they stay
        // here and feed the deferred `chrome_sheet`. (Settings lane P1.)

        // The orrery's per-frame update (node state/shape, resize, recenter, mirror,
        // strategy) and the node-card snapshot now run *above* the chrome render, so the
        // cards read this-frame positions/colors and align with the scene (no one-frame
        // lag). See the "Orrery-as-element" block before the snapshot.
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
                orrery.set_render_as_cards(false); // secondary panes keep their gnodes
                if !orrery.graph_visible() {
                    orrery.recenter();
                }
                if let Some(id) = orrery.layout_strategy().map(str::to_string) {
                    let positions =
                        platen::project_orrery_strategy(&id, orrery.graph(), orrery.focused_key(), sw, sh);
                    orrery.apply_strategy_positions(&positions);
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
                self.shared.content.constellation.drive(member, &url, state, cw, ch, sheet);
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
        // An open settings tile scrolls its `.settings-pane-body`; add the offset to the
        // shell ScrollOffsets now that the pane is in the document, so the one render scrolls
        // it + serval draws the thumb (chrome_click mirrors this for the hit-test). Settings
        // panes are folded after the list panes, so this insertion is here, not above. (Menu
        // / pane scroll.)
        if self.view.settings_panes_open() {
            let dom = self.view.dom.borrow();
            if let Some(node) = first_with_class(&dom, dom.document(), "settings-pane-body") {
                chrome_scroll.insert(node, (0.0, self.view.settings_scroll));
            }
        }

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
            &chrome_scroll,
        );
        chrome_us = chrome_t.elapsed().as_micros();
        // The chrome document's `<external-texture>` elements + their laid-out rects, enumerated
        // now that the chrome session has been laid out, composited at the compositor pass below
        // so each external surface's placement comes from the document. (cond 5.)
        let external_texture_placements = self.external_texture_placements();
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
                        super::card::anchored_card_rect(
                            nx,
                            ny,
                            super::card::SNAP_W,
                            super::card::SNAP_H,
                            orrery_rect,
                        )
                    })
                    .or_else(|| super::card::card_rect(orrery_rect));
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
                        let (scene, content_height, _links) = super::card::render_content_scene(
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
                        super::card::anchored_card_rect(
                            nx,
                            ny,
                            super::card::UNVIS_W,
                            super::card::UNVIS_H,
                            orrery_rect,
                        )
                    })
                    .map(|(x0, y0, x1, y1, _, _)| (member, [x0, y0, x1, y1]));
            }
        }

        // Reap every compat WebView whose member isn't a surface shown this frame:
        // a tile that was closed / unpinned, or a card that lost focus, is torn down
        // here (reap-on-deselect) so its visual can't freeze on screen. The shared
        // composition target persists, so the surviving panes are untouched. (X3
        // lifecycle; multi-tile.)
        let shown: std::collections::HashSet<GraphMemberId> =
            scrying_surfaces.iter().map(|(m, _)| *m).collect();
        self.view.scrying.retain(&shown);

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
        if let Some((member, _, rect, built)) = &snapshot_card {
            // Claim the card's hit-rect only once its snapshot is cached + shown (`built` is
            // `None` = already cached). While it is still building there is no visible card,
            // so its rect must not intercept clicks on the node beneath it — that phantom
            // rect was making the first double-click-to-open flaky. (Snapshot — no phantom rect.)
            if built.is_none() {
                self.view.content_rects.push((*member, *rect));
            }
        }
        for (member, rect) in &scrying_surfaces {
            self.view.content_rects.push((*member, *rect));
        }

        // The omnibar follows focus: point it at the focused tile / node when that
        // changed (next frame, like the chrome strips were — the scene above is
        // already built).
        self.sync_location();
        // Back/forward enabled-state tracks the focused node's own history.
        self.sync_nav_buttons();
        self.drain_portable_diagnostics();

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
        for (member, dest, (cw, ch)) in &cards {
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
                            core.renderer().build_box_shadow_mask(
                                m.key,
                                m.dim,
                                m.bounds,
                                m.corner_radius,
                                m.blur_radius_px,
                                m.invert,
                            );
                        }
                        let (tex, view) =
                            core.rasterize(scene, *cw, band_px, ColorLoad::Clear(card_bg));
                        self.view.tile_textures.insert(
                            *member,
                            super::CachedTile { version, size: (*cw, band_px), tex, view },
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
                    let doc_scene = self
                        .shared
                        .content
                        .constellation
                        .packet(*member)
                        .map(|(packet, fonts)| {
                            crate::card::lower_window(packet, fonts, new_band_y, band_h, doc_palette)
                        });
                    if let Some(scene) = doc_scene {
                        let (tex, view) =
                            core.rasterize(&scene, *cw, band_px, ColorLoad::Clear(card_bg));
                        self.view.tile_textures.insert(
                            *member,
                            super::CachedTile { version, size: (*cw, band_px), tex, view },
                        );
                        self.view.tile_bands.insert(*member, new_band_y);
                    }
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
                            let wy0 = dest[1] + (r[1] - scroll) * s;
                            let wy1 = dest[1] + (r[3] - scroll) * s;
                            // Cull a match scrolled out of the card's visible band.
                            if wy1 <= dest[1] || wy0 >= dest[3] {
                                continue;
                            }
                            let wx0 = (dest[0] + r[0] * s).max(dest[0]);
                            let wx1 = (dest[0] + r[2] * s).min(dest[2]);
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
                    super::card::recovering_card_scene(rw, rh, self.shared.presentation.document_palette);
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

            // Minimap swatch.
            let (nodes, edges) = self.orrery().minimap_geometry();
            let mw = (minimap_rect[2] - minimap_rect[0]).round().max(1.0) as u32;
            let mh = (minimap_rect[3] - minimap_rect[1]).round().max(1.0) as u32;
            let (scene, local) = super::gloss::minimap_scene(
                &nodes,
                &edges,
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
            let (rscene, rlocal) = super::gloss::recent_scene(
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
        // The add-tag prompt: a centered text-entry box over the content while the
        // host captures a tag for the selected node(s). Drawn last so it sits over
        // the orrery + panes. (Add-tag.)
        if let Some(buf) = self.view.tagging.clone() {
            let pw: u32 = 360;
            let ph: u32 = 40;
            let scene = super::tags::tag_prompt_scene(
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
