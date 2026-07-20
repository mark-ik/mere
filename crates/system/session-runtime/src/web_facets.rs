// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `web.*` facet namespace — per-node browser-runtime state as facets.
//!
//! What the *browser* knows about a node — restore fidelity (scroll offset,
//! form draft) and viewing preference (viewer override, compat mode, live
//! content on) — persisted as atomic `web.*` facets in
//! [`facets.json`](crate::facet_store), keyed by the node's stable UUID. The
//! graph stays correct if every facet is deleted (a fresh visit re-derives or
//! defaults each field); a browser engine id must never become a graph-library
//! fact.
//!
//! This is the `web.*` convergence rung: the bespoke
//! [`browser_node_state`](crate::browser_node_state) sidecar
//! (`browser_nodes.json`) stops being written; the host reads it once as
//! legacy fallback (absorbing it into facets) and saves facets only. The
//! in-memory types ([`BrowserNodeState`] / [`BrowserNodeStates`]) stay the
//! host's working set — this module is the persistence boundary between that
//! map and the facet store.
//!
//! Each field is its own facet (independently addressable — a modder can read
//! `web.viewer` alone), written only when non-default so the store carries no
//! noise:
//!
//! | Facet | Payload | Meaning |
//! | --- | --- | --- |
//! | `web.scroll` | `{"x": f32, "y": f32}` | last scroll offset |
//! | `web.form_draft` | string | best-effort form draft |
//! | `web.viewer` | string | viewer override (absent = automatic) |
//! | `web.compat` | `true` | platform-WebView compat mode |
//! | `web.content` | `true` | live content was ON at save |

use serde_json::{Value, json};
use uuid::Uuid;

use crate::browser_node_state::{BrowserNodeState, BrowserNodeStates};
use crate::facet_store::{AcceptAll, FacetId, NodeFacetStore};

/// Facet id: last known scroll offset. Payload `{"x": f32, "y": f32}`.
pub const WEB_SCROLL: &str = "web.scroll";
/// Facet id: best-effort form draft. Payload: a string.
pub const WEB_FORM_DRAFT: &str = "web.form_draft";
/// Facet id: user-set viewer override. Payload: a string (absent = automatic).
pub const WEB_VIEWER: &str = "web.viewer";
/// Facet id: per-node compat mode (platform WebView). Payload: `true` (absent = off).
pub const WEB_COMPAT: &str = "web.compat";
/// Facet id: live content was ON at save (rung-6 respawn). Payload: `true`.
pub const WEB_CONTENT: &str = "web.content";

/// The five `web.*` facet ids, the set this module owns (used by the rewrite
/// to clear stale fields).
const WEB_FACETS: [&str; 5] = [WEB_SCROLL, WEB_FORM_DRAFT, WEB_VIEWER, WEB_COMPAT, WEB_CONTENT];

/// Replace the store's `web.*` facets with `states` — the save-time write.
/// Every existing `web.*` facet is cleared first (a node absent from `states`,
/// or a field gone default, carries none afterwards); other namespaces are
/// untouched. Default-valued fields are not written, mirroring the old
/// sidecar's empty-entry pruning.
pub fn write_web_states(store: &mut NodeFacetStore, states: &BrowserNodeStates) {
    let facets: Vec<FacetId> = WEB_FACETS.iter().map(|id| FacetId::new(*id)).collect();
    let stale: Vec<Uuid> = store
        .iter()
        .filter(|(_, node_facets)| facets.iter().any(|f| node_facets.has(f)))
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        for facet in &facets {
            store.remove(&id, facet);
        }
    }
    for (id, state) in &states.nodes {
        write_web_state(store, *id, state);
    }
}

/// Write one node's `web.*` facets (non-default fields only). Does not clear
/// other nodes; [`write_web_states`] is the whole-set rewrite.
pub fn write_web_state(store: &mut NodeFacetStore, node: Uuid, state: &BrowserNodeState) {
    let mut set = |id: &str, payload: Value| {
        let _ = store.set(node, FacetId::new(id), payload, &AcceptAll);
    };
    if let Some((x, y)) = state.scroll {
        set(WEB_SCROLL, json!({ "x": x, "y": y }));
    }
    if let Some(draft) = &state.form_draft {
        set(WEB_FORM_DRAFT, json!(draft));
    }
    if let Some(viewer) = &state.viewer_override {
        set(WEB_VIEWER, json!(viewer));
    }
    if state.compat_mode {
        set(WEB_COMPAT, json!(true));
    }
    if state.content_on {
        set(WEB_CONTENT, json!(true));
    }
}

