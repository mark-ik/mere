// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portable leaf types used by the graph model and persistence schema.
//!
//! These types are shared between `kernel` modules and must be
//! WASM-clean: no platform I/O, no UI framework dependencies.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rkyv::{Archive, Deserialize, Serialize};

use crate::graph::ProvenanceSubKind;

// ---------------------------------------------------------------------------
// Frame layout types
// ---------------------------------------------------------------------------

/// Durable member reference used by frame layout hints.
///
/// `NodeKey` is process-local and not stable across restart, so persistent frame
/// layout metadata uses the member node's stable UUID string instead.
pub type FrameLayoutNodeId = String;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum SplitOrientation {
    Vertical,
    Horizontal,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum DominantEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum GraphScope {
    #[default]
    Default,
    Source,
    User,
    Agent,
    Moot,
    Custom(String),
}

static NEXT_LOCAL_STATEMENT_NONCE: AtomicU64 = AtomicU64::new(1);
static STATEMENT_MINTER_SALT: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

/// Seed the statement-id minter's per-process salt. The kernel's identity
/// doctrine: OS randomness is native-only, wasm hosts supply theirs — a wasm
/// host should call this once at boot with 64 host-random bits (unseeded wasm
/// falls back to `0`, i.e. the pre-salt time+nonce behaviour, and loses the
/// cross-device guarantee). Native self-seeds from `Uuid::new_v4()` on first
/// mint; a later seed call is a no-op.
pub fn seed_statement_minter(salt: u64) {
    let _ = STATEMENT_MINTER_SALT.set(salt);
}

fn statement_minter_salt() -> u64 {
    *STATEMENT_MINTER_SALT.get_or_init(|| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let bytes = uuid::Uuid::new_v4();
            u64::from_le_bytes(bytes.as_bytes()[..8].try_into().expect("8 bytes"))
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    })
}

/// Mint a [`SemanticStatement`](crate::graph::SemanticStatement) id — the
/// FEDERATION-SAFE story the petgraph-RDF plan's Phase 1 required (the fact
/// handle reification, precise retract, snapshot migration, and federation
/// tombstones point at):
///
/// `{unix_ms:012x}-{process_salt:016x}-{counter:016x}`
///
/// - Device-safe: two devices collide only on a 64-bit salt collision
///   (2^-64 per process pair), independent of clocks and counters.
/// - Time-sortable by prefix (12 hex ms digits reach year ~10889).
/// - Opaque to consumers: dedup happens on statement CONTENT, never by
///   parsing this id, so legacy `{ts:016x}-{nonce:016x}` ids already in
///   snapshots remain valid handles forever.
pub(crate) fn mint_local_statement_id() -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let salt = statement_minter_salt();
    let nonce = NEXT_LOCAL_STATEMENT_NONCE.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp_ms:012x}-{salt:016x}-{nonce:016x}")
}

#[cfg(test)]
mod minting_tests {
    use super::*;

