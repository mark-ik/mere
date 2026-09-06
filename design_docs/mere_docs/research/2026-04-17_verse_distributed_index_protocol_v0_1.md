> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/archive_docs/checkpoint_2026-04-17/verse_docs/technical_architecture/2026-04-17_verse_distributed_index_protocol_v0.1.md` *(historical citation)* <!-- doc-audit: historical-path -->
> - Source commit: `6ab4c22f`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.
>
> The filename was normalized from `..._v0.1.md` to `..._v0_1.md` to match its
> successor. It was already archived in the donor: VGCP replaced it as protocol
> authority the same day, and VGCP is recovered beside it as
> `2026-04-17_verse_graph_contribution_protocol_v0_1.md`.

---

# Verse Distributed Index Protocol (VDIP) v0.1

> Archived on 2026-04-17. Superseded by [2026-04-17_verse_graph_contribution_protocol_v0_1.md](2026-04-17_verse_graph_contribution_protocol_v0_1.md). This file was moved from active Verse technical architecture docs into the archive checkpoint after VGCP replaced it as the active protocol authority.

**Status:** Draft v0.1
**Document type:** Core protocol specification with reference profiles and Graphshell mapping
**Audience:** Protocol design, Graphshell implementation planning, future interop work

This document promotes the April 16 draft set into a single canonical spec for the Verse distributed index protocol. It defines the protocol core, keeps transport and ecosystem choices out of the normative layer where possible, and records the current Graphshell implementation boundary explicitly.

## 1. Scope

VDIP defines how peers exchange signed, immutable, content-addressed graphlets: shareable Entry, Visit, and NavigationEdge artifacts plus optional acceleration packages.

The protocol core covers:

- graph artifact objects and signing rules
- content canonicalization and compatibility profiles
- community admission and revocation semantics
- search and ranking semantics over accepted local artifacts and communal graph structure

The protocol core does not require:

- public DHT-scale discovery
- raw HTML or WARC redistribution by default
- global cross-community federation
- on-chain economics, FLora, or Nostr/DVM integration

Graphshell remains a host and renderer. Verso remains the bilateral peer layer. VDIP belongs to the community-scale Verse layer.

## 2. Design Principles

- Local-first: capture, canonicalization, indexing, and trust evaluation happen locally.
- Immutable artifacts: shared artifacts are append-only and content-addressed.
- Derived-data sharing by default: raw captured content remains local unless explicitly exported.
- Separation of durable truth and acceleration: durable graph artifacts stay valid even if the local search engine changes.
- Transport independence: the protocol core defines bytes and semantics, not one mandatory transport stack.
- Graph-native collaboration: communal structure matters, so navigation edges are first-class protocol objects rather than a deferred afterthought.

## 3. Normative Language

The key words `MUST`, `SHOULD`, and `MAY` are normative.

## 4. Terminology

| Term | Definition |
|------|------------|
| `Entry` | Stable local content/resource identity in Graphshell's local Entry/Visit/Owner substrate. |
| `Visit` | One situated local occurrence or arrival at an Entry. |
| `Owner` | Local actor or cursor context, such as a pane, tab, or device-local navigation owner. Owner context is never shared directly by VDIP v0.1. |
| `EntryCard` | Shareable projection of a local Entry. Carries canonicalized content identity and search-facing metadata. |
| `VisitCard` | Shareable projection of a local Visit. Carries situated occurrence, arrival semantics, and reference to an EntryCard. |
| `ObservationCard` | Historical draft term superseded here by `VisitCard`. |
| `NavigationEdgeCard` | Shareable projection of a transition between two VisitCards. |
| `SplitPackage` | Optional acceleration artifact containing a prebuilt search index bundle and manifest. |
| `CommunityManifest` | Community-scoped state defining governance, profile compatibility, and the active commit head. |
| `IndexCommit` | Signed append-only commit that adds artifacts and revocation references to a community history. |
| `RevocationRecord` | Immutable tombstone-like record removing an artifact from the active accepted set without rewriting history. |
| `ValidatorReceipt` | Optional signed validation result about a candidate artifact. |
| `UDC` | Universal Decimal Classification-derived semantic tagging namespace used for topical scoping, faceting, and ranking. |
| `Graphlet` | Any shareable subset of `{EntryCard, VisitCard, NavigationEdgeCard}` contributed together or separately. |
| `index_profile_hash` | BLAKE3 hash of the canonical index compatibility profile. |

### 4.1 UDC Tags and Scope

In this document, `UDC` refers to Universal Decimal Classification-derived semantic tags used as a shared topic namespace.

- `udc_tags` are the tags attached to one observation.
- `udc_scope` is the topical boundary declared by a community.
- `udc_summary` is an aggregate summary over a package.
- `udc_match` is the ranking component that scores topical fit between query, observation, and community scope.

Communities `SHOULD` document the exact encoding and granularity they accept. A typical tag may look like `004.738.5`, but v0.1 leaves representation as a profile-level choice so long as the community treats it consistently.

## 5. Identity, Serialization, and Addressing

- Identities `MUST` be Ed25519 keypairs.
- Signed objects `MUST` be serialized as canonical CBOR.
- Signatures `MUST` cover canonical manifest bytes, never opaque compressed payload bytes.
- Content identity `MUST` use raw BLAKE3-256.
- Implementations `MAY` expose those digests through a CIDv1 wrapper, but raw BLAKE3 is normative.
- Archive packaging `MUST` be deterministic before hashing.

Rationale: raw BLAKE3 stays close to the underlying blob-transfer model while preserving a clean path to CIDv1 interop later.

## 6. Compatibility Profiles

VDIP defines three compatibility layers. Implementations `MUST` treat these profiles as part of interoperability, not as purely local implementation detail.

### 6.1 CanonicalizationProfile

Defines:

- extraction method
- normalization rules
- volatile-field stripping
- whitespace and token normalization rules

### 6.2 FingerprintProfile

Defines:

- exact-hash algorithm
- near-duplicate algorithm
- shingling rules
- parameterization and seeds

For v0.1, implementations `MUST` support MinHash as the near-duplicate baseline. Parameterization remains profile-defined.

### 6.3 IndexProfile

Defines:

- index schema
- tokenizer and analyzer chain
- canonicalization profile version
- fingerprint profile version
- search engine compatibility markers

The compatibility fingerprint is:

```text
index_profile_hash = BLAKE3(
    canonical_cbor({
        canonicalization_profile,
        fingerprint_profile,
        index_schema,
        tokenizer_config,
        analyzer_config,
        engine_compatibility,
    })
)
```

`SplitPackage`s are binary-compatible only when their `index_profile_hash` matches an accepted profile for the target community.

## 7. Content Canonicalization Pipeline

Before an EntryCard or VisitCard is recorded, content `MUST` be canonicalized:

1. Extraction: readability-style main-content extraction or equivalent profile-defined algorithm.
2. Normalization: remove volatile tokens, session identifiers, ad markup, and profile-declared noise.
3. Exact fingerprinting: compute BLAKE3 over canonical extracted text or fields.
4. Near-duplicate fingerprinting: compute MinHash over profile-defined shingles.
5. UDC tagging: derive semantic tags from explicit user annotation, classifiers, or both.

Raw HTML, WARC payloads, screenshots, and similar captures `SHOULD` remain local by default.

### 7.1 Local Substrate Projection Boundary

Graphshell's local model may distinguish `Entry`, `Visit`, and `Owner` as separate concepts. VDIP v0.1 preserves that distinction on the wire for shareable graph structure while keeping Owner private.

- `EntryCard` is the shareable projection of a local Entry.
- `VisitCard` is the shareable projection of a local Visit and references one EntryCard.
- `NavigationEdgeCard` is the shareable projection of a transition between two VisitCards.
- `Owner` identity, owner-specific branch state, local cursor position, and other private coordination context never leave the device.
- Communities may exchange partial graphlets, but a valid graphlet preserves the dependency order `EntryCard -> VisitCard -> NavigationEdgeCard`.
- This is a conscious privacy boundary: shareable communal graph structure is preserved, private ownership structure is not.

## 8. Core Protocol Objects

### 8.1 TransitionKind

```rust
enum TransitionKind {
    LinkClick,
    TypedUrl,
    Redirect,
    Back,
    Forward,
    Reload,
    Restore,
    Imported,
    Unknown,
}
```

### 8.2 EntryCard

```rust
struct EntryCard {
    entry_id: [u8; 32],
    canonical_url: String,
    title: Option<String>,
    snippet: Option<String>,
    content_hash: [u8; 32],
    minhash: Vec<u64>,
    udc_tags: Vec<String>,
    profile_hash: [u8; 32],
}

