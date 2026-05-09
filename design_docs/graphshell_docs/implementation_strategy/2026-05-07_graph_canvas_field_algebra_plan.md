# Graph-Canvas Field Algebra & Rhai Composition Plan

**Date**: 2026-05-07
**Status**: Active plan (Phase 0 — design)
**Scope**: Promote the existing scattered field-shaped primitives in `graph-canvas` (`ZSource`, `SceneRegionEffect`, motion profiles) into a unified **field algebra** with a custom AST that lowers to Burn for GPU-accelerated evaluation. Layer **Rhai** as the per-canvas composition surface for fields and coupling rules. Extend the projection ladder (2D ↔ 2.5D ↔ 3D) with presets at each level while preserving the lossless `(x, y)` truth contract. Reframe the scripting module from "Wasmtime-only" to "Rhai-first; sandboxed wasm later if needed."

**Related**:

- [`README.md`](../../../README.md) — Mere crate roles
- [`2026-05-06_graphshell_migration_plan.md`](2026-05-06_graphshell_migration_plan.md) — migration order; portable crates first
- [`../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) — protocol architecture (Graphshell consumes; does not duplicate)
- Inherited (graphshell): [`view_dimension_spec.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/graph/view_dimension_spec.md) — original ZSource contract
- Inherited (graphshell): [`2026-04-10_vello_scene_canvas_rapier_scene_mode_architecture_plan.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/graph/2026-04-10_vello_scene_canvas_rapier_scene_mode_architecture_plan.md) — prior Wasmtime canonicalization (this plan supersedes the language choice)
- Inherited (graphshell): [`2026-04-03_twod_twopointfive_isometric_plan.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/graph/2026-04-03_twod_twopointfive_isometric_plan.md) — projection ladder predecessor

---

## 0. Context — Existing State

The `graph-canvas` crate already contains the primitives this plan generalizes:

| Existing | Today | After this plan |
| --- | --- | --- |
| [`projection.rs`](../../../crates/graph/graph-canvas/src/projection.rs) `ZSource` | 5 fixed variants (`Zero`, `Recency`, `BfsDepth`, `UdcLevel`, `Manual`) → scalar z per node | One application of `FieldProjection`; back-compat shim retains the enum |
| [`scene_region.rs`](../../../crates/graph/graph-canvas/src/scene_region.rs) `SceneRegionEffect` | Bounded effects (`Attractor`, `Repulsor`, `Dampener`, `Wall`) | One backend lowering of the field algebra; bounded shapes become a kernel kind |
| [`scene_physics.rs`](../../../crates/graph/graph-canvas/src/scene_physics.rs) (715 LOC ⚠️) | Hand-rolled separation, containment, region effects | Split (see §6) and re-grounded as field-coupling lowerings |
| [`scripting.rs`](../../../crates/graph/graph-canvas/src/scripting.rs) | Stub for Wasmtime/Extism, capability flags, hit shapes | Rhai becomes the primary composition surface; wasm sandbox deferred to a layered concern |

**No code is replaced wholesale.** The plan generalizes existing primitives and adds new layers. Old enums remain as public API for migration distance.

---

## 1. Goals

1. One field algebra: scalar / vector / (later) tensor fields over canvas coordinates, with explicit primitives for `Gradient`, `Curl`, `Divergence`, kernel composition.
2. Custom AST → Burn lowering. Multiple backends pluggable behind one expression IR (Burn-wgpu primary; analytic closed forms where known; WGSL direct-into-Vello later).
3. `FieldProjection` generalizes `ZSource`. Z-derivation becomes one application of evaluating a scalar field at node positions.
4. Rhai is the per-canvas composition language. Authors compose fields, attach coupling rules, declare edge-path rules, set projection presets.
5. Lossless dimension ladder with **presets at each level**: 2D presets, 2.5D projection family (Isometric, Cabinet, Cavalier, Tilted, MildPerspective), 3D presets (deferred to architecture-only).
6. Browser/PWA-compatible toolchain: Rhai + Burn-wgpu both work on `wasm32-unknown-unknown` + WebGPU. No JIT-required runtime in the critical path.
7. File hygiene: every new module ≤ 600 LOC. Existing `scene_physics.rs` split as part of this work.

## 2. Non-Goals

