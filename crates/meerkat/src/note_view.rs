/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `EngineDocument` → genet views: the **document-family** block→view mapper.
//!
//! Maps the portable block model (`Block` / `InlineSpan`, what
//! `DjotKnotEngine` and every other engine produce) into cambium element
//! views, so a document renders through genet-layout + netrender like any web
//! page — the same `ScriptedDom` path the chrome builds every frame.
//!
//! This is the document family's one renderer (native smolweb rendering plan,
//! 2026-06-27, Phase D): djot/knot, markdown, and reader-mode HTML all ride this
//! mapper, because all three lower to `Block`. The **smolweb family**
//! (gemtext, gopher, feed, …) does not come here — each gets its own native genet
//! view (`genet/smolweb-views`) so it stays shareable with pelt without
//! `Block`. `Block` keeps capture + cards; focused viewing goes
//! native.
//!
//! Slice 1 of the djot reframe: the mapper itself. `Block::Table` is a
//! pending prerequisite (the variant does not exist yet) that lands before the live
//! djot/markdown tile. The render-to-`Scene` surface is `crate::note_surface`.

use inker::{Block, EngineDocument, InlineSpan};
use cambium::{AnyView, GenetCtx, GenetElement, el, text};

/// A type-erased note view child: a genet element or text node.
///
/// State/Action are `()` for now — the rendered note is read-only in slice 1
/// (links render as `<a href>`, but click-navigation is not yet wired). When edit
/// mode and link navigation land, this gains the tile's interaction types.
pub type NoteChild = Box<dyn AnyView<(), (), GenetCtx, GenetElement>>;

/// Map a rendered document into genet block views, in document order. The caller
/// wraps these in the tile's container element.
pub fn document_views(doc: &EngineDocument) -> Vec<NoteChild> {
    doc.blocks.iter().map(block_view).collect()
}

