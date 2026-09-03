// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::typed::list_typed;
use async_trait::async_trait;
use std::collections::HashMap;

// The in-memory test store is muniment's (2026-07-12): eidetic's
// hand-rolled one was the same map behind the same seam.
use muniment::MemoryBackend as InMemoryStore;

fn page(url: &str) -> PageRef {
    PageRef {
        url: url.to_string(),
        title: Some(format!("title of {url}")),
    }
}

fn event(from: Option<&str>, to: &str, at_ms: u64) -> TraceEvent {
    TraceEvent {
        from: from.map(page),
        to: page(to),
        transition: TraceTransition::LinkClick,
        at_ms,
        dwell_ms: None,
        candidates: Vec::new(),
    }
}

#[test]
fn a_trace_round_trips_as_a_localonly_typed_payload() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        bootstrap_browsing_schema(&mut store).await.unwrap();

        let trace = BrowsingTrace::from_events(
            "mark",
            vec![
                event(None, "https://a.example", 10),
                event(Some("https://a.example"), "https://b.example", 20),
            ],
        );
        assert_eq!(trace.started_at_ms, 10);
        assert_eq!(trace.ended_at_ms, 20);

        let id = save_trace(&mut store, &trace, 99).await.unwrap();
        let manifests = list_typed::<BrowsingTrace>(&mut store).await.unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].privacy, PrivacyClass::LocalOnly);

        let mut fetcher = NoFetcher;
        let loaded: BrowsingTrace = load_typed(&mut store, &mut fetcher, id)
            .await
            .unwrap()
            .expect("stored trace loads");
        assert_eq!(loaded, trace);
    });
}

#[test]
fn bootstrap_is_idempotent() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        bootstrap_browsing_schema(&mut store).await.unwrap();
        let count_after_first = store.list("").await.unwrap().len();
        bootstrap_browsing_schema(&mut store).await.unwrap();
        assert_eq!(store.list("").await.unwrap().len(), count_after_first);
    });
}

#[test]
fn reads_answer_over_flushed_and_open_segments_together() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut memory = BrowsingMemory::new(2);

        // Two events flush into a stored trace; a third stays open.
        assert!(!memory.record_traversal("mark", event(None, "https://a.example", 10)));
        assert!(memory.record_traversal(
            "mark",
            event(Some("https://a.example"), "https://b.example", 20)
        ));
        let ids = memory.flush(&mut store, 100).await.unwrap();
        assert_eq!(ids.len(), 1);
        memory.record_traversal(
            "mark",
            event(Some("https://b.example"), "https://c.example", 30),
        );

        // Corridor spans both: the last 2 of 3 events, chronological.
        let corridor = memory.recent_corridor(2);
        assert_eq!(corridor.len(), 2);
        assert_eq!(corridor[0].to.url, "https://b.example");
        assert_eq!(corridor[1].to.url, "https://c.example");

        // Window slices and dedups.
        let window = memory.nodes_visited_in_window(15, 35);
        let urls: Vec<&str> = window.iter().map(|p| p.url.as_str()).collect();
        assert_eq!(urls, vec!["https://b.example", "https://c.example"]);

        // Co-occurrence counts direct traversals in either direction.
        assert_eq!(
            memory.co_occurrence("https://a.example", "https://b.example"),
            1
        );
        assert_eq!(
            memory.co_occurrence("https://b.example", "https://a.example"),
            1
        );
        assert_eq!(
            memory.co_occurrence("https://a.example", "https://c.example"),
            0
        );
    });
}

#[test]
fn load_reconstructs_memory_from_the_store() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut memory = BrowsingMemory::new(1);
        memory.record_traversal("mark", event(None, "https://a.example", 10));
        memory.flush(&mut store, 50).await.unwrap();
        memory.record_traversal(
            "mark",
            event(Some("https://a.example"), "https://b.example", 20),
        );
        memory.flush(&mut store, 60).await.unwrap();

        let reloaded = BrowsingMemory::load(&mut store, 8).await.unwrap();
        assert_eq!(reloaded.traces().count(), 2);
        let corridor = reloaded.recent_corridor(10);
        assert_eq!(corridor.len(), 2);
        assert_eq!(corridor[1].to.url, "https://b.example");
    });
}