struct SignedEntryCard {
    card: EntryCard,
    signer: [u8; 32],
    signature: [u8; 64],
}
```

`entry_id` is the stable content/resource identity carried across visits and packages.

### 8.3 VisitCard

```rust
struct VisitCard {
    visit_id: [u8; 32],
    entry_id: [u8; 32],
    observed_at: u64,
    arrival_kind: Option<TransitionKind>,
    profile_hash: [u8; 32],
}

struct SignedVisitCard {
    card: VisitCard,
    signer: [u8; 32],
    signature: [u8; 64],
}
```

`VisitCard` is the graph-native successor to the earlier `ObservationCard` term.

`arrival_kind` captures Visit-level transition semantics without publishing full owner-specific tree state.

### 8.4 NavigationEdgeCard

```rust
struct NavigationEdgeCard {
    edge_id: [u8; 32],
    from_visit: [u8; 32],
    to_visit: [u8; 32],
    transition_kind: TransitionKind,
    observed_at: u64,
    profile_hash: [u8; 32],
}

struct SignedNavigationEdgeCard {
    card: NavigationEdgeCard,
    signer: [u8; 32],
    signature: [u8; 64],
}
```

Edges are first-class because communal web-mapping depends on structure, not just isolated visits.

### 8.5 SplitPackageManifest

```rust
struct SplitPackageManifest {
    package_id: [u8; 32],
    payload_size: u64,
    profile_hash: [u8; 32],
    min_observed_at: u64,
    max_observed_at: u64,
    entry_count: u64,
    visit_count: u64,
    edge_count: u64,
    entry_refs: Vec<[u8; 32]>,
    visit_refs: Vec<[u8; 32]>,
    edge_refs: Vec<[u8; 32]>,
    udc_summary: Vec<(String, u32)>,
    contributor: [u8; 32],
    supersedes: Vec<[u8; 32]>,
    created_at: u64,
}

