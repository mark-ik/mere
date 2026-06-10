# The field system, and the decomposition of graph-canvas

**Date**: 2026-05-30
**Status**: Architecture decision + decomposition plan. Establishes **fields as a
third graph primitive** (beside nodes and edges), extracts the field-algebra
runtime into its own crate, names the physics-substrate sibling pair
(**`aether`** + **`gyre`**), and maps where the rest of the 9.6k-LOC
`graph-canvas` crate goes as it comes apart.
**Related**: [composition spine](2026-05-21_mere_composition_spine.md),
[cartography-aether seam](2026-05-29_cartography_aether_layout_seam.md),
[serval-as-host eval](2026-05-29_serval_as_host_evaluation.md),
[cartography brief](../research/2026-05-10_cartography_layer_brief.md),
[local-intelligence research](../research/2026-05-08_local_intelligence_integration_research.md).

---

## 1. The realization that drives everything

`graph-canvas/src/fields/coupling.rs` says it in its own header: *"Force-directed
layout falls out as one such coupling (a per-node-emitted repulsive scalar
field),"* and the `scene_region` effects (Attractor / Repulsor / Dampener / Wall)
are bounded-shape fields with fixed responses.

So the field system is not a graph-canvas feature. **It is the general framework
of which the physics integrator's hand-written forces are special cases.** The
integrator owns *motion* (rapier bodies, collision, stepping); the field system
is the *source* of what it integrates. `NodeExclusion` is a per-node repulsive
field coupled with a force response; `EdgeSpring` is an edge-length field;
`Boundary` is a centering field. They are instances.

The two crates are named in §5: the field system is **`aether`**, the integrator
**`gyre`**. This is the load-bearing insight; everything below follows from it.

## 2. Fields are a third graph primitive

Decision (Mark, 2026-05-30): a **Field** is a first-class graph primitive in the
kernel, on the order of a node or an edge, *not* a node-kind with a payload. The
reason is lifecycle: a real graph element gets creation, activation, retirement,
persistence, and federation, and a field needs all of those (you author a field,
it lives in the graph, it is shared, it is retired). Building lifecycle on top of
the field definition is only honest if the field is a real element.

The shape:

- **`Field`** (new primitive, `FieldKey` beside `NodeKey` / `EdgeKey`): identity,
  a portable field *definition* (the scalar/vector AST as data, serializable, no
  Rhai/Burn dependency), an *extent* (global / a region / attached to a node),
  and lifecycle state. The definition is truth; the runtime that evaluates it is
  not.
- **`Coupling`** (a relation, field → targets): which elements respond (the
  existing `NodeSelector`: all / by tag / by kind / not-tag), *how* they respond,
  and a strength. A coupling is edge-shaped (it connects a field to the elements
  it acts on), so it joins the relation taxonomy as a new **Coupling family**
  beside Semantic / Traversal / Containment / Arrangement / Imported / Provenance.

The kernel-model integration detail (whether the petgraph store grows
heterogeneous endpoints, or fields live in a parallel keyed store that edges can
reference) is for the kernel slice; the decision here is that fields and
couplings are *truth*, persisted and federated like the rest.

**Coupling is deliberately more than physics and visual.** A response is "how an
element reacts to a field's value at its position," and that is extensible by
design: force (→ the physics substrate), visual (→ paint overlays/styling), and
beyond — navigational (a field that pulls the camera or biases fit), selection (a
field that gathers nodes), semantic (a field that tags or scores elements),
trigger (a field whose threshold fires an action). The v1 surface is force +
visual; the *contract* is open, because a spatial-influence framework over a
graph is worth more than a physics helper.

## 3. The field-algebra runtime is its own crate (`aether`)

The evaluation engine moves out of graph-canvas into a dedicated substrate-tier
crate, **`aether`**:

