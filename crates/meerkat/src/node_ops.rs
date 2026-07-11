/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Focused-node and content operations for a window: deleting the focused node,
//! toggling its background / compatibility-view flags, retry / stop / pin of its
//! live operation, the needed-members set the constellation reconciles to, the
//! per-node activation state and content
//! silhouette the orrery colours / shapes from, resolving the focused member, and
//! the durable content cache load / save. Factored out of `frame_ops.rs` to keep
//! files under the 600-LOC ceiling.

use std::cell::RefCell;
use std::collections::HashMap;

use mere::forme::GraphMemberId;
use mere::canvas::{NodeShape, NodeState};
use session_runtime::content_store;

use crate::fetch::{ContentState, Fetched};
use crate::resources::{ResourceLoader, ResourceStore};

use super::{WindowCtx, fetch};

impl WindowCtx<'_> {
    fn persist_node_thumbnail_png(
        &mut self,
        member: GraphMemberId,
        png_bytes: Vec<u8>,
        width: u32,
        height: u32,
        url_hint: Option<String>,
    ) {
        // Never deposit a blank/uniform thumbnail. A blank peek compresses to a few
        // hundred bytes while a real content capture is kilobytes; depositing a blank
        // sticks a dead preview the snapshot card then serves as truth (§5 item 1
        // reads thumbnails first). This is the funnel every capture-based deposit
        // routes through — scrying, live band, idle refresh — so one guard keeps them
        // all honest. The synthesized-peek path is gated at its own source (an empty
        // scene never rasterizes). (Honest deposit — summoning design §4.)
        const BLANK_THUMBNAIL_MAX: usize = 1024;
        if png_bytes.len() <= BLANK_THUMBNAIL_MAX {
            return;
        }
        let cache_url = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.url().to_string())
            .or(url_hint);
        if let (Some(url), Some(data_uri)) = (cache_url, crate::render::png_data_uri(&png_bytes)) {
            self.view.snapshot_data_uris.insert(
                member,
                crate::window_view::SnapshotDataUri { url, data_uri },
            );
        }
        // Tally against the per-session thumbnail byte budget the idle-refresh pass
        // checks (node/card summoning design, §5 item 4). This funnel accounts for
        // every deposit path here; only the idle pass actually gates on the total.
        self.shared.session.thumbnail_bytes_this_session += png_bytes.len();
        self.orrery_mut()
            .set_node_thumbnail(member, png_bytes, width, height);
    }

    pub(super) fn persist_scrying_thumbnail(
        &mut self,
        member: GraphMemberId,
        thumbnail: crate::scrying_host::CapturedThumbnail,
    ) {
        self.persist_node_thumbnail_png(
            member,
            thumbnail.png_bytes,
            thumbnail.width,
            thumbnail.height,
            thumbnail.url,
        );
    }

    pub(super) fn persist_scrying_thumbnails(
        &mut self,
        thumbnails: Vec<(GraphMemberId, crate::scrying_host::CapturedThumbnail)>,
    ) {
        for (member, thumbnail) in thumbnails {
            self.persist_scrying_thumbnail(member, thumbnail);
        }
    }

    pub(super) fn persist_workbench_tile_thumbnail(&mut self, member: GraphMemberId) {
        const PEEK_W: u32 = 300;
        const PEEK_H: u32 = 390;
        const RENDER_H: u32 = 600;
        let url = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.url().to_string());
        // Read once, up front: every fallback below (no cached texture, a zero-sized
        // one, or a failed readback) needs it to reproduce the tile's last-known
        // scroll position rather than always the page top. (Node/card summoning
        // design, §5 — the "exact last viewport" fix.)
        let scroll = self.view.scroll.get(&member).copied().unwrap_or(0.0);
        let Some(core) = self.render_core else {
            return;
        };
        let Some(cached) = self.view.tile_textures.get(&member) else {
            if let Some(url) = url.as_deref() {
                self.persist_synthesized_tile_thumbnail(
                    member, url, core, PEEK_W, PEEK_H, RENDER_H, scroll,
                );
            }
            return;
        };
        let (tex_w, tex_h) = cached.size;
        if tex_w == 0 || tex_h == 0 {
            if let Some(url) = url.as_deref() {
                self.persist_synthesized_tile_thumbnail(
                    member, url, core, PEEK_W, PEEK_H, RENDER_H, scroll,
                );
            }
            return;
        }
        let band_y = self.view.tile_bands.get(&member).copied().unwrap_or(0.0);
        let viewport_h = self
            .view
            .tile_rects
            .iter()
            .find(|(m, _)| *m == member)
            .map(|(_, rect)| (rect[3] - rect[1]).round().max(1.0) as u32)
            .unwrap_or(tex_h);
        let crop_y = (scroll - band_y).floor().max(0.0) as u32;
        let crop_h = viewport_h.max(1).min(tex_h.saturating_sub(crop_y).max(1));
        let rgba = crate::render::read_texture_rgba(
            core.device(),
            core.queue(),
            &cached.tex,
            tex_w,
            tex_h,
        );
        let Some((png_bytes, width, height)) = crate::render::thumbnail_png_bytes_from_rgba(
            &rgba,
            tex_w,
            tex_h,
            Some((0, crop_y.min(tex_h.saturating_sub(1)), tex_w, crop_h)),
            PEEK_W,
            PEEK_H,
        ) else {
            if let Some(url) = url.as_deref() {
                self.persist_synthesized_tile_thumbnail(
                    member, url, core, PEEK_W, PEEK_H, RENDER_H, scroll,
                );
            }
            return;
        };
        self.persist_node_thumbnail_png(member, png_bytes, width, height, None);
    }

    /// Also the idle-cadence snapshot-refresh pass's call target, once per open
    /// window: same deposit, triggered by "sat idle a while" rather than an actual
    /// boundary crossing. (Node/card summoning design, §5 item 4.)
    pub(super) fn persist_workbench_boundary_thumbnails(&mut self) {
        let focused = if self.workbench_active() {
            self.view.focused_tile
        } else {
            None
        };
        if let Some(member) = focused {
            self.persist_live_tile_thumbnail(member);
        }
        let members: Vec<_> = self.view.workbench.open_members().to_vec();
        for member in members {
            if Some(member) != focused {
                self.persist_workbench_tile_thumbnail(member);
            }
        }
    }

    pub(super) fn persist_live_tile_thumbnail(&mut self, member: GraphMemberId) {
        let Some(url) = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.url().to_string())
        else {
            return;
        };
        if self.is_surface_tier(member, &url) {
            if let Some(thumbnail) = self.view.scrying.capture_thumbnail(member) {
                self.persist_scrying_thumbnail(member, thumbnail);
            }
        } else {
            self.persist_workbench_tile_thumbnail(member);
        }
    }

    pub(super) fn persist_live_tile_thumbnail_before_navigation(&mut self, member: GraphMemberId) {
        if !self.workbench_active() || self.view.focused_tile != Some(member) {
            return;
        }
        self.persist_live_tile_thumbnail(member);
    }

    pub(super) fn set_focused_tile(&mut self, next: Option<GraphMemberId>) {
        let current = self.view.focused_tile;
        if current == next {
            return;
        }
        if let Some(member) = current {
            self.persist_live_tile_thumbnail(member);
        }
        self.view.focused_tile = next;
    }

    pub(super) fn focus_workbench_member(&mut self, member: GraphMemberId) {
        self.view.workbench.activate(member);
        self.set_focused_tile(Some(member));
        self.view.active_content = crate::ContentPane::Workbench;
    }

    pub(super) fn focus_orrery_content(&mut self) {
        if self.view.active_content == crate::ContentPane::Workbench {
            if let Some(member) = self.view.focused_tile {
                self.persist_live_tile_thumbnail(member);
            }
        }
        self.view.active_content = crate::ContentPane::Orrery;
    }

    /// `band_y` is the tile's last-known scroll offset (`self.view.scroll`, or `0.0`
    /// when never scrolled this session) — the durable-cache re-render windows from
    /// there instead of the page top, so this fallback means "last seen viewport",
    /// not "reconstructed page top". (Node/card summoning design, §5 — the "exact
    /// last viewport" fix.)
    fn persist_synthesized_tile_thumbnail(
        &mut self,
        member: GraphMemberId,
        url: &str,
        core: &serval_winit_host::RenderCore,
        peek_w: u32,
        peek_h: u32,
        render_h: u32,
        band_y: f32,
    ) {
        let Some(stored) = self.load_cached(url) else {
            return;
        };
        let state = ContentState::Ready(Fetched {
            content_type: stored.content_type,
            body: String::from_utf8_lossy(&stored.body).into_owned(),
        });
        let store = RefCell::new(ResourceStore::default());
        let wanted = RefCell::new(Vec::new());
        let loader = ResourceLoader::new(&store, url, &wanted);
        let (scene, _content_height, _links) = crate::card::render_content_scene(
            url,
            Some(&state),
            &self.shared.content.engine_registry,
            &self.shared.content.route_policy,
            &loader,
            peek_w,
            render_h,
            band_y,
            &self.shared.presentation.document_sheet_composed(),
        );
        let (tex, _view) = core.rasterize_for(
            crate::render::surface_keys::NODE_THUMBNAIL,
            &scene,
            peek_w,
            peek_h,
            // Page-like light, matching the on-demand peek (the same faithful-web
            // reasoning): a synthesized deposit must not bake a dark slab. (Honest deposit.)
            netrender::ColorLoad::Clear(crate::THUMBNAIL_BG),
        );
        let rgba =
            crate::render::read_texture_rgba(core.device(), core.queue(), &tex, peek_w, peek_h);
        let Some(png_bytes) = crate::render::png_bytes_from_rgba(&rgba, peek_w, peek_h) else {
            return;
        };
        self.persist_node_thumbnail_png(member, png_bytes, peek_w, peek_h, Some(url.to_string()));
    }

    /// Delete the focused node from the graph and reap its activation (the actor
    /// winds down on drop). A no-op when zero or many nodes are focused. Deletion
    /// removes the node's data; deactivation just stops its actor — this does
    /// both, because the node itself is gone.
    pub(super) fn delete_focused_node(&mut self) {
        // Capture the to-be-deleted node's data for the eidetic tombstone *before*
        // removing it (the deleted-nodes log; the Trail pane's "Removed" section).
        let tombstone = self.focused_tombstone();
        if let Some(member) = self.orrery_mut().remove_focused() {
            self.shared.content.constellation.reap(member);
            // Record the deletion in private memory. fjall resolves synchronously,
            // so this does not stall the UI.
            if let (Some(deleted), Some(store)) = (tombstone, self.shared.content.store.as_mut()) {
                let _ = pollster::block_on(eidetic::record_deleted(store, &deleted));
            }
            self.save_session();
            self.view.request_redraw();
        }
    }

    /// Run Athanor's forgetting pass over the focused graph: drop the cached content of
    /// short-term (untagged) nodes the eviction policy considers stale, leaving the nodes
    /// themselves (structure) in place. Records the count as a Steward-visible diagnostic
    /// (Athanor's live passes surface in Steward) and redraws. The Alembic Recent section's
    /// "forget" affordance drives this. (Alembic slice C eviction / slice D forgetting.)
    pub(super) fn run_forgetting_pass(&mut self) {
        use session_runtime::athanor;

        // Snapshot + per-node last-visit timing from the nav history. Both are owned, so the
        // immutable graph borrows end before the mutable store borrow below.
        let snapshot = self.orrery().graph().to_snapshot();
        let timing: HashMap<String, u64> = self
            .orrery()
            .graph()
            .recent_visited(100_000)
            .into_iter()
            .map(|rv| (rv.node.to_string(), rv.last_visit_at_ms))
            .collect();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // The persisted, user-editable policy (the Alembic Recent header control), not a
        // hardcoded default. (Editable eviction policy, B4.)
        let policy = self.shared.presentation.eviction_policy;
        let current_session = self.shared.session.current_session_count;
        let proposal =
            athanor::propose_forgetting(&snapshot, &timing, policy, now_ms, current_session);

        if proposal.is_empty() {
            self.shared.observability.record_diagnostic(
                "alembic.forget",
                super::observability::Severity::Info,
                "Forgetting: nothing stale in short-term memory".to_string(),
            );
            // Record the pass (dropped 0) so Steward shows it really ran. (B2.)
            self.shared.observability.record_forgetting_pass(0);
            return;
        }
        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let dropped = pollster::block_on(athanor::apply_forgetting(store, &proposal)).unwrap_or(0);
        self.shared.observability.record_diagnostic(
            "alembic.forget",
            super::observability::Severity::Info,
            format!("Forgetting: dropped {dropped} stale short-term page(s)"),
        );
        self.shared.observability.record_forgetting_pass(dropped);
        self.view.request_redraw();
    }

    /// Keep a Recent node: add the reserved `saved` tag (promote to long-term Saved memory) and
    /// persist. Promotion is a tag (decision #2: tagging is the promotion act), so a bookmark is
    /// just this reserved tag; a no-op if the node is gone or already kept. (Alembic promote, B3.)
    pub(super) fn keep_node(&mut self, url: &str) {
        if self.orrery_mut().tag_node_by_url(url, "saved") {
            self.save_session();
            self.view.request_redraw();
        }
    }

    /// Release a Saved node: remove the reserved `saved` tag and persist. A node still tagged
    /// otherwise stays Saved (only this tag is touched). (Alembic demote, B3.)
    pub(super) fn release_node(&mut self, url: &str) {
        if self.orrery_mut().untag_node_by_url(url, "saved") {
            self.save_session();
            self.view.request_redraw();
        }
    }

    /// The focused node as an eidetic tombstone (url + title + tags), for the
    /// deleted-nodes log. `None` when zero or many nodes are focused.
    fn focused_tombstone(&self) -> Option<eidetic::DeletedNode> {
        let member = self.focused_member()?;
        let (_, node) = self.orrery().graph().get_node_by_id(member)?;
        Some(eidetic::DeletedNode {
            node_id: member.to_string(),
            url: node.url().to_string(),
            title: (!node.title.is_empty()).then(|| node.title.clone()),
            tags: node.tags.iter().cloned().collect(),
            graph_id: None,
            deleted_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        })
    }

    /// Recover a removed node from its eidetic tombstone: find it by `node_id`,
    /// re-mint it (url + title + tags) into the focused graph, select it, and
    /// persist. The Trail › Removed rows queue `recover:<node_id>`; this is the
    /// other half of [`delete_focused_node`](Self::delete_focused_node)'s tombstone
    /// write. (Recover-deleted-node, Lane 0.) A no-op if the store or tombstone is
    /// gone. The tombstone is intentionally left in the log (re-mint is duplicate-
    /// friendly per the node-identity model); pruning it on recover is a follow-up.
    pub(super) fn recover_deleted_node(&mut self, node_id: &str) {
        let tomb = {
            let Some(store) = self.shared.content.store.as_mut() else {
                return;
            };
            pollster::block_on(eidetic::list_deleted(store))
                .unwrap_or_default()
                .into_iter()
                .find(|t| t.node_id == node_id)
        };
        let Some(tomb) = tomb else {
            return;
        };
        self.orrery_mut()
            .recover_node(&tomb.url, tomb.title.as_deref(), &tomb.tags);
        self.orrery_mut().select_by_url(&tomb.url);
        self.save_session();
        self.view.request_redraw();
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
        if self
            .shared
            .content
            .constellation
            .set_background(member, next)
        {
            tracing::info!(%member, background = next, "toggled node background");
            self.view.request_redraw();
        }
    }

    /// Toggle the focused node's compatibility view: render it through the
    /// system WebView (the scrying pool) instead of a content actor. Pinning
    /// opens the node as a **pelt tile** so the WebView shows immediately: the
    /// workbench surface-tier path drives the off-window WebView and composites
    /// its captured texture at the tile rect (no floating live card, no on-window
    /// DWM visual). The node's actor (if any) is reaped since the WebView replaces
    /// it. (Scrying tile plan, X1; session-local pin — the durable `compat_mode`
    /// node field takes over in X3.)
    pub(super) fn toggle_focus_compat(&mut self) {
        // Target what's in focus in the active mode: the focused tile in Tree, the
        // selected node in Cartography. The renderer scries this same target, so the
        // pin lands on (and the WebView fills) the surface the user is looking at.
        let Some(member) = self.nav_target_member() else {
            return;
        };
        // The pin is shared session state; the WebView that serves it is this
        // window's per-view pool. Toggle this node's `scrying.web` pin specifically
        // (a different engine pin from the picker is left alone), reap the local tile.
        let scrying = inker::routing::ENGINE_SCRYING_WEB;
        let on = if self
            .shared
            .content
            .engine_pins
            .get(&member)
            .map(String::as_str)
            == Some(scrying)
        {
            self.shared.content.engine_pins.remove(&member);
            if let Some(thumbnail) = self.view.scrying.reap(member) {
                self.persist_scrying_thumbnail(member, thumbnail);
            }
            false
        } else {
            self.shared
                .content
                .engine_pins
                .insert(member, scrying.to_string());
            true
        };
        if on {
            // Capture the serval side's place + session and stage it as a flip, so the
            // system WebView loads where the user was — same URL, same scroll, same
            // login — rather than blank and signed-out. This turns the stateless
            // engine-switch into a verso flip. Forms still degrade (they need the
            // off-thread serval DOM); the SESSION cookies come from the shared jar.
            // (Verso flip; native session store plan.)
            if let Some(url) = self
                .orrery()
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url().to_string())
            {
                let scroll_y = self.member_scroll_y(member);
                let cookies = super::fetch::session_cookies_for(&url);
                self.view.scrying.begin_flip(
                    member,
                    verso_tile::api::PortableViewState {
                        url: Some(url),
                        scroll: (0.0, scroll_y),
                        cookies,
                        ..Default::default()
                    },
                );
            }
            // The system WebView renders this node, so reap any serval actor, then open
            // it as a pelt tile: the workbench surface-tier path drives the off-window
            // WebView and composites its captured texture at the tile rect — no floating
            // live card, no on-window DWM visual. (Scry-in-pelt.)
            self.shared.content.constellation.reap(member);
            self.open_workbench();
            self.view.workbench.open_tile(member);
            self.set_focused_tile(Some(member));
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
        self.retry_content_url(&url);
    }

    /// Stop the focused live operation: reap its content actor. In Tree it also
    /// closes the focused tile, because a visible tile is, by definition, still a
    /// needed operation and would otherwise respawn.
    pub(super) fn stop_focused_operation(&mut self) {
        let Some(member) = self.focused_member() else {
            self.shared.observability.record_diagnostic(
                "meerkat.agent.intent_dropped",
                super::observability::Severity::Warn,
                "stop focused operation: no focused node",
            );
            return;
        };
        self.stop_operation(member);
    }

    /// Keep the focused operation alive in the background. Opens the node as a pelt
    /// (workbench) tile so a real actor exists to pin, then marks it background.
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
        // Open the node as a pelt tile so a real surface/actor exists to background-pin.
        // (Node-rep P4.)
        self.open_workbench();
        self.view.workbench.open_tile(member);
        self.set_focused_tile(Some(member));
        let gid = self.view.focused_graph;
        let needed: Vec<_> = self
            .needed_members()
            .into_iter()
            .map(|m| (m, gid))
            .collect();
        self.shared.content.constellation.reconcile(&needed);
        self.pin_operation(member);
    }

    /// The set of graph members that should be active this frame: in Tree the open
    /// tiles, in Cartography just the focused node (if any). The constellation
    /// reconciles its actor pool to this.
    pub(super) fn needed_members(&self) -> Vec<GraphMemberId> {
        // The active set is the open workbench tiles (the orrery keeps no live actor;
        // its focused-node snapshot renders host-side from the durable cache). When the
        // workbench pane is open, every open tile across every slot.
        let mut needed: Vec<GraphMemberId> = Vec::new();
        if self.workbench_open() {
            for member in self.view.workbench.open_members() {
                if !needed.contains(&member) {
                    needed.push(member);
                }
            }
        }
        needed
    }

    /// The per-node activation state for the orrery's node coloring. A node with
    /// real fetched (`Ready`) content is `Open` (green) when a live actor is
    /// showing it, else `Closed` (red); everything else — a local / synthesized
    /// page, a blank (loading) one, or an errored one — is `Idle` (blue).
    pub(super) fn node_states(&self) -> HashMap<GraphMemberId, NodeState> {
        self.orrery()
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
        self.orrery()
            .graph()
            .nodes()
            .filter_map(
                |(_key, node)| match self.shared.content.pages.get(node.url()) {
                    Some(fetch::ContentState::Ready(fetched)) => {
                        Some((node.id, content_shape(fetched.content_type.as_deref())))
                    }
                    _ => None,
                },
            )
            .collect()
    }

    /// The engine routing decision for `member` at nav time: its pin + the address
    /// scheme + any per-host override, over the active policy, filtered to engines
    /// this host actually has ([`engine_available`](Self::engine_available)). The
    /// document-engine re-route by content-type is the actor's second pass. The
    /// `engine_id` tells the host which lane to drive. (engine-picker Phase 0.)
    pub(super) fn route_engine(
        &self,
        member: GraphMemberId,
        url: &str,
    ) -> inker::routing::EngineRouteDecision {
        let request = inker::routing::EngineRouteRequest {
            workspace_id: inker::routing::WorkspaceRouteId::new("meerkat"),
            view: None,
            node: None,
            address: url.to_string(),
            content_type: None,
            pinned_engine: self.shared.content.engine_pins.get(&member).cloned(),
        };
        self.shared
            .content
            .route_policy
            .route_filtered(&request, |id| self.engine_available(id))
    }

    /// Whether engine `id` can be routed to this session: it is **present** on this
    /// host (registered / known) **and** **active** (not deactivated). The routing
    /// policy walks past engines this returns false for, so a pin to an absent or
    /// deactivated engine falls back to the scheme rule. (engine-picker Phase 0/1.)
    pub(super) fn engine_available(&self, id: &str) -> bool {
        self.engine_present(id) && self.engine_active(id)
    }

    /// Whether engine `id` is **present** on this host: a registered document engine,
    /// the serval html lane, the host-handled internal / external / ingest lanes, or
    /// a known surface engine available on this platform. (engine-picker Phase 0.)
    fn engine_present(&self, id: &str) -> bool {
        use inker::routing::{ENGINE_EXTERNAL_PROTOCOL, ENGINE_SCRYING_WEB, ENGINE_SERVAL_WEB};
        use mere::routing::{ENGINE_GRAPHSHELL_INTERNAL, ENGINE_LINKED_DATA_INGEST};
        if self.shared.content.engine_registry.contains(id) {
            return true; // nematic.* document engines, registered at startup
        }
        // Lanes meerkat handles without a document-registry entry: the serval html
        // lane, mere:// internal pages, the OS hand-off fallback, and JSON-LD ingest.
        if matches!(
            id,
            ENGINE_SERVAL_WEB
                | ENGINE_GRAPHSHELL_INTERNAL
                | ENGINE_EXTERNAL_PROTOCOL
                | ENGINE_LINKED_DATA_INGEST
        ) {
            return true;
        }
        // The scripted serval rung is a host-handled lane like the static one, present
        // only in the `scripted` build (the base build links no JS engine). (Ladder.)
        #[cfg(feature = "scripted")]
        if id == inker::routing::ENGINE_SERVAL_SCRIPTED {
            return true;
        }
        #[cfg(feature = "scripted-nova")]
        if id == inker::routing::ENGINE_SERVAL_SCRIPTED_NOVA {
            return true;
        }
        // Graft (Servo) / weld (CEF) surface engines: present when their cargo feature
        // is compiled in. The host pools (GraftHost / WeldHost) are no-op seams today —
        // the producer deps and the real frame-drive land in the follow-on — so with a
        // feature on, a node pinned to that engine renders a blank tile until then. Both
        // are off by default, so the base build never routes to them.
        #[cfg(feature = "engine-graft")]
        if id == inker::routing::ENGINE_GRAFT_SERVO {
            return true;
        }
        #[cfg(feature = "engine-weld")]
        if id == inker::routing::ENGINE_WELD_CHROMIUM {
            return true;
        }
        // The scrying surface engine is the system WebView pool — Windows + engine-scry.
        cfg!(all(target_os = "windows", feature = "engine-scry")) && id == ENGINE_SCRYING_WEB
    }

    /// Whether engine `id` is **active** (not deactivated this session). The host
    /// lanes (mere:// internal, the OS hand-off fallback, JSON-LD ingest) are
    /// structural and never user-deactivatable; every real engine honors the
    /// session's activation set (global default + per-session override). (Phase 1.)
    fn engine_active(&self, id: &str) -> bool {
        use inker::routing::ENGINE_EXTERNAL_PROTOCOL;
        use mere::routing::{ENGINE_GRAPHSHELL_INTERNAL, ENGINE_LINKED_DATA_INGEST};
        if matches!(
            id,
            ENGINE_GRAPHSHELL_INTERNAL | ENGINE_EXTERNAL_PROTOCOL | ENGINE_LINKED_DATA_INGEST
        ) {
            return true;
        }
        self.shared.content.engine_activation.is_enabled(id)
    }

    /// The engine rows for the settings manager: each present, user-facing engine
    /// (the surface / web engines headline, then the nematic document engines) with
    /// its active state. Host lanes are structural and not listed. (Phase 2.)
    pub(super) fn engine_rows(&self) -> Vec<crate::settings_lane::EngineRow> {
        use inker::routing::{ENGINE_SCRYING_WEB, ENGINE_SERVAL_WEB};
        let row = |id: &str, name: String| crate::settings_lane::EngineRow {
            id: id.to_string(),
            name,
            active: self.shared.content.engine_activation.is_enabled(id),
        };
        let mut rows = Vec::new();
        for &(id, name) in &[
            (ENGINE_SERVAL_WEB, "Serval (web)"),
            (ENGINE_SCRYING_WEB, "System WebView"),
        ] {
            if self.engine_present(id) {
                rows.push(row(id, name.to_string()));
            }
        }
        // The scripted serval rung (only in the `scripted` build). (Render ladder.)
        #[cfg(feature = "scripted")]
        {
            let id = inker::routing::ENGINE_SERVAL_SCRIPTED;
            if self.engine_present(id) {
                rows.push(row(id, "Serval (scripted, Boa)".to_string()));
            }
        }
        #[cfg(feature = "scripted-nova")]
        {
            let id = inker::routing::ENGINE_SERVAL_SCRIPTED_NOVA;
            if self.engine_present(id) {
                rows.push(row(id, "Serval (scripted, Nova)".to_string()));
            }
        }
        // The nematic document engines (protocol renderers), sorted for a stable order.
        let mut doc_ids: Vec<String> = self
            .shared
            .content
            .engine_registry
            .engine_ids()
            .map(str::to_string)
            .collect();
        doc_ids.sort();
        for id in doc_ids {
            let name = id.strip_prefix("nematic.").unwrap_or(&id).to_string();
            rows.push(row(&id, name));
        }
        rows
    }

    /// Flip engine `id`'s activation (the apparatus toggle): update the global
    /// default, push the new deactivated set to the actor pool, persist, and redraw.
    /// Surface engines take effect immediately via the UI-thread tier gate + the
    /// render's `retain`; document-engine changes apply to new navigations (an actor's
    /// registry is fixed at spawn time). (engine-picker Phase 2.)
    pub(super) fn toggle_engine(&mut self, id: &str) {
        let now_active = self.shared.content.engine_activation.is_enabled(id);
        self.shared
            .content
            .engine_activation
            .set_global(id, !now_active);
        let disabled = self
            .shared
            .content
            .engine_activation
            .global_disabled_vec()
            .into_iter()
            .collect();
        self.shared
            .content
            .constellation
            .set_disabled_engines(disabled);
        self.persist_settings();
        self.view.request_redraw();
    }

    /// Whether `member`'s node renders through a tier-2 **surface** engine (a system
    /// WebView via scrying, etc.) this frame. Surface-tier nodes drive the scrying
    /// pool; the rest drive the constellation (document / web lane). (Phase 0.)
    pub(super) fn is_surface_tier(&self, member: GraphMemberId, url: &str) -> bool {
        inker::routing::is_surface_engine(&self.route_engine(member, url).engine_id)
    }

    /// The focused node's graph member, if a node is focused (resolved URL → node
    /// UUID via the kernel node id).
    pub(super) fn focused_member(&self) -> Option<GraphMemberId> {
        let url = self.orrery().focused_url()?;
        self.orrery()
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
