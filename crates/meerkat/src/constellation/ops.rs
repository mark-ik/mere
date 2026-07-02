/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Constellation methods: config, reconcile/drive, scene + script accessors, reaping.

use super::*;

impl Constellation {
    pub fn new(wake: Wake) -> Self {
        Self {
            wake,
            active: HashMap::new(),
            pool: Pool::new(),
            cap: DEFAULT_TAB_CAP,
            touch_clock: 0,
            disabled_engines: HashSet::new(),
            auto_ingest_linked_data: false,
            script_bindings: Vec::new(),
            dpr: 1.0,
        }
    }

    /// Set the display device-pixel-ratio (winit `scale_factor`); the host pushes it
    /// each frame. Clamped to a sane floor so a stray 0 can't divide-by-zero the
    /// logical viewport. (Auto-DPI D2.)
    pub fn set_device_pixel_ratio(&mut self, dpr: f32) {
        self.dpr = dpr.max(0.1);
    }

    /// Toggle whether new content actors auto-harvest embedded linked data on load.
    /// Off by default (a page's structured data would otherwise flood the graph);
    /// actors snapshot it at spawn, so the caller reaps affected members to respawn
    /// them when it changes. (Linked-data ingest.)
    pub fn set_auto_ingest_linked_data(&mut self, on: bool) {
        self.auto_ingest_linked_data = on;
    }

