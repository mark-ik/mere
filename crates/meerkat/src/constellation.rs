/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The constellation: the pool of **active nodes**.
//!
//! A graph node is persistent data (dormant). It becomes *active* when it has a
//! live content actor rendering it off the UI thread — an [`Activation`]. The
//! constellation is the kernel's pool of those, keyed by graph member (node
//! UUID). The presentation mode decides how an active node is shown: in
//! Cartography the focused node's activation is composited as a floating card; in
//! Tree the open set's activations are composited as tiles. Either way the *same*
//! pool backs them, so "active node == has an actor" is one lifecycle, not two.
//!
//! Each frame the host computes the **needed** set (the focused node, or the open
//! tiles) and [`reconcile`](Constellation::reconcile)s the pool to it: needed
//! nodes that are not active spawn an actor; active nodes that are no longer
//! needed are reaped (their actor's command channel closes, ending its thread) —
//! *unless* they are flagged [`background`](Activation::background), the
//! headless-active state for nodes doing work behind the view (a feed, a sync, a
//! compute). Reaping is dropping the [`Activation`]; the graph datum is untouched,
//! so the node simply returns to dormant.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Generations, Wake};
use forme::GraphMemberId;
use linked_data::GraphContribution;
use netrender::Scene;

use crate::content::{spawn_content, ContentCommand, ContentUpdate};
use crate::fetch::ContentState;

/// A node brought to life: its content actor plus the per-activation
/// bookkeeping. Reaped (actor wound down) when the node leaves the needed set,
/// unless `background` is set.
struct Activation {
    handle: ActorHandle<ContentCommand>,
    rx: Receiver<ContentUpdate>,
    /// The generation pair stamped on `Show` / `Resize`, so a scene built for a
    /// document or size this node has left is dropped on arrival.
    gens: Generations,
    /// `(url, content-tag, w, h)` the actor was last told to show; a change
    /// drives a `Show` (new document) or `Resize` (same document, new size).
    shown: Option<(String, u8, u32, u32)>,
    /// The latest generation-accepted scene, composited at the node's pane.
    scene: Option<Scene>,
    /// Keep the actor alive even when the node is not in the needed set (headless
    /// background work outlives the view).
    background: bool,
}

/// The pool of active nodes (the live half of the graph).
pub struct Constellation {
    /// The wake every spawned actor pokes to drive the host's event loop.
    wake: Wake,
    active: HashMap<GraphMemberId, Activation>,
}

/// What a [`Constellation::drain`] surfaced for the host to act on. Scenes are
/// applied inside the pool; these are the cross-cutting effects the host owns.
#[derive(Default)]
pub struct Drained {
    /// A generation-accepted scene landed on at least one activation (redraw).
    pub any_scene: bool,
    /// Subresources a node's render wants, per node. The host fetches each (a
    /// durable-cache hit feeds the node directly via [`Constellation::send_resource`];
    /// a miss spawns a network fetch whose bytes return as a broadcast).
    pub wanted: Vec<(GraphMemberId, Vec<String>)>,
    /// Linked data harvested from active documents, for the host to apply to the
    /// graph.
    pub contributions: Vec<GraphContribution>,
}

impl Constellation {
    /// A new, empty pool. `wake` is cloned into every actor it spawns.
    pub fn new(wake: Wake) -> Self {
        Self { wake, active: HashMap::new() }
    }

    /// Whether `member` currently has a live actor.
    pub fn is_active(&self, member: GraphMemberId) -> bool {
        self.active.contains_key(&member)
    }

    /// How many nodes are active.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Spawn actors for needed-but-dormant nodes and reap active-but-unneeded
    /// ones (keeping any flagged `background`). Called each frame with the
    /// presentation's needed set (the focused node, or the open tiles).
    pub fn reconcile(&mut self, needed: &[GraphMemberId]) {
        let active: Vec<GraphMemberId> = self.active.keys().copied().collect();
        let background: Vec<GraphMemberId> =
            self.active.iter().filter(|(_, a)| a.background).map(|(m, _)| *m).collect();
        let (to_spawn, to_reap) = plan(&active, needed, &background);
        for member in to_reap {
            self.active.remove(&member); // drop → ActorHandle closes the channel → thread ends
        }
        for member in to_spawn {
            let (handle, rx) = spawn_content(self.wake.clone());
            self.active.insert(
                member,
                Activation {
                    handle,
                    rx,
                    gens: Generations::default(),
                    shown: None,
                    scene: None,
                    background: false,
                },
            );
        }
    }

