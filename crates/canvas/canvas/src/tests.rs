// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
mod physics_catalog;
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
