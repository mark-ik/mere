> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verse_docs/technical_architecture/2026-04-17_verse_graph_contribution_protocol_v0_1.md` *(historical citation)* <!-- doc-audit: historical-path -->
> - Source commit: `6ab4c22f`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.
>
> This is the later half of a protocol pair: VGCP superseded VDIP on the same
> day. Its predecessor is recovered beside it as
> `2026-04-17_verse_distributed_index_protocol_v0_1.md`.

---

# Verse Graph Contribution Protocol (VGCP) v0.1

**Status:** Draft v0.1
**Document type:** Core protocol specification with reference profiles and Graphshell mapping
**Audience:** Protocol design, Graphshell implementation planning, future interop work
**Supersedes:** 2026-04-17_verse_distributed_index_protocol_v0.1.md (VDIP v0.1)

This document replaces VDIP v0.1. The change is substantive, not cosmetic. VDIP treated search observations as the unit of contribution and edges as an afterthought. VGCP treats graph contributions as the unit, with isolated observations as the degenerate (zero-edge) case. The protocol is graph-native; Verses are communities that map the web, and the map is the shareable artifact.

## 1. Scope

VGCP defines how peers exchange graph-shaped knowledge about the web — across HTTP, smolweb protocols (Gemini, Gopher, Scroll, Spartan, and others), and in principle any protocol whose resources admit canonicalization — as signed, immutable, content-addressed artifacts.

The protocol core covers:

- the Entry/Visit/Owner substrate and its projection rule
- graph contribution objects and signing rules
- protocol-neutral content canonicalization and compatibility profiles
- structural edge semantics grounded in protocol-defined relationships
- attestation and aggregation semantics across contributions
- community admission, revocation (whole and fragmentary), profile migration, and declarative filtering
- search and ranking semantics over accepted local artifacts

The protocol core does not require:

- public DHT-scale discovery
- raw payload redistribution by default
- global cross-community federation
- on-chain economics, storage-time-bank tokenization, or governance staking
- Nostr/DVM, Matrix, or other social host integrations

Graphshell remains a host and renderer. Verso remains the bilateral peer layer. VGCP belongs to the community-scale Verse layer.

## 2. Design Principles

- Graph-native: contributions are subgraphs. Single orphans are the zero-edge case, not a separate artifact type.
- Structure over behavior: shared edges describe protocol-defined relationships between resources, not local navigation events. How a contributor traversed the graph is optional aggregate metadata; the graph itself is structural.
- Protocol-neutral at the core: the object model accommodates any protocol whose resources have stable canonicalization and whose references can be projected into edges.
- Local-first: capture, canonicalization, indexing, and trust evaluation happen locally.
- Immutable artifacts, mutable projection: shared artifacts are append-only and content-addressed. The community's *active accepted set* is a read-time projection that honors revocations and filters.
- Projection-not-mirror: shared artifacts are projections of local state that strip Visit and Owner context by construction.
- Derived-data sharing by default: raw captured content remains local unless explicitly exported.
- Transport independence: the protocol core defines bytes and semantics, not one mandatory transport stack.
- Communal authority: manifest-level authority is attested by the process the community chose, not by a designated individual.

## 3. Normative Language

The key words `MUST`, `SHOULD`, and `MAY` are normative.

## 4. Terminology

| Term | Definition |
|------|------------|
| `Entry` | Deduplicated resource identity (the node). A URL plus canonicalized content, under a community's canonicalization profile. Shareable. |
| `Visit` | A situated local occurrence of an Entry: when the contributor was there, in what context, via what transition. Local-only; never shared. |
| `Owner` | A local cursor-bearing actor (tab, pane, session, graph view). Local-only; never shared. |
| `EntryRecord` | The shareable projection of an Entry: identity hash, content-equivalence hash, metadata, and community-scoped fingerprints. |
| `EdgeRecord` | The shareable projection of a structural relationship between two Entries. |
| `GraphContribution` | A signed, canonicalized bundle of EntryRecords and EdgeRecords from one contributor. The unit of contribution. |
| `SplitPackage` | Optional acceleration artifact: a prebuilt search-index bundle over accepted contributions. |
| `CommunityManifest` | Community-scoped state defining governance, profile compatibility, filter policy, and the active commit head. |
| `SignedCommunityManifest` | Canonical manifest bytes plus the set of governance attestations that authorize this manifest version. |
| `ManifestGovernance` | Rule that determines which attestations authorize a manifest update. |
| `ManifestAttestation` | One attester's signature over canonical `CommunityManifest` bytes. |
| `IndexCommit` | Signed append-only commit that adds contributions and revocations to a community history. |
| `RevocationRecord` | Immutable tombstone-like record removing a whole contribution, or specific Entries or Edges within it, from the active accepted set. |
| `FilterPolicy` | Declarative community-level rules applied at aggregation time to include or exclude contribution elements. |
| `ValidatorReceipt` | Optional signed validation result about a candidate artifact. |
| `KeyRotation` | Cross-signed declaration that one identity key is succeeded by another for contributor-equivalence purposes. |
| `Attestation` | The fact of an EntryRecord or EdgeRecord appearing in a contribution; weighting and aggregation are attestation-derived. |
| `profile_hash` | Wire-field name for the BLAKE3 hash of the canonical index compatibility profile; prose may call this the community's `index_profile_hash`. |

## 5. The Entry/Visit/Owner Substrate

VGCP presumes a local data model compatible with the Entry/Visit/Owner substrate.

- **Entry**: deduplicated identity of a web resource. One Entry per canonicalized (URL, content) tuple under a profile.
- **Visit**: a concrete persisted occurrence. Always distinct; `VisitId`s are never reused.
- **Owner**: a local cursor-bearing actor carrying Visit parentage and per-Owner forward-choice state.

### 5.1 The Projection Rule

A GraphContribution is the Entry-level projection of a subgraph of the contributor's local Entry graph — the graph induced by structural relationships between Entries the contributor has captured. The projection:

- `MUST` preserve: Entry identity and content-equivalence hashes, community-relevant Entry metadata, Edge endpoints, edge kinds and their protocol-defined metadata.
- `MUST` strip: VisitIds, Visit timestamps at navigation granularity, Owner identity, Owner forward-choice state, local parent pointers between Visits.
- `SHOULD` coarsen: temporal information. Timestamps `SHOULD` be expressed in buckets (e.g., day-granularity).
- `MAY` include: optional aggregate navigation metadata on Edges (traversal counts, observation windows) as a separate concern from the Edge's structural existence.

This projection rule is the privacy boundary between Graphshell (local) and Verse (shared). It is structural, not conventional.

### 5.2 Structural vs. Behavioral Edges

A central design commitment: Edges represent *structural* relationships that any contributor fetching the source resource would derive identically — links defined in a page's markup, transclusions declared in its syntax, redirects declared by the server. Edges are not records of user behavior.

Behavioral navigation (what was clicked, what was typed, what was back-buttoned) lives on local Visits and does not cross the projection boundary as structural signal. It `MAY` cross as optional aggregate metadata — "this `Link` was traversed N times in this window by this contributor" — but the existence of the Edge is structural, not behavioral.

Rationale: structural edges are verifiable by any contributor re-canonicalizing the source Entry, which collapses most forgery attack surface. Behavioral edges are attestation-only and provide no basis for cross-contributor corroboration.

## 6. Identity, Serialization, and Addressing

- Identities `MUST` be Ed25519 keypairs.
- Signed objects `MUST` be serialized as canonical CBOR.
- Signatures `MUST` cover canonical manifest bytes, never opaque compressed payload bytes.
- Content identity `MUST` use raw BLAKE3-256.
- Implementations `MAY` expose those digests through a CIDv1 wrapper, but raw BLAKE3 is normative.
- Archive packaging `MUST` be deterministic before hashing.

### 6.1 Key Rotation

Identity keys are long-lived but not assumed permanent.

```rust
struct KeyRotation {
        rotation_id: [u8; 32],
        old_key: [u8; 32],
        new_key: [u8; 32],
        rotated_at: u64,
}

