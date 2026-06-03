/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Harvesting linked data from a completed fetch into the session graph.
//!
//! When meerkat's netfetcher delivers a document whose content-type is JSON-LD
//! (`application/ld+json`), or HTML carrying embedded `<script type="…ld+json">`
//! blocks, its `@id`s / predicates / curated literals merge into the graph
//! through the host-neutral [`linked_data`] bridge — remote `@context`s resolved
//! offline against the bundled **full** pack (schema.org / ActivityStreams /
//! Dublin Core). This module is the content-type classifier + merge; the host
//! drives the actual graph mutation through `Orrery::ingest_graph`, so the
//! spatial view reconciles around the new nodes.
//!
//! Anything that is not linked data (markdown, plain HTML with no JSON-LD, …) is
//! a no-op here and renders through the normal card pipeline instead.

use kernel::graph::Graph;
use linked_data::{
    apply_contribution, from_html_with_contexts, from_jsonld_with_contexts, ContextCache,
};

/// The bare media type, lowercased and without parameters (`; charset=…`).
fn media_type(content_type: Option<&str>) -> Option<String> {
    content_type.map(|ct| ct.split(';').next().unwrap_or(ct).trim().to_ascii_lowercase())
}

/// Harvest any linked data in `body` (a fetched document of type `content_type`)
/// into `graph`, resolving remote `@context`s against the bundled full pack.
/// Returns whether the graph changed; a non-linked-data document is a no-op.
pub fn harvest(graph: &mut Graph, content_type: Option<&str>, body: &str) -> bool {
    match media_type(content_type).as_deref() {
        Some("application/ld+json") => {
            match from_jsonld_with_contexts(body.as_bytes(), ContextCache::full()) {
                Ok(contribution) => merged(graph, &contribution),
                Err(_) => false,
            }
        },
        Some("text/html") => {
            // A page can carry several JSON-LD blocks; merge each.
            let mut changed = false;
            for contribution in from_html_with_contexts(body, ContextCache::full()) {
                changed |= merged(graph, &contribution);
            }
            changed
        },
        _ => false,
    }
}

/// Apply one contribution and report whether it added anything.
fn merged(graph: &mut Graph, contribution: &linked_data::GraphContribution) -> bool {
    let outcome = apply_contribution(graph, contribution);
    outcome.nodes_created > 0 || outcome.edges_asserted > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harvests_jsonld_graph_into_the_graph() {
        let mut graph = Graph::new();
        let body = r#"{"@context":{"name":"https://schema.org/name","cites":"https://mere.computer/ns/rel#cites"},
            "@graph":[
              {"@id":"mere://a","name":"A","cites":{"@id":"mere://b"}},
              {"@id":"mere://b","name":"B"}
            ]}"#;
        assert!(harvest(&mut graph, Some("application/ld+json"), body));
        assert!(graph.get_node_by_url("mere://a").is_some());
        assert!(graph.get_node_by_url("mere://b").is_some());
    }

    #[test]
    fn harvests_embedded_jsonld_from_html() {
        let mut graph = Graph::new();
        let body = r#"<!doctype html><html><head>
            <script type="application/ld+json">
            {"@context":{"name":"https://schema.org/name"},"@id":"mere://h","name":"H"}
            </script></head><body></body></html>"#;
        assert!(harvest(&mut graph, Some("text/html"), body));
        assert!(graph.get_node_by_url("mere://h").is_some());
    }

    #[test]
    fn resolves_a_remote_schema_org_context_offline() {
        let mut graph = Graph::new();
        let body = r#"{"@context":"https://schema.org/","@id":"https://x.test/","name":"X"}"#;
        assert!(harvest(&mut graph, Some("application/ld+json; charset=utf-8"), body));
        assert!(graph.get_node_by_url("https://x.test/").is_some());
    }

    #[test]
    fn non_linked_data_is_a_no_op() {
        let mut graph = Graph::new();
        assert!(!harvest(&mut graph, Some("text/markdown"), "# hello"));
        assert!(!harvest(&mut graph, None, "{}"));
        assert_eq!(graph.nodes().count(), 0);
    }
}
