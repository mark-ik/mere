# Field/Coupling Kernel-Primitive Plan

**Date**: 2026-05-31
**Status**: Approved; decisions locked 2026-05-31; **P0 landed (216/216 kernel tests green); P1 next.** This is **step 3** of the
[field-system extraction](../technical_architecture/2026-05-30_field_system_extraction.md)
§8 ("the bigger, truth-level slice; its own plan"). Steps 0 (gyre rename) and 1
(aether crate) have already landed (see Findings); this plan is the remaining
truth-level work. No code written yet.
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
- **Done**: add/retire/query round-trips; selector eval correct against
  tags + kind; all existing node/edge tests stay green.

### Phase 2 — Persistence

- `PersistedField` / `PersistedCoupling` in `GraphSnapshot`; `to`/`from` snapshot;
  additive `#[serde(default)]` so old `graph.json` loads with an empty field layer;
  rkyv snapshot path updated.
- **Done**: an old snapshot loads (empty fields/couplings); a seeded field +
  coupling round-trips save/load; a migration test pins the additive behavior.

### Phase 3 — The aether seam (coordinates with field-system step 2)

- aether depends on kernel and reads Field/Coupling from the graph (its in-memory
  `FieldRegistry` becomes derived, or is dropped).
- Re-express one gyre built-in (e.g. `NodeExclusion`) as a kernel coupling to prove
  equivalence, keeping the built-in as the fast default path.
- **Done**: aether evaluates a kernel-stored field; the coupling-expressed
  built-in matches the built-in's output; gyre integrates it.

### Phase 4 — Deferred, each its own slice

- The **open response vocabulary** (visual / navigational / selection / semantic /
  trigger) via the recognized-core-plus-open-tail hybrid — this is where the
  statements-over-schema stance and the just-landed strum descriptor pattern pay
  off. v1 stays force-only.
- Federation of fields/couplings; lifecycle UX (activate/retire); aether's
  `EdgePath`/`EdgePathRule` (field-driven edge geometry) as a coupling concern.

---

## Open questions / non-goals

- Definition ownership (i vs ii): recommend (i); confirm by reading `aether/ast.rs`
  in full at Phase 0 before moving the AST.
- aether `FieldId(u64)` reconciliation: kernel UUID id is canonical; whether aether
  keeps the u64 as a runtime cache key or adopts the kernel id is a Phase-3 detail.
- Selector-eval performance (re-eval on graph mutation): acceptable for now;
  optimize only under a real frame-rate signal.
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
