// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host-neutral web-clip capture and projection helpers.

use forme::GraphMemberId;
use image::ImageEncoder;
use inker::{
    Block, DocumentProvenance, DocumentTrustState, EngineDocument, EngineInput, EngineRegistry,
    EngineRoutePolicy, EngineRouteRequest, InlineSpan, WorkspaceRouteId,
};
use kernel::graph::{EdgeAssertion, Graph, NodeKey, ProvenanceSubKind};
use kernel::types::{ImageRef, ImageRole};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct ClipFragment {
    pub source_url: String,
    pub title: Option<String>,
    pub text: String,
    pub html: Option<String>,
    pub selector: Option<String>,
    pub links: Vec<String>,
    pub rect: Option<ClipRect>,
    pub visual: Option<ClipVisual>,
    blocks: Option<Vec<Block>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipVisual {
    pub png_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub data_uri: String,
}

#[derive(Deserialize)]
struct WebClipPayload {
    ok: Option<bool>,
    error: Option<String>,
    title: Option<String>,
    #[serde(rename = "pageTitle")]
    page_title: Option<String>,
    text: Option<String>,
    html: Option<String>,
    selector: Option<String>,
    #[serde(default)]
    links: Vec<String>,
    rect: Option<ClipRect>,
}

pub fn web_clip_script(x: i32, y: i32) -> String {
    format!(
        r##"(function() {{
  const rawX = {x};
  const rawY = {y};
  const scale = window.devicePixelRatio || 1;
  const x = rawX / scale;
  const y = rawY / scale;
  function cssPath(el) {{
    const parts = [];
    while (el && el.nodeType === 1 && el !== document.documentElement) {{
      let part = el.localName;
      if (el.id) {{
        part += "#" + CSS.escape(el.id);
        parts.unshift(part);
        break;
      }}
      const cls = Array.from(el.classList || []).slice(0, 2).map(c => "." + CSS.escape(c)).join("");
      part += cls;
      const parent = el.parentElement;
      if (parent) {{
        const same = Array.from(parent.children).filter(n => n.localName === el.localName);
        if (same.length > 1) {{
          part += ":nth-of-type(" + (same.indexOf(el) + 1) + ")";
        }}
      }}
      parts.unshift(part);
      el = parent;
    }}
    return parts.join(" > ");
  }}
  let el = document.elementFromPoint(x, y);
  if (!el) return JSON.stringify({{ ok: false, error: "no element at cursor" }});
  if (el.nodeType !== 1) el = el.parentElement;
  const clone = el.cloneNode(true);
  clone.querySelectorAll("script,style,noscript,iframe,object,embed").forEach(n => n.remove());
  const rect = el.getBoundingClientRect();
  const heading = el.querySelector("h1,h2,h3,[role='heading']");
  const title = ((heading && heading.textContent) ||
    el.getAttribute("aria-label") ||
    el.getAttribute("alt") ||
    document.title ||
    location.href ||
    "").trim();
  const links = Array.from(el.querySelectorAll("a[href]")).slice(0, 50).map(a => a.href);
  return JSON.stringify({{
    ok: true,
    title,
    pageTitle: document.title || "",
    text: (el.innerText || el.textContent || "").trim(),
    html: clone.outerHTML,
    selector: cssPath(el),
    links,
    rect: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }}
  }});
}})()"##
    )
}

pub fn parse_web_clip(raw: &str, source_url: &str) -> Result<ClipFragment, String> {
    let payload = parse_payload(raw)?;
    if payload.ok == Some(false) {
        return Err(payload
            .error
            .unwrap_or_else(|| "selected element could not be captured".to_string()));
    }
    let title = first_nonempty([payload.title, payload.page_title]);
    let mut text = payload
        .text
        .as_deref()
        .map(clean_multiline)
        .unwrap_or_default();
    if text.is_empty() {
        text = title.clone().unwrap_or_default();
    }
    if text.is_empty() {
        return Err("selected element had no text".to_string());
    }
    Ok(ClipFragment {
        source_url: source_url.to_string(),
        title,
        text,
        html: payload.html.filter(|html| !html.trim().is_empty()),
        selector: payload
            .selector
            .filter(|selector| !selector.trim().is_empty()),
        links: payload
            .links
            .into_iter()
            .filter(|link| !link.trim().is_empty())
            .collect(),
        rect: payload.rect,
        visual: None,
        blocks: None,
    })
}

