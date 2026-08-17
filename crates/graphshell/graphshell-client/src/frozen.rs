//! The frozen realization: a scene as navigable semantics.
//!
//! A projection has two realizations, and only one of them is usually built.
//! The interactive one is a canvas a person points at. The frozen one is what
//! remains when the pixels are unavailable or unusable: a screen reader, a
//! text-only client, a printed page, a receipt in a proof. None of the
//! visualization grammars surveyed for the projection grammar report treat that
//! second form as a first-class target, which is why this is built from the W3C
//! anatomy instead: WAI's guidance for complex images, Graphics ARIA's roles,
//! and SVG's structural title/desc pairing.
//!
//! This module produces a *structure*, not markup. A host renders it into its
//! own accessibility surface, whether that is a genet DOM tree, an AccessKit
//! node tree, or an HTML table, and the same receipt can be asserted in a test
//! without a browser. That also keeps a DOM engine out of the client.
//!
//! Two facts about the contract shape this had to work around, both worth
//! stating where a reader meets them:
//!
//! 1. **A scene carries no names.** [`ProjectedItem`] identifies itself with a
//!    [`SourceRef`], which is an address, not a label. Names live on the
//!    protocol's presentation plane, in `chirograph::PresentationSemantics`,
//!    so a caller supplies them here and the source id is the fallback. A
//!    fallback name is honest but poor, and [`FrozenScene::unnamed`] counts
//!    them so a receipt can say how much of the scene was legible.
//! 2. **Relations name instances, not sources.** They are resolved to names
//!    here so the frozen form never asks a reader to follow an index.

use std::collections::HashMap;

use sceno::{HeldPlacement, InstanceId, Representation, Scene, SourceRef};
use serde::{Deserialize, Serialize};

/// What kind of thing a reader is being told about.
///
/// Deliberately coarser than [`Representation`]: a reader cares whether an
/// entry is a symbol, a piece of content, or live, not which rung the host
/// picked to draw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrozenRole {
    /// A themed mark standing for a thing: Graphics ARIA `graphics-symbol`.
    Symbol,
    /// Content with its own substance: a card, an image, a capture.
    Object,
    /// Live content whose frozen form is necessarily a description of
    /// something that was moving.
    LiveContent,
}

impl FrozenRole {
    fn of(representation: &Representation) -> Self {
        match representation {
            Representation::Glyph => Self::Symbol,
            Representation::Card | Representation::Sprite | Representation::Snapshot => {
                Self::Object
            }
            Representation::LivePane => Self::LiveContent,
            // An unrecognized rung is content until a host says otherwise;
            // calling it a symbol would understate it.
            Representation::Open { .. } => Self::Object,
        }
    }
}

/// One entry a reader can navigate to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrozenInstance {
    pub instance: InstanceId,
    pub source: SourceRef,
    /// The supplied name, or the source id when none was supplied.
    pub name: String,
    /// True when `name` fell back to the source id.
    pub named_by_fallback: bool,
    pub role: FrozenRole,
}

/// One relation, resolved to the names at both ends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrozenRelation {
    pub from: String,
    pub to: String,
    /// The relation's own word for itself, when it has one.
    pub kind: Option<String>,
}

/// A scene rendered as navigable semantics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrozenScene {
    /// What the whole projection is, for the document-level label.
    pub name: String,
    /// The long-form alternate WAI asks for when the visual alone is
    /// insufficient. Generated from the scene's own counts rather than
    /// invented, so it cannot drift from what is listed below it.
    pub summary: String,
    pub instances: Vec<FrozenInstance>,
    pub relations: Vec<FrozenRelation>,
    /// Placements the solver could not honor, carried into the accessible form
    /// because "this was asked to sit here and does not" is exactly the kind of
    /// fact a visual reader gets for free and a screen-reader user does not.
    pub unmet_holds: Vec<HeldPlacement>,
    /// How many instances are named only by their source id.
    pub unnamed: usize,
}

