/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Djot substrate proof-of-concept for knot bodies (design doc §10, Phase 1).
//!
//! Demonstrates that djot's *native* constructs replace the CommonMark knot's
//! fenced-code-as-data hacks (§2.2):
//!
//! - a **definition list** (`Term` / `: value`) becomes a [`DocumentBlock::MetadataRow`]
//!   — no `metadata-row` fence;
//! - a **div with a class** (`::: feed-entry`, `::: badge`) becomes the matching
//!   semantic block, reading typed attributes via `Attributes::get_value` — no
//!   `feed-entry` code-fence-as-data;
//! - headings / paragraphs / code blocks map structurally.
//!
//! This is **additive**: the shipped CommonMark [`super::KnotEngine`] is
//! untouched, and this module is not wired into the engine registry. Inline
//! link / wikilink-predicate handling (which needs an `InlineSpan` predicate
//! slot, tied to the linked-data plan) and the `to_djot_knot` round-trip are
//! later phases of §10.5. Parser: `jotdown` (the Rust djot pull-parser).

use inker::{
    DocumentBlock, DocumentDiagnostic, DocumentProvenance, DocumentTrustState, Engine,
    EngineDocument, EngineError, EngineInput, InlineSpan, inline_text,
};
use jotdown::{Container, Event, Parser};

/// Parse a djot knot body into [`DocumentBlock`]s. Proof-of-concept scope; see
/// the module docs.
/// Parse a djot knot body into [`DocumentBlock`]s (blocks only). See
/// [`parse_djot_knot_body_validated`] for the schema diagnostics.
pub fn parse_djot_knot_body(body: &str) -> Vec<DocumentBlock> {
    parse_djot_knot_body_validated(body).0
}

/// Parse a djot knot body, returning blocks plus schema diagnostics (an unknown
/// div class, or a recognized div missing a required attribute). The recognized
/// vocabulary is declared data ([`KNOT_DIV_SCHEMA`]) — the Markdoc lesson
/// (§10.4): validate against a schema, render the unrecognized generically.
pub fn parse_djot_knot_body_validated(
    body: &str,
) -> (Vec<DocumentBlock>, Vec<DocumentDiagnostic>) {
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    let mut text = String::new();
    let mut heading_level: Option<u8> = None;
    let mut code_language: Option<String> = None;
    let mut div: Option<DivCtx> = None;
    let mut dl: Option<DlCtx> = None;
    // A standalone attribute line (`{title=… url=…}`) is emitted by jotdown as
    // `Event::Attributes` and applies to the element that follows. Stash it and
    // fold it into the next div, so attribute attachment works whether jotdown
    // folds it into `Start(Div, attrs)` or emits it separately.
    let mut pending = PendingAttrs::default();

    for event in Parser::new(body) {
        match event {
            Event::Attributes(attrs) => {
                pending.title = attrs.get_value("title").map(|v| v.to_string());
                pending.url = attrs.get_value("url").map(|v| v.to_string());
                pending.date = attrs.get_value("date").map(|v| v.to_string());
            }
            Event::Start(Container::Heading { level, .. }, _) => {
                heading_level = Some(level as u8);
                text.clear();
            }
            Event::End(Container::Heading { .. }) => {
                if let Some(level) = heading_level.take() {
                    out.push(DocumentBlock::Heading {
                        level,
                        spans: text_spans(std::mem::take(&mut text)),
                    });
                }
            }
            Event::Start(Container::Div { class }, attrs) => {
                div = Some(DivCtx {
                    class: class.to_string(),
                    title: attrs.get_value("title").map(|v| v.to_string()).or(pending.title.take()),
                    url: attrs.get_value("url").map(|v| v.to_string()).or(pending.url.take()),
                    date: attrs.get_value("date").map(|v| v.to_string()).or(pending.date.take()),
                });
                text.clear();
            }
            Event::End(Container::Div { .. }) => {
                if let Some(ctx) = div.take() {
                    if let Some(diagnostic) = validate_div(&ctx) {
                        diagnostics.push(diagnostic);
                    }
                    out.push(ctx.into_block(std::mem::take(&mut text)));
                }
            }
            Event::Start(Container::DescriptionList, _) => dl = Some(DlCtx::default()),
            Event::End(Container::DescriptionList) => dl = None,
            Event::Start(Container::DescriptionTerm, _) => text.clear(),
            Event::End(Container::DescriptionTerm) => {
                if let Some(d) = dl.as_mut() {
                    d.term = std::mem::take(&mut text).trim().to_string();
                }
            }
            Event::Start(Container::DescriptionDetails, _) => text.clear(),
            Event::End(Container::DescriptionDetails) => {
                if let Some(d) = dl.as_mut() {
                    out.push(DocumentBlock::MetadataRow {
                        label: d.term.clone(),
                        value: std::mem::take(&mut text).trim().to_string(),
                    });
                }
            }
            Event::Start(Container::CodeBlock { language }, _) => {
                code_language = Some(language.to_string());
                text.clear();
            }
            Event::End(Container::CodeBlock { .. }) => {
                out.push(DocumentBlock::CodeBlock {
                    language: code_language.take().filter(|s| !s.is_empty()),
                    text: std::mem::take(&mut text),
                });
            }
            Event::End(Container::Paragraph) => {
                // A *top-level* paragraph emits a block; a paragraph inside a div
                // or description-details lets its text accumulate for that
                // container's own `End` handler instead.
                if div.is_none() && dl.is_none() && heading_level.is_none() {
                    let t = std::mem::take(&mut text);
                    if !t.trim().is_empty() {
                        out.push(DocumentBlock::Paragraph { spans: text_spans(t) });
                    }
                }
            }
            Event::Str(s) => text.push_str(s.as_ref()),
            Event::Softbreak => text.push(' '),
            Event::Hardbreak => text.push('\n'),
            _ => {}
        }
    }
    (out, diagnostics)
}

