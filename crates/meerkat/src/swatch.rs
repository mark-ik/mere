/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The **swatch**: a portable, chrome-understood representation of a graph element (gloss
//! design §2a). It renders as **DOM** — serval lays it out, themes it, hit-tests it — not an
//! opaque `netrender::Scene`, so it can embed anywhere a graph element wants representing (a
//! node facet pane, a menu, a djot script block, an orrery card).
//!
//! First embedding: a **single node** scoped swatch — the node's sprite face plus its editable
//! collider hull (the Body axis) — in the `node:<id>/appearance` facet pane (the shape editor).
//! It renders the sprite (optional, a tracing underlay) + the hull polygon + a dot per vertex,
//! and is a full **body designer**: the host hit-tests the swatch through the chrome session
//! (the object-card press-gate pattern), walks up to the `node-swatch` container, reads its
//! `data-subject`, and drives editing from the cursor (serval has no native DOM pointer-drag):
//! drag a vertex to move it, click a hull edge to add a corner, right-click a vertex to remove
//! it. A node with no sprite can seed a default hull and shape it from scratch. The view is
//! now **host-generic** over the embedder state `S` (swatch-primitive plan P1): the facet pane
//! mounts it as `SwatchView<SettingsPanesState>`, and the focus-card slot mounts the same view
//! for the connections swatch (P2). Generalizing was a type-parameterization, since the swatch
//! carries no state-bound view callbacks. (Node body & face — the shape editor; swatch template #1.)

use mere::kernel::graph::{EdgeFamily, RelationKind};
use xilem_serval::{AnyView, ServalCtx, ServalElement, el};

/// What a node swatch shows: the sprite face (a PNG data-URI) and its collider hull (the
/// opaque-region convex polygon in face-normalized coords, `[-0.5, 0.5]`). A node without a
/// sprite renders an empty swatch (the silhouette case joins later).
pub(crate) struct SwatchSpec {
    pub sprite: Option<String>,
    pub hull: Vec<(f32, f32)>,
    /// The graph node this swatch is scoped to, when it is editable: emitted as the
    /// container's `data-subject` so the host vertex-drag knows whose hull to rewrite.
    /// `None` for a non-node or read-only swatch. (Swatch — Stage B.)
    pub subject: Option<uuid::Uuid>,
}

/// The swatch's on-screen edge length (px) in the facet pane.
const SWATCH: f32 = 220.0;
/// A vertex handle's diameter (px).
const HANDLE: f32 = 12.0;

/// Map a face-normalized coord (`-0.5..=0.5`) to a swatch-local pixel (`0..=SWATCH`).
pub(crate) fn norm_to_swatch_px(n: f32) -> f32 {
    (n + 0.5) * SWATCH
}

/// The swatch's edge length (px), so the host hit-test can place the swatch rect. (Stage B.)
pub(crate) fn swatch_edge_px() -> f32 {
    SWATCH
}

/// A host-generic swatch view: the chrome-understood DOM subtree an embedder mounts, erased over
/// the embedder state `S`. The swatch has no state-bound view callbacks (interaction routes through
/// the host hit-test on `data-subject`), so it is generic over any `S: 'static` — the facet pane
/// and the focus-card slot mount the same view. (Swatch primitive — P1 host-generic lift.)
pub(crate) type SwatchView<S> = Box<dyn AnyView<S, (), ServalCtx, ServalElement>>;

/// Build the node swatch as DOM: the sprite image, its collider hull as a translucent
/// clip-path polygon over it, and a dot at each hull vertex. Read-only (Stage A). The hull
/// is mapped from normalized `[-0.5, 0.5]` to the swatch's `0..100%` (clip-path) / `0..SWATCH`
/// (dots). (Swatch — node shape editor; the first swatch template.)
pub(crate) fn swatch_view<S: 'static>(spec: &SwatchSpec) -> SwatchView<S> {
    let mut children: Vec<SwatchView<S>> = Vec::new();

    // The sprite fills the swatch (cover-fit) — the surface the hull is traced over.
    if let Some(uri) = &spec.sprite {
        children.push(Box::new(
            el::<_, S, ()>("img", ()).attr("src", uri.clone()).attr(
                "style",
                format!(
                    "position:absolute;left:0;top:0;width:{SWATCH}px;height:{SWATCH}px;\
                 object-fit:cover;border-radius:6px;display:block"
                ),
            ),
        ));
    }

    if spec.hull.len() >= 3 {
        // The collider region: a translucent polygon clipped to the hull, so you see exactly
        // what the physics body covers. Stage-B note: a vertex dragged inward makes the stored
        // polygon concave — this clip-path follows it, but parry reconvexifies the collider, so
        // the visual and the physics body diverge on a concave edit (expanding/convex edits stay
        // in lockstep). Accepted for now; a convexity constraint or dual-shape draw comes later.
        let pts: Vec<String> = spec
            .hull
            .iter()
            .map(|&(nx, ny)| format!("{:.2}% {:.2}%", (nx + 0.5) * 100.0, (ny + 0.5) * 100.0))
            .collect();
        children.push(Box::new(el::<_, S, ()>("div", ()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{SWATCH}px;height:{SWATCH}px;\
                 background-color:rgba(120,170,255,0.16);clip-path:polygon({})",
                pts.join(", ")
            ),
        )));
        // A dot per vertex — the editor's handles: drag to move, right-click to remove; click a
        // bare edge to add a new one. (Node body & face — the shape editor.)
        let half = HANDLE / 2.0;
        for &(nx, ny) in &spec.hull {
            let cx = norm_to_swatch_px(nx);
            let cy = norm_to_swatch_px(ny);
            children.push(Box::new(el::<_, S, ()>("div", ()).attr(
                "style",
                format!(
                    "position:absolute;left:{cx:.1}px;top:{cy:.1}px;width:{HANDLE}px;height:{HANDLE}px;\
                     margin-left:-{half}px;margin-top:-{half}px;border-radius:50%;\
                     background-color:#ffffff;border:1px solid rgba(0,0,0,0.55);box-sizing:border-box"
                ),
            )));
        }
    }

    // The swatch container: a positioned, sized box the children layer inside. The
    // `node-swatch` class + `data-subject` let the host hit-test walk up to it and learn
    // whose hull a vertex drag edits. (Swatch — Stage B.)
    let mut container = el::<_, S, ()>("div", children)
        .attr("class", "node-swatch")
        .attr(
            "style",
            format!(
                "position:relative;width:{SWATCH}px;height:{SWATCH}px;margin-top:8px;\
                 background-color:rgba(0,0,0,0.25);border-radius:6px"
            ),
        );
    if let Some(subject) = spec.subject {
        container = container.attr("data-subject", subject.to_string());
    }
    Box::new(container)
}

