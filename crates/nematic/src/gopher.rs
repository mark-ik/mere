/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Gopher menu engine — parses RFC 1436 gopher menus into a portable
//! document.
//!
//! A gopher menu is a sequence of tab-delimited lines, each of the form:
//!
//! ```text
//! <type><display>\t<selector>\t<host>\t<port>\r\n
//! ```
//!
//! The first character of each line is the item type. A bare `.` on its own
//! line terminates the menu.
//!
//! Item types this engine handles explicitly:
//!
//! - `i` informational text (no resource) — merged into paragraph runs
//! - `0` text file — emitted as link
//! - `1` submenu / directory — emitted as link
//! - `7` full-text search server — emitted as link
//! - `9` binary — emitted as link
//! - `g` / `I` images — emitted as link
//! - `s` sound — emitted as link
//! - `T` telnet — emitted as link
//! - `h` URL item (selector starts with `URL:`) — extracted URL emitted
//!   as link
//! - `3` server error — folded into informational paragraph text
//!
//! Unknown types are still emitted as a synthesised `gopher://` link so the
//! menu remains navigable even when the type isn't in the table above.
//!
//! References:
//! - RFC 1436 (The Internet Gopher Protocol)
//! - RFC 4266 (gopher URI scheme)

use inker::{
    DocumentBlock, DocumentProvenance, DocumentTrustState, Engine, EngineDocument, EngineError,
    EngineInput, InlineSpan,
};

/// Stable engine identifier.
pub const ENGINE_ID: &str = "nematic.gopher";

/// Gopher menu engine.
pub struct GopherEngine;

impl GopherEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GopherEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for GopherEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError> {
        let mut blocks: Vec<DocumentBlock> = Vec::new();
        let mut info_run: Vec<String> = Vec::new();

        for line in input.body.lines() {
            if line == "." {
                // RFC 1436 terminator.
                break;
            }
            if line.is_empty() {
                continue;
            }

            let Some(item) = parse_line(line) else {
                continue;
            };

            match item {
                Item::Info(text) => info_run.push(text),
                Item::Resource { display, url } => {
                    flush_info(&mut info_run, &mut blocks);
                    blocks.push(link_paragraph(display, url));
                }
            }
        }
        flush_info(&mut info_run, &mut blocks);

        Ok(EngineDocument {
            address: input.address.clone(),
            title: None,
            content_type: input
                .content_type
                .clone()
                .unwrap_or_else(|| "application/gopher-menu".to_string()),
            lang: None,
            provenance: DocumentProvenance::for_engine(self.engine_id(), &input.address),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks,
        })
    }
}

enum Item {
    Info(String),
    Resource { display: String, url: String },
}

fn parse_line(line: &str) -> Option<Item> {
    let mut chars = line.chars();
    let type_char = chars.next()?;
    let rest = chars.as_str();

    let mut parts = rest.splitn(4, '\t');
    let display = parts.next()?.to_string();
    let selector = parts.next().unwrap_or("");
    let host = parts.next().unwrap_or("");
    let port = parts.next().unwrap_or("70");

    match type_char {
        'i' => Some(Item::Info(display)),
        '3' => Some(Item::Info(format!("[error] {display}"))),
        'h' => {
            // URL items: selector is typically "URL:https://..." or
            // "URL:gemini://...". Strip the prefix; if the selector doesn't
            // carry a usable URL, skip the line.
            let url = selector.strip_prefix("URL:").unwrap_or(selector).trim();
            if url.is_empty() {
                return None;
            }
            Some(Item::Resource {
                display,
                url: url.to_string(),
            })
        }
        _ => {
            if host.is_empty() {
                return None;
            }
            let url = synthesise_gopher_url(type_char, host, port, selector);
            Some(Item::Resource { display, url })
        }
    }
}

fn synthesise_gopher_url(type_char: char, host: &str, port: &str, selector: &str) -> String {
    let port_part = if port.is_empty() || port == "70" {
        String::new()
    } else {
        format!(":{port}")
    };
    // RFC 4266: gopher-path = <gophertype><selector>. The type character is
    // the first path segment, immediately followed by the selector
    // (selectors may already begin with `/`).
    format!("gopher://{host}{port_part}/{type_char}{selector}")
}

fn flush_info(info_run: &mut Vec<String>, blocks: &mut Vec<DocumentBlock>) {
    if info_run.is_empty() {
        return;
    }
    let mut spans = Vec::with_capacity(info_run.len() * 2);
    let mut first = true;
    for line in info_run.drain(..) {
        if !first {
            spans.push(InlineSpan::SoftBreak);
        }
        spans.push(InlineSpan::Text(line));
        first = false;
    }
    blocks.push(DocumentBlock::Paragraph { spans });
}