struct SignedSplitPackage {
    manifest: SplitPackageManifest,
    signature: [u8; 64],
}
```

The payload is an implementation-defined, deterministic archive of search-engine artifacts and graph-side acceleration data. The manifest is the signed and gossiped unit.

### 8.6 CommunityManifest

```rust
struct CommunityManifest {
    community_id: [u8; 32],
    name: String,
    description: String,
    udc_scope: Vec<String>,
    primary_profile: [u8; 32],
    accepted_profiles: Vec<[u8; 32]>,
    admin_keys: Vec<[u8; 32]>,
    moderator_keys: Vec<[u8; 32]>,
    ranking_weights: RankingWeights,
    invite_policy: InvitePolicy,
    preferred_head: Option<[u8; 32]>,
    head_epoch: u64,
}

struct RankingWeights {
    bm25: f32,
    udc_match: f32,
    trust: f32,
    freshness: f32,
    novelty: f32,
    graph_structure: f32,
}
```

`accepted_profiles` allows explicit dual-profile migration periods. `preferred_head` resolves the active commit-set question for v0.1.

### 8.7 IndexCommit

```rust
struct IndexCommit {
    commit_id: [u8; 32],
    parents: Vec<[u8; 32]>,
    community_id: [u8; 32],
    added_entries: Vec<[u8; 32]>,
    added_visits: Vec<[u8; 32]>,
    added_edges: Vec<[u8; 32]>,
    added_packages: Vec<[u8; 32]>,
    added_receipts: Vec<[u8; 32]>,
    revocations: Vec<[u8; 32]>,
    timestamp: u64,
    author: [u8; 32],
}

struct SignedCommit {
    commit: IndexCommit,
    signature: [u8; 64],
}
```

The commit graph is a DAG. Consumers `MUST` treat the active accepted set as the closure of commits reachable from the community's `preferred_head`.

### 8.8 RevocationRecord

```rust
enum ArtifactKind {
    Entry,
    Visit,
    NavigationEdge,
    SplitPackage,
    ValidatorReceipt,
}

struct RevocationRecord {
    revocation_id: [u8; 32],
    community_id: [u8; 32],
    target_kind: ArtifactKind,
    target_id: [u8; 32],
    reason_code: String,
    note: Option<String>,
    revoked_at: u64,
    revoked_by: [u8; 32],
}

struct SignedRevocation {
    record: RevocationRecord,
    signature: [u8; 64],
}
```

Revocation removes an artifact from the active set but does not rewrite historical records.

### 8.9 ValidatorReceipt

```rust
enum ValidationDecision {
    Accept,
    Reject,
    Warn,
}

struct ValidatorReceipt {
    receipt_id: [u8; 32],
    community_id: [u8; 32],
    subject_kind: ArtifactKind,
    subject_id: [u8; 32],
    validator: [u8; 32],
    decision: ValidationDecision,
    reason_code: Option<String>,
    observed_at: u64,
    expires_at: Option<u64>,
}