pub fn fragment_from_body(
    source_url: &str,
    title: Option<String>,
    content_type: Option<&str>,
    body: &str,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
) -> ClipFragment {
    if let Some(doc) = engine_document(source_url, content_type, body, registry, policy)
        && !doc.blocks.is_empty()
    {
        let text = clean_multiline(&blocks_text(&doc.blocks));
        if !text.is_empty() {
            let links = block_links(&doc.blocks);
            return ClipFragment {
                source_url: source_url.to_string(),
                title: first_nonempty([doc.title.clone(), title]),
                text,
                html: None,
                selector: None,
                links,
                rect: None,
                visual: None,
                blocks: Some(doc.blocks),
            };
        }
    }

    let text = clean_multiline(&document_text(content_type, body));
    ClipFragment {
        source_url: source_url.to_string(),
        title: title.filter(|title| !title.trim().is_empty()),
        text,
        html: None,
        selector: None,
        links: Vec::new(),
        rect: None,
        visual: None,
        blocks: None,
    }
}

/// Build a host-neutral semantic clip when a retained document lane exposes
/// selected or document text rather than raw response bytes.
pub fn fragment_from_text(
    source_url: impl Into<String>,
    title: Option<String>,
    text: impl Into<String>,
    selector: Option<String>,
    links: Vec<String>,
) -> ClipFragment {
    ClipFragment {
        source_url: source_url.into(),
        title: title.filter(|title| !title.trim().is_empty()),
        text: clean_multiline(&text.into()),
        html: None,
        selector: selector.filter(|selector| !selector.trim().is_empty()),
        links: links
            .into_iter()
            .filter(|link| !link.trim().is_empty())
            .collect(),
        rect: None,
        visual: None,
        blocks: None,
    }
}

pub fn attach_cropped_visual(
    fragment: &mut ClipFragment,
    snapshot_png: &[u8],
    surface_size: (u32, u32),
) -> bool {
    let Some(rect) = fragment.rect.as_ref() else {
        return false;
    };
    if rect.width <= 1.0 || rect.height <= 1.0 || snapshot_png.is_empty() {
        return false;
    }
    let Ok(image) = image::load_from_memory(snapshot_png) else {
        return false;
    };
    let rgba = image.to_rgba8();
    let (image_w, image_h) = rgba.dimensions();
    if image_w == 0 || image_h == 0 {
        return false;
    }
    let scale_x = image_w as f32 / surface_size.0.max(1) as f32;
    let scale_y = image_h as f32 / surface_size.1.max(1) as f32;
    let x = scaled_start(rect.x, scale_x, image_w);
    let y = scaled_start(rect.y, scale_y, image_h);
    let width = scaled_len(rect.width, scale_x, image_w - x);
    let height = scaled_len(rect.height, scale_y, image_h - y);
    if width == 0 || height == 0 {
        return false;
    }

    const CLIP_VISUAL_MAX: u32 = 1024;
    let cropped = image::imageops::crop_imm(&rgba, x, y, width, height).to_image();
    let cropped = if width > CLIP_VISUAL_MAX || height > CLIP_VISUAL_MAX {
        image::DynamicImage::ImageRgba8(cropped)
            .thumbnail(CLIP_VISUAL_MAX, CLIP_VISUAL_MAX)
            .to_rgba8()
    } else {
        cropped
    };
    let (width, height) = cropped.dimensions();
    let Some(png_bytes) = encode_png_rgba(cropped.as_raw(), width, height) else {
        return false;
    };
    let data_uri = format!("data:image/png;base64,{}", encode_base64(&png_bytes));
    fragment.visual = Some(ClipVisual {
        png_bytes,
        width,
        height,
        data_uri,
    });
    true
}

pub fn clip_title(fragment: &ClipFragment) -> String {
    let base = fragment
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&fragment.source_url);
    let mut title = format!("Clip: {base}");
    if title.chars().count() > 96 {
        title = title.chars().take(93).collect::<String>();
        title.push_str("...");
    }
    title
}

