# The kernel as a statement store

Status: **canonical model.** The conceptual spine under the per-statement-edge
decision in the petgraph-RDF plan. It changes no code by itself; it names the model
several plans already assume, and sets out the lifecycle / GC behavior and the
personal-corpus value that follow from it.

Cross-refs:

- [petgraph_rdf_plan](../implementation_strategy/2026-06-18_petgraph_rdf_plan.md)
  — statement records inside pair-local edge buckets; the Mere RDF projection
  profile.
- [statements_over_schema_stance](2026-05-22_statements_over_schema_stance.md) — the
  open-predicate model this completes.
- [two_natured_kernel_brief](../research/2026-05-30_two_natured_kernel_brief.md) —
  content-authoritative / experience-derived; one authority, derived projections.
- [rdf_native_kernel_feasibility](../research/2026-06-18_rdf_native_kernel_feasibility.md)
  — the benchmark behind "petgraph is the runtime, never a triple store on the hot path."
- [memory_tiers_brief](../research/2026-05-11_memory_tiers_brief.md);
  [alembic_memory_and_engrams](2026-06-09_alembic_memory_and_engrams.md) — short-term
  vs long-term memory; the alembic model + the armillary distillation daemon.
- [geist_models_brief](../research/2026-05-10_geist_models_brief.md) — personal LoRA
  adapters as engrams; the corpus as grounding for personal intelligence.

---

## Every edge family is a statement

The kernel's edge families (semantic relations, traversal, arrangement,
containment, provenance, couplings) are all statements. A statement is
`(subject, predicate, object)` with metadata; the predicate carries the meaning,
and the meaning determines the ramifications along four axes:

- **nature**: content (inside the RDF projection profile) or experience (native only);
- **behavior**: what it triggers (a semantic relation, a physics spring, a
  navigation record, a layout hint);
- **durability**: how long it lives (see lifecycle below);
- **projectability**: whether it leaves the machine as standards RDF.

This is the open-predicate stance taken to its conclusion: the kernel is a statement
store, and predicate semantics live in a registry rather than in hardcoded family
enums. That dissolves the expression problem the two-natured brief flagged (adding a
relation becomes registering a predicate, not a shotgun edit across closed enums).

## The dividing line: recorded fact vs derived state

Not everything in the kernel is a statement. The line is recorded fact vs
derived/live state:

- A **statement** is a recorded fact that accumulates: a semantic relation, a
  navigation event, an arrangement pin, a classification.
- A **derived value** is computed each frame and never stored as a statement: a
  node's live position, velocity, selection, focus, materialization, a gyre body.

So the kernel is a statement store of recorded facts (content plus durable
experience) alongside derived runtime projections that are recomputed, not recorded.
That separation is what keeps the hot path fast: the frame loop reads derived typed
indices, not a triple log. The petgraph index is one such derived structure, and the
feasibility benchmark is why petgraph stays the runtime and a triple store never sits
on the hot path.

A single gesture fans out across the line. "Clicked a hyperlink on A's page to open B
in a new tile" yields:

- **A `hyperlink` B** — a content statement (durable, in the profile);
- **navigation A→B via click at T** — an experience-event statement (accumulates);
- **B reached-via that gesture** — provenance (a statement about the navigation);
- **B's tile position** — derived, gyre-computed, not a statement.

One act, three statements of different families plus one derived value.

## Per-predicate lifecycle is the GC and compaction model

Because statements accumulate (every navigation is an event), the set is a
garbage-collection and compaction concern. The concern is bounded to the
event/experience statements; the content graph is bounded by graph size and is kept.
The kernel already has the bones of this, scattered across the families:

- `EdgeMetrics` keeps durable traversal aggregates even when the rolling-window
  records are evicted (edge_data.rs:38) — rollup GC for navigation events;
  `collect_node_traversals` returns `DissolvedTraversalRecord`s (edge_ops.rs:149),
  where dissolution is the eviction step.
- `RelationDurability` is `Durable` or `Session` (edge_taxonomy.rs:197) — session-
  scoped statements collect at session end.
