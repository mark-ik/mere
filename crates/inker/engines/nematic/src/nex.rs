/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Nex engine — parser for Nex (`nex://`), the minimal smolweb protocol
//! (<https://nex.nightfall.city>).
//!
//! Nex distinguishes two response shapes:
//!
//! - **Directory listings** — one entry per line. Lines ending in `/` are
//!   subdirectories; lines without a trailing `/` are files. There is no
//!   item-type prefix (unlike gopher).
//! - **Plain text** — content responses; the body is just text.
//!
//! Detection is by line shape: if every non-empty line is *short* and
//! either ends with `/` or contains no whitespace, treat the body as a
//! directory listing. Otherwise delegate body parsing to the text engine.
//!
//! Directory entries are emitted as a [`DocumentBlock::List`] of
//! [`InlineSpan::Link`] spans, with URLs synthesised relative to
//! [`EngineInput::address`].

use inker::{
    DocumentBlock, DocumentProvenance, DocumentTrustState, Engine, EngineDocument, EngineError,
    EngineInput, InlineSpan,
};

use crate::TextEngine;

/// Stable engine identifier.
pub const ENGINE_ID: &str = "nematic.nex";

/// Nex protocol body engine.
pub struct NexEngine {
    text: TextEngine,
}

impl NexEngine {
    pub fn new() -> Self {
        Self {
            text: TextEngine::new(),
        }
    }
}

impl Default for NexEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for NexEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError> {
        if looks_like_directory(&input.body) {
            Ok(render_directory(&input.address, &input.body))
        } else {
            let mut doc = self.text.render(input)?;
            doc.provenance = DocumentProvenance {
                source_kind: Some(ENGINE_ID.to_string()),
                canonical_uri: Some(input.address.clone()),
                fetched_at: None,
                source_label: Some("nematic.text".to_string()),
            };
            doc.trust = DocumentTrustState::Unknown;
            Ok(doc)
        }
    }
}

/// True when every non-empty line is a plausible directory entry: either
/// ends with `/`, or is a single whitespace-free token of bounded length.
fn looks_like_directory(body: &str) -> bool {
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return false;
    }
    lines
        .iter()
        .all(|line| is_directory_entry_line(line.trim()))
}

fn is_directory_entry_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    if line.len() > 200 {
        return false;
    }
    if line.ends_with('/') {
        return !line.contains(char::is_whitespace);
    }
    !line.contains(char::is_whitespace)
}

fn render_directory(address: &str, body: &str) -> EngineDocument {
    let base = base_url(address);
    let items: Vec<Vec<DocumentBlock>> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let url = format!("{base}{line}");
            vec![DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Link {
                    url,
                    title: None,
                    spans: vec![InlineSpan::Text(line.to_string())],
                    predicate: None,
                }],
            }]
        })
        .collect();

    let blocks = if items.is_empty() {
        Vec::new()
    } else {
        vec![DocumentBlock::List {
            ordered: false,
            items,
        }]
    };

    EngineDocument {
        address: address.to_string(),
        title: None,
        content_type: "application/x-nex-listing".to_string(),
        lang: None,
        provenance: DocumentProvenance::for_engine(ENGINE_ID, address),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks,
    }
}

/// Compute the base URL that directory entries should resolve against.
/// `nex://host/path/` → `nex://host/path/`. `nex://host/page` →
/// `nex://host/`. Falls back to appending `/` if the address has no slash.
fn base_url(address: &str) -> String {
    if address.ends_with('/') {
        return address.to_string();
    }
    if let Some(idx) = address.rfind('/') {
        // Ensure the slash we keep is part of the path, not the `://` scheme
        // separator. Find the start of the authority section and bail if we
        // would land inside it.
        if let Some(scheme_end) = address.find("://") {
            if idx <= scheme_end + 2 {
                return format!("{address}/");
            }
        }
        return address[..=idx].to_string();
    }
    format!("{address}/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(address: &str, body: &str) -> EngineDocument {
        NexEngine::new()
            .render(&EngineInput::new(address, body))
            .expect("render")
    }

    #[test]
    fn engine_id_is_stable() {
        assert_eq!(NexEngine::new().engine_id(), "nematic.nex");
    }

    #[test]
    fn directory_listing_emits_list_of_links() {
        let body = "README.txt\nabout/\nphotos/\ncontact.txt\n";
        let doc = render("nex://example.test/", body);
        assert_eq!(doc.content_type, "application/x-nex-listing");

        let DocumentBlock::List { items, .. } = &doc.blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 4);

        let urls = doc.outgoing_links();
        assert_eq!(
            urls,
            vec![
                "nex://example.test/README.txt",
                "nex://example.test/about/",
                "nex://example.test/photos/",
                "nex://example.test/contact.txt",
            ]
        );
    }

    #[test]
    fn directory_listing_resolves_against_path_base() {
        let body = "child.txt\nsub/\n";
        let doc = render("nex://example.test/path/", body);
        assert_eq!(
            doc.outgoing_links(),
            vec![
                "nex://example.test/path/child.txt",
                "nex://example.test/path/sub/",
            ]
        );
    }

    #[test]
    fn directory_listing_falls_back_when_address_lacks_trailing_slash() {
        // `nex://host/page` — the `/page` part is dropped; entries resolve
        // against the parent.
        let body = "next.txt\n";
        let doc = render("nex://example.test/page", body);
        assert_eq!(doc.outgoing_links(), vec!["nex://example.test/next.txt"]);
    }

    #[test]
    fn content_response_dispatches_to_text() {
        let body = "This is a content response.\n\nIt has multiple paragraphs of prose with spaces and punctuation.\n";
        let doc = render("nex://example.test/page", body);
        // Text engine produces paragraphs, not a List.
        assert!(matches!(doc.blocks[0], DocumentBlock::Paragraph { .. }));
        assert_eq!(doc.content_type, "text/plain");
        assert_eq!(doc.provenance.source_kind.as_deref(), Some("nematic.nex"));
        assert_eq!(doc.provenance.source_label.as_deref(), Some("nematic.text"));
    }

    #[test]
    fn empty_body_falls_back_to_text() {
        let doc = render("nex://example.test/empty", "");
        // Empty bodies are not directory listings — TextEngine produces no
        // blocks for empty bodies.
        assert!(doc.blocks.is_empty());
    }

    #[test]
    fn lines_with_whitespace_disqualify_directory_detection() {
        // First line is a valid entry, but second line has whitespace ("a b").
        let body = "ok\na b\n";
        let doc = render("nex://example.test/", body);
        assert!(matches!(doc.blocks[0], DocumentBlock::Paragraph { .. }));
    }

    #[test]
    fn dispatches_through_inker_registry() {
        use inker::EngineRegistry;
        use inker::routing::{
            EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId,
        };

        let mut registry = EngineRegistry::new();
        registry.register(Box::new(NexEngine::new()));
        let decision = EngineRouteDecision {
            engine_id: ENGINE_ID.to_string(),
            surface_contract: SurfaceContract {
                target: SurfaceTargetId::new("nex:1"),
                mode: SurfaceContractMode::CompositedTexture,
            },
        };
        let doc = registry
            .dispatch(
                &decision,
                &EngineInput::new("nex://example.test/", "alpha\nbeta/\n"),
            )
            .expect("dispatch");
        assert_eq!(doc.outgoing_links().len(), 2);
    }
}
