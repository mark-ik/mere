// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Navigation — resolve an address to content, then render it through the
//! engine seam.
//!
//! This is the omnibar's other half: [`open`] takes an address, fetches its
//! bytes, and routes them through `inker` via [`crate::engine_tile`]. There is
//! no network fetcher yet (that is the `netfetcher` slice), so [`fetch`]
//! handles only what the host can resolve locally: the seeded `mere://`
//! pages and `file://` paths. Network schemes return an honest placeholder.

use std::path::Path;

use inker::routing::{
    ENGINE_LINKED_DATA_INGEST, EngineRoutePolicy, EngineRouteRequest, WorkspaceRouteId,
    is_graph_contribution_route,
};
use inker::{DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan};
use kernel::graph::Graph;
use linked_data::{apply_contribution, from_html, from_jsonld};

use crate::engine_tile::{RenderedTile, error_document, render_address};

/// Seeded welcome page, served at `mere://welcome`.
pub(crate) const WELCOME_MD: &str = "\
# Welcome to Mere

A **spatial browser** on the *composition spine*: graph truth → forme →
platen → verso → inker.

Type an address above and press Enter. Navigation routes through `inker`'s
engine policy; this page was parsed by the `nematic` markdown engine.

## What you can open now

- `mere://welcome` — this page
- `file:///path/to/notes.md` — a local markdown / gemtext / text file
- network schemes (`https://`, `gemini://`) — *not yet*; that is the
  `netfetcher` slice
- `mere://demo.jsonld` — a JSON-LD document that *merges into the graph*
  rather than rendering (open it, then watch the orrery)
- `mere://demo.html` — an HTML page whose embedded JSON-LD is *harvested* into
  the graph as it opens

---

    omnibar → fetch(address) → route(content_type) → Engine → EngineDocument
";

/// A seeded linked-data document at `mere://demo.jsonld`, so the ingest path is
/// exercisable without a network fetcher. Inline `@context` (no remote fetch);
/// two papers joined by a `cites` edge.
pub(crate) const DEMO_JSONLD: &str = r#"{
  "@context": {
    "name": "https://schema.org/name",
    "cites": "https://mere.computer/ns/rel#cites"
  },
  "@graph": [
    { "@id": "mere://node/paper-a", "name": "Paper A", "cites": { "@id": "mere://node/paper-b" } },
    { "@id": "mere://node/paper-b", "name": "Paper B" }
  ]
}"#;

/// A seeded HTML page at `mere://demo.html` with embedded JSON-LD, so the
/// HTML-harvest path is exercisable. Opening it renders the page (no HTML
/// renderer is registered yet, so the tile is a placeholder) and merges the
/// embedded data into the graph.
pub(crate) const DEMO_HTML: &str = r#"<!doctype html>
<html><head><title>Demo HTML</title>
<script type="application/ld+json">
{"@context":{"name":"https://schema.org/name","cites":"https://mere.computer/ns/rel#cites"},
 "@id":"mere://node/html-a","name":"HTML Paper A","cites":{"@id":"mere://node/html-b"}}
</script>
</head><body>
<h1>A page with embedded linked data</h1>
</body></html>"#;

/// Resolve `address` to content and render it. Always returns a tile (errors
/// and unsupported schemes become a rendered page, never a panic).
pub fn open(address: &str) -> RenderedTile {
    let (body, content_type) = fetch(address);
    render_address(address, &body, content_type.as_deref())
}