- `ClassificationStatus` is `Suggested` / `Accepted` / `Rejected` (types.rs) — agent
  proposals carry an accept/reject lifecycle, so rejected ones are prunable.
- `remove_node` and the removal API (graph/mod.rs:353) give explicit deletion.
- eidetic plus armillary are the memory-tier version: raw short-term accumulation
  distilled into durable long-term engrams (memory_tiers_brief; alembic doc). That is
  compaction one tier up.

The statement lens unifies these into one idea: **every statement-type declares a
lifecycle policy keyed by its predicate** (durable-keep, session-drop,
rollup-to-aggregate, prune-if-rejected, distill-to-engram), and GC/compaction applies
those policies. Today the policies are denormalized into per-family fields; the model
makes statement lifecycle a first-class, per-predicate concern.

Three subtleties:

- **Referential integrity**: collecting a statement must collect its reifier /
  provenance (the statements about it), or annotations dangle.
- **Federation makes GC a retraction, not a delete**: a statement shared into a moot
  cannot be unilaterally deleted; its collection is a tombstone other peers can see.
  Local-only statements collect freely; shared ones are CRDT-shaped. This is one
  reason the profile keeps provenance: you need a statement's reach before you can
  collect it.
- **Term-dictionary compaction**: the interned-slotmap endgame inherits the
  monotonic-id-never-freed problem; reclaiming id space needs a real compaction pass,
  not just dropping memory.

## The corpus as an eidetic dataset

The accumulated statements across browsing, local media, documents, and notes become
a private, queryable personal corpus. The value is specific:

- **Cross-silo joins** no single tool gives today: "everything I read, saved, or
  noted about X" across web, local files, and notes, unified by relation.
- **A temporal record**: statements are timestamped recorded facts, so "what did I
  believe about X six months ago" and "what changed my mind" become answerable.
- **Provenance**: every statement carries where it came from and who asserted it.
- **Grounding for personal intelligence**: retrieval and reasoning over a typed,
  provenanced, temporal graph, richer than retrieval over a pile of text, with
  citeable sources and recency. This is the substrate the embeddings layer and the
  personal LoRA/engram thread (geist_models_brief) point at.

Honest bounds, so the model is not oversold:

- The value lives in the **asserted and distilled layers, not the raw log.** The
  event firehose is mostly chaff and is the GC target; the asset is what you (or an
  agent) asserted plus the engram distillate. eidetic/armillary exist to turn the
  firehose into that asset.
- It is **back-loaded**: close to worthless on day one, compounding over months. A
  moat and a cold-start problem at once.
- It is **quality-gated and UX-gated**: cross-silo queries are only as good as
  statement fidelity, and the value is unlocked by a humane query surface
  (natural-language-to-query, semantic search, the orrery as a spatial query
  interface), not raw SPARQL.
- The personal-knowledge-graph lineage has a graveyard (capture cost exceeding
  retrieval payoff). Mere's bet against it: auto-capture lowers capture cost, spatial
  and local-AI retrieval lower retrieval cost, and one graph across all silos makes
  the payoff broader than notes-only or browsing-only tools. Whether that overcomes
  the capture/payoff problem is the product bet, not a settled fact.

## Implementation stance

The lens is conceptual; the storage stays typed and compact. Statements are
statement records inside pair-local `EdgePayload` buckets (and typed node
properties) with family-typed payloads, not a generic dynamically-typed triple log.
Predicate semantics (nature, behavior, durability, projectability) live in a registry.
The RDF projection
(petgraph_rdf_plan) is how content statements leave as standards RDF; the lifecycle
policies are how experience statements stay bounded; eidetic/armillary are where the
distillate lives.

## What this does not settle

- The predicate registry's concrete shape (where nature / behavior / durability /
  projectability are declared) is not designed here.
- The lifecycle policy table (which predicate gets which policy) is named, not
  enumerated.
- Federation retraction semantics (tombstones, CRDT shape) are federation-tier work,
  flagged not solved.