fn text_spans(text: String) -> Vec<InlineSpan> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![InlineSpan::Text(text)]
    }
}

#[derive(Default)]
struct PendingAttrs {
    title: Option<String>,
    url: Option<String>,
    date: Option<String>,
}

struct DivCtx {
    class: String,
    title: Option<String>,
    url: Option<String>,
    date: Option<String>,
}

impl DivCtx {
    fn into_block(self, content: String) -> DocumentBlock {
        let summary = {
            let trimmed = content.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        match self.class.as_str() {
            "feed-entry" => DocumentBlock::FeedEntry {
                title: self.title.unwrap_or_default(),
                date: self.date,
                summary,
                article_url: self.url,
                source_url: None,
            },
            "feed-header" => DocumentBlock::FeedHeader {
                title: self.title.unwrap_or_default(),
                subtitle: None,
                summary,
                source_url: self.url,
            },
            "badge" => DocumentBlock::Badge {
                text: content.trim().to_string(),
            },
            // Unknown div classes degrade to a stored quote rather than being
            // dropped — the open-tail discipline (recognized core, stored tail).
            _ => DocumentBlock::Quote {
                blocks: vec![DocumentBlock::Paragraph {
                    spans: text_spans(content),
                }],
            },
        }
    }
}

#[derive(Default)]
struct DlCtx {
    term: String,
}

/// A recognized knot div class and the attributes it requires. The recognized
/// vocabulary is declared data (the Markdoc lesson, §10.4): the parser validates
/// divs against this table and renders unrecognized ones generically.
struct DivClassSpec {
    class: &'static str,
    required: &'static [&'static str],
}

const KNOT_DIV_SCHEMA: &[DivClassSpec] = &[
    DivClassSpec {
        class: "feed-entry",
        required: &["title"],
    },
    DivClassSpec {
        class: "feed-header",
        required: &["title"],
    },
    DivClassSpec {
        class: "badge",
        required: &[],
    },
];

fn div_spec(class: &str) -> Option<&'static DivClassSpec> {
    KNOT_DIV_SCHEMA.iter().find(|spec| spec.class == class)
}

