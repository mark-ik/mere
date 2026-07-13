/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Card rendering tests.

use genet_layout::NoImageLoader;

use super::*;

fn heads_with(doc: &EngineDocument, text: &str) -> bool {
    matches!(
        doc.blocks.first(),
        Some(Block::Heading { spans, .. })
            if matches!(spans.first(), Some(InlineSpan::Text(t)) if t == text)
    )
}

fn body_text(doc: &EngineDocument) -> String {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph { spans } | Block::Heading { spans, .. } => {
                Some(spans.iter().filter_map(|s| match s {
                    InlineSpan::Text(t) => Some(t.as_str()),
                    _ => None,
                }))
            }
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

fn nematic_registry() -> EngineRegistry {
    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    registry
}

fn markdown_document_and_packet(body: &str) -> (EngineDocument, DocumentRenderPacket) {
    let registry = nematic_registry();
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/markdown".into()),
        body: body.into(),
    });
    let doc = engine_document_for(
        "https://example.test/",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
    )
    .expect("markdown content routes through a document engine");
    let RenderedContent::Document { packet, .. } = render_content(
        "https://example.test/",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0,
        360,
        &card_sheet(card_vocabulary()),
    ) else {
        panic!("markdown routes to the document lane");
    };
    (doc, packet)
}

#[test]
fn welcome_document_leads_with_a_heading() {
    let doc = content_document("mere://welcome", None);
    assert_eq!(doc.address, "mere://welcome");
    assert!(matches!(
        doc.blocks.first(),
        Some(Block::Heading { level: 1, .. })
    ));
}

#[test]
fn fetch_states_render_distinctly() {
    let url = "https://example.com";
    assert!(heads_with(
        &content_document(url, Some(&ContentState::Loading)),
        url
    ));
    assert!(body_text(&content_document(url, Some(&ContentState::Loading))).contains("Fetching"));

    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/plain".into()),
        body: "First paragraph.\n\nSecond paragraph.".into(),
    });
    let doc = content_document(url, Some(&ready));
    let text = body_text(&doc);
    assert!(text.contains("First paragraph."), "fetched body renders");
    assert!(
        text.contains("Second paragraph."),
        "blank lines split paragraphs"
    );

    let failed = ContentState::Failed("HTTP 404".into());
    assert!(body_text(&content_document(url, Some(&failed))).contains("404"));
}

#[test]
fn card_scene_lowers_text_to_glyph_runs() {
    let doc = content_document("mere://welcome", None);
    let (scene, _, _) = render_card_scene(&doc, 420, 360);
    let glyph_runs = scene
        .ops
        .iter()
        .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
        .count();
    assert!(
        glyph_runs >= 1,
        "the welcome card lowers its text to glyph runs"
    );
}

#[test]
fn card_rect_fits_the_content_band_and_vanishes_when_tiny() {
    let (x0, y0, x1, y1, cw, ch) =
        card_rect([0.0, 64.0, 1024.0, 600.0]).expect("a card fits a normal window");
    assert!(x1 > x0 && y1 > y0, "non-empty rect");
    assert!(x1 <= 1024.0 && y1 <= 600.0, "within the window");
    assert!(y0 >= 64.0, "below the toolbar band");
    assert!(cw >= 1 && ch >= 1);
    assert!(
        card_rect([0.0, 64.0, 120.0, 120.0]).is_none(),
        "no card when the band is too small"
    );
}