    /// Set the active-tab cap (the configurable setting; clamped to at least 1).
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap.max(1);
    }

    /// Push the host's deactivated-engine set. New actors register only the document
    /// engines outside it. Existing actors keep their registry, so the caller reaps
    /// the affected active members (forcing a respawn) when the set changes for them.
    /// (engine-picker Phase 1b.)
    pub fn set_disabled_engines(&mut self, disabled: HashSet<String>) {
        self.disabled_engines = disabled;
    }

    /// Push the host's installed DocumentScript origin bindings (follow-on #2). On a
    /// fresh navigation in [`drive`](Self::drive) whose origin matches one, the
    /// actor is sent an `AttachScript` for that component — auto-attach via the same
    /// path the omnibar `>attach-script` verb uses. The host rebuilds + re-pushes
    /// these when the bindings file or the session script-permissions change.
    pub fn set_script_bindings(
        &mut self,
        bindings: Vec<crate::content::script::ResolvedScriptBinding>,
    ) {
        self.script_bindings = bindings;
    }

    /// The installed DocumentScript bindings currently in effect (what auto-attach matches
    /// against). The settings lane's Scripts page lists these read-only, reading them from here
    /// rather than re-parsing the binding files each frame. (Settings perf.)
    pub fn script_bindings(&self) -> &[crate::content::script::ResolvedScriptBinding] {
        &self.script_bindings
    }

    /// Whether `member` currently has a live actor.
    pub fn is_active(&self, member: GraphMemberId) -> bool {
        self.active.contains_key(&member)
    }

    /// How many nodes are active.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Snapshot live operations for Steward/Apparatus-style inspection without
    /// exposing the actor handles or receivers.
    pub fn active_operations(&self) -> Vec<ActiveOperation> {
        let mut operations: Vec<_> = self
            .active
            .iter()
            .map(|(member, activation)| ActiveOperation {
                member: *member,
                url: activation.shown.as_ref().map(|(url, ..)| url.clone()),
                background: activation.background,
                recovering: activation.respawns > 0
                    && activation.scene.is_none()
                    && activation.packet.is_none(),
                scene_version: activation.scene_version,
                content_height: activation.content_height,
            })
            .collect();
        operations.sort_by_key(|operation| operation.member);
        operations
    }

    /// Reconcile the pool to the needed set: spawn an actor for any
    /// needed-but-dormant node, then **keep every active node warm** — an open tab
    /// persists after you navigate away; it is *not* reaped on blur. The only
    /// involuntary reaping is the active-tab cap: when the pool exceeds
    /// [`cap`](Self::cap), the least-recently-touched tab that is neither needed
    /// now nor `background` is evicted, until within the cap (or none remain to
    /// evict). Explicit close is [`reap`](Self::reap).
    pub fn reconcile(&mut self, needed: &[(GraphMemberId, GraphId)]) {
        // Spawn needed-but-dormant nodes, each touch-stamped so LRU is well-ordered
        // and graph-stamped (the requesting pane's graph) so its contributions route
        // back to the right orrery.
        for &(member, graph_id) in needed {
            if !self.active.contains_key(&member) {
                self.touch_clock += 1;
                let touch = self.touch_clock;
                let (handle, rx) = spawn_content_with_transport(
                    &self.pool,
                    self.wake.clone(),
                    self.disabled_engines.clone(),
                    self.auto_ingest_linked_data,
                    content_update_transport(),
                );
                self.active.insert(
                    member,
                    Activation {
                        handle,
                        rx,
                        gens: Generations::default(),
                        shown: None,
                        engine: String::new(),
                        scene: None,
                        packet: None,
                        fonts: FontTable::default(),
                        masks: Vec::new(),
                        content_height: 0,
                        band: (0, 0),
                        requested_band: (0, 0),
                        links: Vec::new(),
                        find_matches: Vec::new(),
                        find_query: String::new(),
                        scene_version: 0,
                        background: false,
                        last_touched: touch,
                        respawns: 0,
                        graph_id,
                    },
                );
            }
        }
        // Enforce the cap: evict the least-recently-touched evictable tab (neither
        // needed now nor backgrounded) until within the cap, or until none remain.
        let needed_set: HashSet<GraphMemberId> = needed.iter().map(|(m, _)| *m).collect();
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
                }
                None => break, // every remaining tab is needed or background
            }
        }
    }

    /// Drive `member`'s actor for the current frame: a fresh document (url /
    /// fetch-state change) is a `Show` (new nav generation; blank the scene until
    /// it re-renders), a same-document size change a `Resize`. No-op if the node
    /// is not active or nothing changed.
    pub fn drive(
        &mut self,
        member: GraphMemberId,
        url: &str,
        state: Option<ContentState>,
        cw: u32,
        ch: u32,
        sheet: DocumentStyleSheet,
        // The host-routed engine id (the node's pin or the policy decision), passed to
        // the actor so it can take the scripted lane for `serval.scripted`. (Ladder.)
        engine: &str,
    ) {
        let tag = ContentState::tag(state.as_ref());
        self.touch_clock += 1;
        let touch = self.touch_clock;
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        activation.last_touched = touch; // shown this frame → freshest against eviction
        activation.engine = engine.to_string(); // current routing, for rung queries
        let key = (url.to_string(), tag, cw, ch);
        if activation.shown.as_ref() == Some(&key) {
            return;
        }
        let same_doc = activation
            .shown
            .as_ref()
            .is_some_and(|(u, t, ..)| u == url && *t == tag);
        // The actor lays out in logical (DIP) coords; the viewport it is told is the
        // physical tile size divided by the device-pixel-ratio. The host rasterizes the
        // resulting logical scene at physical res. (Auto-DPI D2.)
        let lcw = ((cw as f32) / self.dpr).round().max(1.0) as u32;
        let lch = ((ch as f32) / self.dpr).round().max(1.0) as u32;
        if same_doc {
            activation.gens.viewport.bump();
            activation.handle.command(ContentCommand::Resize {
                viewport: (lcw, lch),
                viewport_gen: activation.gens.viewport,
            });
        } else {
            activation.gens.nav.bump();
            activation.scene = None;
            activation.links.clear();
            // A new document invalidates the old find matches + query (a stale
            // highlight must not survive a navigation). (Find-in-page.)
            activation.find_matches.clear();
            activation.find_query.clear();
            activation.handle.command(ContentCommand::Show {
                url: url.to_string(),
                state,
                engine: engine.to_string(),
                viewport: (lcw, lch),
                nav: activation.gens.nav,
                viewport_gen: activation.gens.viewport,
                sheet,
            });
            // Auto-attach (follow-on #2): a fresh navigation whose origin matches an
            // installed binding attaches that DocumentScript over the page, via the
            // same `AttachScript` the omnibar verb sends. Tied to the fresh-Show path
            // so re-navigation re-attaches (new page) but a steady frame does not.
            if let Some(binding) = crate::content::script::binding_for(url, &self.script_bindings) {
                activation.handle.command(ContentCommand::AttachScript {
                    component_path: binding.component_path.clone(),
                    log: binding.log,
                    document: binding.document,
                    net: binding.net,
                    viewport_gen: activation.gens.viewport,
                });
            }
        }
        // Both a Show and a Resize re-anchor the actor's band to the top, so the
        // last requested band is stale; clear it so the next `request_scroll` always
        // re-commands the genuinely-wanted band. (HTML scroll.)
        activation.requested_band = (0, 0);
        activation.shown = Some(key);
    }

    /// Re-render every active document with a new composed style sheet (a live
    /// theme or typography change). Each content actor re-lays its document; a
    /// bumped viewport generation makes the re-render clear the generation gate so
    /// the new packet is accepted. The HTML/serval lane ignores the sheet (it
    /// themes through its own CSS). (Document theming P3; typography surface D1.)
    pub fn retheme(&mut self, sheet: DocumentStyleSheet) {
        for activation in self.active.values_mut() {
            activation.gens.viewport.bump();
            activation.handle.command(ContentCommand::Retheme {
                sheet: sheet.clone(),
                viewport_gen: activation.gens.viewport,
            });
        }
    }

    /// The latest composited scene for `member`, if it has rendered one.
    pub fn scene(&self, member: GraphMemberId) -> Option<&Scene> {
        self.active.get(&member).and_then(|a| a.scene.as_ref())
    }

    /// The blurred box-shadow mask requests for `member`'s latest HTML scene. The
    /// host builds + registers these GPU masks before rasterizing [`scene`](Self::scene).
    /// Empty for the document lane, an unrendered node, or a page without blurred
    /// shadows. (Box-shadow.)
    pub fn scene_masks(&self, member: GraphMemberId) -> &[paint_list_render::BoxShadowMaskRequest] {
        self.active.get(&member).map_or(&[], |a| &a.masks)
    }

    /// The member's scene version — bumped each time a new scene is accepted. The
    /// host caches a tile's rasterized texture against this so an unchanged tile is
    /// not re-rasterized every frame. `0` if the member is not active or has no scene.
    pub fn scene_version(&self, member: GraphMemberId) -> u64 {
        self.active.get(&member).map_or(0, |a| a.scene_version)
    }

    /// The full content height (px) of `member`'s latest scene — the tall texture
    /// height the host rasterizes and scrolls a window of. `0` if not active or
    /// not yet rendered.
    pub fn content_height(&self, member: GraphMemberId) -> u32 {
        // Actor heights are logical; report physical so the host's scroll math stays
        // physical. (Auto-DPI D2.)
        self.active
            .get(&member)
            .map_or(0, |a| ((a.content_height as f32) * self.dpr).round() as u32)
    }

    /// The vertical band `(band_y, band_h)` the member's latest HTML/serval scene
    /// covers: the page scrolled to `band_y` into a `band_h`-tall window. The host
    /// composites the flat scene at this offset (the visible UV row is
    /// `scroll − band_y`) and requests a new band via [`request_scroll`](Self::request_scroll)
    /// once the scroll leaves it. `(0, 0)` for a document-lane node (which windows
    /// its packet directly), an unrendered node, or before the first HTML scene.
    /// (HTML scroll.)
    pub fn scene_band(&self, member: GraphMemberId) -> (u32, u32) {
        // The actor reports the band in logical coords; scale to physical so the host's
        // compose offset + band texture height are physical. (Auto-DPI D2.)
        let s = self.dpr;
        self.active.get(&member).map_or((0, 0), |a| {
            (
                ((a.band.0 as f32) * s).round() as u32,
                ((a.band.1 as f32) * s).round() as u32,
            )
        })
    }

    /// Ask `member`'s actor to re-emit the HTML/serval band covering
    /// `(band_y, band_h)` — the document scrolled to `band_y` into a `band_h`-tall
    /// viewport — so a tall dense page is windowed one band at a time instead of
    /// rasterized whole (which overflows the GPU encode budget). Deduped against the
    /// last request, so holding the scroll still does not re-command (and re-lay-out)
    /// the actor every frame. No-op for a node that is not active or is on the
    /// document lane (whose packet the host windows itself). (HTML scroll.)
    pub fn request_scroll(&mut self, member: GraphMemberId, band_y: u32, band_h: u32) {
        // The host requests in physical coords; the actor works in logical. Convert in,
        // and dedup in logical space. (Auto-DPI D2.)
        let band_y = ((band_y as f32) / self.dpr).round() as u32;
        let band_h = ((band_h as f32) / self.dpr).round().max(1.0) as u32;
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        // The document lane carries a packet the host windows directly; only the
        // flat HTML/serval scene needs an actor-side re-emit.
        if activation.packet.is_some() {
            return;
        }
        if activation.requested_band == (band_y, band_h) {
            return;
        }
        activation.requested_band = (band_y, band_h);
        activation.handle.command(ContentCommand::Scroll {
            band_y,
            band_h,
            // Scroll is not a resize: the viewport generation is unchanged, so the
            // re-emitted band is accepted at the current generation.
            viewport_gen: activation.gens.viewport,
        });
    }

    /// Ask `member`'s actor to find `query` in its current document (find-in-page),
    /// shipping back the match rects the host highlights. Deduped against the last
    /// query, so the host can call this every frame the find bar is open without
    /// re-commanding. An empty query clears the matches locally (and tells the actor
    /// to clear too). No-op for a node that is not active. (Find-in-page.)
    pub fn request_find(&mut self, member: GraphMemberId, query: &str) {
        let Some(activation) = self.active.get_mut(&member) else {
            return;
        };
        if activation.find_query == query {
            return;
        }
        activation.find_query = query.to_string();
        if query.is_empty() {
            // Clear locally now; no need to round-trip an empty query for rects.
            activation.find_matches.clear();
        }
        activation.handle.command(ContentCommand::Find {
            query: query.to_string(),
            viewport_gen: activation.gens.viewport,
        });
    }

    /// Materialize `member`'s outbound-link neighborhood as graph nodes +
    /// `Semantic:Hyperlink` edges (relational-browse V1): the host invokes this to
    /// place the open page's link neighborhood on the canvas. The actor parses the
    /// already-fetched body and emits a `Contribution` — render-free, no target fetch,
    /// no new actor. No-op for a node that is not active. (Extraction lane, single-hop.)
    pub fn materialize_links(&self, member: GraphMemberId) {
        let Some(activation) = self.active.get(&member) else {
            return;
        };
        activation.handle.command(ContentCommand::MaterializeLinks {
            viewport_gen: activation.gens.viewport,
        });
    }

    /// Attach a DocumentScript at `component_path` to `member`'s page, under the
    /// host-resolved `log` / `document` capability permissions (P2.5). The actor
    /// mirrors the page into a mutable `ScriptedDom` the script edits and re-renders
    /// from it; a denied required capability fails instantiation (reported via
    /// `ScriptOutcome`). No-op for a node that is not active.
    pub fn attach_script(
        &self,
        member: GraphMemberId,
        component_path: PathBuf,
        log: ResolvedPermission,
        document: ResolvedPermission,
        net: ResolvedPermission,
    ) {
        let Some(activation) = self.active.get(&member) else {
            return;
        };
        activation.handle.command(ContentCommand::AttachScript {
            component_path,
            log,
            document,
            net,
            viewport_gen: activation.gens.viewport,
        });
    }

    /// Deliver one event to `member`'s attached DocumentScript; the script's batch is
    /// applied to the live DOM and the tile re-renders. No-op if no script is
    /// attached or the node is not active. (P2.5.)
    pub fn deliver_script_event(&self, member: GraphMemberId, kind: String, payload: String) {
        let Some(activation) = self.active.get(&member) else {
            return;
        };
        activation.handle.command(ContentCommand::DeliverEvent {
            kind,
            payload,
            viewport_gen: activation.gens.viewport,
        });
    }

    /// Whether `member` is active and on the scripted render rung (`serval.scripted`).
    /// The host checks this to decide a click on the tile routes to script dispatch
    /// ([`click_scripted`](Self::click_scripted)) rather than the orrery beneath.
    /// (Render ladder phase 3.)
    #[cfg(feature = "scripted")]
    pub fn is_scripted(&self, member: GraphMemberId) -> bool {
        self.active
            .get(&member)
            .is_some_and(|a| a.engine == inker::routing::ENGINE_SERVAL_SCRIPTED)
    }

    /// Forward a pointer click at card-local scene point `(x, y)` (device px) to
    /// `member`'s scripted render rung: the live document hit-tests the point and
    /// dispatches a `click`, so the page's listeners run, then the tile re-renders.
    /// No-op if the node is not active or not on the scripted rung. The host calls
    /// this when a click lands on a `serval.scripted` tile. (Render ladder phase 3 —
    /// the input → event bridge.)
    #[cfg(feature = "scripted")]
    pub fn click_scripted(&self, member: GraphMemberId, x: f32, y: f32) {
        let Some(activation) = self.active.get(&member) else {
            return;
        };
        activation.handle.command(ContentCommand::ScriptedClick {
            x,
            y,
            viewport_gen: activation.gens.viewport,
        });
    }

    /// Detach `member`'s DocumentScript (runs its `deactivate`) and revert to the
    /// static page. No-op for a node that is not active. (P2.5.)
    pub fn detach_script(&self, member: GraphMemberId) {
        let Some(activation) = self.active.get(&member) else {
            return;
        };
        activation.handle.command(ContentCommand::DetachScript {
            viewport_gen: activation.gens.viewport,
        });
    }

    /// `member`'s find-in-page match rects (full-document px, one inner slice per
    /// match), from the latest [`request_find`](Self::request_find). Empty when no
    /// query is active or nothing matched. The host maps these into the card the same
    /// way it maps link rects (offset by the card scroll). (Find-in-page.)
    pub fn find_matches(&self, member: GraphMemberId) -> &[Vec<[f32; 4]>] {
        self.active.get(&member).map_or(&[], |a| &a.find_matches)
    }

    /// The member's retained document-lane packet (plus font sidecar), if it is a
    /// document-lane node that has rendered. The host windows + lowers a band of this
    /// per scroll; `None` for an HTML-lane node (use [`scene`](Self::scene)) or an
    /// unrendered one. (Retained-text / tiled render.)
    pub fn packet(&self, member: GraphMemberId) -> Option<(&DocumentRenderPacket, &FontTable)> {
        self.active
            .get(&member)
            .and_then(|a| a.packet.as_ref().map(|p| (p, &a.fonts)))
    }

    /// The URL of the clickable link whose content-local rect contains `(x, y)`, if
    /// any. `(x, y)` is in the document's own coordinate space (the host subtracts
    /// the card's window origin and adds its scroll before calling). Last match wins,
    /// so a link nested in an enclosing region resolves to the innermost. `None` when
    /// the point lands on no link (the caller keeps its normal click). (Inline-link nav.)
    pub fn link_at(&self, member: GraphMemberId, x: f32, y: f32) -> Option<&str> {
        // The host passes physical document coords; link rects are in the actor's
        // logical space. Convert the query point in. (Auto-DPI D2.)
        let (x, y) = (x / self.dpr, y / self.dpr);
        let activation = self.active.get(&member)?;
        // Document lane: hit-test the retained packet's interactions directly (the
        // query API subsumes the old parallel link-rect table). The Scene lanes
        // (HTML, smolweb, scripted) have no packet, so they walk `links` — the
        // rect table each lane harvests at render time and ships on
        // `ContentUpdate::Scene` (link_harvest / IncrementalLayout::link_rects).
        // (A prior version of this comment said `links` was "empty until the
        // Phase 5 lane parity"; that went stale and misled a review — every lane
        // populates it now.)
        if let Some(packet) = &activation.packet {
            return packet.link_at(x, y);
        }
        activation
            .links
            .iter()
            .rev()
            .find(|l| x >= l.rect[0] && x <= l.rect[2] && y >= l.rect[1] && y <= l.rect[3])
            .map(|l| l.url.as_str())
    }

    /// Whether `member` is recovering from a crash: its actor was respawned and has
    /// not delivered a fresh scene yet. The host shows a placeholder for it (a tab
    /// that crashed before ever rendering would otherwise be blank). A tab that
    /// crashed *after* rendering keeps its last scene and reads as not-recovering.
    pub fn is_recovering(&self, member: GraphMemberId) -> bool {
        self.active
            .get(&member)
            .is_some_and(|a| a.respawns > 0 && a.scene.is_none() && a.packet.is_none())
    }

    /// Deactivate `member` now — its actor winds down on drop. For when a tile is
    /// closed, or the node is gone (deleted). The node's "last
    /// visit" snapshot is re-derived host-side from the durable cache, so nothing
    /// needs retaining here.
    pub fn reap(&mut self, member: GraphMemberId) {
        self.active.remove(&member);
    }

    /// Reap **every** active node, returning the pool to empty. The host calls
    /// this on a multi-graph switch so the prior session's content actors stop
    /// and don't bleed into the new graph. (Multi-graph MG2.)
    pub fn clear(&mut self) {
        self.active.clear();
        self.pool = Pool::new();
    }

    /// Reap only the active nodes belonging to `graph` — the per-graph form of
    /// [`clear`](Self::clear). A session switch parks the *outgoing* graph this way,
    /// so another live graph's actors (in this or another window's panes) survive.
    /// Unlike `clear` it does **not** reset the worker pool: the pool is shared
    /// across every live graph, so resetting it would kill the survivors' actors.
    /// (Window composition P1, multi-graph.)
    pub fn reap_graph(&mut self, graph: GraphId) {
        self.active.retain(|_, a| a.graph_id != graph);
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
            }
            None => false,
        }
    }

    /// Feed a fetched subresource to one node's actor (a durable-cache hit routed
    /// to the node that wanted it).
    pub fn send_resource(&self, member: GraphMemberId, url: String, bytes: Vec<u8>) {
        if let Some(activation) = self.active.get(&member) {
            activation
                .handle
                .command(ContentCommand::Resource { url, bytes });
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
}