    /// Drive `member`'s actor for the current frame: a fresh document (url /
    /// fetch-state change) is a `Show` (new nav generation; blank the scene until
    /// it re-renders), a same-document size change a `Resize`. No-op if the node
    /// is not active or nothing changed.
    pub fn drive(&mut self, member: GraphMemberId, url: &str, state: Option<ContentState>, cw: u32, ch: u32) {
        let tag = ContentState::tag(state.as_ref());
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        let key = (url.to_string(), tag, cw, ch);
        if activation.shown.as_ref() == Some(&key) {
            return;
        }
        let same_doc = activation.shown.as_ref().is_some_and(|(u, t, ..)| u == url && *t == tag);
        if same_doc {
            activation.gens.viewport.bump();
            activation.handle.command(ContentCommand::Resize {
                viewport: (cw, ch),
                viewport_gen: activation.gens.viewport,
            });
        } else {
            activation.gens.nav.bump();
            activation.scene = None;
            activation.handle.command(ContentCommand::Show {
                url: url.to_string(),
                state,
                viewport: (cw, ch),
                nav: activation.gens.nav,
                viewport_gen: activation.gens.viewport,
            });
        }
        activation.shown = Some(key);
    }

    /// The latest composited scene for `member`, if it has rendered one.
    pub fn scene(&self, member: GraphMemberId) -> Option<&Scene> {
        self.active.get(&member).and_then(|a| a.scene.as_ref())
    }

    /// Deactivate `member` now — its actor winds down on drop. For when the node
    /// itself is gone (deleted), so `reconcile` would not re-spawn it anyway.
    pub fn reap(&mut self, member: GraphMemberId) {
        self.active.remove(&member);
    }

    /// Whether `member` is flagged to keep working in the background.
    pub fn is_background(&self, member: GraphMemberId) -> bool {
        self.active.get(&member).is_some_and(|a| a.background)
    }

    /// Set (or clear) `member`'s background flag. Returns whether the node was
    /// active to flag.
    pub fn set_background(&mut self, member: GraphMemberId, background: bool) -> bool {
        match self.active.get_mut(&member) {
            Some(activation) => {
                activation.background = background;
                true
            },
            None => false,
        }
    }

    /// Feed a fetched subresource to one node's actor (a durable-cache hit routed
    /// to the node that wanted it).
    pub fn send_resource(&self, member: GraphMemberId, url: String, bytes: Vec<u8>) {
        if let Some(activation) = self.active.get(&member) {
            activation.handle.command(ContentCommand::Resource { url, bytes });
        }
    }

    /// Feed a subresource to every active node. The fetch stream is keyed by URL,
    /// not by which node wanted it, so a network result fans out; each actor
    /// dedups via its own resource store and only the one that wanted it
    /// re-renders.
    pub fn broadcast_resource(&self, url: &str, bytes: &[u8]) {
        for activation in self.active.values() {
            activation.handle.command(ContentCommand::Resource {
                url: url.to_string(),
                bytes: bytes.to_vec(),
            });
        }
    }

