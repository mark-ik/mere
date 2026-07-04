# Data-Oriented Doctrine Brief

**Status:** research brief. Names the representational discipline the stack
already follows, grounds it in the code, and answers the "should we build one
literal substrate?" question (no; the doctrine is the unit of reuse, plus a
short list of cheap cross-cutting instruments that are worth sharing).
**Date:** 2026-07-02.
**Scope:** cross-cutting. Companion to the
[substrate/parallelism composition brief](2026-06-21_substrate_parallelism_composition_brief.md)
(which owns the serialization seams this brief leans on) and the
[composition spine](mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md)
(which owns the layer tower itself). The README's "stack's technical
architecture" section is the public one-paragraph form of this brief.

---

## 1. The doctrine

One representational discipline, repeated at every layer of the stack:

1. **Identity is an index, not an address.** Nodes live in tables (arenas);
   handles are copyable ids, never pointers. Nothing outside the table owns
   node data.
2. **Meaning is a kind, not a class.** A discriminant tag selects the arena,
   the semantics, and the interpretation of the payload.
3. **Structure is explicit id-valued edges.** Ordered where semantic (the
   `in` direction: argument lists, child lists). Indexed where derived (the
   `out` direction: reverse maps kept only so invalidation can walk them).
4. **Everything else is kind-dependent data** in a table picked by the kind.
5. **Change is a recorded delta stream** against the tables. The delta log is
   the only write path worth trusting; replay from empty must reproduce the
   tables.
6. **Every downstream layer is an incremental fold over the stream above it.**

Clause 3's asymmetry is load-bearing: only one edge direction is
authoritative. `in` is an array (order and duplicates carry meaning; it is
what you serialize). `out` is a set (it answers only "who do I notify when I
change"; it is an index, derivable from all the `in`s, never edited directly,
never serialized).

## 2. Provenance

The kind / in / out / data node model is Aapo Alasuutari's account of the
automation-system UI data model at Valmet: a directed acyclic dataflow graph
(~10 MiB per instance) whose nodes carry `kind` (semantics), `in` (ordered
input array, duplicates significant), `out` (unordered output set), and
`data` (kind-dependent payload). He cites it as the direct inspiration for
the Nova JavaScript engine's heap design. The same discipline has been
discovered independently at least four times under four names: ECS (games),
the relational model (databases), incremental/dataflow computation
(spreadsheets, build systems, Salsa), and event sourcing. It keeps being
rediscovered because one representational choice pays off five independent
ways (§4).

## 3. The instances already in the stack

Seven, counting two prospective ones. None of them share code; all of them
share the shape.

| # | Instance | Table / id | Kind | Delta stream | Where |
|---|---|---|---|---|---|
| 1 | Nova JS heap | kind-segregated vectors; `Value` = pointer-sized tagged index; compacting GC moves indexes (hence `Bindable`/`GcScope`) | heap-kind tag | GC work lists | `serval:components/script-engine-nova` (fork: mark-ik/nova, `serval-embedder`) |
| 2 | Scripted DOM | `NodeId`-keyed arena, slots never reused, debug doc-tag fence in high bits | `NodeKind` | `DomMutation` stream via `drain_mutations` | `serval:components/serval-scripted-dom` |
| 3 | Box/fragment tree | layout arena over the DOM holding `Arc<ComputedValues>` per node; Taffy traverses via trait impls | display/box kind | `RestyleDamage` per batch | `serval:components/serval-layout` (`box_tree.rs`, `incremental.rs`) |
| 4 | Xilem view layer | retained view tree diffed against app state | view type | diff → `DomMutation`s (eager apply, batch at relayout boundary) | `serval:components/xilem-serval` |
| 5 | netrender Scene | flat op list + font/transform/image palettes keyed by id | `SceneOp` variant | per-frame ops; asset bytes sent once by id; postcard capture/replay | netrender repo; transport in [transfer.rs](../crates/meerkat/src/content/transfer.rs) |
| 6 | Orrery graph | petgraph `StableGraph` (arena-backed, stable indices) | node/edge taxonomy | `history.rs` + two-phase `apply.rs` | [graph-kernel](../crates/graph/graph-kernel/src/graph/) |
| 7a | *prospective:* DocumentScript mutation contract | coarse mutation variants over a flat canonical ABI | variant tag | per-turn batched contract | D-doc §10.2/§10.3 |
| 7b | *prospective:* Nova-wasm | wasm modules/instances/memories as new heap kinds; `externref` = `Value`; one GC over JS + wasm | heap-kind tag | same GC | upstream intent (repo tagline); fork README still lists wasm as unimplemented |

