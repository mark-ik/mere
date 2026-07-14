/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Deferred shell-level operations requested by per-window handlers.

use incipit::{GraphId, SessionId};
use winit::window::WindowId;

/// A per-window handler requests these when work needs full `&mut Shell`, the
/// window registry, or the event loop. `WindowCtx` reaches exactly one window;
/// the event loop drains this queue after the ctx borrow ends.
pub(crate) enum ShellCommand {
    /// Open a new OS window over the shared session.
    SpawnWindow,
    /// Tear a node out into a leaf window over the donor graph.
    TearOut { node: uuid::Uuid, from: GraphId },
    /// Copy a node from one pooled graph into another.
    CopyNodeAcross {
        node: uuid::Uuid,
        from: GraphId,
        to: GraphId,
    },
    /// Move a node from one pooled graph into another.
    MoveNodeAcross {
        node: uuid::Uuid,
        from: GraphId,
        to: GraphId,
    },
    /// Fork a node's connected component into an independent session.
    ForkNode { node: uuid::Uuid, from: GraphId },
    /// Branch a node into a graphlet-scoped window on the same graph.
    BranchNode { node: uuid::Uuid, from: GraphId },
    /// Grow a branch graphlet's roster after scoped navigation.
    RecordBranchMember {
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
        node: uuid::Uuid,
    },
    /// Mint and open a Linked graphlet over the donor graph.
    OpenLinkedGraphlet {
        node: uuid::Uuid,
        from: GraphId,
        kind: mere::forme::GraphletKind,
        selectors: Vec<String>,
        chip: &'static str,
    },
    /// Crystallize the graph's current multi-selection into a Session graphlet.
    CrystallizeSelection { from: GraphId },
    /// Reconcile every Linked graphlet for a graph after graph truth changes.
    ReconcileGraphlets { graph: GraphId },
    /// Reconcile one Linked graphlet from the Roster Graphlet Card.
    ReconcileGraphlet {
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
    },
    /// Convert a graphlet to an unlinked session grouping.
    KeepGraphletAsSession {
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
    },
    /// Toggle a relation-family selector on a Linked graphlet's spec, from the
    /// Roster Graphlet Card (selector/family editing).
    ToggleGraphletFamilySelector {
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
        family: mere::kernel::graph::EdgeFamily,
    },
    /// Branch an existing graphlet and open the branch.
    BranchGraphlet {
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
    },
    /// Open an existing graphlet in a scoped window.
    OpenExistingGraphlet {
        graph: GraphId,
        graphlet: mere::forme::GraphletId,
    },
    /// Close a secondary window.
    #[allow(dead_code)]
    CloseWindow(WindowId),
    /// Mint a fresh session and make it active.
    CreateSession,
    /// Switch to an existing session.
    SwitchSession(SessionId),
    /// Cycle to the next or previous session.
    CycleSession(bool),
    /// Close a session, switching away first if needed.
    CloseSession(SessionId),
    /// Open a session graph beside the current graph.
    OpenGraphBeside(SessionId),
    /// Thaw an engram into a read-only adjacent graph pane.
    OpenEngramBeside(String),
}
