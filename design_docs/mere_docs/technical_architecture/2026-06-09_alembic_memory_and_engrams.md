# Alembic memory + engrams as composable graphs

**Date**: 2026-06-09
**Status**: Architecture decision (design seed). Defines Mere's memory system: the
**Alembic** pane (short-term / long-term memory + distillation), the **Athanor**
distillation daemon, and graph engrams as one composable engram schema.

**Related**:

- [peripheral panes architecture](2026-06-06_peripheral_panes_architecture.md) — the dock/pane catalog Alembic joins; carries the 2026-06-09 Apparatus/Steward axis refinement this doc drove.
- [memory tiers brief](../research/2026-05-11_memory_tiers_brief.md) — the prior two-tier (short/long) partition. This doc refines its vocabulary (see §2).
- [geist models brief](../research/2026-05-10_geist_models_brief.md) — geists as base model + LoRA-adapter engrams; the loras lane. This doc names the geist as one engram schema (§6).
- [system diagnostics + accessibility plan](../implementation_strategy/2026-06-08_system_diagnostics_and_accessibility_plan.md) — Athanor is a first customer of its observability spine; the Apparatus/Steward correction lives there too.
- [composition spine](2026-05-21_mere_composition_spine.md) — graph truth projected into surfaces; the orrery is the live graph's spatial surface, an engram is a frozen one.
- Kernel snapshot: [`graph/snapshot/mod.rs`](../../../crates/graph/graph-kernel/src/graph/snapshot/mod.rs), node fields: [`graph/node.rs`](../../../crates/graph/graph-kernel/src/graph/node.rs).

---

## 1. The one idea

An **engram** is Eidetic's immutable envelope for a schema-typed payload:
`schema`, `content_hash`, privacy, provenance, trust, time bounds, envelope
version, and payload bytes. **Eidetic** is the store/protocol layer that persists,
verifies, indexes, and eventually federates engrams; the engram is the object
inside that layer.

A **graph engram** is one kind of engram: its schema declares that its payload is
a graph snapshot. Clicking into a graph engram opens a graph you can explore, and
graph engrams **compose**: union two sessions' graph snapshots into one graph
engram while retaining the per-member context that says which session each member
came from. The live orrery and a stored graph engram are the same `Graph` type at
two temperatures: the orrery is the mutable working graph, a graph engram is a
frozen, content-addressed snapshot of one.

Around that sit two surfaces:

- **Alembic** is the memory pane: short-term and long-term memory, and the place
  you distil either into engrams.
- **Athanor** is the distillation daemon: the steady background furnace that
  consolidates memory and mints distillates while you work.

The names are the alchemical apparatus they describe: an athanor is the
self-feeding furnace that holds a constant low heat; the alembic is the still that
sits on it. In-UI labels stay plain (Recent, Saved, Engrams); Alembic and Athanor
are product names.

---

## 2. Three levels of memory

This refines the [memory tiers brief](../research/2026-05-11_memory_tiers_brief.md),
which named two tiers and folded "durable but not engram" into a *medium-term*
footnote. Mark's model promotes that middle to a real level and makes the engram a
distinct act on top:

| Level | Lasts | Addressable / shareable | Made how |
| --- | --- | --- | --- |
| **Short-term** | The immediate session; evicted by default | No | Auto-captured from networked content |
| **Long-term** | Across sessions (durable) | Not necessarily | Promotion from short-term, or durable working state |
| **Engram** | Forever (immutable, content-hashed); deletable only | Yes (content-addressed; federation-ready) | Distilled from short-term **or** long-term |

The key correction to the 05-11 brief: **long-term is not the same as engram.**
Some memory is durable yet not addressable or easy to share (a kept working set).
Making it an engram is what buys addressability, integrity, and shareability. So
the consolidation gesture the 05-11 brief describes ("consolidate as engram")
becomes the third level here, not the second.

### Promotion, eviction, deletion

- **Promotion** short to long is an affirmative act. **Tagging or bookmarking a
  node promotes it** (and, by extension, the members you bookmark as a group) from
  short-term to long-term, making it persistent.
- **Eviction** of short-term is the default and is **configurable**: by sessions
  (drop after N sessions) or in real time (drop after N days). Per the
  configurability rule, the default is a user-overridable policy, never a silent
  one.
