/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Edge taxonomy types — families, kinds, and sub-kinds.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel-mod
//! decomposition pass. Per-family runtime data structs (Traversal,
//! EdgeMetrics, *Data types, predicate IRI mapping) live in `edge_data.rs`.
//! [`EdgePayload`](super::EdgePayload) in `edge_payload.rs` is the primary
//! consumer; `Graph` methods in `mod.rs` reach for these types via re-exports.

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
    /// A verbatim duplicate of the source — the cross-graph copy/tear-out
    /// "fork" relation (tear-out brief §7.5). Unlike the content-transformation
    /// sub-kinds above, the copy carries the source's content unchanged; only
    /// its identity and host graph differ. Appended last so existing ordinals
    /// (and the positional `tag` encoding) stay stable.
    CopiedFrom,
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
    pub(crate) fn contributes_to_forward_count(self) -> bool {
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

