/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A human-meaningful **display label** for a node.
//!
//! Linked-data ingest mints a node per JSON-LD subject and per referenced
//! resource (see `linked-data`): a page's `author` / `publisher` / `image` and
//! the like. Anonymous subjects (no `@id`) are skolemized to `urn:mere:bnode:…`,
//! and `add_node_with_id` seeds an untitled node's `title` to its URL, so a naive
//! `title`-or-URL label shows that raw URN (or a bare image URL) on the canvas and
//! in the roster. This derives something a person can read instead, from data the
//! node already carries.
//!
//! The derivation **branches on node kind**, which is what keeps it honest: a
//! linked page reached by a `hyperlink` edge must not be labelled "hyperlink", so
//! the incoming-edge *role* is consulted only for anonymous entities, where the
//! role (`author` / `publisher` / `image`) genuinely is the node's identity.
//!
//! - **Anonymous entity** (`urn:mere:bnode:…`): real title → incoming-edge role →
//!   humanized `@type` → a short id.
//! - **Real resource** (an addressable URL): real title → filename (a URL's last
//!   path segment when it carries an extension, e.g. `Rust_logo.svg`) →
//!   cached host → humanized `@type` → the scheme-stripped URL.
//!
//! It is presentation-neutral (deterministic graph-derived text, no theme/host
//! types) and recomputed on read, so ingest stays faithful and the label is free
//! to evolve. Truncation to a widget's width is the caller's concern.

use crate::graph::identity::NodeKey;
use crate::graph::node::Node;
use crate::graph::Graph;
use crate::types::ClassificationScheme;

/// The skolemized-blank-node URI prefix (`linked_data::ingest::skolemize`).
const BNODE_PREFIX: &str = "urn:mere:bnode:";

impl Graph {
    /// A human-meaningful label for `key`'s node (see the module docs for the
    /// derivation). Empty string if the key is not in the graph. Full text, not
    /// width-clamped: a caller drawing into a fixed box truncates.
    pub fn node_display_label(&self, key: NodeKey) -> String {
        let Some(node) = self.get_node(key) else {
            return String::new();
        };
        let url = node.url();

        // A real title (one that is not just the seeded copy of the URL) is always
        // the best label: a page's `<title>`, or an entity's `schema:name`.
        let title = node.title.trim();
        if !title.is_empty() && title != url {
            return title.to_string();
        }

        if let Some(stripped) = url.strip_prefix(BNODE_PREFIX) {
            // Anonymous linked-data entity: name it by its role to its referrer
            // (author / publisher / image), then by its type, then a short id.
            self.incoming_role(key)
                .or_else(|| first_type_term(node))
                .unwrap_or_else(|| short_blank_id(stripped))
        } else {
            // Addressable resource: a filename, else the host, else its type, else
            // the scheme-stripped URL. The host comes from `cached_host` when set
            // (navigated nodes), else parsed from the URL — an ingested node (a
            // linked-data resource that is an edge target, never visited) has no
            // cached host, so without this it fell through to the raw URL.
            url_filename(url)
                .or_else(|| {
                    node.cached_host
                        .as_deref()
                        .filter(|h| !h.trim().is_empty())
                        .map(str::to_string)
                })
                .or_else(|| host_of(url))
                .or_else(|| first_type_term(node))
                .unwrap_or_else(|| scheme_stripped(url))
        }
    }

    /// The humanized role of `key`'s first meaningful incoming edge: the local term
    /// of its predicate IRI (`https://schema.org/author` → `author`). Skips
    /// structural link predicates (`hyperlink` / `references`) that describe the
    /// link, not the node. `None` if no incoming edge carries a usable predicate.
    fn incoming_role(&self, key: NodeKey) -> Option<String> {
        for src in self.in_neighbors(key) {
            let Some(edge_key) = self.find_edge_key(src, key) else {
                continue;
            };
            let Some(sem) = self.get_edge(edge_key).and_then(|p| p.semantic_data()) else {
                continue;
            };
            // The open predicate IRI (raw web predicate from ingest), or the
            // canonical IRI of a recognized sub-kind for a natively-asserted edge.
            let iri = sem.predicate.clone().or_else(|| {
                sem.sub_kinds
                    .iter()
                    .next()
                    .map(|&sk| super::predicate_iri(sk).to_string())
            })?;
            let term = iri_term(&iri);
            if term.is_empty() || term == "hyperlink" || term == "references" {
                continue;
            }
            return Some(humanize_term(term));
        }
        None
    }
}

/// The humanized local term of the node's first `rdf:type` classification
/// (`https://schema.org/Organization` → `Organization`, `…/ImageObject` →
/// `Image Object`). `None` if the node carries no ingested `@type`.
fn first_type_term(node: &Node) -> Option<String> {
    node.classifications
        .iter()
        .find(|c| matches!(&c.scheme, ClassificationScheme::Custom(s) if s == "rdf:type"))
        .map(|c| humanize_term(iri_term(&c.value)))
        .filter(|t| !t.is_empty())
}

