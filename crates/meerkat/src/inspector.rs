/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Focused-content inspection rows for the D8 Inspector pane.

use inker::{
    Block, DocumentDiagnostic, DocumentTrustState, EngineDocument, EngineInput, EngineRegistry,
};
use kernel::graph::Node;

use crate::fetch::{ContentState, Fetched};

pub(super) fn inspector_rows(
    node: Option<&Node>,
    state: Option<&ContentState>,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    match node {
        Some(node) => {
            rows.push(("Title".to_string(), display_title(node)));
            rows.push(("URL".to_string(), node.url().to_string()));
            rows.push(("Node id".to_string(), node.id.to_string()));
            rows.push(("Addresses".to_string(), node.addresses.len().to_string()));
            rows.push(("Pinned".to_string(), yes_no(node.is_pinned)));
            rows.push((
                "Tags".to_string(),
                summarize_strings(node.tags.iter().map(String::as_str)),
            ));
            rows.push((
                "Import provenance".to_string(),
                summarize_import_provenance(node),
            ));
            rows.push((
                "Classifications".to_string(),
                summarize_classifications(node),
            ));
            rows.push((
                "Node metadata".to_string(),
                format!(
                    "mime_hint={}; viewer={}; compat={}",
                    node.mime_hint.as_deref().unwrap_or("none"),
                    node.viewer_override.as_deref().unwrap_or("auto"),
                    node.compat_mode
                ),
            ));
        }
        None => rows.push(("Focused node".to_string(), "none".to_string())),
    }

    rows.extend(content_rows(node.map(Node::url), state));
    rows
}

fn content_rows(url: Option<&str>, state: Option<&ContentState>) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    rows.push(("Fetch state".to_string(), content_state_label(state)));
    match state {
        Some(ContentState::Ready(fetched)) => {
            rows.push((
                "Content type".to_string(),
                fetched
                    .content_type
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string(),
            ));
            rows.push((
                "Body".to_string(),
                format!(
                    "{} chars / {} bytes",
                    fetched.body.chars().count(),
                    fetched.body.len()
                ),
            ));
            rows.extend(document_rows(url.unwrap_or(""), fetched));
        }
        Some(ContentState::Failed(reason)) => {
            rows.push(("Failure".to_string(), truncate(reason, 120)));
            rows.push(("Parse state".to_string(), "not parsed".to_string()));
        }
        Some(ContentState::Loading) => {
            rows.push(("Parse state".to_string(), "waiting for fetch".to_string()));
        }
        None => rows.push(("Parse state".to_string(), "no fetched media".to_string())),
    }
    rows
}

fn document_rows(url: &str, fetched: &Fetched) -> Vec<(String, String)> {
    if is_html(fetched.content_type.as_deref()) {
        let extract =
            serval_extract::extract(&serval_static_dom::StaticDocument::parse(&fetched.body));
        return vec![
            ("Parser lane".to_string(), "serval.html".to_string()),
            (
                "Document title".to_string(),
                extract.title.as_deref().unwrap_or("none").to_string(),
            ),
            ("Trust".to_string(), inferred_transport_trust(url)),
            (
                "Document structure".to_string(),
                summarize_page_extract(&extract),
            ),
            (
                "Outgoing links".to_string(),
                extract.links.len().to_string(),
            ),
            (
                "Parse diagnostics".to_string(),
                "serval-extract PageExtract".to_string(),
            ),
        ];
    }

    let Some(engine_id) = engine_id_for(fetched.content_type.as_deref()) else {
        return vec![
            ("Parser lane".to_string(), "plain fallback".to_string()),
            ("Trust".to_string(), inferred_transport_trust(url)),
            (
                "Document structure".to_string(),
                "fallback plain text document".to_string(),
            ),
            ("Parse diagnostics".to_string(), "none".to_string()),
        ];
    };

    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    let Some(engine) = registry.engine(engine_id) else {
        return vec![(
            "Parser lane".to_string(),
            format!("{engine_id} unavailable"),
        )];
    };
    let mut input = EngineInput::new(url, fetched.body.clone());
    if let Some(content_type) = &fetched.content_type {
        input = input.with_content_type(content_type.clone());
    }
    match engine.render(&input) {
        Ok(document) => parsed_document_rows(engine_id, &document),
        Err(err) => vec![
            ("Parser lane".to_string(), engine_id.to_string()),
            (
                "Parse diagnostics".to_string(),
                format!("parse failed: {err}"),
            ),
        ],
    }
}