#[test]
fn per_owner_segments_flush_into_per_owner_traces() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut memory = BrowsingMemory::new(8);
        memory.record_traversal("work", event(None, "https://docs.example", 10));
        memory.record_traversal("home", event(None, "https://fun.example", 11));
        memory.flush(&mut store, 100).await.unwrap();

        let owners: Vec<&str> = memory.traces().map(|t| t.owner.as_str()).collect();
        assert_eq!(owners.len(), 2);
        assert!(owners.contains(&"work") && owners.contains(&"home"));
    });
}

#[test]
fn quota_ages_out_the_oldest_stored_traces() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut memory = BrowsingMemory::new(1);
        for (i, url) in [
            "https://one.example",
            "https://two.example",
            "https://three.example",
        ]
        .iter()
        .enumerate()
        {
            memory.record_traversal("mark", event(None, url, 10 + i as u64));
            memory.flush(&mut store, 100 + i as u64).await.unwrap();
        }
        assert_eq!(
            list_typed::<BrowsingTrace>(&mut store).await.unwrap().len(),
            3
        );

        let deleted = memory.apply_quota(&mut store, 2).await.unwrap();
        assert_eq!(deleted, 1);

        // The store keeps only the two newest; the corpus agrees.
        let remaining = list_typed::<BrowsingTrace>(&mut store).await.unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(memory.traces().count(), 2);
        assert!(
            memory
                .traces()
                .all(|t| t.events[0].to.url != "https://one.example")
        );

        // Under quota: nothing to delete.
        assert_eq!(memory.apply_quota(&mut store, 5).await.unwrap(), 0);
    });
}

#[test]
fn forget_url_removes_every_trace_mentioning_it() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut memory = BrowsingMemory::new(1);
        // An origin visit to the page, a hop *from* it, and an unrelated trace.
        memory.record_traversal("mark", event(None, "https://gone.example", 10));
        memory.flush(&mut store, 100).await.unwrap();
        memory.record_traversal(
            "mark",
            event(Some("https://gone.example"), "https://next.example", 11),
        );
        memory.flush(&mut store, 101).await.unwrap();
        memory.record_traversal(
            "mark",
            event(Some("https://keep.example"), "https://stay.example", 12),
        );
        memory.flush(&mut store, 102).await.unwrap();
        assert_eq!(
            list_typed::<BrowsingTrace>(&mut store).await.unwrap().len(),
            3
        );

        // Forget the page: both the origin visit and the hop from it go; the
        // unrelated trace stays.
        let forgotten = memory
            .forget_url(&mut store, "https://gone.example")
            .await
            .unwrap();
        assert_eq!(forgotten, 2);

        let remaining = list_typed::<BrowsingTrace>(&mut store).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(memory.traces().all(|t| t.events.iter().all(|e| {
            e.to.url != "https://gone.example"
                && e.from
                    .as_ref()
                    .is_none_or(|f| f.url != "https://gone.example")
        })));

        // Idempotent: forgetting an already-absent url removes nothing.
        assert_eq!(
            memory
                .forget_url(&mut store, "https://gone.example")
                .await
                .unwrap(),
            0
        );
    });
}

#[test]
fn adopted_traces_join_reads_but_not_quota() {
    pollster::block_on(async {
        let mut store = InMemoryStore::default();
        let mut memory = BrowsingMemory::new(4);
        memory.adopt(BrowsingTrace::from_events(
            "imported",
            vec![event(None, "https://old.example", 5)],
        ));
        assert_eq!(memory.recent_corridor(5).len(), 1);
        // Nothing stored, so quota deletes nothing and the adopted trace stays.
        assert_eq!(memory.apply_quota(&mut store, 0).await.unwrap(), 0);
        assert_eq!(memory.traces().count(), 1);
    });
}