The tower is the second half of the picture: the layers are joined *by* delta
streams. View diff → `DomMutation` → restyle damage → fragment updates →
Scene ops → `ContentUpdate` across the actor boundary. Same shape at every
layer, same shape between layers.

## 4. Why the shape pays (five independent ways)

1. **Performance.** Struct-of-arrays traversals touch dense memory; no
   pointer chasing. Nova's whole pitch.
2. **Ownership without fights.** Ids are `Copy`; the borrow checker fights
   pointer graphs, not `Vec` + index. This is why Rust codebases converge
   here independently of the other four reasons.
3. **Serialization for free.** Tables of ids are position-independent. The
   Scene postcard-encodes in ~40µs and ArrayBuffer-transfers precisely
   because it is already flat (composition brief §5a). A pointer graph needs
   a fixup pass; an arena is its own wire format.
4. **Diffing and incrementality for free.** Deltas against indexes are small,
   comparable, coalescible (`classify`/`coalesce` in serval-layout), and
   replayable (netrender capture/replay, graph history).
5. **Instrumentation for free.** Per-kind memory is `len × size_of` per
   table. Live counts, dirty-set sizes, deltas per frame, palette hit rates
   are integers the tables already hold. A pointer model cannot answer "how
   big is the data model" without a traversal; a table model answers in
   O(kinds). This is how the Valmet "~10 MiB a pop" figure was knowable at
   all.

## 5. The unification question

Should the doctrine be instantiated literally, as one generic substrate crate
(generic kind/in/out/data tables, one delta-log type, one dirty propagator)
that every layer adopts?

**No.** Two reasons.

