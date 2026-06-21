/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The settings-lane provider seam (consolidation P1): resolve a pelt `SettingsRef` body
//! (e.g. `"pelt/appearance"`) to a named page of controls, and list a namespace's pages for
//! the index spine. Pages reuse the [`PaneItem`](crate::list_pane::PaneItem) model the
//! apparatus list-pane already renders and drains, so a control's key (a theme id,
//! `engine:toggle:<id>`, `phys:damping:up`) is handled by the existing host drain unchanged.
//! The `pelt` provider builds its pages from the host's current settings state via the shared
//! apparatus section builders; the `node:<id>` (facets) and `moot:<id>` providers join as
//! those land. See `2026-06-21_settings_lane_consolidation_plan`.
//!
//! Staged foundation: the provider seam is built + tested-by-compile but not yet wired —
//! the P1 render arm (the `settings://` content dispatch + the per-tile settings pane in the
//! shell document + `>settings`) is its consumer. Allowed dead until that lands.
#![allow(dead_code)]

use crate::WindowCtx;
use crate::apparatus::{engine_section_items, physics_section_items, theme_section_items};
use crate::list_pane::PaneItem;

/// One entry in a provider's index spine: the ref-suffix id and the display title.
pub(crate) struct SettingsPageRef {
    pub id: &'static str,
    pub title: &'static str,
}

/// A resolved settings page: its title plus the controls the settings tile renders.
pub(crate) struct SettingsPage {
    pub title: String,
    pub items: Vec<PaneItem>,
}

/// The pages a settings namespace offers, in index-spine order. `pelt` is the app settings;
/// the `node:<id>` (facets) and `moot:<id>` providers resolve once they land. A static seam
/// (no host state), so it is a free function. (Settings lane P1.)
pub(crate) fn settings_index(namespace: &str) -> Vec<SettingsPageRef> {
    match namespace {
        "pelt" => vec![
            SettingsPageRef { id: "appearance", title: "Appearance" },
            SettingsPageRef { id: "engines", title: "Engines" },
            SettingsPageRef { id: "physics", title: "Physics" },
        ],
        _ => Vec::new(),
    }
}

impl WindowCtx<'_> {
    /// Resolve a `SettingsRef` body (`"pelt/appearance"`) to its page; `None` for an unknown
    /// namespace or page. (Settings lane P1.)
    pub(crate) fn settings_page(&self, reference: &str) -> Option<SettingsPage> {
        let (namespace, page) = reference.split_once('/')?;
        match namespace {
            "pelt" => self.pelt_settings_page(page),
            _ => None,
        }
    }

    /// The `pelt` (app) provider: each page's controls built from the host's current state via
    /// the shared apparatus section builders, so the apparatus pane and the lane page never
    /// drift. (Settings lane P1.)
    fn pelt_settings_page(&self, page: &str) -> Option<SettingsPage> {
        let (title, items) = match page {
            "appearance" => ("Appearance", theme_section_items(&self.theme_options())),
            "engines" => ("Engines", engine_section_items(&self.engine_rows())),
            "physics" => ("Physics", physics_section_items(self.physics_damping())),
            _ => return None,
        };
        Some(SettingsPage { title: title.to_string(), items })
    }
}
