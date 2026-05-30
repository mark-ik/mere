# The field system, and the decomposition of graph-canvas

**Date**: 2026-05-30
**Status**: Architecture decision + decomposition plan. Establishes **fields as a
third graph primitive** (beside nodes and edges), extracts the field-algebra
runtime into its own crate, names the physics-substrate sibling pair, and maps
where the rest of the 9.6k-LOC `graph-canvas` crate goes as it comes apart.
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
of which aether's hand-written force fields are special cases.** That inverts the
earlier sketch ("aether owns fields"): aether is the *integrator*, and the field
system is the *source* of what it integrates. `NodeExclusion` is a per-node
repulsive field coupled with a force response; `EdgeSpring` is an edge-length
field; `Boundary` is a centering field. They are instances.

This is the load-bearing insight. Everything below follows from it.

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

## 3. The field-algebra runtime is its own crate

The evaluation engine moves out of graph-canvas into a dedicated crate in the
graph-substrate tier:

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

**To the physics substrate (forces).** The runtime resolves a force coupling into
the existing `aether::Field` contract: a coupling compiles to a `Field` whose
`apply` writes the field's force onto the bodies it selects. aether integrates
(rapier bodies, collision, stepping) and owns nothing about how the force was
defined. aether's built-in `NodeExclusion` / `EdgeSpring` / `Boundary` stay as a
fast Rust default-path that everyone gets cheaply; the field runtime is the
general, scriptable path that produces the same kind of output. They coexist,
which matches the configurability stance: a cheap default, a deep override.

**To paint (visual).** A visual coupling resolves to overlays the renderer draws
through [`platen::scene_paint`](2026-05-29_cartography_aether_layout_seam.md) (the
Projection → PaintList path). Field isolines, gradient washes, and region tints
are PaintCmds like any other scene content.

**To intelligence (meaning).** This is the seam that pays off. A Burn embedding
(the [local-intelligence](../research/2026-05-08_local_intelligence_integration_research.md)
layer) becomes a vector field in this crate; a coupling makes selected nodes
follow its gradient; aether integrates the result. *Force-directed-by-meaning*
stops being a bespoke layout strategy and becomes "a semantic field, coupled."
One Burn substrate serves both the intelligence layer and the field runtime,
rather than two.

## 5. Naming: the substrate sibling pair

The substrate now has two crates that want to read as siblings: the one that
*defines* the fields of influence, and the one that *realizes* them as motion.
aether's own doc already calls it "the medium through which forces propagate,"
which is exactly the integrator's role, so the question is mostly what to name the
new field crate (and whether to re-pitch aether against it).

The conceptual pair is **potential → motion** (Aristotle's dynamis → energeia: a
capacity, and its realization). Options, recommendation first:

| Field-algebra crate | Integrator crate | Reading |
|---|---|---|
| **`dynamis`** | **`aether`** (kept) | the potentials (dynamis) propagate through the medium (aether) into motion. Keeps aether's apt, established name; adds a scholarly sibling. **Recommended.** |
| `dynamis` | `kinesis` | a matched Greek pair, potential / motion, if aether should be renamed for symmetry. Loses aether's "medium" resonance. |
| `numen` | `aether` (kept) | more numinous: a `numen` is a presiding influence/emanation over space. Reads less like physics, more like presence. |
| `flux` | `aether` (kept) | physics-plain: the field/flow (`flux`) through the medium. Clearest, least evocative. |

Recommendation: **`dynamis` + `aether`**. It keeps the name that already means
the right thing, and "dynamis" (potential, power, the capacity to influence) names
the field crate for what a field *is* before it acts. The naming is yours; this is
the proposal.

## 6. Where the rest of graph-canvas goes

| graph-canvas module(s) | Destination |
|---|---|
| `fields/*`, `scene_region` | **the field system** (§2–§3): definitions → kernel `Field`/`Coupling` primitives; AST + Rhai + Burn → the `dynamis` crate. `scene_region`'s four effects become built-in field shapes. |
| `scene_physics`, `scene_composition`, `hit_test` | **aether** (physics + the QueryPipeline; already there). |
| `projection` | **cartography** / **graph-layout** (positions, already there). |
| `derive`, `packet`, `scene`, `backend` | the graph→draw pipeline collapses into **cartography** (positions) → **platen::scene_paint** (PaintList). |
| `camera`, `navigation` | **understory** view2d steal-the-shape + a small portable canvas-feel config (the `NavigationPolicy` knobs). |
| `node_style` | **register-theme** (the look layer). |
| `lod` | cull mechanics → aether; LOD *policy* rides with the renderer (platen). |
| `engine`, `interaction`, `input` | the **host**. Under serval-as-host, serval's hit-test + dispatch own interaction; the `CanvasAction` indirection collapses. |
| `scripting` | reconciles into the Rhai scripting story (the Wasmtime/Extism framing here is superseded by the browser/PWA Rhai+Burn direction). |
| `math`, `types` | absorbed at use sites (geometry → `kernel::geometry`). |

Net: graph-canvas dissolves. Its physics was already aether's, its projection
cartography's, its paint platen's; its one irreducible contribution, the field
algebra, graduates to a first-class primitive plus a substrate crate.

## 7. Spine placement

Fields join the **truth** row of the spine (kernel: nodes, edges, fields), beside
node-lineage as another thing the graph carries. The **`dynamis` runtime** and
**aether** are the physics substrate. The flow:

```
truth (kernel: nodes / edges / fields + couplings)
  → dynamis (evaluate fields, resolve couplings: Burn on GPU)
  → aether (integrate forces: rapier bodies, collision, settle)
  → cartography (positions as a Projection)
  → platen::scene_paint (Projection + visual couplings → PaintList)
  → netrender → host
```

The spine's substrate tier gains one sentence: *fields are first-class graph
elements; the `dynamis` runtime evaluates them into forces and visual responses,
aether integrates the forces, and the result projects and paints like any scene.*

## 8. Sequencing

Not all at once. A workable order, each slice host-agnostic and testable:

1. **Extract `dynamis`** from `graph-canvas/fields/*` as a standalone crate
   (AST + eval + Rhai + Burn + registry + coupling), depending only on kernel +
   Rhai/Burn. Pure move; its existing tests come with it.
2. **The `aether` seam**: a coupling → `aether::Field` adapter, so a force
   coupling drives the rapier bodies. Re-express one built-in (e.g.
   `NodeExclusion`) as a coupling to prove equivalence, keep the built-in as the
   fast path.
3. **The `Field`/`Coupling` kernel primitive** + lifecycle (the bigger, truth-
   level slice; its own plan). Until it lands, `dynamis` can read fields from an
   in-memory registry rather than the graph.
4. **Dissolve graph-canvas**: relocate the rows in §6 as their consumers need
   them; retire the shell when empty (the fit-map's adopt/retire call, now
   answered: retire, harvesting the field algebra).

Steps 1–2 are the immediate host-agnostic work; step 3 is the load-bearing kernel
change that deserves its own plan; step 4 is opportunistic cleanup.