- The field **AST** (scalar / vector field algebra).
- **Rhai** authoring (script → AST), feature-gated (`field-rhai`).
- **Burn** lowering (AST → a Burn tensor program), CPU `ndarray` and GPU `wgpu`
  backends, feature-gated (`field-burn` / `field-burn-wgpu`). Burn is a *real*
  substrate here, not aspirational — the integration was built far enough to
  download a model and exercise it.
- The `FieldRegistry` (evaluators keyed by `FieldId`) and the coupling resolver.

It reads `Field` + `Coupling` primitives from the graph, evaluates the fields
(Burn on GPU when present, `ndarray` otherwise), resolves couplings, and emits the
responses. It depends on kernel (the primitives) and Rhai/Burn (the runtime); it
is portable (Rhai + Burn run everywhere wasm32 ships, matching the
browser/PWA scripting direction), so it survives the serval-as-host flip
untouched.

## 4. The seams

**To the physics substrate (forces).** `aether` resolves a force coupling into a
force that **`gyre`** integrates. `gyre` is the rapier crate (bodies, collision,
stepping); its per-tick force hook is the trait the built-ins implement today as
`aether::Field` and which becomes `gyre::Force` after the rename (§5), since
"field" now names the `aether` crate. A coupling compiles to one of those force
contributors. `gyre`'s built-in `NodeExclusion` / `EdgeSpring` / `Boundary` stay a
fast Rust default-path that everyone gets cheaply; `aether` is the general,
scriptable path that produces the same kind of output. They coexist, which
matches the configurability stance: a cheap default, a deep override.

**To paint (visual).** A visual coupling resolves to overlays the renderer draws
through [`platen::scene_paint`](2026-05-29_cartography_aether_layout_seam.md) (the
Projection → PaintList path). Field isolines, gradient washes, and region tints
are PaintCmds like any other scene content.

**To intelligence (meaning).** This is the seam that pays off. A Burn embedding
(the [local-intelligence](../research/2026-05-08_local_intelligence_integration_research.md)
layer) becomes a vector field in `aether`; a coupling makes selected nodes follow
its gradient; `gyre` integrates the result. *Force-directed-by-meaning* stops
being a bespoke layout strategy and becomes "a semantic field, coupled." One Burn
substrate serves both the intelligence layer and `aether`, rather than two.

## 5. Naming: `aether` (field) + `gyre` (integrator)

Decided (Mark, 2026-05-30). The substrate splits into the crate that *defines*
fields of influence and the crate that *realizes* them as motion, and the names
are an etymological pair, not an arbitrary one:

- **`aether`** = the field-algebra crate. The luminiferous aether was precisely
  the *field-bearing medium*: Faraday's lines of force and Maxwell's
  electromagnetic field were states and stresses *of* the aether. The field is
  what the aether *does*, so the name belongs on the crate that defines fields,
  not on the body-mover. (This is the correction to the earlier framing, which
  had aether as the integrator.)
- **`gyre`** = the integrator. The wheeling, turning motion of bodies (Yeats's
  "the widening gyre"); `gyre` realizes the aether's potentials as motion. It also
  shares a sibilance with **orrery** (the t1 graph view), so the physics that
  moves the orrery's bodies reads as kin to the orrery itself.

Rejected: `dynamis` (swamped), `welkin` / `firmament` (a static expanse, which
fits a field-arena, not a motion-engine, and once `aether` takes the field role
they are redundant), `kinesis` (swamped), `numen` / `flux` (weaker than the
etymological pair).

**Consequence — a crate rename.** Today's `aether` crate *is* the rapier
integrator, so it is renamed to `gyre`, freeing `aether` for the new
field-algebra crate. The integrator's `aether::Field` trait (its per-tick force
hook) becomes `gyre::Force`. The [cartography-aether seam](2026-05-29_cartography_aether_layout_seam.md)
doc and the `aether::Simulation` references in it rename to `gyre` during that
slice (§8 step 0).

## 6. Where the rest of graph-canvas goes