/// Validate a div against [`KNOT_DIV_SCHEMA`]: an unknown class (rendered
/// generically) or a recognized class missing a required attribute yields one
/// diagnostic; a well-formed div yields `None`. Required-attribute checking is
/// limited to the attributes the parser captures (`title` / `url` / `date`);
/// preserving *unknown* attributes (the rest of the open tail) needs attribute
/// iteration and is a later phase.
fn validate_div(ctx: &DivCtx) -> Option<DocumentDiagnostic> {
    let Some(spec) = div_spec(&ctx.class) else {
        return Some(DocumentDiagnostic::UnsupportedConstruct(format!(
            "unknown knot div class '{}'; rendered generically",
            ctx.class
        )));
    };
    let present = |attr: &str| match attr {
        "title" => ctx.title.is_some(),
        "url" => ctx.url.is_some(),
        "date" => ctx.date.is_some(),
        _ => false,
    };
    spec.required.iter().find(|&&attr| !present(attr)).map(|&attr| {
        DocumentDiagnostic::ParseWarning(format!(
            "knot '{}' div missing required attribute '{}'",
            ctx.class, attr
        ))
    })
}

/// Serialize document blocks back into a djot knot body — the dual of
/// [`parse_djot_knot_body`] (design doc §10.5 Phase 2). Semantic blocks emit as
/// the native djot constructs they parsed from: `MetadataRow` → a definition
/// list, `FeedEntry` / `FeedHeader` / `Badge` → an attributed div. Round-trips
/// on the recognized subset (see `round_trip_preserves_semantic_blocks`);
/// byte-faithful protocol-fence preservation is a later phase.
pub fn blocks_to_djot(blocks: &[DocumentBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if !out.is_empty() {
            out.push('\n');
        }
        emit_block(block, &mut out);
    }
    out
}

fn emit_block(block: &DocumentBlock, out: &mut String) {
    match block {
        DocumentBlock::Heading { level, spans } => {
            for _ in 0..(*level).max(1) {
                out.push('#');
            }
            out.push(' ');
            out.push_str(&inline_text(spans));
            out.push('\n');
        }
        DocumentBlock::Paragraph { spans } => {
            out.push_str(&inline_text(spans));
            out.push('\n');
        }
        DocumentBlock::MetadataRow { label, value } => {
            out.push_str(": ");
            out.push_str(label);
            out.push_str("\n\n  ");
            out.push_str(value);
            out.push('\n');
        }
        DocumentBlock::Badge { text } => {
            out.push_str("::: badge\n");
            out.push_str(text);
            out.push_str("\n:::\n");
        }
        DocumentBlock::FeedEntry {
            title,
            date,
            summary,
            article_url,
            ..
        } => {
            emit_div_attrs(out, title, article_url.as_deref(), date.as_deref());
            out.push_str("::: feed-entry\n");
            if let Some(s) = summary {
                out.push_str(s);
                out.push('\n');
            }
            out.push_str(":::\n");
        }
        DocumentBlock::FeedHeader {
            title,
            summary,
            source_url,
            ..
        } => {
            emit_div_attrs(out, title, source_url.as_deref(), None);
            out.push_str("::: feed-header\n");
            if let Some(s) = summary {
                out.push_str(s);
                out.push('\n');
            }
            out.push_str(":::\n");
        }
        DocumentBlock::CodeBlock { language, text } => {
            out.push_str("```");
            if let Some(lang) = language {
                out.push_str(lang);
            }
            out.push('\n');
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n");
        }
        DocumentBlock::Preformatted { text } => {
            out.push_str("```\n");
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n");
        }
        DocumentBlock::Quote { blocks } => {
            let mut inner = String::new();
            for (i, b) in blocks.iter().enumerate() {
                if i > 0 {
                    inner.push('\n');
                }
                emit_block(b, &mut inner);
            }
            for line in inner.lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
        }
        DocumentBlock::List { ordered, items } => {
            for item in items {
                let mut inner = String::new();
                for (i, b) in item.iter().enumerate() {
                    if i > 0 {
                        inner.push(' ');
                    }
                    emit_block(b, &mut inner);
                }
                out.push_str(if *ordered { "1. " } else { "- " });
                out.push_str(inner.trim());
                out.push('\n');
            }
        }
        DocumentBlock::Image { url, alt } => {
            out.push_str("![");
            out.push_str(alt);
            out.push_str("](");
            out.push_str(url);
            out.push_str(")\n");
        }
        DocumentBlock::Rule => out.push_str("----\n"),
    }
}

