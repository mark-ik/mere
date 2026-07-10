/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Visual couplings → paint overlays: the paint-side consumer of the field
//! system's open response tail.
//!
//! This is the aether→platen seam, the mirror of the aether→seiche
//! [`seiche::CouplingForce`] on the force side. The kernel's response vocabulary is
//! recognized-core-plus-open-tail: the six force responses are seiche's; everything
//! else rides as `CouplingResponse::Open { predicate }` under
//! [`kernel::graph::COUPLING_VOCAB`]. This module is the consumer that recognizes
//! the `visual/*` slice of that tail.
//!
//! Platen owns the visual vocabulary; the kernel stays force-core-only. A
//! `visual/*` IRI platen does not recognize is simply skipped — never dropped
//! from the graph, never breaking anything — per the
//! [statements-over-schema stance](../../../../design_docs/mere_docs/technical_architecture/2026-05-22_statements_over_schema_stance.md):
//! recognized behavior lives in the lens, not the substrate.
//!
//! Resolution mirrors [`seiche::CouplingForce::from_coupling`]: look up the field
//! ([`Graph::field`](kernel::graph::Graph::field)), the targets
//! ([`Graph::nodes_matching`](kernel::graph::Graph::nodes_matching)), seed the
//! registry from [`Graph::fields`](kernel::graph::Graph::fields) so `Sample`
//! resolves, evaluate the scalar field at each target's projected position, and
//! map field value × strength to an overlay's intensity.

use std::collections::HashMap;

use quint::{FieldRegistry, eval_scalar};
use cartography::Projection;
use kernel::geometry::PortablePoint;
use kernel::graph::{COUPLING_VOCAB, FieldDefinition, Graph, NodeKey};
use paint_list_api::{
    ColorF, CommonPlacement, DeviceIntSize, LayoutPoint, LayoutRect, PaintCmd, RectItem,
};

use crate::scene_paint::{Camera, CanvasPaintList, ScenePaintStyle, paint_projection};

/// A `visual/*` response platen recognizes. The kernel keeps these as open IRIs;
/// platen owns their meaning here. Unrecognized `visual/*` IRIs are skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognizedVisual {
    /// A glow around the node, growing in size and opacity with the field value.
    Halo,
    /// A color wash over the node, rising in opacity with the field value.
    Tint,
}

impl RecognizedVisual {
    /// The recognized visual response for a coupling's open predicate IRI, if any.
    /// `None` for a non-visual or unrecognized predicate.
    pub fn from_predicate(predicate: &str) -> Option<Self> {
        match predicate.strip_prefix(COUPLING_VOCAB)? {
            "visual/halo" => Some(Self::Halo),
            "visual/tint" => Some(Self::Tint),
            _ => None,
        }
    }
}

/// Resolve the graph's recognized `visual/*` couplings into world-space overlay
/// paint commands, evaluating each coupling's field at its targets' projected
/// positions. The commands are in world space (the same space the projection
/// paints in); merge them inside the scene's camera transform — see
/// [`paint_projection_with_visuals`], or [`CanvasPaintList::splice_world_overlays`].
///
/// Skipped (never an error): a recognized force-core response (seiche's), an
/// unrecognized `visual/*` IRI, an unknown or non-scalar field, a target absent
/// from this projection, or a non-positive resolved intensity.
pub fn visual_overlays(
    graph: &Graph,
    projection: &Projection,
    style: &ScenePaintStyle,
) -> Vec<PaintCmd> {
    // Projected position + drawn radius per node (the coords the scene paints in).
    let placed: HashMap<NodeKey, (PortablePoint, f32)> = projection
        .nodes
        .iter()
        .map(|n| {
            let r = if n.radius > 0.0 {
                n.radius
            } else {
                style.default_node_radius
            };
            (n.node, (n.position, r))
        })
        .collect();

    // Seed the registry from the whole field layer so inter-field `Sample`
    // references resolve (mirrors `CouplingForce::from_coupling`).
    let mut registry = FieldRegistry::new();
    for f in graph.fields() {
        registry.insert_with_id(f.id, f.definition.clone());
    }

    let mut cmds = Vec::new();
    for coupling in graph.couplings() {
        // Only open-tail responses carry a predicate; the force core is seiche's.
        let Some(visual) = coupling
            .response
            .predicate()
            .and_then(RecognizedVisual::from_predicate)
        else {
            continue;
        };
        let Some(field) = graph.field(coupling.field) else {
            continue;
        };
        let FieldDefinition::Scalar(scalar) = &field.definition else {
            continue; // visual intensity is scalar-valued in v1
        };
        for node in graph.nodes_matching(&coupling.selector) {
            let Some(&(pos, radius)) = placed.get(&node) else {
                continue;
            };
            let value = eval_scalar(scalar, &registry, pos.x, pos.y, 0.0);
            let intensity = (value * coupling.strength).clamp(0.0, 1.0);
            if intensity <= f32::EPSILON {
                continue;
            }
            cmds.push(overlay_rect(visual, pos, radius, intensity, style));
        }
    }
    cmds
}

