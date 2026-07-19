// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Omnibar suggestions, sourced from chrome-owned [`History`] but expressed in
//! the **reused** [`OmnibarMatch`] vocabulary from the graphshell chrome domain.
//!
//! graphshell's omnibar generates matches by graph search (node / edge / tab
//! fuzzy matching) behind an async provider mailbox. A host may have no graph and no
//! async provider, so it generates matches synchronously from its history plus a
//! direct-navigation and a web-search affordance — but it still produces the
//! domain's [`OmnibarMatch`] types, so the rendering is "the next host widget
//! over the chrome domain" rather than a parallel model.
//!
//! Only the two host-relevant variants are produced: [`OmnibarMatch::NodeUrl`]
//! (a history URL, or the typed text resolved to a URL) and
//! [`OmnibarMatch::SearchQuery`] (a web search for the raw text).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::omnibar::{HistoricalNodeMatch, OmnibarMatch, SearchProviderKind};

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
/// history URLs that contain the text, ranked best-first — host matches (prefix
/// or substring, a leading `www.` ignored) outrank incidental path/query
/// matches, and within a tier the most *frecent* URL leads (visit frequency in
/// the back/forward path, with recency breaking ties). A web-search row for the
/// raw text is always last. Empty/blank text yields no suggestions (the dropdown
/// stays closed).
///
/// Title- and favicon-aware ranking await [`History`] carrying that per-visit
/// metadata — it is a linear back/forward URL stack today, so `display_label`
/// on the emitted matches is left unset and ranking sees only URLs.
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

    // Direct navigation, pinned first when the text resolves to a URL — the
    // literal thing the user typed is the strongest intent signal.
    if let NavTarget::Url(url) = nav::classify(q) {
        if seen.insert(url.clone()) {
            out.push(OmnibarMatch::NodeUrl(HistoricalNodeMatch::without_label(
                url,
            )));
        }
    }
    // History matches, frecency-ranked within match-quality tiers.
    for url in ranked_history(&needle, history) {
        if out.len() >= MAX_HISTORY_SUGGESTIONS {
            break;
        }
        if seen.insert(url.clone()) {
            out.push(OmnibarMatch::NodeUrl(HistoricalNodeMatch::without_label(
                url,
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

/// A history URL scored for the suggestion ordering.
struct Ranked {
    url: String,
    /// Match-quality tier — where the needle hits, higher is better. Host
    /// matches (4 exact, 3 prefix, 2 substring) outrank a path/query-only match
    /// (1). The primary sort key, so a host match never loses to an incidental
    /// path hit, however frecent the latter.
    tier: u32,
    /// Visit frequency (occurrences in the back/forward path) with recency
    /// folded in as a sub-unit tie-breaker, so equal-frequency URLs order
    /// newest-first. The secondary sort key, within a tier.
    frecency: f64,
}

/// History URLs containing `needle` (already lowercased), ordered best-first by
/// `(tier, frecency)`. Returns unique URLs: the back/forward path can list the
/// same URL more than once, but here it appears once with its visits tallied.
fn ranked_history(needle: &str, history: &History) -> Vec<String> {
    let entries = history.entries();
    // Recency is the entry's index normalized to `0..=1` (1 = newest). Nothing
    // to rank with a single entry; guard the divisor against zero regardless.
    let denom = entries.len().saturating_sub(1).max(1) as f64;

    // One pass: per unique URL, tally frequency and keep its newest index.
    let mut by_url: HashMap<&str, (u32, usize)> = HashMap::new();
    for (i, url) in entries.iter().enumerate() {
        let slot = by_url.entry(url.as_str()).or_insert((0, i));
        slot.0 += 1;
        slot.1 = i; // entries run oldest-first, so the last write is the newest
    }

    let mut ranked: Vec<Ranked> = by_url
        .into_iter()
        .filter(|(url, _)| url.to_lowercase().contains(needle))
        .map(|(url, (freq, newest))| Ranked {
            url: url.to_string(),
            tier: match_tier(url, needle),
            frecency: freq as f64 + (newest as f64 / denom) * 0.5,
        })
        .collect();

    // Tier first, then frecency, then the URL itself for a deterministic order.
    ranked.sort_by(|a, b| {
        b.tier
            .cmp(&a.tier)
            .then_with(|| {
                b.frecency
                    .partial_cmp(&a.frecency)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| a.url.cmp(&b.url))
    });
    ranked.into_iter().map(|r| r.url).collect()
}

/// The match-quality tier for `url` against `needle` (already lowercased): how
/// directly the needle hits the host. A leading `www.` is ignored, so
/// `www.servo.org` ranks like `servo.org`. Tier 1 means the match is only in the
/// path/query (the caller passes URLs already known to contain `needle`).
fn match_tier(url: &str, needle: &str) -> u32 {
    let host = host_of(url).unwrap_or_default().to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(host.as_str());
    if host == needle {
        4
    } else if host.starts_with(needle) {
        3
    } else if host.contains(needle) {
        2
    } else {
        1
    }
}

/// The host of `url`, if it parses and carries one. Non-standard schemes with an
/// authority resolve (`mere://welcome` → `welcome`); authority-less ones
/// (`about:blank`) yield `None`.
fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_owned)
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

    #[test]
    fn frequency_outranks_a_single_visit_within_a_tier() {
        let mut h = History::new("mere://welcome");
        h.visit("https://a.example".into());
        h.visit("https://b.example".into());
        h.visit("https://a.example".into()); // a.example visited twice
        let found = urls(&suggestions("example", &h));
        let ia = found.iter().position(|u| u == "https://a.example").unwrap();
        let ib = found.iter().position(|u| u == "https://b.example").unwrap();
        assert!(ia < ib, "more-frequent URL ranks first: {found:?}");
    }

    #[test]
    fn host_match_outranks_a_more_recent_path_match() {
        let mut h = History::new("mere://welcome");
        h.visit("https://example.com".into()); // host-prefix match (older)
        h.visit("https://news.test/about-example".into()); // path-only match (newer)
        let found = urls(&suggestions("example", &h));
        let host = found
            .iter()
            .position(|u| u == "https://example.com")
            .unwrap();
        let path = found
            .iter()
            .position(|u| u == "https://news.test/about-example")
            .unwrap();
        assert!(
            host < path,
            "a host match leads despite being older: {found:?}"
        );
    }

    #[test]
    fn www_prefix_is_ignored_for_host_ranking() {
        let mut h = History::new("mere://welcome");
        h.visit("https://www.servo.org".into()); // host match via www-strip (older)
        h.visit("https://docs.test/intro-to-servo".into()); // path-only match (newer)
        let found = urls(&suggestions("servo", &h));
        let host = found
            .iter()
            .position(|u| u == "https://www.servo.org")
            .unwrap();
        let path = found
            .iter()
            .position(|u| u == "https://docs.test/intro-to-servo")
            .unwrap();
        assert!(
            host < path,
            "www.servo.org ranks as a host match: {found:?}"
        );
    }

    #[test]
    fn recency_breaks_ties_within_a_tier() {
        let mut h = History::new("mere://welcome");
        h.visit("https://old.example".into());
        h.visit("https://new.example".into()); // same tier + frequency, newer
        let found = urls(&suggestions("example", &h));
        // Of two equal-tier, equal-frequency matches, the newer visit leads.
        assert_eq!(
            found.first().map(String::as_str),
            Some("https://new.example")
        );
    }
}