fn parsed_document_rows(engine_id: &str, document: &EngineDocument) -> Vec<(String, String)> {
    vec![
        ("Parser lane".to_string(), engine_id.to_string()),
        (
            "Document title".to_string(),
            document.title.as_deref().unwrap_or("none").to_string(),
        ),
        ("Trust".to_string(), trust_label(document.trust).to_string()),
        (
            "Provenance".to_string(),
            format!(
                "source={}; canonical={}",
                document
                    .provenance
                    .source_kind
                    .as_deref()
                    .unwrap_or("unknown"),
                document
                    .provenance
                    .canonical_uri
                    .as_deref()
                    .unwrap_or(&document.address)
            ),
        ),
        (
            "Document structure".to_string(),
            summarize_blocks(&document.blocks),
        ),
        (
            "Outgoing links".to_string(),
            document.outgoing_links().len().to_string(),
        ),
        (
            "Parse diagnostics".to_string(),
            summarize_diagnostics(&document.diagnostics),
        ),
    ]
}

fn summarize_blocks(blocks: &[Block]) -> String {
    let mut summary = BlockSummary::default();
    for block in blocks {
        summary.visit(block);
    }
    format!(
        "blocks={} headings={} paragraphs={} lists={} feed_entries={} metadata={} code={} images={}",
        summary.blocks,
        summary.headings,
        summary.paragraphs,
        summary.lists,
        summary.feed_entries,
        summary.metadata_rows,
        summary.code_blocks,
        summary.images
    )
}

fn summarize_page_extract(extract: &serval_extract::PageExtract) -> String {
    format!(
        "headings={} text={} reader_text={} metadata={}",
        extract.headings.len(),
        extract.text.chars().count(),
        extract
            .main_text
            .as_deref()
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or(0),
        extract.metadata.open_graph.len()
            + usize::from(extract.metadata.description.is_some())
            + usize::from(extract.metadata.canonical.is_some())
    )
}

#[derive(Default)]
struct BlockSummary {
    blocks: usize,
    headings: usize,
    paragraphs: usize,
    lists: usize,
    feed_entries: usize,
    metadata_rows: usize,
    code_blocks: usize,
    images: usize,
}

impl BlockSummary {
    fn visit(&mut self, block: &Block) {
        self.blocks += 1;
        match block {
            Block::Heading { .. } => self.headings += 1,
            Block::Paragraph { .. } => self.paragraphs += 1,
            Block::CodeBlock { .. } | Block::Preformatted { .. } => self.code_blocks += 1,
            Block::Quote { blocks } => {
                for child in blocks {
                    self.visit(child);
                }
            }
            Block::List { items, .. } => {
                self.lists += 1;
                for item in items {
                    for child in item {
                        self.visit(child);
                    }
                }
            }
            Block::Image { .. } => self.images += 1,
            Block::FeedEntry { .. } => self.feed_entries += 1,
            Block::MetadataRow { .. } => self.metadata_rows += 1,
            Block::Rule | Block::FeedHeader { .. } | Block::Badge { .. } | Block::Table { .. } => {}
        }
    }
}

fn summarize_html_structure(body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    format!(
        "h1={} h2={} p={} a={} img={} script={}",
        lower.matches("<h1").count(),
        lower.matches("<h2").count(),
        lower.matches("<p").count(),
        lower.matches("<a ").count() + lower.matches("<a>").count(),
        lower.matches("<img").count(),
        lower.matches("<script").count()
    )
}

fn summarize_diagnostics(diagnostics: &[DocumentDiagnostic]) -> String {
    if diagnostics.is_empty() {
        return "none".to_string();
    }
    let mut unsupported = 0usize;
    let mut degraded = 0usize;
    let mut warnings = 0usize;
    let mut raw = 0usize;
    for diagnostic in diagnostics {
        match diagnostic {
            DocumentDiagnostic::UnsupportedConstruct(_) => unsupported += 1,
            DocumentDiagnostic::DegradedRendering(_) => degraded += 1,
            DocumentDiagnostic::ParseWarning(_) => warnings += 1,
            DocumentDiagnostic::RawSourceFallback => raw += 1,
        }
    }
    format!("unsupported={unsupported} degraded={degraded} warnings={warnings} raw={raw}")
}

fn display_title(node: &Node) -> String {
    if node.title.trim().is_empty() {
        node.url().to_string()
    } else {
        node.title.clone()
    }
}