/// Resolve `address`, then either **ingest** it into `graph` (a
/// graph-contribution route — currently `application/ld+json`) or **render** it
/// (everything else, exactly as [`open`]). The ingest branch parses the body to
/// a `GraphContribution` and merges it.
///
/// Classification uses the *unfiltered* policy on purpose: the linked-data
/// marker ([`ENGINE_LINKED_DATA_INGEST`]) is not a registered engine, so a
/// registry-filtered route (as in [`crate::engine_tile::render_address`]) would
/// skip its rule and fall through.
pub fn resolve(address: &str, graph: &mut Graph) -> (RenderedTile, bool) {
    let (body, content_type) = fetch(address);
    let request = EngineRouteRequest {
        workspace_id: WorkspaceRouteId::new("mere"),
        view: None,
        node: None,
        address: address.to_string(),
        content_type: content_type.clone(),
        pinned_engine: None,
    };
    let decision = EngineRoutePolicy::default().route(&request);
    if is_graph_contribution_route(&decision.engine_id) {
        return (ingest_jsonld(address, &body, graph), true);
    }
    // An HTML page also contributes any embedded JSON-LD while it renders.
    let mut graph_changed = false;
    if content_type.as_deref().is_some_and(is_html) {
        for contribution in from_html(&body) {
            let outcome = apply_contribution(graph, &contribution);
            graph_changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
        }
    }
    (render_address(address, &body, content_type.as_deref()), graph_changed)
}

/// Whether a content type is HTML (ignoring any `; charset=…` suffix).
fn is_html(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .eq_ignore_ascii_case("text/html")
}

/// Parse `body` as JSON-LD and merge it into `graph`; the returned tile is a
/// short summary, or an error page on a parse failure. `@id`s become nodes,
/// `rel#` / raw predicates become `Semantic` edges, `schema:name` / `keywords`
/// become title / tags. No remote `@context` is fetched (inline / expanded only).
fn ingest_jsonld(address: &str, body: &str, graph: &mut Graph) -> RenderedTile {
    match from_jsonld(body.as_bytes()) {
        Ok(contribution) => {
            let outcome = apply_contribution(graph, &contribution);
            RenderedTile {
                engine_id: ENGINE_LINKED_DATA_INGEST.to_string(),
                document: ingest_summary_document(address, &outcome),
            }
        }
        Err(err) => RenderedTile {
            engine_id: ENGINE_LINKED_DATA_INGEST.to_string(),
            document: error_document(address, ENGINE_LINKED_DATA_INGEST, &err.to_string()),
        },
    }
}

/// A small "what was ingested" page, shown in the document pane after a merge.
fn ingest_summary_document(address: &str, outcome: &linked_data::ApplyOutcome) -> EngineDocument {
    let mut blocks = vec![
        DocumentBlock::Badge {
            text: "linked data ingested".to_string(),
        },
        DocumentBlock::Paragraph {
            spans: vec![InlineSpan::Text(format!(
                "Merged {} node(s) and {} edge(s) from {address} into the graph.",
                outcome.nodes_created, outcome.edges_asserted
            ))],
        },
    ];
    if outcome.edges_skipped > 0 {
        blocks.push(DocumentBlock::Paragraph {
            spans: vec![InlineSpan::Text(format!(
                "{} edge(s) skipped (unresolved endpoint).",
                outcome.edges_skipped
            ))],
        });
    }
    EngineDocument {
        address: address.to_string(),
        title: Some("Linked data ingested".to_string()),
        content_type: "application/ld+json".to_string(),
        lang: None,
        provenance: DocumentProvenance::for_engine(ENGINE_LINKED_DATA_INGEST, address),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks,
    }
}

