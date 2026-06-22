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
            SettingsPageRef { id: "reading", title: "Reading" },
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
                items.extend(self.theme_editor_items());
                items.push(PaneItem::text("app-title", "Tabs"));
                items.extend(tab_cap_items(self.view.chrome().settings.tab_cap));
                ("Appearance", items)
            }
            "reading" => ("Reading", self.reading_settings_items()),
            "engines" => ("Engines", engine_section_items(&self.engine_rows())),
            "physics" => ("Physics", physics_section_items(self.physics_damping())),
            "orrery" => ("Orrery", self.orrery_settings_items()),
            _ => return None,
        };
        Some(SettingsPage { title: title.to_string(), items })
    }

    /// The theme-editor controls under the theme picker on the Appearance page:
    /// a fork action always, and for the active **user** theme the seed editor
    /// (mode toggle + per-seed HSL steppers) + remove. Built-ins are read-only,
    /// so editing one is a fork. The steppers drain `theme:fork` / `theme:mode` /
    /// `theme:seed:<seed>:<h|s|l>:<down|up>` / `theme:remove`. (Seed-palette T5.)
    fn theme_editor_items(&self) -> Vec<PaneItem> {
        let mut items = vec![PaneItem::text("app-title", "Customize")];
        items.push(PaneItem::button(
            "app-btn",
            "+ New custom (fork current)".to_string(),
            "theme:fork".to_string(),
        ));

        let active = self.shared.presentation.active_theme_id.clone();
        let Some(def) = self.shared.presentation.theme.theme_def(&active) else {
            return items;
        };
        if def.source != register_theme::theme::ThemeSource::User {
            items.push(PaneItem::text(
                "app-row-muted",
                "Fork a built-in to edit its seed colours.".to_string(),
            ));
            return items;
        }

        let mode = if def.seeds.dark { "Dark" } else { "Light" };
        items.push(PaneItem::button(
            "app-btn",
            format!("Mode: {mode}  (toggle)"),
            "theme:mode".to_string(),
        ));

        // Harmony: how the accents relate to the base. `Custom` keeps each accent
        // independent; `Lock current` + the presets tie the secondary/tertiary hue
        // to the primary, so editing the base rotates the whole triad and its
        // derived activity accents stay coordinated. (Seed-palette harmony.)
        use register_theme::theme::Harmony;
        let locked = matches!(def.harmony, Harmony::Locked { .. });
        let near = |a: f32, b: f32| (a - b).abs() < 0.5;
        let active = |want: &str| match def.harmony {
            Harmony::Custom => want == "custom",
            Harmony::Locked { secondary_deg: s, tertiary_deg: t } => match want {
                "triadic" => near(s, 120.0) && near(t, 240.0),
                "analogous" => near(s, 30.0) && near(t, -30.0),
                "complementary" => near(s, 180.0) && near(t, 150.0),
                "mono" => near(s, 0.0) && near(t, 0.0),
                "lock" => ![(120.0, 240.0), (30.0, -30.0), (180.0, 150.0), (0.0, 0.0)]
                    .iter()
                    .any(|&(ps, pt)| near(s, ps) && near(t, pt)),
                _ => false,
            },
        };
        items.push(PaneItem::text("app-title", "Harmony".to_string()));
        for (key, lbl) in [
            ("custom", "Custom"),
            ("lock", "Lock current"),
            ("triadic", "Triadic"),
            ("analogous", "Analogous"),
            ("complementary", "Complementary"),
            ("mono", "Monochrome"),
        ] {
            let cls = if active(key) { "app-btn-active" } else { "app-btn" };
            items.push(PaneItem::button(cls, lbl.to_string(), format!("theme:harmony:{key}")));
        }

        // The hex label shows the *effective* (harmony-applied) colour so it
        // matches what renders; the sliders read the *stored* seed, since that is
        // what they edit (the hue slider is hidden for hue-locked accents anyway).
        // (Seed-palette harmony.)
        let eff = register_theme::seed::harmonized_seeds(def);
        for (seed, label) in [
            ("primary", "Primary"),
            ("secondary", "Secondary"),
            ("tertiary", "Tertiary"),
            ("neutral", "Neutral"),
        ] {
            let (stored, shown) = match seed {
                "primary" => (def.seeds.primary, eff.primary),
                "secondary" => (def.seeds.secondary, eff.secondary),
                "tertiary" => (def.seeds.tertiary, eff.tertiary),
                _ => (def.seeds.neutral, eff.neutral),
            };
            let (h, s, l) = tincture::color_to_hsl(stored);
            items.push(PaneItem::text(
                "app-row",
                format!("{label}: {}", tincture::color_to_hex(shown)),
            ));
            // When the triad is locked, the accents' hue is derived from the
            // primary, so their hue slider is replaced by a hint. Saturation +
            // lightness stay per-accent. (Seed-palette harmony.)
            let accent = seed == "secondary" || seed == "tertiary";
            if locked && accent {
                items.push(PaneItem::text("app-row-muted", "Hue follows primary".to_string()));
            } else {
                items.push(PaneItem::slider("Hue", format!("theme:seed:{seed}:h"), (h / 360.0) as f32, 24, true));
            }
            items.push(PaneItem::slider("Saturation", format!("theme:seed:{seed}:s"), s as f32, 16, false));
            items.push(PaneItem::slider("Lightness", format!("theme:seed:{seed}:l"), l as f32, 16, false));
        }

        items.push(PaneItem::button(
            "app-btn",
            "Remove this theme".to_string(),
            "theme:remove".to_string(),
        ));
        items
    }

    /// The `pelt/reading` page: document typography — base text size + line
    /// spacing sliders, the `⇒`/`⇗` link-arrows toggle, a curated body / code
    /// font choice, and a reset. The controls drain `doc:size:<i>:<count>` /
    /// `doc:linespacing:<i>:<count>` / `doc:arrows` / `doc:bodyfont:<name>` /
    /// `doc:monofont:<name>` / `doc:reset` to the [`crate::doc_style`] edit
    /// methods. Colours stay theme-owned (the Appearance page). (Typography.)
    fn reading_settings_items(&self) -> Vec<PaneItem> {
        let s = &self.shared.presentation.document_sheet;
        let mut items = vec![PaneItem::text("app-title", "Document text".to_string())];

        items.push(PaneItem::text("app-row", format!("Text size: {:.0} px", s.body_font_size)));
        let size_frac = ((s.body_font_size - 10.0) / (24.0 - 10.0)).clamp(0.0, 1.0);
        items.push(PaneItem::slider("Size", "doc:size".to_string(), size_frac, 14, false));
        items.push(PaneItem::text(
            "app-row",
            format!("Line spacing: {:.0}%", s.line_height_ratio * 100.0),
        ));
        let spacing_frac = ((s.line_height_ratio - 1.0) / (2.0 - 1.0)).clamp(0.0, 1.0);
        items.push(PaneItem::slider("Spacing", "doc:linespacing".to_string(), spacing_frac, 10, false));

        let arrows_on = matches!(s.link_adornment, document_canvas::LinkAdornment::SchemeArrow);
        items.push(PaneItem::button(
            if arrows_on { "app-btn-active" } else { "app-btn" },
            format!("Link arrows: {}", if arrows_on { "on" } else { "off" }),
            "doc:arrows".to_string(),
        ));

        items.push(PaneItem::text("app-title", "Body font".to_string()));
        for name in crate::doc_style::BODY_FONTS {
            let active = s.body_font_family == *name;
            items.push(PaneItem::button(
                if active { "app-btn-active" } else { "app-btn" },
                name.to_string(),
                format!("doc:bodyfont:{name}"),
            ));
        }
        items.push(PaneItem::text("app-title", "Code font".to_string()));
        for name in crate::doc_style::MONO_FONTS {
            let active = s.mono_font_family == *name;
            items.push(PaneItem::button(
                if active { "app-btn-active" } else { "app-btn" },
                name.to_string(),
                format!("doc:monofont:{name}"),
            ));
        }

        items.push(PaneItem::button("app-btn", "Reset to defaults".to_string(), "doc:reset".to_string()));
        items
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