- Replacing Burn with hand-rolled WGSL (deferred backend, not required for first horizon).
- Tensor (rank-2) fields in v1. Scalar + vector cover the immediate need.
- Wasm-based per-object scripts in v1. Layering wasmi/Wasmtime for sandboxed third-party object scripts is a separate later plan.
- Full free-camera 3D. `ProjectionMode::Standard` remains architecture-only.
- Field reactivity beyond per-frame eval. (No spreadsheet-style propagation graph yet.)
- A new graph truth source. Fields read graph state and produce deltas/projections; they do not own node identity, edge identity, or topology.

---

## 3. Architecture

### 3.1 Field algebra (the IR)

Custom AST, lives in a new `fields/` module under `graph-canvas/src/`. Marked **illustrative-signature-only**:

```rust
// illustrative
pub enum ScalarField {
    Const(f32),
    CoordX, CoordY, Time,
    Gaussian { center: VectorField, sigma: f32 },
    Disk     { center: VectorField, radius: f32, falloff: Falloff },
    Linear   { normal: VectorField, offset: f32 },
    Add(Box<ScalarField>, Box<ScalarField>),
    Mul(Box<ScalarField>, Box<ScalarField>),
    Dot(Box<VectorField>, Box<VectorField>),
    Compose(Box<ScalarField>, Box<ScalarField>),  // f∘g
    Sample(FieldId),                              // reference a registered field
}

pub enum VectorField {
    ConstVec(Vec2),
    Coord,                                        // identity (x, y)
    Gradient(Box<ScalarField>),                   // ∇: scalar → vector
    Perp(Box<VectorField>),                       // 2D rotate 90°
    Add(Box<VectorField>, Box<VectorField>),
    Scale(Box<VectorField>, Box<ScalarField>),    // pointwise
    Sample(FieldId),
}
```

Calculus operators (`Gradient`, eventually `Curl`, `Divergence`) are first-class — a deliberate departure from raw tensor ops. Lowering can recognize known closed forms (e.g. `Gradient(Gaussian { center, sigma })` simplifies analytically) and skip Burn entirely for those.

### 3.2 Field registry

Each canvas owns a `FieldRegistry` mapping `FieldId → ScalarField | VectorField`. Persisted as part of the per-view scene composition state. Rhai script side constructs entries; Rust side evaluates them.

### 3.3 Coupling rules

Couplings are how nodes/edges respond to fields. A coupling is the bridge from "field exists in space" to "node experiences a delta" — and is exactly the generalization of today's `SceneRegionEffect::Attractor` etc. **Illustrative**:

```rust
// illustrative
pub struct Coupling {
    pub selector: NodeSelector,        // tag, kind, all
    pub field: FieldId,
    pub response: CouplingResponse,
    pub strength: f32,
}

pub enum CouplingResponse {
    AttractToMin,    // -∇φ ; gradient descent on a scalar potential
    RepelFromMax,    // +∇φ
    AlignVelocity,   // velocity := field(pos)
    FlowAdvect,      // pos += dt · field(pos)
    DampenInside,    // velocity *= 1 - factor·φ(pos)
    ContainmentWall, // hard pushout when φ > 0 (replaces Wall region)
}
```

Today's `SceneRegionEffect::Attractor { strength }` is recovered as: `Coupling { selector: All, field: <bounded gaussian>, response: AttractToMin, strength }`. Old enum stays as a public-API ergonomic shortcut that constructs the canonical coupling under the hood.

### 3.4 Edge-path rules

Edges currently have implicit straight-line paths. The plan adds a `EdgePathRule` per edge role that selects a path strategy. **Illustrative**:

```rust
// illustrative
pub enum EdgePath {
    Straight,
    Spline,
    FieldLine { field: FieldId, max_steps: u32 },
    Custom(EdgePathFn),
}
```

`FieldLine` integrates a vector field from source to target via RK4. This is the monocurl-inspired primitive Mark called out as a key motivator.

### 3.5 Lowering — backends behind the IR

Three backends, ordered by horizon:

1. **Burn-wgpu** (primary). Lowering walks the AST, builds a Burn tensor expression of `eval_field(field, points: Tensor[N×2]) -> Tensor[N]` (scalar) or `Tensor[N×2]` (vector). One dispatch per field per frame. Burn's CubeCL fusion combines composed fields into one kernel.
2. **Analytic** (opportunistic). Pattern-match known closed forms before falling through to Burn. `Gradient(Gaussian)` has an exact form; `Gradient(Add(a,b)) = Gradient(a) + Gradient(b)` simplifies recursively.
3. **WGSL direct** (deferred). Emit WGSL source that the Vello compute pipeline consumes directly. Useful when fields drive shader-level effects (background visualization). Not required for v1.