struct SignedValidatorReceipt {
    receipt: ValidatorReceipt,
    signature: [u8; 64],
}
```

Validator receipts are optional in v0.1. Communities `MAY` require them in local admission policy, and they also provide the accountability path for curator-attested imports of donated unsigned artifacts.

## 9. Community State and Head Selection

VDIP resolves commit-head selection minimally in v0.1:

- The authoritative active head is `CommunityManifest.preferred_head`.
- Only admin keys `MUST` be allowed to advance `preferred_head`.
- Consumers `MUST` apply only commits reachable from `preferred_head` when computing the active accepted set.
- Consumers `MAY` maintain local provisional heads for staging or review, but those heads `MUST NOT` be treated as canonical community state unless published through an admin-authorized manifest update.
- `head_epoch` `MUST` increase monotonically when `preferred_head` changes.

This keeps DAG history available without leaving active-state selection undefined.

This is a pragmatic v0.1 simplification, not a claim that admin-published heads are the intended long-term decentralization model.

## 10. Admission, Revocation, and Profile Migration

- Communities `MUST` define an invite or access policy.
- Communities `SHOULD` require individually signed `EntryCard`, `VisitCard`, and `NavigationEdgeCard` artifacts for ordinary participation.
- Communities `MAY` apply additional admission checks using validator receipts, trust scores, or local moderation rules.
- Communities `MAY` admit donated unsigned graph artifacts only through signed attestation by a validator, curator, or import gateway.
- When unsigned donated artifacts are admitted, accountability attaches to the attesting signer, not to an anonymous donor.
- An accepted `VisitCard` `MUST` reference an accepted `EntryCard` present in the same or an earlier reachable commit.
- An accepted `NavigationEdgeCard` `MUST` reference accepted `VisitCard`s present in the same or an earlier reachable commit.
- Revoked artifacts `MUST NOT` remain in the active accepted set after the relevant `RevocationRecord` becomes reachable from the preferred head.
- Communities `SHOULD` use `accepted_profiles` to support controlled profile migration.
- During migration, communities `MAY` accept graph artifacts from more than one profile, but `SplitPackage` mounting still requires exact binary compatibility with one accepted profile.

## 11. Trust and Ranking

Trust is community-local and implementation-defined, but the protocol assumes these inputs are available to local policy:

- invite lineage
- endorsements and flags
- identity age
- utilization in local search
- novelty relative to existing community content
- graph structure quality, edge frequency, and redirect anomaly signals

Implementations `MAY` compute trust using EigenTrust-like or other graph-based algorithms.

Recommended enforcement rule for v0.1:

- trust gates `SplitPackage` mounting and optional artifact admission
- query-time ranking combines textual score with UDC relevance, trust, freshness, novelty, and graph-structure signals

## 12. Search Semantics

Search execution `MUST` be local over the consumer's accepted artifacts.

1. Scope the query to one or more communities.
2. Gather locally accepted entries, visits, edges, and mounted split packages reachable from each community's preferred head.
3. Query across the local search engine's readers for entries and visits.
4. Deduplicate exact matches by `entry_id` and `content_hash`.
5. Cluster near-duplicates using MinHash similarity.
6. Apply visit-level freshness and arrival semantics.
7. Apply edge-derived graph priors, redirect-chain penalties, or provenance/path boosts where a community chooses to use them.
8. Rank with community-defined weights.
9. Return results with provenance.

In v0.1, exact identity is centered on `EntryCard`, situated relevance is centered on `VisitCard`, and communal structure is centered on `NavigationEdgeCard`.

The composite score is community-defined but typically has the form:

```text
score = w_bm25    * bm25
      + w_udc     * udc_match
      + w_trust   * contributor_trust
      + w_fresh   * freshness_decay(observed_at)
      + w_novelty * novelty_vs_community
      + w_graph   * graph_structure_signal