/// The local term of an IRI: its fragment or last path segment
/// (`https://schema.org/author` → `author`, `…/rel#cites` → `cites`).
fn iri_term(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

/// Humanize a vocabulary term for display: split `camelCase` and `-` / `_`
/// boundaries into spaces (`ImageObject` → `Image Object`, `thumbnail_url` →
/// `thumbnail url`), leaving an already-plain term (`author`) unchanged.
fn humanize_term(term: &str) -> String {
    let mut out = String::with_capacity(term.len() + 4);
    let mut prev_alnum = false;
    for ch in term.chars() {
        if ch == '-' || ch == '_' {
            if prev_alnum {
                out.push(' ');
            }
            prev_alnum = false;
            continue;
        }
        if ch.is_uppercase() && prev_alnum && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(ch);
        // A lowercase letter or digit before an uppercase marks a camelCase break.
        prev_alnum = ch.is_alphanumeric() && !ch.is_uppercase();
    }
    out.trim().to_string()
}

/// A fetchable URL's filename: its last non-empty path segment when that segment
/// carries a file extension (`…/Rust_logo.svg` → `Rust_logo.svg`). `None` for a
/// directory URL or an extensionless page slug (which falls through to the host).
fn url_filename(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let last = parsed.path_segments()?.filter(|s| !s.is_empty()).next_back()?;
    let dot = last.rfind('.')?;
    // A real extension: a dot with non-empty name and extension around it.
    if dot > 0 && dot + 1 < last.len() {
        Some(last.to_string())
    } else {
        None
    }
}

/// The host of a URL (`https://en.wikipedia.org/wiki/Foo` → `en.wikipedia.org`),
/// for a node whose `cached_host` was never computed (an ingested resource node).
fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_string)
}

/// A short tail for an anonymous node when nothing better is known: the blank
/// label after the leading document-hash namespace (`<16hex>:_:b3` → `node _:b3`).
/// The namespace is the first colon segment; the blank label itself may contain a
/// colon (`_:b3`), so split only off the namespace.
fn short_blank_id(stripped: &str) -> String {
    let label = stripped.split_once(':').map(|(_, rest)| rest).unwrap_or(stripped);
    format!("node {label}")
}

/// A URL with its scheme dropped, capped to a sane label width.
fn scheme_stripped(url: &str) -> String {
    let body = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    body.chars().take(48).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ClassificationProvenance, ClassificationScheme, ClassificationStatus, NodeClassification,
    };
    use euclid::default::Point2D;
    use uuid::Uuid;

    fn rdf_type(iri: &str) -> NodeClassification {
        NodeClassification {
            scheme: ClassificationScheme::Custom("rdf:type".to_string()),
            value: iri.to_string(),
            label: None,
            confidence: 1.0,
            provenance: ClassificationProvenance::Imported,
            status: ClassificationStatus::Imported,
            primary: false,
        }
    }

    fn add(graph: &mut Graph, url: &str) -> NodeKey {
        graph.add_node_with_id(Graph::node_namespace_id(url), url.to_string(), Point2D::zero())
    }

    #[test]
    fn real_title_wins_for_any_node() {
        let mut g = Graph::new();
        let page = add(&mut g, "https://ex.test/article");
        g.get_node_mut(page).unwrap().title = "Rust (programming language)".to_string();
        assert_eq!(g.node_display_label(page), "Rust (programming language)");
    }

    #[test]
    fn anonymous_entity_falls_to_incoming_role_then_type() {
        let mut g = Graph::new();
        let page = add(&mut g, "https://ex.test/article");
        // A nameless publisher blank node typed Organization, reached via publisher.
        let publisher = add(&mut g, "urn:mere:bnode:deadbeefdeadbeef:_:b0");
        g.add_node_classification(publisher, rdf_type("https://schema.org/Organization"));
        g.assert_semantic_predicate(page, publisher, "https://schema.org/publisher".to_string());
        // Role wins over type.
        assert_eq!(g.node_display_label(publisher), "publisher");

        // With no incoming edge, the same node falls to its humanized @type.
        let author = add(&mut g, "urn:mere:bnode:deadbeefdeadbeef:_:b1");
        g.add_node_classification(author, rdf_type("https://schema.org/ImageObject"));
        assert_eq!(g.node_display_label(author), "Image Object");
    }

    #[test]
    fn anonymous_entity_with_a_name_uses_the_name() {
        let mut g = Graph::new();
        let page = add(&mut g, "https://ex.test/article");
        let publisher = add(&mut g, "urn:mere:bnode:deadbeefdeadbeef:_:b0");
        g.get_node_mut(publisher).unwrap().title = "Wikimedia Foundation".to_string();
        g.assert_semantic_predicate(page, publisher, "https://schema.org/publisher".to_string());
        assert_eq!(g.node_display_label(publisher), "Wikimedia Foundation");
    }

    #[test]
    fn image_resource_uses_its_filename() {
        let mut g = Graph::new();
        let img = add(&mut g, "https://upload.example.test/wiki/Rust_logo.svg");
        assert_eq!(g.node_display_label(img), "Rust_logo.svg");
    }

    #[test]
    fn extensionless_page_falls_to_its_host() {
        let mut g = Graph::new();
        let page = add(&mut g, "https://en.wikipedia.org/wiki/Rust_(programming_language)");
        // `add_node_with_id` caches the host; no extension on the slug, so host wins.
        assert_eq!(g.node_display_label(page), "en.wikipedia.org");
    }

    #[test]
    fn resource_without_cached_host_derives_host_from_url() {
        let mut g = Graph::new();
        let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
        let key = add(&mut g, url);
        // An ingested resource node (a linked-data edge target, never navigated) has
        // no cached host; the host is parsed from the URL instead of the raw URL.
        g.get_node_mut(key).unwrap().cached_host = None;
        assert_eq!(g.node_display_label(key), "en.wikipedia.org");
    }

    #[test]
    fn anonymous_with_nothing_known_gets_a_short_id() {
        let mut g = Graph::new();
        let blank = add(&mut g, "urn:mere:bnode:deadbeefdeadbeef:_:b7");
        assert_eq!(g.node_display_label(blank), "node _:b7");
    }
}