impl FrozenScene {
    /// Freeze `scene`, taking names from `names` and falling back to source ids.
    ///
    /// Invisible items are omitted: the frozen form describes what the scene
    /// presents, and an item the interactive realization does not draw is not
    /// something a reader is missing.
    pub fn freeze(scene: &Scene, name: &str, names: &HashMap<SourceRef, String>) -> Self {
        let mut instances = Vec::new();
        let mut name_by_instance = HashMap::new();
        let mut unnamed = 0;

        for (index, item) in scene.items.iter().enumerate() {
            if !item.visible {
                continue;
            }
            let Some(source) = scene.sources.get(item.source.0 as usize) else {
                continue;
            };
            let supplied = names.get(source);
            if supplied.is_none() {
                unnamed += 1;
            }
            let resolved = supplied.cloned().unwrap_or_else(|| source.id.clone());
            let instance = InstanceId(index as u32);
            name_by_instance.insert(instance, resolved.clone());
            instances.push(FrozenInstance {
                instance,
                source: source.clone(),
                name: resolved,
                named_by_fallback: supplied.is_none(),
                role: FrozenRole::of(&item.representation),
            });
        }

        let relations = scene
            .relations
            .iter()
            .filter_map(|relation| {
                Some(FrozenRelation {
                    from: name_by_instance.get(&relation.from)?.clone(),
                    to: name_by_instance.get(&relation.to)?.clone(),
                    kind: relation.kind.clone(),
                })
            })
            .collect::<Vec<_>>();

        let summary = summarize(instances.len(), relations.len(), scene.unmet_holds.len());

        Self {
            name: name.to_owned(),
            summary,
            instances,
            relations,
            unmet_holds: scene.unmet_holds.clone(),
            unnamed,
        }
    }

