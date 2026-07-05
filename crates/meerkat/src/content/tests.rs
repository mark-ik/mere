/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Content actor tests.

use std::sync::Arc;

use super::*;
use crate::fetch::Fetched;

fn noop_wake() -> Wake {
    Arc::new(|| {})
}

fn show(url: &str, content_type: &str, body: &str) -> ContentCommand {
    ContentCommand::Show {
        url: url.to_string(),
        state: Some(ContentState::Ready(Fetched {
            content_type: Some(content_type.to_string()),
            body: body.to_string(),
        })),
        engine: inker::routing::ENGINE_SERVAL_WEB.to_string(),
        viewport: (420, 360),
        nav: NavGeneration::default(),
        viewport_gen: ViewportGeneration::default(),
        sheet: DocumentStyleSheet::default(),
    }
}

fn glyph_runs(scene: &Scene) -> usize {
    scene
        .ops
        .iter()
        .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
        .count()
}

#[test]
fn show_renders_a_scene_off_thread() {
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(show(
        "https://example.com/",
        "text/html",
        "<h1>Hi</h1><p>There</p>",
    ));
    handle.join();

    let scene = updates
        .iter()
        .find_map(|u| match u {
            ContentUpdate::Scene { scene, .. } => Some(scene),
            _ => None,
        })
        .expect("a scene update");
    assert!(
        glyph_runs(&scene) >= 1,
        "the off-thread render lowered text to glyph runs"
    );
}

#[test]
fn transfer_transport_decodes_actor_scene_update() {
    let (handle, updates) = spawn_content_transfer(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(show(
        "https://example.com/",
        "text/html",
        "<h1>Hi</h1><p>There</p>",
    ));
    handle.join();

    let mut decoder = SceneTransferDecoder::default();
    let scene = updates
        .iter()
        .find_map(|buffer| {
            match ContentUpdate::from_transfer_buffer(buffer.as_bytes(), &mut decoder)
                .expect("decode transferred update")
            {
                ContentUpdate::Scene { scene, .. } => Some(scene),
                _ => None,
            }
        })
        .expect("a transferred scene update");
    assert!(
        glyph_runs(&scene) >= 1,
        "the transfer channel decoded the actor scene update"
    );
}

#[test]
fn materialize_links_emits_a_hyperlink_contribution() {
    // An explicit invoke (not gated by auto-ingest): the open page's outbound
    // links become graph nodes joined by Semantic:Hyperlink edges. (Relational
    // browse V1.)
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(show(
        "https://seed.test/page",
        "text/html",
        "<a href='/x'>X</a><a href='https://other.test/y'>Y</a>",
    ));
    handle.command(ContentCommand::MaterializeLinks {
        viewport_gen: ViewportGeneration::default(),
    });
    handle.join();

    let materialized = updates.iter().any(|u| match u {
        ContentUpdate::Contribution { contributions } => contributions.iter().any(|c| {
            c.edges.iter().any(|e| {
                e.subject == "https://seed.test/page"
                    && e.predicate == "https://mere.computer/ns/rel#hyperlink"
            }) && c.nodes.iter().any(|n| n.id == "https://other.test/y")
        }),
        _ => false,
    });
    assert!(
        materialized,
        "MaterializeLinks emitted the Hyperlink-edged neighborhood"
    );
}

#[test]
fn show_harvests_embedded_jsonld_into_a_contribution() {
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        true,
    );
    handle.command(show(
        "https://example.com/",
        "text/html",
        r#"<script type="application/ld+json">
           {"@context":{"name":"https://schema.org/name"},"@id":"mere://z","name":"Z"}
           </script><p>body</p>"#,
    ));
    handle.join();

    let harvested = updates.iter().any(|u| match u {
        ContentUpdate::Contribution { contributions } => contributions
            .iter()
            .any(|c| c.nodes.iter().any(|n| n.id == "mere://z")),
        _ => false,
    });
    assert!(harvested, "embedded JSON-LD harvested into a Contribution");
}