struct SignedKeyRotation {
        rotation: KeyRotation,
        old_signature: [u8; 64],
        new_signature: [u8; 64],
}
```

Rules for v0.1:

- a rotation is valid only if both the old key and new key sign the same
    canonical `KeyRotation` bytes,
- rotations are not active until a community explicitly accepts them through an
    `IndexCommit`,
- accepted rotations form a linear chain in v0.1; forked rotation graphs are
    deferred,
- contributions signed by a retired key remain valid historical facts,
- lost predecessor keys cannot rotate; recovery mechanisms are deferred.

## 7. Compatibility Profiles

VGCP defines three compatibility layers.

### 7.1 CanonicalizationProfile

Protocol-scoped. A profile may declare canonicalization rules for one or more protocols. Mixed-protocol communities declare rules for each protocol they accept.

Per protocol, a profile defines:

- resource fetch and extraction method
- main-content extraction (where applicable)
- normalization rules (volatile-field stripping, session-identifier scrubbing, whitespace and token normalization)
- metadata extraction, including explicit UDC tags where the protocol supports them (Scroll is the notable case)

### 7.2 FingerprintProfile

Defines:

- exact-hash algorithm (BLAKE3-256 in v0.1)
- near-duplicate algorithm (MinHash in v0.1)
- shingling rules, parameterization, and seeds

### 7.3 IndexProfile

Defines:

- entry record schema
- edge record schema
- edge kind enumeration (see Section 9.3)
- protocol-specific profile_extension schemas (see Section 9.3)
- tokenizer and analyzer chain
- canonicalization profile version
- fingerprint profile version
- search engine compatibility markers

Compatibility fingerprint:

```text
index_profile_hash = BLAKE3(
    canonical_cbor({
        canonicalization_profile,
        fingerprint_profile,
        entry_schema,
        edge_schema,
        edge_kind_enum,
        protocol_extension_schemas,
        tokenizer_config,
        analyzer_config,
        engine_compatibility,
    })
)
```

`SplitPackage`s are binary-compatible only when their `index_profile_hash` matches an accepted profile for the target community.

On the wire, structures carry this digest in a field named `profile_hash`.
`index_profile_hash` is prose shorthand for the same compatibility fingerprint.

## 8. Canonicalization Pipeline

Before a contribution is assembled, local state `MUST` be canonicalized:

1. **Entry canonicalization**: for each Entry, apply protocol-appropriate extraction and normalization per the target community's CanonicalizationProfile. Compute `content_hash` as the hash of canonical `(URL, content)` bytes under the profile, compute `content_only_hash` as the hash of canonical content bytes under that same profile, and compute `minhash` over the canonicalized text projection used for near-duplicate clustering. Derive or extract UDC tags — from explicit document metadata where the protocol supports it, from classifier output otherwise.
2. **Structural edge extraction**: for each Entry in the contribution, extract protocol-defined structural relationships (links, includes, redirects, references) to other Entries also in the contribution. Edges pointing outside the contribution's Entry set are excluded.
3. **Optional behavioral enrichment**: Edges `MAY` be annotated with aggregate traversal metadata from local Visits. Timestamps are coarsened to buckets. Duplicate (from, to, kind) tuples are merged.
4. **Privacy-class filtering**: contributor-local policy `MUST` exclude any Entry marked private, and `MUST` drop any Edge whose endpoints include an excluded Entry. Filtering happens before canonicalization so excluded material does not influence the signed bytes.
5. **Canonical ordering**: Entries sorted by `content_hash`; Edges sorted by `(from, to, kind, canonical_extension_digest)`.
6. **Signing**: canonical CBOR of the GraphContribution is signed with the contributor's Ed25519 key.

Raw payload capture data `SHOULD` remain local by default.

## 9. Core Protocol Objects

### 9.1 GraphContribution

The primary artifact.

```rust
struct GraphContribution {
    contribution_id: [u8; 32],      // BLAKE3 of canonical manifest minus this field and signature
    entries: Vec<EntryRecord>,      // 1..N; orphans are valid contributions
    edges: Vec<EdgeRecord>,         // 0..M; zero edges is valid
    contributor: [u8; 32],
    profile_hash: [u8; 32],
    created_at: u64,                // coarse timestamp (bucket granularity recommended)
}

