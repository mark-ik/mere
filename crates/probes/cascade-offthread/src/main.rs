/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Probe: does the serval cascade run off the main thread and concurrently?
//!
//! P2 of the actor constellation plan moves `render_content_scene` onto a
//! content-actor thread, one per tile. The serval cascade leans on Stylo's
//! process-global `GLOBAL_STYLE_DATA`, a `CascadeGuard` thread-local, and a leaked
//! per-thread sharing cache. This probe runtime-verifies that the cascade
//! initializes and runs correctly off the main thread and across N concurrent
//! threads (the real P2 condition: several content actors cascading at once),
//! before P2 is called low-risk. It replays the exact `card.rs::html_scene` path.

use std::thread;

use pelt_live::scene_from_layout_dom;
use serval_layout::{NoImageLoader, ScrollOffsets, inline_stylesheets};
use serval_static_dom::StaticDocument;

const HTML: &str = "<style>p { color: rgb(20, 20, 20); } h1 { font-size: 30px; }</style>\
     <h1>Heading</h1><p>One paragraph of text.</p><p>Another paragraph here.</p>";

const SHEET: &[&str] =
    &["html, body, div, p, h1 { display: block; }", "body { padding: 16px; }"];

/// Replays `card.rs::html_scene`: parse the document, layer the page's own inline
/// `<style>` over the card defaults, run the cascade + layout, and count the glyph
/// runs the scene lowers to. A non-zero count means the cascade ran and produced
/// text.
fn render_glyph_runs(w: u32, h: u32) -> usize {
    let doc = StaticDocument::parse(HTML);
    let inline = inline_stylesheets(&doc);
    let mut sheets: Vec<&str> = SHEET.to_vec();
    sheets.extend(inline.iter().map(String::as_str));
    let scroll = ScrollOffsets::default();
    let scene = scene_from_layout_dom(&doc, &sheets, &NoImageLoader, w, h, &scroll);
    scene
        .ops
        .iter()
        .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
        .count()
}

fn main() {
    // Baseline on the main thread.
    let baseline = render_glyph_runs(420, 360);
    println!("main-thread glyph runs:    {baseline}");
    assert!(baseline >= 1, "FAIL: the baseline cascade produced no glyph runs");

    // A single render off the main thread.
    let off = thread::spawn(|| render_glyph_runs(420, 360))
        .join()
        .expect("FAIL: the off-thread cascade panicked");
    println!("off-thread glyph runs:     {off}");
    assert_eq!(off, baseline, "FAIL: the off-thread cascade disagreed with the baseline");

    // N content-actor threads cascading at once, each at a slightly different size
    // (independent layouts, the real concurrent condition).
    const N: u32 = 8;
    let handles: Vec<_> = (0..N)
        .map(|i| thread::spawn(move || render_glyph_runs(360 + i * 12, 340 + i * 6)))
        .collect();
    let mut ok = true;
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.join() {
            Ok(n) => {
                println!("concurrent thread {i}:      {n} glyph runs");
                if n == 0 {
                    ok = false;
                }
            },
            Err(_) => {
                println!("concurrent thread {i}:      PANICKED");
                ok = false;
            },
        }
    }
    assert!(ok, "FAIL: a concurrent cascade thread produced no glyphs or panicked");

    println!("\nPASS: the serval cascade runs off the main thread and across {N} concurrent threads.");
}
