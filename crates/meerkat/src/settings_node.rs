/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The `node:<id>` facets provider for the settings lane (consolidation P3): per-node
//! settings pages, the home for the per-node config that lives in the context menu today
//! (engine pin, representation) plus the Inspector's selected-object view. The namespace is
//! `node:<uuid>`; a page ref is `node:<uuid>/<page>` (`info` / `engine` / `appearance`),
//! resolved here by parsing the id, finding the node in the focused graph, and building the
//! page's controls in the lane's [`PaneItem`](crate::list_pane::PaneItem) model — so it
//! renders + drains through the same shell-document settings pane the `pelt` provider uses.
//!
//! First slice: the **info** page (read-only node facts). The `engine` + `appearance` pages
//! (which migrate the context menu's per-node pickers) join next; `settings_index` lists only
//! the pages this provider actually serves so the spine never offers a dead page.
//! See `2026-06-21_settings_lane_consolidation_plan` (P3).

use forme::GraphMemberId;
use orrery::Representation;

use crate::WindowCtx;
use crate::list_pane::PaneItem;
use crate::settings_lane::{SettingsPage, SettingsPageRef};

/// The pages the `node:<id>` provider serves, in spine order. (Settings lane P3.)
pub(crate) fn node_settings_index() -> Vec<SettingsPageRef> {
    vec![
        SettingsPageRef { id: "info", title: "Info" },
        SettingsPageRef { id: "appearance", title: "Appearance" },
        SettingsPageRef { id: "engine", title: "Engine" },
    ]
}

