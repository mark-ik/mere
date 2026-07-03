/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Card geometry: find, snapshot/object-card sizing, card rects, recovery scene.

use super::*;

/// Find every occurrence of `query` in the focused node's HTML content, returning the
/// highlight rects per match in full-document px (`[x0, y0, x1, y1]`). Only the
/// HTML/serval lane is searched here: it ships a flat scene the host cannot query, so
/// the actor runs the search where the layout lives. Document-lane content returns no
/// matches (its find rides the retained packet, a separate path). An empty query, or
/// non-Ready / non-HTML content, yields nothing. (Find-in-page.)
pub fn find_content(
    url: &str,
    state: Option<&ContentState>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
    query: &str,
) -> Vec<Vec<[f32; 4]>> {
    let Some(ContentState::Ready(fetched)) = state else {
        return Vec::new();
    };
    if route_document_engine(url, fetched.content_type.as_deref(), registry, policy)
        != inker::routing::ENGINE_SERVAL_WEB
    {
        return Vec::new();
    }
    let doc = StaticDocument::parse(&fetched.body);
    let inline = inline_stylesheets(&doc);
    let linked = linked_stylesheets_with_loader(&doc, loader);
    let mut sheets: Vec<&str> = HTML_SHEET.to_vec();
    sheets.extend(inline.iter().map(String::as_str));
    sheets.extend(linked.iter().map(String::as_str));
    serval_layout::find_text_rects_from_layout_dom(&doc, &sheets, loader, w, h, query)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentSelection {
    pub rects: Vec<[f32; 4]>,
    pub text: String,
}

#[derive(Clone, Copy)]
struct DocumentTextEntry<'a> {
    source_index: usize,
    top_index: usize,
    block: &'a Block,
}

/// Find every occurrence of `query` in a retained document-lane page, returning one
/// rect-set per match in full-document px (`[x0, y0, x1, y1]`). Without cluster
/// offsets this is block-scoped: multiple hits in one rendered block share that
/// block's rects, but the match count and stepping stay truthful. (Retained-text P3.)
pub(crate) fn find_document_content(
    doc: &EngineDocument,
    packet: &DocumentRenderPacket,
    query: &str,
) -> Vec<Vec<[f32; 4]>> {
    let needle = query.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let needle = needle.to_lowercase();
    let mut matches = Vec::new();
    for entry in document_text_entries(doc) {
        let text = block_plain_text(entry.block);
        if text.is_empty() {
            continue;
        }
        let rects = rendered_rects_for_source(packet, entry.source_index);
        if rects.is_empty() {
            continue;
        }
        let hay = text.to_lowercase();
        let mut from = 0usize;
        while let Some(found) = hay[from..].find(&needle) {
            matches.push(rects.clone());
            from += found + needle.len();
        }
    }
    matches
}

/// Select the inclusive document-block range between `anchor_source` and
/// `focus_source`, using the layout's synthetic child-index scheme. The returned text
/// is flattened through `EngineDocument::to_text`, so copy matches the document's
/// plain-text exporter rather than whatever glyphs happened to be painted. (P4.)
pub(crate) fn select_document_content(
    doc: &EngineDocument,
    packet: &DocumentRenderPacket,
    anchor_source: usize,
    focus_source: usize,
) -> Option<DocumentSelection> {
    let entries = document_text_entries(doc);
    if entries.is_empty() {
        return None;
    }
    let anchor = selection_span(&entries, anchor_source)?;
    let focus = selection_span(&entries, focus_source)?;
    let (start, end) = if anchor.0 <= focus.1 {
        (anchor.0.min(focus.0), anchor.1.max(focus.1))
    } else {
        (focus.0.min(anchor.0), focus.1.max(anchor.1))
    };
    let chosen = &entries[start..=end];
    let mut rects = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut blocks = Vec::new();
    for entry in chosen {
        if seen.insert(entry.source_index) {
            rects.extend(rendered_rects_for_source(packet, entry.source_index));
        }
        blocks.push(entry.block.clone());
    }
    if rects.is_empty() {
        return None;
    }
    let text = EngineDocument {
        address: String::new(),
        title: None,
        content_type: "text/plain".to_string(),
        lang: None,
        provenance: Default::default(),
        trust: Default::default(),
        diagnostics: Vec::new(),
        blocks,
    }
    .to_text();
    Some(DocumentSelection { rects, text })
}