/// Locally resolve an address to `(body, content_type)`. No network.
fn fetch(address: &str) -> (String, Option<String>) {
    if address == "mere://welcome" {
        return (WELCOME_MD.to_string(), Some("text/markdown".to_string()));
    }

    if address == "mere://demo.jsonld" {
        return (DEMO_JSONLD.to_string(), Some("application/ld+json".to_string()));
    }

    if address == "mere://demo.html" {
        return (DEMO_HTML.to_string(), Some("text/html".to_string()));
    }

    if let Some(name) = address.strip_prefix("mere://") {
        // Any other mere:// address gets a generated page, so orrery nodes
        // (seeded with mere:// urls) open to real rendered content.
        return (
            format!(
                "# {name}\n\nA seeded Mere page at `{address}`.\n\nOpened from the orrery. \
                 Navigation routed through `inker`; rendered by the `nematic` markdown engine."
            ),
            Some("text/markdown".to_string()),
        );
    }

    if let Some(raw) = address.strip_prefix("file://") {
        let path = normalize_file_path(raw);
        return match std::fs::read_to_string(&path) {
            Ok(body) => (body, content_type_for_path(&path)),
            Err(e) => (
                format!("# Cannot read file\n\n`{path}`\n\n{e}"),
                Some("text/markdown".to_string()),
            ),
        };
    }

    (
        format!(
            "# No fetcher yet\n\nMere can't fetch `{address}` yet — network fetch is the \
             `netfetcher` slice. Try `mere://welcome` or a `file://` path to a local \
             `.md` / `.gmi` / `.txt` file."
        ),
        Some("text/markdown".to_string()),
    )
}

/// Turn the part after `file://` into a filesystem path. Handles the
/// `file:///C:/...` Windows form (a leading slash before a drive letter) while
/// leaving POSIX absolute paths (`/home/...`) intact.
fn normalize_file_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        raw[1..].to_string()
    } else {
        raw.to_string()
    }
}

/// Content type from a file extension; `None` lets the file engine sniff.
fn content_type_for_path(path: &str) -> Option<String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ct = match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "gmi" | "gemini" => "text/gemini",
        "txt" | "text" => "text/plain",
        "jsonld" => "application/ld+json",
        "html" | "htm" => "text/html",
        _ => return None,
    };
    Some(ct.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonld_demo_ingests_papers_and_a_cites_edge() {
        use kernel::graph::Graph;
        let mut graph = Graph::new();
        let (tile, changed) = resolve("mere://demo.jsonld", &mut graph);
        assert!(changed, "ingest mutated the graph");
        assert_eq!(tile.engine_id, ENGINE_LINKED_DATA_INGEST);
        let (a, _) = graph
            .get_node_by_url("mere://node/paper-a")
            .expect("paper-a ingested");
        let (b, _) = graph
            .get_node_by_url("mere://node/paper-b")
            .expect("paper-b ingested");
        assert!(graph.find_edge_key(a, b).is_some(), "cites edge");
    }

    #[test]
    fn html_demo_harvests_embedded_jsonld_into_the_graph() {
        use kernel::graph::Graph;
        let mut graph = Graph::new();
        let (_, changed) = resolve("mere://demo.html", &mut graph);
        assert!(changed, "harvest mutated the graph");
        assert!(graph.get_node_by_url("mere://node/html-a").is_some());
        assert!(graph.get_node_by_url("mere://node/html-b").is_some());
    }

    #[test]
    fn welcome_routes_to_markdown_engine() {
        let tile = open("mere://welcome");
        assert_eq!(tile.engine_id, "nematic.markdown");
        assert!(!tile.document.blocks.is_empty());
    }

    #[test]
    fn mere_path_renders_a_generated_markdown_page() {
        let tile = open("mere://node/3");
        assert_eq!(tile.engine_id, "nematic.markdown");
        assert!(!tile.document.blocks.is_empty());
    }

    #[test]
    fn unknown_scheme_renders_a_placeholder_not_a_panic() {
        let tile = open("https://example.com");
        // Rendered as markdown; the body names the missing fetcher.
        assert!(tile.document.title.as_deref() == Some("No fetcher yet"));
    }

    #[test]
    fn windows_drive_path_loses_its_leading_slash() {
        assert_eq!(normalize_file_path("/C:/x/y.md"), "C:/x/y.md");
        assert_eq!(normalize_file_path("/home/u/y.md"), "/home/u/y.md");
    }

    #[test]
    fn content_type_by_extension() {
        assert_eq!(content_type_for_path("a.md").as_deref(), Some("text/markdown"));
        assert_eq!(content_type_for_path("a.gmi").as_deref(), Some("text/gemini"));
        assert_eq!(content_type_for_path("a.bin"), None);
    }
}