/// The connections swatch's on-screen dot diameter (px) for a selected node.
const CONN_DOT: f32 = 10.0;

/// One placed node in a connections swatch: its position normalized into the card `[0, 1]`, plus the
/// node's uuid (emitted as `data-element` for the P4 hit-test). (Swatch primitive — P2.)
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnNode {
    pub x: f32,
    pub y: f32,
    pub subject: uuid::Uuid,
}

/// One placed relation cell in a connections swatch: the two endpoints (normalized `[0, 1]`),
/// stable endpoint ids, and relation kind. Multiple cells on the same pair fan apart.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnEdge {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub from: uuid::Uuid,
    pub to: uuid::Uuid,
    pub kind: RelationKind,
    pub family: EdgeFamily,
}

/// One chip in the strip: the classifier's display label plus the stable kind tag the chip's
/// DOM carries (`data-shape-kind`), so a click can crystallize the selection *as* that shape.
/// (Swatch primitive — P3, chip-click crystallize.)
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ShapeChip {
    pub label: String,
    pub kind_tag: String,
}

/// What a connections swatch shows: the selected nodes plus the edges among them, laid out in the
/// card. The Selection-scope payload, sibling to [`SwatchSpec`] (the Node scope). A unified `Scope`
/// / `SwatchInstance` config follows as more scopes converge. (Swatch primitive — P2.)
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ConnectionsSpec {
    pub nodes: Vec<ConnNode>,
    pub edges: Vec<ConnEdge>,
    /// The classifier's ranked shapes (dominant first) for the chip strip, or empty when
    /// unclassified. (Swatch primitive — P3.)
    pub shape_chips: Vec<ShapeChip>,
}