struct SignedGraphContribution {
    contribution: GraphContribution,
    signature: [u8; 64],
}
```

**Structural invariants:**

- Every `EdgeRecord.from` and `EdgeRecord.to` `MUST` equal a `content_hash` of some `EntryRecord` in the same contribution.
- A contribution `MAY` be disconnected. A single contribution may contain multiple connected components and orphan nodes.
- `entries.len() >= 1`.
- `edges` `MAY` be empty.

### 9.2 EntryRecord

```rust
struct EntryRecord {
    url: String,                    // includes scheme: "https://", "gemini://", "gopher://", "scroll://", etc.
    protocol: String,               // canonical lowercase: "https", "gemini", "gopher", "scroll", ...
    content_hash: [u8; 32],         // BLAKE3 of canonical (URL, content) bytes; Entry identity
    content_only_hash: [u8; 32],    // BLAKE3 of canonical content bytes; cross-URL exact-content equivalence
    minhash: Vec<u64>,
    udc_tags: Vec<String>,
    udc_source: UdcSource,          // Explicit, Classifier, Hybrid
    title: Option<String>,
    snippet: Option<String>,
    observed_at_bucket: u64,
}

enum UdcSource {
    Explicit,                       // UDC declared in document (e.g., Scroll metadata)
    Classifier,                     // UDC inferred by local classifier
    Hybrid,                         // Explicit, supplemented by classifier
}
```

Entry identity in v0.1 is canonical `(URL, content)` rather than content-only.
Two mirrors or republications carrying the same body bytes at different URLs are
distinct Entries and therefore have distinct `content_hash` values, because
canonicalization includes canonical URL in the identity hash.

`content_only_hash` is an additional exact-content equivalence fingerprint, not
an alternate Entry identity. Entries that share a `content_only_hash` remain
separate Entries with separate attestation and ranking histories; the shared
hash exists so implementations can ask read-time questions like "which Entries
carry this same canonical content across different URLs?" without collapsing
those Entries into one node.

`udc_source` is a signal-quality indicator. Community ranking `MAY` weight `Explicit` contributions more heavily.

### 9.3 EdgeRecord

```rust
struct EdgeRecord {
    from: [u8; 32],                         // content_hash of source EntryRecord
    to: [u8; 32],                           // content_hash of destination EntryRecord
    kind: EdgeKind,
    label: Option<String>,                  // universal: visible link/menu/reference text where applicable
    source_protocol: String,                // protocol of the 'from' Entry
    profile_extensions: Vec<(String, Vec<u8>)>,  // profile-defined, protocol-scoped metadata
    // Optional aggregate navigation metadata; may be omitted entirely.
    traversal_count: Option<u32>,
    first_observed_bucket: Option<u64>,
    last_observed_bucket: Option<u64>,
}