    /// The minted id is device-safe in shape: time prefix + a non-zero
    /// process salt (native self-seeds) + a monotonic counter, all distinct
    /// across consecutive mints.
    #[test]
    fn statement_ids_carry_salt_and_stay_unique() {
        let a = mint_local_statement_id();
        let b = mint_local_statement_id();
        assert_ne!(a, b);
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(parts.len(), 3, "ts-salt-counter shape: {a}");
        assert_eq!(parts[0].len(), 12);
        assert_eq!(parts[1].len(), 16);
        assert_eq!(parts[2].len(), 16);
        assert_ne!(parts[1], "0000000000000000", "native salt self-seeds");
        let b_parts: Vec<&str> = b.split('-').collect();
        assert_eq!(parts[1], b_parts[1], "salt is per-process stable");
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum FrameLayoutHint {
    SplitHalf {
        first: FrameLayoutNodeId,
        second: FrameLayoutNodeId,
        orientation: SplitOrientation,
    },
    SplitPamphlet {
        members: [FrameLayoutNodeId; 3],
        orientation: SplitOrientation,
    },
    SplitTriptych {
        dominant: FrameLayoutNodeId,
        dominant_edge: DominantEdge,
        wings: [FrameLayoutNodeId; 2],
    },
    SplitQuartered {
        top_left: FrameLayoutNodeId,
        top_right: FrameLayoutNodeId,
        bottom_left: FrameLayoutNodeId,
        bottom_right: FrameLayoutNodeId,
    },
}

// ---------------------------------------------------------------------------
// Import provenance types
// ---------------------------------------------------------------------------

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct NodeImportProvenance {
    pub source_id: String,
    pub source_label: String,
}

/// Cross-graph derivation provenance: a record that this node was derived from
/// a node in another graph (the tear-out "fork" / cross-graph copy).
///
/// This is the node-anchored analog of an in-graph `Provenance` edge. A
/// node→node derivation *within* one graph rides a petgraph `Provenance` edge,
/// but a cross-graph derivation has its object node in a *different* graph, so
/// it cannot be a petgraph edge — it is recorded here on the derived node
/// instead, beside [`NodeImportProvenance`] and `NodeClassification`. It
/// projects to a `<this> {provenance predicate} <source>` statement when the
/// RDF projection lands (per the petgraph-rdf projection profile).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct NodeDerivation {
    /// Which provenance relation this is (e.g. [`ProvenanceSubKind::CopiedFrom`]).
    pub sub_kind: ProvenanceSubKind,
    /// The source node's stable id, as a string (matches the kernel's
    /// node-id-as-string convention in persisted records). The object of the
    /// derivation statement.
    pub source_node: String,
    /// The source graph's id (the constellation `GraphId` rendered as a string),
    /// or `None` when same-graph or unknown — the named-graph scope of the source.
    pub source_graph: Option<String>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ImportRecordMembership {
    pub node_id: String,
    pub suppressed: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ImportRecord {
    pub record_id: String,
    pub source_id: String,
    pub source_label: String,
    pub imported_at_secs: u64,
    pub memberships: Vec<ImportRecordMembership>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeImportRecordSummary {
    pub record_id: String,
    pub source_id: String,
    pub source_label: String,
    pub imported_at_secs: u64,
}

pub fn format_imported_at_secs(imported_at_secs: u64) -> String {
    time::OffsetDateTime::from_unix_timestamp(imported_at_secs as i64)
        .ok()
        .and_then(|timestamp| {
            timestamp
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| format!("{}s", imported_at_secs))
}

// ---------------------------------------------------------------------------
// Node classification — Stage A durable enrichment schema
// ---------------------------------------------------------------------------

/// Classification scheme identifier.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ClassificationScheme {
    /// Universal Decimal Classification (primary semantic taxonomy).
    #[default]
    Udc,
    /// Content-kind classification (page, article, repo, …).
    ContentKind,
    /// Custom namespaced scheme (e.g. `"myns:custom"`).
    Custom(String),
}

/// Origin of a classification or tag.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ClassificationProvenance {
    /// Explicitly authored by the user.
    #[default]
    UserAuthored,
    /// Imported from an external data source (bookmarks, history, file, …).
    Imported,
    /// Inherited from a source/parent node relationship.
    InheritedFromSource,
    /// Derived by the knowledge registry (UDC lookup, content analysis, …).
    RegistryDerived,
    /// Proposed by an agent/model; not yet accepted by the user.
    AgentSuggested,
    /// Synced from the community/Verse network.
    CommunitySynced,
}

/// Lifecycle status of a classification record.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ClassificationStatus {
    /// User has explicitly accepted this classification.
    Accepted,
    /// Proposed but not yet reviewed (e.g. agent-suggested).
    #[default]
    Suggested,
    /// User has explicitly rejected this classification.
    Rejected,
    /// Verified by an authoritative external source.
    Verified,
    /// Imported from an external record without explicit user review.
    Imported,
}

/// A single provenance-bearing classification record on a node.
///
/// Multiple records can coexist; at most one should have `primary: true` per scheme.
#[derive(
    Debug, Clone, PartialEq, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize,
)]
pub struct NodeClassification {
    pub scheme: ClassificationScheme,
    /// Scheme-specific classification value (e.g. `"udc:519.6"`, `"article"`).
    pub value: String,
    /// Human-readable label resolved from the scheme (e.g. `"Computational mathematics"`).
    pub label: Option<String>,
    /// Confidence score in `[0.0, 1.0]`; `1.0` for user-authored.
    pub confidence: f32,
    pub provenance: ClassificationProvenance,
    pub status: ClassificationStatus,
    /// Whether this is the primary presentation classification for its scheme.
    pub primary: bool,
}

/// A literal property on a node: an open predicate IRI and its value. Holds the
/// non-curated literals an ingest preserves — the kernel has no other general
/// key→value bag (`title` / `tags` stay the curated fast-paths).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct NodeProperty {
    /// Stable handle for this literal statement.
    #[serde(default = "mint_local_statement_id")]
    pub statement_id: String,
    /// The predicate IRI (e.g. `https://schema.org/datePublished`).
    pub predicate: String,
    /// The literal value.
    pub value: String,
    /// Explicit datatype IRI when the literal is not the default `xsd:string`.
    #[serde(default)]
    pub datatype: Option<String>,
    /// BCP47 language tag for an RDF language-tagged string.
    #[serde(default)]
    pub lang: Option<String>,
    /// Named-graph scope for this literal statement.
    #[serde(default)]
    pub graph_scope: GraphScope,
    /// Optional provenance agent/persona IRI for this assertion.
    #[serde(default)]
    pub provenance_iri: Option<String>,
    /// Optional assertion time in unix epoch milliseconds.
    #[serde(default)]
    pub asserted_at_ms: Option<u64>,
}

