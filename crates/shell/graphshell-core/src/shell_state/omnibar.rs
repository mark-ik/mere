/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable omnibar session state.
//!
//! The omnibar — the toolbar's unified location-bar / search-provider
//! / graph-scoped search surface — owns a session bundle that lives on
//! `GraphshellRuntime`. Pre-M4 slice 5 this was coupled to
//! `crossbeam_channel::Receiver` (non-WASM). Slice 5 introduced
//! [`AsyncRequestState`](crate::async_request::AsyncRequestState) so the
//! session state is driven from a host-side `ProviderSuggestionDriver`.
//! Slice 5b (this module, 2026-04-22) replaces the remaining
//! `std::time::Instant` with [`PortableInstant`](crate::time::PortableInstant)
//! so the whole omnibar session can move to graphshell-core.
//!
//! The concrete receiver + generation tag still live in a shell-side
//! `ProviderSuggestionDriver`; this module defines only the portable
//! state the runtime owns and the host-neutral types that flow through
//! the request pipeline (search mode, provider kind, match shapes,
//! fetch outcomes).

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use crate::async_request::AsyncRequestState;
use crate::graph::NodeKey;
use crate::time::PortableInstant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmnibarSessionKind {
    Graph(OmnibarSearchMode),
    SearchProvider(SearchProviderKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SearchProviderKind {
    DuckDuckGo,
    Bing,
    Google,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmnibarSearchMode {
    Mixed,
    NodesLocal,
    NodesAll,
    TabsLocal,
    TabsAll,
    EdgesLocal,
    EdgesAll,
}

#[derive(Clone, Debug)]
pub struct HistoricalNodeMatch {
    pub url: String,
    pub display_label: Option<String>,
}

impl HistoricalNodeMatch {
    pub fn new(url: impl Into<String>, display_label: Option<String>) -> Self {
        Self {
            url: url.into(),
            display_label,
        }
    }

    pub fn without_label(url: impl Into<String>) -> Self {
        Self::new(url, None)
    }
}

impl PartialEq for HistoricalNodeMatch {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for HistoricalNodeMatch {}

impl Hash for HistoricalNodeMatch {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OmnibarMatch {
    Node(NodeKey),
    NodeUrl(HistoricalNodeMatch),
    SearchQuery {
        query: String,
        provider: SearchProviderKind,
    },
    Edge {
        from: NodeKey,
        to: NodeKey,
    },
    /// A durable graphlet peer of a warm node that is currently `Cold`
    /// (no live tile). Shown with ○ in the `TabsLocal` empty-query
    /// roster; activating opens a tile via graphlet routing.
    ColdGraphletMember(NodeKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderSuggestionStatus {
    Idle,
    Loading,
    Ready,
    Failed(ProviderSuggestionError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderSuggestionError {
    Network,
    HttpStatus(u16),
    Parse,
}

/// `Clone` is required by [`AsyncRequestState::arm_pending`](crate::async_request::AsyncRequestState::arm_pending)
/// which clones the prior state when transitioning. The outcome is
/// cheap to clone (a `Vec<OmnibarMatch>` + a small enum), and this
/// fires ~once per debounce window at the outer fetch path, so the
/// clone cost is negligible.
#[derive(Clone)]
pub struct ProviderSuggestionFetchOutcome {
    pub matches: Vec<OmnibarMatch>,
    pub status: ProviderSuggestionStatus,
}

/// Portable omnibar provider-suggestion mailbox.
///
/// No threading primitives inside — the concrete
/// `crossbeam_channel::Receiver<ProviderSuggestionFetchOutcome>` lives
/// in a shell-side `ProviderSuggestionDriver` that drains the channel
/// and calls `AsyncRequestState::resolve(generation, value)` on the
/// mailbox at frame boundaries (M4 slice 5).
///
/// Time fields use [`PortableInstant`] so the whole type is WASM-safe
/// (M4 slice 5b). Host supplies `PortableInstant` drawn from its
/// monotonic clock (desktop: `Instant::elapsed()`; wasm:
/// `performance.now()`).
///
/// Generation counter: each new provider-suggestion request bumps
/// `next_generation` via [`arm_new_request`] and arms the
/// [`AsyncRequestState`] to that generation. A value from a generation
/// that has since been superseded is rejected as stale.
///
/// [`arm_new_request`]: Self::arm_new_request
pub struct ProviderSuggestionMailbox {
    pub request_query: Option<String>,
    pub result: AsyncRequestState<ProviderSuggestionFetchOutcome>,
    /// Monotonic counter for [`arm_new_request`]. Private so callers
    /// use the bump-and-arm helper rather than hand-setting the field.
    next_generation: u64,
    pub debounce_deadline: Option<PortableInstant>,
    pub status: ProviderSuggestionStatus,
}

impl ProviderSuggestionMailbox {
    pub fn idle() -> Self {
        Self {
            request_query: None,
            result: AsyncRequestState::Idle,
            next_generation: 0,
            debounce_deadline: None,
            status: ProviderSuggestionStatus::Idle,
        }
    }

    pub fn debounced(request_query: String, debounce_deadline: PortableInstant) -> Self {
        Self {
            request_query: Some(request_query),
            result: AsyncRequestState::Idle,
            next_generation: 0,
            debounce_deadline: Some(debounce_deadline),
            status: ProviderSuggestionStatus::Loading,
        }
    }

    pub fn ready() -> Self {
        Self {
            status: ProviderSuggestionStatus::Ready,
            ..Self::idle()
        }
    }

    /// `true` when a provider request is armed or has landed a result
    /// the caller has not yet consumed. An `Interrupted` state is NOT
    /// considered pending (the request has terminated, even if
    /// unsuccessfully).
    pub fn has_pending_result(&self) -> bool {
        matches!(
            self.result,
            AsyncRequestState::Pending { .. } | AsyncRequestState::Ready { .. }
        )
    }

    /// Bump the internal generation counter, arm the portable state,
    /// and return the generation the host-side driver should attach to
    /// the receiver it is about to spawn. After this call the mailbox
    /// is in `AsyncRequestState::Pending { generation }`.
    pub fn arm_new_request(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1);
        self.result.arm_pending(self.next_generation);
        self.next_generation
    }

    pub fn clear_pending(&mut self) {
        self.request_query = None;
        self.result.clear();
        self.debounce_deadline = None;
    }
}

pub struct OmnibarSearchSession {
    pub kind: OmnibarSessionKind,
    pub query: String,
    pub matches: Vec<OmnibarMatch>,
    pub active_index: usize,
    pub selected_indices: HashSet<usize>,
    pub anchor_index: Option<usize>,
    pub provider_mailbox: ProviderSuggestionMailbox,
}

impl OmnibarSearchSession {
    pub fn new_graph(
        kind: OmnibarSearchMode,
        query: impl Into<String>,
        matches: Vec<OmnibarMatch>,
    ) -> Self {
        Self {
            kind: OmnibarSessionKind::Graph(kind),
            query: query.into(),
            matches,
            active_index: 0,
            selected_indices: HashSet::new(),
            anchor_index: None,
            provider_mailbox: ProviderSuggestionMailbox::idle(),
        }
    }

    pub fn new_search_provider(
        provider: SearchProviderKind,
        query: impl Into<String>,
        matches: Vec<OmnibarMatch>,
        provider_mailbox: ProviderSuggestionMailbox,
    ) -> Self {
        Self {
            kind: OmnibarSessionKind::SearchProvider(provider),
            query: query.into(),
            matches,
            active_index: 0,
            selected_indices: HashSet::new(),
            anchor_index: None,
            provider_mailbox,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_idle_is_not_pending() {
        let mailbox = ProviderSuggestionMailbox::idle();
        assert!(!mailbox.has_pending_result());
        assert!(mailbox.debounce_deadline.is_none());
        assert_eq!(mailbox.status, ProviderSuggestionStatus::Idle);
    }

    #[test]
    fn mailbox_debounced_stores_portable_deadline() {
        let now = PortableInstant(1_000);
        let deadline = now.saturating_add_ms(75);
        let mailbox = ProviderSuggestionMailbox::debounced("rust async".into(), deadline);
        assert_eq!(mailbox.debounce_deadline, Some(PortableInstant(1_075)));
        assert_eq!(mailbox.request_query.as_deref(), Some("rust async"));
        assert_eq!(mailbox.status, ProviderSuggestionStatus::Loading);
    }

    #[test]
    fn mailbox_arm_new_request_bumps_generation_monotonically() {
        let mut mailbox = ProviderSuggestionMailbox::idle();
        let gen1 = mailbox.arm_new_request();
        let gen2 = mailbox.arm_new_request();
        assert!(gen2 > gen1, "arm_new_request must be monotonic");
        assert!(mailbox.has_pending_result());
    }

    #[test]
    fn mailbox_clear_pending_resets_deadline_and_query() {
        let mut mailbox = ProviderSuggestionMailbox::debounced(
            "rust".into(),
            PortableInstant(1_000).saturating_add_ms(100),
        );
        mailbox.arm_new_request();

        mailbox.clear_pending();

        assert!(mailbox.request_query.is_none());
        assert!(mailbox.debounce_deadline.is_none());
        assert!(!mailbox.has_pending_result());
    }

    #[test]
    fn session_new_graph_starts_with_idle_mailbox() {
        let session = OmnibarSearchSession::new_graph(OmnibarSearchMode::Mixed, "rust", Vec::new());
        assert!(matches!(
            session.kind,
            OmnibarSessionKind::Graph(OmnibarSearchMode::Mixed)
        ));
        assert_eq!(session.query, "rust");
        assert_eq!(session.active_index, 0);
        assert!(session.selected_indices.is_empty());
        assert!(session.anchor_index.is_none());
        assert!(matches!(
            session.provider_mailbox.status,
            ProviderSuggestionStatus::Idle
        ));
    }

    #[test]
    fn session_new_search_provider_uses_supplied_mailbox() {
        let mailbox = ProviderSuggestionMailbox::debounced(
            "async".into(),
            PortableInstant(2_000).saturating_add_ms(50),
        );
        let session = OmnibarSearchSession::new_search_provider(
            SearchProviderKind::DuckDuckGo,
            "@d async",
            Vec::new(),
            mailbox,
        );
        assert!(matches!(
            session.kind,
            OmnibarSessionKind::SearchProvider(SearchProviderKind::DuckDuckGo)
        ));
        assert_eq!(session.query, "@d async");
        assert_eq!(
            session.provider_mailbox.debounce_deadline,
            Some(PortableInstant(2_050))
        );
    }

    #[test]
    fn historical_node_match_equality_uses_url_only() {
        // PartialEq + Hash ignore display_label — two entries with the
        // same URL but different labels are considered "the same match"
        // for dedup purposes. Pin the contract.
        let a = HistoricalNodeMatch::new("https://example.test", Some("Example".into()));
        let b = HistoricalNodeMatch::new("https://example.test", Some("Different".into()));
        assert_eq!(a, b);
        let c = HistoricalNodeMatch::without_label("https://other.test");
        assert_ne!(a, c);
    }

    #[test]
    fn omnibar_match_variants_can_coexist_in_hashset() {
        // OmnibarMatch is Hash + Eq so it can be used as a set element
        // for dedup in the match list. Pin that each variant distinguishes.
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(OmnibarMatch::Node(NodeKey::new(1)));
        set.insert(OmnibarMatch::Node(NodeKey::new(2)));
        set.insert(OmnibarMatch::NodeUrl(HistoricalNodeMatch::without_label(
            "https://a.test",
        )));
        set.insert(OmnibarMatch::SearchQuery {
            query: "rust".into(),
            provider: SearchProviderKind::DuckDuckGo,
        });
        set.insert(OmnibarMatch::ColdGraphletMember(NodeKey::new(3)));
        assert_eq!(set.len(), 5);

        // Duplicate insertion is a no-op.
        assert!(!set.insert(OmnibarMatch::Node(NodeKey::new(1))));
    }

    #[test]
    fn deadline_comparison_against_now_drives_fire_decision() {
        // End-to-end check of the debounce pattern using PortableInstant.
        let now = PortableInstant(1_000);
        let deadline = now.saturating_add_ms(100);
        let mailbox = ProviderSuggestionMailbox::debounced("rust".into(), deadline);

        // 50ms later: not fired.
        let fifty_ms = PortableInstant(1_050);
        assert!(!fifty_ms.has_reached(mailbox.debounce_deadline.unwrap()));

        // 150ms later: fired.
        let past_deadline = PortableInstant(1_150);
        assert!(past_deadline.has_reached(mailbox.debounce_deadline.unwrap()));
    }
}
