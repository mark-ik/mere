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
    ContextCache, EdgeContribution, GraphContribution, NodeContribution, apply_contribution,
    from_html_with_contexts, from_jsonld_with_contexts,
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
    let extract = serval_extract::extract(&StaticDocument::parse(body));
    contribution_from_page_extract(url, extract)
}

/// Map an already-computed [`PageExtract`](serval_extract::PageExtract) to a page-node
/// [`GraphContribution`] (title / description / canonical / OpenGraph). The shared
/// mapping behind both the static path ([`page_extract_contribution`]) and the
/// **headless-scripted** path (the host runs the scripted rung, then feeds
/// `ScriptedDocument::extract()`'s post-JS extract here so an SPA's JS-rendered
/// metadata contributes too). `None` when the page declares nothing extractable.
pub fn contribution_from_page_extract(
    url: &str,
    extract: serval_extract::PageExtract,
) -> Option<GraphContribution> {
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

/// Materialize the seed page's **outbound-link neighborhood** as a graph
/// contribution: every `<a href>` becomes a target node (`id` = resolved URL,
/// `title` = anchor text), joined to the seed by a `Semantic:Hyperlink` edge. The
/// single-hop link-graph materializer (relational-browse V1) — a render-free parse
/// of an already-fetched body, with **no target fetch and no new actor**. `None`
/// when the page has no outbound links.
pub fn harvest_links(seed_url: &str, body: &str) -> Option<GraphContribution> {
    let links = serval_extract::extract_links(&StaticDocument::parse(body));
    links_contribution(seed_url, links)
}

/// The shared seed-links → contribution mapping behind both the static path
/// ([`harvest_links`]) and the post-JS path (feed a `ScriptedDocument::extract()`'s
/// links here for an SPA's outbound graph). Targets dedup by resolved URL (first
/// non-empty anchor text wins the title); non-navigable hrefs (empty, `#`-fragment,
/// `javascript:` / `mailto:` / `tel:` / `data:` / `blob:` / `about:`) are skipped.
/// The seed node is included so the edges are self-contained. `None` when no outbound
/// target remains.
pub fn links_contribution(
    seed_url: &str,
    links: Vec<serval_extract::Link>,
) -> Option<GraphContribution> {
    let predicate = kernel::graph::predicate_iri(kernel::graph::SemanticSubKind::Hyperlink);
    // Dedup by resolved URL, keeping the first non-empty anchor text. BTreeMap for a
    // deterministic node/edge order (tests, and a stable graph apply).
    let mut targets: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();
    for link in links {
        let href = link.href.trim();
        if href.is_empty() || href.starts_with('#') || is_non_navigable(href) {
            continue;
        }
        let resolved = crate::nav::resolve_href(seed_url, href);
        if resolved == seed_url {
            continue; // a self-link is not an outbound edge
        }
        let title = {
            let t = link.text.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        targets.entry(resolved).or_insert(title);
    }
    if targets.is_empty() {
        return None;
    }
    let mut nodes = Vec::with_capacity(targets.len() + 1);
    let mut edges = Vec::with_capacity(targets.len());
    // The seed node first, so the edges' subject resolves in a self-contained apply.
    nodes.push(NodeContribution {
        id: seed_url.to_string(),
        types: Vec::new(),
        title: None,
        tags: Vec::new(),
        properties: Vec::new(),
    });
    for (url, title) in targets {
        edges.push(EdgeContribution {
            subject: seed_url.to_string(),
            predicate: predicate.to_string(),
            object: url.clone(),
        });
        nodes.push(NodeContribution {
            id: url,
            types: Vec::new(),
            title,
            tags: Vec::new(),
            properties: Vec::new(),
        });
    }
    Some(GraphContribution { nodes, edges })
}

/// URL schemes that are not navigable crawl targets (action / contact / data URIs).
fn is_non_navigable(href: &str) -> bool {
    let lower = href.to_ascii_lowercase();
    ["javascript:", "mailto:", "tel:", "data:", "blob:", "about:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
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
        assert!(node.properties.contains(&(
            SCHEMA_DESCRIPTION.to_string(),
            "What it is about.".to_string()
        )));
        assert!(
            node.properties
                .contains(&(SCHEMA_URL.to_string(), "https://x.test/canon".to_string()))
        );
        assert!(node.properties.contains(&(
            "https://ogp.me/ns#image".to_string(),
            "https://x.test/og.png".to_string()
        )));
        // Links are NOT contributed as edges here (the crawl frontier owns the link graph).
        assert!(c.edges.is_empty(), "no link edges from a single visit");
    }

    #[test]
    fn harvest_links_materializes_the_outbound_neighborhood() {
        let body = "<body>\
            <a href='/one'>First</a>\
            <a href='https://other.test/two'>Second</a>\
            <a href='/one'>duplicate of first</a>\
            <a href='#section'>in-page anchor</a>\
            <a href='mailto:x@y.z'>email</a>\
            <a href='javascript:void(0)'>script</a>\
         </body>";
        let c = harvest_links("https://seed.test/page", body).expect("outbound links");
        // Two distinct outbound targets: the dup collapses; fragment / mailto / js skip.
        assert_eq!(c.edges.len(), 2, "two outbound edges: {:?}", c.edges);
        assert_eq!(c.nodes.len(), 3, "seed node + two target nodes");
        assert!(
            c.nodes.iter().any(|n| n.id == "https://seed.test/page"),
            "seed node present"
        );
        assert!(
            c.nodes
                .iter()
                .any(|n| n.id == "https://seed.test/one" && n.title.as_deref() == Some("First")),
            "relative href resolved against seed, first anchor text kept: {:?}",
            c.nodes,
        );
        assert!(c.nodes.iter().any(|n| n.id == "https://other.test/two"));
        assert!(
            c.edges.iter().all(|e| e.subject == "https://seed.test/page"
                && e.predicate == "https://mere.computer/ns/rel#hyperlink"),
            "every edge is seed —Hyperlink→ target: {:?}",
            c.edges,
        );
        assert!(c.edges.iter().any(|e| e.object == "https://seed.test/one"));
        assert!(c.edges.iter().any(|e| e.object == "https://other.test/two"));
    }

    #[test]
    fn harvest_links_is_none_without_outbound_links() {
        assert!(harvest_links("https://seed.test/", "<body><p>no links here</p></body>").is_none());
        // A page whose only links are in-page anchors has no outbound neighborhood.
        assert!(
            harvest_links("https://seed.test/", "<body><a href='#top'>top</a></body>").is_none()
        );
    }

    #[test]
    fn page_extract_is_none_without_metadata_or_for_non_html() {
        // HTML that declares nothing extractable → no contribution.
        assert!(
            page_extract_contribution(
                "https://x.test/",
                Some("text/html"),
                "<body><p>hi</p></body>"
            )
            .is_none(),
            "a page with no title/description/og contributes nothing",
        );
        // Non-HTML → no contribution (JSON-LD has its own harvest path).
        assert!(
            page_extract_contribution("https://x.test/", Some("application/ld+json"), "{}")
                .is_none(),
            "non-HTML is not extracted here",
        );
    }
}