Backend selection is per `FieldId`, not global. The registry chooses the cheapest backend that handles the expression.

### 3.6 Rhai composition surface

Rhai owns canvas-author-time composition. **Illustrative DSL**:

```rhai
// illustrative — not yet wired
let focus = field("focus", gaussian(at: cursor, sigma: 200));
let flow  = field("citation_flow", gradient(focus));

project_z_from(focus);                          // generalizes ZSource

couple(role: "paper", field: focus, response: "attract_to_min", strength: 1.0);
couple(role: "paper", repel_kind: "paper", strength: 0.3);    // force-directed falls out

edge_path(role: "cites", trace: flow);

projection_preset("two_point_five.cabinet");
twod_preset("paper");
```

Rhai engine instance is per-canvas. Hot reload: re-parse + re-build registry + diff couplings. AST-walker perf is fine for compose-time (kHz at most).

### 3.7 Scripting — what stays, what changes

| Concern | Substrate |
| --- | --- |
| Per-canvas field & coupling composition | **Rhai** (new) |
| Per-canvas event hooks (`on_select`, `on_drag`, `on_layout_settle`) | **Rhai** |
| Per-object behavior scripts (today: `scripting.rs::SceneObjectOutput`) | **Rhai** for first-party (default) |
| Untrusted third-party object scripts | **Deferred**. When the trust boundary becomes real, layer `wasmi` (browser-friendly) or `Wasmtime` (native-only). Not v1 scope. |
| Multi-language script ecosystems | **Deferred** (same reasons). |

The existing `scripting.rs` stays as a data-types module: `ScriptCapability`, `SceneObjectOutput`, `ScriptDiagnostic`. Rename intent in module docs from "Wasmtime/Extism" to "host-side script runtime (Rhai for v1)." Capability gating is preserved — Rhai scripts are also capability-checked at the `#[export_module]` boundary.

### 3.8 Lossless dimension ladder + presets

Extends the existing `ProjectionMode`. **Illustrative**:

```rust
// illustrative
pub enum TwoDPreset {
    Plain,
    Paper,            // subtle warm background, soft drop shadows
    Terrain,          // contour lines from a designated scalar field
    Heatmap(FieldId), // false-color a scalar field as background
    Grid,
}

pub enum TwoPointFiveProjection {
    Isometric { dimetric_angle: f32 },        // 30°/30° default; covers dimetric variants
    Cabinet { z_scale: f32, angle: f32 },     // classic CAD half-depth
    Cavalier { angle: f32 },                  // full-depth axonometric
    Tilted { tilt_deg: f32 },                 // top-down camera tilted
    MildPerspective { focal: f32 },           // soft vanishing-point hint
}

pub enum ViewDimension {
    TwoD { preset: TwoDPreset },
    TwoPointFive { projection: TwoPointFiveProjection, z_field: FieldId },
    ThreeD { /* deferred */ },
}
```

