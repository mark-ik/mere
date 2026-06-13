/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Omnibar suggestions, sourced from meerkat's own [`History`] but expressed in
//! the **reused** [`OmnibarMatch`] vocabulary from the graphshell chrome domain.
//!
//! graphshell's omnibar generates matches by graph search (node / edge / tab
//! fuzzy matching) behind an async provider mailbox. meerkat has no graph and no
//! async provider, so it generates matches synchronously from its history plus a
//! direct-navigation and a web-search affordance — but it still produces the
//! domain's [`OmnibarMatch`] types, so the rendering is "the next host widget
//! over the chrome domain" rather than a parallel model.
//!
//! Only the two host-relevant variants are produced: [`OmnibarMatch::NodeUrl`]
//! (a history URL, or the typed text resolved to a URL) and
//! [`OmnibarMatch::SearchQuery`] (a web search for the raw text).

use std::collections::HashSet;

use chrome::omnibar::{HistoricalNodeMatch, OmnibarMatch, SearchProviderKind};

use crate::nav::{self, History, NavTarget};

/// How many history rows a suggestion list shows at most (before the trailing
/// search row).
const MAX_HISTORY_SUGGESTIONS: usize = 6;

/// The provider a free-text search suggestion routes to. DuckDuckGo for now;
/// becomes a user setting when omnibar provider selection lands.
const SEARCH_PROVIDER: SearchProviderKind = SearchProviderKind::DuckDuckGo;

/// Build the suggestion list for `query` against `history`.
///
/// Ordering: a direct-navigation row first when the text looks like a URL, then
/// history URLs containing the text (most-recent first, deduped, capped), then
/// always a web-search row for the raw text. Empty/blank text yields no
/// suggestions (the dropdown stays closed).
pub fn suggestions(query: &str, history: &History) -> Vec<OmnibarMatch> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    // Command mode (`>expr`) is not an address: offer no navigation / search rows
    // (it would read as "search the web for >roster"). Command-aware hints can
    // hang off this branch later.
    if let NavTarget::Command(_) = nav::classify(q) {
        return Vec::new();
    }
    let needle = q.to_lowercase();
    let mut out: Vec<OmnibarMatch> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Direct navigation, when the text resolves to a URL (not a search).
    if let NavTarget::Url(url) = nav::classify(q) {
        if seen.insert(url.clone()) {
            out.push(OmnibarMatch::NodeUrl(HistoricalNodeMatch::without_label(
                url,
            )));
        }
    }
    // History matches, most-recent first.
    for url in history.entries().iter().rev() {
        if out.len() >= MAX_HISTORY_SUGGESTIONS {
            break;
        }
        if url.to_lowercase().contains(&needle) && seen.insert(url.clone()) {
            out.push(OmnibarMatch::NodeUrl(HistoricalNodeMatch::without_label(
                url.clone(),
            )));
        }
    }
    // Always offer a web search for the raw text.
    out.push(OmnibarMatch::SearchQuery {
        query: q.to_string(),
        provider: SEARCH_PROVIDER,
    });
    out
}

/// The user-facing label for a suggestion row.
pub fn match_label(m: &OmnibarMatch) -> String {
    match m {
        OmnibarMatch::NodeUrl(h) => h.url.clone(),
        OmnibarMatch::SearchQuery { query, .. } => {
            format!("Search the web for \u{201c}{query}\u{201d}")
        }
        // meerkat never produces the graph-scoped variants.
        _ => String::new(),
    }
}

/// The URL a suggestion navigates to, if it is one meerkat produces.
pub fn resolve_match(m: &OmnibarMatch) -> Option<String> {
    match m {
        OmnibarMatch::NodeUrl(h) => Some(h.url.clone()),
        OmnibarMatch::SearchQuery { query, .. } => Some(NavTarget::Search(query.clone()).resolve()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(matches: &[OmnibarMatch]) -> Vec<String> {
        matches.iter().filter_map(resolve_match).collect()
    }

    #[test]
    fn blank_query_has_no_suggestions() {
        let h = History::new("mere://welcome");
        assert!(suggestions("", &h).is_empty());
        assert!(suggestions("   ", &h).is_empty());
    }

    #[test]
    fn url_like_query_offers_direct_navigation_first() {
        let h = History::new("mere://welcome");
        let s = suggestions("example.com", &h);
        // First row is the resolved direct-navigation URL.
        assert_eq!(resolve_match(&s[0]).as_deref(), Some("https://example.com"));
        // A trailing web-search row is always present.
        assert!(matches!(s.last(), Some(OmnibarMatch::SearchQuery { .. })));
    }

    #[test]
    fn history_substring_matches_are_offered() {
        let mut h = History::new("mere://welcome");
        h.visit("https://www.servo.org".into());
        h.visit("https://servo.org/blog".into());
        let s = suggestions("servo", &h);
        let found = urls(&s);
        assert!(found.iter().any(|u| u == "https://www.servo.org"));
        assert!(found.iter().any(|u| u == "https://servo.org/blog"));
    }

    #[test]
    fn direct_nav_and_history_do_not_duplicate() {
        let mut h = History::new("mere://welcome");
        h.visit("https://example.com".into());
        let s = suggestions("https://example.com", &h);
        let count = urls(&s)
            .iter()
            .filter(|u| *u == "https://example.com")
            .count();
        assert_eq!(count, 1, "the URL appears once across direct-nav + history");
    }

    #[test]
    fn free_text_offers_only_a_search() {
        let h = History::new("mere://welcome");
        let s = suggestions("rust async traits", &h);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0], OmnibarMatch::SearchQuery { .. }));
        assert_eq!(
            resolve_match(&s[0]).as_deref(),
            Some("https://duckduckgo.com/?q=rust+async+traits")
        );
    }

    #[test]
    fn search_label_quotes_the_query() {
        let m = OmnibarMatch::SearchQuery {
            query: "servo".into(),
            provider: SEARCH_PROVIDER,
        };
        assert_eq!(match_label(&m), "Search the web for \u{201c}servo\u{201d}");
    }
}