enum EdgeKind {
    Link,        // one resource references another for the reader to follow
    Include,     // one resource transcludes another (mostly HTTP/HTML: iframe, img, script, style)
    Redirect,    // one resource replaces another (HTTP 3xx, meta-refresh, protocol-defined redirection)
    Reference,   // relationship metadata between resources (canonical, alternate, prev/next, hreflang; mostly HTML)
}
```

`EdgeKind` is closed and bounded by what's defined in widely-deployed protocol standards rather than by local user behavior. Additions require a profile bump.

`profile_extensions` carry protocol-specific metadata. The IndexProfile declares the schema per protocol. Examples of expected extension fields:

- `source_protocol = "https"`; `kind = Link`: `region` (Nav, Main, Aside, Footer, Header, Other), `rel` (nofollow, noopener, sponsored, ...)
- `source_protocol = "gemini"`; `kind = Link`: typically none beyond `label`
- `source_protocol = "gopher"`; `kind = Link`: `item_type` (Gopher item type octet)
- `source_protocol = "scroll"`; `kind = Link`: Scroll-defined `Link` context; possibly UDC-scoped

Implementations `MUST NOT` populate `profile_extensions` with keys the profile does not declare. Canonicalization sorts extension pairs by key.

**Open uncertainty (disclaimed as future work):** The granularity of HTML `Link` context — specifically, whether `region` as a coarse enum (Nav, Main, Aside, Footer, Header, Other) is the right resolution, or whether finer DOM-path or CSS-selector context should be preserved — is not settled. Coarse region is likely sufficient for ranking and compact for serialization, but may under-specify contexts communities care about. This is deferred to a later profile revision; the `profile_extensions` structure accommodates refinement without breaking the edge model.

### 9.4 SplitPackageManifest

```rust
struct SplitPackageManifest {
    package_id: [u8; 32],
    payload_size: u64,
    profile_hash: [u8; 32],
    min_observed_at: u64,
    max_observed_at: u64,
    contribution_refs: Vec<[u8; 32]>,
    entry_count: u64,
    edge_count: u64,
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

### 9.5 CommunityManifest

```rust
struct CommunityManifest {
    community_id: [u8; 32],
    name: String,
    description: String,
    version: u64,
    previous_manifest_hash: Option<[u8; 32]>,
    udc_scope: Vec<String>,
    supported_protocols: Vec<String>,    // "https", "gemini", "scroll", etc.
    primary_profile: [u8; 32],
    accepted_profiles: Vec<[u8; 32]>,
    admin_keys: Vec<[u8; 32]>,
    moderator_keys: Vec<[u8; 32]>,
    manifest_governance: ManifestGovernance,
    ranking_weights: RankingWeights,
    filter_policy: FilterPolicy,
    invite_policy: InvitePolicy,
    preferred_head: Option<[u8; 32]>,
    head_epoch: u64,
}

struct RankingWeights {
    bm25: f32,
    udc_match: f32,
    udc_explicit_bonus: f32,             // extra weight for Entries with UdcSource::Explicit
    trust: f32,
    freshness: f32,
    novelty: f32,
    edge_support: f32,                   // weight of edge-attestation evidence
    structural_centrality: f32,          // weight of graph-centrality signal
}

struct FilterPolicy {
    entry_url_blocklist: Vec<String>,            // URL patterns, profile-defined matching
    entry_url_allowlist: Option<Vec<String>>,    // if Some, only matches admitted
    entry_content_blocklist: Vec<[u8; 32]>,      // specific content_hashes
    udc_tag_blocklist: Vec<String>,
    udc_tag_allowlist: Option<Vec<String>>,
    protocol_blocklist: Vec<String>,             // e.g., block "finger"
    protocol_allowlist: Option<Vec<String>>,
    edge_kind_blocklist: Vec<EdgeKind>,          // e.g., block Include to limit tracking-pixel surfaces
    contributor_blocklist: Vec<[u8; 32]>,
    max_entries_per_contribution: Option<u32>,
    max_edges_per_contribution: Option<u32>,
}

enum ManifestGovernance {
    Genesis {
        bootstrap_key: [u8; 32],
    },
    Threshold {
        steward_keys: Vec<[u8; 32]>,
        threshold: u32,
    },
    Delegated {
        required_attesters: Vec<[u8; 32]>,
        threshold: u32,
        policy_doc: Option<String>,
    },
}

struct ManifestAttestation {
    attester: [u8; 32],
    signature: [u8; 64],
}

struct SignedCommunityManifest {
    manifest: CommunityManifest,
    attestations: Vec<ManifestAttestation>,
}
```

Rules for manifest governance in v0.1:

- `Genesis` is valid only for version `0` manifests and is self-attesting,
- non-genesis manifests `MUST NOT` use `Genesis`,
- `Threshold` uses a fixed steward key set and an `N-of-M` rule,
- `Delegated` is the protocol hook for voting systems, rotating councils, or
  automated governance; protocol validation is still only on
  `required_attesters` plus `threshold`, while `policy_doc` is advisory for
  humans.

When validating manifest `N+1`, consumers `MUST` validate its attestations
against manifest `N`'s `manifest_governance`, regardless of what governance rule
`N+1` declares for its own successors. Governance evolves, but each transition
is validated by the prior rule.

Manifest verification procedure:

1. Canonicalize the candidate `CommunityManifest` bytes using canonical CBOR.
2. Verify each `ManifestAttestation.signature` against those bytes.
3. If `version == 0`, require `previous_manifest_hash == None`, require the
   manifest to declare `ManifestGovernance::Genesis`, and require a valid
   self-attestation by the declared `bootstrap_key`.
4. If `version > 0`, load the previous accepted manifest, read its
   `manifest_governance`, and evaluate the candidate attestations against that
   prior rule.
5. Succeed iff the set of distinct valid attesters satisfies the applicable
   governance rule.

Filter policy updates, ranking weight changes, and profile acceptance changes
are manifest updates governed by `manifest_governance`; they are not admin-only
actions.

### 9.6 IndexCommit

```rust
struct IndexCommit {
    commit_id: [u8; 32],
    parents: Vec<[u8; 32]>,
    community_id: [u8; 32],
    added_contributions: Vec<[u8; 32]>,
    added_packages: Vec<[u8; 32]>,
    added_receipts: Vec<[u8; 32]>,
    added_rotations: Vec<[u8; 32]>,
    revocations: Vec<[u8; 32]>,
    timestamp: u64,
    author: [u8; 32],
}

struct SignedCommit {
    commit: IndexCommit,
    signature: [u8; 64],
}
```

### 9.7 RevocationRecord

Revocation supports whole-artifact removal and fragmentary removal of Entries or Edges within a contribution. Signed artifacts are never mutated; fragmentary revocation applies at read-time projection.

```rust
enum ArtifactKind {
    GraphContribution,
    SplitPackage,
    ValidatorReceipt,
    KeyRotation,
}

enum RevocationTarget {
    WholeArtifact {
        kind: ArtifactKind,
        target_id: [u8; 32],
    },
    Entry {
        contribution_id: [u8; 32],
        content_hash: [u8; 32],
    },
    Edge {
        contribution_id: [u8; 32],
        from: [u8; 32],
        to: [u8; 32],
        kind: EdgeKind,
    },
}

struct RevocationRecord {
    revocation_id: [u8; 32],
    community_id: [u8; 32],
    target: RevocationTarget,
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

Revocation of an Entry within a contribution `MUST` also suppress (at projection time) all Edges in that contribution touching that Entry. Consumers compute this closure when assembling the active accepted set.

At high revocation ratios within a single contribution, communities `MAY`
choose to issue `WholeArtifact` revocation rather than many fragmentary
revocations. This is an operational heuristic; the protocol does not define a
threshold.

### 9.8 ValidatorReceipt

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

Validator receipts are optional in v0.1.

## 10. Attestation and Aggregation

Community-level state is the aggregation of accepted contributions after filter and revocation are applied.

### 10.1 Active Accepted Set

For a given community, the active accepted set `MUST` be computed as:

1. Start with all contributions referenced by commits reachable from `preferred_head`.
2. Drop contributions whose `WholeArtifact` revocation is reachable from `preferred_head`.
3. For each remaining contribution, apply `FilterPolicy`:
   - drop the contribution if contributor is in `contributor_blocklist` or contribution exceeds size limits
   - drop Entries that match blocklist rules or fail allowlist; drop all Edges touching those Entries
   - drop Edges that match `edge_kind_blocklist` or whose `source_protocol` is blocked
4. Apply fragmentary revocations: drop specifically-revoked Entries (and Edges touching them), drop specifically-revoked Edges.
5. The surviving Entry and Edge records are the active accepted set.

For contributor-equivalence counting, identities connected by an accepted
linear `KeyRotation` chain are treated as one contributor. Rotated-and-accepted
keys do not produce separate attestation counts.

### 10.2 Entry Aggregation

For each unique `content_hash` in the active accepted set:

- **attestation count**: distinct contributors attesting this Entry
- **trust-weighted attestation**: sum of per-contributor trust over attestations
- **first-seen / last-seen buckets**
- **UDC tag consensus**: weighted union, preferring `UdcSource::Explicit` attestations

Multiple contributions from the same contributor count as one attestation.
Contributions signed by keys linked through an accepted `KeyRotation` chain are
treated as coming from that same contributor for counting purposes.

### 10.3 Content Equivalence

Implementations `MAY` derive a read-time content-equivalence view keyed by
`content_only_hash`.

For each unique `content_only_hash` in the active accepted set, implementations
`MAY` compute:

- the set of distinct `content_hash` identities carrying that content,
- the set of distinct URLs carrying that content,
- distinct-contributor attestation counts across all matching Entries,
- trust-weighted corroboration across those Entries,
- first-seen / last-seen buckets for the equivalence class.

This is an equivalence view, not a merge rule. Content equivalence `MUST NOT`
erase per-Entry identity, provenance, or trust state. Ranking remains per
Entry unless an implementation explicitly introduces content-cluster-aware
presentation at query time.

### 10.4 Edge Aggregation

For each unique `(from, to, kind)` tuple (with protocol-specific extensions compared per the profile):

- **attestation count**: distinct contributors attesting this structural edge
- **trust-weighted attestation**
- **aggregate traversal count**: optional; sum of per-contribution counts, trust-weighted
- **first-seen / last-seen buckets**

Edge attestation is the primary check on structural manipulation: a forged edge lacks corroboration from other contributors who fetched the source Entry and derived its structural links.

### 10.5 Aggregation is Read-Time

Aggregation `MUST NOT` produce a new signed artifact. It is a local computation over the active accepted set, governed by the profile. This preserves the append-only artifact model.

## 11. Community State, Manifest Governance, and Head Selection

### 11.1 Manifest Updates vs. Head Advancement

VGCP separates two kinds of authority:

- **high-frequency operational authority**: advancing `preferred_head`, bulk
    operational moderation, and other day-to-day workflow handled by
    `admin_keys` and `moderator_keys`,
- **low-frequency governance authority**: changing the manifest itself,
    including `ranking_weights`, `filter_policy`, `accepted_profiles`, and future
    successors, handled by attestations satisfying `manifest_governance`.

Rules:

- the authoritative active head is `CommunityManifest.preferred_head`,
- only `admin_keys` `MUST` be allowed to advance `preferred_head`,
- `moderator_keys` handle per-artifact operational decisions such as receipts,
    revocations, and moderation workflows,
- manifest updates are authorized only by attestations satisfying the previous
    manifest's `manifest_governance`,
- `admin_keys` do not bypass manifest governance,
- consumers `MUST` apply only commits reachable from the latest accepted
    manifest's `preferred_head` when computing the active accepted set,
- consumers `MAY` maintain local provisional heads for staging; such heads
    `MUST NOT` be treated as canonical unless published through accepted manifest
    state,
- `head_epoch` `MUST` increase monotonically when `preferred_head` changes.

This keeps operational head movement lightweight without collapsing governance
authority into admin-only control.

### 11.2 Manifest Update Side Effects

Manifest updates are not merely descriptive; some changes have required local
side effects.

- changing `ranking_weights` invalidates query caches and ranking-derived
    materializations,
- changing `filter_policy` requires recomputing the active accepted set,
- changing `accepted_profiles` requires unmounting any `SplitPackage` whose
    `profile_hash` no longer matches an accepted profile,
- changing `supported_protocols` may likewise remove previously-mounted data
    from the active accepted set.

Admin-only head advancement does not authorize these semantic changes; they flow
through the manifest update path.

## 12. Admission, Revocation, Filtering, and Profile Migration

- Communities `MUST` define an invite or access policy.
- Communities `MAY` apply additional admission checks using validator receipts, trust scores, or local moderation rules.
- Filter policy applies categorically to all contributions at read time; revocation applies to specific targeted artifacts or fragments.
- Revoked artifacts and fragments `MUST NOT` appear in the active accepted set after the relevant `RevocationRecord` becomes reachable from the preferred head.
- Communities `SHOULD` use `accepted_profiles` to support controlled profile migration.
- During migration, communities `MAY` accept contributions from more than one profile, but `SplitPackage` mounting still requires exact binary compatibility with one accepted profile.

## 13. Trust and Ranking

Trust is community-local and implementation-defined. VGCP assumes these inputs:

- invite lineage
- endorsements and flags
- identity age
- attestation density (independent corroboration of the contributor's Entries and Edges)
- utilization in local search
- novelty relative to existing community content

Implementations `MAY` compute trust using EigenTrust-like or other graph-based algorithms.

### 13.1 Structural Manipulation

Because Edges are structural rather than behavioral, most manipulation vectors collapse into verifiability problems:

- **edge forgery**: a forged `Link` between real Entries A and B is detectable — any contributor fetching and canonicalizing A independently derives its real link set. Forged edges fail to accumulate corroborating attestations.
- **contribution stuffing**: padding contributions with junk Entries to inflate attestation counts is bounded by `max_entries_per_contribution` in `FilterPolicy`.
- **coordinated attestation**: colluding contributors can still mutually corroborate forged edges by all serving falsified content. Mitigation is social (trust graph structure) and reputational (low-trust clusters discount their own corroboration).

Recommended enforcement:

- trust gates `SplitPackage` mounting and optional contribution admission
- high `edge_support` in ranking requires attestation from ≥ N distinct trusted contributors
- raw counts are never used; aggregation is trust-weighted throughout

### 13.2 Query-Time Ranking

Query-time ranking combines textual score with UDC relevance (bonused when explicit), trust, freshness, novelty, edge-support, and structural centrality.

## 14. Search Semantics

Search execution `MUST` be local over the consumer's active accepted set.

1. Scope the query to one or more communities.
2. Gather contributions in the active accepted set.
3. Query across the local search engine's readers; compute Entry and Edge aggregations.
4. Deduplicate exact Entry-identity matches by `content_hash`.
5. Optionally cluster exact cross-URL matches by `content_only_hash`.
6. Cluster near-duplicates via MinHash.
7. Rank with community-defined weights.
8. Return results with provenance.

Composite score:

```text
score = w_bm25               * bm25
      + w_udc                * udc_match
      + w_udc_explicit_bonus * (udc_source == Explicit ? 1 : 0)
      + w_trust              * contributor_trust
      + w_fresh              * freshness_decay(observed_at)
      + w_novelty            * novelty_vs_community
      + w_edge_support       * edge_attestation_weight
      + w_centrality         * graph_centrality
```

### 14.1 Graph-Aware Queries

Implementations `MAY` expose graph-shaped queries beyond keyword search:

- "Entries reachable from X within N Edges of a given kind"
- "Entries with high betweenness centrality under UDC scope S"
- "Edges with attestation count ≥ K"
- "Entries with identical content across multiple URLs"
- "Cross-protocol references: Entries in one protocol referencing Entries in another"

Standardization deferred.

## 15. Privacy Baseline

- Raw capture data `SHOULD` remain local by default.
- Shared artifacts `SHOULD` prefer derived search and graph data over raw payloads.
- The projection rule (Section 5.1) is the structural privacy boundary.

### 15.1 Structural Privacy Considerations

Graph-shaped contributions carry more information than isolated observations:

- subgraph shape can be fingerprinting; sufficiently unique structure may be attributable even under pseudonymous identities.
- including optional behavioral traversal metadata compounds this.

Contributors `SHOULD` be able to choose contribution richness: orphans only, structural-only (no traversal metadata), or structural-plus-behavioral. Contributors preferring minimum disclosure can contribute zero-edge contributions and still participate fully.

Query privacy is deferred.

## 16. Protocol Support

VGCP is protocol-neutral at the core. `EntryRecord.protocol` and `EdgeRecord.source_protocol` are canonical lowercase strings corresponding to URI schemes. A community's `CommunityManifest.supported_protocols` declares which it admits; contributions using other protocols are filtered at aggregation.

### 16.1 Expected Protocol Families

VGCP anticipates support for, at minimum:

- **HTTP / HTTPS**: the high-ceremony case. Rich `Link` metadata (region, rel), extensive `Include` semantics, HTTP-status `Redirect`s, HTML-specific `Reference`s (canonical, alternate, prev/next, hreflang). Most complex canonicalization.
- **Gemini**: line-based gemtext. Links are `=> URL [label]` on their own lines. No inline linkage; no transclusion; no `Reference`s in the HTML sense. `Link` edges with a `label` extension are sufficient for most contributions. Simple, clean canonicalization.
- **Gopher / Gopher+**: menu-structured with typed items. Menu entries project to `Link` edges with an `item_type` extension. Gopher+ adds metadata views; canonicalization profile chooses which views to include.
- **Scroll**: notable for native UDC incorporation. Canonicalization `SHOULD` extract UDC tags directly from document metadata, setting `UdcSource::Explicit`. `Link` semantics resemble Gemini's; communities standardizing on Scroll benefit from high-quality classification without classifier inference.
- **Spartan**: closely related to Gemini; similar canonicalization shape.
- **Nex**, **Text**, **SuperText**: minimalist text-first protocols. `Link` extraction depends on protocol specifics; generally a reduced `Link`-only edge model.
- **Mercury**, **Scorpion**, **Guppy**, **Molerat**, **Terse**: smolweb protocols with varying linkage models. Each requires its own canonicalization profile entries; the `EdgeKind` model should cover them without new kinds.

### 16.2 Protocols with Sparse Graph Structure

Some protocols fit the resource-reference-graph model, but naturally produce
sparser graphs. That is not a protocol defect; it is often exactly what their
native use case implies.

- **Finger**: a query-response protocol for user information. Finger URLs identify people, not documents; linkage between Finger resources is not a standard feature. Finger Entries are therefore usually orphans, but that is still useful for people-indexing communities. Sparse graphs are fine when the community goal is directory-style person discovery rather than dense web mapping.
- **FSP**: a file distribution protocol. Files can be Entries, and directory structure yields a small but meaningful set of `Link`-like containment relationships between directories and files. Sparse graph structure is still useful for file-archive communities.

Communities `MAY` admit these protocols for completeness even when the resulting graph is sparse.

### 16.3 Cross-Protocol References

An edge `MAY` cross protocol families — a Scroll page linking to a Gemini capsule, an HTML page linking to a Gopher menu, and so on. The `source_protocol` of the Edge is the protocol of the source Entry; the target's protocol is implicit in the target's URL. Cross-protocol edges are structurally the same as within-protocol edges and participate identically in aggregation and ranking.

## 17. Reference Profiles

### 17.1 Profile A: Iroh-First Trusted Exchange

- blob transfer: `iroh-blobs`
- community announcements: `iroh-gossip`
- replicated community state: `iroh-docs`
- connectivity and relays: `iroh`

### 17.2 Profile B: Broader Discovery Overlay

Public discovery overlays such as libp2p-based routing may be added later. Not required for v0.1 conformance.

## 18. Graphshell Implementation Mapping

- [../../../mods/native/verse/mod.rs](../../../mods/native/verse/mod.rs) *(historical citation)* <!-- doc-audit: historical-link -->: Ed25519-backed iroh identity, trusted-peer storage, workspace-grant concepts usable for community identity and admission.
- [../../../model/archive.rs](../../../model/archive.rs) *(historical citation)* <!-- doc-audit: historical-link -->: signed portable archive objects and privacy classes; useful precedent for signed envelopes.
- [../../../app/clip_capture.rs](../../../app/clip_capture.rs) *(historical citation)* <!-- doc-audit: historical-link -->: structured capture data; a precursor to canonicalization.
- [../../../services/query/mod.rs](../../../services/query/mod.rs) *(historical citation)* <!-- doc-audit: historical-link -->, [../../../services/facts/mod.rs](../../../services/facts/mod.rs) *(historical citation)* <!-- doc-audit: historical-link -->: local structured querying over projected history facts; no distributed contribution support yet.

Gaps before Graphshell can claim a VGCP implementation:

- Entry/Visit/Owner substrate implementation (history-core port from atlas-engineer/history-tree)
- protocol-aware canonicalization profile machinery, starting with HTTPS and Gemini
- BLAKE3 and MinHash fingerprinting over canonicalized Entries
- structural edge extraction per supported protocol
- GraphContribution assembly with privacy-class filtering
- optional traversal-metadata enrichment from local Visits
- attestation-aware aggregation with FilterPolicy and fragmentary-revocation application
- Tantivy or equivalent split-package production and mounting
- community manifest replication beyond bilateral sync
- commit and revocation application logic
- trust-graph computation for admission and mount-time gating

## 19. Rollout Order

1. Entry/Visit/Owner substrate in Graphshell (precondition; tracked separately).
2. GraphContribution serialization, canonical CBOR, per-contribution signing.
3. Canonicalization profiles for HTTPS; BLAKE3 and MinHash generation.
4. Structural edge extraction for HTTPS (Link with region/rel, Include, Redirect, Reference).
5. Two-peer contribution exchange using a reference transport. (Stress-test the wire format early.)
6. Local-only community manifest, preferred-head handling, FilterPolicy application.
7. Local graph-aware search across active accepted set.
8. Fragmentary revocation support.
9. Second protocol: Gemini canonicalization and edge extraction. Validates multi-protocol design.
10. SplitPackage production and mounting.
11. Attestation-aware aggregation and ranking.
12. Community trust and optional validator receipts.
13. Scroll canonicalization with explicit-UDC extraction.
14. Additional smolweb protocols as needed.
15. Broader discovery overlays and advanced privacy features.

## 20. Deferred Work

- Decentralized head selection (replacing admin-key-only head advancement as the sole operational mechanism).
- Batch-signing extensions for large contributions.
- Private community discovery with existence-hiding properties.
- Advanced query privacy (oblivious search, client-side filtering).
- Mandatory public discovery overlays.
- Cross-community search federation.
- Raw snapshot redistribution by default.
- Standardized graph-shaped query extension.
- **HTML Link context granularity**: whether coarse `region` is adequate or DOM-path / CSS-selector context should be preserved in `profile_extensions`. Accommodated by the current extension structure; revisit in a future profile revision.
- **Smolweb protocol profiles**: community-maintained canonicalization profiles for Gemini, Gopher, Scroll, Spartan, Nex, Mercury, Scorpion, Text, Guppy, Molerat, Terse, FSP, SuperText, and others. Protocol support is ordered by community demand.
- **Storage economics**: time-bank model, tokenized storage receipts, contextual credit for hosting, threshold-gated issuance tied to round-trip network work. GraphContribution is the intended substrate for staking and governance; economics spec is separate.
- **Governance**: staking-based privilege assignment within Verses, Verse-level budget allocation of staked storage, hosted social primitives (Nostr NIPs, Matrix rooms, etc.) as community-chosen layers atop VGCP.

## 21. References

- 2026-04-17_verse_distributed_index_protocol_v0.1.md (superseded)
- 2026-04-16_verse_index_protocol_drafts.md
- VERSE_AS_NETWORK.md
- 2026-02-23_verse_tier2_architecture.md
- atlas-engineer/history-tree (upstream library for Entry/Visit/Owner substrate)
- 2026-04-17_graph_memory_architecture_note.md (Graphshell-specific memory architecture note)
