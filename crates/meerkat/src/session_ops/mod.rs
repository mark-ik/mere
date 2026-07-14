/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Session lifecycle for the shell: persisting a session (graph + camera +
//! frame), renaming sessions in the switcher, rebuilding switcher thumbnails and
//! labels, and the `Shell`-level create / switch / cycle / close ops that re-key
//! the focused orrery in the pool (the step a per-window `WindowCtx` cannot do).
//! The per-window pieces hang off `WindowCtx`; the pool re-keying off `Shell`.
//! Factored out of `frame_ops.rs` to keep files under the 600-LOC ceiling.

use incipit::{GraphId, SessionId};
use mere::kernel::graph::Graph;
use session_runtime::{
    ViewIntent, frisket_store, manifest::GraphSessionManifest, session_graph_store,
    view_intent_store,
};

use super::{DEFAULT_FRAME, DEFAULT_PANE, WindowCtx};

/// Filename for the workbench tiling sidecar (beside `graph.json`): the platen
/// bridge's canonical `(arrangement, geometry)` pair, so a workbench's split shape,
/// tab stacks, and active tab survive a restart. The live `Pane` tree carries no
/// serde, so it persists through the bridge rather than directly. (A3 persistence.)
const WORKBENCH_FILE: &str = "workbench.json";

/// Kill-switch for restoring persisted workbench tiling on load. The read already
/// falls back to an empty workbench on any IO/parse error; flipping this to `false`
/// disables the whole path, should the round-trip ever prove wanting in the field.
const RESTORE_WORKBENCH_TILING: bool = true;

/// Load a persisted workbench from `session_dir`, pruned to `present` (the loaded
/// graph's member ids, so a tile whose node was deleted is reconciled away). Empty
/// workbench when the feature is off, the file is absent, or the JSON fails to
/// parse. Shared by the session-switch path ([`WindowCtx::restore_workbench`]) and
/// the boot restore in `main.rs`, so a restart reloads the tiling too. (A3.)
pub(crate) fn load_workbench(
    session_dir: &std::path::Path,
    present: &std::collections::HashSet<mere::forme::GraphMemberId>,
) -> mere::platen::Workbench {
    if !RESTORE_WORKBENCH_TILING {
        return mere::platen::Workbench::new();
    }
    std::fs::read_to_string(session_dir.join(WORKBENCH_FILE))
        .ok()
        .and_then(|json| mere::platen::Workbench::from_persisted_json(json.as_str(), present))
        .unwrap_or_else(mere::platen::Workbench::new)
}

/// Filename for the cartography position sidecar (beside `graph.json`): the orrery's
/// settled node positions, member-keyed (the Cartography projection geometry, the
/// counterpart of `workbench.json`'s TreeGeometry). The live force-directed layout is
/// never committed to the kernel graph, so this is what makes a session's settled
/// layout durable across a restart. (Position sidecar.)
const CARTOGRAPHY_FILE: &str = "cartography.json";

/// Kill-switch for restoring persisted orrery positions on load. On any miss the host
/// falls back to the graph's own load-time seed (the prior behavior). (Position sidecar.)
const RESTORE_CARTOGRAPHY: bool = true;

/// Load the persisted cartography positions from `session_dir`, pruned to `present`
/// (the loaded graph's members). `None` when the feature is off, the file is absent,
/// or the JSON fails to parse — the orrery then keeps its graph-seeded layout. Shared
/// by the session-switch and boot restore paths. (Position sidecar.)
pub(crate) fn load_cartography(
    session_dir: &std::path::Path,
    present: &std::collections::HashSet<mere::forme::GraphMemberId>,
) -> Option<mere::canvas::CartographyGeometry> {
    if !RESTORE_CARTOGRAPHY {
        return None;
    }
    std::fs::read_to_string(session_dir.join(CARTOGRAPHY_FILE))
        .ok()
        .and_then(|json| {
            mere::canvas::CartographyGeometry::from_persisted_json(json.as_str(), present)
        })
}

// Session ops live on `Shell`, not `WindowCtx`: switching a session re-keys the
// orrery pool, and a `WindowCtx` holds exactly one orrery borrowed *out* of the
// pool, so it cannot insert or re-key entries. Per-window input handlers request
// these by pushing a [`ShellCommand`]; `Shell::apply` runs them after the ctx
// borrow ends (the same seam as spawn/close). WindowCtx-shaped sub-steps
// (save_session, the cache reset, thumbnails) re-enter through `self.ctx()`, which
// resolves the focused view primary-or-pending and bundles its pooled orrery.
// (Window composition P1, multi-graph.)

mod shell_load;
mod shell_session;
mod view_intent;
mod windowctx;

pub(crate) use view_intent::{hidden_relation_records, restore_hidden_relations};

/// A short switcher label for a session with no user-set display name: the first
/// non-intro node's cached host (else its title), or "New" for an empty /
/// welcome-only graph. (Host text path.)
fn derive_session_label(graph: &Graph) -> String {
    graph
        .nodes()
        .map(|(_, node)| node)
        .find(|node| node.url() != "mere://welcome")
        .and_then(|node| {
            node.cached_host
                .clone()
                .filter(|h| !h.trim().is_empty())
                .or_else(|| Some(node.title.clone()))
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "New".to_string())
}
