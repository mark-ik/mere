/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Author CSS for local note tiles.

use super::*;

/// Build the routed note tile's author CSS. Notes are local Mere documents, so
/// they follow the shell theme instead of third-party page CSS.
pub(crate) fn note_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        format!(
            ".note-sheet {{ display: block; background-color: {}; color: {}; padding: 28px 36px; }}",
            rgb(c.surface_bg),
            rgb(c.body_text)
        ),
        format!(
            ".note-sheet h1 {{ font-size: 30px; color: {}; margin: 0 0 18px 0; }} \
             .note-sheet h2 {{ font-size: 24px; color: {}; margin: 22px 0 12px 0; }} \
             .note-sheet h3 {{ font-size: 20px; color: {}; margin: 18px 0 10px 0; }}",
            rgb(c.strong_text),
            rgb(c.strong_text),
            rgb(c.strong_text)
        ),
        format!(
            ".note-sheet p, .note-sheet li, .note-sheet td, .note-sheet th {{ font-size: 17px; line-height: 1.55; color: {}; }} \
             .note-sheet a {{ color: {}; }}",
            rgb(c.body_text),
            rgb(c.strong_text)
        ),
        format!(
            ".note-sheet blockquote {{ display: block; margin: 16px 0; padding: 8px 18px; background-color: {}; color: {}; }} \
             .note-sheet pre {{ display: block; font-size: 14px; padding: 14px; background-color: {}; color: {}; }} \
             .note-sheet code {{ font-size: 14px; color: {}; }}",
            rgb(c.field_bg),
            rgb(c.body_text),
            rgb(c.field_bg),
            rgb(c.body_text),
            rgb(c.strong_text)
        ),
        format!(
            ".note-sheet img {{ display: block; max-width: 100%; margin: 16px 0; }} \
             .note-sheet table {{ display: table; margin: 16px 0; }} \
             .note-sheet th {{ color: {}; background-color: {}; }} \
             .note-sheet td, .note-sheet th {{ padding: 6px 10px; }} \
             .note-sheet .badge {{ color: {}; background-color: {}; padding: 4px 8px; }}",
            rgb(c.strong_text),
            rgb(c.field_bg),
            rgb(c.muted_text),
            rgb(c.field_bg)
        ),
    ]
}