/// The overlay-slot host seam end to end through the content actor (overlay-roots
/// P1): a host-laid-out satellite paint list registered on a live page via
/// `SetOverlay` composites engine-side into the emitted band, the page does **not**
/// reflow around it (its own text is byte-stable), and `ClearOverlay` restores the
/// exact baseline band. This is the meerkat integration proof; the geometric
/// survival half — an overlay re-deriving its position across scroll bands and an
/// anchor-moving mutation — is proven headless engine-side by serval-layout's
/// `overlay_slot_tracks_its_anchor_across_bands` (the retained layout the actor
/// re-emits per band is the same one those tests exercise).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn overlay_slot_composites_over_a_live_page_without_reflow() {
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(show(
        "https://example.com/",
        "text/html",
        "<h1>Hi</h1><p>There</p>",
    ));
    // A satellite "chip": a single distinctly-coloured fill, laid out host-side
    // (here built directly — the actor is oblivious to how the host produced it).
    let mut chip = serval_layout::ServalPaintList::new(paint_list_api::DeviceIntSize::new(40, 20));
    chip.push_fill(
        0.0,
        0.0,
        40.0,
        20.0,
        paint_list_api::ColorF {
            r: 1.0,
            g: 0.55,
            b: 0.10,
            a: 0.9,
        },
    );
    handle.command(ContentCommand::SetOverlay {
        name: "counter".to_string(),
        anchor: OverlayAnchor::Root,
        content: chip,
        viewport_gen: ViewportGeneration::default(),
    });
    handle.command(ContentCommand::ClearOverlay {
        name: "counter".to_string(),
        viewport_gen: ViewportGeneration::default(),
    });
    handle.join();

    let scenes: Vec<Scene> = updates
        .iter()
        .filter_map(|u| match u {
            ContentUpdate::Scene { scene, .. } => Some(scene),
            _ => None,
        })
        .collect();
    // Show, SetOverlay, ClearOverlay each ship a scene (the arms bust the band
    // fingerprint so the overlay change is not deduped away).
    assert!(
        scenes.len() >= 3,
        "Show + SetOverlay + ClearOverlay each emit a scene (got {})",
        scenes.len()
    );
    let base = &scenes[0];
    let with_overlay = &scenes[1];
    let cleared = scenes.last().unwrap();
    // No reflow leak: the page's own text lowers to the same glyph runs with the
    // overlay registered, with it re-emitted, and after it clears.
    assert_eq!(
        glyph_runs(base),
        glyph_runs(with_overlay),
        "registering the overlay did not reflow the page's text",
    );
    assert_eq!(
        glyph_runs(base),
        glyph_runs(cleared),
        "clearing the overlay left the page's text unchanged",
    );
    // The overlay composites: the satellite adds paint ops in-band, and clearing
    // returns the band to exactly its pre-overlay op count (top-layer append +
    // removal, no residue).
    assert!(
        with_overlay.ops.len() > base.ops.len(),
        "the overlay composited extra paint ops in-band ({} vs {})",
        with_overlay.ops.len(),
        base.ops.len(),
    );
    assert_eq!(
        cleared.ops.len(),
        base.ops.len(),
        "clearing the overlay restored the exact baseline band",
    );
}

/// A `<body>` whose only text is injected by an inline `<script>`. Proves the
/// scripted render rung runs page JS end to end through the content actor: the
/// mutated DOM renders glyph runs the markup alone would not. (Render ladder 2a.)
#[cfg(feature = "scripted")]
const INLINE_SCRIPT_PAGE: &str = "<body><script>\
    var p = document.createElement('p');\
    p.appendChild(document.createTextNode('injected by JS'));\
    document.body.appendChild(p);\
    </script></body>";

#[cfg(feature = "scripted")]
fn scripted_show(url: &str, body: &str) -> ContentCommand {
    scripted_show_with_engine(inker::routing::ENGINE_SERVAL_SCRIPTED, url, body)
}

#[cfg(feature = "scripted")]
fn scripted_show_with_engine(engine: &str, url: &str, body: &str) -> ContentCommand {
    ContentCommand::Show {
        url: url.to_string(),
        state: Some(ContentState::Ready(Fetched {
            content_type: Some("text/html".to_string()),
            body: body.to_string(),
        })),
        engine: engine.to_string(),
        viewport: (420, 360),
        nav: NavGeneration::default(),
        viewport_gen: ViewportGeneration::default(),
        sheet: DocumentStyleSheet::default(),
    }
}

