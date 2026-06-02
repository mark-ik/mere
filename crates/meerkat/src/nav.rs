/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat's navigation model: location-bar text classification plus a
//! linear back/forward history.
//!
//! The reused `chrome` domain projects `can_go_back` / `can_go_forward`
//! from a content viewer's *own* history (servo's, in graphshell). meerkat
//! has no such viewer, so it owns this small linear history directly and
//! feeds those flags back into the reused [`ToolbarState`](chrome::toolbar::ToolbarState).
//!
//! URL handling here is deliberately string-light: prefix a scheme onto a
//! bare host, route everything unrecognizable to search. The content engine
//! (the content-root slice) canonicalizes the resolved URL on load — meerkat
//! does not try to be a URL parser.

/// What a piece of typed location-bar text resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// Text already usable as a URL — it carried a scheme, or looked like a
    /// bare host that we prefix with `https://`.
    Url(String),
    /// Free text with no usable scheme or host: a web search.
    Search(String),
}

impl NavTarget {
    /// The absolute URL to actually load. [`Url`](Self::Url) passes through;
    /// [`Search`](Self::Search) becomes a provider query URL (DuckDuckGo for
    /// now — the provider becomes configurable when the omnibar session lands).
    pub fn resolve(&self) -> String {
        match self {
            NavTarget::Url(u) => u.clone(),
            NavTarget::Search(q) => format!("https://duckduckgo.com/?q={}", encode_query(q)),
        }
    }
}

/// Classify location-bar text into a [`NavTarget`].
///
/// - Leading / trailing whitespace is trimmed.
/// - Anything containing `://`, or an authority-less scheme we recognize
///   (`about:`, `data:`, `mailto:`), is taken as a URL verbatim.
/// - Otherwise a single whitespace-free token whose authority is `localhost`
///   or carries a dotted host (`example.com`, `sub.example.co.uk/path`) gets
///   an `https://` prefix.
/// - Everything else is a search query.
pub fn classify(input: &str) -> NavTarget {
    let s = input.trim();
    if s.is_empty() {
        return NavTarget::Search(String::new());
    }
    if is_verbatim_url(s) {
        return NavTarget::Url(s.to_string());
    }
    if looks_like_host(s) {
        return NavTarget::Url(format!("https://{s}"));
    }
    NavTarget::Search(s.to_string())
}

/// URLs we pass through untouched: anything with an explicit authority
/// (`scheme://…`) and the authority-less schemes a user might type directly.
fn is_verbatim_url(s: &str) -> bool {
    s.contains("://")
        || s.starts_with("about:")
        || s.starts_with("data:")
        || s.starts_with("mailto:")
}

/// Heuristic "this is a host, not a search": a single whitespace-free token
/// whose authority (text before the first `/`, host part before any `:port`)
/// is `localhost` or a dotted name with non-empty labels.
fn looks_like_host(s: &str) -> bool {
    if s.split_whitespace().count() != 1 {
        return false;
    }
    let authority = s.split('/').next().unwrap_or(s);
    let host = authority.split(':').next().unwrap_or(authority);
    host == "localhost" || (host.contains('.') && !host.starts_with('.') && !host.ends_with('.'))
}

/// Percent-encode a search query's bytes, keeping the RFC 3986 unreserved set
/// (`ALPHA` / `DIGIT` / `-` `.` `_` `~`) and mapping spaces to `+` (the form
/// convention every search provider accepts).
fn encode_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A linear back/forward history of resolved URLs.
///
/// Always holds at least one entry, so [`current`](Self::current) never
/// panics. A fresh [`visit`](Self::visit) truncates any forward entries (the
/// classic browser "new branch discards the forward stack" behavior); a repeat
/// of the current entry is treated as a reload and does not grow the stack.
#[derive(Debug, Clone)]
pub struct History {
    entries: Vec<String>,
    cursor: usize,
}

impl History {
    /// A history seeded with one entry (the cold-start location).
    pub fn new(initial: impl Into<String>) -> Self {
        Self {
            entries: vec![initial.into()],
            cursor: 0,
        }
    }

    /// The current entry — the URL the content root should show.
    pub fn current(&self) -> &str {
        &self.entries[self.cursor]
    }