fn emit_div_attrs(out: &mut String, title: &str, url: Option<&str>, date: Option<&str>) {
    out.push_str("{title=\"");
    out.push_str(title);
    out.push('"');
    if let Some(url) = url {
        out.push_str(" url=\"");
        out.push_str(url);
        out.push('"');
    }
    if let Some(date) = date {
        out.push_str(" date=\"");
        out.push_str(date);
        out.push('"');
    }
    out.push_str("}\n");
}

/// Stable engine identifier for the experimental djot knot engine.
pub const ENGINE_ID: &str = "nematic.knot-djot";

/// Experimental knot engine whose body grammar is **djot** rather than
/// CommonMark (design doc §10). Frontmatter handling (title / provenance /
/// trust / `note_kind` / `tags`) is shared with the shipped [`super::KnotEngine`]
/// via `super::apply_frontmatter`, so a djot knot and a CommonMark knot carry
/// identical note semantics. Not registered in `engines()` (it shares the
/// `text/x-knot` content-type), so a host routes to it by [`ENGINE_ID`].
pub struct DjotKnotEngine;

impl DjotKnotEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DjotKnotEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for DjotKnotEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError> {
        let (frontmatter, body) = super::split_frontmatter(&input.body);
        let (mut blocks, diagnostics) = parse_djot_knot_body_validated(body);
        // Parity with the CommonMark knot: expand protocol-tagged code fences
        // (gemtext / gopher / nex / feed-entry / …) into real blocks, then
        // rewrite `[[wikilinks]]` and `#hashtags`. Both passes are engine-agnostic
        // (they walk `DocumentBlock`s), so the djot body gains the same semantics
        // and CommonMark-style fenced knots still work under the djot default.
        super::expand::expand_fenced_blocks(&mut blocks);
        super::expand::rewrite_inline_extensions(&mut blocks);
        let mut doc = EngineDocument {
            address: input.address.clone(),
            title: None,
            content_type: "text/x-knot".to_string(),
            lang: None,
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::default(),
            diagnostics,
            blocks,
        };
        super::apply_frontmatter(
            &mut doc,
            &frontmatter,
            ENGINE_ID,
            &input.address,
            input.content_type.as_deref(),
        );
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_div_class_renders_generically_with_a_diagnostic() {
        let (blocks, diagnostics) = parse_djot_knot_body_validated("::: mystery\nx\n:::\n");
        assert!(matches!(blocks.first(), Some(DocumentBlock::Quote { .. })));
        assert!(matches!(
            diagnostics.first(),
            Some(DocumentDiagnostic::UnsupportedConstruct(m)) if m.contains("mystery")
        ));
    }

    #[test]
    fn feed_entry_missing_required_title_warns() {
        let (blocks, diagnostics) = parse_djot_knot_body_validated("::: feed-entry\nbody\n:::\n");
        assert!(matches!(blocks.first(), Some(DocumentBlock::FeedEntry { .. })));
        assert!(matches!(
            diagnostics.first(),
            Some(DocumentDiagnostic::ParseWarning(m)) if m.contains("title")
        ));
    }

