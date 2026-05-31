/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Edge taxonomy types — families, kinds, sub-kinds, and per-family
//! data structs.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel-mod
//! decomposition pass (memory: feedback_mere_file_size_ceiling). The
//! [`EdgePayload`](super::EdgePayload) impl in `edge_payload.rs` is
//! the primary consumer; `Graph` methods in `mod.rs` reach for these
//! types via the parent module re-exports.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rkyv::{Archive, Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum EdgeFamily {
    Semantic,
    Traversal,
    Containment,
    Arrangement,
    Imported,
    Provenance,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
#[derive(strum::EnumIter, strum::FromRepr)]
pub enum SemanticSubKind {
    Hyperlink,
    UserGrouped,
    AgentDerived,
    Cites,
    Quotes,
    Summarizes,
    Elaborates,
    ExampleOf,
    Supports,
    Contradicts,
    Questions,
    SameEntityAs,
    DuplicateOf,
    CanonicalMirrorOf,
    DependsOn,
    Blocks,
    NextStep,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
#[derive(strum::EnumIter, strum::FromRepr)]
pub enum ArrangementSubKind {
    FrameMember,
    TileGroup,
    SplitPair,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
#[derive(strum::EnumIter, strum::FromRepr)]
pub enum ContainmentSubKind {
    UrlPath,
    Domain,
    FileSystem,
    UserFolder,
    ClipSource,
    NotebookSection,
    CollectionMember,
}

impl ContainmentSubKind {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::UrlPath => "url-path",
            Self::Domain => "domain",
            Self::FileSystem => "filesystem",
            Self::UserFolder => "user-folder",
            Self::ClipSource => "clip-source",
            Self::NotebookSection => "notebook-section",
            Self::CollectionMember => "collection-member",
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
#[derive(strum::EnumIter, strum::FromRepr)]
pub enum ImportedSubKind {
    BookmarkFolder,
    HistoryImport,
    SessionImport,
    RssMembership,
    FileSystemImport,
    ArchiveMembership,
    SharedCollection,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(compare(PartialEq, PartialOrd), derive(PartialEq, Eq, PartialOrd, Ord))]
#[derive(strum::EnumIter, strum::FromRepr)]
pub enum ProvenanceSubKind {
    ClippedFrom,
    ExcerptedFrom,
    SummarizedFrom,
    TranslatedFrom,
    RewrittenFrom,
    GeneratedFrom,
    ExtractedFrom,
    ImportedFromSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum RelationDurability {
    Durable,
    Session,
}

impl RelationDurability {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::Durable => "durable",
            Self::Session => "session",
        }
    }
}

impl ArrangementSubKind {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::FrameMember => "frame-member",
            Self::TileGroup => "tile-group",
            Self::SplitPair => "split-pair",
        }
    }

    pub fn durability(self) -> RelationDurability {
        match self {
            Self::FrameMember => RelationDurability::Durable,
            Self::TileGroup | Self::SplitPair => RelationDurability::Session,
        }
    }

    pub fn provenance(self) -> &'static str {
        match self {
            Self::FrameMember => "workbench.frame_snapshot",
            Self::TileGroup => "workbench.tile_grouping",
            Self::SplitPair => "workbench.split_pairing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub enum EdgeAssertion {
    Semantic {
        sub_kind: SemanticSubKind,
        label: Option<String>,
        decay_progress: Option<f32>,
    },
    Containment {
        sub_kind: ContainmentSubKind,
    },
    Arrangement {
        sub_kind: ArrangementSubKind,
    },
    Imported {
        sub_kind: ImportedSubKind,
    },
    Provenance {
        sub_kind: ProvenanceSubKind,
    },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum RelationSelector {
    Family(EdgeFamily),
    Semantic(SemanticSubKind),
    Containment(ContainmentSubKind),
    Arrangement(ArrangementSubKind),
    Imported(ImportedSubKind),
    Provenance(ProvenanceSubKind),
}

/// Trigger classification for a traversal event.
///
/// Expanded per the 2026-05-11 relation-taxonomy-and-edge-mutation
/// plan §2: `Traversal` stays the one family without sub-kinds;
/// temporal nuance instead lives here in the trigger vocabulary.
/// Five additions to cover the cases the prior plan called out:
/// HTTP / meta redirects, session-restore navigations, in-document
/// fragment jumps, find-in-page-style jumps, and imported-history
/// hypotheses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub enum NavigationTrigger {
    Unknown,
    LinkClick,
    Back,
    Forward,
    AddressBarEntry,
    PanePromotion,
    Programmatic,
    /// Server redirect (3xx) or meta-refresh — the user didn't
    /// directly initiate this hop.
    Redirect,
    /// Navigation re-issued by a session restore (window reopen,
    /// "restore previous tabs," persisted-session rehydration).
    ReopenSession,
    /// In-document anchor jump (fragment / `#section`) — same
    /// document, scroll destination changes.
    JumpAnchor,
    /// "Find in page" or analogous within-document search jumps
    /// that surface as a navigation event.
    InPageSearchJump,
    /// Imported from another browser's history database. Distinct
    /// from `Unknown` so importers can tag confidence-bounded
    /// traversal hypotheses without conflating them with native
    /// user navigation.
    ImportedHistory,
}

impl NavigationTrigger {
    #[allow(dead_code)]
    fn contributes_to_forward_count(self) -> bool {
        !matches!(self, Self::Back)
    }
}

/// Canonical read-side classifier for an edge's relation. Combines
/// family + sub-kind into one enum that callers reaching for "what
/// kind of relation is this?" can match on without touching the
/// write-side `EdgeAssertion` payload.
///
/// Per the 2026-05-11 relation-taxonomy plan §3: `RelationKind`
/// is the discriminant shape (`RelationKind ≈
/// discriminant(EdgeAssertion)`), used by canvas hit-tests, render
/// policy, filter UIs, action-target inference, and view-local
/// hide keys. `EdgeAssertion` remains the write contract — it
/// carries construction-time payload (labels, decay state, etc.)
/// that read sites don't need.
///
/// `Traversal` is the one variant without a sub-kind: traversal
/// is event-shaped, with temporal nuance in [`NavigationTrigger`]
/// rather than a `TraversalSubKind` enum.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum RelationKind {
    Semantic(SemanticSubKind),
    Traversal,
    Containment(ContainmentSubKind),
    Arrangement(ArrangementSubKind),
    Imported(ImportedSubKind),
    Provenance(ProvenanceSubKind),
}

