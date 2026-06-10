# Field/Coupling Kernel-Primitive Plan

**Date**: 2026-05-31
**Status**: Approved; decisions locked 2026-05-31. **P0–P3 landed; Phase 4 done at
the kernel-truth level (EdgePath/EdgePathRule, the open response vocabulary, and
the activate/retire lifecycle primitive). kernel 232 tests green (236 with
`store`); aether 69; gyre 18. Only federation (needs the federation substrate) and
the host-side lifecycle UX remain, both outside this kernel-truth plan, so this
plan is effectively complete.** This is **step 3** of the
[field-system extraction](../technical_architecture/2026-05-30_field_system_extraction.md)
§8 ("the bigger, truth-level slice; its own plan"). Steps 0 (gyre rename) and 1
(aether crate) have already landed (see Findings); the truth-level work of this
plan is now done bar the deferred Phase 4 tail.
**Scope**: Make `Field` and `Coupling` first-class **kernel truth** (persisted,
stable-id, federatable), read by aether, rather than the in-memory aether
registry they are today. Resolves the field-system doc's open kernel question.
**Related**:

- [field-system extraction](../technical_architecture/2026-05-30_field_system_extraction.md)
  — the parent decision (Field as a third primitive, Coupling as a relation,
  aether/gyre split). This plan is its step 3.
- [two-natured kernel brief](../research/2026-05-30_two_natured_kernel_brief.md)
  — content-authoritative / experience-derived, one-way sync. The field
  *definition* is content truth; aether eval + gyre integration are derived.
- [statements-over-schema stance](../technical_architecture/2026-05-22_statements_over_schema_stance.md)
  — recognized-core-plus-open-tail; the response-vocabulary expansion (Phase 4)
  is an instance of it.
- Code: kernel [`graph/mod.rs`](../../../crates/graphshell/graph/graph-kernel/src/graph/mod.rs),
  [`graph/edge_payload.rs`](../../../crates/graphshell/graph/graph-kernel/src/graph/edge_payload.rs),
  [`graph/identity.rs`](../../../crates/graphshell/graph/graph-kernel/src/graph/identity.rs);
  aether [`coupling.rs`](../../../crates/graphshell/graph/aether/src/coupling.rs),
  [`registry.rs`](../../../crates/graphshell/graph/aether/src/registry.rs).

---

## Findings (grounded against the live code, 2026-05-31)

### The state that reshapes this plan

Field-system **step 1 is already done** (commit `9ea5858`, today). The standalone
`aether` field-algebra crate exists at `graphshell/graph/aether/`: `ast.rs`
(`ScalarField` / `VectorField`), `coupling.rs`, `eval.rs`, `lower_burn.rs` (Burn),
`rhai_bindings.rs`, `registry.rs`. Its `Cargo.toml` deps are serde plus *optional*
`burn` (`field-burn`) and `rhai` (`field-rhai`) — so the core (AST, coupling,
registry) is **serde-only and Rhai/Burn-free**, and **aether has no `kernel`
dependency today**.

So the Field/Coupling *types* already exist as aether **runtime** state:

- `FieldRegistry` is an in-memory store keyed by a **registry-local** `FieldId(u64)`
  (`next_id`-assigned, not stable across sessions, not federatable).
- `Coupling { selector: NodeSelector, field: FieldId, response: CouplingResponse, strength: f32 }`.
- `NodeSelector { All | Tagged(String) | Kind(String) | NotTagged(String) }`.
- `CouplingResponse` is a **closed enum of 6 force responses** (AttractToMin,
  RepelFromMax, AlignVelocity, FlowAdvect, DampenInside, ContainmentWall). The
  field-system doc's "open contract" (visual / navigational / selection /
  semantic / trigger) is **not present yet** — today's responses are force-only.

This is exactly the "until the kernel primitive lands, aether reads fields from an
in-memory registry rather than the graph" state the field-system doc anticipated.
Step 3 is the gap: promote Field/Coupling from runtime-registry to kernel truth.

### The kernel as it is

- `Graph = StableGraph<Node, EdgePayload, Directed>` plus side indexes
  (`url_to_nodes`, `id_to_node`, `import_records`). `NodeKey`/`EdgeKey` are
  **petgraph indices** (session-unstable); `Node` carries a stable `Uuid id`.
- Topology mutators are **crate-internal** (the single-write-path boundary;
  runtime routes through reducer intents).
- `EdgePayload` is an `Option`-sidecar struct; `EdgeFamily` (6) is **derived** from
  which sidecars are populated; write via `EdgeAssertion`, read via
  `RelationSelector`. **Edges are node→node.**