/// Build a connections-swatch spec from the selected nodes' raw positions and their inter-edges:
/// normalize the positions into the card's `[0, 1]` box (inset by a margin so dots are not flush to
/// the edge), carrying the edges in the same normalized space. Pure (the normalization is
/// unit-tested); a single or all-coincident axis centers at `0.5`. (Swatch primitive — P2.)
pub(crate) fn connections_spec_from(
    nodes: Vec<(uuid::Uuid, f32, f32)>,
    edges: Vec<(uuid::Uuid, uuid::Uuid, RelationKind)>,
) -> ConnectionsSpec {
    const MARGIN: f32 = 0.12;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(_, x, y) in &nodes {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let norm = |v: f32, min: f32, span: f32| -> f32 {
        if span <= f32::EPSILON {
            0.5
        } else {
            MARGIN + (v - min) / span * (1.0 - 2.0 * MARGIN)
        }
    };
    let (span_x, span_y) = (max_x - min_x, max_y - min_y);
    let mut pos: std::collections::HashMap<uuid::Uuid, (f32, f32)> =
        std::collections::HashMap::new();
    let conn_nodes: Vec<ConnNode> = nodes
        .iter()
        .map(|&(id, x, y)| {
            let (nx, ny) = (norm(x, min_x, span_x), norm(y, min_y, span_y));
            pos.insert(id, (nx, ny));
            ConnNode {
                x: nx,
                y: ny,
                subject: id,
            }
        })
        .collect();
    let mut counts: std::collections::HashMap<(uuid::Uuid, uuid::Uuid), usize> =
        std::collections::HashMap::new();
    for &(a, b, _) in &edges {
        *counts.entry(edge_pair_key(a, b)).or_insert(0) += 1;
    }
    let mut seen: std::collections::HashMap<(uuid::Uuid, uuid::Uuid), usize> =
        std::collections::HashMap::new();
    let conn_edges: Vec<ConnEdge> = edges
        .iter()
        .filter_map(|&(a, b, kind)| {
            let &(x0, y0) = pos.get(&a)?;
            let &(x1, y1) = pos.get(&b)?;
            let key = edge_pair_key(a, b);
            let ordinal = *seen.entry(key).and_modify(|n| *n += 1).or_insert(0);
            let total = counts.get(&key).copied().unwrap_or(1);
            let (x0, y0, x1, y1) = fan_edge(x0, y0, x1, y1, ordinal, total);
            Some(ConnEdge {
                x0,
                y0,
                x1,
                y1,
                from: a,
                to: b,
                kind,
                family: kind.family(),
            })
        })
        .collect();
    ConnectionsSpec {
        nodes: conn_nodes,
        edges: conn_edges,
        shape_chips: Vec::new(),
    }
}

fn edge_pair_key(a: uuid::Uuid, b: uuid::Uuid) -> (uuid::Uuid, uuid::Uuid) {
    if a <= b { (a, b) } else { (b, a) }
}

fn fan_edge(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    ordinal: usize,
    total: usize,
) -> (f32, f32, f32, f32) {
    if total <= 1 {
        return (x0, y0, x1, y1);
    }
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    let (px, py) = if len <= f32::EPSILON {
        (1.0, 0.0)
    } else {
        (-dy / len, dx / len)
    };
    let center = (total as f32 - 1.0) * 0.5;
    let offset = (ordinal as f32 - center) * 0.026;
    (
        (x0 + px * offset).clamp(0.04, 0.96),
        (y0 + py * offset).clamp(0.04, 0.96),
        (x1 + px * offset).clamp(0.04, 0.96),
        (y1 + py * offset).clamp(0.04, 0.96),
    )
}

/// Render a connections swatch as host-generic DOM: the selected nodes as dots, the edges among them
/// as transform-free dotted lines (serval's CSS subset has no `transform`, so a single rotated bar is
/// unavailable; the dotted line is position-only), family-coloured. Read-only for P2; the projection
/// strip (P3) and per-cell hit-test (P4) ride the same `data-element` tags. (Swatch primitive — P2.)
pub(crate) fn connections_swatch_view<S: 'static>(spec: &ConnectionsSpec) -> SwatchView<S> {
    const SEG: usize = 8;
    let mut children: Vec<SwatchView<S>> = Vec::new();
    // Edges first, so the node dots paint over them.
    for e in &spec.edges {
        let color = family_color(e.family);
        for i in 1..SEG {
            let t = i as f32 / SEG as f32;
            let x = (e.x0 + (e.x1 - e.x0) * t) * 100.0;
            let y = (e.y0 + (e.y1 - e.y0) * t) * 100.0;
            children.push(Box::new(
                el::<_, S, ()>("div", ())
                    .attr("data-element", "relation-cell")
                    .attr("data-from", e.from.to_string())
                    .attr("data-to", e.to.to_string())
                    .attr("data-relation-tag", e.kind.tag().to_string())
                    .attr(
                        "style",
                        format!(
                            "position:absolute;left:{x:.2}%;top:{y:.2}%;width:4px;height:4px;\
                             margin-left:-2px;margin-top:-2px;border-radius:50%;background-color:{color}"
                        ),
                    ),
            ));
        }
    }
    for n in &spec.nodes {
        let (x, y) = (n.x * 100.0, n.y * 100.0);
        children.push(Box::new(
            el::<_, S, ()>("div", ())
                .attr("data-element", format!("node:{}", n.subject))
                .attr(
                    "style",
                    format!(
                        "position:absolute;left:{x:.2}%;top:{y:.2}%;width:{CONN_DOT}px;\
                         height:{CONN_DOT}px;margin-left:-{half}px;margin-top:-{half}px;\
                         border-radius:50%;background-color:#cdd6f4;\
                         border:1px solid rgba(0,0,0,0.55);box-sizing:border-box",
                        half = CONN_DOT / 2.0,
                    ),
                ),
        ));
    }
    // The classifier's ranked shapes, as a chip strip across the top of the card: the dominant chip
    // highlighted, the runners-up dimmed. Each chip carries `data-element="shape-chip"` +
    // `data-shape-kind` so a host click crystallizes the selection as that shape. (Swatch
    // primitive — P3, the strip + chip-click crystallize.)
    if !spec.shape_chips.is_empty() {
        let chips: Vec<SwatchView<S>> = spec
            .shape_chips
            .iter()
            .enumerate()
            .map(|(i, chip)| {
                let bg = if i == 0 {
                    "rgba(137,180,250,0.32)"
                } else {
                    "rgba(0,0,0,0.45)"
                };
                Box::new(
                    el::<_, S, ()>("div", chip.label.clone())
                        .attr("data-element", "shape-chip")
                        .attr("data-shape-kind", chip.kind_tag.clone())
                        .attr(
                            "style",
                            format!(
                                "display:inline-block;margin-right:4px;font-size:11px;line-height:1;\
                                 color:#cdd6f4;background-color:{bg};padding:2px 6px;border-radius:5px;\
                                 white-space:nowrap"
                            ),
                        ),
                ) as SwatchView<S>
            })
            .collect();
        children.push(Box::new(
            el::<_, S, ()>("div", chips)
                .attr("style", "position:absolute;left:6px;top:5px;right:6px"),
        ));
    }
    Box::new(
        el::<_, S, ()>("div", children)
            .attr("class", "connections-swatch")
            .attr(
                "style",
                "position:relative;width:100%;height:100%;box-sizing:border-box;\
             background-color:rgba(0,0,0,0.25);border-radius:6px",
            ),
    )
}