#[cfg(feature = "scripted")]
fn first_scene(updates: std::sync::mpsc::Receiver<ContentUpdate>) -> Scene {
    updates
        .iter()
        .find_map(|u| match u {
            ContentUpdate::Scene { scene, .. } => Some(scene),
            _ => None,
        })
        .expect("a scene update")
}

#[cfg(feature = "scripted")]
#[test]
fn scripted_rung_runs_inline_script_and_renders() {
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(scripted_show("https://example.com/app", INLINE_SCRIPT_PAGE));
    handle.join();
    assert!(
        glyph_runs(&first_scene(updates)) >= 1,
        "the scripted lane ran the inline script and rendered the injected text",
    );
}

#[cfg(feature = "scripted-nova")]
#[test]
fn scripted_nova_rung_runs_inline_script_and_renders() {
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(scripted_show_with_engine(
        inker::routing::ENGINE_SERVAL_SCRIPTED_NOVA,
        "https://example.com/app",
        INLINE_SCRIPT_PAGE,
    ));
    handle.join();
    assert!(
        glyph_runs(&first_scene(updates)) >= 1,
        "the Nova scripted lane ran the inline script and rendered the injected text",
    );
}

/// A click forwarded to a scripted tile dispatches to the page's listener: a page
/// whose click handler injects text emits no glyphs on load, then — after a
/// `ScriptedClick` over the clickable element — re-renders with the injected text.
/// Proves the input → event bridge runs end to end through the content actor.
/// (Render ladder phase 3.)
#[cfg(feature = "scripted")]
#[test]
fn scripted_rung_click_dispatches_to_script() {
    const PAGE: &str = "<body>\
        <div id='hit' style='width:300px;height:200px'></div>\
        <script>document.getElementById('hit').addEventListener('click', function(){\
            var p = document.createElement('p');\
            p.appendChild(document.createTextNode('clicked'));\
            document.body.appendChild(p);\
        });</script></body>";
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(scripted_show("https://example.com/app", PAGE));
    handle.command(ContentCommand::ScriptedClick {
        x: 50.0,
        y: 50.0, // inside the 300×200 div
        viewport_gen: ViewportGeneration::default(),
    });
    handle.join();

    let scenes: Vec<Scene> = updates
        .iter()
        .filter_map(|u| match u {
            ContentUpdate::Scene { scene, .. } => Some(scene),
            _ => None,
        })
        .collect();
    assert!(
        scenes.len() >= 2,
        "Show then ScriptedClick each emit a scene (got {})",
        scenes.len()
    );
    assert_eq!(
        glyph_runs(&scenes[0]),
        0,
        "the empty body paints no text before the click"
    );
    assert!(
        glyph_runs(scenes.last().unwrap()) >= 1,
        "the click ran the listener, injecting text that renders",
    );
}

/// Control: the same page on the static serval lane (its default engine) never
/// runs the script, so the otherwise-empty body paints no text — proving the
/// glyphs above came from the JS, not the markup.
#[cfg(feature = "scripted")]
#[test]
fn static_lane_leaves_the_inline_script_unrun() {
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        false,
    );
    handle.command(show(
        "https://example.com/app",
        "text/html",
        INLINE_SCRIPT_PAGE,
    ));
    handle.join();
    assert_eq!(
        glyph_runs(&first_scene(updates)),
        0,
        "the static lane ignores <script>; the empty body paints no text",
    );
}

