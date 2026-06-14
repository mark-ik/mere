/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Focused-node and content operations for a window: deleting the focused node,
//! toggling its background / compatibility-view flags, retry / stop / pin of its
//! live operation, the live-preview toggle, the needed-members set the
//! constellation reconciles to, the per-node activation state and content
//! silhouette the orrery colours / shapes from, resolving the focused member, and
//! the durable content cache load / save. Factored out of `frame_ops.rs` to keep
//! files under the 600-LOC ceiling.

use std::collections::HashMap;

use forme::GraphMemberId;
use orrery::{NodeShape, NodeState};
use session_runtime::content_store;

use super::{WindowCtx, fetch};

impl WindowCtx<'_> {
    /// Delete the focused node from the graph and reap its activation (the actor
    /// winds down on drop). A no-op when zero or many nodes are focused. Deletion
    /// removes the node's data; deactivation just stops its actor — this does
    /// both, because the node itself is gone.
    pub(super) fn delete_focused_node(&mut self) {
        if let Some(member) = self.orrery.remove_focused() {
            self.view.live_previews.remove(&member);
            self.shared.content.constellation.reap(member);
            self.save_session();
            self.view.request_redraw();
        }
    }

    /// Toggle the focused node's background flag: when set, its actor keeps
    /// running after focus moves away (the headless-active state for background
    /// work). A no-op when nothing is focused or the focused node has no live
    /// actor yet (focused but not rendered — press again once it has).
    pub(super) fn toggle_focus_background(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        let next = !self.shared.content.constellation.is_background(member);
        if self.shared.content.constellation.set_background(member, next) {
            tracing::info!(%member, background = next, "toggled node background");
            self.view.request_redraw();
        }
    }

    /// Toggle the focused node's compatibility view: render it through the
    /// system WebView (the scrying pool) instead of a content actor. Pinning
    /// also opens the live card so the WebView is visible immediately; the
    /// node's actor (if any) is reaped since the WebView replaces it.
    /// (Scrying tile plan, X1; session-local pin — the durable `compat_mode`
    /// node field takes over in X3.)
    pub(super) fn toggle_focus_compat(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        // The pin is shared session state; the WebView that serves it is this
        // window's per-view pool. Toggle the shared pin, reap the local tile.
        let on = if self.shared.content.compat_pins.remove(&member) {
            self.view.scrying.reap(member);
            false
        } else {
            self.shared.content.compat_pins.insert(member);
            true
        };
        if on {
            self.view.live_previews.insert(member);
            self.shared.content.constellation.reap(member);
        } else if self.view.scrying_input_focus == Some(member) {
            self.view.scrying_input_focus = None; // unpinned the tile that held the keyboard
        }
        tracing::info!(%member, compat = on, "toggled compatibility view");
        self.view.request_redraw();
    }

    /// Retry the focused node's page fetch. Unlike `ensure_content`, this bypasses
    /// the durable cache so Steward can ask the live fetch actor to try again.
    pub(super) fn retry_focused_content(&mut self) {
        let Some(url) = self.current_focus_url() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "retry focused content: no focused node",
            );
            return;
        };
        if !fetch::is_fetchable(&url) {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                format!("retry focused content: not fetchable: {url}"),
            );
            return;
        }
        self.shared.content.pages
            .insert(url.clone(), fetch::ContentState::Loading);
        self.shared.observability
            .record_actor("fetch", "started", Some(url.clone()));
        self.shared.content.fetch_handle.command(fetch::FetchCommand::Page(url));
        self.view.request_redraw();
    }

    /// Stop the focused live operation. In Cartography this demotes a live
    /// preview; in Tree it closes the focused tile because a visible tile is, by
    /// definition, still a needed operation and would otherwise respawn.
    pub(super) fn stop_focused_operation(&mut self) {
        let Some(member) = self.focused_member() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "stop focused operation: no focused node",
            );
            return;
        };
        if self.workbench_active() {
            self.view.workbench.close_tile(member);
            if self.view.workbench.open_members().is_empty() {
                self.close_workbench();
            } else if self.view.focused_tile == Some(member) {
                self.view.focused_tile = self.view.workbench.open_members().first().copied();
            }
        }
        self.view.live_previews.remove(&member);
        self.shared.content.constellation.reap(member);
        self.shared.observability
            .record_actor("content", "stopped", Some(member.to_string()));
        self.view.request_redraw();
    }

    /// Keep the focused operation alive in the background. If the node is dormant,
    /// promote it to a live preview first so the actor exists to pin.
    pub(super) fn pin_focused_operation(&mut self) {
        let Some(member) = self.focused_member() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "pin focused operation: no focused node",
            );
            return;
        };
        if let Some(url) = self.current_focus_url() {
            self.ensure_content(&url);
        }
        self.view.live_previews.insert(member);
        let gid = self.view.focused_graph;
        let needed: Vec<_> = self.needed_members().into_iter().map(|m| (m, gid)).collect();
        self.shared.content.constellation.reconcile(&needed);
        if self.shared.content.constellation.set_background(member, true) {
            self.shared.observability
                .record_actor("content", "pinned", Some(member.to_string()));
        }
        self.view.request_redraw();
    }

    /// The set of graph members that should be active this frame: in Tree the open
    /// tiles, in Cartography just the focused node (if any). The constellation
    /// reconciles its actor pool to this.
    pub(super) fn needed_members(&self) -> Vec<GraphMemberId> {
        // The orrery and the workbench coexist, so the active set is the union: the
        // orrery's live-preview cards plus (when the workbench pane is open) every
        // open tile across every slot. A node showing in both counts once.
        let mut needed: Vec<GraphMemberId> = self.view.live_previews.iter().copied().collect();
        if self.workbench_open() {
            for member in self.view.workbench.open_members() {
                if !needed.contains(&member) {
                    needed.push(member);
                }
            }
        }
        needed
    }

    /// Double-click on the focused node toggles its **live preview**: promote the
    /// static "last visit" snapshot card to a live actor card, or demote it back.
    /// The live set drives [`needed_members`](Self::needed_members) in Cartography,
    /// so promoting spawns the actor next frame and demoting reaps it (its last
    /// scene is kept as the node's snapshot). (Card system P2/P3.)
    pub(super) fn toggle_live_preview(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        if self.view.live_previews.remove(&member) {
            self.shared.content.constellation.reap(member); // demote: actor down, scene -> snapshot
        } else {
            self.view.live_previews.insert(member);
        }
        self.view.request_redraw();
    }

    /// The per-node activation state for the orrery's node coloring. A node with
    /// real fetched (`Ready`) content is `Open` (green) when a live actor is
    /// showing it, else `Closed` (red); everything else — a local / synthesized
    /// page, a blank (loading) one, or an errored one — is `Idle` (blue).
    pub(super) fn node_states(&self) -> HashMap<GraphMemberId, NodeState> {
        self.orrery
            .graph()
            .nodes()
            .map(|(_key, node)| {
                let state = match self.shared.content.pages.get(node.url()) {
                    Some(fetch::ContentState::Ready(_)) => {
                        if self.shared.content.constellation.is_active(node.id) {
                            NodeState::Open
                        } else {
                            NodeState::Closed
                        }
                    }
                    _ => NodeState::Idle,
                };
                (node.id, state)
            })
            .collect()
    }

    /// The per-node content silhouette for the orrery's node shaping, computed
    /// from each node's fetched content type (the same content map `node_states`
    /// reads). Only this-session-fetched nodes get an entry; unknown / unfetched
    /// nodes fall back to [`NodeShape::Square`] (the orrery's default), so a node
    /// takes its content shape as soon as it loads.
    pub(super) fn node_shapes(&self) -> HashMap<GraphMemberId, NodeShape> {
        self.orrery
            .graph()
            .nodes()
            .filter_map(|(_key, node)| match self.shared.content.pages.get(node.url()) {
                Some(fetch::ContentState::Ready(fetched)) => {
                    Some((node.id, content_shape(fetched.content_type.as_deref())))
                }
                _ => None,
            })
            .collect()
    }

    /// The focused node's graph member, if a node is focused (resolved URL → node
    /// UUID via the kernel node id).
    pub(super) fn focused_member(&self) -> Option<GraphMemberId> {
        let url = self.orrery.focused_url()?;
        self.orrery
            .graph()
            .get_node_by_url(url)
            .map(|(_, node)| node.id)
    }

    /// Load durably-cached content for `url` (page or subresource), or `None`.
    /// The fjall store's futures are ready, so `block_on` does not stall the UI.
    pub(super) fn load_cached(&mut self, url: &str) -> Option<content_store::StoredContent> {
        let store = self.shared.content.store.as_mut()?;
        pollster::block_on(content_store::load_content(store, url))
            .ok()
            .flatten()
    }

    /// Persist `body` (+ its content-type) for `url` to the durable content cache,
    /// so a reload need not re-fetch it. Best-effort; a write failure is logged.
    pub(super) fn save_cached(&mut self, url: &str, content_type: Option<String>, body: &[u8]) {
        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let stored = content_store::StoredContent {
            content_type,
            body: body.to_vec(),
        };
        if let Err(err) = pollster::block_on(content_store::save_content(store, url, &stored)) {
            tracing::warn!(%err, url, "failed to cache content");
        }
    }
}

/// Map a fetched content type to its orrery [`NodeShape`] (a first-cut vocabulary;
/// a theme / lens concern eventually). Feeds read as a circle, small-web menus /
/// directories as a rounded square, and documents (HTML / markdown / gemtext /
/// plain text / …) plus anything unrecognized as the default square.
pub(super) fn content_shape(content_type: Option<&str>) -> NodeShape {
    let Some(ct) = content_type else {
        return NodeShape::Square;
    };
    let base = ct
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "application/rss+xml" | "application/atom+xml" | "application/feed+json" => {
            NodeShape::Circle
        }
        "application/gopher-menu"
        | "application/x-nex"
        | "application/x-guppy"
        | "text/x-finger" => NodeShape::Rounded,
        _ => NodeShape::Square,
    }
}
