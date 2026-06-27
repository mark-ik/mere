/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `EngineDocument` → serval views: the note render path.
//!
//! Maps the portable block model (`DocumentBlock` / `InlineSpan`, what
//! `DjotKnotEngine` and every other engine produce) into xilem_serval element
//! views, so a note renders through serval-layout + netrender like any web
//! document — the same `ScriptedDom` path the chrome builds every frame. This is
//! the block→view mapper of the note-as-routed-serval-document-tile reframe (djot
//! editor plan, 2026-06-27): the real web engine renders the note, so
//! document-canvas stays off the note path.
//!
//! Slice 1 of the reframe: pure mapping, no tile wiring yet. The render
//! integration (a content surface over these views, proven on `mere://welcome`)
//! is the next slice.

use inker::{DocumentBlock, EngineDocument, InlineSpan};
use xilem_serval::{el, text, AnyView, ServalCtx, ServalElement};

/// A type-erased note view child: a serval element or text node.
///
/// State/Action are `()` for now — the rendered note is read-only in slice 1
/// (links render as `<a href>`, but click-navigation is not yet wired). When edit
/// mode and link navigation land, this gains the tile's interaction types.
pub type NoteChild = Box<dyn AnyView<(), (), ServalCtx, ServalElement>>;

/// Map a rendered document into serval block views, in document order. The caller
/// wraps these in the tile's container element.
pub fn document_views(doc: &EngineDocument) -> Vec<NoteChild> {
    doc.blocks.iter().map(block_view).collect()
}

/// The outer HTML element tag a block maps to. Split out so the tag decisions are
/// unit-testable without driving the view tree.
fn block_tag(block: &DocumentBlock) -> &'static str {
    match block {
        DocumentBlock::Heading { level, .. } => heading_tag(*level),
        DocumentBlock::Paragraph { .. } => "p",
        DocumentBlock::CodeBlock { .. } | DocumentBlock::Preformatted { .. } => "pre",
        DocumentBlock::Quote { .. } => "blockquote",
        DocumentBlock::List { ordered: false, .. } => "ul",
        DocumentBlock::List { ordered: true, .. } => "ol",
        DocumentBlock::Image { .. } => "img",
        DocumentBlock::Rule => "hr",
        DocumentBlock::FeedHeader { .. } => "header",
        DocumentBlock::FeedEntry { .. } => "article",
        DocumentBlock::MetadataRow { .. } | DocumentBlock::Badge { .. } => "p",
    }
}

fn heading_tag(level: u8) -> &'static str {
    match level {
        1 => "h1",
        2 => "h2",
        3 => "h3",
        4 => "h4",
        5 => "h5",
        _ => "h6",
    }
}

/// One block → one serval element view.
fn block_view(block: &DocumentBlock) -> NoteChild {
    match block {
        DocumentBlock::Heading { level, spans } => {
            Box::new(el(heading_tag(*level), span_views(spans)))
        }
        DocumentBlock::Paragraph { spans } => Box::new(el("p", span_views(spans))),
        DocumentBlock::CodeBlock { language, text: code } => {
            let code_el: Vec<NoteChild> = vec![Box::new(el("code", text(code.clone())))];
            let mut pre = el("pre", code_el);
            if let Some(lang) = language {
                pre = pre.attr("data-lang", lang.clone());
            }
            Box::new(pre)
        }
        DocumentBlock::Quote { blocks } => {
            let inner: Vec<NoteChild> = blocks.iter().map(block_view).collect();
            Box::new(el("blockquote", inner))
        }
        DocumentBlock::List { ordered, items } => {
            let lis: Vec<NoteChild> = items
                .iter()
                .map(|item| {
                    let item_blocks: Vec<NoteChild> = item.iter().map(block_view).collect();
                    Box::new(el("li", item_blocks)) as NoteChild
                })
                .collect();
            Box::new(el(if *ordered { "ol" } else { "ul" }, lis))
        }
        DocumentBlock::Image { url, alt } => {
            Box::new(el("img", ()).attr("src", url.clone()).attr("alt", alt.clone()))
        }
        DocumentBlock::Preformatted { text: t } => Box::new(el("pre", text(t.clone()))),
        DocumentBlock::Rule => Box::new(el("hr", ())),
        // Semantic blocks (feed / protocol engines): a knot note rarely carries
        // these, but the mapper is total so any routed document renders legibly.
        DocumentBlock::FeedHeader { title, subtitle, .. } => {
            let mut kids: Vec<NoteChild> = vec![Box::new(el("h1", text(title.clone())))];
            if let Some(sub) = subtitle {
                kids.push(Box::new(el("p", text(sub.clone()))));
            }
            Box::new(el("header", kids))
        }
        DocumentBlock::FeedEntry { title, summary, .. } => {
            let mut kids: Vec<NoteChild> = vec![Box::new(el("h2", text(title.clone())))];
            if let Some(s) = summary {
                kids.push(Box::new(el("p", text(s.clone()))));
            }
            Box::new(el("article", kids))
        }
        DocumentBlock::MetadataRow { label, value } => {
            let kids: Vec<NoteChild> = vec![
                Box::new(el("strong", text(format!("{label}: ")))),
                Box::new(text(value.clone())),
            ];
            Box::new(el("p", kids))
        }
        DocumentBlock::Badge { text: t } => {
            Box::new(el("p", text(t.clone())).attr("class", "badge"))
        }
    }
}