#[test]
fn route_document_engine_maps_known_types() {
    let mut reg = inker::EngineRegistry::new();
    for engine in nematic::engines() {
        reg.register(engine);
    }
    let policy = inker::EngineRoutePolicy::default();
    // Content-type rules win over the scheme, so the url scheme doesn't matter
    // for these; pass an https url to exercise the scheme fallback in the last case.
    let route =
        |ct: Option<&str>| route_document_engine("https://example.test/x", ct, &reg, &policy);
    assert_eq!(
        route(Some("text/markdown")).as_str(),
        nematic::ENGINE_MARKDOWN
    );
    assert_eq!(
        route(Some("text/plain; charset=utf-8")).as_str(),
        nematic::ENGINE_TEXT
    );
    assert_eq!(route(Some("text/gemini")).as_str(), nematic::ENGINE_GEMTEXT);
    assert_eq!(
        route(Some("application/x-nex")).as_str(),
        nematic::ENGINE_NEX
    );
    assert_eq!(
        route(Some("application/x-guppy")).as_str(),
        nematic::ENGINE_GUPPY
    );
    assert_eq!(
        route(Some("application/x-titan")).as_str(),
        nematic::ENGINE_TITAN
    );
    assert_eq!(
        route(Some("message/x-misfin")).as_str(),
        nematic::ENGINE_MISFIN
    );
    assert_eq!(
        route(Some("application/gopher-menu")).as_str(),
        nematic::ENGINE_GOPHER
    );
    // HTML routes to the genet web lane by content-type, regardless of scheme.
    assert_eq!(
        route(Some("text/html")).as_str(),
        inker::routing::ENGINE_GENET_WEB
    );
    assert_eq!(
        route(Some("application/xhtml+xml")).as_str(),
        inker::routing::ENGINE_GENET_WEB
    );
    // No content-type over an https url falls to the scheme rule (genet).
    assert_eq!(route(None).as_str(), inker::routing::ENGINE_GENET_WEB);
}

fn glyph_runs(scene: &netrender::Scene) -> usize {
    scene
        .ops
        .iter()
        .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
        .count()
}

#[test]
fn html_with_an_unloaded_image_stays_scene_consistent() {
    // Repro for the crash Mark hit (genet -> scry -> genet, open a new node):
    // the dormant-node snapshot renders HTML through an empty image loader
    // (render.rs), so an `<img>` whose bytes are not cached has no source. The
    // lowered scene must NOT carry a `SceneImage` op whose key is absent from
    // `image_sources`, or netrender's rasterizer `.expect()`s and crashes the app
    // ("SceneImage references unknown ImageKey").
    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: r#"<html><body><p>before</p>
            <img src="https://example.com/missing.png" width="80" height="60">
            <p>after</p></body></html>"#
            .into(),
    });
    let RenderedContent::Html { scene, .. } = render_content(
        "https://example.com/",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0,
        360,
        &card_sheet(card_vocabulary()),
    ) else {
        panic!("text/html routes to the genet HTML lane");
    };
    let missing = unsourced_image_keys(&scene);
    assert!(
        missing.is_empty(),
        "an unloaded <img> left a SceneImage with no source (rasterizer would \
         panic): {missing:?}"
    );
}

#[test]
fn html_with_an_undecodable_image_stays_scene_consistent() {
    // The decode-failure case: the loader returns bytes, but they are not a valid
    // image. If genet emits a SceneImage op without sourcing it (because the
    // decode failed), the rasterizer panics. The scene must stay consistent.
    struct GarbageLoader;
    impl genet_layout::ImageLoader for GarbageLoader {
        fn load(&self, _url: &str) -> Option<Vec<u8>> {
            Some(vec![0xDE, 0xAD, 0xBE, 0xEF, 0, 1, 2, 3, 4, 5]) // not a valid image
        }
    }
    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: r#"<html><body><p>before</p>
            <img src="https://example.com/broken.png" width="80" height="60">
            <p>after</p></body></html>"#
            .into(),
    });
    let RenderedContent::Html { scene, .. } = render_content(
        "https://example.com/",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &GarbageLoader,
        420,
        360,
        0,
        360,
        &card_sheet(card_vocabulary()),
    ) else {
        panic!("text/html routes to the genet HTML lane");
    };
    let missing = unsourced_image_keys(&scene);
    assert!(
        missing.is_empty(),
        "an undecodable <img> left a SceneImage with no source (rasterizer would \
         panic): {missing:?}"
    );
}