    #[test]
    fn recognized_div_with_required_attrs_has_no_diagnostics() {
        let (_, diagnostics) =
            parse_djot_knot_body_validated("{title=\"Article\"}\n::: feed-entry\nbody\n:::\n");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn djot_engine_expands_protocol_fences_for_parity() {
        let input = EngineInput {
            address: "knot:test".to_string(),
            body: "---\ntitle: T\n---\n\n```gemtext\n=> gemini://x/ a link\n```\n".to_string(),
            content_type: None,
        };
        let doc = DjotKnotEngine::new().render(&input).unwrap();
        // The gemtext fence expanded into real blocks (the gemini link), so no
        // raw gemtext code block remains — parity with the CommonMark knot.
        assert!(!doc.blocks.iter().any(|b| matches!(
            b,
            DocumentBlock::CodeBlock { language, .. } if language.as_deref() == Some("gemtext")
        )));
        assert!(doc.outgoing_links().iter().any(|u| u.contains("gemini://x/")));
    }

    #[test]
    fn round_trip_preserves_semantic_blocks() {
        let source = concat!(
            "# My research\n\n",
            "A note about things.\n\n",
            "{title=\"Article\" url=\"https://blog.test/post\" date=\"2026-05-08\"}\n",
            "::: feed-entry\nA summary.\n:::\n\n",
            "::: badge\nresearch\n:::\n\n",
            ": Trust\n\n  tofu\n",
        );
        let blocks = parse_djot_knot_body(source);
        let reparsed = parse_djot_knot_body(&blocks_to_djot(&blocks));
        assert_eq!(blocks, reparsed);
    }

    #[test]
    fn djot_engine_renders_frontmatter_and_body() {
        let input = EngineInput {
            address: "knot:test".to_string(),
            body: "---\ntitle: My Note\ntrust: tofu\nnote_kind: clip\n---\n\n# Heading\n\nBody.\n"
                .to_string(),
            content_type: None,
        };
        let doc = DjotKnotEngine::new().render(&input).unwrap();
        assert_eq!(doc.title.as_deref(), Some("My Note"));
        assert_eq!(doc.trust, DocumentTrustState::Tofu);
        assert_eq!(doc.content_type, "text/x-knot");
        assert_eq!(doc.provenance.source_kind.as_deref(), Some(ENGINE_ID));
        // `note_kind` prefixes a MetadataRow; the djot body follows.
        assert!(matches!(
            doc.blocks.first(),
            Some(DocumentBlock::MetadataRow { label, value }) if label == "kind" && value == "clip"
        ));
        assert!(
            doc.blocks
                .iter()
                .any(|b| matches!(b, DocumentBlock::Heading { .. }))
        );
    }

    #[test]
    fn description_list_becomes_metadata_rows() {
        // Djot description-list syntax: `: term` then an indented definition.
        let blocks = parse_djot_knot_body(": Trust\n\n  tofu\n\n: Login\n\n  alice\n");
        assert_eq!(
            blocks,
            vec![
                DocumentBlock::MetadataRow {
                    label: "Trust".into(),
                    value: "tofu".into(),
                },
                DocumentBlock::MetadataRow {
                    label: "Login".into(),
                    value: "alice".into(),
                },
            ]
        );
    }

    #[test]
    fn heading_and_paragraph_map_structurally() {
        let blocks = parse_djot_knot_body("# My research\n\nA note about things.\n");
        assert_eq!(
            blocks,
            vec![
                DocumentBlock::Heading {
                    level: 1,
                    spans: vec![InlineSpan::Text("My research".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("A note about things.".into())],
                },
            ]
        );
    }

    #[test]
    fn badge_div_becomes_badge() {
        let blocks = parse_djot_knot_body("::: badge\nresearch\n:::\n");
        assert_eq!(
            blocks,
            vec![DocumentBlock::Badge {
                text: "research".into()
            }]
        );
    }

    #[test]
    fn feed_entry_div_reads_attributes() {
        let body = "{title=\"Article\" url=\"https://blog.test/post\" date=\"2026-05-08\"}\n::: feed-entry\nA summary.\n:::\n";
        let blocks = parse_djot_knot_body(body);
        assert_eq!(
            blocks,
            vec![DocumentBlock::FeedEntry {
                title: "Article".into(),
                date: Some("2026-05-08".into()),
                summary: Some("A summary.".into()),
                article_url: Some("https://blog.test/post".into()),
                source_url: None,
            }]
        );
    }
}
