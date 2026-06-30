/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Settings page builders (pelt/theme/reading/orrery/crawl/script).

use super::*;

impl WindowCtx<'_> {
    /// Resolve a `SettingsRef` body (`"pelt/appearance"`) to its page; `None` for an unknown
    /// namespace or page. (Settings lane P1.)
    pub(crate) fn settings_page(&self, reference: &str) -> Option<SettingsPage> {
        let (namespace, page) = reference.split_once('/')?;
        match namespace {
            "pelt" => self.pelt_settings_page(page),
            // The `node:<id>` facets provider (per-node config). (Settings lane P3.)
            ns if ns.starts_with("node:") => self.node_settings_page(ns, page),
            _ => None,
        }
    }

    /// The `pelt` (app) provider: each page's controls built from the host's current state via
    /// the shared apparatus section builders, so the apparatus pane and the lane page never
    /// drift. (Settings lane P1.)
    pub(crate) fn pelt_settings_page(&self, page: &str) -> Option<SettingsPage> {
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
            "scene" => ("Scene", scene_section_items()),
            "crawl" => ("Crawl", self.crawl_settings_items()),
            "scripts" => ("Scripts", self.script_settings_items()),
            "menu" => ("Menu", self.menu_settings_items()),
            _ => return None,
        };
        Some(SettingsPage {
            title: title.to_string(),
            items,
        })
    }

    /// The `pelt/menu` page: the persona-configurable context menu editor (command registry P4).
    /// The menu is an **inclusion** list — *any* registry command (a `Command` verb or a context
    /// action) can be added to or removed from the right-click menu. Two sections: the commands
    /// currently in the menu (in order, ✓, click to remove) and every other registry command
    /// (click to add). A configured command still only renders where it **applies** (its
    /// [`MenuScope`](meerkat::command::MenuScope)), so adding a selection action doesn't clutter
    /// the empty-canvas menu. Each row drains `menu:toggle:<id>`; the list rides the persona
    /// settings store. (Command registry P4.)
    pub(crate) fn menu_settings_items(&self) -> Vec<PaneItem> {
        let in_menu = &self.shared.presentation.menu_actions;
        let mut items = vec![
            PaneItem::text("app-title", "Context menu"),
            PaneItem::text(
                "app-row-muted",
                "Choose which commands appear in the right-click menu. Each shows only where it \
                 applies (a selection, the empty canvas)."
                    .to_string(),
            ),
        ];
        // In the menu, in order — the grip drags to reorder; the label click removes; the ▲ / ▼
        // buttons step one place. A drag in flight dims its row and marks the drop target.
        items.push(PaneItem::text("app-title", "In the menu"));
        let drag = self.view.row_reorder_drag.as_ref();
        for id in in_menu {
            let label = meerkat::command::registry_label(id).unwrap_or(id.as_str());
            let mut row = PaneItem::reorder_row(
                "app-btn-active",
                format!("{label}  \u{2713}"),
                format!("menu:toggle:{id}"),
                id.clone(),
                format!("menu:move:{id}:up"),
                format!("menu:move:{id}:down"),
            );
            if let Some(spec) = row.reorder.as_mut() {
                spec.dragging = drag.is_some_and(|d| d.id == *id);
                spec.drop_target = drag.is_some_and(|d| {
                    d.moved && d.id != *id && d.target.as_deref() == Some(id.as_str())
                });
            }
            items.push(row);
        }
        // Every other registry command — click to add.
        items.push(PaneItem::text("app-title", "Add a command"));
        for id in meerkat::command::all_registry_ids() {
            if in_menu.iter().any(|a| a == id) {
                continue;
            }
            let label = meerkat::command::registry_label(id).unwrap_or(id);
            items.push(PaneItem::button(
                "app-btn",
                label.to_string(),
                format!("menu:toggle:{id}"),
            ));
        }
        items.push(PaneItem::text("app-title", "Reset"));
        items.push(PaneItem::button(
            "app-btn",
            "Reset to default menu".to_string(),
            "menu:reset".to_string(),
        ));
        items
    }

    /// The theme-editor controls under the theme picker on the Appearance page:
    /// a fork action always, and for the active **user** theme the seed editor
    /// (mode toggle + per-seed HSL steppers) + remove. Built-ins are read-only,
    /// so editing one is a fork. The steppers drain `theme:fork` / `theme:mode` /
    /// `theme:seed:<seed>:<h|s|l>:<down|up>` / `theme:remove`. (Seed-palette T5.)
    pub(crate) fn theme_editor_items(&self) -> Vec<PaneItem> {
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
            Harmony::Locked {
                secondary_deg: s,
                tertiary_deg: t,
            } => match want {
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
            // Harmony is a single-selection picker (one active relation).
            items.push(PaneItem::radio(
                active(key),
                lbl.to_string(),
                format!("theme:harmony:{key}"),
            ));
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
                items.push(PaneItem::text(
                    "app-row-muted",
                    "Hue follows primary".to_string(),
                ));
            } else {
                items.push(PaneItem::slider(
                    "Hue",
                    format!("theme:seed:{seed}:h"),
                    (h / 360.0) as f32,
                    24,
                    true,
                ));
            }
            items.push(PaneItem::slider(
                "Saturation",
                format!("theme:seed:{seed}:s"),
                s as f32,
                16,
                false,
            ));
            items.push(PaneItem::slider(
                "Lightness",
                format!("theme:seed:{seed}:l"),
                l as f32,
                16,
                false,
            ));
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
    pub(crate) fn reading_settings_items(&self) -> Vec<PaneItem> {
        let s = &self.shared.presentation.document_sheet;
        let mut items = vec![PaneItem::text("app-title", "Document text".to_string())];

        items.push(PaneItem::text(
            "app-row",
            format!("Text size: {:.0} px", s.body_font_size),
        ));
        let size_frac = ((s.body_font_size - 10.0) / (24.0 - 10.0)).clamp(0.0, 1.0);
        items.push(PaneItem::slider(
            "Size",
            "doc:size".to_string(),
            size_frac,
            14,
            false,
        ));
        items.push(PaneItem::text(
            "app-row",
            format!("Line spacing: {:.0}%", s.line_height_ratio * 100.0),
        ));
        let spacing_frac = ((s.line_height_ratio - 1.0) / (2.0 - 1.0)).clamp(0.0, 1.0);
        items.push(PaneItem::slider(
            "Spacing",
            "doc:linespacing".to_string(),
            spacing_frac,
            10,
            false,
        ));

        let arrows_on = matches!(
            s.link_adornment,
            document_canvas::LinkAdornment::SchemeArrow
        );
        // Link arrows is an independent on / off switch.
        items.push(PaneItem::switch(
            arrows_on,
            format!("Link arrows: {}", if arrows_on { "on" } else { "off" }),
            "doc:arrows".to_string(),
        ));

        // The body / code font choices are each a single-selection picker.
        items.push(PaneItem::text("app-title", "Body font".to_string()));
        for name in crate::doc_style::BODY_FONTS {
            items.push(PaneItem::radio(
                s.body_font_family == *name,
                name.to_string(),
                format!("doc:bodyfont:{name}"),
            ));
        }
        items.push(PaneItem::text("app-title", "Code font".to_string()));
        for name in crate::doc_style::MONO_FONTS {
            items.push(PaneItem::radio(
                s.mono_font_family == *name,
                name.to_string(),
                format!("doc:monofont:{name}"),
            ));
        }

        items.push(PaneItem::button(
            "app-btn",
            "Reset to defaults".to_string(),
            "doc:reset".to_string(),
        ));
        items
    }

    /// The `pelt/orrery` page: the focused orrery's scene presentation settings — the layout
    /// strategy picker (force-directed + the registered strategies, active marked), the
    /// size-by-degree toggle, and the live workbench-mirror toggle. The controls drain
    /// `orrery:layout:<id>` / `orrery:sizebydegree` / `orrery:mirror` to the shared scene-toggle
    /// methods (the same ones the context menu drives). (Settings lane P2b.)
    pub(crate) fn orrery_settings_items(&self) -> Vec<PaneItem> {
        // `pick`: a member of a single-selection group (role=radio — the layout,
        // gloss-lens, and metric pickers). `flip`: an independent on / off switch
        // (role=switch — the size-by / rings / mirror toggles).
        let pick = |label: String, on: bool, key: String| PaneItem::radio(on, label, key);
        let flip = |label: String, on: bool, key: String| PaneItem::switch(on, label, key);
        let mut items = vec![PaneItem::text("app-title", "Layout")];
        let active = self.orrery().layout_strategy();
        items.push(pick(
            "Force-directed".to_string(),
            active.is_none(),
            "orrery:layout:".to_string(),
        ));
        for &(id, label) in platen::ORRERY_LAYOUT_STRATEGIES {
            items.push(pick(
                label.to_string(),
                active == Some(id),
                format!("orrery:layout:{id}"),
            ));
        }

        items.push(PaneItem::text("app-title", "Map"));
        let sbd = self.orrery().size_by_degree();
        let check = |label: &str, on: bool| {
            if on {
                format!("{label}  \u{2713}")
            } else {
                label.to_string()
            }
        };
        items.push(flip(
            check("Size by degree", sbd),
            sbd,
            "orrery:sizebydegree".to_string(),
        ));
        let sbi = self.orrery().size_by_importance();
        items.push(flip(
            check("Size by importance", sbi),
            sbi,
            "orrery:sizebyimportance".to_string(),
        ));
        // When size-by-importance is on, pick the metric it reads: degree (cheap) or betweenness
        // (structural brokerage — a bridge node stands out beyond its degree). (Graph signals.)
        if sbi {
            let by_degree = self.orrery().importance_metric() == orrery::ImportanceMetric::Degree;
            items.push(pick(
                check("  by degree", by_degree),
                by_degree,
                "orrery:importance:degree".to_string(),
            ));
            items.push(pick(
                check("  by betweenness", !by_degree),
                !by_degree,
                "orrery:importance:betweenness".to_string(),
            ));
        }
        // Community rings: halo each node in its Louvain community's colour, in any layout, so the
        // graph's clusters read spatially. (Graph signals — community to a ring.)
        let rings = self.orrery().show_community_rings();
        items.push(flip(
            check("Show community rings", rings),
            rings,
            "orrery:communityrings".to_string(),
        ));
        // Bridge rings: bold the graph's critical connectors, in any layout. The metric chooses
        // which notion: betweenness brokers (high traffic) or articulation points (cut vertices /
        // single points of failure). (Graph signals — bridges / articulation points.)
        let bridges = self.orrery().show_bridge_rings();
        items.push(flip(
            check("Show bridge rings", bridges),
            bridges,
            "orrery:bridgerings".to_string(),
        ));
        if bridges {
            let by_between = self.orrery().bridge_metric() == orrery::BridgeMetric::Betweenness;
            items.push(pick(
                check("  by betweenness", by_between),
                by_between,
                "orrery:bridge:betweenness".to_string(),
            ));
            items.push(pick(
                check("  by cut-vertex", !by_between),
                !by_between,
                "orrery:bridge:articulation".to_string(),
            ));
        }
        // Gloss lens: the gloss swatch can mirror the main view (a minimap) or show its OWN
        // arrangement (an independent lens, e.g. spectral while the main view is force-directed),
        // carrying the same community / bridge rings at its own positions. A picker like the main
        // layout one. (Graph signals — P6 / P6b, the independent gloss projection.)
        items.push(PaneItem::text("app-title", "Gloss lens"));
        let gloss = self.orrery().gloss_strategy();
        items.push(pick(
            "Mirror main view".to_string(),
            gloss.is_none(),
            "orrery:gloss:".to_string(),
        ));
        for &(id, label) in platen::ORRERY_LAYOUT_STRATEGIES {
            items.push(pick(
                label.to_string(),
                gloss == Some(id),
                format!("orrery:gloss:{id}"),
            ));
        }
        // The lens's own scope + encoding (independent of the main view): crop to the selection,
        // and size nodes by the importance signal. (Graph signals — P6c.)
        let gloss_scope = self.orrery().gloss_scope_selection();
        items.push(flip(
            check("Gloss: selection only", gloss_scope),
            gloss_scope,
            "orrery:glossscope".to_string(),
        ));
        let gloss_size = self.orrery().gloss_size_by_importance();
        items.push(flip(
            check("Gloss: size by importance", gloss_size),
            gloss_size,
            "orrery:glosssize".to_string(),
        ));
        // Cluster by affinity: an extra force-directed pull between structurally-similar nodes
        // (shared-neighbourhood Jaccard), drawing communities into tight clusters. (Graph signals — P4.)
        let affinity = self.orrery().cluster_by_affinity();
        items.push(flip(
            check("Cluster by affinity", affinity),
            affinity,
            "orrery:affinity".to_string(),
        ));
        let mirror = self.view.mirror_tiles;
        items.push(flip(
            check("Mirror open tiles", mirror),
            mirror,
            "orrery:mirror".to_string(),
        ));
        items
    }

    /// The `pelt/crawl` page: the scope / depth a `>crawl` roams under (relational-browse
    /// V2 controls). Scope picks which hosts links may lead into (same host → domain →
    /// any), depth how many link-hops from the seed. Each row drains `crawl:scope:<key>` /
    /// `crawl:depth:<n>` to the crawl session and persists. The same-host, shallow default
    /// keeps an accidental crawl cheap and polite.
    pub(crate) fn crawl_settings_items(&self) -> Vec<PaneItem> {
        use crate::crawl::HostScope;
        // `pick`: a member of a single-selection group (role=radio — Scope / Depth /
        // Page cap). `flip`: an independent on / off switch (role=switch — sitemap).
        let pick = |label: String, on: bool, key: String| PaneItem::radio(on, label, key);
        let flip = |label: String, on: bool, key: String| PaneItem::switch(on, label, key);
        let check = |label: &str, on: bool| {
            if on {
                format!("{label}  \u{2713}")
            } else {
                label.to_string()
            }
        };

        let current_scope = self.shared.content.crawl.scope();
        let mut items = vec![PaneItem::text("app-title", "Scope")];
        for scope in [
            HostScope::SameHost,
            HostScope::SameDomain,
            HostScope::AnyHost,
        ] {
            let on = scope == current_scope;
            items.push(pick(
                check(scope.label(), on),
                on,
                format!("crawl:scope:{}", scope.as_key()),
            ));
        }

        // Depth presets, plus the active value as a "Custom" row when it is not one of
        // them (e.g. hand-edited in settings.json), so the current depth is always shown
        // and re-selectable rather than leaving every row unchecked. (Crawl controls.)
        items.push(PaneItem::text("app-title", "Depth"));
        let current_depth = self.shared.content.crawl.max_depth();
        let mut depths: Vec<(u32, String)> = vec![
            (1, "Shallow (1 hop)".to_string()),
            (2, "Default (2 hops)".to_string()),
            (4, "Deep (4 hops)".to_string()),
        ];
        if !depths.iter().any(|(d, _)| *d == current_depth) {
            depths.push((current_depth, format!("Custom ({current_depth} hops)")));
            depths.sort_by_key(|(d, _)| *d);
        }
        for (depth, label) in &depths {
            let on = *depth == current_depth;
            items.push(pick(check(label, on), on, format!("crawl:depth:{depth}")));
        }

        // "Crawl whole site": seed from the site's sitemap.xml (its canonical page list)
        // rather than only the seed page's links — comprehensive, still bounded by the
        // page cap. Off keeps a crawl to the focused neighborhood. (Crawl controls.)
        items.push(PaneItem::text("app-title", "Mode"));
        let whole_site = self.shared.content.crawl.seed_sitemap();
        items.push(flip(
            check("Crawl whole site (sitemap)", whole_site),
            whole_site,
            "crawl:sitemap".to_string(),
        ));

        // Page cap: the hard stop on pages fetched (the runaway backstop). It is the bound
        // that actually limits a wide crawl — same-domain / any-host / whole-site all lean
        // on it — so it is the knob to raise for a comprehensive crawl. Off-preset values
        // show as "Custom". (Crawl controls.)
        items.push(PaneItem::text("app-title", "Page cap"));
        let current_pages = self.shared.content.crawl.max_pages();
        let mut caps: Vec<(usize, String)> = vec![
            (50, "50 pages (default)".to_string()),
            (200, "200 pages".to_string()),
            (1000, "1000 pages".to_string()),
        ];
        if !caps.iter().any(|(p, _)| *p == current_pages) {
            caps.push((current_pages, format!("Custom ({current_pages} pages)")));
            caps.sort_by_key(|(p, _)| *p);
        }
        for (pages, label) in &caps {
            let on = *pages == current_pages;
            items.push(pick(check(label, on), on, format!("crawl:pages:{pages}")));
        }
        items
    }

    /// The `pelt/scripts` page: DocumentScript capability permissions (§11.4). Each of
    /// `log` / `document` / `net` is a button that cycles default → Allow → Prompt →
    /// Deny (draining `script:cap:<cap>`), plus a read-only list of the installed
    /// auto-attach bindings so the user sees what runs. `net` (network egress) defaults
    /// **Deny** and is same-origin scoped — this page is where a user grants it for a
    /// trusted script. Reads the on-disk opinion (the host caches none), so the labels
    /// reflect exactly what an attach will resolve.
    pub(crate) fn script_settings_items(&self) -> Vec<PaneItem> {
        // The capability opinion comes from the cache `snapshot_settings_panes` refreshed on open
        // (not a disk read), and the bindings list from the constellation's live set — so this
        // per-frame rebuild touches no files. (Settings perf.)
        let prefs = self.view.script_caps.unwrap_or_default();
        let label = |opt: Option<Permission>| match opt {
            None => "default",
            Some(Permission::Allow) => "Allow",
            Some(Permission::Prompt) => "Prompt",
            Some(Permission::Deny) => "Deny",
            Some(Permission::Inherit) => "inherit",
        };
        let mut items = vec![PaneItem::text("app-title", "Capabilities")];
        for (name, cur) in [
            ("log", prefs.log),
            ("document", prefs.document),
            ("net", prefs.net),
        ] {
            items.push(PaneItem::button(
                "app-btn",
                format!("{name}: {}", label(cur)),
                format!("script:cap:{name}"),
            ));
        }
        items.push(PaneItem::text(
            "app-row-muted",
            "net = network egress (default Deny, same-origin only)",
        ));

        // Read-only: the installed auto-attach bindings (user file + installed mods), read from
        // the constellation's resolved set — the same list auto-attach matches against, already
        // in memory, so no per-frame binding-file parse. (Settings perf.)
        items.push(PaneItem::text("app-title", "Installed bindings"));
        let bindings = self.shared.content.constellation.script_bindings();
        if bindings.is_empty() {
            items.push(PaneItem::text("app-row-muted", "none"));
        } else {
            for b in bindings {
                let net = if b.net.effective == Permission::Allow {
                    " [net]"
                } else {
                    ""
                };
                items.push(PaneItem::text(
                    "app-row",
                    format!("{}  →  {}{net}", b.origin, b.component_path.display()),
                ));
            }
        }
        items
    }
}
