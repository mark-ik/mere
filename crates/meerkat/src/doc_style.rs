/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Document typography editing. The `pelt/reading` page drives these: each
//! mutates the user's `document_sheet` (typography only — colours stay
//! theme-owned), then re-composes the sheet, broadcasts it to the live content
//! actors so open cards re-lay, drops themed snapshots, persists, and redraws.
//! Mirrors `theme_edit` for the theme side. (Document typography surface.)

use document_canvas::{DocumentStyleSheet, LinkAdornment};

use super::WindowCtx;

/// Base text-size slider range (px). The slider fraction maps across it.
const SIZE_MIN: f32 = 10.0;
const SIZE_MAX: f32 = 24.0;
/// Line-spacing slider range (line-height ratio).
const SPACING_MIN: f32 = 1.0;
const SPACING_MAX: f32 = 2.0;

/// Curated starter font sets (full system-font enumeration is a follow-on). The
/// host font system resolves the name and falls back if a face is absent.
pub(crate) const BODY_FONTS: &[&str] = &["system-ui", "Georgia", "Verdana"];
pub(crate) const MONO_FONTS: &[&str] = &["monospace", "Consolas", "Courier New"];

impl WindowCtx<'_> {
    /// Set the base body text size to `fraction` (0..1) of [`SIZE_MIN`]..[`SIZE_MAX`].
    pub(super) fn set_doc_text_size(&mut self, fraction: f64) {
        let f = fraction.clamp(0.0, 1.0) as f32;
        self.shared.presentation.document_sheet.body_font_size =
            SIZE_MIN + f * (SIZE_MAX - SIZE_MIN);
        self.apply_doc_style();
    }

    /// Set the line-spacing (line-height ratio) to `fraction` (0..1) of the range.
    pub(super) fn set_doc_line_spacing(&mut self, fraction: f64) {
        let f = fraction.clamp(0.0, 1.0) as f32;
        self.shared.presentation.document_sheet.line_height_ratio =
            SPACING_MIN + f * (SPACING_MAX - SPACING_MIN);
        self.apply_doc_style();
    }

    /// Toggle the `⇒` / `⇗` link adornment on document-lane links.
    pub(super) fn toggle_doc_link_arrows(&mut self) {
        let sheet = &mut self.shared.presentation.document_sheet;
        sheet.link_adornment = match sheet.link_adornment {
            LinkAdornment::SchemeArrow => LinkAdornment::None,
            LinkAdornment::None => LinkAdornment::SchemeArrow,
        };
        self.apply_doc_style();
    }

    /// Set the body font family (a curated name; the font system resolves it).
    pub(super) fn set_doc_body_font(&mut self, name: &str) {
        self.shared.presentation.document_sheet.body_font_family = name.to_string();
        self.apply_doc_style();
    }

    /// Set the monospace (code) font family.
    pub(super) fn set_doc_mono_font(&mut self, name: &str) {
        self.shared.presentation.document_sheet.mono_font_family = name.to_string();
        self.apply_doc_style();
    }

    /// Reset the document typography to the built-in look.
    pub(super) fn reset_doc_style(&mut self) {
        self.shared.presentation.document_sheet = DocumentStyleSheet::default();
        self.apply_doc_style();
    }

    /// Re-compose the sheet (typography ⊕ theme colours), broadcast it to the live
    /// content actors so open cards re-lay, drop themed snapshots so they re-bake,
    /// persist, and redraw. The shared tail of every typography edit.
    fn apply_doc_style(&mut self) {
        let sheet = self.shared.presentation.document_sheet_composed();
        self.shared.content.constellation.retheme(sheet);
        self.view.snapshot_data_uris.clear();
        self.persist_settings();
        self.view.request_redraw();
    }
}
