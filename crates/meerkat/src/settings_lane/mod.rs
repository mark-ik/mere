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
//! Wired by the P1 render arm: the `settings://` content dispatch (render.rs) records each
//! open settings tile, and [`WindowCtx::snapshot_settings_panes`] resolves them through this
//! seam into the shell document's settings panes each frame.

use forme::GraphMemberId;
use kernel::permissions::Permission;
use register_theme::chrome::ChromeTheme;
use session_runtime::settings_store;

use crate::WindowCtx;
use crate::apparatus::{engine_section_items, physics_section_items, theme_section_items};
use crate::list_pane::PaneItem;
use crate::scene_settings::scene_section_items;
use crate::settings_pane_view::{SettingsPane, SettingsSpineEntry};
use crate::swatch::SwatchSpec;

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
            SettingsPageRef {
                id: "appearance",
                title: "Appearance",
            },
            SettingsPageRef {
                id: "reading",
                title: "Reading",
            },
            SettingsPageRef {
                id: "engines",
                title: "Engines",
            },
            SettingsPageRef {
                id: "physics",
                title: "Physics",
            },
            SettingsPageRef {
                id: "orrery",
                title: "Orrery",
            },
            SettingsPageRef {
                id: "scene",
                title: "Scene",
            },
            SettingsPageRef {
                id: "crawl",
                title: "Crawl",
            },
            SettingsPageRef {
                id: "scripts",
                title: "Scripts",
            },
            SettingsPageRef {
                id: "menu",
                title: "Menu",
            },
        ],
        // The `node:<id>` facets provider lists its own pages. (Settings lane P3.)
        ns if ns.starts_with("node:") => crate::settings_node::node_settings_index(),
        _ => Vec::new(),
    }
}

mod ops;
mod pages;
pub(crate) use ops::*;
