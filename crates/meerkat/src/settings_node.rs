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

use mere::forme::GraphMemberId;
use mere::canvas::Face;

use crate::WindowCtx;
use crate::list_pane::PaneItem;
use crate::settings_lane::{SettingsPage, SettingsPageRef};

/// The pages the `node:<id>` provider serves, in spine order. (Settings lane P3.)
pub(crate) fn node_settings_index() -> Vec<SettingsPageRef> {
    vec![
        SettingsPageRef {
            id: "info",
            title: "Info",
        },
        SettingsPageRef {
            id: "appearance",
            title: "Appearance",
        },
        SettingsPageRef {
            id: "engine",
            title: "Engine",
        },
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
            format!("Face: {:?}", orrery.node_face(key)),
        ));
        let body = if orrery.node_sprite_hull(key).is_some() {
            "custom shape"
        } else {
            "standard"
        };
        items.push(PaneItem::text("app-row", format!("Body: {body}")));
        items.push(PaneItem::text(
            "app-row",
            format!("Size: {:.0} px", orrery.node_size(key)),
        ));
        items
    }

    /// The `node:<id>/appearance` page: the per-node **face** picker (Favicon / Sprite / Plain)
    /// plus a **reset shape** that drops a custom body hull, beside the swatch shape editor.
    /// Controls drain `nodefacet:<id>:face:<kind>` / `shape:reset`, applied by
    /// [`apply_node_facet_key`](Self::apply_node_facet_key) to this node. Face and body are
    /// independent axes; this is a per-node override. (Node body & face — Face axis.)
    fn node_appearance_items(&self, member: GraphMemberId) -> Vec<PaneItem> {
        let orrery = self.orrery();
        let Some((key, _)) = orrery.graph().get_node_by_id(member) else {
            return Vec::new();
        };
        let face = orrery.node_face(key);
        let has_sprite = orrery.node_sprite(key).is_some();
        let has_body = orrery.node_sprite_hull(key).is_some();
        // The Face axis is a single-selection picker (one active face).
        let mut items = vec![PaneItem::text("app-title", "Face")];
        items.push(PaneItem::radio(
            face == Face::Favicon,
            "Favicon".to_string(),
            format!("nodefacet:{member}:face:favicon"),
        ));
        items.push(PaneItem::radio(
            face == Face::Sprite,
            if has_sprite {
                "Sprite".to_string()
            } else {
                "Sprite (none)".to_string()
            },
            format!("nodefacet:{member}:face:sprite"),
        ));
        items.push(PaneItem::radio(
            face == Face::Bare,
            "Plain".to_string(),
            format!("nodefacet:{member}:face:bare"),
        ));
        // The body axis: author a custom hull from scratch, or reset to the content silhouette.
        // The swatch below edits its vertices (drag to move, click an edge to add, right-click to
        // remove). (Node body & face — the Body axis.)
        items.push(PaneItem::text("app-title", "Body"));
        if has_body {
            items.push(PaneItem::button(
                "app-btn",
                "Reset shape".to_string(),
                format!("nodefacet:{member}:shape:reset"),
            ));
        } else {
            items.push(PaneItem::button(
                "app-btn",
                "Add shape".to_string(),
                format!("nodefacet:{member}:shape:seed"),
            ));
        }

        // Size: the per-node size tier (the object card's size stepper, as a facets control).
        items.push(PaneItem::text("app-title", "Size"));
        items.push(PaneItem::text(
            "app-row",
            format!(
                "{:.0} px (tier {})",
                orrery.node_size(key),
                orrery.node_size_tier(key)
            ),
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

        // Material (the Body axis's physics): bounce / grip / weight, as steppers. A node can be
        // made heavier, bouncier, or grippier; the value persists in the sidecar. (Body & face.)
        let mat = orrery.node_material(key);
        let weight = mat.density / mere::canvas::NODE_BODY_DENSITY;
        items.push(PaneItem::text("app-title", "Material"));
        items.push(PaneItem::text(
            "app-row",
            format!(
                "Bounce {:.1} · Grip {:.1} · Weight {weight:.1}\u{00d7}",
                mat.restitution, mat.friction
            ),
        ));
        for (label, knob) in [("Bounce", "bounce"), ("Grip", "grip"), ("Weight", "mass")] {
            items.push(PaneItem::button(
                "app-btn",
                format!("\u{2212} {label}"),
                format!("nodefacet:{member}:material:{knob}:down"),
            ));
            items.push(PaneItem::button(
                "app-btn",
                format!("+ {label}"),
                format!("nodefacet:{member}:material:{knob}:up"),
            ));
        }
        items.push(PaneItem::button(
            "app-btn",
            "Reset material".to_string(),
            format!("nodefacet:{member}:material:reset"),
        ));
        items
    }

    /// The `node:<id>/engine` page: the per-node engine pin picker (Auto + the pickable web
    /// engines), migrated from the context menu. Controls drain `nodefacet:<id>:engine:auto`
    /// / `:engine:pin:<engine_id>`. The web engines render only http/https, so a non-web node
    /// gets just Auto + a note. (Settings lane P3.)
    fn node_engine_items(&self, member: GraphMemberId) -> Vec<PaneItem> {
        let mut pickable: Vec<(&str, &str)> = vec![
            (inker::routing::ENGINE_GENET_WEB, "Genet (web)"),
            (inker::routing::ENGINE_SCRYING_WEB, "System WebView"),
        ];
        // The scripted genet rung is pickable only in the `scripted` build. (Ladder.)
        #[cfg(feature = "scripted")]
        pickable.push((
            inker::routing::ENGINE_GENET_SCRIPTED,
            "Genet (scripted, Boa)",
        ));
        #[cfg(feature = "scripted-nova")]
        pickable.push((
            inker::routing::ENGINE_GENET_SCRIPTED_NOVA,
            "Genet (scripted, Nova)",
        ));
        let pin = self
            .shared
            .content
            .engine_pins
            .get(&member)
            .map(String::as_str);
        let is_web = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .and_then(|(_, n)| inker::routing::address_scheme(n.url()).map(str::to_string))
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https"));
        // The engine pin is a single-selection picker (Auto, or one pinned engine).
        let mut items = vec![PaneItem::text("app-title", "Engine")];
        items.push(PaneItem::radio(
            pin.is_none(),
            "Auto (default routing)".to_string(),
            format!("nodefacet:{member}:engine:auto"),
        ));
        if is_web {
            for &(id, name) in &pickable {
                if self.engine_available(id) {
                    items.push(PaneItem::radio(
                        pin == Some(id),
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
    /// `engine_pins`, face via `set_node_face`, body reset via `clear_node_body` — the same
    /// underlying writes the context-menu pickers use, but targeting the facets tile's node.
    /// (Settings lane P3; node body & face.)
    pub(crate) fn apply_node_facet_key(&mut self, key: &str) {
        let Some((id_str, action)) = key.split_once(':') else {
            return;
        };
        let Ok(member) = id_str.parse::<GraphMemberId>() else {
            return;
        };
        match action {
            "engine:auto" => {
                self.shared.content.engine_pins.remove(&member);
            }
            a if a.starts_with("engine:pin:") => {
                let engine = &a["engine:pin:".len()..];
                self.shared
                    .content
                    .engine_pins
                    .insert(member, engine.to_string());
            }
            "face:favicon" => self.orrery_mut().set_node_face(member, Face::Favicon),
            "face:sprite" => self.orrery_mut().set_node_face(member, Face::Sprite),
            "face:bare" => self.orrery_mut().set_node_face(member, Face::Bare),
            "shape:reset" => self.orrery_mut().clear_node_body(member),
            "shape:seed" => self.orrery_mut().seed_node_body(member),
            "size:up" => {
                self.orrery_mut().step_node_size_tier(member, 1);
            }
            "size:down" => {
                self.orrery_mut().step_node_size_tier(member, -1);
            }
            "material:reset" => self.orrery_mut().clear_node_material(member),
            a if a.starts_with("material:") => {
                self.step_node_material(member, &a["material:".len()..])
            }
            _ => return,
        }
        // Body + face edits (hull shape, material, face) persist in the cartography sidecar:
        // save now so an edit survives a reload even on a non-graceful exit, like the swatch drag.
        // (Node body & face.)
        if action.starts_with("shape:")
            || action.starts_with("material:")
            || action.starts_with("face:")
        {
            self.save_session();
        }
        self.view.request_redraw();
    }

    /// Step one knob of a node's physical material (the facets `material:<knob>:<dir>` controls):
    /// bounce / grip by ±0.1 (clamped), weight by ×/÷1.5 (clamped to 0.25..8× the default
    /// density). Reads the current material, adjusts, and pushes it live. (Node body & face.)
    fn step_node_material(&mut self, member: GraphMemberId, knob: &str) {
        let Some((key, _)) = self.orrery().graph().get_node_by_id(member) else {
            return;
        };
        let mut mat = self.orrery().node_material(key);
        let round1 = |x: f32| (x * 10.0).round() / 10.0;
        match knob {
            "bounce:up" => mat.restitution = round1((mat.restitution + 0.1).min(1.0)),
            "bounce:down" => mat.restitution = round1((mat.restitution - 0.1).max(0.0)),
            "grip:up" => mat.friction = round1((mat.friction + 0.1).min(2.0)),
            "grip:down" => mat.friction = round1((mat.friction - 0.1).max(0.0)),
            "mass:up" => mat.density = (mat.density * 1.5).min(mere::canvas::NODE_BODY_DENSITY * 8.0),
            "mass:down" => mat.density = (mat.density / 1.5).max(mere::canvas::NODE_BODY_DENSITY * 0.25),
            _ => return,
        }
        self.orrery_mut().set_node_material(member, mat);
    }
}