    /// The tabular alternate: one row per instance, then one per relation.
    ///
    /// Rows are `(kind, name, detail)` so a host can lay them out as a table, a
    /// definition list, or read them aloud in order without re-deriving
    /// anything.
    pub fn rows(&self) -> Vec<(String, String, String)> {
        let mut rows = Vec::with_capacity(self.instances.len() + self.relations.len());
        for instance in &self.instances {
            rows.push((
                "instance".to_owned(),
                instance.name.clone(),
                match instance.role {
                    FrozenRole::Symbol => "symbol".to_owned(),
                    FrozenRole::Object => "object".to_owned(),
                    FrozenRole::LiveContent => "live content".to_owned(),
                },
            ));
        }
        for relation in &self.relations {
            rows.push((
                "relation".to_owned(),
                format!("{} to {}", relation.from, relation.to),
                relation.kind.clone().unwrap_or_else(|| "related".to_owned()),
            ));
        }
        for held in &self.unmet_holds {
            rows.push((
                "unmet placement".to_owned(),
                held.source.id.clone(),
                format!("asked for ({}, {})", held.at.x, held.at.y),
            ));
        }
        rows
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

fn summarize(instances: usize, relations: usize, unmet: usize) -> String {
    let mut summary = format!(
        "{} and {}.",
        plural(instances, "item", "items"),
        plural(relations, "relationship", "relationships")
    );
    if unmet > 0 {
        summary.push(' ');
        summary.push_str(&format!(
            "{} could not be placed where it was asked to sit.",
            plural(unmet, "placement", "placements")
        ));
    }
    summary
}

/// Escape the five characters that would otherwise change the tree's shape.
///
/// Names arrive from an adapter and may contain anything; a projection whose
/// accessible form can be broken by a node called `<b>` is not accessible.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

impl FrozenRole {
    /// The Graphics ARIA role a host should carry into its accessibility tree.
    pub fn aria_role(self) -> &'static str {
        match self {
            // A mark standing for a thing, with no internal structure to explore.
            Self::Symbol => "graphics-symbol",
            // Content with substance. Live content freezes into a description
            // of something that was moving, which is still an object, so the
            // distinction is carried in the text rather than by inventing a
            // role the specification does not define.
            Self::Object | Self::LiveContent => "graphics-object",
        }
    }
}

impl FrozenScene {
    /// Render the frozen realization as semantic markup.
    ///
    /// The anatomy is the catalog's own W3C citations rather than anything
    /// invented here: a `graphics-document` root, SVG's title/desc pairing
    /// expressed as `aria-labelledby` and `aria-describedby`, Graphics ARIA
    /// roles per instance, and WAI's long-form alternate as a real table for
    /// the case where the visual alone is insufficient.
    ///
    /// A string rather than a live tree on purpose: this crate stays free of a
    /// DOM engine, and every host that has one can parse it. Relations are
    /// listed as text at both ends because a reader following a relation needs
    /// names, not indices.
    pub fn to_html(&self, id_prefix: &str) -> String {
        let name_id = format!("{id_prefix}-name");
        let summary_id = format!("{id_prefix}-summary");
        let mut html = String::new();

        html.push_str(&format!(
            "<figure role=\"graphics-document\" aria-labelledby=\"{}\" aria-describedby=\"{}\">",
            escape(&name_id),
            escape(&summary_id)
        ));
        html.push_str(&format!(
            "<figcaption id=\"{}\">{}</figcaption>",
            escape(&name_id),
            escape(&self.name)
        ));
        html.push_str(&format!(
            "<p id=\"{}\">{}</p>",
            escape(&summary_id),
            escape(&self.summary)
        ));

        html.push_str("<ul class=\"frozen-instances\">");
        for instance in &self.instances {
            html.push_str(&format!(
                "<li role=\"{}\" aria-label=\"{}\" data-source-id=\"{}\">{}</li>",
                instance.role.aria_role(),
                escape(&instance.name),
                escape(&instance.source.id),
                escape(&instance.name)
            ));
        }
        html.push_str("</ul>");

        if !self.relations.is_empty() {
            html.push_str("<ul class=\"frozen-relations\">");
            for relation in &self.relations {
                let kind = relation.kind.clone().unwrap_or_else(|| "related to".to_owned());
                html.push_str(&format!(
                    "<li>{} {} {}</li>",
                    escape(&relation.from),
                    escape(&kind),
                    escape(&relation.to)
                ));
            }
            html.push_str("</ul>");
        }

        // The long-form alternate. Rendered unconditionally: it is the form a
        // reader falls back to, so making it conditional on complexity would
        // mean guessing when someone needs it.
        html.push_str("<table class=\"frozen-alternate\">");
        html.push_str("<caption>Every item and relationship in this projection</caption>");
        html.push_str("<thead><tr><th scope=\"col\">Kind</th><th scope=\"col\">Name</th><th scope=\"col\">Detail</th></tr></thead><tbody>");
        for (kind, name, detail) in self.rows() {
            html.push_str(&format!(
                "<tr><td>{}</td><th scope=\"row\">{}</th><td>{}</td></tr>",
                escape(&kind),
                escape(&name),
                escape(&detail)
            ));
        }
        html.push_str("</tbody></table></figure>");
        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{Footprint, ProjectedItem, RoutedRelation, Score, Transform2, Vec2};

    /// The P5 fixture, solved: a real scene rather than a hand-built stand-in.
    fn coastal() -> Scene {
        let score: Score = serde_json::from_str(include_str!(
            "../../../scenograph/scenomise/fixtures/coastal_map.json"
        ))
        .expect("the coastal map fixture parses");
        scenomise::solve(&score)
    }

    fn named(pairs: &[(&str, &str, &str)]) -> HashMap<SourceRef, String> {
        pairs
            .iter()
            .map(|(adapter, id, name)| (SourceRef::new(*adapter, *id), (*name).to_owned()))
            .collect()
    }

    #[test]
    fn a_frozen_scene_enumerates_every_visible_instance_by_name() {
        let scene = coastal();
        let names = named(&[
            ("fixture.map", "harbor", "Harbor"),
            ("fixture.map", "beacon", "Beacon"),
        ]);
        let frozen = FrozenScene::freeze(&scene, "Coastal map", &names);

        let visible = scene.items.iter().filter(|item| item.visible).count();
        assert_eq!(frozen.instances.len(), visible, "every visible item is listed");
        assert!(frozen.instances.iter().any(|i| i.name == "Harbor"));
        assert!(frozen.instances.iter().any(|i| i.name == "Beacon"));
        // The underlay had no supplied name, so it falls back and is counted.
        assert!(frozen.unnamed > 0);
        assert!(
            frozen
                .instances
                .iter()
                .any(|i| i.named_by_fallback && i.name == "coastal-outline")
        );
    }

    #[test]
    fn the_summary_counts_what_the_listing_shows() {
        let scene = coastal();
        let frozen = FrozenScene::freeze(&scene, "Coastal map", &HashMap::new());
        // The alternate must not be able to drift from the enumeration under it.
        assert!(frozen.summary.contains(&format!("{} items", frozen.instances.len())));
        assert!(frozen.instances.iter().all(|i| i.named_by_fallback));
        assert_eq!(frozen.unnamed, frozen.instances.len());
    }

    #[test]
    fn an_invisible_item_is_not_something_a_reader_is_missing() {
        let mut scene = coastal();
        let before = FrozenScene::freeze(&scene, "Coastal map", &HashMap::new())
            .instances
            .len();
        scene.items[0].visible = false;
        let after = FrozenScene::freeze(&scene, "Coastal map", &HashMap::new());
        assert_eq!(after.instances.len(), before - 1);
    }

    #[test]
    fn relations_are_resolved_to_names_not_indices() {
        let mut scene = Scene::new();
        for id in ["north", "south"] {
            let source = scene.intern_source(SourceRef::new("fixture", id));
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(0.0, 0.0),
                footprint: Footprint::Point,
                representation: Representation::Glyph,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });
        }
        scene.relations.push(RoutedRelation {
            from: InstanceId(0),
            to: InstanceId(1),
            space: Scene::WORLD,
            points: vec![Vec2::ZERO, Vec2::ZERO],
            kind: Some("feeds".to_owned()),
            weight: None,
        });

        let frozen = FrozenScene::freeze(
            &scene,
            "Two stations",
            &named(&[("fixture", "north", "North station"), ("fixture", "south", "South station")]),
        );
        assert_eq!(frozen.relations.len(), 1);
        assert_eq!(frozen.relations[0].from, "North station");
        assert_eq!(frozen.relations[0].to, "South station");
        assert_eq!(frozen.relations[0].kind.as_deref(), Some("feeds"));
    }

