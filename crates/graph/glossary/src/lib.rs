/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # glossary
//!
//! Graph digest projections — pure `Graph -> human-facing view` for the Mere
//! browser. The textual / statistical sibling of `cartography` (spatial
//! projection) and `linked-data` (RDF / JSON-LD interchange): it turns the
//! kernel's graph into consumer-facing summaries the gloss Navigator, apparatus,
//! and export render. Host-free, DOM-free, `&Graph`-immutable.
//!
//! - [`outline_djot`] — a hierarchical djot outline nested by parsed URL structure.
//! - [`graph_metrics`] — cheap kernel-sourced summary stats ([`GraphMetrics`]).
//!
//! Renamed from `mere-orrery` 2026-06-23; its a11y `project_graph` moved host-side
//! into meerkat's `orrery_a11y_tree` (unified-document-host slice 4) and was retired.

#![doc(html_root_url = "https://docs.rs/glossary/0.0.1")]

use std::collections::BTreeMap;

use kernel::graph::{EdgeFamily, Graph};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

// ───────────────────────────── metrics ─────────────────────────────

/// Cheap, kernel-sourced summary statistics over a graph, for the gloss readout
/// and apparatus diagnostics. Expensive signals (centrality, community) are
/// deliberately **not** here: they come from `intel/signals` and the outline lens
/// consumes them separately, never reproduced. (gloss-outline plan P0.)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphMetrics {
    /// Number of nodes.
    pub node_count: usize,
    /// Number of node-pair edges (a multi-relation edge between one pair counts once).
    pub edge_count: usize,
    /// Number of relation rows (each typed relation on a multi-relation edge counts).
    pub relation_count: usize,
    /// Relation rows bucketed by edge family.
    pub relations_by_family: BTreeMap<EdgeFamily, usize>,
    /// Nodes with no incoming or outgoing edge.
    pub orphan_count: usize,
    /// Number of weakly-connected components.
    pub component_count: usize,
    /// Size of the largest weakly-connected component (0 for an empty graph).
    pub largest_component: usize,
}

/// Project cheap summary metrics from a graph. Consumes the kernel's own queries
/// (`node_count` / `edge_count` / `relations` / `orphan_node_keys` /
/// `weakly_connected_components`); never recomputes what the kernel stores.
/// (gloss-outline plan P0.)
pub fn graph_metrics(graph: &Graph) -> GraphMetrics {
    let mut relations_by_family: BTreeMap<EdgeFamily, usize> = BTreeMap::new();
    let mut relation_count = 0usize;
    for rel in graph.relations() {
        *relations_by_family.entry(rel.kind.family()).or_insert(0) += 1;
        relation_count += 1;
    }
    let components = graph.weakly_connected_components();
    GraphMetrics {
        node_count: graph.node_count(),
        edge_count: graph.edge_count(),
        relation_count,
        relations_by_family,
        orphan_count: graph.orphan_node_keys().len(),
        component_count: components.len(),
        largest_component: components.iter().map(Vec::len).max().unwrap_or(0),
    }
}

// ───────────────────────────── outline ─────────────────────────────

/// One node of the URL-structure outline trie: the graph node (if any) whose
/// address is exactly this path, plus child path segments (a `BTreeMap` so the
/// emitted order is deterministic).
#[derive(Default)]
struct OutlineTrie {
    /// `(label, url)` of the graph node living exactly at this path.
    here: Option<(String, String)>,
    children: BTreeMap<String, OutlineTrie>,
}

/// Project a graph into a hierarchical djot outline, nested by the URL structure of
/// each node's address: the host is the top level and each path segment a nested
/// level (`example.com` -> `docs` -> `guide`). A node lands at its full-path leaf; a
/// path segment with no node of its own renders as a plain structural bullet.
/// Addresses that do not parse as `scheme://host[/path]` are listed flat at the end.
///
/// Nested by URL structure rather than containment edges by design: `UrlPath`
/// containment is not auto-populated on the live path, so reading it would be flat;
/// the projection derives the host/path tree from the addresses themselves. Pure,
/// deterministic, no engine dependency (djot is plain text). (gloss-outline plan P0.)
pub fn outline_djot(graph: &Graph) -> String {
    let mut root = OutlineTrie::default();
    let mut loose: Vec<(String, String)> = Vec::new();

    for (_key, node) in graph.nodes() {
        let url = node.url();
        let label = node_label(&node.title, url);
        match parse_host_path(url) {
            Some((host, segments)) => {
                let mut cur = root.children.entry(host).or_default();
                for seg in segments {
                    cur = cur.children.entry(seg).or_default();
                }
                cur.here = Some((label, url.to_string()));
            }
            None => loose.push((label, url.to_string())),
        }
    }

    let mut out = String::new();
    for (host, child) in &root.children {
        emit_trie(&mut out, host, child, 0);
    }
    loose.sort();
    for (label, url) in &loose {
        push_bullet(&mut out, 0, &format!("[{}]({url})", escape(label)));
    }
    out
}