**The layers are already unified in the two senses that matter.**
Operationally, the boundaries between them are delta streams; that is the
working unification and it already exists (§3's tower). Conceptually, each
layer conforms to the same discipline, which is what makes the stack
learnable and lets patterns transfer between layers. What the layers do not
share is code, and that is correct:

- Each table is shaped by a master the substrate cannot absorb. Nova's
  vectors are shaped by its compacting GC; the DOM arena by Stylo's
  style-sharing cache (it asserts `NodeId` is pointer-sized); the box tree by
  Taffy's traversal and cache traits; the orrery by petgraph's algorithm
  suite. A generic substrate would either leak all these masters' constraints
  into its API or force each layer through an impedance mismatch.
- Genericity here buys abstraction cost without new capability. Every listed
  instance already works. Porting them onto a shared framework is migration
  risk in exchange for uniformity that the doctrine already provides for
  free.

**The unit of reuse is the discipline, not a crate.** When a new layer is
designed (a cache, an index, a protocol payload), the doctrine is the
checklist: what are the kinds, what is the id, which edge direction is
authoritative, what is the delta, what folds over it. The D-doc mutation
contract is the sixth instance because it was recognized as one, not because
it imports a substrate crate.

The literal-substrate sketch stays useful as a *teaching artifact* (the
shape in ~20 lines, illustrative signatures only):

```rust
// Illustrative signatures only, not compile-ready.
struct NodeId(u32);                  // identity is an index
enum Kind { /* one per semantic */ } // meaning is a kind
struct Nodes {
    kind: Vec<Kind>,
    ins:  Vec<Vec<NodeId>>,          // ordered, duplicates allowed: authoritative
    outs: Vec<SmallSet<NodeId>>,     // derived reverse index, never serialized
    // data: one side table per kind
}
enum Delta { Insert(..), Retarget(..), SetData(..), Remove(..) }
// apply() is the only write path and logs every Delta;
// dirty_from() walks outs; eval_order() topo-sorts over ins.
```

If a genuinely new table-graph is ever needed from scratch (no petgraph, no
Stylo, no Taffy master), start from this sketch and flatten `ins` to CSR
(one contiguous `Vec<NodeId>` plus a range per node) once the shape
stabilizes; that is the step that makes the whole model one buffer, hence
serializable and transferable like the Scene.

## 6. Leverage: what is actually worth sharing

Cheap, cross-cutting, and additive. Shared *technique and convention*, not a
shared core.

1. **One wire discipline (already policy).** Flat position-independent
   buffers, postcard, assets deduped by id, transfer over clone. Shared
   toolchain, never shared schema (composition brief §5). Any new
   cross-boundary payload joins this family by default.
2. **Capture/replay as the universal debugging idiom.** netrender has
   `snapshot_postcard`/`replay_postcard`; graph-kernel has history + apply.
   The convention to hold: every delta stream in §3's tower should be
   captureable at its boundary and replayable against a fresh fold. A
   recorded `DomMutation` batch that reproduces a layout bug offline is the
   DOM-layer equivalent of netrender's A2 capture, and the arena model makes
   it nearly free. Done condition per layer: a failing frame can be reduced
   to a serialized delta log plus a table snapshot that replays the failure
   headlessly. Planned 2026-07-02 for the two unrecorded streams:
   [graph_delta_capture_apparatus_stats_plan](mere_docs/implementation_strategy/2026-07-02_graph_delta_capture_apparatus_stats_plan.md)
   (mere: `GraphDelta`) and
   `serval:docs/2026-07-02_dom_mutation_capture_replay_plan.md`
   (serval: `DomMutation`).
3. **Uniform table instrumentation.** A tiny convention (not a framework):
   each arena exposes per-kind row counts and byte sizes, deltas per frame,
   and dirty-set size per propagation. Four integers per layer characterize
   the whole model ("this document's model is 9.8 MiB and a keystroke
   dirties 14 nodes"). Surface in the apparatus panel. This is the
   measurement half of the Valmet story and it costs a method per table.
   Planned 2026-07-02: apparatus surfacing in the graph-delta plan above
   (Phase D); engine-arena stats in the serval plan (Phase 4).
4. **Oracle diffing for parallel implementations.** The box tree kept
   `TaffyTree` as its diff-test oracle until parity. That pattern (old fold
   as oracle, new fold diffed against it over the same delta stream) is the
   standard migration harness anywhere in the tower, because both folds
   consume identical replayable input.
5. **Delta logs as the persistence story.** Where durability is wanted
   (graph history already; eidetic impressions), persist the log, not the
   tables; snapshot as an optimization. The arena model makes snapshots flat
   and the logs small.
6. **Recognize new instances instead of designing from scratch.** Standing
   review question for any new cache/index/payload design: which existing
   instance is this, and what are its kind/in/out/data. If the answer is
   "none, genuinely new," §5's sketch is the starting point.
7. **Dormancy ladder for live executable state.** Item 5's answer (persist the
   log, not the tables) is for *data* models; it does not transfer to a JS
   heap, which holds executable state (closures over live environments,
   bytecode) with no data-shaped mirror to serialize. But real browsers do
   not persist heaps across tab discard either — they persist a small data
   mirror (URL, scroll, form state, history) and re-execute on restore. That
   mirror already exists in this stack, one layer above the heap: the
   compositing snapshot data-URI (dormant surfaces, already named "the
   suspended-tab model" in
   [native_surface_compositing_plan](archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md)),
   the DOM-as-HTML snapshot (item 2's DOM plan), the native cookie/storage
   store, and the session-override replay lane (`SetNodeFormDraft`/
   `SetNodeSessionScroll` in the graph-delta plan). The mirror pattern holds;
   it just applies one altitude above the heap, to the session, not the
   runtime.

   Three dormancy tiers follow, each with a different honesty contract:
   **live** (actor running); **heap-clone suspend** (same process,
   `GcAgent::snapshot_clone`, exact resume — Promise chains and WeakMap
   identity intact); **discarded** (survives restart, thaw re-executes
   against the restored session mirror, and the surface must say so — a
   tier-3 tab that returns must not present as one that never left). Nova's
   index-shaped heap is the tier-2 differentiator: a clone is a handful of
   `Vec`/`SoAVec` memcpys (§4 point 1), not a pointer-graph walk, so many
   suspended-but-warm heaps can plausibly live in one process where a
   pointer-heavy engine could not afford it, pushing tier 3 to fire under
   real memory pressure rather than by default.

   The irreducible residue is host entanglement, not code: `snapshot_clone`
   already clears only each realm's `[[HostDefined]]` slot and refuses to run
   with jobs still queued; bytecode and closures clone as ordinary heap data
   alongside everything else. The one heap-serialization idea that stays
   honest is a controlled-checkpoint snapshot: clone a realm once its
   intrinsics are warmed but before user closures or host references exist —
   the same move V8/SpiderMonkey startup snapshots make, and the one the WPT
   harness already exploits post-`testharness.js`-eval. Its same-process form
   is proven; a cross-restart form is the one heap-serialization project with
   bounded scope, worth keeping named rather than planned — and its payoff
   for scripted-tile spin-up (as opposed to WPT's testharness.js case) is
   unconfirmed until Mere's own scripted tiles are shown to run a
   prelude-shaped cost of their own.

## 7. Anti-goals

- No generic substrate crate; no porting existing layers onto one (§5).
- No editing or serializing a derived `out` index anywhere; if a design needs
  to persist one, the edge direction is misassigned.
- No new cross-boundary payload formats outside the flat-buffer family
  without a stated reason.

---

## Grounding / links

- [substrate_parallelism_composition_brief](2026-06-21_substrate_parallelism_composition_brief.md)
  (the serialization seams, §5/§5a measurements, the flat-Scene transfer path).
- [mere_composition_spine](mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md)
  (the layer tower: kernel → forme → platen → surfaces → host).
- [document_script_substrate_plan](archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md)
  (D-doc §10.2/§10.3: the per-turn batched mutation contract, instance 7a).
- serval repo: `components/script-engine-nova/lib.rs` (data-oriented `Value`,
  reflector bridge), `components/serval-scripted-dom/lib.rs` (NodeId arena,
  mutation stream, doc-tag fence), `components/serval-layout/box_tree.rs` +
  `incremental.rs` (layout arena, classify/coalesce, paint-only skip),
  `components/xilem-serval/src/lib.rs` (view diff → DomMutation).
- §6 item 7 (dormancy ladder): nova_vm
  `ecmascript/execution/agent.rs` (`GcAgent::snapshot_clone`,
  `Agent::clone_for_snapshot`) and `heap.rs` (`Heap::clone`) for the
  same-process primitive; serval `components/script-engine-nova/lib.rs`
  (`NovaEngine::snapshot_clone`) and `components/script-runtime-api/lib.rs`
  (`Runtime::snapshot_clone`) for the embedding wrapper; serval
  `docs/2026-06-24_wpt_harness_exactness_plan.md` §H2 for the
  checkpoint-snapshot precedent (clone posted after `testharness.js` eval);
  [native_surface_compositing_plan](archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md)
  for the pre-existing visual-layer "suspended-tab model" the ladder extends.
- Nova provenance: [What is the Nova JavaScript engine?](https://trynova.dev/blog/what-is-the-nova-javascript-engine),
  [Web Engines Hackfest 2024 slides](https://webengineshackfest.org/2024/slides/nova_javascript_engine_exploring_a_data-oriented_engine_design_by_aapo_alasuutari.pdf).
  Wasm intent: upstream repo tagline ("A JavaScript and WebAssembly engine
  written in Rust"); the serval-embedder fork README still lists wasm
  execution as unimplemented.
