/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The find-in-page worker: a single off-thread actor that lays out a page and
//! returns its match rects.
//!
//! Find runs `find_content` (a full serval cascade + layout) which costs ~1-2s on a
//! large page — far too slow on the UI thread per keystroke. A dedicated worker keeps
//! it off the critical path: the host ships the focused page's body + query stamped
//! with a generation, the worker lays out and ships the rects back, and the host
//! keeps only the latest generation's result. Unlike the content actor, this worker is
//! never busy rendering, so find is answered promptly regardless of a node's activation
//! state — sidestepping the snapshot-not-active and render-starvation failures of the
//! actor path.
//!
//! Subresources (stylesheets/images) converge as for the content actor: a layout that
//! misses one records it [`Wanted`](FindResult::wanted); the host feeds the cached
//! bytes back as [`Resource`](FindCommand::Resource) and the next query lays out styled.
//! The worker keeps one shared resource store across pages.

use std::cell::RefCell;
use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Emitter, Wake, spawn};
use mere::forme::GraphMemberId;
use inker::{EngineRegistry, EngineRoutePolicy};

use crate::fetch::{ContentState, Fetched};
use crate::resources::{ResourceLoader, ResourceStore};

/// A command to the find worker.
pub enum FindCommand {
    /// Find `query` in `body` (the focused page), laid out at `(w, h)`. Stamped with a
    /// monotonic `generation` so the host drops all but the latest; `member` is echoed
    /// back so the host applies the rects to the right node.
    Find {
        generation: u64,
        member: GraphMemberId,
        url: String,
        content_type: Option<String>,
        body: String,
        w: u32,
        h: u32,
        query: String,
    },
    /// A fetched subresource (CSS/image) for the worker's shared store, so the next
    /// layout resolves it (the same converge-over-queries path the content actor uses).
    Resource { url: String, bytes: Vec<u8> },
}

/// The worker's reply: the match rects (full-document px) plus any subresources the
/// layout wanted but missed (the host fetches them from its cache and feeds them back).
pub struct FindResult {
    pub generation: u64,
    pub member: GraphMemberId,
    pub matches: Vec<Vec<[f32; 4]>>,
    pub wanted: Vec<String>,
}

/// Spawn the find worker on its own thread. Returns the command handle + the reply
/// receiver the host drains. The worker builds its own (`!Send`) engine registry on the
/// thread, like the content actor. One dedicated thread (not pooled): there is exactly
/// one find worker and it is long-lived.
pub fn spawn_find_worker(wake: Wake) -> (ActorHandle<FindCommand>, Receiver<FindResult>) {
    spawn(wake, move |commands, out: Emitter<FindResult>| {
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            registry.register(engine);
        }
        let policy = EngineRoutePolicy::default();
        let store = RefCell::new(ResourceStore::default());

        while let Ok(first) = commands.recv() {
            // Coalesce the queue: apply every pending Resource and keep only the *latest*
            // Find, so fast typing lays out once (the current query) rather than once per
            // keystroke. A full layout costs seconds, so skipping superseded queries is
            // the difference between one search and a backlog of stale ones.
            let mut latest_find = None;
            let mut next = Some(first);
            loop {
                match next.take() {
                    Some(FindCommand::Resource { url, bytes }) => {
                        store.borrow_mut().insert(url, bytes);
                    }
                    Some(find) => latest_find = Some(find),
                    None => {}
                }
                match commands.try_recv() {
                    Ok(command) => next = Some(command),
                    Err(_) => break,
                }
            }
            if let Some(FindCommand::Find {
                generation,
                member,
                url,
                content_type,
                body,
                w,
                h,
                query,
            }) = latest_find
            {
                let state = ContentState::Ready(Fetched { content_type, body });
                let wanted = RefCell::new(Vec::new());
                let matches = {
                    let loader = ResourceLoader::new(&store, &url, &wanted);
                    crate::card::find_content(
                        &url,
                        Some(&state),
                        &registry,
                        &policy,
                        &loader,
                        w,
                        h,
                        &query,
                    )
                };
                out.emit(FindResult {
                    generation,
                    member,
                    matches,
                    wanted: wanted.into_inner(),
                });
            }
        }
    })
}
