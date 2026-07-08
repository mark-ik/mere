# Generic Graph Substrate Plan

**Date:** 2026-07-08
**Status:** planning. Decisions below locked with Mark 2026-07-08; the substrate
crate name is the one open pick. Home is mere's design_docs because mere is the
donor of the model and the largest eventual consumer, but the substrate itself
will be a standalone repo (the muniment/codicil pattern), not a mere crate.

## 1. Decisions locked

1. **Fully generic core.** The substrate is `Graph<N, E>`: no mandated container
   struct, capability traits for interop, provided default payloads for apps
   that do not need bespoke ones. Mere's web node becomes one instantiation.
2. **Fresh minimal core, mere re-bases last.** Build small, validate on a simple
   consumer, harvest mere's mature machinery piece by piece (RDF projection
   above all), and re-base mere at the end. Not an in-place generalization of
   the 14.8k-LOC concrete model.
3. **One history spine.** codicil is the graph-level authority (an ordered log
   of edits); snapshots are muniment-stored materializations (compaction, not a
   second history); node-level lineage is a projection over the spine, promoted
   from `node-lineage`. Fork/copy/duplicate handling flows from lineage's model
   into codicil's roadmap.
4. **Names (locked 2026-07-08).** The substrate is **chartulary**, aliased
   `chart` in consumer workspaces (`chart = { package = "chartulary" }`, the
   tinct/tincture pattern in reverse) and called "chart" colloquially. The
   lineage crate is **stemma**; the RDF projection harvest is **scholia**. All
   three checked free on crates.io; `chart` itself is also free but reads as a
   plotting library on a shelf, so the long form is the published identity
   (section 8).

## 2. The stack

```text
muniment    bytes: content-addressed blobs (node content) + slots (snapshots)
codicil     the ordered edit log (graph-level authority)
substrate   Graph<N, E> on petgraph: ops, filter, capability traits,
            provided Container/Relation defaults
stemma      per-node lineage: a projection over the spine (from node-lineage)
scholia     RDF projection over the semantic ring (from linked-data)  [Tier 2]
analytics   aether / signals / arrangements, retargeted               [Tier 2]
content     notes / tags / lists (the commonplace layer)              [Tier 2]
```

Consumers: isometry (entity/world graph), woodshed (notes, tags, practice
sets), strophe (session relations), mere (the full web graph, at re-base).

## 3. The generic core

- `Graph<N, E>` over petgraph `StableGraph` (stable keys, serde). Typed
  add/remove/query/filter ops; the shape of mere's `graph/` ops without the
  concrete payload.
- **One required bound:** a stable identity on `N` (the `Identified` trait: a
  key that survives serialization, mere's `id: Uuid` generalized).
- **Capability traits, each optional, each unlocking a feature:**
  - `Addressed`: multi-scheme address claims, primary + aliases (gemini, https,
    file, mere, app schemes). Unlocks address lookup and the RDF `@id`.
  - `ContentBearing`: a content reference (muniment `Hash`) + media-type hint.
    Unlocks content-addressed container behavior.
  - `Labeled`: title + tags. Unlocks curated RDF literals (`schema:name`,
    `schema:keywords`).
  - On `E`: `Classified` (family/kind discriminant, for filtering and render
    policy) and `Predicated` (an optional predicate IRI: the edge joins the
    semantic ring and projects to RDF).
- **Provided defaults:** a `Container` struct and a `Relation` type implementing
  the traits, so woodshed or isometry can start without designing payloads.
  Mere skips the defaults and implements the traits on its own `WebNode`.
- **Deliberately not core:** position/velocity (canvas concerns; mere carries
  them in its payload), rendering, physics, RDF (scholia), analytics, and the
  webview-runtime facets (compat mode, session scroll, favicon).

## 4. Relations: two rings

- **The shared semantic ring** (interop): a recognized core (Cites, Supports,
  Contradicts, DependsOn, SameEntityAs, ...) with canonical IRIs and standard
  vocabulary alignment, plus open predicates (any raw IRI passes through
  verbatim and round-trips). Only this ring projects to RDF.
- **App-private families** (experience): registered per app, typed, never
  projected. isometry: Occupies, FacesToward, InInitiativeAfter. strophe:
  TrackContains, OverdubsOnto. woodshed: InPracticeSet, PrecedesInRotation.
  mere: Traversal, Containment, Arrangement, Imported (its current experience
  families, unchanged in meaning).
- **Open design point (settle at G0):** the family registry. mere's precedent
  is a closed enum pair with a `u32` tag encoding (family byte + sub-kind
  ordinal) for transport through layers that cannot see the types. The generic
  form needs an app-registered namespace: candidate shapes are (a) a recognized
  core enum + `(family: interned str, kind: u16)` for app families, keeping a
  compact tag encoding, or (b) fully string-keyed. Bias to (a): mere's tag
  trick is load-bearing for cheap canvas hit-test transport.

## 5. History: one spine, three views

The unification, per Mark's framing: lineage is node-level history, codicil is
graph-level, and they integrate. Refined into mechanism:

- **codicil is the single authority.** Graph mutations are `GraphEdit` entries
  appended to a codicil. The materialized `Graph<N, E>` is `replay(log)`. One
  clarification against the "log of snapshots" phrasing: the codicil logs
  *edits*; snapshots are periodic muniment-stored materializations so load is
  checkpoint + tail-replay rather than full replay (codicil's existing P2
  roadmap). Both exist; the log is the truth, snapshots are an optimization.
- **stemma is a projection, not a second store.** The survey's key finding:
  `node-lineage` is *already generic* (its `EntryIdentityKey` / `OwnerIdentity`
  / `MemoryPayload` are blanket trait bounds; the navigation wording is
  vocabulary, not coupling), so it promotes near-verbatim, the armillary way.
  Its standing rule ("visits own the tree; edges are projected, never stored
  separately") and its R0 invariants (temporal-integrity: append-only, the past
  is never rewritten; replay-isolation: reads never mutate; shared-projection:
  derived views are projections over one authority) become the *whole spine's*
  contract, not just lineage's.
- **Fork / copy / duplicates: lineage's lessons flow into codicil.** From
  mere's cross-graph work (`cross_graph.rs`, `NodeDerivation`,
  `ProvenanceSubKind::CopiedFrom`): forking a graph is forking its log (a new
  codicil whose header records source log id + seq at fork, git-style);
  duplicates across graphs are handled by derivation provenance records, never
  by log deduplication. These land as codicil roadmap items (a fork primitive
  and a provenance header), not as substrate complexity.
