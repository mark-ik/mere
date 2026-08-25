//! The canvas test suite, split by subject into `src/tests/` so no single file
//! carries the whole surface (the workspace keeps files under 600 LOC). This
//! root holds only what the subject modules share: the imports they glob
//! through `use super::*`, and the one cross-cutting fixture below.

use super::build::hyperlink;
use super::*;
use crate::edge_cells::selector_for_relation_kind;
use kernel::geometry::PortablePoint;
use kernel::graph::fixtures::GraphFixtures;
use kernel::graph::{Graph, RelationKind, RelationSelector, SemanticSubKind};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::collections::HashMap;

mod affinity;
mod camera;
mod fold_and_source_time;
mod gloss;
mod layout_and_drag;
mod node_face;
mod node_minting;
mod node_state;
mod relations;
mod restore_and_queries;
mod rings;
mod scope_and_cartography;
mod score_and_physics;
mod selection;
mod sizing;

fn first_edge_cell_between(
    canvas: &Canvas,
    a: kernel::graph::NodeKey,
    b: kernel::graph::NodeKey,
) -> EdgeCell {
    canvas
        .graph()
        .relations()
        .find_map(|relation| {
            let same_pair = (relation.from == a && relation.to == b)
                || (relation.from == b && relation.to == a);
            same_pair.then_some(EdgeCell {
                from: relation.from,
                to: relation.to,
                selector: selector_for_relation_kind(relation.kind),
            })
        })
        .expect("relation cell between the endpoints")
}