impl RelationKind {
    /// Project to the relation's family. Pure function — no payload
    /// access required.
    pub fn family(self) -> EdgeFamily {
        match self {
            RelationKind::Semantic(_) => EdgeFamily::Semantic,
            RelationKind::Traversal => EdgeFamily::Traversal,
            RelationKind::Containment(_) => EdgeFamily::Containment,
            RelationKind::Arrangement(_) => EdgeFamily::Arrangement,
            RelationKind::Imported(_) => EdgeFamily::Imported,
            RelationKind::Provenance(_) => EdgeFamily::Provenance,
        }
    }

    /// Encode this relation as an opaque `u32` tag for transport
    /// through layers that can't depend on `kernel` (e.g.
    /// `graph-canvas::CanvasEdge::tag` / `HitProxy::Edge::tag`).
    /// The top byte is the family ordinal (0..5); the bottom three
    /// bytes are the sub-kind ordinal within the family.
    /// [`Self::from_tag`] is the inverse.
    pub fn tag(self) -> u32 {
        let (family, sub) = match self {
            RelationKind::Semantic(sk) => (0u32, sk as u32),
            RelationKind::Traversal => (1, 0),
            RelationKind::Containment(sk) => (2, sk as u32),
            RelationKind::Arrangement(sk) => (3, sk as u32),
            RelationKind::Imported(sk) => (4, sk as u32),
            RelationKind::Provenance(sk) => (5, sk as u32),
        };
        (family << 24) | (sub & 0x00ff_ffff)
    }

    /// Decode a `u32` tag produced by [`Self::tag`]. Returns `None`
    /// for tags that don't correspond to a known relation (unknown
    /// family byte or sub-kind ordinal out of range).
    pub fn from_tag(tag: u32) -> Option<Self> {
        let family = tag >> 24;
        let sub = tag & 0x00ff_ffff;
        match family {
            0 => SemanticSubKind::from_repr(sub as usize).map(RelationKind::Semantic),
            1 => (sub == 0).then_some(RelationKind::Traversal),
            2 => ContainmentSubKind::from_repr(sub as usize).map(RelationKind::Containment),
            3 => ArrangementSubKind::from_repr(sub as usize).map(RelationKind::Arrangement),
            4 => ImportedSubKind::from_repr(sub as usize).map(RelationKind::Imported),
            5 => ProvenanceSubKind::from_repr(sub as usize).map(RelationKind::Provenance),
            _ => None,
        }
    }
}

// The u32 transport tag's ordinal mapping (see `RelationKind::tag` /
// `from_tag`) is derived, not hand-maintained: the forward direction is the
// field-less-enum cast `sk as u32`, the reverse is `strum::FromRepr`'s
// `from_repr`, both keyed by declaration order. The hand-written
// `*_sub_ordinal` / `*_from_ordinal` mirror tables that used to live here are
// gone, so adding a sub-kind no longer means editing parallel match arms.

#[cfg(test)]
mod relation_tag_tests {
    use super::*;
    use strum::IntoEnumIterator;