pub fn write_clip_node(
    graph: &mut Graph,
    source_key: NodeKey,
    clip_url: &str,
    fragment: &ClipFragment,
    visual: Option<ImageRef>,
) -> Option<GraphMemberId> {
    if graph.get_node(source_key).is_none() {
        return None;
    }
    use kernel::graph::apply::{self as graph_apply, GraphDelta, apply_graph_delta};

    let clip_key = graph_apply::add_node(graph, None, clip_url.to_string(), Default::default());
    let _ = apply_graph_delta(
        graph,
        GraphDelta::SetNodeTitle {
            key: clip_key,
            title: clip_title(fragment),
        },
    );
    let _ = apply_graph_delta(
        graph,
        GraphDelta::SetNodeMimeHint {
            key: clip_key,
            mime_hint: Some("text/x-knot".to_string()),
        },
    );
    let _ = apply_graph_delta(
        graph,
        GraphDelta::SetNodeBody {
            key: clip_key,
            body: Some(fragment_to_knot(fragment)),
        },
    );
    let member = graph.get_node(clip_key).map(|n| n.id);
    // The caller stores `fragment.visual`'s bytes (it holds the image store,
    // and saving is async) and hands back the reference; the graph carries the
    // handle only.
    if let Some(visual) = visual {
        let _ = apply_graph_delta(
            graph,
            GraphDelta::SetNodeImage {
                key: clip_key,
                role: ImageRole::Preview,
                image: visual,
            },
        );
    }
    let _ = graph_apply::assert_relation(
        graph,
        clip_key,
        source_key,
        EdgeAssertion::Provenance {
            sub_kind: ProvenanceSubKind::ClippedFrom,
        },
    );
    member
}

fn parse_payload(raw: &str) -> Result<WebClipPayload, String> {
    let trimmed = raw.trim();
    if let Ok(decoded) = serde_json::from_str::<String>(trimmed) {
        serde_json::from_str(&decoded).map_err(|err| err.to_string())
    } else {
        serde_json::from_str(trimmed).map_err(|err| err.to_string())
    }
}

fn engine_document(
    source_url: &str,
    content_type: Option<&str>,
    body: &str,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
) -> Option<EngineDocument> {
    let request = EngineRouteRequest {
        workspace_id: WorkspaceRouteId::new("meerkat"),
        view: None,
        node: None,
        address: source_url.to_string(),
        content_type: content_type.map(str::to_string),
        pinned_engine: None,
    };
    let decision = policy.route_filtered(&request, |id| {
        registry.contains(id) || id == inker::routing::ENGINE_GENET_WEB
    });
    if decision.engine_id == inker::routing::ENGINE_GENET_WEB {
        return None;
    }
    let mut input = EngineInput::new(source_url, body.to_string());
    if let Some(content_type) = content_type {
        input = input.with_content_type(content_type.to_string());
    }
    registry.dispatch(&decision, &input).ok()
}

fn document_text(content_type: Option<&str>, body: &str) -> String {
    if content_type
        .map(|ct| ct.to_ascii_lowercase().contains("html"))
        .unwrap_or(false)
    {
        let doc = genet_static_dom::StaticDocument::parse(body);
        if let Some(text) = genet_extract::extract_main_text(&doc) {
            return text;
        }
    }
    body.to_string()
}

fn scaled_start(value: f32, scale: f32, limit: u32) -> u32 {
    if limit == 0 {
        return 0;
    }
    (value.max(0.0) * scale)
        .floor()
        .clamp(0.0, (limit - 1) as f32) as u32
}

fn scaled_len(value: f32, scale: f32, limit: u32) -> u32 {
    if limit == 0 {
        return 0;
    }
    (value.max(1.0) * scale).ceil().clamp(1.0, limit as f32) as u32
}

fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(png)
}