`Isometric` becomes a member of the 2.5D family (Mark's framing) rather than a sibling mode. Lossless transitions:

- `2D → 2.5D`: derive z by evaluating the chosen z-field at every node position; pick a 2.5D projection preset.
- `2.5D ↔ 2.5D` (preset swap): swap projection params; z stays.
- `2.5D → 3D`: unlock camera at the projection preset's current pose; z stays; couplings unchanged.
- `Any → 2D`: discard z; `(x, y)` preserved.
- `Preset within 2D`: visual change only; no state mutation.

The existing `projection.rs` `TwoPointFiveConfig`/`IsometricConfig` types become param structs *inside* `TwoPointFiveProjection` variants.

---

## 4. Promotion: `ZSource` → `FieldProjection`

`ZSource` today maps node metadata → scalar z. The same shape generalizes:

```rust
// illustrative
pub struct FieldProjection {
    pub fields: HashMap<FieldId, FieldDef>,
    pub couplings: Vec<Coupling>,
    pub edge_path_rules: Vec<EdgePathRule>,
    pub z_field: Option<FieldId>,            // recovers ZSource role
    pub canvas_field_policy: CanvasFieldPolicy,
}
```

The old `ZSource::{Zero, Recency, BfsDepth, UdcLevel, Manual}` enum is preserved as a constructor convenience that builds an equivalent `FieldDef` and sets it as `z_field`. No public API break in the migration step.

---

## 5. Phases

### Phase 1 — `fields/` module skeleton

- New `crates/graph/graph-canvas/src/fields/` with: `ast.rs`, `registry.rs`, `coupling.rs`, `eval.rs` (no backend yet; pure AST construction + serde).
- All files ≤ 400 LOC target.
- Tests: AST construction, serde roundtrip, registry insert/lookup.

**Done condition**: `cargo test -p graph-canvas` green; no Burn dependency yet.

### Phase 2 — Burn-wgpu lowering

- Add `burn` + `burn-wgpu` to `graph-canvas/Cargo.toml` behind a feature flag (`field-burn`, default on).
- New `fields/lower_burn.rs` implementing AST → Burn tensor program.
- New `fields/eval.rs` driver: `eval_scalar(field, &[Point2D]) → Vec<f32>` and vector counterpart.
- Tests: known fields evaluated against analytic ground truth.

**Done condition**: a Gaussian field, its gradient (computed by Burn), and a force-directed coupling pass produce the same deltas as today's `compute_region_effects` for an Attractor region (within ε).

### Phase 3 — Promote ZSource

- Add `FieldProjection` alongside `ZSource`. `ProjectionMode::TwoPointFive { z_source }` gains a `z_field: Option<FieldId>` companion.
- `ZSource` enum constructors that produce `FieldDef` entries.
- Document migration path in projection.rs module doc.

**Done condition**: existing `ZSource`-driven tests pass unchanged; new `z_field`-driven tests exercise the same projections via the field algebra.

### Phase 4 — Rhai composition surface

- New crate dependency: `rhai`.
- New `fields/rhai_bindings.rs` exposing `field`, `gradient`, `couple`, `edge_path`, `project_z_from`, `projection_preset`.
- Per-canvas Rhai engine instance with hot-reload of the canvas script.
- Capability allowlist enforced via `#[export_module]` exposure scope (no `print`, no `eval`, no I/O imports).

**Done condition**: a sample canvas script composes a focus field, attaches a coupling, sets a projection preset, and `cargo test` validates the resulting registry matches a hand-built equivalent.

### Phase 5 — Lossless ladder + presets

- Refactor `projection.rs` to introduce `TwoDPreset`, `TwoPointFiveProjection` enums; keep old types as deprecated re-exports for one cycle.
- Add transition logic: `transition(view, from, to) -> TransitionPlan` that names what's preserved/discarded.
- Diagnostics for blocked transitions (e.g. `2D → 3D` while Standard is unrenderable).

**Done condition**: 2D ↔ 2.5D ↔ preset-swap roundtrips preserve `(x, y)` and selection; transition diagnostics emit the right reason codes.

### Phase 6 — Force-directed as a coupling

- Replace the special-case node-separation pass in `scene_physics.rs` with a coupling rule (`AlignVelocity` against a node-emitted-repulsor field).
- Edge attraction becomes `AttractToMin` against a per-edge spring potential.
- Validate: existing layout looks/feels equivalent on demo graphs.

**Done condition**: layout demo produces visually-equivalent settling behavior; LOC of `scene_physics.rs` reduced (separation logic now lives as field-eval).

### Phase 7 — File-split discipline (parallel hygiene)

Concurrent with Phases 1–6, enforce the 600 LOC ceiling on touched files. See §6.

### Phase 8 — `FieldLine` edge paths (motivator delivery)

- Implement `EdgePath::FieldLine { field, max_steps }` via RK4 integration.
- Demo canvas with edges traced through a curl-free flow field.

**Done condition**: monocurl-style field-line edges render with selectable target field per edge role.

### Phase 9 (deferred) — Sandboxed wasm layer

- Only when third-party untrusted scripts become a real requirement.
- Choice: `wasmi` (browser-compatible) or `Wasmtime` (native-only) per target.
- Plan to be drafted then; not part of this plan.

### Phase 10 (deferred) — WGSL direct backend

- Emit WGSL source for fields driving shader-level effects.
- Integrate with Vello compute pipeline once Vello adoption lands.

---

## 6. File hygiene — 600 LOC ceiling enforcement

| File | Today | After this plan |
| --- | --- | --- |
| `scene_physics.rs` | **715 LOC** ⚠️ over | Split: `scene_physics/separation.rs`, `scene_physics/containment.rs`, `scene_physics/region_effects.rs`, `scene_physics/motion_profile.rs`, `scene_physics/mod.rs` re-exports |
| `projection.rs` | 451 LOC (near) | Split when presets land: `projection/types.rs`, `projection/math.rs`, `projection/presets.rs` |
| `scripting.rs` | comfortably under | Stays as data-types-only |
| New `fields/ast.rs` | — | Target ≤ 350 LOC |
| New `fields/lower_burn.rs` | — | Target ≤ 450 LOC |
| New `fields/coupling.rs` | — | Target ≤ 300 LOC |
| New `fields/rhai_bindings.rs` | — | Target ≤ 350 LOC |

Ceiling rule: if a file approaches 550 LOC, split before adding more. Test files exempt from the ceiling.

---

## 7. Findings

### 7.1 Browser/PWA constraint determined the scripting choice

Wasmtime cannot run inside a browser sandbox (requires JIT, browser disallows). Rhai compiles to `wasm32-unknown-unknown` natively. Burn-wgpu targets WebGPU. The combination `Rhai + Burn-wgpu + wgpu` is one toolchain on every target Mere is likely to ship to (native, PWA, MV3 service worker). Wasmtime-everywhere would force a second integration (`wasmi`) for the browser case. The prior `2026-04-10` plan's Wasmtime canonicalization predates the cross-platform ambition this workspace is building toward.

### 7.2 ZSource was already the seed of the field idea

The existing `ZSource::{Recency, BfsDepth, UdcLevel}` variants are scalar projections from node metadata. Generalizing them is the cleaner move than parallel construction.

### 7.3 SceneRegionEffect was already a bounded field

`Attractor`, `Repulsor`, `Dampener`, `Wall` are field-coupling primitives confined to a spatial shape. Promoting them to be one expressive form within the field algebra (as a `Disk` kernel × `AttractToMin` coupling) eliminates a parallel system.

### 7.4 `scene_physics.rs` predates the planned split

At 715 LOC it already violates the workspace ceiling. The split is owed regardless of this plan; folding it into Phase 7 ensures it lands as part of the field re-grounding rather than as orphaned hygiene work.

### 7.5 Open architectural question — graph_docs/ vs. graphshell_docs/

`graph-canvas`, `graph-memory`, `graph-tree` form a parallel crate family under `crates/graph/`, but no `graph_docs/` directory exists in `design_docs/`. This plan lives in `graphshell_docs/implementation_strategy/` for now. If more graph-family plans materialize, propose spinning up `graph_docs/` per `DOC_POLICY.md` §1.

---

## 8. Progress

| Date | Phase | Note |
| --- | --- | --- |
| 2026-05-07 | 0 | Plan drafted. Existing primitives surveyed; promotion path identified; phases sequenced. |
| 2026-05-07 | 1 | `fields/` module landed in `graph-canvas`: `ast.rs` (`ScalarField`, `VectorField`, `Falloff`), `registry.rs` (`FieldId`, `FieldDef`, `FieldRegistry`), `coupling.rs` (`Coupling`, `CouplingResponse`, `NodeSelector`, `EdgePath`, `EdgePathRule`), `eval.rs` (`eval_scalar`, `eval_vector`, `grad_scalar` with analytic+finite-diff fallback). 53 new tests; serde roundtrips covered. All files under the 600 LOC ceiling. |
| 2026-05-07 | 2 | Burn-lowering scaffold landed as `fields/lower_burn.rs` behind the `field-burn` feature flag. Real `burn`/`burn-wgpu` deps **not yet added** — the module ships as `lower_scalar`/`lower_vector` stubs returning `LowerError::NotImplemented`. The seam exists; the dep + actual lowering is a follow-up slice gated on a Burn version probe against the workspace's Rust 1.92.0 toolchain and CubeCL fusion maturity. |
| 2026-05-07 | 2 (real) | Burn 0.16.1 wired in as an optional dep behind `field-burn` (ndarray + std features; wgpu deferred). `lower_scalar` / `lower_vector` / `lower_gradient` walk the AST and emit Burn tensor programs evaluated at rank-1 `(xs, ys)` inputs. Operators landed: `Const`, `CoordX`, `CoordY`, `Time`, `Add`, `Mul`, `Scale`, `Negate`, `Gaussian` (const center), `Linear` (const normal), `Sample`, `VectorField::{ConstVec, Coord, Add, ScaleConst, Perp, Sample}`, plus closed-form `Gradient(Gaussian)` and `Gradient(Linear)`. 16 backend tests assert Burn output matches CPU analytic eval within 1e-5 on Gaussian, Linear, gradients, and combinators. The plan's Phase 2 "Burn-computed Gaussian gradient" done condition is satisfied; force-directed-coupling delta equivalence (the second half of that condition) carries over to Phase 6. Unsupported variants (`Disk`, `Dot`, `Mul`-gradient, `Vector::Scale`) return typed `LowerError`. |
| 2026-05-07 | 3 | `FieldProjection` landed at `fields/projection.rs` alongside the existing `ZSource` enum. `FieldProjectionBuilder::from_z_source` constructs equivalent registries for the legacy `ZSource::{Recency, BfsDepth, UdcLevel}` variants. No public API break — `ZSource` is preserved as the migration distance. |
| 2026-05-07 | 4 | Rhai composition surface scaffold landed as `fields/rhai_bindings.rs` behind the `field-rhai` feature flag. Real `rhai` dep **not yet added** — the module ships as a `build_from_script` stub returning `BuildError::NotImplemented`. Same rationale as Phase 2: ship the seam, defer the dep until script-storage shape (per §9 Q5) is decided. |
| 2026-05-07 | 4 (real) | Rhai 1.24 wired in as an optional dep behind `field-rhai` (default-features off + std). `build_engine()` registers `FieldProjection`, `ScalarField`, `VectorField` as Rhai types and exposes constructors (`gaussian`, `linear`, `scalar_const`, `scalar_x/y/time`, `vector_const`, `vector_coord`, `gradient`, `perp`), combinators (`add`, `mul`, `negate`, `scale_by`), projection methods (`add_scalar`, `add_vector`, `set_z_field`, `clear_z_field`), coupling methods (`couple_attract`, `couple_repel`, `couple_align`, `couple_advect`, `couple_wall`, `couple_dampen_inside`), and edge-path methods (`edge_path_field_line`, `edge_path_straight`, `edge_path_spline`). `build_from_script` evaluates a script and returns a `FieldProjection`. 11 binding tests cover empty scripts, single-field composition, id chaining, multiple couplings, dampen-inside factor preservation, field-line edge paths, full canvas script (registry + couplings + edge rules + z_field together), parse-error reporting, and the combinator surface. |
| 2026-05-07 | 5 | `projection.rs` split into `projection/{mod,types,math,presets}.rs`. Public re-exports preserve the existing `crate::projection::*` surface (no consumer-facing breakage). New `presets.rs` adds `TwoDPreset` (Plain, Paper, Terrain, Heatmap, Grid) and `TwoPointFiveProjection` (Isometric, Cabinet, Cavalier, Tilted, MildPerspective) as additive types — wiring them into `ProjectionMode` itself is a follow-up. Each split file under 350 LOC. |
| 2026-05-07 | — | **Verification (scaffold pass)**: 286 graph-canvas lib tests passing with `--features field-burn,field-rhai` (283 default); `cargo check --workspace --all-targets` zero warnings; `cargo test --workspace` zero failures. Nothing in dependent crates broke from the projection split. |
| 2026-05-07 | — | **Verification (real Burn + Rhai pass)**: 310 graph-canvas lib tests passing with `--features field-burn,field-rhai` (283 default; +16 Burn lowering, +11 Rhai bindings, −0 net regressions). Workspace `cargo test --workspace` clean. `cargo fmt -p graph-canvas` clean. |
| 2026-05-07 | dependency hygiene | Burn version-probe: bumped from 0.16.1 → 0.21.0 (5 minor versions of CubeCL / fusion improvement). One API delta: `Backend::Device` moved to `BackendTypes::Device` — single `use burn::tensor::backend::BackendTypes` import fix. Rhai already at latest 1.24.0 (Cargo resolved `"1.20"` to current). |
| 2026-05-07 | 2 (extended) | Burn operator coverage extended: `Disk` forward (all four falloffs — Hard / Linear / Smoothstep / Quadratic, with mask-fill outside radius), `Dot` (vector·vector → scalar), `VectorField::Scale` (scalar-times-vector), and `Mul` gradient via the **product rule** `∇(a·b) = a∇b + b∇a`. Plus a new `field-burn-wgpu` feature flag enabling `burn/wgpu` for downstream consumers (no wgpu-specific code in graph-canvas; lowering remains generic over `B: Backend`). 7 new Burn tests; only `Gradient(Disk)` and `Gradient(Dot)` remain unsupported (piecewise / nontrivial). |
| 2026-05-07 | 7 | `scene_physics.rs` (715 LOC) split into a directory module: `mod.rs` (`ScenePhysicsConfig`, `NodeSnapshot`, `SceneEvent`), `separation.rs`, `containment.rs`, `region_effects.rs`, `motion_profile.rs`. All sub-files under 240 LOC. 22 tests redistributed into co-located test modules; behavior parity preserved. |
| 2026-05-07 | 5 (full break) | `ViewDimension` reshaped to `TwoD { preset: TwoDPreset } \| TwoPointFive { projection: TwoPointFiveProjection, z_field: Option<FieldId> } \| ThreeD`. **`ZSource` and `ThreeDMode` enums removed**. `Isometric` is now a member of the 2.5D projection family, not a sibling. `ProjectionMode` kept as a `pub type ProjectionMode = ViewDimension;` alias for back-compat. `IsometricConfig` / `TwoPointFiveConfig` removed (per-projection params now in variant fields). `ProjectionConfig` retained as an empty struct so existing `derive.rs` signatures don't churn. New axonometric/tilted/mild-perspective projection math in `projection/math.rs`. Consumer updates: `derive.rs`, `scene.rs`, all `layout/*` test sites, `platen::canvas_scene`, `graphshell::app_state::composition` test cases. |
| 2026-05-07 | — | **Final verification**: 314 graph-canvas tests passing with `--features field-burn,field-rhai`; workspace `cargo test --workspace` zero failures across all crates (graphshell, platen, verso-tile, inker, etc.); `cargo check --workspace --all-targets` zero warnings; `cargo fmt --all` clean. |
| 2026-05-08 | tier-2 statistical | New `intelligence-embeddings` crate delivers the field-algebra-adjacent statistical-intelligence tier: `EmbeddingProvider` trait, `HashedEmbeddingProvider` (deterministic test impl), pure-Rust flat `VectorIndex<K>`, eidetic-backed persistence, `SemanticSearch<K, P>` facade, and Bridge-A field integration via `register_query_similarity_field` (renders per-node similarity-to-query as a sum of weighted gaussians composed inside the existing field-algebra AST — no new AST variant needed). 35+ tests; pure-Rust, browser-friendly. |
| 2026-05-09 | tier-2 BERT | Burn-backed BERT provider wired end-to-end behind the `bert` feature flag. Full layer stack implemented in Burn 0.21 (`BertEmbeddings`/`BertSelfAttention`/`BertSelfOutput`/`BertAttention`/`BertIntermediate`/`BertOutput`/`BertLayer`/`BertEncoder`/`BertModel`), with `from_loaded` constructors that bypass random init via direct `Param::from_tensor` field assignment on Burn's nn primitives. Safetensors loader (`load_artifacts` + `load_into_model`) handles HF layout (config.json + tokenizer.json + model.safetensors), validates expected tensor names + shapes, transposes PyTorch `[out, in]` → Burn `[in, out]` at the extraction boundary, maps HF `LayerNorm.weight/bias` → Burn `gamma/beta` at the construct boundary. `BertEmbeddingProvider::<B>::load(model_dir, device)` is the one-shot entry point. Tiered validation strategy in place: Tier-1 cheap fixture comparison + Tier-2 continuous (gated on `bert-validation` feature). 108 active unit tests + 5 integration tests + 6 ignored-pending-real-weights tests. Outstanding work is environmental (capture reference fixtures from any source, run validation tier-1, confirm/fix three known empirical sites). |

---

## 9. Open questions to resolve before Phase 2

1. **Burn version pinning**: which Burn release? CubeCL fusion maturity matters for the per-frame eval cost. Worth a research probe before Phase 2.
2. **Field metadata schema**: do we serialize the full AST in canvas snapshots, or compile to a stable bytecode? AST is simpler; bytecode is more durable across schema evolution.
3. **`CanvasFieldPolicy` extension point**: parallel to existing `CanvasStylePolicy`/`CanvasNavigationPolicy`/`CanvasTopologyPolicy`? Likely yes; confirm naming.
4. **Coupling-vs-region public API**: do we deprecate `SceneRegionEffect` immediately or leave both surfaces for one release cycle?
5. **Rhai script storage**: per-canvas script as a string field on the view snapshot, or referenced asset? String is simpler; asset enables sharing.
