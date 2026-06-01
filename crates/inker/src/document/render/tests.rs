/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::super::{
    DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan,
};

fn doc(blocks: Vec<DocumentBlock>) -> EngineDocument {
    EngineDocument {
        address: "doc:1".into(),
        title: None,
        content_type: "text/plain".into(),
        lang: None,
        provenance: DocumentProvenance::default(),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks,
    }
}

#[test]
fn to_markdown_renders_heading_paragraph_and_link() {
    let document = doc(vec![
        DocumentBlock::Heading {
            level: 1,
            spans: vec![InlineSpan::Text("Hello".into())],
        },
        DocumentBlock::Paragraph {
            spans: vec![
                InlineSpan::Text("see ".into()),
                InlineSpan::Link {
                    url: "https://x.test/".into(),
                    title: None,
                    spans: vec![InlineSpan::Text("docs".into())],
                    predicate: None,
                },
            ],
        },
    ]);
    let md = document.to_markdown();
    assert!(md.contains("# Hello"));
    assert!(md.contains("[docs](https://x.test/)"));
}

#[test]
fn to_gemini_renders_paragraph_with_link_lines() {
    let document = doc(vec![DocumentBlock::Paragraph {
        spans: vec![
            InlineSpan::Text("see ".into()),
            InlineSpan::Link {
                url: "https://x.test/".into(),
                title: None,
                spans: vec![InlineSpan::Text("docs".into())],
                predicate: None,
            },
            InlineSpan::Text(" please".into()),
        ],
    }]);
    let gem = document.to_gemini();
    assert!(gem.contains("see docs please\n"));
    assert!(gem.contains("=> https://x.test/ docs\n"));
}

#[test]
fn to_markdown_renders_feed_entry_as_h2_block() {
    let document = doc(vec![DocumentBlock::FeedEntry {
        title: "Title".into(),
        date: Some("2026-05-08".into()),
        summary: Some("Summary text.".into()),
        article_url: Some("https://feed.test/x".into()),
        source_url: None,
    }]);
    let md = document.to_markdown();
    assert!(md.contains("## Title"));
    assert!(md.contains("*2026-05-08*"));
    assert!(md.contains("Summary text."));
    assert!(md.contains("[Open article](https://feed.test/x)"));
}

#[test]
fn to_gemini_renders_metadata_row_as_label_value() {
    let document = doc(vec![DocumentBlock::MetadataRow {
        label: "Login".into(),
        value: "alice".into(),
    }]);
    assert_eq!(document.to_gemini(), "Login: alice\n");
}

// -----------------------------------------------------------------
// to_knot frontmatter round-trip
// -----------------------------------------------------------------

fn doc_with_metadata(
    title: Option<&str>,
    provenance: DocumentProvenance,
    trust: DocumentTrustState,
) -> EngineDocument {
    EngineDocument {
        address: "doc:1".into(),
        title: title.map(String::from),
        content_type: "text/x-knot".into(),
        lang: None,
        provenance,
        trust,
        diagnostics: Vec::new(),
        blocks: vec![DocumentBlock::Paragraph {
            spans: vec![InlineSpan::Text("Body.".into())],
        }],
    }
}

#[test]
fn to_knot_omits_frontmatter_when_no_metadata() {
    let document = doc(vec![DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Text("Just body.".into())],
    }]);
    let knot = document.to_knot();
    assert!(
        !knot.starts_with("---"),
        "expected no frontmatter; got: {knot:?}"
    );
}

#[test]
fn to_knot_emits_title_in_frontmatter() {
    let document = doc_with_metadata(
        Some("My Title"),
        DocumentProvenance::default(),
        DocumentTrustState::Unknown,
    );
    let knot = document.to_knot();
    assert!(knot.starts_with("---\n"));
    assert!(knot.contains("title: My Title"));
    assert!(knot.contains("Body."));
}

#[test]
fn to_knot_emits_provenance_fields_in_frontmatter() {
    let provenance = DocumentProvenance {
        source_kind: Some("nematic.knot".into()),
        canonical_uri: Some("https://example.test/article".into()),
        fetched_at: Some("2026-05-10T14:23:00Z".into()),
        source_label: Some("Example Blog".into()),
    };
    let document = doc_with_metadata(None, provenance, DocumentTrustState::Tofu);
    let knot = document.to_knot();
    assert!(knot.contains("source: https://example.test/article"));
    assert!(knot.contains("captured: 2026-05-10T14:23:00Z"));
    assert!(knot.contains("source_label: Example Blog"));
    assert!(knot.contains("trust: tofu"));
}

#[test]
fn to_knot_omits_trust_when_unknown() {
    let document = doc_with_metadata(
        Some("Title"),
        DocumentProvenance::default(),
        DocumentTrustState::Unknown,
    );
    let knot = document.to_knot();
    assert!(!knot.contains("trust:"));
}

#[test]
fn to_knot_emits_each_trust_state_correctly() {
    for (state, expected) in [
        (DocumentTrustState::Trusted, "trust: trusted"),
        (DocumentTrustState::Tofu, "trust: tofu"),
        (DocumentTrustState::Insecure, "trust: insecure"),
        (DocumentTrustState::Broken, "trust: broken"),
    ] {
        let document = doc_with_metadata(None, DocumentProvenance::default(), state);
        assert!(document.to_knot().contains(expected));
    }
}
