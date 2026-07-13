# Two-natured kernel: content truth and experience state

**Date**: 2026-05-30
**Status**: Research brief. Direction #3 from the kernel-design discussion (is the
graph-kernel one-natured or two?), researched with two external fan-outs (Nova;
ECS / relational / two-store prior art) and grounded against the live code. No
code change is proposed here; this frames the question, the prior art, and a
staged path. **Reconciled the same day with the
[field-system decision](../technical_architecture/2026-05-30_field_system_extraction.md)**,
which lands a third kernel primitive and the `aether` / `gyre` naming this brief
adopts (see §5).
**Scope**: The graph-kernel's data-model shape. Whether the content/world
relations and the experience/workspace relations want one engine or two, what the
prior art says about doing that cleanly, and how genet-as-host (and the Nova
engine already inside it) bears on the answer.
**Related**:

- [statements-over-schema stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md)
  — the open-statement substrate and the world/experience cut this brief carries
  down to the engine layer.
- [field-system extraction](../technical_architecture/2026-05-30_field_system_extraction.md)
  — same-day decision: fields as a third kernel primitive, `Coupling` as a new
  relation family, and the `aether` (field algebra) / `gyre` (rapier integrator)
  rename. Reconciled in §5.
- [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md)
  — "graph truth, projected into surfaces"; the authority asymmetry this brief
  leans on (§14.1 pin/save write-back).
- [genet-as-host eval](../technical_architecture/2026-05-29_genet_as_host_evaluation.md)
  — §6 orrery-as-element; the forcing function for naming the experience core.
