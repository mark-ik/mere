/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The lineage bridge — live navigation memory projected into durable traces.
//!
//! `stemma` owns the live, graph-shaped visit authority (its standing
//! rule: visits own the tree, views are projections). This module is one
//! more such projection, pointed outward: a [`GraphMemorySnapshot`] becomes
//! per-owner [`BrowsingTrace`]s, so live browsing turns into durable memory
//! **by reading, never by keeping a second visited-set** (the derivation
//! plan's bridge rule).
//!
//! The snapshot's entry payload and owner identity are generic (the kernel
//! owns those types), so the caller supplies two small extractors: which
//! entries are pages, and how an owner is tagged. Entries that are not pages
//! project to nothing.

use chartulary::stemma::{StemmaSnapshot as GraphMemorySnapshot, TransitionKind};

use super::{BrowsingTrace, PageRef, TraceEvent, TraceTransition};

impl From<TransitionKind> for TraceTransition {
    fn from(kind: TransitionKind) -> Self {
        match kind {
            TransitionKind::LinkClick => TraceTransition::LinkClick,
            TransitionKind::UrlTyped => TraceTransition::UrlTyped,
            TransitionKind::Back => TraceTransition::Back,
            TransitionKind::Forward => TraceTransition::Forward,
            TransitionKind::Reload => TraceTransition::Reload,
            TransitionKind::Redirect => TraceTransition::Redirect,
            TransitionKind::TabSpawn => TraceTransition::TabSpawn,
            TransitionKind::Restore => TraceTransition::Restore,
            TransitionKind::Imported => TraceTransition::Imported,
            TransitionKind::Unknown => TraceTransition::Unknown,
        }
    }
}

/// Project a lineage snapshot into one [`BrowsingTrace`] per owner.
///
/// - `page_of` maps an entry payload to its [`PageRef`]; `None` skips the
///   entry (not every graph node is a page).
/// - `owner_tag` names an owner (persona) for the trace's `owner` field.
///
/// Within an owner's trace, events are that owner's visits in chronological
/// order (`created_at_ms`, index as tiebreak); `from` is the parent visit's
/// page; the transition comes from the visit's inbound record. A visit bound
/// to several owners appears in each of their traces — that is the per-owner
/// view, not duplication. Owners with no page visits produce no trace.
pub fn project_lineage<K, E, O, X>(
    snapshot: &GraphMemorySnapshot<K, E, O, X>,
    mut page_of: impl FnMut(&E) -> Option<PageRef>,
    mut owner_tag: impl FnMut(&O) -> String,
) -> Vec<BrowsingTrace>
where
    K: chartulary::stemma::EntryIdentityKey,
    E: chartulary::stemma::MemoryPayload,
    O: chartulary::stemma::OwnerIdentity,
    X: chartulary::stemma::MemoryPayload,
{
    let pages: Vec<Option<PageRef>> = snapshot
        .entries
        .iter()
        .map(|e| page_of(&e.payload))
        .collect();

    let mut traces = Vec::new();
    for owner in &snapshot.owners {
        let mut visit_indices: Vec<usize> = owner
            .owned_visits
            .iter()
            .copied()
            .filter(|&i| i < snapshot.visits.len())
            .collect();
        visit_indices.sort_by_key(|&i| (snapshot.visits[i].created_at_ms, i));

        let mut events = Vec::new();
        for i in visit_indices {
            let visit = &snapshot.visits[i];
            let Some(Some(to)) = pages.get(visit.entry) else {
                continue;
            };
            let from = visit
                .parent
                .and_then(|p| snapshot.visits.get(p))
                .and_then(|parent| pages.get(parent.entry))
                .and_then(|page| page.clone());
            let transition = visit
                .inbound
                .map(|t| TraceTransition::from(t.kind))
                .unwrap_or(TraceTransition::Unknown);
            events.push(TraceEvent {
                from,
                to: to.clone(),
                transition,
                at_ms: visit.created_at_ms,
                dwell_ms: None,
                candidates: Vec::new(),
            });
        }
        if !events.is_empty() {
            traces.push(BrowsingTrace::from_events(
                owner_tag(&owner.identity),
                events,
            ));
        }
    }
    traces
}

