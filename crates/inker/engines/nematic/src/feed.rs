/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Feed engine — parses RSS 2.0, Atom 1.0, and JSON Feed 1.x into a
//! portable document.
//!
//! The two XML formats share one event-driven walker: they differ in
//! element names but share the same logical shape — a feed-level title
//! plus a sequence of entries, each carrying a title, link, and summary.
//! RSS expresses links as element text; Atom uses `<link href=…>`
//! attributes. JSON Feed is parsed separately (it is a JSON object, not
//! XML) but lands in the same intermediate [`Parsed`] shape, so all three
//! flavours share [`build_document_blocks`]. The flavour is chosen by
//! [`looks_like_json_feed`]: the declared content type first, a body sniff
//! as fallback.
//!
//! The output layout is:
//!
//! 1. Feed title becomes [`EngineDocument::title`].
//! 2. Each entry projects as a level-2 heading (entry title) followed by a
//!    paragraph containing the entry link. If a summary / description /
//!    content body is present, a third paragraph holds the de-tagged
//!    summary text.
//!
//! Summary handling is deliberately lossy in v1: HTML in `<description>` /
//! `<content>` is stripped to plain text. A future slice can add a real
//! HTML parse path that preserves emphasis / links inside summaries.

use inker::{
    DocumentBlock, DocumentDiagnostic, DocumentProvenance, DocumentTrustState, Engine,
    EngineDocument, EngineError, EngineInput,
};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use serde::Deserialize;

/// Stable engine identifier.
pub const ENGINE_ID: &str = "nematic.feed";

/// RSS / Atom feed engine.
pub struct FeedEngine;

impl FeedEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FeedEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for FeedEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError> {
        let is_json = looks_like_json_feed(input);
        let parsed = if is_json {
            parse_json(&input.body)?
        } else {
            parse(&input.body)?
        };
        let title = parsed.feed_title.clone();
        let lang = parsed.feed_lang.clone();
        let (blocks, diagnostics) = build_document_blocks(parsed);

        let default_content_type = if is_json {
            "application/feed+json"
        } else {
            "application/feed+xml"
        };

        Ok(EngineDocument {
            address: input.address.clone(),
            title,
            content_type: input
                .content_type
                .clone()
                .unwrap_or_else(|| default_content_type.to_string()),
            lang,
            provenance: DocumentProvenance::for_engine(self.engine_id(), &input.address),
            trust: DocumentTrustState::Unknown,
            diagnostics,
            blocks,
        })
    }
}

#[derive(Default)]
struct Parsed {
    feed_title: Option<String>,
    feed_subtitle: Option<String>,
    feed_link: Option<String>,
    feed_lang: Option<String>,
    entries: Vec<Entry>,
    /// Count of entry summaries / contents that contained HTML tags we
    /// stripped. Surfaces as a `DegradedRendering` diagnostic on the doc.
    html_stripped_count: usize,
}

#[derive(Default)]
struct Entry {
    title: Option<String>,
    link: Option<String>,
    date: Option<String>,
    summary: Option<String>,
}