fn summarize_import_provenance(node: &Node) -> String {
    if node.import_provenance.is_empty() {
        return "none".to_string();
    }
    summarize_strings(node.import_provenance.iter().map(|p| {
        if p.source_label.is_empty() {
            p.source_id.as_str()
        } else {
            p.source_label.as_str()
        }
    }))
}

fn summarize_classifications(node: &Node) -> String {
    if node.classifications.is_empty() {
        return "none".to_string();
    }
    let labels = node.classifications.iter().map(|classification| {
        classification
            .label
            .as_deref()
            .unwrap_or(classification.value.as_str())
    });
    format!(
        "{} ({})",
        node.classifications.len(),
        summarize_strings(labels)
    )
}

fn summarize_strings<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let mut values: Vec<_> = values.filter(|value| !value.trim().is_empty()).collect();
    values.sort_unstable();
    if values.is_empty() {
        return "none".to_string();
    }
    let shown = values
        .iter()
        .take(4)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > 4 {
        format!("{shown}, +{}", values.len() - 4)
    } else {
        shown
    }
}

fn content_state_label(state: Option<&ContentState>) -> String {
    match state {
        Some(ContentState::Loading) => "loading".to_string(),
        Some(ContentState::Ready(_)) => "ready".to_string(),
        Some(ContentState::Failed(_)) => "failed".to_string(),
        None => "none".to_string(),
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn trust_label(trust: DocumentTrustState) -> &'static str {
    match trust {
        DocumentTrustState::Trusted => "trusted",
        DocumentTrustState::Tofu => "tofu",
        DocumentTrustState::Insecure => "insecure",
        DocumentTrustState::Broken => "broken",
        DocumentTrustState::Unknown => "unknown",
    }
}

fn inferred_transport_trust(url: &str) -> String {
    if url.starts_with("http://") {
        "insecure transport".to_string()
    } else {
        "unknown".to_string()
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn is_html(content_type: Option<&str>) -> bool {
    content_type
        .map(base_type)
        .is_some_and(|base| base == "text/html" || base == "application/xhtml+xml")
}

fn engine_id_for(content_type: Option<&str>) -> Option<&'static str> {
    let base = base_type(content_type?);
    Some(match base.as_str() {
        "text/markdown" | "text/x-markdown" => nematic::ENGINE_MARKDOWN,
        "text/gemini" => nematic::ENGINE_GEMTEXT,
        "text/plain" => nematic::ENGINE_TEXT,
        "application/gopher-menu" => nematic::ENGINE_GOPHER,
        "text/x-finger" => nematic::ENGINE_FINGER,
        "application/x-nex" => nematic::ENGINE_NEX,
        "application/x-guppy" => nematic::ENGINE_GUPPY,
        "application/x-titan" => nematic::ENGINE_TITAN,
        "message/x-misfin" => nematic::ENGINE_MISFIN,
        "application/rss+xml" | "application/atom+xml" | "application/feed+json" => {
            nematic::ENGINE_FEED
        }
        _ => return None,
    })
}

fn base_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Fetched;

    #[test]
    fn inspector_reports_markdown_document_structure() {
        let fetched = Fetched {
            content_type: Some("text/markdown".to_string()),
            body: "# Title\n\nBody with [link](https://example.test).".to_string(),
        };
        let rows = document_rows("https://example.test", &fetched);
        assert!(
            rows.iter()
                .any(|(k, v)| k == "Parser lane" && v == nematic::ENGINE_MARKDOWN)
        );
        assert!(
            rows.iter()
                .any(|(k, v)| k == "Document structure" && v.contains("headings=1"))
        );
        assert!(rows.iter().any(|(k, v)| k == "Outgoing links" && v == "1"));
    }

    #[test]
    fn inspector_reports_serval_extract_for_html() {
        let fetched = Fetched {
            content_type: Some("text/html".to_string()),
            body: "<html><head><title>The Page</title><meta name='description' content='Desc'></head><body><main><h1>Title</h1><p>Body text.</p><a href='/next'>Next</a></main></body></html>".to_string(),
        };
        let rows = document_rows("https://example.test", &fetched);
        assert!(
            rows.iter()
                .any(|(k, v)| k == "Document title" && v == "The Page")
        );
        assert!(
            rows.iter()
                .any(|(k, v)| k == "Document structure" && v.contains("headings=1"))
        );
        assert!(rows.iter().any(|(k, v)| k == "Outgoing links" && v == "1"));
        assert!(
            rows.iter()
                .any(|(k, v)| k == "Parse diagnostics" && v == "serval-extract PageExtract")
        );
    }
}
