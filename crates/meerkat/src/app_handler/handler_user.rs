/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell::on_user_event — actor-wakeup drain (extracted from user_event).

use super::*;

impl Shell {
    pub(crate) fn on_user_event(&mut self) {
        // An actor may wake us before the primary window exists; the typed channels
        // buffer, so we drain on the next wake after `resumed`. (MW2 (d).)
        if self.primary.is_none() {
            return;
        }
        let mut wc = self.ctx();
        wc.drain_portable_diagnostics();
        wc.drain_a11y_actions();
        // The kernel inbox dispatch: the one documented place that applies what the
        // actors tell the kernel. Each typed stream is drained and folded into
        // canonical state here on the kernel thread; the actors never touch it.
        let mut card_changed = false;
        let mut graph_changed = false;
        // One `FetchUpdate` stream carries both page documents and subresources.
        while let Ok(update) = wc.shared.inbox.fetch.try_recv() {
            match update {
                fetch::FetchUpdate::Page(outcome) => {
                    let state = match outcome.result {
                        Ok(fetched) => {
                            // Persist so a reload shows this page without
                            // re-fetching. Linked-data harvest now happens in the
                            // content actor (on `Show`), which ships a `Contribution`.
                            wc.save_cached(
                                &outcome.url,
                                fetched.content_type.clone(),
                                fetched.body.as_bytes(),
                            );
                            wc.shared.observability.record_actor(
                                "fetch",
                                "succeeded",
                                Some(outcome.url.clone()),
                            );
                            // Best-effort: fetch this page's favicon so its graph
                            // tile can show a real icon. The bytes route back as
                            // `FetchUpdate::Favicon`, keyed to this page url.
                            // (Favicon-on-tile.)
                            if let Some(icon_url) = favicon_url_for(&outcome.url, &fetched.body) {
                                wc.shared.content.fetch_handle.command(
                                    fetch::FetchCommand::Favicon {
                                        owner_url: outcome.url.clone(),
                                        url: icon_url,
                                    },
                                );
                            }
                            fetch::ContentState::Ready(fetched)
                        }
                        Err(reason) => {
                            let detail = format!("{}: {reason}", outcome.url);
                            wc.shared.observability.record_actor(
                                "fetch",
                                "failed",
                                Some(detail.clone()),
                            );
                            wc.shared.observability.record_diagnostic(
                                "meerkat.actor.fetch.failed",
                                Severity::Warn,
                                detail,
                            );
                            fetch::ContentState::Failed(reason)
                        }
                    };
                    wc.shared.content.pages.insert(outcome.url, state);
                    card_changed = true;
                    // The fetch may have set cookies; persist this persona's session
                    // so a login survives a restart (dirty-gated, no-op when nothing
                    // changed). (Native session store; durability thread.)
                    if let Some(store) = wc.shared.content.store.as_mut() {
                        fetch::persist_cookies(store, wc.shared.session.active_persona);
                    }
                }
                // A subresource (page CSS / media): persist it (its content-type is
                // unknown here) so the page's assets survive restart, then broadcast
                // the bytes to every active node's actor — the fetch stream is keyed
                // by URL, not by which node wanted it; each actor dedups via its own
                // resource store and only the one that wanted it re-renders.
                fetch::FetchUpdate::Subresource(sub) => {
                    wc.shared.observability.record_actor(
                        "fetch",
                        "subresource",
                        Some(sub.url.clone()),
                    );
                    wc.save_cached(&sub.url, None, &sub.bytes);
                    wc.shared
                        .content
                        .constellation
                        .broadcast_resource(&sub.url, &sub.bytes);
                }
                // A page's favicon arrived: decode it to RGBA and stamp it on the
                // node currently at the owner url, so the orrery paints it on that
                // node's tile. Snapshot persistence carries it across restarts.
                // (Favicon-on-tile.)
                fetch::FetchUpdate::Favicon { owner_url, bytes } => {
                    if let Some(decoded) = serval_layout::decode_image_bytes(&bytes) {
                        graph_changed |= wc.orrery_mut().set_node_favicon(
                            &owner_url,
                            decoded.rgba,
                            decoded.width,
                            decoded.height,
                        );
                    }
                }
            }
        }
        // Drain the find worker's replies: apply the latest query's match rects (older
        // generations are dropped) and feed any wanted subresources back from the cache.
        // Collected first so the receiver borrow ends before the &mut apply. (Find.)
        let mut find_results = Vec::new();
        while let Ok(result) = wc.shared.inbox.find.try_recv() {
            find_results.push(result);
        }
        for result in find_results {
            card_changed |= wc.apply_find_result(result);
        }
        // Drain every active node's actor in one pass: scenes land in the pool, the
        // wanted subresources + harvested contributions come back for the host.
        let drained = wc.shared.content.constellation.drain();
        card_changed |= drained.any_scene;
        if !drained.respawned.is_empty() {
            // A content tile's actor died (panic, isolated to its thread) and the
            // pool respawned it; redraw so the next frame re-Shows it (self-healing).
            tracing::warn!(
                count = drained.respawned.len(),
                "respawned crashed content tile(s)"
            );
            wc.shared.observability.record_actor(
                "content",
                "respawned",
                Some(format!("count={}", drained.respawned.len())),
            );
            wc.shared.observability.record_diagnostic(
                "meerkat.actor.content.respawned",
                Severity::Warn,
                format!("count={}", drained.respawned.len()),
            );
            card_changed = true;
        }
        for (member, urls) in drained.wanted {
            // The actor deduped these; a durable-cache hit feeds that node directly,
            // a miss spawns a network fetch whose bytes broadcast back on arrival.
            for url in urls {
                if let Some(stored) = wc.load_cached(&url) {
                    wc.shared
                        .content
                        .constellation
                        .send_resource(member, url, stored.body);
                } else {
                    wc.shared
                        .content
                        .fetch_handle
                        .command(fetch::FetchCommand::Subresource(url));
                }
            }
        }
        if !drained.contributions.is_empty() {
            // Each contribution carries the graph its harvesting node belongs to.
            // Apply the focused graph's here, through the ctx's bundled orrery; a
            // background graph's contributions route to that graph's pooled orrery
            // with the multi-graph flip (at one focused graph there are none).
            let focused = wc.view.focused_graph;
            graph_changed |= wc.orrery_mut().ingest_graph(|g| {
                let mut changed = false;
                for (gid, contribution) in &drained.contributions {
                    if *gid != focused {
                        continue;
                    }
                    let outcome = linked_data::apply_contribution(g, contribution);
                    changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
                }
                changed
            });
        }
        // The crawl actor (relational-browse V2) harvests off the render path; drain its
        // link + metadata contributions on this same wake and apply them to the crawl's
        // graph, exactly as the content harvests above. (At one focused graph, a crawl
        // seeded elsewhere awaits the multi-graph flip, like content contributions.)
        let crawl_pairs = wc.shared.content.crawl.drain();
        if !crawl_pairs.is_empty() {
            let focused = wc.view.focused_graph;
            graph_changed |= wc.orrery_mut().ingest_graph(|g| {
                let mut changed = false;
                for (gid, contribution) in &crawl_pairs {
                    if *gid != focused {
                        continue;
                    }
                    let outcome = linked_data::apply_contribution(g, contribution);
                    changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
                }
                changed
            });
        }
        // Fold the crawl's progress into its toolbar chip, but only when it changed (the
        // drain runs on every wake; the chip update should not churn the chrome). The
        // chip reads "crawling/crawled: N pages", or hides when no crawl has run. Captured
        // for the MW3 chrome fan-out below, since one crawl is shared kernel state and
        // every window's toolbar should show the same chip.
        let mut latest_crawl = None;
        {
            let progress = wc.shared.content.crawl.progress();
            let (running, fetched) = (progress.running, progress.fetched);
            let current = wc.view.chrome().crawl.clone();
            if current.running != running || current.fetched != fetched {
                let indicator = meerkat::CrawlIndicator { running, fetched };
                wc.view.chrome_update(|c| c.crawl = indicator.clone());
                latest_crawl = Some(indicator);
            }
        }
        // P2P sync status (S5.0): the same wake also carries lane-status changes.
        // Fold the latest into the chrome chip (the host owns the mutation).
        //
        // MW2 (d2) deferred to MW3: the sync-chip + comms writes below target the
        // primary's chrome (`wc.view.runner`). At N>1 they fan out to every window
        // whose template carries that chrome — a two-phase restructure (drain +
        // collect here, then replay per `self.windows.values_mut()` after the ctx
        // borrow ends). Done when the second window exists to test it against, so the
        // fan-out isn't an untested loop over one element through the actor-drain
        // hot path. (Multi-window plan MW3.)
        let mut latest_sync = None;
        while let Ok(update) = wc.shared.inbox.sync.try_recv() {
            wc.shared.observability.record_actor(
                "sync",
                "status",
                Some(format!(
                    "syncing={};ops={}",
                    update.status.syncing, update.status.ops_received
                )),
            );
            latest_sync = Some(sync::to_indicator(&update, sync::LANE_LABEL));
        }
        // Apply to the primary inline; `latest_sync` is replayed onto chrome-bearing
        // secondaries after the ctx borrow ends (step-5 fan-out below), so borrow it
        // here rather than moving it out.
        if let Some(indicator) = &latest_sync {
            wc.view.chrome_update(|c| c.sync = indicator.clone());
            wc.view.request_redraw();
        }
        // Comms (P6c): the comms actor delivers conversation lists + threads here;
        // fold each into the docked pane (the host owns the chrome mutation). Each
        // update is also collected for the step-5 fan-out onto chrome-bearing secondaries.
        let mut comms_changed = false;
        let mut comms_updates: Vec<comms_host::CommsUpdate> = Vec::new();
        while let Ok(update) = wc.shared.inbox.comms.try_recv() {
            comms_updates.push(update.clone());
            match update {
                comms_host::CommsUpdate::Inbox(inbox) => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "inbox",
                        Some(format!("conversations={}", inbox.conversations.len())),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("inbox".to_string()),
                    );
                    wc.view.chrome_update(|c| c.comms.set_inbox(inbox.clone()));
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Thread(id, messages) => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "thread",
                        Some(format!("{} messages={}", id.key, messages.len())),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("thread".to_string()),
                    );
                    wc.view.chrome_update(|c| {
                        if c.comms.selected() == Some(&id) {
                            c.comms.set_thread(messages.clone());
                        }
                    });
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Sent(_id) => {
                    wc.shared.observability.record_actor("comms", "sent", None);
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("sent".to_string()),
                    );
                    wc.view.chrome_update(|c| c.clear_comms_draft());
                    comms_changed = true;
                }
                comms_host::CommsUpdate::SendOutcome(line) => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "send_outcome",
                        Some(line.clone()),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("send_outcome".to_string()),
                    );
                    wc.view
                        .chrome_update(|c| c.comms.set_send_status(line.clone()));
                    comms_changed = true;
                }
                comms_host::CommsUpdate::Identity {
                    misfin_address,
                    cabal_ticket,
                } => {
                    wc.shared.observability.record_actor(
                        "comms",
                        "identity",
                        Some(format!(
                            "misfin={};cabal={}",
                            misfin_address,
                            cabal_ticket.is_some()
                        )),
                    );
                    wc.shared.observability.record_actor(
                        "comms",
                        "succeeded",
                        Some("identity".to_string()),
                    );
                    wc.view.chrome_update(|c| {
                        c.comms
                            .set_identity(misfin_address.clone(), cabal_ticket.clone())
                    });
                    comms_changed = true;
                }
            }
        }
        if comms_changed {
            wc.view.request_redraw();
        }
        if graph_changed {
            wc.save_session();
        }
        if card_changed || graph_changed {
            wc.view.request_redraw();
        }
        // The physics actor shares this wake: a fresh layout snapshot is waiting to
        // be folded in. The orrery is always shown now, so kick a redraw — `frame()`
        // drains the snapshot and reports whether to keep going (the settle then
        // self-sustains through `needs_redraw`).
        wc.drain_portable_diagnostics();
        wc.view.request_redraw();
        drop(wc);
        // MW3 step 5 (the deferred d2): the drain above ran through the primary's ctx
        // (`ctx()` always bundles the primary), so only the primary was woken. Every
        // secondary window shares the same kernel, caches, and pooled orreries and shows
        // the same physics-animated graph, so this same wake must repaint them too. Fan
        // the redraw out now the ctx borrow has ended (the two-phase shape the d2 note
        // called for).
        self.redraw_secondary_windows();
        // MW3 step 5 chrome fan-out: the sync chip + comms updates were applied to the
        // primary's chrome inline above; replay them onto every *other* window now the
        // ctx borrow has ended. Note a slim leaf is NOT skipped: "slim" omits the
        // *shellbar*, but the leaf keeps the toolbar (where the sync chip lives) and can
        // open the comms pane, so it must see these updates too — driving caught a leaf
        // showing a stale "p2p off" while the primary showed real standing. (MW3 step 5.)
        if latest_sync.is_some() || latest_crawl.is_some() || !comms_updates.is_empty() {
            let primary = self.primary;
            for (id, view) in self.windows.iter_mut() {
                if Some(*id) == primary {
                    continue;
                }
                if let Some(indicator) = &latest_sync {
                    view.chrome_update(|c| c.sync = indicator.clone());
                }
                if let Some(indicator) = &latest_crawl {
                    view.chrome_update(|c| c.crawl = indicator.clone());
                }
                for update in &comms_updates {
                    apply_comms_to_chrome(view, update);
                }
                view.request_redraw();
            }
        }
    }
}