fn document_text_entries(doc: &EngineDocument) -> Vec<DocumentTextEntry<'_>> {
    let mut out = Vec::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        collect_document_text_entries(block, i, i, &mut out);
    }
    out
}

fn collect_document_text_entries<'a>(
    block: &'a Block,
    source_index: usize,
    top_index: usize,
    out: &mut Vec<DocumentTextEntry<'a>>,
) {
    match block {
        Block::Quote { blocks } => {
            for (i, child) in blocks.iter().enumerate() {
                collect_document_text_entries(
                    child,
                    source_index.saturating_mul(1000) + i,
                    top_index,
                    out,
                );
            }
        }
        Block::List { items, .. } => {
            for (i, item) in items.iter().enumerate() {
                for (j, child) in item.iter().enumerate() {
                    collect_document_text_entries(
                        child,
                        source_index.saturating_mul(1000) + i.saturating_mul(100) + j,
                        top_index,
                        out,
                    );
                }
            }
        }
        _ => out.push(DocumentTextEntry {
            source_index,
            top_index,
            block,
        }),
    }
}

fn selection_span(
    entries: &[DocumentTextEntry<'_>],
    source_index: usize,
) -> Option<(usize, usize)> {
    if let Some((i, _)) = entries
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.source_index == source_index)
    {
        return Some((i, i));
    }
    let top = source_index;
    let mut first = None;
    let mut last = None;
    for (i, entry) in entries.iter().enumerate() {
        if entry.top_index == top {
            first.get_or_insert(i);
            last = Some(i);
        }
    }
    Some((first?, last?))
}

fn block_plain_text(block: &Block) -> String {
    EngineDocument {
        address: String::new(),
        title: None,
        content_type: "text/plain".to_string(),
        lang: None,
        provenance: Default::default(),
        trust: Default::default(),
        diagnostics: Vec::new(),
        blocks: vec![block.clone()],
    }
    .to_text()
    .trim()
    .to_string()
}

fn rendered_rects_for_source(packet: &DocumentRenderPacket, source_index: usize) -> Vec<[f32; 4]> {
    let mut rects = Vec::new();
    collect_rendered_rects(&packet.blocks, source_index, &mut rects);
    rects
}

fn collect_rendered_rects(
    blocks: &[document_canvas::RenderedBlock],
    source_index: usize,
    out: &mut Vec<[f32; 4]>,
) {
    for block in blocks {
        let before = out.len();
        let child_matches =
            if let document_canvas::RenderedBlockKind::Group { children } = &block.kind {
                collect_rendered_rects(children, source_index, out);
                out.len() > before
            } else {
                false
            };
        if block.source_block_index == source_index && !child_matches {
            out.push([
                block.bounds.origin.x,
                block.bounds.origin.y,
                block.bounds.max_x(),
                block.bounds.max_y(),
            ]);
        }
    }
}

/// The floating card rectangle within the content band (top-right, inset by
/// [`CARD_MARGIN`]). Returns `(x0, y0, x1, y1, w, h)` — window-space corners for
/// the composite plus the pixel size to rasterize at — or `None` when the band
/// is too small to host a readable card.
/// The focused-node snapshot card footprint (px): a small fixed thumbnail anchored beside
/// the node. Shared by the shell element placement (`compute_focus_card`) and the host
/// texture build, so the element and its content agree on size. (Layering fix.)
pub(crate) const SNAP_W: u32 = 200;
pub(crate) const SNAP_H: u32 = 260;
/// The never-visited placeholder card footprint (px).
pub(crate) const UNVIS_W: u32 = 200;
pub(crate) const UNVIS_H: u32 = 120;
/// The object card width (px) — a narrow control card. Its height grows with the widget
/// count via [`object_card_height`]. (Object card — P1.)
pub(crate) const OBJCARD_W: u32 = 200;
/// The object card height (px) for `n` widget rows: the container padding plus a labeled
/// control row each. (Object card — P1.)
pub(crate) fn object_card_height(n: usize) -> u32 {
    22 + n as u32 * 52
}

