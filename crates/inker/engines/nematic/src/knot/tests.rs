/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::*;
use inker::InlineSpan;

fn render(body: &str) -> EngineDocument {
    KnotEngine::new()
        .render(&EngineInput::new("knot:test", body))
        .expect("render")
}

#[test]
fn engine_id_is_stable() {
    assert_eq!(KnotEngine::new().engine_id(), "nematic.knot");
}

#[test]
fn frontmatter_title_overrides_body_h1() {
    let body = "---\ntitle: Frontmatter Title\n---\n\n# Body Heading\n\nText.\n";
    let doc = render(body);
    assert_eq!(doc.title.as_deref(), Some("Frontmatter Title"));
}

#[test]
fn body_h1_is_used_when_frontmatter_lacks_title() {
    let body = "---\nsource: https://example.test/\n---\n\n# Body Heading\n";
    let doc = render(body);
    assert_eq!(doc.title.as_deref(), Some("Body Heading"));
}

#[test]
fn source_field_populates_canonical_uri() {
    let doc = render("---\nsource: https://example.test/post\n---\n\nBody.\n");
    assert_eq!(
        doc.provenance.canonical_uri.as_deref(),
        Some("https://example.test/post")
    );
    // source_kind should be the knot engine, not the inner markdown one.
    assert_eq!(doc.provenance.source_kind.as_deref(), Some("nematic.knot"));
}

#[test]
fn captured_field_populates_fetched_at() {
    let doc = render("---\ncaptured: 2026-05-08T14:23:00Z\n---\n\nBody.\n");
    assert_eq!(
        doc.provenance.fetched_at.as_deref(),
        Some("2026-05-08T14:23:00Z")
    );
}

#[test]
fn trust_field_parses_known_states() {
    for (input, expected) in [
        ("trusted", DocumentTrustState::Trusted),
        ("tofu", DocumentTrustState::Tofu),
        ("insecure", DocumentTrustState::Insecure),
        ("broken", DocumentTrustState::Broken),
        ("garbage", DocumentTrustState::Unknown),
        ("TOFU", DocumentTrustState::Tofu),
    ] {
        let doc = render(&format!("---\ntrust: {input}\n---\n\nBody.\n"));
        assert_eq!(doc.trust, expected, "trust value: {input}");
    }
}

#[test]
fn tags_array_emits_metadata_row() {
    let doc = render("---\ntags: [research, semantics, mere]\n---\n\nBody.\n");
    let tags_row = doc.blocks.iter().find_map(|b| match b {
        DocumentBlock::MetadataRow { label, value } if label == "tags" => Some(value.as_str()),
        _ => None,
    });
    assert_eq!(tags_row, Some("research, semantics, mere"));
}

#[test]
fn note_kind_emits_metadata_row() {
    let doc = render("---\nnote_kind: clip\n---\n\nBody.\n");
    let kind_row = doc.blocks.iter().find_map(|b| match b {
        DocumentBlock::MetadataRow { label, value } if label == "kind" => Some(value.as_str()),
        _ => None,
    });
    assert_eq!(kind_row, Some("clip"));
}

#[test]
fn metadata_blocks_appear_before_body_blocks() {
    let doc = render("---\nnote_kind: clip\ntags: [a, b]\n---\n\n# Body Heading\n\ntext\n");
    // The first two blocks should be MetadataRows; then the body's
    // markdown blocks follow.
    assert!(matches!(doc.blocks[0], DocumentBlock::MetadataRow { .. }));
    assert!(matches!(doc.blocks[1], DocumentBlock::MetadataRow { .. }));
    assert!(matches!(doc.blocks[2], DocumentBlock::Heading { .. }));
}

#[test]
fn no_frontmatter_parses_as_plain_markdown() {
    let doc = render("# Plain markdown\n\ntext.\n");
    assert_eq!(doc.title.as_deref(), Some("Plain markdown"));
    assert!(matches!(doc.blocks[0], DocumentBlock::Heading { .. }));
    // No metadata rows should be added when there's no frontmatter.
    assert!(
        !doc.blocks
            .iter()
            .any(|b| matches!(b, DocumentBlock::MetadataRow { .. }))
    );
}