/// Inline spans → inline serval views (text, `em`, `strong`, `code`, `a`, `br`).
fn span_views(spans: &[InlineSpan]) -> Vec<NoteChild> {
    spans.iter().map(span_view).collect()
}

fn span_view(span: &InlineSpan) -> NoteChild {
    match span {
        InlineSpan::Text(t) => Box::new(text(t.clone())),
        InlineSpan::Code(t) => Box::new(el("code", text(t.clone()))),
        InlineSpan::Emphasis(inner) => Box::new(el("em", span_views(inner))),
        InlineSpan::Strong(inner) => Box::new(el("strong", span_views(inner))),
        InlineSpan::Link { url, title, spans, .. } => {
            let mut a = el("a", span_views(spans)).attr("href", url.clone());
            if let Some(t) = title {
                a = a.attr("title", t.clone());
            }
            Box::new(a)
        }
        InlineSpan::LineBreak => Box::new(el("br", ())),
        InlineSpan::SoftBreak => Box::new(text(" ".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(blocks: Vec<DocumentBlock>) -> EngineDocument {
        EngineDocument {
            address: "knot://test".into(),
            title: None,
            content_type: "text/x-knot".into(),
            lang: None,
            provenance: Default::default(),
            trust: Default::default(),
            diagnostics: Vec::new(),
            blocks,
        }
    }

    #[test]
    fn block_tags_map_to_html_elements() {
        assert_eq!(block_tag(&DocumentBlock::Heading { level: 1, spans: vec![] }), "h1");
        assert_eq!(block_tag(&DocumentBlock::Heading { level: 3, spans: vec![] }), "h3");
        // Levels past 6 clamp to h6 (HTML has no h7+).
        assert_eq!(block_tag(&DocumentBlock::Heading { level: 9, spans: vec![] }), "h6");
        assert_eq!(block_tag(&DocumentBlock::Paragraph { spans: vec![] }), "p");
        assert_eq!(
            block_tag(&DocumentBlock::CodeBlock { language: None, text: String::new() }),
            "pre"
        );
        assert_eq!(block_tag(&DocumentBlock::Quote { blocks: vec![] }), "blockquote");
        assert_eq!(block_tag(&DocumentBlock::List { ordered: false, items: vec![] }), "ul");
        assert_eq!(block_tag(&DocumentBlock::List { ordered: true, items: vec![] }), "ol");
        assert_eq!(block_tag(&DocumentBlock::Rule), "hr");
        assert_eq!(
            block_tag(&DocumentBlock::Image { url: String::new(), alt: String::new() }),
            "img"
        );
    }

    #[test]
    fn document_views_one_per_block() {
        let d = doc(vec![
            DocumentBlock::Heading { level: 1, spans: vec![InlineSpan::Text("Mere".into())] },
            DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text("A graph-shaped browser.".into())],
            },
        ]);
        assert_eq!(document_views(&d).len(), 2);
    }

    #[test]
    fn every_block_and_span_maps_without_panic() {
        // Constructs every block variant and every inline span, then maps the
        // whole document — exercising every match arm (no panic = all handled).
        // Construction is enough to drive the mapping logic; rendering the tree is
        // the headed-verify step in the next slice.
        let spans = vec![
            InlineSpan::Text("t".into()),
            InlineSpan::Code("c".into()),
            InlineSpan::Emphasis(vec![InlineSpan::Text("e".into())]),
            InlineSpan::Strong(vec![InlineSpan::Text("s".into())]),
            InlineSpan::Link {
                url: "u".into(),
                title: Some("ti".into()),
                spans: vec![InlineSpan::Text("l".into())],
                predicate: None,
            },
            InlineSpan::LineBreak,
            InlineSpan::SoftBreak,
        ];
        let d = doc(vec![
            DocumentBlock::Heading { level: 2, spans: spans.clone() },
            DocumentBlock::Paragraph { spans: spans.clone() },
            DocumentBlock::CodeBlock { language: Some("rust".into()), text: "fn x() {}".into() },
            DocumentBlock::Quote {
                blocks: vec![DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("q".into())],
                }],
            },
            DocumentBlock::List {
                ordered: false,
                items: vec![vec![DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("i".into())],
                }]],
            },
            DocumentBlock::Image { url: "img".into(), alt: "a".into() },
            DocumentBlock::Preformatted { text: "pre".into() },
            DocumentBlock::Rule,
            DocumentBlock::FeedHeader {
                title: "f".into(),
                subtitle: Some("s".into()),
                summary: None,
                source_url: None,
            },
            DocumentBlock::FeedEntry {
                title: "e".into(),
                date: None,
                summary: Some("s".into()),
                article_url: None,
                source_url: None,
            },
            DocumentBlock::MetadataRow { label: "k".into(), value: "v".into() },
            DocumentBlock::Badge { text: "b".into() },
        ]);
        assert_eq!(document_views(&d).len(), 12);
    }
}