#[test]
fn markdown_routes_through_nematic_to_glyph_runs() {
    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/markdown".into()),
        body: "# Heading\n\nA paragraph.".into(),
    });
    let (scene, _, _) = render_content_scene(
        "https://example.com",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0.0,
        &card_sheet(card_vocabulary()),
    );
    assert!(
        glyph_runs(&scene) >= 1,
        "markdown renders text via the nematic document lane"
    );
}

#[test]
fn document_lane_surfaces_link_hit_regions() {
    // The document lane (here markdown) lays hit-testable link regions onto the
    // retained packet; the host queries them via `DocumentRenderPacket::link_at`.
    // (Inline-link nav; Phase 2 query API.)
    let mut registry = EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/markdown".into()),
        body: "See [the spec](https://example.test/spec) for details.".into(),
    });
    let RenderedContent::Document { packet, .. } = render_content(
        "https://example.test/",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0,
        360,
        &card_sheet(card_vocabulary()),
    ) else {
        panic!("markdown routes to the document lane");
    };
    // The packet carries the link as a positive-area interaction region...
    let region = packet
        .interactions
        .iter()
        .find(|r| matches!(&r.kind, InteractionKind::Link { url } if url == "https://example.test/spec"))
        .expect("the markdown link is laid out as an interaction region");
    let b = region.bounds;
    assert!(
        b.size.width > 0.0 && b.size.height > 0.0,
        "positive-area link bounds: {b:?}"
    );
    // ...and link_at at its center resolves to the URL.
    assert_eq!(
        packet.link_at(
            b.origin.x + b.size.width * 0.5,
            b.origin.y + b.size.height * 0.5
        ),
        Some("https://example.test/spec"),
        "link_at resolves the link at its center"
    );
}

#[test]
fn document_lane_find_counts_repeated_hits_per_block() {
    let (doc, packet) = markdown_document_and_packet("alpha alpha\n\nalpha");
    let matches = find_document_content(&doc, &packet, "alpha");
    assert_eq!(matches.len(), 3, "three textual hits stay visible to find");
    assert!(!matches[0].is_empty(), "matches carry block rects");
    assert_eq!(
        matches[0], matches[1],
        "two hits in one paragraph share that paragraph's block rects"
    );
    assert_ne!(
        matches[1], matches[2],
        "a later paragraph resolves to a different block rect set"
    );
}

#[test]
fn document_lane_selection_spans_multiple_blocks() {
    let (doc, packet) = markdown_document_and_packet("first para\n\nsecond para");
    let selection = select_document_content(&doc, &packet, 0, 1).expect("adjacent blocks select");
    assert!(
        selection.rects.len() >= 2,
        "multi-block selection carries rects for both paragraphs"
    );
    assert_eq!(selection.text.trim(), "first para\n\nsecond para");
}

#[test]
fn document_lane_selection_prefers_nested_block_rects_over_group_bounds() {
    let doc = EngineDocument {
        address: "https://example.test/quote".into(),
        title: None,
        content_type: "text/plain".into(),
        lang: None,
        provenance: Default::default(),
        trust: Default::default(),
        diagnostics: Vec::new(),
        blocks: vec![Block::Quote {
            blocks: vec![Block::Paragraph {
                spans: vec![InlineSpan::Text("quoted line".into())],
            }],
        }],
    };
    let RenderedContent::Document { packet, .. } =
        layout_document_content(&doc, 420, 360, &card_sheet(card_vocabulary()))
    else {
        panic!("manual document uses the document lane");
    };
    let selection = select_document_content(&doc, &packet, 0, 0).expect("quoted child selects");
    assert_eq!(
        selection.rects.len(),
        1,
        "selection should highlight the quoted paragraph, not both the group and child"
    );
    assert_eq!(selection.text.trim(), "quoted line");
}

#[test]
fn html_routes_through_genet_to_glyph_runs() {
    // The genet lane needs no document engine registered.
    let registry = EngineRegistry::new();
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: "<h1>Hello</h1><p>World</p>".into(),
    });
    let (scene, _, _) = render_content_scene(
        "https://example.com",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0.0,
        &card_sheet(card_vocabulary()),
    );
    assert!(
        glyph_runs(&scene) >= 1,
        "HTML renders text via the genet lane"
    );
}