/// Read every node's `web.*` facets back into the host's working map — the
/// load-time read. A malformed field reads as its default (one bad facet must
/// not cost the node its other state); a node with no well-formed `web.*`
/// facet is absent from the map.
pub fn read_web_states(store: &NodeFacetStore) -> BrowserNodeStates {
    let mut states = BrowserNodeStates::new();
    let scroll = FacetId::new(WEB_SCROLL);
    let draft = FacetId::new(WEB_FORM_DRAFT);
    let viewer = FacetId::new(WEB_VIEWER);
    let compat = FacetId::new(WEB_COMPAT);
    let content = FacetId::new(WEB_CONTENT);
    for (id, facets) in store.iter() {
        let state = BrowserNodeState {
            scroll: facets.get(&scroll).and_then(|v| {
                let x = v.get("x")?.as_f64()? as f32;
                let y = v.get("y")?.as_f64()? as f32;
                (x.is_finite() && y.is_finite()).then_some((x, y))
            }),
            form_draft: facets
                .get(&draft)
                .and_then(Value::as_str)
                .map(str::to_string),
            viewer_override: facets
                .get(&viewer)
                .and_then(Value::as_str)
                .map(str::to_string),
            compat_mode: facets
                .get(&compat)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            content_on: facets
                .get(&content)
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        if !state.is_empty() {
            states.nodes.insert(*id, state);
        }
    }
    states
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BrowserNodeStates {
        let mut states = BrowserNodeStates::new();
        let a = states.entry(Uuid::from_u128(0xa));
        a.scroll = Some((0.0, 420.5));
        a.viewer_override = Some("reader".to_string());
        a.content_on = true;
        let b = states.entry(Uuid::from_u128(0xb));
        b.compat_mode = true;
        b.form_draft = Some("dear sir".to_string());
        states
    }

    #[test]
    fn write_then_read_round_trips() {
        let mut store = NodeFacetStore::new();
        let original = sample();
        write_web_states(&mut store, &original);
        assert_eq!(read_web_states(&store), original);
    }

    #[test]
    fn default_fields_write_no_facet() {
        let mut store = NodeFacetStore::new();
        write_web_states(&mut store, &sample());
        let a = Uuid::from_u128(0xa);
        assert!(store.get(&a, &FacetId::new(WEB_COMPAT)).is_none());
        assert!(store.get(&a, &FacetId::new(WEB_FORM_DRAFT)).is_none());
        assert!(store.get(&a, &FacetId::new(WEB_CONTENT)).is_some());
    }

    #[test]
    fn a_rewrite_clears_stale_web_state_but_not_other_namespaces() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(0xa);
        write_web_states(&mut store, &sample());
        store
            .set(a, FacetId::new("arrangement.pin"), json!(true), &AcceptAll)
            .unwrap();
        // The next save has no state at all (all nodes went default).
        write_web_states(&mut store, &BrowserNodeStates::new());
        assert!(read_web_states(&store).nodes.is_empty());
        assert_eq!(
            store.get(&a, &FacetId::new("arrangement.pin")),
            Some(&json!(true)),
            "the web rewrite must not disturb other namespaces"
        );
    }

    #[test]
    fn a_malformed_field_defaults_without_losing_the_rest() {
        let mut store = NodeFacetStore::new();
        let a = Uuid::from_u128(0xa);
        store
            .set(a, FacetId::new(WEB_SCROLL), json!("nope"), &AcceptAll)
            .unwrap();
        store
            .set(a, FacetId::new(WEB_CONTENT), json!(true), &AcceptAll)
            .unwrap();
        let states = read_web_states(&store);
        let state = states.get(a).expect("the node still reads");
        assert!(state.scroll.is_none(), "garbage scroll defaults");
        assert!(state.content_on, "the good field survives");
    }
}