- Persistence: `Persisted*` DTOs + `GraphSnapshot`, rkyv snapshot + `graph.json`
  serde live path, **additive `#[serde(default)]`** migration (per the linked-data
  plan's findings).

### The core fork, resolved

A `Coupling` is `field → NodeSelector (a dynamic set) × response × strength`
(confirmed against aether's actual struct). It is **not** node→node, so:

- **Option A — fields as heterogeneous petgraph node weights, couplings as
  petgraph edges. Rejected.** It would make the `StableGraph` weight an enum and
  ripple through every `node_weight` / query / snapshot / layout / gyre consumer
  that assumes `Node`; selector targeting (all nodes with tag X) does not
  materialize as fixed node→node edges; and it conflates two natures.
- **Option B — a parallel keyed store on `Graph`. Adopted.** Fields and couplings
  live in their own keyed stores beside the node/edge petgraph, which stays
  homogeneous (`Node` weights, 6-family `EdgePayload`). This is the two-natured
  brief made concrete: field/coupling **definitions are content truth**
  (authoritative, persisted); aether evaluation and gyre integration are **derived
  experience**, one-way, with no write-back except an explicit pin/save.

**"Coupling = the 7th relation family"** is honored *conceptually* (a coupling is a
first-class relation kind in the lens) but realized as a **first-class coupling
primitive in the field layer, not an `EdgeFamily` variant / `EdgePayload`
sidecar.** `EdgeFamily` stays 6. This refines the field-system doc's "joins the
relation taxonomy as a new family" line, which predates the confirmation that
`EdgeFamily` is *derived from node-edge sidecars* and so structurally cannot host a
field→selector relation.

### Three decisions the plan commits to

1. **Identity.** aether's `FieldId(u64)` is registry-local. Kernel truth needs a
   **stable, UUID-backed `FieldId`** (mirroring `Node.id`) plus a `CouplingId`.
   The kernel id is canonical; aether's u64 stays at most an internal runtime
   cache key.
2. **Definition ownership (the crux).** The portable AST (`ScalarField` /
   `VectorField`, serde, Rhai/Burn-free) is the field *definition*.
   - *(i, recommended)* The **kernel owns** the portable Field/Coupling/AST truth
     types (rkyv + serde); **aether gains a `kernel` dependency** and uses them for
     eval (Rhai/Burn stay in aether). Typed, rkyv-native, kernel-validated. This is
     the spine-correct direction (truth → aether → gyre). Cost: the AST + coupling
     types move *down* from aether into the kernel.
   - *(ii, alternative)* The kernel stores the definition as an **opaque
     serialized blob**; aether keeps the AST type. Keeps the kernel thin; loses
     kernel-side typing/validation; awkward under rkyv.
   - **Decided (Mark, 2026-05-31): (i)** — the kernel owns the portable AST and aether gains a `kernel` dependency. A shared minimal `field-types` crate below both was considered
     and set aside (the kernel already owns the truth vocabulary; avoid a crate for
     a two-consumer type set).
3. **Layering flip.** aether goes from standalone to **depending on kernel**,
   reading Field/Coupling from the graph; its in-memory `FieldRegistry` becomes a
   derived cache fed from the graph (or is dropped). gyre already depends on
   kernel; the gyre seam (field-system step 2) consumes couplings as forces.

---

## Plan (done-conditions, not dates)

### Phase 0 — Kernel truth types

- New kernel modules `graph/field.rs` + `graph/coupling.rs` (new files, to respect
  the 600-LOC ceiling rather than bloating `mod.rs`).
- `FieldId(Uuid)` / `CouplingId(Uuid)`; `Field { id, definition, extent, lifecycle }`;
  `FieldExtent { Global | Region(..) | AttachedToNode(Uuid) }`;
  `Coupling { id, field: FieldId, selector: NodeSelector, response: CouplingResponse, strength: f32 }`;
  `NodeSelector`; `CouplingResponse` (the 6 force responses, parity with aether).
- Port the portable AST per decision 2(i) (or wrap opaquely per 2(ii) — finalize
  after reading `aether/ast.rs` in full). rkyv + serde derives, mirroring the
  existing taxonomy types.
- **Done (landed 2026-05-31, 216/216 kernel tests green)**: types compile
  WASM-clean (no Rhai/Burn in kernel), serde round-trip + construction tests pass.
  rkyv moved to the P2 DTO layer, where the recursive-AST decision is settled
  against `snapshot/{to,from}.rs`.

### Phase 1 — Graph integration

- Add `fields: HashMap<FieldId, Field>` and `couplings: HashMap<CouplingId, Coupling>`
  to `Graph`.
- Crate-internal mutators behind the single-write-path boundary: `add_field` /
  `retire_field` / `add_coupling` / `retract_coupling`.
- Query API: `fields()`, `field(id)`, `couplings()`, `couplings_for_field(id)`, and
  **selector evaluation** `nodes_matching(&NodeSelector) -> impl Iterator<NodeKey>`
  against node tags/classifications.
- **Done (landed 2026-05-31, `9a1f736`; kernel 220).** `Graph` gained the parallel
  keyed `fields`/`couplings` stores + the mutators/queries above in `graph/field_ops.rs`;
  `nodes_matching` resolves `All`/`Tagged`/`NotTagged` against node tags and `Kind`
  against classification value. This commit also carried the P0 truth types to first
  commit (they were drafted but uncommitted). add/retire/query round-trips +
  selector eval tested; all node/edge tests stayed green.

### Phase 2 — Persistence

- `PersistedField` / `PersistedCoupling` in `GraphSnapshot`; `to`/`from` snapshot;
  additive `#[serde(default)]` so old `graph.json` loads with an empty field layer;
  rkyv snapshot path updated.
- **The recursive-AST decision (resolved): serde-blob, not rkyv `omit_bounds`.** The
  scalar/vector definition rides as a serde-JSON string (`PersistedField::definition_json`);
  the flat enums (extent/lifecycle/selector/response) archive natively. Rationale:
  the kernel AST is deliberately serde-only, the definition is loaded once and
  evaluated in memory by aether (no zero-copy benefit), `serde_json` is already a
  base kernel dep, and a blob keeps the archive flat and the round-trip robust. New
  DTOs live in `persistence_fields.rs` (re-exported through `crate::persistence`)
  since `persistence.rs` is already over the per-file ceiling.
- **Done (landed 2026-06-01, `00bbb4c`; kernel 227, 231 with `store`).** An old
  snapshot missing the field keys loads empty (additive migration pinned); a
  field + coupling round-trips (name, Region extent, retired-keeps-definition,
  DampenInside); DTO serde + rkyv round-trips. `from_snapshot` skips malformed
  ids/definitions rather than failing the whole load.

### Phase 3 — The aether seam (coordinates with field-system step 2)

- aether depends on kernel and reads Field/Coupling from the graph (its in-memory
  `FieldRegistry` becomes derived, or is dropped).
- Re-express one gyre built-in (e.g. `NodeExclusion`) as a kernel coupling to prove
  equivalence, keeping the built-in as the fast default path.
- **Done (landed 2026-05-31: `7e077e1` flip + `ab5310f` seam; follow-up `aefa751`).**
  - **3a, the flip:** aether deps kernel and consumes `kernel::graph::{field_ast,
    coupling, field}`, deleting its duplicate AST/coupling/`FieldId`. The registry
    keys on the UUID `FieldId`, minting registry-local ids as
    `FieldId::from_uuid(Uuid::from_u128(counter))` (WASM-safe); the rhai i64 script
    handle bridges through the uuid's low bits. The `FieldRegistry` survives as a
    runtime cache rather than being dropped.
  - **3b, the seam:** `gyre::CouplingForce` compiles a kernel `Coupling` into a
    `gyre::Force`. `from_coupling(coupling, graph)` resolves the field definition
    (`graph.field`) and the selector's nodes (`graph.nodes_matching`), and `apply`
    evaluates via aether and maps the response to motion. Equivalence proven with
    **`Boundary` ≡ `AttractToMin` on the paraboloid ½(x²+y²)** (gradient (x,y) ⇒
    force −pos·strength): side-by-side sims track to ~1.5 units over a 580-unit
    settle. `NodeExclusion` stays a native pairwise force (it is not a static field,
    so it is not a clean single-coupling equivalence; Boundary is).
  - **Follow-up (`aefa751`):** `from_coupling` seeds the registry from
    `graph.fields()` (via the new `FieldRegistry::insert_with_id`) so inter-field
    `Sample(FieldId)` references resolve.

### Phase 4 — Each its own slice

- **Done (landed 2026-06-01, `ac34907`): `EdgePath`/`EdgePathRule` into the kernel.**
  The last field-layer definition types still living in aether moved down to
  `kernel::graph::edge_path` (serde-only, `FieldLine` keyed on the kernel
  `FieldId`); aether's `coupling.rs` collapsed to a thin re-export. Type relocation
  only; Graph storage + persistence for edge-path rules can follow when a consumer
  needs them persisted (they are aether-runtime today).
- **Done (landed 2026-06-01, `2ca940b`): the open response vocabulary.**
  `CouplingResponse` is now a recognized-core-plus-open-tail hybrid: the six force
  responses stay the recognized core gyre dispatches, and `Open { predicate }`
  carries the families beyond force (visual / navigational / selection / semantic /
  trigger) by IRI under `COUPLING_VOCAB`, stored faithfully and ignored by the
  force integrator until a consumer recognizes it. Recognized-core IRIs round-trip
  (`recognized_iri`/`from_iri`, pinned by `strum::EnumIter`); persistence mirrors
  the tail. v1 behavior stays force-only: this opens the contract, no new behavior.
- **Done (landed 2026-06-01, `66c26d0`): lifecycle primitive.** `activate_field`
  joins `retire_field`, making the activate/retire lifecycle fully round-trippable
  from the kernel. The lifecycle *UX* (host buttons) is host-layer work, outside
  this kernel-truth plan.
- **Still deferred — federation of fields/couplings.** Sharing fields/couplings
  across instances needs the federation substrate (the engram/Willow/Keyhive layer),
  not a field-specific slice. It lands when federation does, on the same
  per-statement provenance + identity-as-merge-key seam the statements-over-schema
  stance already names.

---

## Open questions / non-goals

- ~~Definition ownership (i vs ii)~~ **Resolved: (i).** Kernel owns the portable AST;
  aether gained a `kernel` dep (P0/P3a).
- ~~aether `FieldId(u64)` reconciliation~~ **Resolved (P3a):** the kernel UUID
  `FieldId` is canonical and lives in the registry; aether mints registry-local ids
  as `Uuid::from_u128(counter)` and the rhai i64 handle bridges through the uuid's
  low bits. The `FieldRegistry` survives as a runtime/derived cache, not dropped.
- ~~rkyv recursive-AST archiving (`omit_bounds` vs serde-blob)~~ **Resolved (P2):
  serde-blob** the definition as a JSON string; flat enums archive natively.
- Selector-eval performance (re-eval on graph mutation): acceptable for now;
  optimize only under a real frame-rate signal. `CouplingForce` captures its target
  set at build time, so callers rebuild it on graph mutation (still open: a
  rebuild-on-change hook vs eager re-resolve).
- **Non-goals**: moving Rhai/Burn into the kernel (they stay aether-side, optional);
  evaluating fields in the kernel (kernel stores, aether evaluates); the open
  response vocabulary in v1.
- **Coordination**: step 1 (aether crate) DONE (`9ea5858`); step 2 (gyre seam)
  overlaps Phase 3; step 4 (dissolve graph-canvas) is independent cleanup. Phases
  0–2 are kernel-only and can land before the aether seam.

---

## Progress

- **2026-05-31** — Plan created. Grounded against the live kernel
  (`StableGraph<Node, EdgePayload>`, `Option`-sidecar `EdgePayload` with derived
  `EdgeFamily`, petgraph-index keys over stable `Uuid`, `Persisted*` serde-default
  migration) and the **just-landed** aether crate (`9ea5858`: standalone
  field-algebra, serde + optional burn/rhai, registry-local `FieldId(u64)`, no
  kernel dep). Core fork resolved in favor of a parallel keyed store (Option B);
  couplings modeled as a first-class field-layer primitive, not an `EdgeFamily`
  sidecar. No code written.

- **2026-05-31** — Decisions locked (Mark): definition-ownership **(i)** (kernel
  owns the portable AST; aether gains a `kernel` dependency); Coupling stays a
  *tracked* first-class field-layer primitive (identity, lifecycle, persistence),
  not an `EdgeFamily` variant. P0 unblocked.

- **2026-05-31** — **P0 landed.** New kernel modules: `graph/field_ast.rs`
  (ported `Falloff`/`ScalarField`/`VectorField`, serde, mutually-recursive `Box`),
  `graph/field.rs` (`FieldId`/`CouplingId` UUID newtypes, `FieldDefinition`,
  `FieldExtent`, `FieldLifecycle`, `Field`), `graph/coupling.rs` (`NodeSelector`,
  `CouplingResponse`, `Coupling`); wired + re-exported in `graph/mod.rs`.
  serde-only for now (rkyv deferred to the P2 DTO layer, where the recursive-AST
  decision is settled against `snapshot/{to,from}.rs`); `Sample(FieldId)`
  migrated from aether's `u64` to the kernel UUID id. Verified `cargo test -p
  kernel` = **216 passed** (208 prior + 8 new), compile-clean. No `Graph`
  integration yet (that is P1).

- **2026-05-31** — **P1 landed** (`9a1f736`, kernel 220). `Graph` gained the parallel
  keyed `fields`/`couplings` stores + mutators/queries (`graph/field_ops.rs`) +
  `nodes_matching` selector eval. The commit also carried P0's truth types to first
  commit (they were drafted but still uncommitted at that point).

- **2026-05-31** — **Aether seam (Phase 3) landed.** `7e077e1`: aether flipped onto
  the kernel field types (duplicate AST / coupling / `FieldId` dropped; registry now
  keys on the UUID id). `ab5310f`: `gyre::CouplingForce` compiles a kernel `Coupling`
  to a `gyre::Force`; `Boundary` ≡ `AttractToMin` on the paraboloid proven by a
  side-by-side sim. `aefa751`: `from_coupling` seeds the registry from
  `graph.fields()` so inter-field `Sample` resolves (new `FieldRegistry::insert_with_id`).

- **2026-06-01** — **P4 EdgePath slice landed** (`ac34907`, kernel 222).
  `EdgePath`/`EdgePathRule` moved to `kernel::graph::edge_path`; aether re-exports.
  The kernel now owns every field-layer definition type.

- **2026-06-01** — **P2 landed** (`00bbb4c`, kernel 227 / 231 with `store`).
  `PersistedField`/`PersistedCoupling` in `persistence_fields.rs`; `GraphSnapshot`
  gained `#[serde(default)]` `fields`/`couplings` + to/from wiring. The recursive AST
  rides as a serde-JSON blob (decision recorded under Phase 2). Round-trip +
  additive-migration tests green. Truth-level plan done bar the deferred Phase 4 tail.

- **2026-06-01** — Ownership: this plan is now driven by the field-system agent
  (the linked-data/djot work split to a separate owner). Remaining scope is the
  deferred Phase 4 tail (open response vocabulary, federation, lifecycle UX).

- **2026-06-01** — **Phase 4 tail landed (kernel-truth level).** `2ca940b`: the
  open response vocabulary — `CouplingResponse` becomes recognized-core (six force
  responses, gyre-dispatched) plus an `Open { predicate }` tail carrying the
  visual/navigational/selection/semantic/trigger families by IRI under
  `COUPLING_VOCAB`; `recognized_iri`/`from_iri` round-trip (strum-pinned); gyre
  ignores the tail; persistence mirrors it. `66c26d0`: `activate_field` completes
  the activate/retire lifecycle primitive. kernel 232 (236 with `store`), gyre 18.
  Federation and the host-side lifecycle UX remain (both outside kernel truth), so
  the plan is effectively complete.

- **2026-06-01** — **First paint consumer of the open tail landed** (`08fa8ce`,
  platen). `platen::coupling_paint` recognizes the `visual/*` slice (halo / tint) of
  the open response vocabulary and resolves it to overlay paint commands, the
  aether→platen seam mirroring gyre's `CouplingForce` on the force side: same
  resolve path (`graph.field` + `nodes_matching` + a registry seeded from
  `graph.fields()`), evaluate the scalar field at each target's projected position,
  map value × strength to intensity. Validates the open tail end to end on the paint
  side; the kernel stays force-core-only (platen owns its visual vocabulary, an
  unrecognized `visual/*` IRI is skipped). platen 43. The rhai `couple_open`
  authoring surface still waits on a `FieldProjection`→`Graph` commit path.

- **2026-06-01** — **Authoring path closed** (`62bafc0`, aether).
  `FieldProjection::commit_to_graph` writes an authored projection's registry fields
  and couplings into the kernel `Graph` (the inverse of the gyre/platen read path), so
  authored content reaches the truth those consumers read. rhai gains
  `couple_open(kind, field_id, iri, strength)`, making the open-tail (e.g. `visual/*`)
  couplings scriptable. Proven end to end: a script authors a `visual/halo` coupling,
  commit lands it in the `Graph` with its open predicate and field. aether 72. The
  field system now runs full-circle: author (rhai → `FieldProjection`) → commit →
  kernel truth → consumers (gyre force, platen visual). `edge_path_rule` Graph
  storage + stable cross-projection ids remain host/Phase-later concerns.