pub fn card_rect(band: [f32; 4]) -> Option<(f32, f32, f32, f32, u32, u32)> {
    let [bx0, by0, bx1, by1] = band;
    let bw = bx1 - bx0;
    let top = by0 + CARD_MARGIN;
    let avail_w = bw - 2.0 * CARD_MARGIN;
    let avail_h = by1 - top - CARD_MARGIN;
    if avail_w < 160.0 || avail_h < 100.0 {
        return None;
    }
    let cw = (bw * 0.42).clamp(280.0, 460.0).min(avail_w);
    let ch = avail_h.min(560.0);
    let x1 = bx1 - CARD_MARGIN;
    let x0 = x1 - cw;
    let y0 = top;
    let y1 = y0 + ch;
    Some((
        x0,
        y0,
        x1,
        y1,
        cw.round().max(1.0) as u32,
        ch.round().max(1.0) as u32,
    ))
}

/// A floating card of desired pixel size `cw`x`ch`, anchored **next to** the node
/// at window point `(nx, ny)`. Placed just right of the node, flipped to the left
/// when it would overflow; vertically centered on the node and clamped to the
/// content band. Returns window-space corners + the (clamped) size, or `None`
/// when the band is too small. The card follows the node because the caller
/// recomputes it each frame from the live node position. (Card system: anchored.)
pub fn anchored_card_rect(
    nx: f32,
    ny: f32,
    cw: u32,
    ch: u32,
    band: [f32; 4],
) -> Option<(f32, f32, f32, f32, u32, u32)> {
    let [bx0, by0, bx1, by1] = band;
    let margin = 8.0;
    let top = by0 + margin;
    let avail_w = (bx1 - bx0) - 2.0 * margin;
    let avail_h = by1 - top - margin;
    if avail_w < 120.0 || avail_h < 80.0 {
        return None;
    }
    let cwf = (cw as f32).min(avail_w);
    let chf = (ch as f32).min(avail_h);
    // Anchor the card to the node's keep-out box (a `gap`-wide band straddling the node point),
    // placed `RightOf` and flipped `LeftOf` + clamped into the band's horizontal margins by the
    // shared `anchor_point_clamped` — the same side+flip+clamp the submenu drives, instead of a
    // hand-rolled copy. Only the x is taken; the y is the card-specific node-centred clamp below.
    use xilem_serval::{Placement, anchor_point_clamped};
    let gap = 28.0;
    let keepout = (nx - gap, ny, 2.0 * gap, 0.0);
    let (x0, _) = anchor_point_clamped(
        keepout,
        (cwf, chf),
        Placement::RightOf,
        (bx0 + margin, top, bx1 - margin, by1 - margin),
    );
    // Card-specific: vertically center the card on the node, clamped into the band.
    let y0 = (ny - chf * 0.5).clamp(top, (by1 - margin - chf).max(top));
    Some((
        x0,
        y0,
        x0 + cwf,
        y0 + chf,
        cwf.round().max(1.0) as u32,
        chf.round().max(1.0) as u32,
    ))
}

/// The placeholder for a tile whose actor is recovering from a crash (respawned,
/// not yet re-rendered): a centered "Reloading…" hint, shown until the respawned
/// actor delivers a scene whose texture covers it. The same doc → layout → scene
/// path as the cards, but carries no dashed border — it is a
/// transient status, not an affordance. (Workbench tile decoration, re-applied on the
/// pelt surface path.)
pub fn recovering_card_scene(w: u32, h: u32, colors: ColorVocabulary) -> Scene {
    let doc = document("mere://recovering", vec![paragraph("Reloading\u{2026}")]);
    let mut laid = layout_document(&doc, Viewport::new(w as f32, h as f32), &card_sheet(colors));
    laid.packet.viewport = Viewport::new(w as f32, h as f32);
    scene_from_packet(&laid.packet, &laid.fonts, &colors)
}