| graph-canvas module(s) | Destination |
|---|---|
| `fields/*`, `scene_region` | **the field system** (§2–§3): definitions → kernel `Field`/`Coupling` primitives; AST + Rhai + Burn → the `aether` crate. `scene_region`'s four effects become built-in field shapes. |
| `scene_physics`, `scene_composition`, `hit_test` | **`gyre`** (physics + the QueryPipeline; today's `aether` rapier crate, renamed). |
| `projection` | **cartography** / **arrangements** (positions, already there). |
| `derive`, `packet`, `scene`, `backend` | the graph→draw pipeline collapses into **cartography** (positions) → **platen::scene_paint** (PaintList). |
| `camera`, `navigation` | **understory** view2d steal-the-shape + a small portable canvas-feel config (the `NavigationPolicy` knobs). |
| `node_style` | **register-theme** (the look layer). |
| `lod` | cull mechanics → `gyre`; LOD *policy* rides with the renderer (platen). |
| `engine`, `interaction`, `input` | the **host**. Under serval-as-host, serval's hit-test + dispatch own interaction; the `CanvasAction` indirection collapses. |
| `scripting` | reconciles into the Rhai scripting story (the Wasmtime/Extism framing here is superseded by the browser/PWA Rhai+Burn direction). |
| `math`, `types` | absorbed at use sites (geometry → `kernel::geometry`). |

Net: graph-canvas dissolves. Its physics was already the integrator's, its
projection cartography's, its paint platen's; its one irreducible contribution,
the field algebra, graduates to a first-class primitive plus the `aether` crate.

## 7. Spine placement

Fields join the **truth** row of the spine (kernel: nodes, edges, fields), beside
node-lineage as another thing the graph carries. The **`aether` runtime** and
**`gyre`** are the physics substrate. The flow:

```
truth (kernel: nodes / edges / fields + couplings)
  → aether (evaluate fields, resolve couplings: Burn on GPU)
  → gyre   (integrate forces: rapier bodies, collision, settle)
  → cartography (positions as a Projection)
  → platen::scene_paint (Projection + visual couplings → PaintList)
  → netrender → host
```

The spine's substrate tier gains one sentence: *fields are first-class graph
elements; the `aether` runtime evaluates them into forces and visual responses,
`gyre` integrates the forces, and the result projects and paints like any scene.*

## 8. Sequencing

Not all at once. A workable order, each slice host-agnostic and testable:

0. **Rename today's `aether` rapier crate to `gyre`** (and `aether::Field` →
   `gyre::Force`), updating the cartography-aether seam doc + `Simulation`
   references. Mechanical, frees the `aether` name. **Done 2026-05-30 (commits
   `0bb0337`, `1d94746`); the seam doc's references are synced to `gyre`.**
1. **Extract the `aether` field-algebra crate** from `graph-canvas/fields/*`
   (AST + eval + Rhai + Burn + registry + coupling), depending only on kernel +
   Rhai/Burn. Pure move; its existing tests come with it. **Done 2026-05-31
   (commit `9ea5858`)** — the landed crate is standalone (serde + optional
   burn/rhai, *no* kernel dep yet); the kernel dependency arrives with step 3,
   when Field/Coupling become kernel truth.
2. **The `gyre` seam**: a coupling → `gyre::Force` adapter, so a force coupling
   drives the rapier bodies. Re-express one built-in (e.g. `NodeExclusion`) as a
   coupling to prove equivalence, keep the built-in as the fast path.
3. **The `Field`/`Coupling` kernel primitive** + lifecycle (the bigger, truth-
   level slice; its own plan: the
   [field/coupling kernel-primitive plan](../implementation_strategy/2026-05-31_field_coupling_kernel_primitive_plan.md)).
   Until it lands, `aether` reads fields from an in-memory registry rather than
   the graph.
4. **Dissolve graph-canvas**: relocate the rows in §6 as their consumers need
   them; retire the shell when empty (the fit-map's adopt/retire call, now
   answered: retire, harvesting the field algebra).

Steps 0–2 are the immediate host-agnostic work; step 3 is the load-bearing kernel
change that deserves its own plan; step 4 is opportunistic cleanup.
