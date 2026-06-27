/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Settings actions: script cap, snapshot, swatch, open tile.

use super::*;

impl WindowCtx<'_> {
    /// Cycle the `script:cap:<cap>` permission opinion (default → Allow → Prompt →
    /// Deny → default) for the named capability, persist it, and re-derive the
    /// auto-attach bindings so the change takes effect on the next attach. (Tail 3.)
    pub(crate) fn set_script_cap(&mut self, cap: &str) {
        let root = self.shared.session.mere_root.clone();
        let mut settings = settings_store::load_settings(&root).ok().flatten().unwrap_or_default();
        let next = |cur: Option<Permission>| match cur {
            None => Some(Permission::Allow),
            Some(Permission::Allow) => Some(Permission::Prompt),
            Some(Permission::Prompt) => Some(Permission::Deny),
            _ => None,
        };
        match cap {
            "log" => settings.script_permissions.log = next(settings.script_permissions.log),
            "document" => {
                settings.script_permissions.document = next(settings.script_permissions.document)
            }
            "net" => settings.script_permissions.net = next(settings.script_permissions.net),
            _ => return,
        }
        if let Err(err) = settings_store::save_settings(&root, &settings) {
            tracing::warn!(%err, "failed to persist script permission");
            return;
        }
        // Re-push the bindings so a later auto-attach resolves under the new opinion.
        let prefs = settings.script_permissions;
        let mut bindings = crate::content::script::load_resolved_bindings(&root, &prefs);
        bindings.extend(crate::content::script::load_mod_bindings(&root, &prefs));
        self.shared.content.constellation.set_script_bindings(bindings);
        // Keep the open Scripts page's cache in step with the edit (it stays open across the
        // click), so the labels refresh without re-reading disk. (Settings perf.)
        self.view.script_caps = Some(prefs);
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
        // Refresh the Scripts page's permission cache: read `settings.json` only when that page
        // first opens (then keep it across frames), and drop it once no scripts tile is open, so
        // a reopen re-reads and picks up any out-of-band change. This is what keeps the per-frame
        // page rebuild free of disk I/O — the bindings it also lists come from the constellation's
        // in-memory set. (Settings perf.)
        let scripts_open = tiles.iter().any(|(_, reference, _)| reference == "pelt/scripts");
        if scripts_open {
            if self.view.script_caps.is_none() {
                self.view.script_caps = Some(
                    settings_store::load_settings(&self.shared.session.mere_root)
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                        .script_permissions,
                );
            }
        } else {
            self.view.script_caps = None;
        }
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
                let swatch = self.node_appearance_swatch(namespace, active);
                Some(SettingsPane {
                    member,
                    rect,
                    namespace: namespace.to_string(),
                    page_title: page.title,
                    spine,
                    items: page.items,
                    swatch,
                })
            })
            .collect();
        let panel_bg = panel_bg_rgb(&self.shared.presentation.chrome_theme);
        self.view.set_settings_panes(panes, panel_bg);
    }

    /// Build the node shape-editor swatch for a `node:<id>/appearance` page: the subject node's
    /// sprite image (optional, the tracing underlay) + its collider hull (the body), for the
    /// swatch to render and Stage B to edit. Shows for any node with a sprite **or** a custom
    /// body, so a node whose hull was traced then switched to a favicon face still edits its
    /// body. `None` for any other page, an unknown id, or a node with neither (nothing to edit
    /// yet). (Node body & face — the shape editor.)
    pub(crate) fn node_appearance_swatch(&self, namespace: &str, page: &str) -> Option<SwatchSpec> {
        if page != "appearance" {
            return None;
        }
        let subject: GraphMemberId = namespace.strip_prefix("node:")?.parse().ok()?;
        let key = self.orrery().graph().get_node_by_id(subject).map(|(k, _)| k)?;
        let sprite = self.orrery().node_sprite(key).map(str::to_string);
        let hull =
            self.orrery().node_sprite_hull(key).map(<[(f32, f32)]>::to_vec).unwrap_or_default();
        // Nothing to edit yet if the node has neither a sprite nor a body hull (authoring a hull
        // from scratch is a later editor step).
        if sprite.is_none() && hull.len() < 3 {
            return None;
        }
        // Carry the subject so the swatch's vertex drag knows whose hull to edit. (Stage B.)
        Some(SwatchSpec { sprite, hull, subject: Some(subject) })
    }

    /// Open a settings page as a workbench tile: mint (or reuse) its ephemeral `settings://`
    /// node, open the workbench, and focus the tile. The `>settings` command routes here;
    /// the per-frame settings dispatch then renders the page. (Settings lane P1.)
    pub(crate) fn open_settings_tile(&mut self, reference: &str) {
        let url = format!("settings://{reference}");
        // A per-node facet page (`node:<id>/…`) hangs off the node it configures: link the
        // settings node to that subject so the facets sit beside their node in the graph,
        // not floating unlinked. Global pages (`pelt/…`) have no subject. (Settings lane — facet edge.)
        let subject = reference
            .strip_prefix("node:")
            .and_then(|rest| rest.split('/').next())
            .and_then(|id| uuid::Uuid::parse_str(id).ok());
        // Reuse the node for this exact page if it is already in the graph, so repeated
        // `>settings` does not mint duplicate ephemeral nodes; else mint one (linked to its
        // subject for a facet page).
        let existing = self.orrery().graph().get_node_by_url(&url).map(|(_, n)| n.id);
        let member =
            existing.unwrap_or_else(|| self.orrery_mut().open_member_as_new_node(subject, &url));
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
pub(crate) fn panel_bg_rgb(theme: &ChromeTheme) -> String {
    let [r, g, b, _] = theme.panel_bg.to_array();
    format!("rgb({r}, {g}, {b})")
}

/// The active-tab cap control for the `pelt/appearance` page: the current value plus − / +
/// buttons (`tiles:cap:down` / `tiles:cap:up`, drained to `Chrome::dec_tab_cap` /
/// `Chrome::inc_tab_cap`). Migrated from the retired settings overlay. (P2.)
pub(crate) fn tab_cap_items(cap: usize) -> Vec<PaneItem> {
    vec![
        PaneItem::text("app-row", format!("Active tab cap: {cap}")),
        PaneItem::button("app-btn", "\u{2212} fewer active tabs".to_string(), "tiles:cap:down".to_string()),
        PaneItem::button("app-btn", "+ more active tabs".to_string(), "tiles:cap:up".to_string()),
    ]
}