#[test]
fn html_lane_harvests_link_hit_regions() {
    // The HTML/genet lane has no retained packet, so it ships a parallel
    // `LinkHit` table harvested off the fragment plane; the host hit-tests a
    // click against it via `Constellation::link_at`'s HTML branch. (Inline-link
    // nav; Phase 5 lane parity.)
    let registry = EngineRegistry::new();
    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: "<html><body><p>see <a href=\"https://example.test/spec\">the spec</a> now</p></body></html>"
            .into(),
    });
    let RenderedContent::Html { links, .. } = render_content(
        "https://example.test/",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0,
        360,
        &card_sheet(card_vocabulary()),
    ) else {
        panic!("text/html routes to the genet HTML lane");
    };
    let hit = links
        .iter()
        .find(|l| l.url == "https://example.test/spec")
        .expect("the inline <a href> is harvested into a LinkHit");
    let [x0, y0, x1, y1] = hit.rect;
    assert!(
        x1 > x0 && y1 > y0,
        "positive-area link rect: {:?}",
        hit.rect
    );
    // The link follows "see ", so it starts a little right of the content origin
    // (document-px, pre-scroll — the space the host adds the card's scroll into).
    assert!(x0 > 0.0, "link starts after the leading text, got x0={x0}");
}

#[test]
fn html_lane_applies_page_supplied_style() {
    let registry = EngineRegistry::new();
    // A page `<style>` that hides the only paragraph suppresses its text —
    // proof the page's own CSS reaches the cascade (this rule is not in
    // HTML_SHEET, which only sets `p`'s font-size).
    let hidden = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: "<style>p { display: none; }</style><p>Hidden by the page.</p>".into(),
    });
    let (scene, _, _) = render_content_scene(
        "https://example.com",
        Some(&hidden),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0.0,
        &card_sheet(card_vocabulary()),
    );
    assert_eq!(
        glyph_runs(&scene),
        0,
        "a page `display:none` style suppresses the paragraph"
    );

    // Without the hiding style the same paragraph renders.
    let shown = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: "<p>Visible.</p>".into(),
    });
    let (scene, _, _) = render_content_scene(
        "https://example.com",
        Some(&shown),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &NoImageLoader,
        420,
        360,
        0.0,
        &card_sheet(card_vocabulary()),
    );
    assert!(
        glyph_runs(&scene) >= 1,
        "without a hiding style the paragraph renders"
    );
}

#[test]
fn html_lane_applies_head_linked_stylesheet_through_the_loader() {
    use std::cell::RefCell;

    use crate::resources::{ResourceLoader, ResourceStore};

    let registry = EngineRegistry::new();
    // A `<head>` `<link>` to a sheet that hides the paragraph, with its bytes
    // already cached. The link must be seen despite living in `<head>` (full
    // document parse) and apply through the loader seam.
    let store = RefCell::new(ResourceStore::default());
    store.borrow_mut().insert(
        "https://example.com/hide.css".into(),
        b"p { display: none; }".to_vec(),
    );
    let wanted = RefCell::new(Vec::new());
    let loader = ResourceLoader::new(&store, "https://example.com/page.html", &wanted);

    let ready = ContentState::Ready(Fetched {
        content_type: Some("text/html".into()),
        body: "<head><link rel=\"stylesheet\" href=\"hide.css\"></head>\
               <body><p>Hidden by the linked sheet.</p></body>"
            .into(),
    });
    let (scene, _, _) = render_content_scene(
        "https://example.com/page.html",
        Some(&ready),
        &registry,
        &inker::EngineRoutePolicy::default(),
        &loader,
        420,
        360,
        0.0,
        &card_sheet(card_vocabulary()),
    );
    assert_eq!(
        glyph_runs(&scene),
        0,
        "a head <link> stylesheet fetched through the loader hides the paragraph",
    );
}