#[test]
fn unterminated_frontmatter_treated_as_body() {
    // No closing `---`, so the whole thing is the body.
    let body = "---\ntitle: Forgot to close\n\n# Body\n\ntext\n";
    let doc = render(body);
    // Body parsing sees the literal `---` text in the markdown stream.
    // What matters is we didn't silently swallow it as frontmatter; the
    // title override should NOT have applied.
    assert_ne!(doc.title.as_deref(), Some("Forgot to close"));
}

#[test]
fn quoted_strings_have_quotes_stripped() {
    let doc = render("---\ntitle: \"Quoted Title\"\nnote_kind: 'clip'\n---\n\nBody.\n");
    assert_eq!(doc.title.as_deref(), Some("Quoted Title"));
    let kind_row = doc.blocks.iter().find_map(|b| match b {
        DocumentBlock::MetadataRow { label, value } if label == "kind" => Some(value.as_str()),
        _ => None,
    });
    assert_eq!(kind_row, Some("clip"));
}

#[test]
fn full_clip_round_trip_through_to_markdown() {
    let body = "---\ntitle: Note Title\nsource: https://example.test/article\ncaptured: 2026-05-08T14:23:00Z\ntrust: tofu\ntags: [research, semantics]\nnote_kind: clip\n---\n\n# Body heading\n\nFirst paragraph with *emphasis* and a [link](https://other.test/).\n";
    let doc = render(body);

    assert_eq!(doc.title.as_deref(), Some("Note Title"));
    assert_eq!(doc.trust, DocumentTrustState::Tofu);
    assert_eq!(
        doc.provenance.canonical_uri.as_deref(),
        Some("https://example.test/article")
    );

    // Round-trip via to_markdown — confirms the document model is
    // serialisable back into a CommonMark string.
    let md = doc.to_markdown();
    assert!(md.contains("**kind:** clip"));
    assert!(md.contains("**tags:** research, semantics"));
    assert!(md.contains("# Body heading"));
    assert!(md.contains("[link](https://other.test/)"));
}

#[test]
fn dispatches_through_inker_registry() {
    use inker::EngineRegistry;
    use inker::routing::{
        EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId,
    };

    let mut registry = EngineRegistry::new();
    registry.register(Box::new(KnotEngine::new()));
    let decision = EngineRouteDecision {
        engine_id: ENGINE_ID.to_string(),
        surface_contract: SurfaceContract {
            target: SurfaceTargetId::new("knot:1"),
            mode: SurfaceContractMode::CompositedTexture,
        },
    };
    let body = "---\ntitle: T\nnote_kind: thought\n---\n\nbody\n";
    let doc = registry
        .dispatch(&decision, &EngineInput::new("knot:1", body))
        .expect("dispatch");
    assert_eq!(doc.title.as_deref(), Some("T"));
}

// =============================================================================
// Polyglot fence expansion (post-process pass)
// =============================================================================

#[test]
fn gemtext_fence_expands_to_real_blocks() {
    let body = "Intro paragraph.\n\n```gemtext\n# Capsule\n\n=> gemini://capsule.test/ Visit\n```\n\nOutro paragraph.\n";
    let doc = render(body);

    // Among the blocks should be a Heading from the gemtext fence and a
    // Paragraph containing a Link to gemini://capsule.test/.
    let has_gemtext_heading = doc.blocks.iter().any(|b| {
        matches!(
            b,
            DocumentBlock::Heading { spans, .. }
                if spans.iter().any(|s|
                    matches!(s, InlineSpan::Text(t) if t == "Capsule")
                )
        )
    });
    assert!(has_gemtext_heading, "expected gemtext heading");

    // outgoing_links() should now surface the gemtext link automatically.
    assert!(
        doc.outgoing_links()
            .iter()
            .any(|u| *u == "gemini://capsule.test/")
    );
}

