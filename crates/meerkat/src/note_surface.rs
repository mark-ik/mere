/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Note render surface: an `EngineDocument` → serval `Scene`.
//!
//! Builds the note's serval view tree ([`meerkat::note_view`]) into a `ScriptedDom`
//! through a [`ServalAppRunner`], lays it out with serval-layout's content band
//! path, and lowers the band to a `netrender::Scene`. This is the serval-rendered
//! parallel to [`crate::card::render_card_scene`] (which lowers via
//! document-canvas) for the note-as-routed-serval-document-tile reframe (djot
//! editor plan, 2026-06-27).
//!
//! Slice 1 of the reframe proves the path end to end; the retained, editable,
//! hit-testable surface and the live compositing arrive in the later slices.

use std::cell::RefCell;
use std::rc::Rc;

use inker::EngineDocument;
use netrender::Scene;
use paint_list_api::PaintList;
use serval_layout::{NoImageLoader, ScrollOffsets};
use serval_scripted_dom::ScriptedDom;
use xilem_serval::{ServalAppRunner, el};

use meerkat::note_view::document_views;

/// One rasterizable note band plus the full laid-out document height it came
/// from. The height lets the host scroll/window note tiles like other content.
pub(crate) struct NoteSceneBand {
    pub(crate) scene: Scene,
    pub(crate) content_height: u32,
}

/// Render one vertical band of a note, reporting the full scrollable height.
///
/// `viewport_h` is the visible document viewport the note lays out against;
/// `band_y`/`band_h` select the cached texture band to emit from that layout.
pub(crate) fn note_scene_band(
    doc: &EngineDocument,
    w: u32,
    viewport_h: u32,
    band_y: u32,
    band_h: u32,
    sheets: &[String],
) -> NoteSceneBand {
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let doc = doc.clone();
    let runner = ServalAppRunner::new(
        dom,
        move |_: &()| el("article", document_views(&doc)).attr("class", "note-sheet"),
        (),
    );
    let sheet: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let dom = runner.dom();
    let dom_ref = dom.borrow();
    let viewport_h = viewport_h.max(1);
    let band_h = band_h.max(1);
    let layout = serval_layout::lay_out_content(&*dom_ref, &sheet, &NoImageLoader, w, viewport_h);
    let (list, scroll_range, _links) =
        layout.emit_band(&*dom_ref, band_y, band_h, &ScrollOffsets::default());
    let translated = paint_list_render::translate_paint_cmd_stream(
        list.viewport(),
        list.commands(),
        list.fonts(),
        list.images(),
    );
    NoteSceneBand {
        scene: translated.scene,
        content_height: (viewport_h as f32 + scroll_range.1).ceil().max(1.0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{Block, InlineSpan};

    fn welcome() -> EngineDocument {
        EngineDocument {
            address: "mere://welcome".into(),
            title: None,
            content_type: "text/x-knot".into(),
            lang: None,
            provenance: Default::default(),
            trust: Default::default(),
            diagnostics: Vec::new(),
            blocks: vec![
                Block::Heading {
                    level: 1,
                    spans: vec![InlineSpan::Text("Mere".into())],
                },
                Block::Paragraph {
                    spans: vec![InlineSpan::Text(
                        "A graph-shaped browser, hosted on serval.".into(),
                    )],
                },
            ],
        }
    }

    fn tall_note() -> EngineDocument {
        let blocks = (0..24)
            .map(|i| Block::Paragraph {
                spans: vec![InlineSpan::Text(format!(
                    "Paragraph {i}: a line of note text that should push the document below the viewport."
                ))],
            })
            .collect();
        EngineDocument {
            address: "knot://tall".into(),
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
    fn renders_a_document_to_a_scene() {
        // Exercises the whole note render path end to end: note views →
        // ScriptedDom → cascade + layout → band lowerer → Scene. A
        // non-panicking render proves the pipeline runs on a note; the Scene → GPU
        // half is the chrome's already-proven path.
        let _band = note_scene_band(&welcome(), 800, 600, 0, 600, &[]);
    }

    #[test]
    fn note_band_reports_full_content_height() {
        let band = note_scene_band(&tall_note(), 420, 120, 0, 120, &[]);
        assert!(
            band.content_height > 120,
            "tall note should report scrollable height, got {}",
            band.content_height
        );
    }
}