/// Emit one trie node (a real node as a `[label](url)` link, a structural segment
/// as plain text) then recurse its children.
fn emit_trie(out: &mut String, segment: &str, node: &OutlineTrie, depth: usize) {
    match &node.here {
        Some((label, url)) => push_bullet(out, depth, &format!("[{}]({url})", escape(label))),
        None => push_bullet(out, depth, segment),
    }
    for (seg, child) in &node.children {
        emit_trie(out, seg, child, depth + 1);
    }
}

fn push_bullet(out: &mut String, depth: usize, body: &str) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str("- ");
    out.push_str(body);
    out.push('\n');
}

/// A node's outline label: its title, or the URL's last path segment (the readable
/// slug) when the title is empty or still URL-shaped (seeded to the URL before a load).
fn node_label(title: &str, url: &str) -> String {
    let title = title.trim();
    if !title.is_empty() && title != url {
        return title.to_string();
    }
    url.trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())
        .filter(|s| !s.contains(':'))
        .unwrap_or(url)
        .to_string()
}

/// Parse `scheme://host[/seg/seg...]` into `(host, segments)`, dropping any query or
/// fragment and empty segments. `None` when there is no `scheme://host` (a non-URL
/// address), which routes the node to the flat loose list.
fn parse_host_path(url: &str) -> Option<(String, Vec<String>)> {
    let after_scheme = url.split_once("://")?.1;
    let path = after_scheme.split(['?', '#']).next().unwrap_or(after_scheme);
    let mut parts = path.split('/');
    let host = parts.next().filter(|h| !h.is_empty())?.to_string();
    let segments = parts.filter(|s| !s.is_empty()).map(str::to_string).collect();
    Some((host, segments))
}

/// Minimal djot link-text escaping: a literal `]` would close the link text early.
fn escape(s: &str) -> String {
    s.replace(']', "\\]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Point2D;
    use kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};

    fn p() -> Point2D<f32> {
        Point2D::new(0.0, 0.0)
    }

    #[test]
    fn parse_host_path_splits_scheme_host_path() {
        assert_eq!(
            parse_host_path("https://en.wikipedia.org/wiki/Foo"),
            Some(("en.wikipedia.org".into(), vec!["wiki".into(), "Foo".into()]))
        );
        assert_eq!(
            parse_host_path("mere://welcome"),
            Some(("welcome".into(), vec![]))
        );
        assert_eq!(
            parse_host_path("https://x.test/a?q=1#frag"),
            Some(("x.test".into(), vec!["a".into()]))
        );
        assert_eq!(parse_host_path("not a url"), None);
    }

    #[test]
    fn node_label_prefers_title_else_slug() {
        assert_eq!(node_label("Real Title", "https://x.test/a"), "Real Title");
        assert_eq!(node_label("", "https://x.test/wiki/Foo"), "Foo");
        // title still seeded to the url -> fall back to the slug
        assert_eq!(
            node_label("https://x.test/wiki/Foo", "https://x.test/wiki/Foo"),
            "Foo"
        );
    }

    #[test]
    fn outline_nests_by_host_and_path() {
        let mut g = Graph::new();
        let a = g.add_node("https://site.test/docs/guide".into(), p());
        g.set_node_title(a, "Guide".into());
        let b = g.add_node("https://site.test/docs/intro".into(), p());
        g.set_node_title(b, "Intro".into());
        let c = g.add_node("https://other.test/".into(), p());
        g.set_node_title(c, "Other Home".into());
        let out = outline_djot(&g);
        assert!(out.contains("- site.test\n"), "host header; got:\n{out}");
        assert!(out.contains("  - docs\n"), "structural segment; got:\n{out}");
        assert!(
            out.contains("    - [Guide](https://site.test/docs/guide)"),
            "leaf link; got:\n{out}"
        );
        assert!(
            out.contains("    - [Intro](https://site.test/docs/intro)"),
            "leaf link; got:\n{out}"
        );
        assert!(
            out.contains("- [Other Home](https://other.test/)"),
            "host-root node as a link; got:\n{out}"
        );
    }

    #[test]
    fn outline_lists_non_url_nodes_flat() {
        let mut g = Graph::new();
        let n = g.add_node("just-a-name".into(), p());
        g.set_node_title(n, "Loose".into());
        assert!(outline_djot(&g).contains("- [Loose](just-a-name)"));
    }

    #[test]
    fn outline_empty_graph_is_empty() {
        assert_eq!(outline_djot(&Graph::new()), "");
    }

    #[test]
    fn metrics_count_nodes_edges_components() {
        let mut g = Graph::new();
        let a = g.add_node("https://a.test/".into(), p());
        let b = g.add_node("https://b.test/".into(), p());
        let _orphan = g.add_node("https://c.test/".into(), p());
        g.assert_relation(
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::UserGrouped,
                label: None,
                decay_progress: None,
            },
        );
        let m = graph_metrics(&g);
        assert_eq!(m.node_count, 3);
        assert_eq!(m.edge_count, 1);
        assert_eq!(m.relation_count, 1);
        assert_eq!(m.relations_by_family.get(&EdgeFamily::Semantic), Some(&1));
        assert_eq!(m.orphan_count, 1);
        assert_eq!(m.component_count, 2);
        assert_eq!(m.largest_component, 2);
    }

    #[test]
    fn metrics_empty_graph_is_zero() {
        assert_eq!(graph_metrics(&Graph::new()), GraphMetrics::default());
    }
}
