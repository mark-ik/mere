/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Constellation::drain (per-turn update harvest) + tab respawn self-heal.

use super::*;

impl Constellation {
    /// Drain every active node's update channel, applying generation-accepted
    /// scenes into the pool and returning the wanted subresources + harvested
    /// contributions for the host to handle. A channel that has **disconnected**
    /// means its content actor's thread died (a panic mid-render, isolated to that
    /// thread); the pool respawns it (self-healing, P4) up to [`MAX_RESPAWNS`],
    /// keeping the tab's last scene until the fresh actor renders.
    pub fn drain(&mut self) -> Drained {
        let mut out = Drained::default();
        let members: Vec<GraphMemberId> = self.active.keys().copied().collect();
        let mut dead: Vec<GraphMemberId> = Vec::new();
        for member in members {
            // Drain into an owned Vec first so the receiver borrow ends before we
            // re-borrow the activation to apply scenes. A `Disconnected` means the
            // actor thread is gone.
            let mut updates: Vec<ContentUpdate> = Vec::new();
            match self.active.get(&member) {
                Some(activation) => loop {
                    match activation.rx.try_recv() {
                        Ok(update) => updates.push(update),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            dead.push(member);
                            break;
                        }
                    }
                },
                None => continue,
            }
            for update in updates {
                match update {
                    ContentUpdate::Document {
                        nav,
                        viewport_gen,
                        packet,
                        fonts,
                        content_height,
                    } => {
                        if let Some(activation) = self.active.get_mut(&member) {
                            let stamp = Generations {
                                nav,
                                viewport: viewport_gen,
                            };
                            if activation.gens.accepts(stamp) {
                                activation.packet = Some(packet);
                                activation.fonts = fonts;
                                activation.scene = None; // forget any stale HTML scene
                                activation.masks = Vec::new(); // document lane has no shadow masks
                                activation.content_height = content_height;
                                // Document-lane hit-testing reads the packet; clear any
                                // stale HTML link table. (Phase 2 query API.)
                                activation.links = Vec::new();
                                activation.scene_version += 1;
                                activation.respawns = 0; // a fresh render = recovered
                                out.any_scene = true;
                            }
                        }
                    }
                    ContentUpdate::Scene {
                        nav,
                        viewport_gen,
                        scene,
                        content_height,
                        band_y,
                        band_h,
                        links,
                        masks,
                    } => {
                        if let Some(activation) = self.active.get_mut(&member) {
                            let stamp = Generations {
                                nav,
                                viewport: viewport_gen,
                            };
                            if activation.gens.accepts(stamp) {
                                activation.scene = Some(scene);
                                activation.packet = None; // forget any stale document packet
                                activation.masks = masks;
                                activation.content_height = content_height;
                                // Record the band this scene covers so the host composites
                                // it at the right offset; a Show/Resize that re-anchors the
                                // band to the top arrives as band_y == 0. (HTML scroll.)
                                activation.band = (band_y, band_h);
                                activation.links = links;
                                activation.scene_version += 1;
                                activation.respawns = 0; // a fresh scene = recovered
                                out.any_scene = true;
                            }
                        }
                    }
                    ContentUpdate::Wanted { nav, urls } => {
                        let current = self.active.get(&member).is_some_and(|a| a.gens.nav == nav);
                        if current {
                            out.wanted.push((member, urls));
                        }
                    }
                    ContentUpdate::Contribution { contributions } => {
                        // Pair each with the harvesting node's graph, so the host
                        // routes it to that graph's orrery.
                        if let Some(graph_id) = self.active.get(&member).map(|a| a.graph_id) {
                            out.contributions
                                .extend(contributions.into_iter().map(|c| (graph_id, c)));
                        }
                    }
                    ContentUpdate::FindMatches {
                        nav,
                        viewport_gen,
                        matches,
                    } => {
                        if let Some(activation) = self.active.get_mut(&member) {
                            let stamp = Generations {
                                nav,
                                viewport: viewport_gen,
                            };
                            // Accept only matches for the current document + size; a
                            // find result for a page the node has left is dropped.
                            if activation.gens.accepts(stamp) {
                                activation.find_matches = matches;
                                out.any_scene = true; // redraw to paint the highlights
                            }
                        }
                    }
                    ContentUpdate::ScriptOutcome { nav, outcome } => {
                        // A DocumentScript attach / turn / detach result (P2.5c). The
                        // re-render rides a separate `Scene`; surface the text for
                        // diagnostics (a script console rides this later). Dropped if the
                        // node has navigated away.
                        let current = self.active.get(&member).is_some_and(|a| a.gens.nav == nav);
                        if current {
                            tracing::debug!(outcome = %outcome, "document script outcome");
                        }
                    }
                }
            }
        }
        for member in dead {
            if self.respawn(member) {
                out.respawned.push(member);
            }
        }
        out
    }

    /// Respawn a dead tab's content actor with a fresh thread (the kernel-owned
    /// spec is the tab's own `url`/size, replayed by clearing `shown` so the next
    /// [`drive`](Self::drive) re-`Show`s it). Keeps the last scene (stale-but-live)
    /// until the fresh actor renders. A no-op past [`MAX_RESPAWNS`] (the tab is left
    /// on its last scene rather than storming). Returns whether it respawned.
    pub(crate) fn respawn(&mut self, member: GraphMemberId) -> bool {
        let Some(activation) = self.active.get_mut(&member) else {
            return false;
        };
        if activation.respawns >= MAX_RESPAWNS {
            return false;
        }
        let (handle, rx) = spawn_content(
            &self.pool,
            self.wake.clone(),
            self.disabled_engines.clone(),
            self.auto_ingest_linked_data,
        );
        activation.handle = handle;
        activation.rx = rx;
        activation.gens = Generations::default();
        activation.shown = None; // force the next drive() to re-Show, replaying the page
        activation.respawns += 1;
        true
    }
}