fn encode_base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut chunks = data.chunks_exact(3);
    for chunk in &mut chunks {
        let bits = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        out.push(ALPHABET[((bits >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((bits >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((bits >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(bits & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let bits = match rem {
            [a] => (*a as u32) << 16,
            [a, b] => ((*a as u32) << 16) | ((*b as u32) << 8),
            _ => unreachable!(),
        };
        out.push(ALPHABET[((bits >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((bits >> 12) & 0x3f) as usize] as char);
        if rem.len() == 2 {
            out.push(ALPHABET[((bits >> 6) & 0x3f) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

fn fragment_to_knot(fragment: &ClipFragment) -> String {
    let mut source = DocumentProvenance::default();
    source.canonical_uri = Some(fragment.source_url.clone());
    source.source_kind = Some("meerkat.web_clip".to_string());
    source.source_label = fragment.title.clone();
    nematic::knot::build_clip_knot(
        &fragment_blocks(fragment),
        &source,
        DocumentTrustState::Tofu,
        Some("clip"),
    )
}

/// Render only the semantic Knot blocks of a clip.
///
/// The receiving Knot endpoint owns document provenance and records it from
/// the typed clip intent. Omitting frontmatter here prevents a second
/// document header from being inserted into an existing note.
pub fn fragment_to_knot_body(fragment: &ClipFragment) -> String {
    let mut body = String::new();
    EngineDocument {
        address: fragment.source_url.clone(),
        title: None,
        content_type: "text/x-knot".into(),
        lang: None,
        provenance: DocumentProvenance::default(),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks: fragment_blocks(fragment),
    }
    .write_knot_body(&mut body);
    body
}

fn fragment_blocks(fragment: &ClipFragment) -> Vec<Block> {
    if let Some(source_blocks) = &fragment.blocks {
        let mut blocks = Vec::new();
        if let Some(visual) = &fragment.visual {
            blocks.push(visual_block(fragment, visual));
        }
        blocks.extend(source_blocks.clone());
        return blocks;
    }

    let mut blocks = Vec::new();
    if let Some(title) = fragment
        .title
        .as_ref()
        .filter(|title| !title.trim().is_empty())
    {
        blocks.push(Block::Heading {
            level: 1,
            spans: vec![InlineSpan::Text(title.clone())],
        });
    }
    if let Some(visual) = &fragment.visual {
        blocks.push(visual_block(fragment, visual));
    }
    blocks.push(Block::Paragraph {
        spans: vec![InlineSpan::Text(fragment.text.clone())],
    });
    if !fragment.links.is_empty() {
        blocks.push(Block::List {
            ordered: false,
            items: fragment
                .links
                .iter()
                .take(20)
                .map(|url| {
                    vec![Block::Paragraph {
                        spans: vec![InlineSpan::Link {
                            url: url.clone(),
                            title: None,
                            spans: vec![InlineSpan::Text(url.clone())],
                            predicate: None,
                        }],
                    }]
                })
                .collect(),
        });
    }
    blocks
}

fn visual_block(fragment: &ClipFragment, visual: &ClipVisual) -> Block {
    Block::Image {
        url: visual.data_uri.clone(),
        alt: fragment
            .title
            .clone()
            .unwrap_or_else(|| "Clipped page fragment".to_string()),
    }
}

fn blocks_text(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        append_block_text(block, &mut out);
    }
    out
}

fn append_block_text(block: &Block, out: &mut String) {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph { spans } => {
            push_text(out, &inker::inline_text(spans));
        }
        Block::CodeBlock { text, .. } | Block::Preformatted { text } => push_text(out, text),
        Block::Quote { blocks } => {
            for block in blocks {
                append_block_text(block, out);
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for block in item {
                    append_block_text(block, out);
                }
            }
        }
        Block::Image { alt, .. } => push_text(out, alt),
        Block::Rule => {}
        Block::FeedHeader {
            title,
            subtitle,
            summary,
            ..
        } => {
            push_text(out, title);
            if let Some(subtitle) = subtitle {
                push_text(out, subtitle);
            }
            if let Some(summary) = summary {
                push_text(out, summary);
            }
        }
        Block::FeedEntry {
            title,
            date,
            summary,
            ..
        } => {
            push_text(out, title);
            if let Some(date) = date {
                push_text(out, date);
            }
            if let Some(summary) = summary {
                push_text(out, summary);
            }
        }
        Block::MetadataRow { label, value } => push_text(out, &format!("{label}: {value}")),
        Block::Badge { text } => push_text(out, text),
        Block::Table { header, rows, .. } => {
            for cell in header {
                push_text(out, &inker::inline_text(cell));
            }
            for row in rows {
                for cell in row {
                    push_text(out, &inker::inline_text(cell));
                }
            }
        }
    }
}

fn push_text(out: &mut String, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(text);
}

fn block_links(blocks: &[Block]) -> Vec<String> {
    let mut links = Vec::new();
    for block in blocks {
        collect_block_links(block, &mut links);
    }
    links.sort();
    links.dedup();
    links
}

fn collect_block_links(block: &Block, links: &mut Vec<String>) {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph { spans } => {
            collect_span_links(spans, links);
        }
        Block::Quote { blocks } => {
            for block in blocks {
                collect_block_links(block, links);
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for block in item {
                    collect_block_links(block, links);
                }
            }
        }
        Block::Image { url, .. } => links.push(url.clone()),
        Block::FeedHeader { source_url, .. } => push_optional_link(source_url, links),
        Block::FeedEntry {
            article_url,
            source_url,
            ..
        } => {
            push_optional_link(article_url, links);
            push_optional_link(source_url, links);
        }
        Block::Table { header, rows, .. } => {
            for cell in header {
                collect_span_links(cell, links);
            }
            for row in rows {
                for cell in row {
                    collect_span_links(cell, links);
                }
            }
        }
        Block::CodeBlock { .. }
        | Block::Preformatted { .. }
        | Block::Rule
        | Block::MetadataRow { .. }
        | Block::Badge { .. } => {}
    }
}

fn collect_span_links(spans: &[InlineSpan], links: &mut Vec<String>) {
    for span in spans {
        match span {
            InlineSpan::Link {
                url, spans: inner, ..
            } => {
                links.push(url.clone());
                collect_span_links(inner, links);
            }
            InlineSpan::Emphasis(inner)
            | InlineSpan::Strong(inner)
            | InlineSpan::Submit { spans: inner, .. } => {
                collect_span_links(inner, links);
            }
            InlineSpan::Text(_)
            | InlineSpan::Code(_)
            | InlineSpan::LineBreak
            | InlineSpan::SoftBreak => {}
        }
    }
}

fn push_optional_link(link: &Option<String>, links: &mut Vec<String>) {
    if let Some(link) = link.as_ref().filter(|link| !link.trim().is_empty()) {
        links.push(link.clone());
    }
}

fn clean_multiline(input: &str) -> String {
    let mut out = String::new();
    let mut previous_blank = true;
    for line in input.lines().map(str::trim) {
        if line.is_empty() {
            if !previous_blank {
                out.push('\n');
                previous_blank = true;
            }
            continue;
        }
        if !out.is_empty() && !previous_blank {
            out.push('\n');
        }
        out.push_str(line);
        previous_blank = false;
    }
    out.trim().to_string()
}

fn first_nonempty(values: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::RelationKind;
    use kernel::graph::fixtures::GraphFixtures;

    #[test]
    fn parses_webview2_quoted_json_result() {
        let payload = serde_json::json!({
            "ok": true,
            "title": "Chosen section",
            "text": "  First line\n\nSecond line  ",
            "selector": "main > article",
            "links": ["https://example.test/a"]
        })
        .to_string();
        let raw = serde_json::to_string(&payload).expect("quoted result");
        let clip = parse_web_clip(&raw, "https://example.test").expect("clip");
        assert_eq!(clip.title.as_deref(), Some("Chosen section"));
        assert_eq!(clip.text, "First line\nSecond line");
        assert_eq!(clip.links, vec!["https://example.test/a"]);
    }

    #[test]
    fn web_clip_script_uses_cursor_coordinates() {
        let script = web_clip_script(12, 34);
        assert!(script.contains("const rawX = 12;"));
        assert!(script.contains("const rawY = 34;"));
        assert!(script.contains("document.elementFromPoint"));
    }

    #[test]
    fn fragment_to_knot_records_clip_provenance() {
        let clip = ClipFragment {
            source_url: "https://example.test/post".to_string(),
            title: Some("Interesting bit".to_string()),
            text: "A useful paragraph.".to_string(),
            html: None,
            selector: None,
            links: vec!["https://example.test/ref".to_string()],
            rect: None,
            visual: None,
            blocks: None,
        };
        let knot = fragment_to_knot(&clip);
        assert!(knot.contains("source: https://example.test/post"));
        assert!(knot.contains("note_kind: clip"));
        assert!(knot.contains("A useful paragraph."));
        assert!(knot.contains("https://example.test/ref"));
    }

    #[test]
    fn semantic_clip_body_omits_document_frontmatter() {
        let clip = fragment_from_text(
            "https://example.test/post",
            Some("Interesting bit".into()),
            "A useful paragraph.",
            Some("main > article".into()),
            vec!["https://example.test/ref".into()],
        );
        let body = fragment_to_knot_body(&clip);
        assert!(body.contains("# Interesting bit"));
        assert!(body.contains("A useful paragraph."));
        assert!(body.contains("https://example.test/ref"));
        assert!(!body.starts_with("---\n"));
        assert!(!body.contains("source: https://example.test/post"));
    }

    #[test]
    fn markdown_fallback_preserves_blocks_and_links() {
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            registry.register(engine);
        }
        let clip = fragment_from_body(
            "https://example.test/note.md",
            None,
            Some("text/markdown"),
            "# Heading\n\nA [reference](https://example.test/ref).",
            &registry,
            &EngineRoutePolicy::default(),
        );

        let blocks = clip.blocks.as_ref().expect("parsed blocks");
        assert!(matches!(blocks.first(), Some(Block::Heading { .. })));
        assert_eq!(clip.links, vec!["https://example.test/ref"]);
        assert!(clip.text.contains("Heading"));
        assert!(clip.text.contains("reference"));
    }

    #[test]
    fn submission_target_is_not_imported_as_a_navigation_link() {
        let blocks = vec![Block::Paragraph {
            spans: vec![InlineSpan::Submit {
                target: "spartan://example.test/write".to_string(),
                spans: vec![InlineSpan::Link {
                    url: "gemini://example.test/help".to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("help".to_string())],
                    predicate: None,
                }],
            }],
        }];

        assert_eq!(block_links(&blocks), vec!["gemini://example.test/help"]);
    }

    #[test]
    fn cropped_visual_uses_element_rect() {
        let rgba = vec![255u8; 4 * 4 * 4];
        let png = encode_png_rgba(&rgba, 4, 4).expect("png");
        let mut clip = ClipFragment {
            source_url: "https://example.test/post".to_string(),
            title: Some("Visual".to_string()),
            text: "A useful paragraph.".to_string(),
            html: None,
            selector: None,
            links: Vec::new(),
            rect: Some(ClipRect {
                x: 1.0,
                y: 1.0,
                width: 2.0,
                height: 2.0,
            }),
            visual: None,
            blocks: None,
        };

        assert!(attach_cropped_visual(&mut clip, &png, (4, 4)));
        let visual = clip.visual.as_ref().expect("visual");
        assert_eq!((visual.width, visual.height), (2, 2));
        assert!(visual.data_uri.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn write_clip_node_records_graph_relation_and_thumbnail() {
        let mut graph = Graph::new();
        let source = graph.add_node("https://example.test/post".to_string(), Default::default());
        let png = encode_png_rgba(&vec![128u8; 2 * 2 * 4], 2, 2).expect("png");
        let clip = ClipFragment {
            source_url: "https://example.test/post".to_string(),
            title: Some("Interesting bit".to_string()),
            text: "A useful paragraph.".to_string(),
            html: None,
            selector: None,
            links: Vec::new(),
            rect: None,
            visual: Some(ClipVisual {
                png_bytes: png.clone(),
                width: 2,
                height: 2,
                data_uri: format!("data:image/png;base64,{}", encode_base64(&png)),
            }),
            blocks: None,
        };

        let stored = ImageRef::new([21u8; 32], 2, 2);
        let member = write_clip_node(&mut graph, source, "knot://clip/test", &clip, Some(stored))
            .expect("clip member");
        let (clip_key, clip_node) = graph.get_node_by_id(member).expect("clip node");
        assert_eq!(clip_node.preview(), Some(&stored));
        assert!(
            clip_node
                .body
                .as_deref()
                .unwrap_or("")
                .contains("A useful paragraph.")
        );
        assert!(graph.relations().any(|relation| {
            relation.from == clip_key
                && relation.to == source
                && relation.kind == RelationKind::Provenance(ProvenanceSubKind::ClippedFrom)
        }));
    }
}