- [cartography-aether seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
  — the rapier substrate (`gyre`, the crate formerly named `aether`) as a
  kernel-tier physics layer that speaks kernel types only.
- Code: [`graph-kernel/.../edge_taxonomy.rs`](../../../crates/graph/graph-kernel/src/graph/edge_taxonomy.rs)
  — the closed enums plus hand-rolled mirror tables that prompted the question.

---

## 1. Where this comes from

The kernel today is one `petgraph` of typed edges with closed Rust enums and
`match` dispatch. `edge_taxonomy.rs` defines `EdgeFamily` (6 variants, with a 7th,
`Coupling`, incoming per the field-system decision) and per-family sub-kind enums,
then re-enumerates the same closed set by hand in about eight parallel tables
(`as_tag`, `durability`, `provenance`, `family`, and the `*_sub_ordinal` /
`*_from_ordinal` mirror pair behind the `tag()` / `from_tag()` u32 codec that
exists only because `graph-canvas` cannot depend on `kernel`). Adding one sub-kind
edits the enum, those mirrors, and downstream dispatch sites. That is the
expression problem: enums make adding *operations* cheap and adding *cases* a
shotgun edit.

The question that opened: is a single typed-edge graph the right kernel at all, or
is the kernel latently more than one thing? The
[statements-over-schema stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md)
already drew a world/experience cut in the *data model* (content relations are
RDF-native; browse-trail and workspace relations are Mere's own and have no
standard vocabulary). This brief carries that cut down to the *engine*: do the two
halves want different stores.

## 2. The reframe: Nova is already in the stack

Before researching Nova as a hypothetical, the code settled it. genet already
embeds Nova as its primary native script engine.
[`genet/Cargo.toml`](../../../../genet/Cargo.toml) patches `nova_vm` to a
minimally-forked `mark-ik/nova` (`genet-embedder` branch carrying an
`EmbedderObject` native-data patch);
[`script-engine-nova`](../../../../genet/components/script-engine-nova/Cargo.toml)
is "the PRIMARY backend, NATIVE-ONLY" (Nova's `Value` is usize-sized, so it is
64-bit-bound and gated off wasm32), and
[`script-engine-boa`](../../../../genet/components/script-engine-boa/Cargo.toml)
is the pure-Rust wasm32 backend and conformance oracle.

So genet-as-host brings a data-oriented, vector-backed, handle-indexed Rust
runtime into Mere's host process as a matter of fact. The Nova question for the
kernel is therefore not "should we adopt it" but "what does the data-oriented
runtime already in our stack teach the kernel's design." The `EmbedderObject` hook
the genet fork fills was an empty `todo!()` upstream as of mid-2025, so the
fork's purpose is exactly the embedder-native-data path genet needs.

## 3. The two-natured thesis: validated, with one correction

"Two natures joined by id" is established, repeatedly-shipped prior art, not a
clever-sounding trap:

- **Blender**: original data-blocks plus a separate depsgraph-evaluated copy via
  copy-on-write; only the active depsgraph writes evaluated results back; both
  share the same structs.
- **Figma**: the Materializer maintains derived subtrees computed from
  authoritative sources, with automatic push-based dependency tracking; only
  affected portions re-materialize.
- **Unity ECS**: authoring data is baked one-way into runtime entities; runtime
  data is never transformed back; conversion is incremental and dependency-tracked.
- **CAD**: the feature-history tree is authoritative; the kernel evaluates it into
  a B-rep that carries no design history.

The research adds one non-negotiable condition that the first framing left open.
In every clean instance, **one side is authoritative and the other is
derived/evaluated, joined by a one-way, dependency-tracked rebuild with a single
write-back rule.** The seam goes leaky when both sides claim authority or
write-back is ad hoc. So "a statement core *beside* an ECS core joined by node id"
was the wrong picture; it read as peers. The right picture is **content
authoritative, experience derived.**

Mere already encodes that asymmetry:

- The spine: "graph truth, projected into composable surfaces."
- [genet-as-host §6](../technical_architecture/2026-05-29_genet_as_host_evaluation.md):
  "petgraph stays the truth; the DOM children and the scene are a projection of it."
- [spine §14.1](../technical_architecture/2026-05-21_mere_composition_spine.md):
  arrangement facts promote into durable graph Arrangement-relations "only on an
  explicit pin/save." That is the single write-back rule the prior art requires,
  already in place.

So direction #3 is mostly a *recognition and naming* of structure that exists,
not a peer-store rewrite.

## 4. The experience core already half-exists, as `gyre`

The rapier integrator (the **`gyre`** crate, renamed from `aether` per the
field-system decision in §5, which freed `aether` for a new field-algebra
crate) owns a stateful rapier world: bodies, colliders, the `QueryPipeline`,
drag-pinning, and the `NodeExclusion` / `EdgeSpring` / `Boundary` force hooks. The
[cartography-aether seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
establishes that it *simulates* a layout (a stateful actor with collision and
interaction) rather than *computing* one (a pure function), that it is kernel-tier
(the same tier as kernel and petgraph), that it must not depend on the projection
layer above it, and that it speaks only kernel types. Its landed primitives are
`seed_positions(impl IntoIterator<Item = (NodeKey, Point2D)>)` and
`positions() -> impl Iterator<Item = (NodeKey, Point2D)>` (on `gyre::Simulation`).

That is an entity-component store of experiential state in all but vocabulary:
components (position, velocity, collider, exclusion radius, spring, boundary) keyed
by `NodeKey`, stepped by a system each frame. The experience nature of the kernel
is not a green-field idea; half of it is shipping as the rapier integrator.

genet-as-host sharpens the other half.
[§6](../technical_architecture/2026-05-29_genet_as_host_evaluation.md) describes
the orrery as per-node state that is textbook entity-component: world position
(the integrator), cull/visibility (`cull_aabb`), LOD and materialization state (a
real DOM subtree versus a paint glyph), focus halo, external-texture binding.
`cull_aabb` and the materialize/demote logic are *systems* over those components.
Today that state is smeared across the integrator plus the host widget plus cull
logic. The two-natured framing says: name it as one entity-component store of
*derived* spatial state, with `gyre` as its simulator.

## 5. The fields decision, and how it fits

The [field-system extraction](../technical_architecture/2026-05-30_field_system_extraction.md)
landed the same day and partly answers this brief's question directly. Its
decisions:

- **A `Field` is a third kernel primitive** (`FieldKey` beside `NodeKey` /
  `EdgeKey`): identity, a portable field definition (a scalar/vector AST as data,
  no Rhai/Burn dependency), an extent, and lifecycle. "The definition is truth; the
  runtime that evaluates it is not."
- **`Coupling` is a seventh relation family** (field → targets × response ×
  strength), with responses **extensible by design**: v1 is force plus visual, but
  the contract is open to navigational, selection, semantic, and trigger responses.
- **Naming**: the field-algebra crate is **`aether`** (the field-bearing medium);
  the rapier integrator is **`gyre`** (the wheeling motion of bodies, kin to
  *orrery*). The `aether` rapier crate was renamed `gyre`, and `aether::Field`
  became `gyre::Force`, on 2026-05-30 (the field-system step 0).

Two things follow for this brief.

**It refines the authority asymmetry, it does not contradict it.** "The definition
is truth; the runtime that evaluates it is not" is the same content-authoritative
/ experience-derived rule at finer grain. A field/coupling *definition* is content
truth (persisted, federated, a first-class graph element); the *evaluated* field
values and the `gyre`-integrated motion are derived experience. So the field
system is a second, independent instance of the same one-way pattern, which
strengthens the thesis rather than competing with it. The spine flow it draws
makes this explicit: `truth (nodes / edges / fields + couplings) → aether
(evaluate) → gyre (integrate) → cartography (positions) → platen (paint)`.

**Two axes, both live, and they are orthogonal.** "Two natures" (content versus
experience) is about *storage-engine shape*: statement/relational for content,
entity-component for derived spatial state. "Three primitives" (node / edge /
field) is about *primitive kind* in the truth layer. They do not collide: the
truth layer can be three-primitive *and* statement-shaped; the derived experience
store is the entity-component projection of it (positions from `gyre`, field values
from `aether`). The `Coupling` family is, moreover, a live instance of exactly the
open-core hybrid this brief argues for in §6: a small recognized response core
(force, visual) with a deliberately open contract (navigational, selection,
semantic, trigger). The architecture is already reaching for the pattern.

## 6. The rest of the picture converges on one shape

- **Experience-core shape: Rerun.** [Rerun](https://rerun.io/docs/concepts/chunks)
  is a Rust column-chunk store "purpose-built for multi-rate physical data":
  entity paths, components as columns, latest-at queries (the current value of each
  component at a time) plus range queries. That is exactly positions / LOD /
  physics semantics. Borrow the shape (entity-component plus latest-at), not the
  engine: Rerun is optimized for append and time-travel, not a hot mutable sim, so
  `gyre`/rapier stays the simulator.
- **Content-core shape: statements plus incremental view maintenance.**
  Open-predicate statements (the stance) with datalog / differential-dataflow IVM
  are what let a graph canvas recompute when truth changes.
  [Riffle](https://riffle.systems/essays/prelude/) holds app state in a reactive
  relational DB and expresses UI as queries that converge within a frame;
  [DataScript](https://github.com/tonsky/datascript) shows the triple model cheap
  in-process. Caveat from the research: incremental computation can be
  memory-hungry, and SQL is a poor UI query surface, so budget for IVM state and
  pick a datalog-shaped surface. This is the same engine slot the RDF thread
  pointed Oxigraph at.
- **The expression-problem fix: the hybrid is validated.** The canonical answer is
  a closed enum for the small recognized core (zero-cost exhaustive dispatch;
  `enum_dispatch` recovers about 10x over trait objects) plus a registry /
  descriptor table for the open vocabulary. That is exactly the stance's
  closed-`sub_kinds`-plus-open-predicate split, the `Coupling` family's
  recognized-core-plus-open-responses shape, and the "mechanize the mirror tables"
  cleanup. They are the same move at three scales.

One precision worth keeping: ECS is "baby's first relational model," a strict
subset where every component is foreign-keyed to entity id and systems run every
frame ([SpacetimeDB](https://spacetimedb.com/blog/databases-and-data-oriented-design)).
The frame assumption is the game-leak. It fits the experience core because that
core really is frame-driven; it would leak into the content core, which is not.
The split is what lets each nature use the model that fits.

## 7. Nova and xilem_serval: what is transferable

Grounded facts. Nova is pure-Rust, MPL-2.0, released 1.0 in March 2026, and
genuinely data-oriented: heap values are 32-bit type-discriminated indices into
per-type vectors rather than pointers, with a borrow-checker-enforced rooting GC
(`GcScope` / `NoGcScope`, compacting). It is also experimental, interpreter-only
(no JIT, QuickJS-class speed by the author's own target), about two-thirds
test262, with RegExp and other conformance gaps, and effectively one developer on
grant funding; the author says "if you need an embeddable JS engine in Rust today,
go use Boa," which is exactly why genet pairs Boa (oracle/wasm) with Nova
(native primary).

Bearing on the *kernel* question, three bounded things:

1. **Proof-of-approach, in-repo.** A serious data-oriented, vector-backed,
   handle-indexed Rust runtime already lives in the stack and the team forks it.
   An ECS / data-oriented experience kernel is congruent with the host's own
   design philosophy.
2. **Transferable discipline.** Nova's "value = type-discriminated index into a
   component vector" is the ECS pattern, and its `GcScope` / `NoGcScope` rooting is
   a worked answer to the stale-handle / despawn-safety problem an ECS experience
   core will hit (generational indices). The GC writeups are the reference if the
   experience core ever needs safe cross-frame handles.
3. **Maturity signal.** Nova shows data-oriented design scales to a complex Rust
   runtime.

The caveat, so it is not oversold: Nova's data-orientation is internal to the JS
heap. It is not an exposed app-data-model or a reusable ECS crate. The relevance
is inspiration, transferable patterns, and cultural congruence, not free
infrastructure; genet-as-host will not hand the kernel an ECS store.
`xilem_serval` touches the kernel only indirectly: by making the orrery's
experiential state (physics-positioned DOM, materialization, visibility)
first-class, it is the forcing function that makes modeling that state as a
component store worthwhile. It does not itself participate in the content /
experience split.

## 8. What direction #3 becomes

Not a kernel rewrite. A staged recognition with one guardrail, dovetailed with the
field-system sequencing.

- **First (free, and correct):** mechanize the closed-core dispatch in
  `edge_taxonomy.rs` into a descriptor table (strum / enum-map / a `static`
  per-variant table), deleting the hand-rolled mirror maps and the u32 codec. Done
  when adding a sub-kind is one edit and exhaustiveness still holds. The `Coupling`
  family is about to be added, so mechanizing first means adding it once, cleanly,
  rather than hand-writing another mirror set. (The u32 codec's own justification,
  "`graph-canvas` cannot depend on `kernel`," is itself dissolving, since the
  field-system doc retires `graph-canvas`.)
- **Then (gated on genet-as-host):** name and consolidate the experiential state
  (position, cull/visibility, LOD, materialization, focus) into an explicit
  entity-component store of *derived* state, `gyre` as its simulator, Rerun's
  entity-component / latest-at as the shape reference. genet-as-host §6 is the
  trigger, because it makes materialization and visibility first-class. This is the
  derived-side counterpart to the field-system's step 3 (fields as truth); the two
  are complementary, content-truth and experience-derived.
- **Later (consumer-pulled):** give the content core IVM / datalog reactivity when
  the orrery-recompute-on-truth-change and the SPARQL projection from the
  linked-data thread pull on it.
- **The guardrail, from the prior art:** content stays authoritative, experience
  stays derived, sync stays one-way with the pin/save write-back. The two natures
  must not become bidirectional peers. Blender's "only the depsgraph writes back"
  and Unity's "never transformed back" exist as scar tissue to enforce exactly this.

This closes the loop across the recent threads: the open-statement substrate is
the content core, the world/experience cut is the content/experience seam, fields
are a third truth primitive evaluated into derived experience, and the kernel's
two natures are the content and experience cores joined by node id under the
one-way rule.

## Sources

External (verified against primary sources by the research fan-out):

- Nova: data-oriented heap — <https://trynova.dev/blog/what-is-the-nova-javascript-engine>;
  GC rooting — <https://trynova.dev/blog/guide-to-nova-gc>; 1.0 and "use Boa
  today" — <https://trynova.dev/blog/nova-1.0>; repo — <https://github.com/trynova/nova>
- ECS as a relational subset —
  <https://spacetimedb.com/blog/databases-and-data-oriented-design>
- Rerun chunks / entity-component / latest-at — <https://rerun.io/docs/concepts/chunks>
- Riffle (app-state-as-DB) — <https://riffle.systems/essays/prelude/>;
  DataScript — <https://github.com/tonsky/datascript>
- Figma Materializer —
  <https://www.figma.com/blog/how-we-rebuilt-the-foundations-of-component-instances/>
- Expression problem in Rust —
  <https://purplesyringa.moe/blog/the-expression-problem-and-rust/>;
  `enum_dispatch` — <https://docs.rs/enum_dispatch/>

Internal: the Related docs above, plus `edge_taxonomy.rs` for the live taxonomy.

## Progress

- **2026-05-30** — Brief created from the kernel-design discussion (direction #3)
  plus two research fan-outs (Nova architecture/maturity; ECS / relational /
  two-store prior art), grounded against `edge_taxonomy.rs`, the genet
  `Cargo.toml` script-engine wiring, the genet-as-host eval, the cartography-aether
  seam, and the composition spine. Reconciled the same day with the
  [field-system extraction](../technical_architecture/2026-05-30_field_system_extraction.md):
  adopted the `aether` (field algebra) / `gyre` (rapier integrator) naming, added
  §5 to fold in the `Field` third-primitive and `Coupling` family decisions, and
  noted the two orthogonal axes (content-vs-experience storage shape; node / edge /
  field primitive kind). No code written. The first staged step (mechanize the
  `edge_taxonomy.rs` closed-core dispatch) is a clean, isolated win, made more
  timely by the incoming `Coupling` family.