/// A relation family's edge colour in the connections swatch (the family legend, P2 v0).
fn family_color(family: EdgeFamily) -> &'static str {
    match family {
        EdgeFamily::Semantic => "#89b4fa",
        EdgeFamily::Traversal => "#a6e3a1",
        EdgeFamily::Containment => "#f9e2af",
        EdgeFamily::Provenance => "#cba6f7",
        EdgeFamily::Imported => "#94e2d5",
        EdgeFamily::Arrangement => "#f5c2e7",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connections_spec_normalizes_positions_into_the_card() {
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);
        let spec = connections_spec_from(
            vec![(a, 0.0, 0.0), (b, 10.0, 10.0)],
            vec![(
                a,
                b,
                RelationKind::Semantic(mere::kernel::graph::SemanticSubKind::Cites),
            )],
        );
        assert_eq!(spec.nodes.len(), 2, "both selected nodes are placed");
        assert_eq!(spec.edges.len(), 1, "the one inter-edge is carried");
        let na = spec.nodes.iter().find(|n| n.subject == a).unwrap();
        let nb = spec.nodes.iter().find(|n| n.subject == b).unwrap();
        // a (min corner) sits up-left of b (max corner); both inset inside the card.
        assert!(
            na.x > 0.0 && na.x < nb.x && nb.x < 1.0,
            "a is left of b, both inside"
        );
        assert!(na.y < nb.y, "a is above b");
        assert_eq!(
            spec.edges[0].family,
            EdgeFamily::Semantic,
            "the edge keeps its family"
        );
        assert_eq!(spec.edges[0].from, a, "the edge keeps its source");
    }

    #[test]
    fn connections_spec_centers_a_coincident_axis() {
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);
        // Same x for both, so the x axis is degenerate and centers at 0.5.
        let spec = connections_spec_from(vec![(a, 5.0, 0.0), (b, 5.0, 9.0)], Vec::new());
        for n in &spec.nodes {
            assert!((n.x - 0.5).abs() < 1e-6, "a coincident x axis centers");
        }
    }

    #[test]
    fn connections_spec_fans_multiple_relation_cells() {
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);
        let spec = connections_spec_from(
            vec![(a, 0.0, 0.0), (b, 10.0, 0.0)],
            vec![
                (
                    a,
                    b,
                    RelationKind::Semantic(mere::kernel::graph::SemanticSubKind::Cites),
                ),
                (a, b, RelationKind::Traversal),
            ],
        );
        assert_eq!(spec.edges.len(), 2);
        assert_ne!(
            spec.edges[0].y0, spec.edges[1].y0,
            "parallel relation cells are visually fanned"
        );
    }
}