fn link_paragraph(display: String, url: String) -> DocumentBlock {
    DocumentBlock::Paragraph {
        spans: vec![InlineSpan::Link {
            url,
            title: None,
            spans: vec![InlineSpan::Text(display)],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(body: &str) -> EngineDocument {
        GopherEngine::new()
            .render(&EngineInput::new("gopher://test/", body))
            .expect("render")
    }

    fn line(t: char, display: &str, selector: &str, host: &str, port: &str) -> String {
        format!("{t}{display}\t{selector}\t{host}\t{port}\r\n")
    }

    #[test]
    fn engine_id_is_stable() {
        assert_eq!(GopherEngine::new().engine_id(), "nematic.gopher");
    }

    #[test]
    fn standard_text_item_synthesises_gopher_url() {
        let body = line('0', "Welcome text", "/welcome.txt", "example.test", "70");
        let doc = render(&body);
        let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let InlineSpan::Link { url, .. } = &spans[0] else {
            panic!("expected link");
        };
        assert_eq!(url, "gopher://example.test/0/welcome.txt");
    }

    #[test]
    fn non_default_port_appears_in_url() {
        let body = line('1', "Sub", "/sub", "example.test", "7070");
        let doc = render(&body);
        let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let InlineSpan::Link { url, .. } = &spans[0] else {
            panic!("expected link");
        };
        assert_eq!(url, "gopher://example.test:7070/1/sub");
    }

    #[test]
    fn url_item_extracts_url_prefix() {
        let body = line('h', "External", "URL:https://example.test/", ".", "70");
        let doc = render(&body);
        let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let InlineSpan::Link { url, .. } = &spans[0] else {
            panic!("expected link");
        };
        assert_eq!(url, "https://example.test/");
    }

    #[test]
    fn info_lines_merge_into_one_paragraph() {
        let body = format!(
            "{}{}{}",
            line('i', "Welcome to the menu", "", "example.test", "70"),
            line('i', "More info on a second line", "", "example.test", "70"),
            line('i', "Final info line", "", "example.test", "70"),
        );
        let doc = render(&body);
        assert_eq!(doc.blocks.len(), 1);
        let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let breaks = spans
            .iter()
            .filter(|s| matches!(s, InlineSpan::SoftBreak))
            .count();
        assert_eq!(breaks, 2);
    }

    #[test]
    fn info_then_resource_then_info_yields_three_blocks() {
        let body = format!(
            "{}{}{}",
            line('i', "header", "", "example.test", "70"),
            line('1', "submenu", "/sub", "example.test", "70"),
            line('i', "footer", "", "example.test", "70"),
        );
        let doc = render(&body);
        assert_eq!(doc.blocks.len(), 3);
    }

    #[test]
    fn period_terminator_stops_parsing() {
        let body = format!(
            "{}{}{}",
            line('i', "before", "", "example.test", "70"),
            ".\r\n",
            line('1', "should-not-appear", "/x", "example.test", "70"),
        );
        let doc = render(&body);
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn error_lines_become_info_with_prefix() {
        let body = line('3', "host unreachable", "", "example.test", "70");
        let doc = render(&body);
        let DocumentBlock::Paragraph { spans } = &doc.blocks[0] else {
            panic!("expected paragraph");
        };
        let InlineSpan::Text(text) = &spans[0] else {
            panic!("expected text");
        };
        assert!(text.starts_with("[error]"), "got: {text}");
    }

    #[test]
    fn resource_with_missing_host_is_skipped() {
        // Tab-delimited but with empty host — malformed but realistic.
        let body = "1Bad item\t/sel\t\t70\r\n";
        let doc = render(body);
        assert!(doc.blocks.is_empty());
    }

    #[test]
    fn outgoing_links_collect_all_resource_urls() {
        let body = format!(
            "{}{}{}",
            line('i', "header", "", ".", "70"),
            line('1', "sub", "/sub", "example.test", "70"),
            line('h', "ext", "URL:https://example.test/", ".", "70"),
        );
        let doc = render(&body);
        let urls = doc.outgoing_links();
        assert_eq!(
            urls,
            vec!["gopher://example.test/1/sub", "https://example.test/"]
        );
    }

    #[test]
    fn dispatches_through_inker_registry() {
        use inker::EngineRegistry;
        use inker::routing::{
            EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId,
        };

        let mut registry = EngineRegistry::new();
        registry.register(Box::new(GopherEngine::new()));
        let decision = EngineRouteDecision {
            engine_id: ENGINE_ID.to_string(),
            surface_contract: SurfaceContract {
                target: SurfaceTargetId::new("gopher:1"),
                mode: SurfaceContractMode::CompositedTexture,
            },
        };
        let body = line('1', "Submenu", "/s", "example.test", "70");
        let doc = registry
            .dispatch(
                &decision,
                &EngineInput::new("gopher://example.test/", &body),
            )
            .expect("dispatch");
        assert_eq!(doc.outgoing_links(), vec!["gopher://example.test/1/s"]);
    }
}
