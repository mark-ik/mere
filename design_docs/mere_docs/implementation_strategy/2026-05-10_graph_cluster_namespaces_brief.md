# Graph-Cluster-Derived Namespaces — Design Brief

**Date**: 2026-05-10
**Status**: Proposal (under design)
**Scope**: Defines the §14 direction surfaced in [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md): namespaces in mere derived from the graph's natural community structure rather than admin-imposed paths. Implications for capability scoping (substrate-brief §8.8), sync (substrate-brief §5 Willow / iroh-docs), and the `mere-namespace` crate sketched in the substrate brief.
**Related**:

- [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md) — parent brief; §5 (Willow re-evaluation), §8.8 (capability options), §14 (this direction's seed).
- [`2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — tier framework that namespaces ultimately serve.
- [`../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md`](../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md) — the field algebra producing the graphs being clustered.
- [`../research/2026-05-08_local_intelligence_integration_research.md`](../research/2026-05-08_local_intelligence_integration_research.md) — embeddings/distances feeding cluster algorithms.

---

## 0. The premise

The standard model for capability-scoped namespaces (Willow's meadowcap, UCAN paths, S3 prefix permissions, Unix filesystems) assumes the namespace path is *administratively chosen*. Someone — a system designer, an org admin, a user with a "create folder" intuition — picks a hierarchy and slots data into it. Capability scopes follow that hierarchy: a cap to `/foo/` covers everything under `/foo/`.

Mere's data is a force-directed graph. Nodes have positions, edges, weights. The graph has *natural community structure*: regions of dense internal connectivity that decompose into spatially coherent clusters. These clusters are visible to the user as the layout settles; they are also detectable algorithmically (Louvain, Leiden, label propagation, infomap, etc.).

**Proposal:** the namespace path of a node is its position in the graph's cluster decomposition, computed from the graph itself. Capability scopes follow these data-driven clusters rather than admin-chosen paths. To "share a cluster" with someone is to grant them a capability over the semantically coherent neighborhood that the graph's structure has surfaced — not over an arbitrary directory tree.

This appears novel as a capability-scoping primitive. Standard literature on capability systems (UCAN, biscuits, macaroons, meadowcap, OCaps) all assume admin-chosen scope. Worth pursuing precisely because no one else is.

---

## 1. Why this matters

Three real wins:

**1. Capability scopes carry meaning.** "Alice can read the cluster around the Servo migration thread" is a scope a user can understand and reason about. "Alice can read `/personal/graphs/main/topic/foo/` for 30 days" is a scope only the issuer understands. In a force-directed graph, the visible cluster *is* the unit of meaning; the capability follows that unit.

**2. Sync becomes locality-aware.** Two peers who care about the same cluster can RBSR-fingerprint that cluster's events and skip the rest. Without graph-derived paths, every peer fingerprints the whole space; with them, sync is naturally scoped to the data each peer cares about.

**3. The hierarchy is data-driven, not admin-driven.** Real graphs have community structure that no admin would have predicted. A graph-cluster-derived hierarchy is *correct by definition* — it reflects how the data actually clumps. An admin hierarchy is always a bet about how data *will* clump.

A fourth, more speculative win:

**4. Cluster-paths can serve as a privacy primitive.** A peer that pulls events under one cluster path doesn't leak interest in adjacent clusters. Without hierarchy, every event-set request reveals subscription to the whole space. With cluster-scoped requests, fingerprint exchanges leak only the cluster of interest.

---

## 2. The Willow mapping

Willow's data model is `(NamespaceId, SubspaceId, Path)` where `Path` is a sequence of byte-string components.

| Willow concept | Mere reframe |
|---|---|
| `NamespaceId` | A space's root identity — `Moot(MootId)`, `Personal(MasterPubkey)`, `SharedSession(SessionId)`, `Demesne(DemesneId)` (matches `SpaceId` in the event DAG brief §4) |
| `SubspaceId` | An author identity within the space — usually a persona's pubkey (matches `AuthorId` in the event DAG brief) |
| `Path` | **The node's cluster decomposition: super-cluster → cluster → sub-cluster → … → node-id** |

Each `Path` component is the BLAKE3 of canonical cluster contents (so it's content-addressed; cluster IDs change when membership changes). The leaf component is the node's own ID.

**Concrete example.** A node sits in cluster C₃, which sits inside super-cluster C₁, which sits inside the orrery's root cluster C₀:

```text
NamespaceId    = Personal(mark_master_pubkey)
SubspaceId     = mark_persona_pubkey
Path           = [hash(C₀), hash(C₁), hash(C₃), node_id]
```

Capability scope examples:

- `(Personal(mark), mark_persona, [hash(C₀)])` — cap over Mark's whole graph for himself.
- `(Personal(mark), alice_pubkey, [hash(C₀), hash(C₁), hash(C₃)])` — read cap to Alice for the C₃ cluster.
- `(Moot(servo_migration), *, [hash(rust_subgraph)])` — moot-wide read cap to a topic cluster.

Meadowcap's path-prefix delegation works unmodified: a holder of `[hash(C₀), hash(C₁)]` can delegate any subset of that prefix; sync and capability checks proceed normally.

---

## 3. Cluster computation

The hard part. Three sub-questions: algorithm, determinism, parameters.

### 3.1 Algorithm

Modern community detection algorithms are well-understood:

- **Louvain** (Blondel et al. 2008) — modularity-maximizing, fast, but has a known resolution-limit problem (fails to find small communities in large graphs).
- **Leiden** (Traag et al. 2019) — Louvain successor, fixes the resolution limit, guarantees well-connected communities. Recommended starting point.
- **Label propagation** — fastest, simpler, less stable; good for very large graphs.
- **Infomap** — information-theoretic, captures flow-based community structure. Best when the graph has direction/weight semantics around traversal.
- **Hierarchical clustering** (divisive or agglomerative) — produces a dendrogram directly, not just a flat partition. Aligns with the namespace hierarchy directly.

**Default proposal:** **hierarchical Leiden** (run Leiden recursively on each detected community to produce a multi-level hierarchy). Modularity-resolution-clean, hierarchy-shaped output, well-understood failure modes.

### 3.2 Determinism

Capabilities scope by cluster path, so peers MUST agree on cluster membership for caps to verify. Three options for how peers reach agreement:

**(a) Each peer computes locally.** Simplest, but leads to disagreement: different starting conditions or RNG seeds produce different cluster partitions, and caps under one peer's clustering don't verify under another's. **Not viable** for capability scoping; potentially viable as a pure sync optimization where clustering is just a hint.

**(b) Canonical clustering rule per space.** Each space's constitution declares an algorithm + parameters + RNG seed. Every peer running the same input graph through the same rule gets the same partition. Disagreement only on graph state, which RBSR converges anyway. **Viable.** The cost is fixing the algorithm at space creation; switching algorithms requires a constitutional amendment.

**(c) Clusters as published events.** A designated "clusterer" peer (or a quorum of peers) publishes `ClusterDecomposition` events into the space; other peers index by what's been published. This decouples cluster computation from individual peers. **Most flexible, most coordination overhead.** See §8 for the wire shape.

**Recommendation:** start with (b) for v1 (canonical rule, fixed at space creation). Allow upgrade to (c) for spaces that demand more sophisticated cluster governance.

### 3.3 Parameters

Cluster algorithms have parameters: resolution, minimum cluster size, hierarchy depth, edge weight functions, seed. These need to be part of the canonical rule. The `mere-namespace` crate ships defaults that work for typical mere graphs; spaces can override at creation.

**Configurability is non-negotiable.** Per the workspace `feedback_configurability_over_opinionated_defaults` memory, all algorithm choices and parameters are exposed as space settings. Defaults are opinionated; configurability is required. The full parameter space is tracked in this brief even when v1 ships a narrower slice.

---

## 4. Stability under graph evolution

The hardest design question. Graphs evolve: new nodes appear, edges shift, clusters merge and split. A node that was in cluster C₃ today may be in cluster C₇ tomorrow. What happens to:

- A capability `(Personal(mark), alice, [hash(C₃)])` issued yesterday?
- A sync subscription scoped to C₃?
- An event whose path was `[hash(C₀), hash(C₁), hash(C₃), node_id]` at the time of authorship?

Three resolution strategies:

**(a) Cluster paths are routing hints, not security boundaries.** Caps actually bind to specific node IDs (the leaf component); the cluster prefix is just for sync scoping and human-readable scope summaries. When a node moves clusters, the cap still verifies because the leaf ID is unchanged. **Lowest-friction; cap semantics are stable; sync hints decay gracefully.**

**(b) Caps bind to cluster IDs as they were at issuance.** The cap is over `[hash(C₃-as-of-2026-05-10)]`. Membership in C₃ is recorded as events; verification walks cluster-membership history at the cap's time-of-issuance. **Strict; cap semantics frozen; complex verification.**

**(c) Caps follow nodes through cluster migrations.** A `MembershipChange` event triggers cap re-evaluation: if a node moves from C₃ to C₇, any cap that covered C₃ also covers the migrated node in C₇. **Most semantic; most surprising; hardest to reason about; potential for unintended scope expansion.**

**Recommendation: (a) for v1.** Cluster paths are routing hints + human-readable scope summaries; caps bind to leaf node IDs. This matches how users intuit "I shared this set of nodes with Alice" — the nodes are the unit, the cluster is the *frame*. Strategies (b) and (c) become opt-in for security-sensitive use cases that demand strict cluster semantics.

---

## 5. Algorithmic vs user-defined clusters

Real workflows want both:

- **Algorithm-derived clusters** (canonical) — Louvain/Leiden output, content-addressed by canonical cluster contents. Stable across peers via §3.2(b).
- **User-defined clusters** (overlay) — "my reading list," "Tuesday's meeting prep," "things to revisit." Author-chosen membership, named by the author.

These coexist as parallel namespace dimensions:

```text
Path = [
    canonical: [hash(C₀), hash(C₁), hash(C₃)],   // algorithm-derived
    overlays:  [user_list_id_1, user_list_id_2], // user-chosen
    leaf:      node_id,
]
```

A capability can scope by canonical cluster, by user overlay, or by both. Most user-facing operations route through overlays ("share my reading list"); most automated sync routes through canonical clusters ("subscribe to the rust-graph cluster").

Concrete encoding question — open: do canonical and overlay components live in the same `Path` (with a discriminator byte) or in separate Path-shaped fields? The Willow data model wants one `Path`; multiplexing canonical and overlay components requires either a convention or an extension. v1 likely uses a discriminator byte; revisit if it gets ugly.

---

## 6. Fuzzy / overlapping membership

Real community-detection algorithms produce fuzzy or overlapping memberships: a node belongs to cluster C₃ with weight 0.7 and to C₇ with weight 0.3. RBSR sync wants a totally-ordered key.

Three resolution strategies:

**(a) Pick a primary cluster.** The node's "primary" path is its highest-weight cluster. Other memberships are recorded as separate index events for query but don't affect the canonical sync path. **Simplest.**

**(b) Replicate under all paths.** Each membership causes the node to appear under multiple paths. Sync of any path picks up the node. Storage cost = N × membership-count.

**(c) RBSR over node-id with cluster-paths as secondary indexes.** Canonical sort key is `node_id`; cluster path is a separately-indexed query dimension. Sync converges on node-id space; cluster-scoped queries hit the index.

**Recommendation: (a) for v1.** Keep the path canonical and singular; track secondary memberships as index events. Move to (c) if practice shows secondary memberships are heavily queried.

---

## 7. Capability semantics under cluster paths

Meadowcap's verification logic is unchanged — path-prefix delegation, time bounds, mode (read/write/delegate), recursive delegation. The novelty is that the path prefix has a *user-meaningful* interpretation (a graph cluster) rather than an admin-chosen prefix.

Concrete cap shapes:

- **Read cluster.** "Alice can read everything under C₃ for 30 days." Standard meadowcap read cap with cluster-prefix scope.
- **Write to cluster.** "Bob can add nodes/edges within C₃." Standard write cap; new events must have paths under the cluster prefix.
- **Delegate cluster.** "Carol can grant cluster-C₃ caps to others." Standard delegate cap; cap-issuance events from Carol scoped to C₃.
- **Time-bounded sharing.** "Dan can read C₃ until Friday." Standard time-interval bound on a read cap.
- **Cross-cluster reference cap.** Tricky: an event in C₃ may reference an event in C₇ (cross-cluster edge). Reading the event in C₃ may transitively imply reading the referenced metadata. **Open question:** does following a cross-cluster edge require a separate cap, or does the source-cluster cap propagate to follow-shaped reads? Suggested default: separate cap required; cluster boundaries are real for capability purposes. Listing the open question explicitly because the choice has non-obvious user-experience consequences.

---

## 8. Clusters as published events (deferred-but-sketched)

If we adopt §3.2 strategy (c) — clusters as published events — then cluster computation itself becomes part of the event DAG. This is heavy and not v1, but the wire shape is worth sketching now so v1 doesn't paint into a corner.

```rust
// Illustrative signature only — not implementation-ready (per
// feedback_spec_code_samples_illustrative_vs_implementation_ready).
struct ClusterDecomposition {
    parents: Vec<EventHash>,       // prior decomposition events
    algorithm: AlgorithmId,        // e.g. leiden_v1
    parameters: AlgorithmParams,   // serializable, stable hash
    input_graph_state_hash: Hash,  // BLAKE3 of canonical graph snapshot
    decomposition: BTreeMap<NodeId, ClusterPath>,
    issuer: AuthorId,              // clusterer or quorum
    signature: Ed25519Signature,
}
```

Each `ClusterDecomposition` event references the prior decomposition (so the history of cluster evolution is itself a DAG). Caps over cluster paths can reference *which* decomposition they were issued under, providing strict cap stability without freezing cluster membership in the cap itself — the cap points at the decomposition; the decomposition points at the membership.

This is the right shape for spaces that need cluster-governance as a first-class concern (large mootholds, demesnes with formal cluster admins). Defer until a real space demands it; ship v1 with §3.2(b) canonical rule.

---

## 9. The `mere-namespace` crate (sketch)

Per the substrate brief §5 (revised): a new crate that integrates willow-rs (with `PayloadDigest25` = BLAKE3) over iroh streams, plus graph-cluster path computation.

**Modules** (target files; each <600 LOC per workspace `feedback_mere_file_size_ceiling`):

- `path.rs` — `MerePath`, conversion from graph-cluster-decomposition to Willow `Path`, cluster-id BLAKE3 canonicalization.
- `cluster.rs` — Leiden / Louvain implementations (or an existing crate like `petgraph-community` integration), parameter types, deterministic-RNG handling.
- `space.rs` — `MereSpace` type wrapping Willow's namespace concept; constitution data carrying canonical clustering rule.
- `wgps_iroh.rs` — adapter from willow-rs WGPS sync to iroh streams (~200 LOC estimated).
- `cap.rs` — passthrough re-exports of `meadowcap` types, scoped to `MerePath`-shaped paths (Option A from substrate-brief §8.8).

**Out of scope for v1:**

- Biscuit policy-token wiring for tessera/quota/heartbeat rules
  (deferred to substrate-brief §8.8 Option C eval).
- Keyhive group/key-state integration (deferred to substrate-brief
  §8.8 Option D eval).
- §3.2(c) cluster-as-event governance (deferred until a real space demands it).
- §6(c) secondary-index sync (deferred until query patterns demand it).
- §4(b)/(c) strict cluster-binding cap semantics (deferred until a security-sensitive use case demands it).

**Done condition for v1:**

A single user can stand up a `MereSpace`, add nodes/edges, observe a Leiden decomposition compute deterministically, issue a meadowcap read cap over a cluster prefix, and have a second peer verify the cap and sync the cluster's events over iroh streams.

---

## 10. Open questions

- **Cross-cluster reference caps** (§7). Does following a cross-cluster edge require a separate cap, or does the source-cluster cap propagate?
- **Cluster-rule changes mid-life.** A space may want to upgrade from Leiden v1 to Leiden v2 with new parameters. How is this proposed, ratified, and rolled out without breaking existing caps?
- **Cluster size pathology.** Very small clusters (singletons, pairs) may be artifacts; very large clusters may be uninformative. What's the right minimum/maximum cluster size, and how is it parameterized per space?
- **Algorithm-vs-user overlay precedence in capability scope.** When a user delegates "my reading list," does the cap scope strictly to the overlay or also to canonical clusters that overlap with it?
- **Privacy implications of cluster decomposition itself.** Does the published cluster structure leak who-talks-to-whom information that members didn't intend to expose? May favor §3.2(b) over (c) for privacy-sensitive spaces; may also constrain which algorithms are appropriate for sensitive moots (e.g. infomap reveals more about traversal patterns than Leiden does about membership alone).
- **Graph evolution tempo.** Cluster decomposition is expensive; recomputing on every node insertion is impractical. What's the right batch cadence (every N events? every T seconds? on-demand?), and how do peers stay in sync about which decomposition is current?
- **Path encoding convention.** Canonical + overlay components in one `Path` with a discriminator byte, or as separate Path-shaped fields requiring a Willow extension? §5 calls v1 with discriminator byte; revisit if ugly.
- **Empty-graph and bootstrapping edge cases.** A new space has no graph yet; what's the cluster decomposition? Probably a single root cluster containing all (zero) nodes; new nodes default to root cluster until enough structure exists for Leiden to detect substructure.

---

## 11. First experiments

In rough order of leverage:

1. **Single-graph Leiden in `graphshell`.** Run Leiden on the existing graph view; visualize cluster boundaries in the canvas. Confirms the data has visible community structure worth exploiting.
2. **Hierarchical Leiden.** Recursively cluster within each detected community to produce a multi-level hierarchy. Compare to the user's intuitive sense of "what belongs together."
3. **Cluster-path encoding.** Implement the `MerePath` type with BLAKE3-canonicalized cluster IDs. Round-trip through Willow's `Path` type using `willow_25::PayloadDigest25`.
4. **Single-peer meadowcap over cluster paths.** Issue a read cap over a cluster prefix; verify it admits events whose paths fall under the prefix and rejects others. No sync yet — local verification only.
5. **Two-peer WGPS sync over iroh streams.** Stand up the WGPS-iroh adapter; sync two peers' graph state with cluster-path-keyed events.
6. **Cluster-evolution stability test.** Add and remove edges; observe how clusters shift. Verify §4(a) — caps bound to leaf node IDs survive cluster migrations.

---

## Findings

(Captured during the 2026-05-10 brief-drafting session.)

- The reframe from "namespace path = admin choice" to "namespace path = graph-cluster decomposition" appears novel. No prior art found in capability-systems literature (UCAN, biscuits, macaroons, meadowcap, OCaps) that does this. Worth pursuing precisely because no one else is.
- Willow's data model maps cleanly: `NamespaceId / SubspaceId / Path` absorb mere's space + author + cluster-decomposition without modification. willow-rs is BLAKE3-default and PD-generic (per the substrate brief 2026-05-10 reframe), so the integration cost is the WGPS-iroh adapter (~200 LOC) plus the cluster-path computation, not a hash-substitution fork.
- The hardest design question is §4 (stability under evolution). Strategy (a) — caps bind to leaf nodes; cluster paths are routing hints — is recommended for v1. Stricter semantics are opt-in.
- The second-hardest question is §3.2 (cluster determinism). Strategy (b) — canonical rule per space — is recommended for v1, with §8 (clusters as events) reserved for spaces that need it.
- Configurability is non-negotiable per workspace memory; full parameter space tracked here even when v1 ships a narrower slice.
- Privacy implications of publishing the cluster decomposition itself are a real consideration; may constrain algorithm choice in sensitive moots and may push some spaces toward §3.2(b) over (c) on privacy grounds alone.

---

## Progress

### 2026-05-10

- Brief drafted from the §14 seed in the event-DAG substrate brief.
- DOC_README index update to follow.