    /// Drain every active node's update channel, applying generation-accepted
    /// scenes into the pool and returning the wanted subresources + harvested
    /// contributions for the host to handle.
    pub fn drain(&mut self) -> Drained {
        let mut out = Drained::default();
        let members: Vec<GraphMemberId> = self.active.keys().copied().collect();
        for member in members {
            // Drain into an owned Vec first so the receiver borrow ends before we
            // re-borrow the activation to apply scenes.
            let updates: Vec<ContentUpdate> = match self.active.get(&member) {
                Some(activation) => activation.rx.try_iter().collect(),
                None => continue,
            };
            for update in updates {
                match update {
                    ContentUpdate::Scene { nav, viewport_gen, scene } => {
                        if let Some(activation) = self.active.get_mut(&member) {
                            let stamp = Generations { nav, viewport: viewport_gen };
                            if activation.gens.accepts(stamp) {
                                activation.scene = Some(scene);
                                out.any_scene = true;
                            }
                        }
                    },
                    ContentUpdate::Wanted { nav, urls } => {
                        let current = self.active.get(&member).is_some_and(|a| a.gens.nav == nav);
                        if current {
                            out.wanted.push((member, urls));
                        }
                    },
                    ContentUpdate::Contribution { contributions } => {
                        out.contributions.extend(contributions);
                    },
                }
            }
        }
        out
    }
}

/// The pure spawn/reap decision (no threads), so the activation policy is
/// testable: a needed-but-inactive node is spawned; an active node that is
/// neither needed nor flagged background is reaped.
fn plan(
    active: &[GraphMemberId],
    needed: &[GraphMemberId],
    background: &[GraphMemberId],
) -> (Vec<GraphMemberId>, Vec<GraphMemberId>) {
    let needed_set: HashSet<GraphMemberId> = needed.iter().copied().collect();
    let active_set: HashSet<GraphMemberId> = active.iter().copied().collect();
    let background_set: HashSet<GraphMemberId> = background.iter().copied().collect();
    let to_spawn: Vec<GraphMemberId> =
        needed.iter().copied().filter(|m| !active_set.contains(m)).collect();
    let to_reap: Vec<GraphMemberId> = active
        .iter()
        .copied()
        .filter(|m| !needed_set.contains(m) && !background_set.contains(m))
        .collect();
    (to_spawn, to_reap)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    #[test]
    fn plan_spawns_needed_and_reaps_unneeded() {
        let active = [m(1), m(2)];
        let needed = [m(2), m(3)];
        let (spawn, reap) = plan(&active, &needed, &[]);
        assert_eq!(spawn, vec![m(3)], "a needed node with no actor is spawned");
        assert_eq!(reap, vec![m(1)], "an active node no longer needed is reaped");
    }

    #[test]
    fn plan_keeps_background_nodes_even_when_unneeded() {
        let active = [m(1), m(2)];
        let needed = [m(2)]; // node 1 is no longer needed...
        let background = [m(1)]; // ...but it is doing background work
        let (spawn, reap) = plan(&active, &needed, &background);
        assert!(spawn.is_empty());
        assert!(reap.is_empty(), "a backgrounded node survives leaving the needed set");
    }

    #[test]
    fn plan_is_a_no_op_when_active_matches_needed() {
        let active = [m(1), m(2)];
        let needed = [m(1), m(2)];
        let (spawn, reap) = plan(&active, &needed, &[]);
        assert!(spawn.is_empty() && reap.is_empty());
    }

    #[test]
    fn reconcile_spawns_then_reaps_against_the_pool() {
        let wake: Wake = std::sync::Arc::new(|| {});
        let mut c = Constellation::new(wake);
        c.reconcile(&[m(1), m(2)]);
        assert_eq!(c.active_count(), 2, "two needed nodes spawned");
        assert!(c.is_active(m(1)) && c.is_active(m(2)));

        c.reconcile(&[m(2)]); // node 1 drops out of the needed set
        assert_eq!(c.active_count(), 1, "the unneeded node was reaped");
        assert!(c.is_active(m(2)) && !c.is_active(m(1)));
    }

    #[test]
    fn a_backgrounded_node_survives_reconcile() {
        let wake: Wake = std::sync::Arc::new(|| {});
        let mut c = Constellation::new(wake);
        c.reconcile(&[m(1)]);
        assert!(c.set_background(m(1), true), "flagging an active node succeeds");
        c.reconcile(&[]); // nothing needed...
        assert!(c.is_active(m(1)), "...but the backgrounded node keeps its actor");
        assert!(!c.set_background(m(2), true), "flagging a dormant node reports false");
    }
}