#[test]
fn unknown_fence_languages_pass_through_as_code_blocks() {
    let body = "```python\ndef hello(): print('x')\n```\n";
    let doc = render(body);
    let has_code_block = doc.blocks.iter().any(|b| {
        matches!(
            b,
            DocumentBlock::CodeBlock { language: Some(l), .. } if l == "python"
        )
    });
    assert!(has_code_block, "python fence should stay a code block");
}

#[test]
fn feed_entry_fence_expands_to_feed_entry_block() {
    let body = "```feed-entry\ntitle: Article I clipped\ndate: 2026-05-08\nurl: https://blog.test/post\nsource: https://blog.test/\nsummary: A summary.\n```\n";
    let doc = render(body);
    let entry = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            DocumentBlock::FeedEntry {
                title,
                date,
                summary,
                article_url,
                source_url,
            } => Some((
                title.as_str(),
                date.as_deref(),
                summary.as_deref(),
                article_url.as_deref(),
                source_url.as_deref(),
            )),
            _ => None,
        })
        .expect("expected FeedEntry block");
    assert_eq!(entry.0, "Article I clipped");
    assert_eq!(entry.1, Some("2026-05-08"));
    assert_eq!(entry.2, Some("A summary."));
    assert_eq!(entry.3, Some("https://blog.test/post"));
    assert_eq!(entry.4, Some("https://blog.test/"));
}

#[test]
fn metadata_row_fence_emits_one_row_per_line() {
    let body = "```metadata-row\nLogin: alice\nShell: /bin/zsh\nIdle: 0s\n```\n";
    let doc = render(body);
    let rows: Vec<(&str, &str)> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            DocumentBlock::MetadataRow { label, value } => Some((label.as_str(), value.as_str())),
            _ => None,
        })
        .collect();
    assert!(rows.contains(&("Login", "alice")));
    assert!(rows.contains(&("Shell", "/bin/zsh")));
    assert!(rows.contains(&("Idle", "0s")));
}