/// The scripted rung fetches and runs an external `<script src>` against the
/// host-supplied body (via `from_body` + a fetcher): the script injects text and
/// the mutated DOM renders glyph runs. A mock fetcher keeps it deterministic — no
/// network. (Render ladder 2b.)
#[cfg(feature = "scripted")]
#[test]
fn scripted_rung_runs_external_script() {
    use pelt_desktop::ScriptResourceFetcher;
    struct MapFetcher(std::collections::HashMap<String, Vec<u8>>);
    impl ScriptResourceFetcher for MapFetcher {
        fn fetch(&self, url: &str) -> Option<Vec<u8>> {
            self.0.get(url).cloned()
        }
    }
    let mut files = std::collections::HashMap::new();
    files.insert(
        "http://x/app.js".to_string(),
        b"var p=document.createElement('p');\
          p.appendChild(document.createTextNode('ext'));\
          document.body.appendChild(p);"
            .to_vec(),
    );
    let fetcher = MapFetcher(files);
    let state = ContentState::Ready(Fetched {
        content_type: Some("text/html".to_string()),
        body: "<body><script src=\"app.js\"></script></body>".to_string(),
    });
    let mut doc = build_scripted(
        inker::routing::ENGINE_SERVAL_SCRIPTED,
        "http://x/index.html",
        Some(&state),
        Some(&fetcher),
    )
    .expect("scripted document with an external script");
    assert!(
        glyph_runs(&doc.frame(420, 360)) >= 1,
        "the external script fetched via the host fetcher ran and rendered",
    );
}

/// A page's `document.cookie` write on the scripted rung reaches the process session
/// jar (the same jar HTTP uses) — the cookie convergence. A unique origin keeps it
/// from clashing with the shared jar's other entries. (Render ladder 2c.)
#[cfg(feature = "scripted")]
#[test]
fn scripted_rung_document_cookie_reaches_the_jar() {
    use pelt_desktop::ScriptResourceFetcher;
    struct NoFetch;
    impl ScriptResourceFetcher for NoFetch {
        fn fetch(&self, _url: &str) -> Option<Vec<u8>> {
            None
        }
    }
    let url = "http://rung-2c-cookie.example/";
    let state = ContentState::Ready(Fetched {
        content_type: Some("text/html".to_string()),
        body: "<body><script>document.cookie = 'rung2c=ok';</script></body>".to_string(),
    });
    // A fetcher (even a no-op) is what installs the cookie provider; the script runs
    // on build and writes through it.
    let _doc = build_scripted(
        inker::routing::ENGINE_SERVAL_SCRIPTED,
        url,
        Some(&state),
        Some(&NoFetch),
    )
    .expect("scripted document");

    use netfetcher::{CookieStore, SameSiteContext};
    let parsed = url::Url::parse(url).unwrap();
    let cookies = crate::fetch::session_jar().cookies_for(&parsed, SameSiteContext::same_site());
    assert!(
        cookies.iter().any(|c| c == "rung2c=ok"),
        "the JS-set cookie reached the session jar: {cookies:?}",
    );
}

/// Headless-scripted extraction through the actor: a served shell with no metadata,
/// whose script injects a `<meta name=description>`, contributes that description
/// through the Contribution pipe — proving the post-JS DOM is what gets extracted
/// for a scripted-rung node (a static parse of the shell would find nothing).
/// (Render ladder phase 4.)
#[cfg(feature = "scripted")]
#[test]
fn scripted_rung_post_js_extract_contributes_metadata() {
    const PAGE: &str = "<head></head><body><script>\
        var m = document.createElement('meta');\
        m.setAttribute('name', 'description');\
        m.setAttribute('content', 'JS-rendered summary');\
        document.head.appendChild(m);\
        </script></body>";
    // auto_ingest = true so the actor runs the extraction/harvest path.
    let (handle, updates) = spawn_content(
        &Pool::new(),
        noop_wake(),
        std::collections::HashSet::new(),
        true,
    );
    handle.command(scripted_show("https://spa.test/app", PAGE));
    handle.join();

    let contributed = updates.iter().any(|u| match u {
        ContentUpdate::Contribution { contributions } => contributions.iter().any(|c| {
            c.nodes.iter().any(|n| {
                n.id == "https://spa.test/app"
                    && n.properties.iter().any(|(p, v)| {
                        p == "https://schema.org/description" && v == "JS-rendered summary"
                    })
            })
        }),
        _ => false,
    });
    assert!(
        contributed,
        "the post-JS extract contributed the JS-injected meta description",
    );
}