- **What retires at mere's re-base:** the bespoke `graph/history.rs` and
  `capture.rs` snapshot machinery re-derives over the spine; the in-tree
  `node-lineage` copy retires in favor of stemma.

## 6. scholia: the RDF projection (Tier 2 harvest)

mere's `linked-data` (3,450 LOC) is mature: expanded/compacted JSON-LD both
ways, N-Quads/TriG, SPARQL via spareval, named-graph scopes, RDF 1.2 reified
statement metadata (label, provenance, assertion time), datatype and language
tags, and 3-category standard-vocabulary alignment. Harvest, do not rewrite.
The port re-seams it from the concrete kernel onto the capability traits
(`Predicated` + `Addressed` + `Labeled` drive the quads). The acceptance bar is
its own existing gate test: the full-profile dataset round-trip
("lossless under the profile"), re-passed over the generic substrate.

## 7. Phases

Done-conditions, not durations.

- **G0: skeleton.** The named substrate repo: `Graph<N, E>`, capability traits,
  default `Container`/`Relation`, the family-registry decision. **Done when** a
  toy graph builds, queries, filters, and serde round-trips on the default
  payloads.
- **G1: the spine.** `GraphEdit<N, E>` entries through codicil; apply/replay;
  muniment snapshot + tail-replay. **Done when** replay(log) == live graph
  holds property-style, and checkpoint + tail load equals full replay.
- **G2: stemma.** Promote node-lineage (rename vocabulary, keep the machinery
  and R0 contract), wire as a projection fed by the spine; codicil grows the
  fork primitive + provenance header. **Done when** a forked graph carries
  lineage across the fork and derivation records survive round-trip.
- **G3: first consumer.** One app ships real user data on the substrate.
  Candidates: woodshed's notes/tags/practice-set graph (small, low-risk) or
  isometry's entity graph (wanted by the DM lane, but behind isometry's own
  keystones). Pick whichever app is actively in hand when G2 lands. **Done
  when** user-authored content lives in a substrate graph through muniment.
- **G4: scholia.** The linked-data harvest over the trait seam. **Done when**
  the losslessness gate passes over the generic substrate and a cross-app
  triple (a woodshed note cites a mere document) exports as real JSON-LD.
- **G5: mere re-base + analytics.** mere implements the traits on WebNode,
  re-derives history/capture over the spine, retires in-tree node-lineage;
  aether/signals/arrangements retarget onto the substrate and become promotable.
  **Done when** meerkat runs on the substrate graph with no behavior change.

G5 is the long tail and deliberately last; G0 through G4 never block on it.

## 8. The name: chartulary, "chart" for short

**Decided 2026-07-08: chartulary.** The attested variant spelling of cartulary
(cartulary itself is taken on crates.io): the register book into which a house
copied its charters and muniments. The meaning is exact: the thing that
organizes muniments and their relations is literally a chartulary. Considered
and passed over: pandect, matricula (free; less muniment-tied), catena (free;
implies a linear chain, the wrong shape), cartulary/trellis/tela/rete (taken).

**The short form.** `chart` is free on crates.io and is the same root (charta:
the charter, the paper), but published under that name the crate would read as
a plotting library, and it would sit confusingly near mere's `cartography`. So
the published identity is `chartulary`; consumer workspaces alias it
(`chart = { package = "chartulary" }`, the tinct/tincture pattern in reverse)
so code reads `use chart::Graph`, and "chart" is the colloquial name in docs
and conversation.

Family read: muniment (the kept records), codicil (the appended amendment),
chartulary (the register that binds them), stemma (the descent of copies),
scholia (the commentary in the margins).

## 9. Open questions

1. Family-registry mechanics (section 4): compact-tag hybrid vs string-keyed.
2. Edge multiplicity: mere holds one edge per node pair carrying multiple
   statements; petgraph supports true multigraphs. Pick one semantics at G0.
3. Serialization: core is serde-first; mere's snapshots use rkyv. rkyv as an
   optional feature at G1, or mere-side only at G5?
4. Crate granularity: core + defaults in one crate, or a taxonomy split. Bias
   to one crate until a consumer proves the split.
5. Stemma wiring: does the spine feed stemma automatically (every edit emits a
   visit-shaped event) or is wiring consumer-side? Decide at G2 with real use.

(Resolved: the substrate name, section 8.)

## Provenance

Grounded in 2026-07-08 reads of mere's `graph/edge_taxonomy.rs`,
`graph/node.rs`, `graph/identity.rs`, `linked-data/src/lib.rs`, and
`node-lineage/src/lib.rs`, the muniment/codicil founding proposals
(repos/muniment, repos/codicil), and the tiering + decisions conversation with
Mark (fully generic; fresh core, mere last; one history spine; stemma).