- **Demotion / deletion** of short-term and long-term is free at will.
- **Engrams are immutable**, so they are never mutated or demoted. They can only
  be **deleted** outright. A "refresh" produces a new engram with a new hash; the
  old one stays until deleted (eidetic R0 temporal-integrity).

This keeps the 05-11 brief's durable properties: local-until-engram is the privacy
boundary (only engrams can leave the persona's scope), and eviction stays
observable and user-overridable.

---

## 3. Graph engrams as composable graph objects

The kernel already supplies the spine, so this is mostly wiring rather than new
types.

**Payload = `GraphSnapshot`.** The kernel serializes a whole `Graph` to a
`GraphSnapshot` via rkyv ([`graph/snapshot/mod.rs`](../../../crates/graph/graph-kernel/src/graph/snapshot/mod.rs):
`to_snapshot` / `from_snapshot`), which is what session-runtime already persists.
A graph engram is:

```text
Engram {
  schema: mere.graph-snapshot/v1,
  payload: rkyv(GraphSnapshot),
  content_hash,
  privacy,
  provenance,
  trust,
  bounds,
  envelope_version,
}
```

The low-level primitive remains `Engram::new`; the first production path should be
a typed helper (`save_graph_engram(...)`) that snapshots/redacts the graph,
serializes it, and writes it through Eidetic with the graph-snapshot schema.

**Per-member context has a storage home, not a full merge policy.** A [`Node`](../../../crates/graph/graph-kernel/src/graph/node.rs)
carries `import_provenance: Vec<NodeImportProvenance>`, plus `tags`,
`classifications`, open `properties`, and `addresses` (Primary + aliases), all
snapshot-durable. Because `import_provenance` is a **`Vec`**, one merged node can
hold several provenance records, so the storage shape can retain multiple source
contexts without a schema change. That does **not** define merge by itself:
composition still needs an explicit reconciliation policy over UUIDs, primary
addresses, aliases, content hashes, import records, tags, classifications, and
edge payloads.

**Freeze / thaw duality with the orrery.** "Save as graph engram" freezes a live `Graph`
(`to_snapshot` + redaction policy + hash + engram envelope). "Open as session" thaws an engram
(`from_snapshot` into a new live graph / window). Browsing an engram is read-only,
so immutability holds; editing forks a thaw. Clicking into an engram reuses the
**orrery** (the graph's spatial surface) at engram scope rather than a second
viewer.

**Content-addressed DAG with leaf payloads.** Not everything is a graph: model
weights, a raw content snapshot, a geist's adapter bytes are leaf payloads. These
are not graph-snapshot payloads; they are engrams with their own schemas that
**nodes inside a graph engram point at** by content hash. Graph engrams reference leaf engrams and
other graph engrams, forming a content-addressed DAG.

**Nesting is the federation receive model.** A node can reference a graph engram,
so its content *is* a subgraph: one glyph at low LOD, its graph when zoomed in. A
peer sharing a graph engram lands it as a node you dive into. Engrams compose and
nest; the result is fractal graphs.

---

## 4. Merge semantics (open decision, Athanor-configurable)

When two graphs union and both touched the same address, there are two faithful
readings, and the node model supports both:

- **Merge by identity, layer the context** — one node, `import_provenance` gets
  both records, tags / classifications / properties union. Most directly serves
  "retain each member's context."
- **Keep distinct, draw a same-as edge** — two nodes joined by an alias /
  `Provenance`-family edge. Right when the same URL meant different things in each
  session.

This is a policy choice, and per Mark it is a **configurable** part of the
distillation process (dedupe by content address vs preserve duplicates), so Athanor
exposes it per-compose rather than hardcoding one. This is the doc's main open
decision; it defines what "compose" means.

---

## 5. Alembic, the pane

A docked peripheral pane (per the [pane architecture](2026-06-06_peripheral_panes_architecture.md)),
authority = the memory store. Sections:

- **Recent** (short-term) — the working set / cache, with the eviction policy
  visible and editable.
- **Saved** (long-term) — durable kept memory: bookmarked / tagged nodes and saved
  groups (nodes, edges, fields). Promotion and demotion live here.
- **Engrams** (distillation) — mint engrams from Recent or Saved, choose the target
  schema, set privacy (local vs shareable). Browse existing engrams; clicking one
  opens its graph in the orrery at engram scope.

Context-binding (the dock contract's optional field) lets Alembic toggle global
("all memory") vs selection-filtered ("memory of this node / subgraph over time").
The filtered mode surfaces a node's refetch history as a version chain, a direct
expression of the immutable-engram model.

---

## 6. The engram schema catalog

An engram is `envelope metadata + schema-typed payload`, and the schema set is open by
design (Eidetic ships `schema_def`, a meta-schema, and JSON-Schema / JSON-LD /
MereNative validators). **Graph-snapshot and geist are only the first two
schemas.** Many more want this substrate:

- **graph-snapshot** (§3) — the composable spine. Facet records and saved
  subgraphs are this schema at different scopes.
- **geist** — an engram that reconstructs an instance of an **agent**: an imprint
  you can cast a new agent from on the same or a similar model. Schema = model ref
  + adapter engram(s) + policy / prompt + memory scope + identity (a `soul.md`-style
  persona file is the inspiration). This sits **above** the
  [geist models brief](../research/2026-05-10_geist_models_brief.md), which already
  makes each LoRA adapter a content-addressed engram and treats LoRA stacking as the
  keystone; this doc names the agent-reconstruction engram that references a base +
  those adapters + policy + scope.
- **model** — dataweights + model info. Already modeled in eidetic
  (`ModelManifest` / `ModelComponents` / `ModelLibrary`).
- **lora** — **crucial, and owed a return pass.** The geist brief covers adapters
  as engrams and the compatibility envelope, but the full lane (building a dataset,
  training / applying a LoRA to a model, the Distillery-as-trainer operation) is the
  part to come back to. Tracked as the highest-value open schema.
- Open-ended: **personas, contacts, skills, themes, layouts**, and more. Each is a
  typed payload that rides the same content-addressed, provenance-stamped,
  federation-ready envelope.

Distillation is therefore: select material, choose a target schema, mint the
engram, set its privacy.

---

## 7. Athanor, the distillation daemon

An [armillary](../../../crates/armillary) actor (the same off-UI-thread
shape as fetch / sync): long-lived, low-priority, continuous. It performs memory
consolidation in the background:

- **Forgetting** — evict expired short-term per the eviction policy. The only place
  blobs are dropped, allowed because eviction is forgetting, not mutation.
- **Facet extraction** — derive facet-record candidates from raw captures and
  surface them in Alembic to keep.
- **Embedding / summary** — the derived/semantic layer, once the embedding lane is
  live.
- **Consolidation** — dedup by `content_hash`, relate version chains, maintain the
  indices.
- **Proposal emission** — emit distillation candidates, GC proposals, facet
  candidates, and engram manifests for the host / memory authority to apply.
  Athanor does not directly mutate graph truth.

Two properties matter:

- **Steady heat is a scheduling policy.** An athanor self-regulates to a constant
  gentle temperature, so the daemon runs throttled at idle, never bursts, and yields
  to foreground browsing.
- **It embodies eidetic R0 shared-projection.** R0 says derived views ("recent",
  caches, indices) are projections over the one engram store, never a second
  authority. Athanor is what maintains those projections, and it stays inside R0 by
  proposing/adding engrams and evicting eligible short-term material, never editing
  existing engrams or directly mutating live graph truth.

The configurable distillation knobs (e.g. dedupe-by-content-address vs
preserve-duplicates, §4) are Athanor's, set in Apparatus (config).

---

## 8. Apparatus / Steward axis (correction this drove)

Surfacing Athanor clarified the Apparatus/Steward boundary, and the correction is
recorded in the [pane architecture](2026-06-06_peripheral_panes_architecture.md)
and the [diagnostics plan](../implementation_strategy/2026-06-08_system_diagnostics_and_accessibility_plan.md):

- **Steward** is the **live** plane: the process / daemon monitor (running actors,
  async jobs, distillation passes in flight) shown as status you can act on.
- **Apparatus** is the **at-rest** plane: system diagnostics (the recorded trace
  read after the fact, health snapshot, invariant violations) **plus** system and
  subsystem config.

The axis is **static/config vs live/actionable**, not the old status-vs-trace. One
observability spine feeds both. Athanor maps cleanly: its live passes go to Steward,
its knobs to Apparatus config, its faults to Apparatus diagnostics. Settings may
fold into Apparatus, become its own pane, or live native in the chrome/shellbar
(open).

---

## 9. The structural event log + the Timeline

Memory has a temporal substrate distinct from engrams: an **append-only log of
graph mutations** (node spawned, node removed, edge asserted / retracted, node
moved), with periodic `GraphSnapshot` checkpoints, plus a parallel stream of
view-intent / projection changes so arrangement is replayable too. This log is
**local short/long-term memory, not engrams**: no distillation, no
content-addressing, no federation. It is the eidetic R0 shared-projection rule a
third time, one append-only log with two projections:

- **Alembic** is the log's *current fold* (the live nodes / facets / groups) plus
  the keep-and-distil layer of §2. About WHAT you have.
- **Timeline** is the log's *historical replay* (fold to a checkpoint, replay
  events to time T) so you can return to a previous incarnation of the graph and
  watch spawns, projections, and deletions play out. About HOW the graph got here.

Grounding (2026-06-09): this substrate is mostly net-new. What exists is snapshots
(`to_snapshot` / `from_snapshot`), navigation history (`graph/history.rs`, which is
per-node browse back/forward over node-lineage, i.e. *where each node went*, not
*how the graph changed*), and a typed mutation vocabulary (`GraphMutation` /
`GraphIntent`, not yet promoted out of `app/intents.rs`). What is missing is the
persisted append-only mutation log; today's persistence is snapshot-based
(`session_graph_store`). The event-sourcing *pattern* is proven in-repo though
(tessera and cable both carry a `log_store`; the p2panda side event-sources graph
ops for federation), so it is a known shape to rebuild locally on `GraphMutation`.

Two consequences:

- **Undo/redo is the same substrate** at single-step granularity (undo reverses one
  event; the Timeline scrubs to an arbitrary T). Build the log once, get both.
- **Timeline to engram is the handoff to Alembic**: scrub to a past incarnation,
  then optionally distil *that* state into an engram (the freeze of §3). The
  Timeline visits the past locally; distillation pins a chosen past durably and
  shareably.

Home: the Timeline is orrery-coupled (it replays the orrery's graph), so it reads
as a scrubber / time-axis over the orrery, not a docked pane like Alembic. Alembic
and Timeline share the log; their surfaces differ.

---

## 10. Open decisions

1. **Merge semantics** (§4) — merge-by-identity-with-layered-context vs
   keep-distinct-with-sameas-edges, exposed as an Athanor per-compose option. Defines
   what "compose" means.
2. **Promote-in-place vs always-distil** — can a captured snapshot be saved to
   long-term as-is (raw bytes retained), or does saving always mean distilling its
   facets and letting the bytes expire? Decides whether short to long is one
   retain-flagged pool or a capture-to-distillate handoff.
3. **Settings placement** (§8) — own pane vs fold into Apparatus vs chrome/shellbar.
4. **The lora lane** (§6) — dataset building + training/applying a LoRA, the
   highest-value schema to detail next.
5. **Event-log shape** (§9) — mutation-log granularity + checkpoint cadence, and
   whether view-intent / projection changes share the log or run as a parallel
   stream the Timeline composes.
6. **Timeline surface** (§9) — orrery scrubber vs its own pane, and how far back
   replay stays cheap before a checkpoint is mandatory.
7. **Graph engram redaction policy** (§3) — whether a graph engram includes node
   thumbnails, favicons, session scroll, form drafts, raw metadata, and other
   potentially private `GraphSnapshot` fields, or whether `save_graph_engram`
   strips them by default and requires explicit inclusion.

---

## Progress

- 2026-06-09: Doc written from the Alembic / Athanor / engrams-as-graphs design
  conversation. Grounded in the live kernel (`GraphSnapshot`, `Node.import_provenance`),
  eidetic (`Engram`, R0, `ModelManifest`), and the existing memory-tiers and geist
  briefs. Drove the Apparatus/Steward axis correction in the pane architecture and
  diagnostics plan. No code yet.
- 2026-06-09: Terminology tightened: an engram is Eidetic's immutable envelope for
  a schema-typed payload; a graph engram is one engram schema whose payload is
  `GraphSnapshot`, not the definition of all engrams. Added the typed
  `save_graph_engram(...)` boundary, clarified that `import_provenance: Vec` is
  storage support rather than the merge algorithm, made Athanor a proposal emitter
  rather than a graph-truth mutator, and recorded redaction policy as an open
  implementation decision.
