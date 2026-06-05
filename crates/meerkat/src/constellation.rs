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
/// bookkeeping. Kept **warm** once spawned — an open tab persists after you
/// navigate away — and reaped only on explicit close ([`Constellation::reap`]) or
/// LRU eviction when the active-tab cap is exceeded, unless `background` protects
/// it.
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
    /// Bumped each time a new scene is accepted, so the host can cache a tile's
    /// rasterized texture and re-rasterize only when this changes (not every frame).
    scene_version: u64,
    /// Keep the actor working even when the tab is not shown (headless background
    /// work), and exempt it from cap eviction.
    background: bool,
    /// The pool clock at this tab's last spawn / drive, for LRU eviction: the
    /// least-recently-touched evictable tab is reaped first over the cap.
    last_touched: u64,
}

/// Default cap on warm tabs (active actors) before LRU eviction kicks in. A
/// configurable setting later; the per-tab resource cost keeps real tab counts
/// well under it in practice.
pub const DEFAULT_TAB_CAP: usize = 12;

/// The pool of active nodes (the live half of the graph).
pub struct Constellation {
    /// The wake every spawned actor pokes to drive the host's event loop.
    wake: Wake,
    active: HashMap<GraphMemberId, Activation>,
    /// The most warm tabs to keep; over this, the least-recently-touched
    /// evictable tab is reaped on reconcile.
    cap: usize,
    /// Monotonic clock, bumped on each spawn / drive and stamped into a tab's
    /// `last_touched`, so eviction picks the genuinely-stalest tab.
    touch_clock: u64,
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
        Self { wake, active: HashMap::new(), cap: DEFAULT_TAB_CAP, touch_clock: 0 }
    }

    /// Set the active-tab cap (the configurable setting; clamped to at least 1).
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap.max(1);
    }

    /// Whether `member` currently has a live actor.
    pub fn is_active(&self, member: GraphMemberId) -> bool {
        self.active.contains_key(&member)
    }

    /// How many nodes are active.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Reconcile the pool to the needed set: spawn an actor for any
    /// needed-but-dormant node, then **keep every active node warm** — an open tab
    /// persists after you navigate away; it is *not* reaped on blur. The only
    /// involuntary reaping is the active-tab cap: when the pool exceeds
    /// [`cap`](Self::cap), the least-recently-touched tab that is neither needed
    /// now nor `background` is evicted, until within the cap (or none remain to
    /// evict). Explicit close is [`reap`](Self::reap).
    pub fn reconcile(&mut self, needed: &[GraphMemberId]) {
        // Spawn needed-but-dormant nodes, each touch-stamped so LRU is well-ordered.
        for &member in needed {
            if !self.active.contains_key(&member) {
                self.touch_clock += 1;
                let touch = self.touch_clock;
                let (handle, rx) = spawn_content(self.wake.clone());
                self.active.insert(
                    member,
                    Activation {
                        handle,
                        rx,
                        gens: Generations::default(),
                        shown: None,
                        scene: None,
                        scene_version: 0,
                        background: false,
                        last_touched: touch,
                    },
                );
            }
        }
        // Enforce the cap: evict the least-recently-touched evictable tab (neither
        // needed now nor backgrounded) until within the cap, or until none remain.
        let needed_set: HashSet<GraphMemberId> = needed.iter().copied().collect();
        while self.active.len() > self.cap {
            let victim = self
                .active
                .iter()
                .filter(|(member, a)| !needed_set.contains(member) && !a.background)
                .min_by_key(|(_, a)| a.last_touched)
                .map(|(member, _)| *member);
            match victim {
                Some(member) => {
                    self.active.remove(&member); // drop → channel closes → thread ends
                },
                None => break, // every remaining tab is needed or background
            }
        }
    }

    /// Drive `member`'s actor for the current frame: a fresh document (url /
    /// fetch-state change) is a `Show` (new nav generation; blank the scene until
    /// it re-renders), a same-document size change a `Resize`. No-op if the node
    /// is not active or nothing changed.
    pub fn drive(&mut self, member: GraphMemberId, url: &str, state: Option<ContentState>, cw: u32, ch: u32) {
        let tag = ContentState::tag(state.as_ref());
        self.touch_clock += 1;
        let touch = self.touch_clock;
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        activation.last_touched = touch; // shown this frame → freshest against eviction
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

    /// The member's scene version — bumped each time a new scene is accepted. The
    /// host caches a tile's rasterized texture against this so an unchanged tile is
    /// not re-rasterized every frame. `0` if the member is not active or has no scene.
    pub fn scene_version(&self, member: GraphMemberId) -> u64 {
        self.active.get(&member).map_or(0, |a| a.scene_version)
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
                                activation.scene_version += 1;
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

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn m(n: u128) -> GraphMemberId {
        Uuid::from_u128(n)
    }

    fn noop_wake() -> Wake {
        std::sync::Arc::new(|| {})
    }

    #[test]
    fn reconcile_keeps_tabs_warm() {
        let mut c = Constellation::new(noop_wake());
        c.reconcile(&[m(1), m(2)]);
        assert_eq!(c.active_count(), 2, "two needed nodes spawned");
        c.reconcile(&[m(2)]); // m(1) is no longer needed...
        assert!(c.is_active(m(1)), "...but stays a warm tab — no reap on blur");
        assert_eq!(c.active_count(), 2);
    }

    #[test]
    fn reconcile_evicts_least_recently_touched_over_cap() {
        let mut c = Constellation::new(noop_wake());
        c.set_cap(2);
        c.reconcile(&[m(1)]); // touch 1
        c.reconcile(&[m(2)]); // touch 2 — m(1) is now the stalest
        c.reconcile(&[m(3)]); // touch 3, over the cap of 2 → evict the stalest evictable
        assert_eq!(c.active_count(), 2, "the cap holds");
        assert!(!c.is_active(m(1)), "the least-recently-touched, non-needed tab is evicted");
        assert!(c.is_active(m(2)) && c.is_active(m(3)));
    }

    #[test]
    fn a_background_tab_is_exempt_from_eviction() {
        let mut c = Constellation::new(noop_wake());
        c.set_cap(1);
        c.reconcile(&[m(1)]);
        assert!(c.set_background(m(1), true), "flagging an active node succeeds");
        c.reconcile(&[m(2)]); // over the cap of 1, but m(1) is background → not evictable
        assert!(c.is_active(m(1)), "a background tab survives cap pressure");
        assert!(c.is_active(m(2)), "the needed node is still spawned");
        assert!(!c.set_background(m(3), true), "flagging a dormant node reports false");
    }
}