impl NodeProperty {
    pub fn new(predicate: String, value: String) -> Self {
        Self {
            statement_id: mint_local_statement_id(),
            predicate,
            value,
            datatype: None,
            lang: None,
            graph_scope: GraphScope::Default,
            provenance_iri: None,
            asserted_at_ms: None,
        }
    }

    pub fn with_graph_scope(mut self, graph_scope: GraphScope) -> Self {
        self.graph_scope = graph_scope;
        self
    }

    pub fn with_metadata(
        mut self,
        provenance_iri: Option<String>,
        asserted_at_ms: Option<u64>,
    ) -> Self {
        self.provenance_iri = provenance_iri;
        self.asserted_at_ms = asserted_at_ms;
        self
    }

    pub fn content_eq(&self, other: &Self) -> bool {
        self.predicate == other.predicate
            && self.value == other.value
            && self.datatype == other.datatype
            && self.lang == other.lang
            && self.graph_scope == other.graph_scope
    }
}

// ---------------------------------------------------------------------------
// Preview imagery references (node image externalization plan)
// ---------------------------------------------------------------------------

/// Which preview-image role a reference fills. A node holds at most one image per
/// role. `Favicon` is the site icon on the node face; `Preview` the default
/// thumbnail; `Snapshot` the last-rendered peek the preview card shows. The set
/// is extensible (per-lane snapshots) by adding variants, which is why the node
/// keys images by role rather than holding fixed fields. `Ord` so it can key a
/// `BTreeMap` with a deterministic iteration order.
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
#[rkyv(compare(PartialEq, PartialOrd), derive(Debug, PartialEq, Eq, PartialOrd, Ord))]
pub enum ImageRole {
    Favicon,
    Preview,
    Snapshot,
}

/// A content-addressed reference to a preview image held in the durable blob
/// store (node image externalization plan): the BLAKE3-256 digest of the stored
/// PNG bytes plus the decoded dimensions. ~40 bytes and no pixels, so a graph of
/// 50k nodes carries references, not image data. The kernel only *carries* the
/// handle; `session-runtime::image_store` computes the digest and owns the blob
/// under `content/image/<hex>`, which is why the digest is a plain `[u8; 32]`
/// here (no eidetic dependency in the kernel).
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
#[rkyv(derive(Debug, PartialEq, Eq))]
pub struct ImageRef {
    /// BLAKE3-256 digest of the stored PNG bytes — the blob key. Rendered as hex
    /// in JSON snapshots (compact + hand-inspectable); rkyv archives the raw bytes.
    #[serde(with = "digest_hex")]
    pub digest: [u8; 32],
    /// Decoded pixel width.
    pub width: u32,
    /// Decoded pixel height.
    pub height: u32,
}

impl ImageRef {
    pub fn new(digest: [u8; 32], width: u32, height: u32) -> Self {
        Self {
            digest,
            width,
            height,
        }
    }

    /// Lowercase hex of the digest — the `content/image/<hex>` blob-key suffix.
    /// Matches `eidetic::Hash::to_hex` so a ref built from a saved blob's hash
    /// reads that same blob back.
    pub fn hex(&self) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(64);
        for byte in &self.digest {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

/// Serde helpers rendering a 32-byte digest as a hex string in JSON snapshots.
mod digest_hex {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(digest: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        use std::fmt::Write;
        let mut hex = String::with_capacity(64);
        for byte in digest {
            let _ = write!(hex, "{byte:02x}");
        }
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let hex = String::deserialize(deserializer)?;
        let bytes = hex.as_bytes();
        if bytes.len() != 64 {
            return Err(D::Error::invalid_length(bytes.len(), &"64 hex characters"));
        }
        let nibble = |b: u8| -> Result<u8, D::Error> {
            (b as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or_else(|| D::Error::custom("digest is not valid hex"))
        };
        let mut out = [0u8; 32];
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Badge / tag presentation types (from badge.rs carve-out)
// ---------------------------------------------------------------------------

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub enum BadgeIcon {
    Emoji(String),
    Lucide(String),
    None,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq, Eq))]
pub struct NodeTagPresentationState {
    pub ordered_tags: Vec<String>,
    pub icon_overrides: HashMap<String, BadgeIcon>,
}
