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
use register_theme::chrome::ChromeTheme;

use crate::WindowCtx;
use crate::apparatus::{engine_section_items, physics_section_items, theme_section_items};
use crate::list_pane::PaneItem;
use crate::settings_pane_view::{SettingsPane, SettingsSpineEntry};

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
            SettingsPageRef { id: "orrery", title: "Orrery" },
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
        let (title, items): (&str, Vec<PaneItem>) = match page {
            // Appearance carries the theme buttons plus the active-tab cap (migrated from the
            // retired settings overlay), so global look-and-feel lives on one page. (P2.)
            "appearance" => {
                let mut items = theme_section_items(&self.theme_options());
                items.push(PaneItem::text("app-title", "Tabs"));
                items.extend(tab_cap_items(self.view.chrome().settings.tab_cap));
                ("Appearance", items)
            }
            "engines" => ("Engines", engine_section_items(&self.engine_rows())),
            "physics" => ("Physics", physics_section_items(self.physics_damping())),
            "orrery" => ("Orrery", self.orrery_settings_items()),
            _ => return None,
        };
        Some(SettingsPage { title: title.to_string(), items })
    }

    /// The `pelt/orrery` page: the focused orrery's scene presentation settings — the layout
    /// strategy picker (force-directed + the registered strategies, active marked), the
    /// size-by-degree toggle, and the live workbench-mirror toggle. The controls drain
    /// `orrery:layout:<id>` / `orrery:sizebydegree` / `orrery:mirror` to the shared scene-toggle
    /// methods (the same ones the context menu drives). (Settings lane P2b.)
    fn orrery_settings_items(&self) -> Vec<PaneItem> {
        let toggle = |label: String, on: bool, key: String| {
            PaneItem::button(if on { "app-btn-active" } else { "app-btn" }, label, key)
        };
        let mut items = vec![PaneItem::text("app-title", "Layout")];
        let active = self.orrery().layout_strategy();
        items.push(toggle("Force-directed".to_string(), active.is_none(), "orrery:layout:".to_string()));
        for &(id, label) in platen::ORRERY_LAYOUT_STRATEGIES {
            items.push(toggle(label.to_string(), active == Some(id), format!("orrery:layout:{id}")));
        }

        items.push(PaneItem::text("app-title", "Map"));
        let sbd = self.orrery().size_by_degree();
        let check = |label: &str, on: bool| {
            if on { format!("{label}  \u{2713}") } else { label.to_string() }
        };
        items.push(toggle(check("Size by degree", sbd), sbd, "orrery:sizebydegree".to_string()));
        let mirror = self.view.mirror_tiles;
        items.push(toggle(check("Mirror open tiles", mirror), mirror, "orrery:mirror".to_string()));
        items
    }

    /// Snapshot the open settings tiles into the shell document each frame: resolve each
    /// `(member, ref, body rect)` the content dispatch recorded through the provider seam
    /// (its page controls + the namespace's index spine) and fold them in. An empty list
    /// clears the panes (the last settings tile closed). (Settings lane P1.)
    pub(crate) fn snapshot_settings_panes(
        &mut self,
        tiles: Vec<(GraphMemberId, String, [f32; 4])>,
    ) {
        // The body rects the input path routes presses against (to the shell document, not
        // the workbench surface). Set every frame, including empty. (Settings lane P1.)
        self.view.settings_rects = tiles.iter().map(|(m, _, rect)| (*m, *rect)).collect();
        if tiles.is_empty() {
            if self.view.settings_panes_open() {
                self.view.set_settings_panes(Vec::new(), String::new());
            }
            return;
        }
        let panes: Vec<SettingsPane> = tiles
            .into_iter()
            .filter_map(|(member, reference, rect)| {
                let page = self.settings_page(&reference)?;
                // `pelt/appearance` → namespace `pelt`, active page `appearance`.
                let (namespace, active) =
                    reference.split_once('/').unwrap_or((reference.as_str(), ""));
                let spine = settings_index(namespace)
                    .into_iter()
                    .map(|p| SettingsSpineEntry {
                        id: p.id.to_string(),
                        title: p.title.to_string(),
                        active: p.id == active,
                    })
                    .collect();
                Some(SettingsPane {
                    member,
                    rect,
                    namespace: namespace.to_string(),
                    page_title: page.title,
                    spine,
                    items: page.items,
                })
            })
            .collect();
        let panel_bg = panel_bg_rgb(&self.shared.presentation.chrome_theme);
        self.view.set_settings_panes(panes, panel_bg);
    }

    /// Open a settings page as a workbench tile: mint (or reuse) its ephemeral `settings://`
    /// node, open the workbench, and focus the tile. The `>settings` command routes here;
    /// the per-frame settings dispatch then renders the page. (Settings lane P1.)
    pub(crate) fn open_settings_tile(&mut self, reference: &str) {
        let url = format!("settings://{reference}");
        // Reuse the node for this exact page if it is already in the graph, so repeated
        // `>settings` does not mint duplicate ephemeral nodes; else mint one.
        let existing = self.orrery().graph().get_node_by_url(&url).map(|(_, n)| n.id);
        let member =
            existing.unwrap_or_else(|| self.orrery_mut().open_member_as_new_node(None, &url));
        self.open_workbench();
        self.view.workbench.open_tile(member);
        self.view.focused_tile = Some(member);
        self.view.request_redraw();
    }
}

/// A friendly tab title for a `settings://<ns>/<page>` url (e.g. `Settings: Appearance`),
/// or `None` for a non-settings url (the tab keeps its own title). The tab reads as a
/// settings page, not a raw scheme url. (Settings lane P1.)
pub(crate) fn settings_tab_title(url: &str) -> Option<String> {
    let reference = url.strip_prefix("settings://")?;
    let page = reference.rsplit('/').next().unwrap_or("");
    if page.is_empty() {
        return Some("Settings".to_string());
    }
    let mut title = String::from("Settings: ");
    let mut chars = page.chars();
    if let Some(first) = chars.next() {
        title.extend(first.to_uppercase());
        title.extend(chars);
    }
    Some(title)
}

/// The chrome theme's panel background as an `rgb(...)` string, for the settings pane
/// column containers (so they match the active theme without a sheet of their own).
fn panel_bg_rgb(theme: &ChromeTheme) -> String {
    let [r, g, b, _] = theme.panel_bg.to_array();
    format!("rgb({r}, {g}, {b})")
}

/// The active-tab cap control for the `pelt/appearance` page: the current value plus − / +
/// buttons (`tiles:cap:down` / `tiles:cap:up`, drained to `Chrome::dec_tab_cap` /
/// `Chrome::inc_tab_cap`). Migrated from the retired settings overlay. (P2.)
fn tab_cap_items(cap: usize) -> Vec<PaneItem> {
    vec![
        PaneItem::text("app-row", format!("Active tab cap: {cap}")),
        PaneItem::button("app-btn", "\u{2212} fewer active tabs".to_string(), "tiles:cap:down".to_string()),
        PaneItem::button("app-btn", "+ more active tabs".to_string(), "tiles:cap:up".to_string()),
    ]
}