```

Global mandatory merge is out of scope for v0.1. Fan-out across accepted local artifacts is sufficient.

## 13. Privacy Baseline

- VDIP v0.1 addresses artifact-sharing privacy more directly than query privacy.
- Query privacy is typically more sensitive than artifact privacy because queries reveal current intent, not only past action.
- Collection and sharing of Entry, Visit, and NavigationEdge data are opt-in and community-scoped.
- Raw capture data `SHOULD` remain local by default.
- Shared artifacts `SHOULD` prefer derived search data over raw page archives.
- Query privacy is separate from artifact privacy and is deferred beyond v0.1.
- Owner context, owner-specific branch state, and private cursor structure `MUST NOT` be shared as protocol artifacts.
- Implementations `SHOULD` support local-only search over already-fetched artifacts so remote query disclosure is not mandatory.

## 14. Reference Profiles

These profiles are reference implementation guidance, not mandatory protocol rules.

### 14.1 Profile A: Iroh-First Trusted Exchange

- blob transfer: `iroh-blobs`
- community announcements: `iroh-gossip`
- replicated community state: `iroh-docs`
- connectivity and relays: `iroh`

This is the most direct fit for early Graphshell experimentation.

### 14.2 Profile B: Broader Discovery Overlay

Public discovery overlays such as libp2p-based routing or other announcement fabrics may be added later. They are not required for v0.1 conformance.

## 15. Graphshell Implementation Mapping

The protocol is ahead of the current Graphshell implementation. These reuse points and gaps are the current boundary:

- [../../../crates/graph-memory/src/lib.rs](../../../crates/graph-memory/src/lib.rs) *(historical citation)* <!-- doc-audit: historical-link --> and [../../../crates/graphshell-core/src/graph/mod.rs](../../../crates/graphshell-core/src/graph/mod.rs) *(historical citation)* <!-- doc-audit: historical-link --> express the local Entry/Visit/Owner-style navigation substrate. That substrate is not incidental to Verse; it is the source model for shareable communal graphlets.
- In that framing, `EntryCard` is the shareable projection of Entry identity, `VisitCard` is the shareable projection of situated occurrence, and `NavigationEdgeCard` is the shareable projection of communal traversal structure.
- Graph-memory matters because it lets Verse preserve the web's collaborative shape without exporting private owner context. The protocol keeps communal graph structure while dropping owner-specific cursors and branches.

- [../../../mods/native/verse/mod.rs](../../../mods/native/verse/mod.rs) *(historical citation)* <!-- doc-audit: historical-link --> already provides Ed25519-backed iroh identity, trusted-peer storage, and workspace-grant concepts that can seed community identity and admission work.
- [../../../model/archive.rs](../../../model/archive.rs) *(historical citation)* <!-- doc-audit: historical-link --> already contains signed portable archive objects and privacy classes; that is a useful precedent for signed artifact envelopes, but it is not yet VDIP's object model.
- [../../../app/clip_capture.rs](../../../app/clip_capture.rs) *(historical citation)* <!-- doc-audit: historical-link --> already exposes structured capture data from web content; this is a precursor to canonicalization, not the canonicalization pipeline itself.
- [../../../services/query/mod.rs](../../../services/query/mod.rs) *(historical citation)* <!-- doc-audit: historical-link --> and [../../../services/facts/mod.rs](../../../services/facts/mod.rs) *(historical citation)* <!-- doc-audit: historical-link --> provide local structured querying over projected history facts, but they do not yet implement distributed graphlet exchange, split-package search, or community ranking.

Missing pieces before Graphshell can claim a VDIP implementation:

- canonicalization profile machinery
- BLAKE3 and MinHash artifact fingerprinting for entries and visits
- first-class emission of shareable visit and navigation-edge graphlets from graph-memory
- Tantivy or equivalent split-package production and mounting
- community manifest replication beyond bilateral sync
- commit and revocation application logic
- trust-graph computation for admission or mount-time gating

## 16. Rollout Order

1. EntryCard, VisitCard, and NavigationEdgeCard serialization with canonical CBOR and signatures.
2. Canonicalization profiles plus BLAKE3 and MinHash generation for graph artifacts.
3. Local-only community manifest and preferred-head handling.
4. Two-peer graphlet exchange using a reference transport.
5. Local search across accepted entries and visits, with optional edge-informed ranking and provenance.
6. SplitPackage production and mounting over accepted graphlets.
7. Community trust and optional validator receipts.
8. Broader discovery overlays and advanced privacy features.

## 17. Deferred Work

- batch-signing extensions
- dense graphlet summarization and higher-order communal path models
- head selection without admin-authoritative `preferred_head`
- private community discovery with existence-hiding properties
- advanced query privacy
- mandatory public discovery overlays
- cross-community search federation
- raw snapshot redistribution by default
- economics, FLora, and Nostr/DVM integration

## 18. References

- [2026-04-16_verse_index_protocol_drafts.md](../../../../verse_docs/technical_architecture/2026-04-16_verse_index_protocol_drafts.md) *(historical citation)* <!-- doc-audit: historical-link -->
- [VERSE_AS_NETWORK.md](../../../../verse_docs/technical_architecture/VERSE_AS_NETWORK.md) *(historical citation)* <!-- doc-audit: historical-link -->
- [2026-02-23_verse_tier2_architecture.md](../../../../verse_docs/technical_architecture/2026-02-23_verse_tier2_architecture.md) *(historical citation)* <!-- doc-audit: historical-link -->
