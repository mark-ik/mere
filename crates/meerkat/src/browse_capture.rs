/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! C1 — the live trail recorder.
//!
//! Persists a navigation as a durable, LocalOnly `BrowsingTrace` the moment it
//! happens, so the user's own trail accrues in eidetic instead of being lost.
//! Before this, nothing in the running app wrote a trace — only the
//! `project_lineage` bridge, which has no live caller (the capture/provenance/
//! consent plan, C1).
//!
//! This first slice writes one trace per navigation (durable, crash-safe);
//! batching open events into segments and aging them out under a quota is the
//! retention refinement (plan C4). Titles and a richer transition kind ride the
//! same record once their producers are wired; the record shape carries `from`,
//! `to`, `transition`, and `at_ms` from the first traversal so neither needs a
//! migration later.

use eidetic::browsing::{save_trace, BrowsingTrace, PageRef, TraceEvent, TraceTransition};

use super::WindowCtx;

/// Milliseconds since the Unix epoch — the trace clock. Mirrors the timestamp
/// the eidetic tombstone / forgetting passes use.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Build a single-event browsing trace for a navigation from `from_url` to
/// `to_url`. An empty `from_url` is an origin event (`from: None`): the first
/// visit of a session, or a typed URL into a fresh surface.
fn build_nav_trace(
    owner: &str,
    from_url: &str,
    to_url: &str,
    transition: TraceTransition,
    at_ms: u64,
) -> BrowsingTrace {
    let from = (!from_url.is_empty()).then(|| PageRef {
        url: from_url.to_string(),
        title: None,
    });
    let event = TraceEvent {
        from,
        to: PageRef {
            url: to_url.to_string(),
            title: None,
        },
        transition,
        at_ms,
        dwell_ms: None,
    };
    BrowsingTrace::from_events(owner, vec![event])
}

impl WindowCtx<'_> {
    /// Record a navigation to `to` as a durable, LocalOnly browsing trace (C1).
    ///
    /// A no-op when capture is disabled (the exclusion hook the consent layer
    /// will drive — plan C4) or when no content store is open. The `from` page
    /// is whatever is shown right now, so the caller must invoke this **before**
    /// advancing `self.view.content_location` to the new page. fjall resolves
    /// synchronously, so the `block_on` does not stall the UI thread (the same
    /// discipline as the deleted-node tombstone write).
    pub(super) fn record_browse_nav(&mut self, to: &str, transition: TraceTransition) {
        if !self.shared.content.capture_enabled {
            return;
        }
        // Resolve everything owned first, so no borrow outlives the mutable
        // store borrow below.
        let from_url = self.view.content_location.clone();
        let owner = format!("{}", self.shared.session.active_persona.0);
        let now = now_ms();
        let trace = build_nav_trace(&owner, &from_url, to, transition, now);

        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        if let Err(err) = pollster::block_on(save_trace(store, &trace, now)) {
            tracing::warn!(?err, "failed to record a browsing trace");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_from_is_an_origin_event() {
        let trace = build_nav_trace("p", "", "https://to.test/", TraceTransition::UrlTyped, 42);
        assert_eq!(trace.owner, "p");
        assert_eq!(trace.events.len(), 1);
        let event = &trace.events[0];
        assert!(event.from.is_none(), "an empty from is an origin event");
        assert_eq!(event.to.url, "https://to.test/");
        assert_eq!(event.transition, TraceTransition::UrlTyped);
        assert_eq!(event.at_ms, 42);
        assert_eq!(trace.started_at_ms, 42);
        assert_eq!(trace.ended_at_ms, 42);
    }

    #[test]
    fn a_navigation_carries_both_ends_and_its_kind() {
        let trace = build_nav_trace(
            "persona-1",
            "https://from.test/a",
            "https://to.test/b",
            TraceTransition::Back,
            100,
        );
        let event = &trace.events[0];
        assert_eq!(event.from.as_ref().map(|p| p.url.as_str()), Some("https://from.test/a"));
        assert_eq!(event.to.url, "https://to.test/b");
        assert_eq!(event.transition, TraceTransition::Back);
    }
}
