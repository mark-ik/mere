/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Session persistence orchestration.
//!
//! Bridges the gpui-side entities (`Entity<Graph>` in
//! `GraphRegistry`, `Entity<ManifestStore>`) to the portable
//! file-I/O surfaces in `mere-host-runtime` (`ManifestStore::flush_dirty`,
//! `session_graph_store::save_graph`).
//!
//! Three save paths exist today:
//!
//! 1. **On session creation** — handled implicitly by
//!    [`crate::graph_registry::GraphRegistry::create_graph_seeded`]
//!    which inserts the new manifest (marking it dirty). The next
//!    flush writes it to disk.
//! 2. **On new-tile navigation** — [`flush_session`] is called
//!    from `navigate_to`'s `NewTile` branch right after the new
//!    node is minted. Persists the graph snapshot + flushes
//!    manifests so a crash doesn't lose the user's anchor work.
//! 3. **On app quit** — [`flush_all`] is registered via
//!    `App::on_app_quit` in `bootstrap::run`. Final pass; catches
//!    anything not flushed by the per-mutation path (orrery
//!    drags, edge edits, manifest field updates).

use gpui::{App, Entity};
use mere_frame::GraphId;
use mere_host_runtime::{ManifestStore, session_graph_store};

use crate::graph_registry::GraphRegistry;

/// Persist a single session's graph snapshot to disk and flush any
/// pending manifest writes. Idempotent — safe to call multiple
/// times. Errors are logged + swallowed so a transient disk
/// failure doesn't bring down the user's session.
pub(crate) fn flush_session(
    registry: &Entity<GraphRegistry>,
    manifests: &Entity<ManifestStore>,
    graph_id: GraphId,
    cx: &mut App,
) {
    let Some(sessions_root) = crate::persistence::sessions_dir() else {
        return;
    };
    let Some(session_id) = registry.read(cx).session_for_graph(graph_id) else {
        tracing::warn!(?graph_id, "flush_session: no session for graph");
        return;
    };
    let Some(graph_entity) = registry.read(cx).get(graph_id).cloned() else {
        return;
    };
    let session_dir = sessions_root.join(session_id.as_uuid().to_string());
    if let Err(e) = std::fs::create_dir_all(&session_dir) {
        tracing::warn!(?session_id, error = %e, "create_dir_all failed");
        return;
    }
    {
        let graph_ref = graph_entity.read(cx);
        if let Err(e) = session_graph_store::save_graph(&session_dir, graph_ref) {
            tracing::warn!(?session_id, error = %e, "session graph save failed");
        }
    }
    // The manifest store remembers what's dirty; flushing here
    // catches manifest mutations from create_graph_seeded that
    // haven't been written yet.
    manifests.update(cx, |store, _| {
        if let Err(e) = store.flush_dirty() {
            tracing::warn!(?session_id, error = %e, "manifest flush failed");
        }
    });
}

/// Persist **every** session's graph + flush all dirty manifests.
/// Registered as the app-quit handler. Best-effort: per-session
/// errors are logged but don't block other sessions or quit
/// itself.
pub(crate) fn flush_all(
    registry: &Entity<GraphRegistry>,
    manifests: &Entity<ManifestStore>,
    cx: &mut App,
) {
    let Some(sessions_root) = crate::persistence::sessions_dir() else {
        tracing::warn!("flush_all: no data_local_dir; skipping");
        return;
    };
    let pairs: Vec<_> = manifests
        .read(cx)
        .iter()
        .map(|(sid, m)| (sid, m.root_graph_id))
        .collect();
    let pair_count = pairs.len();
    for (session_id, graph_id) in pairs {
        let session_dir = sessions_root.join(session_id.as_uuid().to_string());
        if let Err(e) = std::fs::create_dir_all(&session_dir) {
            tracing::warn!(?session_id, error = %e, "create_dir_all failed");
            continue;
        }
        let Some(graph_entity) = registry.read(cx).get(graph_id).cloned() else {
            tracing::debug!(?session_id, ?graph_id, "graph not registered; skip");
            continue;
        };
        let graph_ref = graph_entity.read(cx);
        if let Err(e) = session_graph_store::save_graph(&session_dir, graph_ref) {
            tracing::warn!(?session_id, error = %e, "session graph save failed");
        } else {
            tracing::debug!(?session_id, "session.graph_saved");
        }
    }
    manifests.update(cx, |store, _| {
        match store.flush_dirty() {
            Ok(count) => tracing::info!(
                manifests_flushed = count,
                sessions = pair_count,
                "session.flush_all_complete"
            ),
            Err(e) => tracing::warn!(error = %e, "manifest flush_dirty failed"),
        }
    });
}
