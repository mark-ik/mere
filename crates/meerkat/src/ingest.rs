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
//! Dublin Core). [`harvest_contributions`] is the pure producer (the content
//! actor runs it off-thread, owning no graph, and ships the result); [`harvest`]
//! is the fused producer + applier for the host's own fetch drain, applying
//! through `Orrery::ingest_graph` so the spatial view reconciles around the new
//! nodes.
//!
//! Anything that is not linked data (markdown, plain HTML with no JSON-LD, …) is
//! a no-op here and renders through the normal card pipeline instead.

use kernel::graph::Graph;
use linked_data::{
    ContextCache, GraphContribution, NodeContribution, apply_contribution, from_html_with_contexts,
    from_jsonld_with_contexts,
};
use serval_static_dom::StaticDocument;

/// schema.org `description` — the page's own summary.
const SCHEMA_DESCRIPTION: &str = "https://schema.org/description";
/// schema.org `url` — the canonical URL the page claims.
const SCHEMA_URL: &str = "https://schema.org/url";

/// The bare media type, lowercased and without parameters (`; charset=…`).
fn media_type(content_type: Option<&str>) -> Option<String> {
    content_type.map(|ct| {
        ct.split(';')
            .next()
            .unwrap_or(ct)
            .trim()
            .to_ascii_lowercase()
    })
}

/// Produce the graph contributions a fetched document carries, without touching
/// any graph. The content actor runs this off-thread (it owns no graph) and ships
/// the result to the kernel, which applies them: the producer half of the
/// content-actor / kernel contract. Remote `@context`s resolve against the bundled
/// full pack; a non-linked-data document yields `[]`.
pub fn harvest_contributions(content_type: Option<&str>, body: &str) -> Vec<GraphContribution> {
    match media_type(content_type).as_deref() {
        Some("application/ld+json") => {
            from_jsonld_with_contexts(body.as_bytes(), ContextCache::full())
                .map(|contribution| vec![contribution])
                .unwrap_or_default()
        }
        // A page can carry several JSON-LD blocks.
        Some("text/html") => from_html_with_contexts(body, ContextCache::full()),
        _ => Vec::new(),
    }
}

/// Enrich the **page node** (`url`) with what a render-free static-parse extraction
/// finds the page declaring about itself — its `<title>`, description
/// (`<meta name=description>`), the canonical URL, and OpenGraph `og:*` fields —
/// as a single [`GraphContribution`] over the same pipe as the JSON-LD harvest. This
/// fills the gap that harvest leaves for the (vast) majority of pages carrying no
/// structured data: every visited HTML page can still contribute its own metadata.
///
/// Links are deliberately **not** contributed as edges here. The crawl frontier
/// (with its depth / fan-out / politeness caps) owns the link graph, so a single
/// visit enriches only the page node and cannot flood the graph with link targets.
/// `None` for a non-HTML body, or HTML that declares nothing extractable.
pub fn page_extract_contribution(
    url: &str,
    content_type: Option<&str>,
    body: &str,
) -> Option<GraphContribution> {
    if media_type(content_type).as_deref() != Some("text/html") {
        return None;
    }
    let doc = StaticDocument::parse(body);
    let extract = serval_extract::extract(&doc);

    let mut properties: Vec<(String, String)> = Vec::new();
    if let Some(description) = extract.metadata.description {
        properties.push((SCHEMA_DESCRIPTION.to_string(), description));
    }
    if let Some(canonical) = extract.metadata.canonical {
        properties.push((SCHEMA_URL.to_string(), canonical));
    }
    for (key, value) in extract.metadata.open_graph {
        properties.push((format!("https://ogp.me/ns#{key}"), value));
    }

    // Nothing declared → no enrichment (don't mint an empty contribution).
    if extract.title.is_none() && properties.is_empty() {
        return None;
    }
    Some(GraphContribution {
        nodes: vec![NodeContribution {
            id: url.to_string(),
            types: Vec::new(),
            title: extract.title,
            tags: Vec::new(),
            properties,
        }],
        edges: Vec::new(),
    })
}

/// Harvest any linked data in `body` into `graph` directly, returning whether the
/// graph changed. The fused producer + applier, for a caller that already holds
/// the graph (the host's fetch drain); the content actor instead uses the split
/// form, [`harvest_contributions`] plus a kernel-side apply.
pub fn harvest(graph: &mut Graph, content_type: Option<&str>, body: &str) -> bool {
    let mut changed = false;
    for contribution in harvest_contributions(content_type, body) {
        let outcome = apply_contribution(graph, &contribution);
        changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
    }
    changed
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
    fn harvest_contributions_produces_without_a_graph() {
        // The pure producer: contributions out, no graph touched.
        let body = r#"{"@context":{"name":"https://schema.org/name"},"@id":"mere://p","name":"P"}"#;
        let contributions = harvest_contributions(Some("application/ld+json"), body);
        assert_eq!(
            contributions.len(),
            1,
            "one JSON-LD document, one contribution"
        );
        assert!(contributions[0].nodes.iter().any(|n| n.id == "mere://p"));
        assert!(
            harvest_contributions(Some("text/markdown"), "# hi").is_empty(),
            "non-linked-data yields no contributions",
        );
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
        assert!(harvest(
            &mut graph,
            Some("application/ld+json; charset=utf-8"),
            body
        ));
        assert!(graph.get_node_by_url("https://x.test/").is_some());
    }

    #[test]
    fn non_linked_data_is_a_no_op() {
        let mut graph = Graph::new();
        assert!(!harvest(&mut graph, Some("text/markdown"), "# hello"));
        assert!(!harvest(&mut graph, None, "{}"));
        assert_eq!(graph.nodes().count(), 0);
    }

    #[test]
    fn page_extract_enriches_the_page_node_with_its_metadata() {
        // A plain HTML page with no JSON-LD still contributes its declared metadata.
        let body = "<html><head>\
            <title>The Page</title>\
            <meta name=\"description\" content=\"What it is about.\">\
            <link rel=\"canonical\" href=\"https://x.test/canon\">\
            <meta property=\"og:image\" content=\"https://x.test/og.png\">\
            </head><body><a href=\"/other\">link</a></body></html>";
        let c = page_extract_contribution("https://x.test/page", Some("text/html"), body)
            .expect("HTML with metadata contributes");
        assert_eq!(c.nodes.len(), 1, "one node — the page itself");
        let node = &c.nodes[0];
        assert_eq!(node.id, "https://x.test/page");
        assert_eq!(node.title.as_deref(), Some("The Page"));
        assert!(node.properties.contains(&(SCHEMA_DESCRIPTION.to_string(), "What it is about.".to_string())));
        assert!(node.properties.contains(&(SCHEMA_URL.to_string(), "https://x.test/canon".to_string())));
        assert!(node
            .properties
            .contains(&("https://ogp.me/ns#image".to_string(), "https://x.test/og.png".to_string())));
        // Links are NOT contributed as edges here (the crawl frontier owns the link graph).
        assert!(c.edges.is_empty(), "no link edges from a single visit");
    }

    #[test]
    fn page_extract_is_none_without_metadata_or_for_non_html() {
        // HTML that declares nothing extractable → no contribution.
        assert!(
            page_extract_contribution("https://x.test/", Some("text/html"), "<body><p>hi</p></body>")
                .is_none(),
            "a page with no title/description/og contributes nothing",
        );
        // Non-HTML → no contribution (JSON-LD has its own harvest path).
        assert!(
            page_extract_contribution("https://x.test/", Some("application/ld+json"), "{}").is_none(),
            "non-HTML is not extracted here",
        );
    }
}