    #[test]
    fn an_unmet_placement_reaches_the_reader_too() {
        // A sighted reader can see a pin sitting in the wrong place. Without
        // this the frozen form is the one realization that cannot say so.
        let mut scene = coastal();
        scene.unmet_holds.push(sceno::HeldPlacement::pinned(
            SourceRef::new("fixture.map", "ghost"),
            Vec2::new(3.0, 4.0),
        ));
        let frozen = FrozenScene::freeze(&scene, "Coastal map", &HashMap::new());
        assert_eq!(frozen.unmet_holds.len(), 1);
        assert!(frozen.summary.contains("could not be placed"));
        let row = frozen
            .rows()
            .into_iter()
            .find(|(kind, _, _)| kind == "unmet placement")
            .expect("the violation is in the tabular alternate");
        assert_eq!(row.1, "ghost");
        assert!(row.2.contains("(3, 4)"));
    }

    #[test]
    fn the_frozen_form_is_a_real_tree_a_host_can_traverse() {
        use genet_scripted_dom::ScriptedDom;
        use layout_dom_api::LayoutDom;

        let scene = coastal();
        let names = named(&[("fixture.map", "harbor", "Harbor")]);
        let frozen = FrozenScene::freeze(&scene, "Coastal map", &names);
        let html = frozen.to_html("coastal");

        // Parsed, not string-matched: a markup bug that a regex would miss
        // becomes a tree that does not contain what it should.
        let dom = ScriptedDom::from_serialized_document(&format!(
            "<!doctype html><html><body>{html}</body></html>"
        ));
        let serialized = dom.inner_html(dom.document());

        assert!(serialized.contains("graphics-document"), "document role survives parsing");
        assert!(serialized.contains("graphics-symbol") || serialized.contains("graphics-object"));
        assert!(serialized.contains("Harbor"));
        assert!(serialized.contains("Every item and relationship in this projection"));
        // One row per instance plus the header row.
        assert_eq!(
            serialized.matches("<tr").count(),
            frozen.rows().len() + 1,
            "every row reached the tree"
        );
    }

    #[test]
    fn the_document_names_and_describes_itself() {
        let frozen = FrozenScene::freeze(&coastal(), "Coastal map", &HashMap::new());
        let html = frozen.to_html("coastal");
        // SVG's title/desc pairing, expressed the way ARIA carries it.
        assert!(html.contains("aria-labelledby=\"coastal-name\""));
        assert!(html.contains("aria-describedby=\"coastal-summary\""));
        assert!(html.contains("id=\"coastal-name\">Coastal map<"));
        assert!(html.contains(&frozen.summary));
    }

    #[test]
    fn every_instance_carries_a_role_and_a_label() {
        let frozen = FrozenScene::freeze(&coastal(), "Coastal map", &HashMap::new());
        let html = frozen.to_html("coastal");
        assert_eq!(
            html.matches("aria-label=").count(),
            frozen.instances.len(),
            "no instance reaches a reader unlabelled"
        );
        assert_eq!(html.matches("role=\"graphics-").count(), frozen.instances.len() + 1);
    }

    #[test]
    fn a_hostile_name_cannot_reshape_the_tree() {
        use genet_scripted_dom::ScriptedDom;
        use layout_dom_api::LayoutDom;

        // Names come from an adapter and may contain anything. A projection
        // whose accessible form can be broken by a node called `<script>` is
        // not accessible, and is a hole besides.
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("fixture", "x"));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(0.0, 0.0),
            footprint: Footprint::Point,
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        let hostile = "</li></ul><script>alert(1)</script>";
        let frozen = FrozenScene::freeze(
            &scene,
            "Hostile",
            &named(&[("fixture", "x", hostile)]),
        );
        let html = frozen.to_html("h");
        assert!(!html.contains("<script>"), "the tag never survives as markup");
        assert!(html.contains("&lt;script&gt;"), "it survives as text");

        let dom = ScriptedDom::from_serialized_document(&format!(
            "<!doctype html><html><body>{html}</body></html>"
        ));
        let serialized = dom.inner_html(dom.document());
        assert!(
            !serialized.contains("<script>"),
            "and the parser agrees, which is the check that matters"
        );
    }

    #[test]
    fn freezing_is_deterministic() {
        let scene = coastal();
        let names = named(&[("fixture.map", "harbor", "Harbor")]);
        let once = FrozenScene::freeze(&scene, "Coastal map", &names);
        let twice = FrozenScene::freeze(&scene, "Coastal map", &names);
        assert_eq!(once, twice, "a receipt that varies is not a receipt");
        assert_eq!(once.rows(), twice.rows());
    }
}
