/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Note render surface: an `EngineDocument` → serval `Scene`.
//!
//! Builds the note's serval view tree ([`meerkat::note_view`]) into a `ScriptedDom`
//! through a [`ServalAppRunner`], lays it out, and lowers to a `netrender::Scene` —
//! the same `ScriptedDom` → [`scene_from_session`](crate::serval_render::scene_from_session)
//! path the chrome paints through, so a note renders through the web engine. The
//! serval-rendered parallel to [`crate::card::render_card_scene`] (which lowers via
//! document-canvas); the note path of the note-as-routed-serval-document-tile
//! reframe (djot editor plan, 2026-06-27).
//!
//! Slice 1 of the reframe proves the path end to end; the retained, editable,
//! hit-testable surface and the live compositing arrive in the later slices.

use std::cell::RefCell;
use std::rc::Rc;

use inker::EngineDocument;
use netrender::Scene;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::ScriptedDom;
use xilem_serval::{el, ServalAppRunner};

use crate::pane_session::PaneSession;
use meerkat::note_view::document_views;

/// Render a document's note views to a serval `Scene` at `w`×`h`, themed by
/// `sheets`. One-shot: builds a fresh runner + layout each call (mirroring
/// [`crate::card::render_card_scene`]); the retained surface arrives with edit
/// mode. The document is wrapped in an `<article>` root.
pub fn note_scene(doc: &EngineDocument, w: u32, h: u32, sheets: &[String]) -> Scene {
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let doc = doc.clone();
    let mut runner =
        ServalAppRunner::new(dom, move |_: &()| el("article", document_views(&doc)), ());
    let mut session: Option<PaneSession> = None;
    let sheet: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let dom = runner.dom();
    PaneSession::scene(&mut session, &dom, &sheet, w, h, None, &ScrollOffsets::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{DocumentBlock, InlineSpan};

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
                DocumentBlock::Heading {
                    level: 1,
                    spans: vec![InlineSpan::Text("Mere".into())],
                },
                DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text(
                        "A graph-shaped browser, hosted on serval.".into(),
                    )],
                },
            ],
        }
    }

    #[test]
    fn renders_a_document_to_a_scene() {
        // Exercises the whole note render path end to end: note views →
        // ScriptedDom → cascade + layout → scene_from_session → Scene. A
        // non-panicking render proves the pipeline runs on a note; the Scene → GPU
        // half is the chrome's already-proven path.
        let _scene = note_scene(&welcome(), 800, 600, &[]);
    }
}
