# Scriptable Field Regions Plan

A **field region** is a spatial area you place on the graph that carries a rule
set — scriptable in rhai — governing the graph's characteristics inside it:
forces (how nodes are pulled), edge visibility (which relations show), and node
layout (how the contained nodes arrange). The third graph element beside nodes
and edges, but a *rule-bearing* one: place a region, write its rules, and the
graph within it behaves accordingly.

This is the "field" the user means — not a node attribute, a **spatial rule
region**. It unifies three subsystems that already exist separately (forces via
couplings, edge visibility via projection, layout via arrangements) under one
placeable, scriptable spatial primitive, and it is the third rhai lane after the
knot note-blocks and the omnibar command shell: one scripting language, now
governing a region of space.

## Findings (code-verified substrate)

The pieces exist; what is missing is **placement, rendering, and the unifying
rule surface**.

- **The kernel `Field` truth primitive** —
  [`graph-kernel/.../field.rs`](../../../crates/graph/graph-kernel/src/graph/field.rs):
  `Field { id: FieldId, name, definition: FieldDefinition, extent: FieldExtent,
  lifecycle }`. `FieldDefinition = Scalar(ScalarField) | Vector(VectorField)`.
  `FieldExtent = Global | Region{min_x,min_y,max_x,max_y} | AttachedToNode(Uuid)`
  — **`Region` is a world-space box, so a field already carries a spatial
  placement**. The AST (`field_ast.rs`) has spatially-anchored constructors:
  `ScalarField::disk_at(cx,cy,radius,Falloff)`, `gaussian_at(cx,cy,sigma)`,
  `Falloff::{Hard,Linear,Smoothstep,Quadratic}`. Fields live in
  `Graph.fields: HashMap<FieldId,Field>` + `couplings: HashMap<CouplingId,Coupling>`
  (a parallel keyed store, not petgraph weights), mutated via `field_ops.rs`
  (`add_field`, `add_coupling`, `retire_field`, `field(id)`, `fields()`).
- **Forces** — a `Field` does nothing alone; a `Coupling` (field → `NodeSelector`
  → `CouplingResponse` → strength) is resolved by gyre
  [`CouplingForce::from_coupling`](../../../crates/orrery/gyre/src/coupling_force.rs)
  into a rapier force the layout tick runs. So "forces inside the region" = a
  coupling whose selector is "nodes in the field's extent".
- **The rhai authoring path already exists** — aether
  [`FieldProjection`](../../../crates/orrery/aether/src/projection.rs) + its rhai
  bindings (`rhai_bindings.rs`: `gaussian`, `couple_attract`, …), committed to the
  graph via `FieldProjection::commit_to_graph`. **This is the seam the region's
  rule script extends**: today it is registry-id / global authoring; a field
  region scopes it to a placed extent.
- **Edge visibility** — the graphlet
  [`EdgeProjectionSpec`](../research/2026-06-13_edge_system_audit.md) (the design
  in [graphlet derivation](../design/2026-06-13_graphlet_derivation_from_selection.md))
  already models "which edge families count" for a node subset. A field region's
  edge-visibility rule is an `EdgeProjectionSpec` scoped to the nodes in its
  extent.
- **Layout** — the `orrery/arrangements` family (layout strategies) already
  arranges node subsets. A region's layout rule selects an arrangement applied to
  its contained nodes.
- **The gaps**: (1) no gesture to *place* a field at a world point (the just-shipped
  `add_node_at` is the exact pattern to mirror — `Orrery::add_field_at`); (2) the
  orrery does not *render* fields (placed fields are invisible — grep finds no
  `FieldExtent`/`fields()` use in `orrery/orrery/src`); (3) no *unified rule
  surface* binding forces + edge-visibility + layout to one region script.

## Design

A field region is a placed `Field` (with a `Region` extent) plus a **rule
script** the region evaluates over the nodes and edges within its extent. The
script is rhai, with privileged bindings in three domains:

- **Forces** — `couple(response, strength)` (attract/repel toward the field's
  min/max), realized as a `Coupling` over a `NodeSelector` resolving to the
  region's contained nodes. (Substrate: gyre `CouplingForce`.)
- **Edge visibility** — `show_edges(families)` / `hide_edges(families)`, an
  `EdgeProjectionSpec` scoped to the region's nodes (relations among contained
  nodes show/hide per the spec). (Substrate: the graphlet projection.)
- **Layout** — `arrange(strategy)`, applying an arrangement to the contained
  nodes within the region's box. (Substrate: `orrery/arrangements`.)

The region is a first-class visible object: a translucent outline (disk radius /
box) painted in the orrery, **selectable and movable like a node** (drag to
reposition the extent; the rules re-evaluate over the new contents).

**Trust** mirrors the omnibar command shell's privileged tier: the region script
is user-authored (you place and script your own region), so it gets the bound
`couple` / `show_edges` / `arrange` surface — the same two-tier model as the rhai
lanes (sandboxed note-blocks vs the privileged omnibar/region authoring), privilege
= the binding set.

## Phases

- **P0 — Place + render + move** (the foundation). `Orrery::add_field_at(content_band_xy)`
  mirrors `add_node_at`: mint a default disk `Field` with a `Region` extent at the
  cursor world point; render its outline in the orrery scene; make it selectable +
  draggable (move re-anchors the extent). A `ContextAction::AddField` row on the
  no-selection context menu (and an `Add field` row on the add-pill menu). Done
  when you can place a visible field at the cursor, see it, and drag it.
- **P1 — Forces**. The placed region gets a default `Coupling` (so it immediately
  bends the layout — the no-placebo gesture), then a `couple(...)` rule. Done when
  nodes in the region drift per its force rule and a force tick reflects it.
- **P2 — Edge visibility**. A `show_edges`/`hide_edges` rule scopes an
  `EdgeProjectionSpec` to the region's nodes; the orrery edge pass respects it.
  Done when a region can reveal/hide relation families for the nodes it contains.
- **P3 — Layout**. An `arrange(strategy)` rule applies an arrangement to the
  contained nodes within the region. Done when a region re-lays-out its contents.
- **P4 — The rhai rule surface** (the unifier). A region carries a rhai script;
  evaluating it (over the region's contained nodes/edges, via a `FieldContext`
  snapshot like the omnibar shell's `ShellContext`) emits the force/edge/layout
  effects. Built on aether's `FieldProjection` + the existing rhai bindings,
  extended with the placed-extent scope. Done when one region script sets all
  three characteristics over its contents.

## Open decisions

- **Default field shape**: disk (radius) vs box (the `Region` extent). The extent
  is a box; the *definition* can be a disk. Likely a disk-in-a-box default, both
  draggable/resizable.
- **Rule editing surface**: where the region's rhai script is written — an
  inspector pane (the field editor the configurability preference wants), the
  omnibar (`>` over a selected region), or a knot-style block. Likely the inspector
  pane, reusing the rhai infrastructure.
- **Selector semantics**: "nodes in the region" = strictly inside the extent box,
  or weighted by the scalar field's falloff? Falloff is the richer (and already
  modeled) answer.
- **Recompute trigger**: when does a region re-evaluate — on move, on graph
  mutation, every tick? gyre warns couplings snapshot targets at build time
  (rebuild on mutation); the region needs a defined recompute cadence.
- **Persistence**: fields already persist (`PersistedField*` in the kernel); the
  region's rhai *script* needs a persisted home alongside the field.

## Progress

- 2026-06-13: Plan written from the field-system scout (kernel `Field`/`Coupling`,
  aether `FieldProjection` + rhai, gyre `CouplingForce`, `EdgeProjectionSpec`,
  arrangements). User confirmed the vision: a placed spatial region whose rhai
  rules govern forces + edge visibility + node layout — "do it right" (place +
  render + couple, then the full rule surface). No code yet; P0 (place + render +
  move) is the foundation, mirroring the shipped `add_node_at`.