impl WindowCtx<'_> {
    /// Resolve a `node:<uuid>/<page>` ref: parse the member id, confirm the node is in the
    /// focused graph, and build the requested page. `None` for a bad id, a missing node, or
    /// an unknown page. (Settings lane P3.)
    pub(crate) fn node_settings_page(&self, namespace: &str, page: &str) -> Option<SettingsPage> {
        let member: GraphMemberId = namespace.strip_prefix("node:")?.parse().ok()?;
        // The node must still exist (it may have been deleted while the tile is open).
        self.orrery().graph().get_node_by_id(member)?;
        match page {
            "info" => Some(SettingsPage {
                title: "Node: Info".to_string(),
                items: self.node_info_items(member),
            }),
            "appearance" => Some(SettingsPage {
                title: "Node: Appearance".to_string(),
                items: self.node_appearance_items(member),
            }),
            "engine" => Some(SettingsPage {
                title: "Node: Engine".to_string(),
                items: self.node_engine_items(member),
            }),
            _ => None,
        }
    }

    /// The `node:<id>/info` page: read-only facts about the node (the Inspector's
    /// selected-object view, as lane controls). (Settings lane P3.)
    fn node_info_items(&self, member: GraphMemberId) -> Vec<PaneItem> {
        let orrery = self.orrery();
        let Some((key, node)) = orrery.graph().get_node_by_id(member) else {
            return Vec::new();
        };
        let mut items = vec![PaneItem::text("app-title", "Node")];
        if !node.title.is_empty() {
            items.push(PaneItem::text("app-row", format!("Title: {}", node.title)));
        }
        items.push(PaneItem::text("app-row", format!("URL: {}", node.url())));
        items.push(PaneItem::text("app-title", "Presentation"));
        items.push(PaneItem::text(
            "app-row",
            format!("Representation: {:?}", orrery.node_representation(key)),
        ));
        items.push(PaneItem::text("app-row", format!("Size: {:.0} px", orrery.node_size(key))));
        items
    }

    /// The `node:<id>/appearance` page: the per-node representation picker (Show as tile /
    /// shape), migrated from the context menu. Controls drain `nodefacet:<id>:rep:<form>`,
    /// applied by [`apply_node_facet_key`](Self::apply_node_facet_key) to this node. The
    /// scene-wide default stays content-typed; this is a per-node override. (Settings lane P3.)
    fn node_appearance_items(&self, member: GraphMemberId) -> Vec<PaneItem> {
        let orrery = self.orrery();
        let Some((key, _)) = orrery.graph().get_node_by_id(member) else {
            return Vec::new();
        };
        let rep = orrery.node_representation(key);
        let cls = |on: bool| if on { "app-btn-active" } else { "app-btn" };
        let mut items = vec![PaneItem::text("app-title", "Representation")];
        items.push(PaneItem::button(
            cls(rep == Representation::Tile),
            "Show as tile".to_string(),
            format!("nodefacet:{member}:rep:tile"),
        ));
        items.push(PaneItem::button(
            cls(rep == Representation::Shape),
            "Show as shape".to_string(),
            format!("nodefacet:{member}:rep:shape"),
        ));

        // Size: the per-node size tier (the object card's size stepper, as a facets control).
        items.push(PaneItem::text("app-title", "Size"));
        items.push(PaneItem::text(
            "app-row",
            format!("{:.0} px (tier {})", orrery.node_size(key), orrery.node_size_tier(key)),
        ));
        items.push(PaneItem::button(
            "app-btn",
            "\u{2212} smaller".to_string(),
            format!("nodefacet:{member}:size:down"),
        ));
        items.push(PaneItem::button(
            "app-btn",
            "+ larger".to_string(),
            format!("nodefacet:{member}:size:up"),
        ));
        items
    }

    /// The `node:<id>/engine` page: the per-node engine pin picker (Auto + the pickable web
    /// engines), migrated from the context menu. Controls drain `nodefacet:<id>:engine:auto`
    /// / `:engine:pin:<engine_id>`. The web engines render only http/https, so a non-web node
    /// gets just Auto + a note. (Settings lane P3.)
    fn node_engine_items(&self, member: GraphMemberId) -> Vec<PaneItem> {
        const PICKABLE: &[(&str, &str)] = &[
            (inker::routing::ENGINE_SERVAL_WEB, "Serval (web)"),
            (inker::routing::ENGINE_SCRYING_WEB, "System WebView"),
        ];
        let pin = self.shared.content.engine_pins.get(&member).map(String::as_str);
        let is_web = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .and_then(|(_, n)| inker::routing::address_scheme(n.url()).map(str::to_string))
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https"));
        let cls = |on: bool| if on { "app-btn-active" } else { "app-btn" };
        let mut items = vec![PaneItem::text("app-title", "Engine")];
        items.push(PaneItem::button(
            cls(pin.is_none()),
            "Auto (default routing)".to_string(),
            format!("nodefacet:{member}:engine:auto"),
        ));
        if is_web {
            for &(id, name) in PICKABLE {
                if self.engine_available(id) {
                    items.push(PaneItem::button(
                        cls(pin == Some(id)),
                        format!("Open in {name}"),
                        format!("nodefacet:{member}:engine:pin:{id}"),
                    ));
                }
            }
        } else {
            items.push(PaneItem::text(
                "app-row-muted",
                "Engine choice applies to web (http/https) nodes.".to_string(),
            ));
        }
        items
    }

    /// Apply a `nodefacet:<id>:<action>` control key (the per-node facets pages' drain): parse
    /// the subject node id, then apply the action directly to it — engine pin / auto via
    /// `engine_pins`, representation via `set_node_representation` — the same underlying writes
    /// the context-menu pickers use, but targeting the facets tile's node. (Settings lane P3.)
    pub(crate) fn apply_node_facet_key(&mut self, key: &str) {
        let Some((id_str, action)) = key.split_once(':') else { return };
        let Ok(member) = id_str.parse::<GraphMemberId>() else { return };
        match action {
            "engine:auto" => {
                self.shared.content.engine_pins.remove(&member);
            }
            a if a.starts_with("engine:pin:") => {
                let engine = &a["engine:pin:".len()..];
                self.shared.content.engine_pins.insert(member, engine.to_string());
            }
            "rep:tile" => self.orrery_mut().set_node_representation(member, Representation::Tile),
            "rep:shape" => self.orrery_mut().set_node_representation(member, Representation::Shape),
            "size:up" => {
                self.orrery_mut().step_node_size_tier(member, 1);
            }
            "size:down" => {
                self.orrery_mut().step_node_size_tier(member, -1);
            }
            _ => return,
        }
        self.view.request_redraw();
    }
}