/// The outer HTML element tag a block maps to. Split out so the tag decisions are
/// unit-testable without driving the view tree.
fn block_tag(block: &Block) -> &'static str {
    match block {
        Block::Heading { level, .. } => heading_tag(*level),
        Block::Paragraph { .. } => "p",
        Block::CodeBlock { .. } | Block::Preformatted { .. } => "pre",
        Block::Quote { .. } => "blockquote",
        Block::List { ordered: false, .. } => "ul",
        Block::List { ordered: true, .. } => "ol",
        Block::Image { .. } => "img",
        Block::Rule => "hr",
        Block::Table { .. } => "table",
        Block::FeedHeader { .. } => "header",
        Block::FeedEntry { .. } => "article",
        Block::MetadataRow { .. } | Block::Badge { .. } => "p",
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

/// One block → one genet element view.
fn block_view(block: &Block) -> NoteChild {
    match block {
        Block::Heading { level, spans } => Box::new(el(heading_tag(*level), span_views(spans))),
        Block::Paragraph { spans } => Box::new(el("p", span_views(spans))),
        Block::CodeBlock {
            language,
            text: code,
        } => {
            let code_el: Vec<NoteChild> = vec![Box::new(el("code", text(code.clone())))];
            let mut pre = el("pre", code_el);
            if let Some(lang) = language {
                pre = pre.attr("data-lang", lang.clone());
            }
            Box::new(pre)
        }
        Block::Quote { blocks } => {
            let inner: Vec<NoteChild> = blocks.iter().map(block_view).collect();
            Box::new(el("blockquote", inner))
        }
        Block::List { ordered, items } => {
            let lis: Vec<NoteChild> = items
                .iter()
                .map(|item| {
                    let item_blocks: Vec<NoteChild> = item.iter().map(block_view).collect();
                    Box::new(el("li", item_blocks)) as NoteChild
                })
                .collect();
            Box::new(el(if *ordered { "ol" } else { "ul" }, lis))
        }
        Block::Image { url, alt } => Box::new(
            el("img", ())
                .attr("src", url.clone())
                .attr("alt", alt.clone()),
        ),
        Block::Preformatted { text: t } => Box::new(el("pre", text(t.clone()))),
        Block::Rule => Box::new(el("hr", ())),
        // Semantic blocks (feed / protocol engines): a knot note rarely carries
        // these, but the mapper is total so any routed document renders legibly.
        Block::FeedHeader {
            title, subtitle, ..
        } => {
            let mut kids: Vec<NoteChild> = vec![Box::new(el("h1", text(title.clone())))];
            if let Some(sub) = subtitle {
                kids.push(Box::new(el("p", text(sub.clone()))));
            }
            Box::new(el("header", kids))
        }
        Block::FeedEntry { title, summary, .. } => {
            let mut kids: Vec<NoteChild> = vec![Box::new(el("h2", text(title.clone())))];
            if let Some(s) = summary {
                kids.push(Box::new(el("p", text(s.clone()))));
            }
            Box::new(el("article", kids))
        }
        Block::MetadataRow { label, value } => {
            let kids: Vec<NoteChild> = vec![
                Box::new(el("strong", text(format!("{label}: ")))),
                Box::new(text(value.clone())),
            ];
            Box::new(el("p", kids))
        }
        Block::Badge { text: t } => Box::new(el("p", text(t.clone())).attr("class", "badge")),
        Block::Table { header, rows, .. } => {
            // `<table>` with an optional `<thead>` of `<th>` and a `<tbody>` of
            // `<tr>`/`<td>`. Column alignment (the `..`) is a later CSS pass.
            let mut sections: Vec<NoteChild> = Vec::new();
            if !header.is_empty() {
                let cells: Vec<NoteChild> = header
                    .iter()
                    .map(|cell| Box::new(el("th", span_views(cell))) as NoteChild)
                    .collect();
                sections.push(Box::new(el(
                    "thead",
                    vec![Box::new(el("tr", cells)) as NoteChild],
                )));
            }
            let body: Vec<NoteChild> = rows
                .iter()
                .map(|row| {
                    let cells: Vec<NoteChild> = row
                        .iter()
                        .map(|cell| Box::new(el("td", span_views(cell))) as NoteChild)
                        .collect();
                    Box::new(el("tr", cells)) as NoteChild
                })
                .collect();
            sections.push(Box::new(el("tbody", body)));
            Box::new(el("table", sections))
        }
    }
}

/// Inline spans → inline genet views (text, `em`, `strong`, `code`, `a`, `br`).
fn span_views(spans: &[InlineSpan]) -> Vec<NoteChild> {
    spans.iter().map(span_view).collect()
}

fn span_view(span: &InlineSpan) -> NoteChild {
    match span {
        InlineSpan::Text(t) => Box::new(text(t.clone())),
        InlineSpan::Code(t) => Box::new(el("code", text(t.clone()))),
        InlineSpan::Emphasis(inner) => Box::new(el("em", span_views(inner))),
        InlineSpan::Strong(inner) => Box::new(el("strong", span_views(inner))),
        InlineSpan::Link {
            url, title, spans, ..
        } => {
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

    fn doc(blocks: Vec<Block>) -> EngineDocument {
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
        assert_eq!(
            block_tag(&Block::Heading {
                level: 1,
                spans: vec![]
            }),
            "h1"
        );
        assert_eq!(
            block_tag(&Block::Heading {
                level: 3,
                spans: vec![]
            }),
            "h3"
        );
        // Levels past 6 clamp to h6 (HTML has no h7+).
        assert_eq!(
            block_tag(&Block::Heading {
                level: 9,
                spans: vec![]
            }),
            "h6"
        );
        assert_eq!(block_tag(&Block::Paragraph { spans: vec![] }), "p");
        assert_eq!(
            block_tag(&Block::CodeBlock {
                language: None,
                text: String::new()
            }),
            "pre"
        );
        assert_eq!(block_tag(&Block::Quote { blocks: vec![] }), "blockquote");
        assert_eq!(
            block_tag(&Block::List {
                ordered: false,
                items: vec![]
            }),
            "ul"
        );
        assert_eq!(
            block_tag(&Block::List {
                ordered: true,
                items: vec![]
            }),
            "ol"
        );
        assert_eq!(block_tag(&Block::Rule), "hr");
        assert_eq!(
            block_tag(&Block::Image {
                url: String::new(),
                alt: String::new()
            }),
            "img"
        );
    }

    #[test]
    fn document_views_one_per_block() {
        let d = doc(vec![
            Block::Heading {
                level: 1,
                spans: vec![InlineSpan::Text("Mere".into())],
            },
            Block::Paragraph {
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
            Block::Heading {
                level: 2,
                spans: spans.clone(),
            },
            Block::Paragraph {
                spans: spans.clone(),
            },
            Block::CodeBlock {
                language: Some("rust".into()),
                text: "fn x() {}".into(),
            },
            Block::Quote {
                blocks: vec![Block::Paragraph {
                    spans: vec![InlineSpan::Text("q".into())],
                }],
            },
            Block::List {
                ordered: false,
                items: vec![vec![Block::Paragraph {
                    spans: vec![InlineSpan::Text("i".into())],
                }]],
            },
            Block::Image {
                url: "img".into(),
                alt: "a".into(),
            },
            Block::Preformatted { text: "pre".into() },
            Block::Rule,
            Block::FeedHeader {
                title: "f".into(),
                subtitle: Some("s".into()),
                summary: None,
                source_url: None,
            },
            Block::FeedEntry {
                title: "e".into(),
                date: None,
                summary: Some("s".into()),
                article_url: None,
                source_url: None,
            },
            Block::MetadataRow {
                label: "k".into(),
                value: "v".into(),
            },
            Block::Badge { text: "b".into() },
            Block::Table {
                alignments: vec![inker::TableAlignment::Left, inker::TableAlignment::Right],
                header: vec![
                    vec![InlineSpan::Text("h1".into())],
                    vec![InlineSpan::Text("h2".into())],
                ],
                rows: vec![vec![
                    vec![InlineSpan::Text("a".into())],
                    vec![InlineSpan::Text("b".into())],
                ]],
            },
        ]);
        assert_eq!(document_views(&d).len(), 13);
    }
}