#[cfg(test)]
mod tests {
    use super::*;
    use chartulary::stemma::{
        BindingSnapshot, EntryPrivacy, EntrySnapshot, OwnerSnapshot, TransitionRecord,
        VisitSnapshot,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct TestPage {
        url: String,
        title: String,
    }

    fn entry(url: &str) -> EntrySnapshot<String, TestPage> {
        EntrySnapshot {
            key: url.to_string(),
            payload: TestPage {
                url: url.to_string(),
                title: format!("title of {url}"),
            },
            first_seen_at_ms: 0,
            last_seen_at_ms: 0,
            visit_count: 1,
            privacy: EntryPrivacy::LocalOnly,
        }
    }

    fn visit(
        entry: usize,
        parent: Option<usize>,
        at_ms: u64,
        kind: Option<TransitionKind>,
        owner: usize,
    ) -> VisitSnapshot<()> {
        VisitSnapshot {
            entry,
            parent,
            children: Vec::new(),
            created_at_ms: at_ms,
            context: (),
            inbound: kind.map(|kind| TransitionRecord { kind, at_ms }),
            bindings: vec![BindingSnapshot {
                owner,
                forward_child: None,
                last_accessed_at_ms: at_ms,
            }],
        }
    }

    fn owner(identity: &str, owned: Vec<usize>) -> OwnerSnapshot<String> {
        OwnerSnapshot {
            identity: identity.to_string(),
            origin: owned.first().copied(),
            current: owned.last().copied(),
            creator: None,
            pending_origin_parent: None,
            owned_visits: owned,
        }
    }

    #[test]
    fn a_snapshot_projects_into_per_owner_chronological_traces() {
        let snapshot: GraphMemorySnapshot<String, TestPage, String, ()> = GraphMemorySnapshot {
            entries: vec![
                entry("https://a.example"),
                entry("https://b.example"),
                entry("https://c.example"),
            ],
            visits: vec![
                visit(0, None, 10, Some(TransitionKind::UrlTyped), 0),
                visit(1, Some(0), 20, Some(TransitionKind::LinkClick), 0),
                visit(2, None, 15, Some(TransitionKind::TabSpawn), 1),
            ],
            owners: vec![
                owner("work", vec![1, 0]), // deliberately out of order
                owner("home", vec![2]),
            ],
        };

        let traces = project_lineage(
            &snapshot,
            |p: &TestPage| {
                Some(PageRef {
                    url: p.url.clone(),
                    title: Some(p.title.clone()),
                })
            },
            |o: &String| o.clone(),
        );

        assert_eq!(traces.len(), 2);
        let work = traces.iter().find(|t| t.owner == "work").unwrap();
        assert_eq!(work.events.len(), 2);
        // Sorted chronologically despite the out-of-order owned set.
        assert_eq!(work.events[0].to.url, "https://a.example");
        assert_eq!(work.events[0].transition, TraceTransition::UrlTyped);
        assert!(work.events[0].from.is_none());
        assert_eq!(work.events[1].to.url, "https://b.example");
        assert_eq!(work.events[1].transition, TraceTransition::LinkClick);
        assert_eq!(
            work.events[1].from.as_ref().map(|p| p.url.as_str()),
            Some("https://a.example")
        );
        assert_eq!(work.started_at_ms, 10);
        assert_eq!(work.ended_at_ms, 20);

        let home = traces.iter().find(|t| t.owner == "home").unwrap();
        assert_eq!(home.events.len(), 1);
        assert_eq!(home.events[0].transition, TraceTransition::TabSpawn);
    }

    #[test]
    fn non_page_entries_and_empty_owners_project_to_nothing() {
        let snapshot: GraphMemorySnapshot<String, TestPage, String, ()> = GraphMemorySnapshot {
            entries: vec![entry("https://a.example"), entry("internal:scratch")],
            visits: vec![
                visit(1, None, 10, Some(TransitionKind::Unknown), 0),
                visit(0, Some(0), 20, Some(TransitionKind::LinkClick), 0),
            ],
            owners: vec![owner("work", vec![0, 1]), owner("idle", Vec::new())],
        };

        let traces = project_lineage(
            &snapshot,
            |p: &TestPage| {
                // Only http(s) entries are pages.
                p.url.starts_with("https://").then(|| PageRef {
                    url: p.url.clone(),
                    title: None,
                })
            },
            |o: &String| o.clone(),
        );

        assert_eq!(traces.len(), 1, "the empty owner produced no trace");
        let work = &traces[0];
        // The non-page visit is skipped entirely; the page visit keeps its
        // parent link even though the parent was filtered (from = None
        // because the parent's entry is not a page).
        assert_eq!(work.events.len(), 1);
        assert_eq!(work.events[0].to.url, "https://a.example");
        assert!(work.events[0].from.is_none());
    }
}