    /// All visited URLs, oldest first. Used to source omnibar suggestions.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Whether a back step is possible.
    pub fn can_back(&self) -> bool {
        self.cursor > 0
    }

    /// Whether a forward step is possible.
    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    /// Navigate to `url`. A repeat of the current entry is a reload (no
    /// growth); otherwise forward entries are discarded and `url` pushed as the
    /// new current.
    pub fn visit(&mut self, url: String) {
        if self.entries[self.cursor] == url {
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(url);
        self.cursor = self.entries.len() - 1;
    }

    /// Step back, returning the new current entry (or `None` at the oldest).
    pub fn back(&mut self) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(&self.entries[self.cursor])
    }

    /// Step forward, returning the new current entry (or `None` at the newest).
    pub fn forward(&mut self) -> Option<&str> {
        if self.cursor + 1 >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        Some(&self.entries[self.cursor])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_passes_through_scheme_urls() {
        assert_eq!(
            classify("https://example.com/x"),
            NavTarget::Url("https://example.com/x".into())
        );
        assert_eq!(
            classify("mere://welcome"),
            NavTarget::Url("mere://welcome".into())
        );
        assert_eq!(classify("about:blank"), NavTarget::Url("about:blank".into()));
    }

    #[test]
    fn classify_prefixes_bare_hosts() {
        assert_eq!(
            classify("example.com"),
            NavTarget::Url("https://example.com".into())
        );
        assert_eq!(
            classify("sub.example.co.uk/path?q=1"),
            NavTarget::Url("https://sub.example.co.uk/path?q=1".into())
        );
        assert_eq!(
            classify("localhost:8080"),
            NavTarget::Url("https://localhost:8080".into())
        );
    }

    #[test]
    fn classify_treats_free_text_as_search() {
        assert_eq!(classify("hello world"), NavTarget::Search("hello world".into()));
        assert_eq!(classify("rustlang"), NavTarget::Search("rustlang".into()));
        // Whitespace anywhere disqualifies the host heuristic.
        assert_eq!(
            classify("example .com"),
            NavTarget::Search("example .com".into())
        );
    }

    #[test]
    fn search_resolves_to_encoded_provider_query() {
        let url = NavTarget::Search("a b&c".into()).resolve();
        assert_eq!(url, "https://duckduckgo.com/?q=a+b%26c");
    }

    #[test]
    fn url_resolves_to_itself() {
        assert_eq!(
            NavTarget::Url("https://example.com".into()).resolve(),
            "https://example.com"
        );
    }

    #[test]
    fn history_starts_with_one_entry_and_no_steps() {
        let h = History::new("mere://welcome");
        assert_eq!(h.current(), "mere://welcome");
        assert!(!h.can_back());
        assert!(!h.can_forward());
    }

    #[test]
    fn history_visit_advances_and_enables_back() {
        let mut h = History::new("a");
        h.visit("b".into());
        assert_eq!(h.current(), "b");
        assert!(h.can_back());
        assert!(!h.can_forward());
    }

    #[test]
    fn history_back_forward_round_trip() {
        let mut h = History::new("a");
        h.visit("b".into());
        assert_eq!(h.back(), Some("a"));
        assert!(!h.can_back());
        assert!(h.can_forward());
        assert_eq!(h.forward(), Some("b"));
        assert!(h.can_back());
        assert!(!h.can_forward());
        // Stepping past either end is a no-op.
        assert_eq!(h.forward(), None);
        h.back();
        assert_eq!(h.back(), None);
    }

    #[test]
    fn history_visit_truncates_forward_branch() {
        let mut h = History::new("a");
        h.visit("b".into());
        h.visit("c".into());
        h.back(); // at "b", forward = ["c"]
        h.visit("d".into()); // discards "c"
        assert_eq!(h.current(), "d");
        assert!(h.can_back());
        assert!(!h.can_forward());
        h.back();
        assert_eq!(h.current(), "b");
    }

    #[test]
    fn history_reload_does_not_grow_stack() {
        let mut h = History::new("a");
        h.visit("a".into());
        assert!(!h.can_back());
        assert_eq!(h.current(), "a");
    }
}
