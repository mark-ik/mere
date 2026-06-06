// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::Rect;
use crate::graphlet::{GraphletKind, GraphletRef};
use crate::layout::LayoutMode;
use crate::lens::ProjectionLens;
use crate::member::{LayoutOverride, Lifecycle, Provenance, SplitDirection};
use crate::nav::{FocusDirection, NavAction, TreeIntent};

use super::GraphTree;

mod basic;
mod layout_rects;
mod layout_structure;

fn make_layout_tree(mode: LayoutMode) -> GraphTree<u64> {
    let mut tree = GraphTree::new(mode, ProjectionLens::Traversal);
    tree.apply(NavAction::Attach {
        member: 1,
        provenance: Provenance::Anchor,
    });
    tree.apply(NavAction::Attach {
        member: 2,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::Attach {
        member: 3,
        provenance: Provenance::Traversal {
            source: 1,
            edge_kind: None,
        },
    });
    tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
    tree.apply(NavAction::SetLifecycle(3, Lifecycle::Warm));
    tree.apply(NavAction::Activate(1));
    tree.apply(NavAction::ToggleExpand(1));
    tree
}