fn parse(body: &str) -> Result<Parsed, EngineError> {
    let mut reader = Reader::from_str(body);
    // Don't enable trim_text: quick-xml splits text events around entity
    // references (`&lt;`, `&gt;`, etc.), and trimming each chunk eats the
    // spaces *between* them. Element-level trimming happens at commit.
    let mut buf = Vec::new();
    let mut state = State::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                state.start_element(local.clone());
                if local == "link" {
                    if let Some(href) = atom_href(&e) {
                        if state.in_entry() {
                            state.set_entry_link(href);
                        } else if state.parsed.feed_link.is_none() {
                            state.parsed.feed_link = Some(href);
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                // Atom self-closing <link href="..." rel="alternate"/>.
                let local = local_name(e.name().as_ref());
                if local == "link" {
                    if let Some(href) = atom_href(&e) {
                        if state.in_entry() {
                            state.set_entry_link(href);
                        } else if state.parsed.feed_link.is_none() {
                            state.parsed.feed_link = Some(href);
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let raw =
                    std::str::from_utf8(t.as_ref()).map_err(|e| invalid_xml(e.to_string()))?;
                let unescaped = unescape(raw).map_err(invalid_xml)?.into_owned();
                state.append_text(&unescaped);
            }
            Ok(Event::GeneralRef(r)) => {
                // quick-xml 0.39 emits XML entity references (`&lt;`, `&gt;`,
                // `&amp;`, numeric refs, etc.) as their own event, splitting
                // them out of surrounding Text. Resolve to the actual
                // character(s) and append to the current text accumulator
                // so descriptions / summaries see the same string the
                // author wrote.
                let name =
                    std::str::from_utf8(r.as_ref()).map_err(|e| invalid_xml(e.to_string()))?;
                let unescaped = unescape(&format!("&{name};"))
                    .map_err(invalid_xml)?
                    .into_owned();
                state.append_text(&unescaped);
            }
            Ok(Event::CData(c)) => {
                let raw = std::str::from_utf8(c.as_ref())
                    .map_err(|err| EngineError::InvalidContent(err.to_string()))?;
                state.append_text(raw);
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                state.end_element(&local);
            }
            Ok(Event::Eof) => {
                // Truncated content leaves elements on the path stack. Treat
                // that as InvalidContent rather than silently returning a
                // partial document — feed parsers that swallow truncation
                // hide bugs in the upstream fetch pipeline.
                if !state.path.is_empty() {
                    return Err(EngineError::InvalidContent(format!(
                        "feed truncated; unclosed element <{}>",
                        state.path.last().map(String::as_str).unwrap_or("?")
                    )));
                }
                break;
            }
            Err(err) => return Err(invalid_xml(err)),
            _ => {}
        }
        buf.clear();
    }

    Ok(state.into_parsed())
}

#[derive(Default)]
struct State {
    path: Vec<String>,
    pending_text: String,
    current_entry: Option<Entry>,
    parsed: Parsed,
}

impl State {
    fn in_entry(&self) -> bool {
        self.path.iter().any(|p| p == "item" || p == "entry")
    }

    fn start_element(&mut self, name: String) {
        self.pending_text.clear();
        if name == "item" || name == "entry" {
            self.current_entry = Some(Entry::default());
        }
        self.path.push(name);
    }

    fn end_element(&mut self, name: &str) {
        let text = std::mem::take(&mut self.pending_text);
        let trimmed = text.trim();

        // Path snapshot before pop: parent is path[-2].
        let parent = self.path.iter().rev().nth(1).map(String::as_str);

        if !trimmed.is_empty() {
            if let Some(entry) = &mut self.current_entry {
                match name {
                    "title" => {
                        if entry.title.is_none() {
                            entry.title = Some(trimmed.to_string());
                        }
                    }
                    "link" => {
                        if entry.link.is_none() {
                            // Atom emits an empty <link/> with href; only RSS-style
                            // text content reaches this branch.
                            entry.link = Some(trimmed.to_string());
                        }
                    }
                    "pubDate" | "published" | "updated" => {
                        // RSS 2.0 uses `<pubDate>` (RFC 822); Atom uses
                        // `<published>` and `<updated>` (RFC 3339). Take
                        // whichever surfaces first.
                        if entry.date.is_none() {
                            entry.date = Some(trimmed.to_string());
                        }
                    }
                    "description" | "summary" | "content" => {
                        if entry.summary.is_none() {
                            let had_tags = trimmed.contains('<');
                            entry.summary = Some(strip_tags(trimmed));
                            if had_tags {
                                self.parsed.html_stripped_count += 1;
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                match name {
                    "title" => {
                        if matches!(parent, Some("channel") | Some("feed"))
                            && self.parsed.feed_title.is_none()
                        {
                            self.parsed.feed_title = Some(trimmed.to_string());
                        }
                    }
                    "subtitle" | "description" => {
                        // Atom `<subtitle>` and RSS channel `<description>`
                        // both populate the feed subtitle slot.
                        if matches!(parent, Some("channel") | Some("feed"))
                            && self.parsed.feed_subtitle.is_none()
                        {
                            self.parsed.feed_subtitle = Some(strip_tags(trimmed));
                        }
                    }
                    "link" => {
                        // RSS channel `<link>https://…</link>` (text content).
                        // Atom feed-level link handled in start/empty
                        // element via `atom_href` because the URL lives in
                        // attributes.
                        if matches!(parent, Some("channel")) && self.parsed.feed_link.is_none() {
                            self.parsed.feed_link = Some(trimmed.to_string());
                        }
                    }
                    "language" => {
                        if matches!(parent, Some("channel")) && self.parsed.feed_lang.is_none() {
                            self.parsed.feed_lang = Some(trimmed.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        let popped = self.path.pop();
        if popped.as_deref() == Some("item") || popped.as_deref() == Some("entry") {
            if let Some(entry) = self.current_entry.take() {
                if entry.title.is_some() || entry.link.is_some() || entry.summary.is_some() {
                    self.parsed.entries.push(entry);
                }
            }
        }
    }

    fn append_text(&mut self, text: &str) {
        self.pending_text.push_str(text);
    }

    fn set_entry_link(&mut self, url: String) {
        if let Some(entry) = &mut self.current_entry {
            if entry.link.is_none() {
                entry.link = Some(url);
            }
        }
    }

    fn into_parsed(self) -> Parsed {
        self.parsed
    }
}

fn local_name(qualified: &[u8]) -> String {
    let s = std::str::from_utf8(qualified).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s).to_string()
}

fn atom_href(element: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    for attr in element.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"href" {
            return attr
                .unescape_value()
                .ok()
                .map(|v| v.into_owned())
                .filter(|s| !s.is_empty());
        }
    }
    None
}

fn invalid_xml<E: std::fmt::Display>(err: E) -> EngineError {
    EngineError::InvalidContent(err.to_string())
}

/// Decide whether `input` is a JSON Feed rather than RSS/Atom XML. Prefers
/// the declared content type; an XML/RSS/Atom type short-circuits to the
/// walker. With no usable content type, sniff the body: a JSON Feed
/// document is an object, so the first non-whitespace byte is `{`.
fn looks_like_json_feed(input: &EngineInput) -> bool {
    if let Some(content_type) = &input.content_type {
        let lowered = content_type.to_ascii_lowercase();
        if lowered.contains("json") {
            return true;
        }
        if lowered.contains("xml") || lowered.contains("rss") || lowered.contains("atom") {
            return false;
        }
    }
    input.body.trim_start().starts_with('{')
}

/// JSON Feed 1.x top-level object. Only the fields nematic projects are
/// modelled; unknown keys are ignored (forward-compatible with 1.1+).
#[derive(Deserialize)]
struct JsonFeed {
    title: Option<String>,
    home_page_url: Option<String>,
    feed_url: Option<String>,
    description: Option<String>,
    language: Option<String>,
    #[serde(default)]
    items: Vec<JsonFeedItem>,
}

#[derive(Deserialize)]
struct JsonFeedItem {
    id: Option<String>,
    url: Option<String>,
    external_url: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    content_text: Option<String>,
    content_html: Option<String>,
    date_published: Option<String>,
    date_modified: Option<String>,
}

/// Parse a JSON Feed 1.x document into the same [`Parsed`] shape the XML
/// walker produces, so both flavours share [`build_document_blocks`].
fn parse_json(body: &str) -> Result<Parsed, EngineError> {
    let feed: JsonFeed = serde_json::from_str(body)
        .map_err(|err| EngineError::InvalidContent(format!("JSON Feed parse failed: {err}")))?;

    let mut parsed = Parsed {
        feed_title: trimmed_some(feed.title),
        feed_subtitle: trimmed_some(feed.description).map(|text| strip_tags(&text)),
        feed_link: trimmed_some(feed.home_page_url).or_else(|| trimmed_some(feed.feed_url)),
        feed_lang: trimmed_some(feed.language),
        ..Parsed::default()
    };

    for item in feed.items {
        let title = trimmed_some(item.title).or_else(|| trimmed_some(item.id));
        let link = trimmed_some(item.url).or_else(|| trimmed_some(item.external_url));
        let date = trimmed_some(item.date_published).or_else(|| trimmed_some(item.date_modified));

        // Mirror the XML path's summary handling: prefer an explicit
        // summary, then plain-text content, then HTML content; strip tags
        // and count the strip so the doc surfaces a DegradedRendering hint.
        let summary = trimmed_some(item.summary)
            .or_else(|| trimmed_some(item.content_text))
            .or_else(|| trimmed_some(item.content_html))
            .map(|raw| {
                if raw.contains('<') {
                    parsed.html_stripped_count += 1;
                    strip_tags(&raw)
                } else {
                    raw
                }
            });

        if title.is_some() || link.is_some() || summary.is_some() {
            parsed.entries.push(Entry {
                title,
                link,
                date,
                summary,
            });
        }
    }

    Ok(parsed)
}

/// Trim a JSON string field, dropping it if empty after trimming.
fn trimmed_some(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// Naively strip HTML tags from a fragment, preserving text content. Used
/// for v1 summary rendering where preserving tag structure isn't worth the
/// complexity. A future slice can replace this with a real HTML parse.
fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Collapse runs of whitespace introduced by tag stripping.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_ws = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                collapsed.push(' ');
            }
            prev_ws = true;
        } else {
            collapsed.push(ch);
            prev_ws = false;
        }
    }
    collapsed.trim().to_string()
}

/// Build the block list and any diagnostics from a parsed feed.
///
/// Emits one [`DocumentBlock::FeedHeader`] when the feed carries any
/// channel-level metadata, then one [`DocumentBlock::FeedEntry`] per item.
/// These semantic blocks preserve RSS / Atom intent (a feed entry is
/// distinct from "a paragraph with a link in it") so downstream
/// intelligence can match on type, not just text.
fn build_document_blocks(parsed: Parsed) -> (Vec<DocumentBlock>, Vec<DocumentDiagnostic>) {
    let mut blocks = Vec::with_capacity(parsed.entries.len() + 1);

    // Feed-level header — only emit if anything is populated beyond the
    // `EngineDocument.title` itself.
    let header_has_content =
        parsed.feed_title.is_some() || parsed.feed_subtitle.is_some() || parsed.feed_link.is_some();
    if header_has_content {
        blocks.push(DocumentBlock::FeedHeader {
            title: parsed.feed_title.clone().unwrap_or_default(),
            subtitle: parsed.feed_subtitle,
            summary: None,
            source_url: parsed.feed_link,
        });
    }

    for entry in parsed.entries {
        blocks.push(DocumentBlock::FeedEntry {
            title: entry.title.unwrap_or_default(),
            date: entry.date,
            summary: entry.summary,
            article_url: entry.link,
            source_url: None,
        });
    }

    let mut diagnostics = Vec::new();
    if parsed.html_stripped_count > 0 {
        let n = parsed.html_stripped_count;
        let entries = if n == 1 { "entry" } else { "entries" };
        diagnostics.push(DocumentDiagnostic::DegradedRendering(format!(
            "stripped HTML from {n} {entries}"
        )));
    }

    (blocks, diagnostics)
}

// Tests live in `feed/tests.rs` to keep this file under the 600-LOC ceiling.
#[cfg(test)]
mod tests;