#[test]
fn badge_fence_emits_one_badge_per_line() {
    let body = "```badge\ntofu\nstale\n```\n";
    let doc = render(body);
    let badges: Vec<&str> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            DocumentBlock::Badge { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(badges, vec!["tofu", "stale"]);
}

#[test]
fn polyglot_knot_aggregates_protocol_blocks_with_provenance() {
    let body = "---\ntitle: Mixed Clip\nsource: https://aggregator.test/\ncaptured: 2026-05-08T14:23:00Z\ntrust: tofu\nnote_kind: clip\n---\n\nIntro from the user.\n\n```gemtext\n=> gemini://capsule.test/ Visit\n```\n\n```feed-entry\ntitle: Linked article\nurl: https://blog.test/post\n```\n";
    let doc = render(body);

    assert_eq!(doc.title.as_deref(), Some("Mixed Clip"));
    assert_eq!(doc.trust, DocumentTrustState::Tofu);
    let urls = doc.outgoing_links();
    assert!(urls.iter().any(|u| *u == "gemini://capsule.test/"));
    assert!(urls.iter().any(|u| *u == "https://blog.test/post"));
}

// =============================================================================
// to_knot round-trip
// =============================================================================

#[test]
fn to_knot_emits_feed_entry_as_fence() {
    let body = "```feed-entry\ntitle: A\nurl: https://x.test/a\n```\n";
    let doc = render(body);
    let knot = doc.to_knot();
    assert!(knot.contains("```feed-entry"));
    assert!(knot.contains("title: A"));
    assert!(knot.contains("url: https://x.test/a"));
}

#[test]
fn to_knot_emits_metadata_row_as_fence() {
    let body = "```metadata-row\nLogin: alice\n```\n";
    let doc = render(body);
    let knot = doc.to_knot();
    assert!(knot.contains("```metadata-row"));
    assert!(knot.contains("Login: alice"));
}

#[test]
fn to_knot_emits_badge_as_fence() {
    let body = "```badge\ntofu\n```\n";
    let doc = render(body);
    let knot = doc.to_knot();
    assert!(knot.contains("```badge"));
    assert!(knot.contains("tofu"));
}

#[test]
fn to_knot_renders_structural_blocks_as_markdown() {
    // Heading + paragraph should still be regular markdown in knot form.
    let body = "# Hi\n\nHello.\n";
    let doc = render(body);
    let knot = doc.to_knot();
    assert!(knot.contains("# Hi"));
    assert!(knot.contains("Hello."));
}

#[test]
fn full_round_trip_preserves_feed_entry_information() {
    // A feed-entry fence should round-trip: parse → block → re-render →
    // parse again → equivalent block.
    let body = "```feed-entry\ntitle: Round Trip\ndate: 2026-05-08\nurl: https://r.test/x\nsummary: Summary text.\n```\n";
    let doc1 = render(body);
    let knot = doc1.to_knot();
    let doc2 = render(&knot);

    let entries: Vec<_> = doc2
        .blocks
        .iter()
        .filter_map(|b| match b {
            DocumentBlock::FeedEntry {
                title,
                date,
                summary,
                article_url,
                ..
            } => Some((
                title.clone(),
                date.clone(),
                summary.clone(),
                article_url.clone(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(entries.len(), 1);
    let (title, date, summary, url) = &entries[0];
    assert_eq!(title, "Round Trip");
    assert_eq!(date.as_deref(), Some("2026-05-08"));
    assert_eq!(summary.as_deref(), Some("Summary text."));
    assert_eq!(url.as_deref(), Some("https://r.test/x"));
}

// =============================================================================
// Inline wikilinks + hashtags
// =============================================================================

#[test]
fn wikilink_rewrites_to_mere_node_url() {
    let doc = render("See [[my note]] for context.\n");
    let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let link = spans.iter().find_map(|s| match s {
        InlineSpan::Link { url, spans, .. } => Some((url.as_str(), spans.clone())),
        _ => None,
    });
    let (url, inner) = link.expect("expected wikilink");
    assert_eq!(url, "mere://node/my-note");
    // Display text preserves the original surface form.
    assert!(matches!(&inner[0], InlineSpan::Text(t) if t == "my note"));
    // outgoing_links() picks up the wikilink target.
    assert!(
        doc.outgoing_links()
            .iter()
            .any(|u| *u == "mere://node/my-note")
    );
}

#[test]
fn wikilink_lowercases_and_dashes_the_slug() {
    let doc = render("See [[Capital Words With Spaces]].\n");
    let url = doc.outgoing_links()[0];
    assert_eq!(url, "mere://node/capital-words-with-spaces");
}

#[test]
fn wikilinks_inside_existing_links_are_not_rewritten() {
    let doc = render("Read [the [[note]] here](https://x.test/).\n");
    // The outer markdown link should win; the [[note]] inside its display
    // text stays as plain text, not a nested wikilink.
    let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let links: Vec<&str> = spans
        .iter()
        .filter_map(|s| match s {
            InlineSpan::Link { url, .. } => Some(url.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(links, vec!["https://x.test/"]);
}

#[test]
fn unterminated_wikilink_stays_as_text() {
    let doc = render("This is [[unclosed and continues...\n");
    // No wikilink should have been emitted.
    let any_link = doc.blocks.iter().any(|b| match b {
        DocumentBlock::Paragraph { spans } => {
            spans.iter().any(|s| matches!(s, InlineSpan::Link { .. }))
        }
        _ => false,
    });
    assert!(!any_link, "unterminated [[ should not produce a link");
}

#[test]
fn hashtag_extracts_to_badge_after_paragraph() {
    let doc = render("This is a clip about #research and #semantics today.\n");
    let badges: Vec<&str> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            DocumentBlock::Badge { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(badges.contains(&"#research"));
    assert!(badges.contains(&"#semantics"));
}

#[test]
fn hashtag_is_removed_from_paragraph_text() {
    let doc = render("Note tagged #foo at end.\n");
    let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let combined: String = spans
        .iter()
        .filter_map(|s| match s {
            InlineSpan::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    // The "#foo" token should have been consumed; surrounding text remains.
    assert!(combined.contains("Note tagged"));
    assert!(combined.contains("at end."));
    assert!(!combined.contains("#foo"));
}

#[test]
fn hash_in_middle_of_word_is_not_a_hashtag() {
    let doc = render("CSS color #abc123 example.\n");
    let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let combined: String = spans
        .iter()
        .filter_map(|s| match s {
            InlineSpan::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    // Hash *was* preceded by a space, so it's parsed as a hashtag — this
    // is a known limitation. The test pins current behavior so we're aware.
    let has_badge = doc.blocks.iter().any(|b| match b {
        DocumentBlock::Badge { text } => text == "#abc123",
        _ => false,
    });
    assert!(
        has_badge,
        "current behavior: bare # at word boundary is hashtag"
    );
    assert!(!combined.contains("#abc123"));
}

#[test]
fn hashtags_in_headings_stay_as_text() {
    // Headings preserve hashtags inline; only paragraphs extract them.
    let doc = render("# Topic about #research\n\nBody.\n");
    let DocumentBlock::Heading { spans, .. } = &doc.blocks[0] else {
        panic!("expected heading");
    };
    let combined: String = spans
        .iter()
        .filter_map(|s| match s {
            InlineSpan::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert!(combined.contains("#research"), "got: {combined}");
}

#[test]
fn polyglot_knot_with_wikilinks_and_hashtags() {
    let body = "---\ntitle: Mixed\n---\n\nSee [[research-log]] and the entry below #research.\n\n```feed-entry\ntitle: Article\nurl: https://x.test/\n```\n";
    let doc = render(body);

    let urls = doc.outgoing_links();
    assert!(urls.iter().any(|u| *u == "mere://node/research-log"));
    assert!(urls.iter().any(|u| *u == "https://x.test/"));

    let has_badge = doc.blocks.iter().any(|b| match b {
        DocumentBlock::Badge { text } => text == "#research",
        _ => false,
    });
    assert!(has_badge);
}

// =============================================================================
// build_clip_knot helper
// =============================================================================

#[test]
fn build_clip_knot_assembles_frontmatter_plus_blocks() {
    let mut prov = DocumentProvenance::default();
    prov.canonical_uri = Some("https://blog.test/article".to_string());
    prov.source_kind = Some("nematic.markdown".to_string());

    let blocks = vec![DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Text("A clipped paragraph.".to_string())],
    }];

    let knot = build_clip_knot(&blocks, &prov, DocumentTrustState::Tofu, Some("clip"));
    assert!(knot.starts_with("---\n"));
    assert!(knot.contains("source: https://blog.test/article"));
    assert!(knot.contains("trust: tofu"));
    assert!(knot.contains("note_kind: clip"));
    assert!(knot.contains("source_label: nematic.markdown"));
    assert!(knot.contains("A clipped paragraph."));

    // Round-trip: feeding the knot back through the engine should produce a
    // document with the right provenance and the paragraph intact.
    let doc = render(&knot);
    assert_eq!(
        doc.provenance.canonical_uri.as_deref(),
        Some("https://blog.test/article")
    );
    assert_eq!(doc.trust, DocumentTrustState::Tofu);
}

#[test]
fn build_clip_knot_does_not_double_emit_frontmatter() {
    // Regression: EngineDocument::to_knot now emits frontmatter when the
    // document has provenance / trust set. build_clip_knot must use the
    // body-only render path so its own clip-aware frontmatter isn't
    // duplicated.
    let mut prov = DocumentProvenance::default();
    prov.canonical_uri = Some("https://x.test/".to_string());
    let blocks = vec![DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Text("Body".to_string())],
    }];
    let knot = build_clip_knot(&blocks, &prov, DocumentTrustState::Tofu, None);
    // Exactly two `---` lines (opener + closer of the one frontmatter
    // block), then body. A double-frontmatter regression would produce
    // four or more.
    let dash_lines = knot.lines().filter(|l| *l == "---").count();
    assert_eq!(
        dash_lines, 2,
        "expected one frontmatter block, got:\n{knot}"
    );
}

#[test]
fn build_clip_knot_with_block_provenance_emits_block_sources_list() {
    use inker::{BlockProvenance, BlockProvenanceMap};

    let mut doc_prov = DocumentProvenance::default();
    doc_prov.canonical_uri = Some("https://blog.test/composite".to_string());

    let mut other_prov = DocumentProvenance::default();
    other_prov.canonical_uri = Some("gopher://other.test/0/file".to_string());

    let mut map = BlockProvenanceMap::new();
    // Block 0 came from the document's own source — should NOT appear
    // in block_sources.
    map.insert(0, BlockProvenance::from_document(doc_prov.clone()));
    // Block 1 came from a different source with an anchor.
    map.insert(
        1,
        BlockProvenance::from_document(other_prov.clone()).with_anchor("L42-L58"),
    );

    let blocks = vec![
        DocumentBlock::Paragraph {
            spans: vec![InlineSpan::Text("From the composite source.".into())],
        },
        DocumentBlock::Paragraph {
            spans: vec![InlineSpan::Text("From the gopher source.".into())],
        },
    ];

    let knot = build_clip_knot_with_block_provenance(
        &blocks,
        &doc_prov,
        DocumentTrustState::Tofu,
        None,
        &map,
    );

    assert!(knot.contains("block_sources: ["));
    assert!(knot.contains("\"1|gopher://other.test/0/file|L42-L58\""));
    // Block 0 matched the document source with no anchor — must not
    // appear in the list.
    assert!(
        !knot.contains("\"0|"),
        "block 0 should not appear in block_sources:\n{knot}"
    );
}

#[test]
fn build_clip_knot_with_block_provenance_emits_no_list_when_all_match_document() {
    use inker::{BlockProvenance, BlockProvenanceMap};

    let mut doc_prov = DocumentProvenance::default();
    doc_prov.canonical_uri = Some("https://blog.test/".to_string());

    let mut map = BlockProvenanceMap::new();
    map.insert(0, BlockProvenance::from_document(doc_prov.clone()));
    map.insert(2, BlockProvenance::from_document(doc_prov.clone()));

    let blocks = vec![DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Text("body".into())],
    }];
    let knot = build_clip_knot_with_block_provenance(
        &blocks,
        &doc_prov,
        DocumentTrustState::Unknown,
        None,
        &map,
    );
    assert!(
        !knot.contains("block_sources:"),
        "expected no block_sources list when every override matches the document source:\n{knot}"
    );
}

#[test]
fn build_clip_knot_with_block_provenance_sorts_entries_by_index() {
    use inker::{BlockProvenance, BlockProvenanceMap};

    let doc_prov = DocumentProvenance::default();
    let mut a = DocumentProvenance::default();
    a.canonical_uri = Some("gemini://a.test/".to_string());
    let mut b = DocumentProvenance::default();
    b.canonical_uri = Some("gemini://b.test/".to_string());
    let mut c = DocumentProvenance::default();
    c.canonical_uri = Some("gemini://c.test/".to_string());

    let mut map = BlockProvenanceMap::new();
    // Insert in scrambled order; output must still be sorted.
    map.insert(5, BlockProvenance::from_document(c));
    map.insert(1, BlockProvenance::from_document(a));
    map.insert(3, BlockProvenance::from_document(b));

    let blocks = vec![DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Text("x".into())],
    }];
    let knot = build_clip_knot_with_block_provenance(
        &blocks,
        &doc_prov,
        DocumentTrustState::Unknown,
        None,
        &map,
    );
    let a_pos = knot.find("\"1|gemini://a.test/\"").expect("a entry");
    let b_pos = knot.find("\"3|gemini://b.test/\"").expect("b entry");
    let c_pos = knot.find("\"5|gemini://c.test/\"").expect("c entry");
    assert!(
        a_pos < b_pos && b_pos < c_pos,
        "entries not sorted by index"
    );
}