    fn all_kinds() -> Vec<RelationKind> {
        // Driven by `EnumIter`, so a new sub-kind is covered automatically with
        // no parallel list to maintain. `Traversal` is the one family with no
        // sub-kind.
        let mut out = Vec::new();
        out.extend(SemanticSubKind::iter().map(RelationKind::Semantic));
        out.push(RelationKind::Traversal);
        out.extend(ContainmentSubKind::iter().map(RelationKind::Containment));
        out.extend(ArrangementSubKind::iter().map(RelationKind::Arrangement));
        out.extend(ImportedSubKind::iter().map(RelationKind::Imported));
        out.extend(ProvenanceSubKind::iter().map(RelationKind::Provenance));
        out
    }

    #[test]
    fn tag_round_trips_for_every_relation_kind() {
        for kind in all_kinds() {
            let tag = kind.tag();
            let decoded = RelationKind::from_tag(tag);
            assert_eq!(decoded, Some(kind), "tag {tag:#x} for {kind:?}");
        }
    }

    #[test]
    fn tag_is_unique_per_kind() {
        let kinds = all_kinds();
        let tags: std::collections::HashSet<u32> = kinds.iter().map(|k| k.tag()).collect();
        assert_eq!(tags.len(), kinds.len(), "every kind must have a unique tag");
    }

    #[test]
    fn from_tag_rejects_unknown_family() {
        // Family byte 0xff (255) — no family at that ordinal.
        assert!(RelationKind::from_tag(0xff_00_00_00).is_none());
    }

    #[test]
    fn from_tag_rejects_unknown_sub_kind() {
        // Semantic family (0), sub ordinal 99 — out of range.
        assert!(RelationKind::from_tag(99).is_none());
    }
}

/// A temporal traversal event recorded on an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct Traversal {
    pub timestamp_ms: u64,
    pub trigger: NavigationTrigger,
}

impl Traversal {
    pub fn now(trigger: NavigationTrigger) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            timestamp_ms,
            trigger,
        }
    }
}

/// Durable traversal aggregates retained even when rolling-window records are evicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
pub struct EdgeMetrics {
    pub total_navigations: u64,
    pub forward_navigations: u64,
    pub backward_navigations: u64,
    pub last_navigated_at: Option<u64>,
}

impl EdgeMetrics {
    fn new() -> Self {
        Self {
            total_navigations: 0,
            forward_navigations: 0,
            backward_navigations: 0,
            last_navigated_at: None,
        }
    }

    fn record(&mut self, traversal: Traversal) {
        self.total_navigations = self.total_navigations.saturating_add(1);
        if traversal.trigger.contributes_to_forward_count() {
            self.forward_navigations = self.forward_navigations.saturating_add(1);
        } else {
            self.backward_navigations = self.backward_navigations.saturating_add(1);
        }
        self.last_navigated_at = Some(traversal.timestamp_ms);
    }
}

impl Default for EdgeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct SemanticData {
    pub sub_kinds: BTreeSet<SemanticSubKind>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct TraversalData {
    pub traversals: Vec<Traversal>,
    pub metrics: EdgeMetrics,
}

impl TraversalData {
    pub(crate) fn push(&mut self, traversal: Traversal) {
        self.metrics.record(traversal);
        self.traversals.push(traversal);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ArrangementData {
    pub sub_kinds: BTreeSet<ArrangementSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ContainmentData {
    pub sub_kinds: BTreeSet<ContainmentSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ImportedData {
    pub sub_kinds: BTreeSet<ImportedSubKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize, Default)]
pub struct ProvenanceData {
    pub sub_kinds: BTreeSet<ProvenanceSubKind>,
}

impl ContainmentData {
    pub(crate) fn insert(&mut self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.insert(sub_kind)
    }

    pub(crate) fn remove(&mut self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.remove(&sub_kind)
    }

    pub(crate) fn contains(&self, sub_kind: ContainmentSubKind) -> bool {
        self.sub_kinds.contains(&sub_kind)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sub_kinds.is_empty()
    }
}

impl ArrangementData {
    pub(crate) fn insert(&mut self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.insert(sub_kind)
    }

    pub(crate) fn remove(&mut self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.remove(&sub_kind)
    }

    pub(crate) fn contains(&self, sub_kind: ArrangementSubKind) -> bool {
        self.sub_kinds.contains(&sub_kind)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sub_kinds.is_empty()
    }

    pub(crate) fn has_durable_relation(&self) -> bool {
        self.sub_kinds
            .iter()
            .copied()
            .any(|sub_kind| sub_kind.durability() == RelationDurability::Durable)
    }

    pub(crate) fn has_session_relation(&self) -> bool {
        self.sub_kinds
            .iter()
            .copied()
            .any(|sub_kind| sub_kind.durability() == RelationDurability::Session)
    }
}