/// One overlay rect for a resolved visual response. Colors are fixed for now
/// (theming via `register-theme` lands later); `intensity ∈ (0, 1]` drives the
/// alpha, and the halo's size.
fn overlay_rect(
    visual: RecognizedVisual,
    center: PortablePoint,
    radius: f32,
    intensity: f32,
    style: &ScenePaintStyle,
) -> PaintCmd {
    let (extent, color) = match visual {
        // Glow: a translucent square larger than the node, growing with intensity.
        RecognizedVisual::Halo => (
            radius * (1.0 + intensity),
            ColorF::new(1.0, 0.93, 0.6, 0.6 * intensity),
        ),
        // Tint: a node-sized wash in the node's color at rising opacity.
        RecognizedVisual::Tint => (
            radius,
            ColorF::new(
                style.node_color.r,
                style.node_color.g,
                style.node_color.b,
                intensity,
            ),
        ),
    };
    let bounds = LayoutRect::new(
        LayoutPoint::new(center.x - extent, center.y - extent),
        LayoutPoint::new(center.x + extent, center.y + extent),
    );
    PaintCmd::DrawRect(RectItem {
        placement: CommonPlacement::new(bounds),
        color,
    })
}

/// [`paint_projection`] plus the graph's recognized visual-coupling overlays,
/// spliced inside the scene's camera transform. The host-facing entry when the
/// field layer carries visual couplings; overlays paint over the nodes (true
/// behind-node layering for halos is a later refinement).
pub fn paint_projection_with_visuals(
    graph: &Graph,
    projection: &Projection,
    viewport: DeviceIntSize,
    camera: Camera,
    style: &ScenePaintStyle,
    generation: u64,
) -> CanvasPaintList {
    let mut list = paint_projection(projection, viewport, camera, style, generation);
    list.splice_world_overlays(visual_overlays(graph, projection, style));
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartography::projection::PositionedNode;
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::{
        Coupling, CouplingId, CouplingResponse, Field, FieldDefinition, FieldId, NodeSelector,
        ScalarField,
    };
    use uuid::Uuid;

    fn halo_iri() -> String {
        format!("{COUPLING_VOCAB}visual/halo")
    }

    // A graph with one node at the origin and a Gaussian field peaking there
    // (value ≈ 1 at the node), plus a projection placing that node at the origin.
    fn graph_field_and_projection(response: CouplingResponse) -> (Graph, Projection) {
        let mut g = Graph::new();
        let node = g.add_node_with_id(
            Uuid::from_u128(1),
            "mere://a".into(),
            PortablePoint::new(0.0, 0.0),
        );
        let fid = FieldId::from_uuid(Uuid::from_u128(0xF1));
        g.add_field(Field::new(
            fid,
            FieldDefinition::Scalar(ScalarField::gaussian_at(0.0, 0.0, 50.0)),
        ));
        g.add_coupling(Coupling::new(
            CouplingId::from_uuid(Uuid::from_u128(1)),
            fid,
            NodeSelector::All,
            response,
            1.0,
        ));
        let projection = Projection {
            nodes: vec![PositionedNode {
                node,
                position: PortablePoint::new(0.0, 0.0),
                radius: 10.0,
            }],
            ..Projection::empty()
        };
        (g, projection)
    }

    fn rect_alpha(cmd: &PaintCmd) -> f32 {
        match cmd {
            PaintCmd::DrawRect(r) => r.color.a,
            other => panic!("expected DrawRect, got {other:?}"),
        }
    }

    #[test]
    fn recognized_visual_emits_overlay_scaled_by_field() {
        let (g, projection) = graph_field_and_projection(CouplingResponse::open(halo_iri()));
        let style = ScenePaintStyle::default();
        let overlays = visual_overlays(&g, &projection, &style);
        assert_eq!(overlays.len(), 1, "one halo over the targeted node");
        // Gaussian ≈ 1 at the peak, strength 1 → halo alpha = 0.6 * 1.
        let a = rect_alpha(&overlays[0]);
        assert!(
            (a - 0.6).abs() < 1.0e-3,
            "alpha tracks the field value: {a}"
        );
    }

    #[test]
    fn force_core_response_is_not_a_visual_overlay() {
        // A force coupling carries no predicate; the visual pass ignores it (it is
        // seiche's). No overlays.
        let (g, projection) = graph_field_and_projection(CouplingResponse::AttractToMin);
        assert!(visual_overlays(&g, &projection, &ScenePaintStyle::default()).is_empty());
    }

    #[test]
    fn unrecognized_visual_iri_is_skipped() {
        // An open response under a visual IRI platen does not know: skipped, not an error.
        let (g, projection) = graph_field_and_projection(CouplingResponse::open(format!(
            "{COUPLING_VOCAB}visual/sparkle"
        )));
        assert!(visual_overlays(&g, &projection, &ScenePaintStyle::default()).is_empty());
    }

    #[test]
    fn target_far_from_field_gets_no_overlay() {
        // Same field + halo coupling, but the node sits far from the Gaussian peak,
        // so the resolved intensity is ~0 and nothing is drawn.
        let (g, _) = graph_field_and_projection(CouplingResponse::open(halo_iri()));
        let node = g.nodes().next().unwrap().0;
        let far = Projection {
            nodes: vec![PositionedNode {
                node,
                position: PortablePoint::new(5_000.0, 0.0),
                radius: 10.0,
            }],
            ..Projection::empty()
        };
        assert!(visual_overlays(&g, &far, &ScenePaintStyle::default()).is_empty());
    }

    #[test]
    fn with_visuals_splices_overlay_inside_the_transform() {
        let (g, projection) = graph_field_and_projection(CouplingResponse::open(halo_iri()));
        let base = paint_projection(
            &projection,
            DeviceIntSize::new(100, 100),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        let with = paint_projection_with_visuals(
            &g,
            &projection,
            DeviceIntSize::new(100, 100),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        );
        use paint_list_api::PaintList;
        assert_eq!(
            with.commands().len(),
            base.commands().len() + 1,
            "exactly the one halo overlay was spliced in"
        );
        // ...and it landed inside the transform (last command is still PopTransform).
        assert!(matches!(
            with.commands().last(),
            Some(PaintCmd::PopTransform)
        ));
    }
}
