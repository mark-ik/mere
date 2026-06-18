# Scriptable Field Regions Plan

A **field region** is a spatial area you place on the graph that carries a rule
set — scriptable in rhai — governing the graph's characteristics inside it:
forces (how nodes are pulled), edge visibility (which relations show), and node
layout (how the contained nodes arrange). The third graph element beside nodes
and edges, but a *rule-bearing* one: place a region, write its rules, and the
graph within it behaves accordingly.

**Spine**: per the [interaction model spine](../technical_architecture/2026-06-18_interaction_model_spine.md),
this plan owns the **localized / scripted arrangement** half of the *arrange* stage (the
scene-wide arrangement choice is the node-representation plan's), plus the placed rule region
(forces, edge-visibility) and its rhai surface. Field regions are already moveable + resizable
(Progress, `7445e70`).

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

- **Default field shape**: *resolved (2026-06-14)* — disk-in-a-box. The soft disk
  well is the persistent visual; the box is the extent, shown **only on
  interaction** (hover / select / drag), both draggable/resizable.
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

## Field as a first-class object (2026-06-14 user direction)

On first contact with a placed field, the user reframed it from "a thing on the
canvas" to a **manipulable, listable object**, and surfaced the core gap: *"I have
no idea what to do with the field."* The durable requirements:

- **Manipulation** — a field is moved, **resized**, and otherwise handled like any
  object (select, drag, resize handles). Move/resize re-anchor the `Region` extent
  (and the disk center/radius), and the rules re-evaluate over the new contents.
- **Box-on-interaction, not persistent chrome** — the dashed extent box should
  appear only while you are *interacting* with the field (hover / select / drag);
  the soft disk well is the persistent at-rest visual. An always-on box reads as
  clutter. (Supersedes P0b's always-drawn box.)
- **Hideable but findable in the roster** — a field can be **hidden** from the
  canvas yet remain listed in the **roster** (a third member kind beside nodes and
  edge-rows), so it stays findable and re-showable. The roster is the field's
  index + visibility control.
- **Purpose must be tangible** — the "no idea what to do with it" gap is the real
  one: an inert translucent disk has no evident point. The fix is **P1 first** — a
  placed field must *immediately do something visible* (the no-placebo gesture):
  its default coupling gathers / repels the nodes in its extent, so placing and
  dragging a field visibly moves the graph. Forces make the field self-explanatory
  before any rhai rule surface exists. **Gyre wiring note:** `CouplingForce` (gyre,
  `impl Force`) exists but is **not yet added into the orrery's live physics tick**
  (`physics.advance_frame`) — wiring couplings (rebuilt on graph mutation) into the
  sim is the load-bearing P1 work, not a flip.

## Physics tuning — deferred to a settings menu (post-window-composition)

After P1 shipped, testing the force well surfaced three physics issues. The user's
call: these belong in a **physics settings menu**, **deferred until after the
window-composition plan** (which unblocks pelt in meerkat) — not hardcoded guesses
now. Captured here so the menu's scope is ready:

- **Field strength is configurable, and the default is probably too weak.** Nodes
  already inside a field's radius do gather, but the pull wants more force, and the
  right strength varies — so it is a **per-field setting**, not one global constant
  (`configurability over opinionated defaults`). The disk's small gradient
  (~slope/radius) means the strength number is large; the menu exposes it (plus
  field radius / falloff) per field.
- **A coupling does not pick up nodes added after the field** *(correctness, not
  just tuning)*. `CouplingForce::from_coupling` snapshots its target set
  (`NodeSelector::All`) at build time, and the orrery only *adds* the force on
  placement — so a node minted *later* is in no field's target set and feels no
  pull (it drifts on the default forces, which looked like "the new node went to
  the old field"). The fix is the **rebuild-on-mutation** the P1 note already flags:
  on a node/field change, re-resolve every coupling's targets (a gyre force-replace
  API, or a position-preserving sim rebuild — the live sim is add-only today). This
  is the load-bearing gap; arguably worth fixing before the menu since it makes
  fields feel broken.
- **Inertia/damping is not preserved across pause/resume.** Settling damps momentum
  to rest (the right *default*), but when scrubbing with pause/play the user wants
  the momentum preserved so the motion continues from where it froze — a **damping
  toggle** (or a "resume with inertia" mode) in the menu. Today resume kicks a fresh
  settle budget; the body velocities survive a halt, but the settle damping bleeds
  them off.

Also deferred into the same surface: per-field **response** (gather / repel / wall /
dampen), the **move/resize re-aims the well** behavior (today the force snapshots
the field definition at placement, so dragging the field doesn't move its pull
until rebuild), and field **removal** dropping its force.

## Progress

- 2026-06-13: Plan written from the field-system scout (kernel `Field`/`Coupling`,
  aether `FieldProjection` + rhai, gyre `CouplingForce`, `EdgeProjectionSpec`,
  arrangements). User confirmed the vision: a placed spatial region whose rhai
  rules govern forces + edge visibility + node layout — "do it right" (place +
  render + couple, then the full rule surface). No code yet; P0 (place + render +
  move) is the foundation, mirroring the shipped `add_node_at`.
- 2026-06-14: P0a + P0b shipped. `Orrery::add_field_at` places a disk-in-box
  `Region` field at the cursor (`3c62b15`); the orrery renders it as a soft
  radial-gradient well inside a faint dashed square, spliced *under* the edges via
  `CanvasPaintList::splice_world_underlay`, placeable from the empty-space
  right-click and the add-pill (`80c05fb`). User feedback on first contact (above):
  fields want manipulation (move/**resize**), box-on-interaction only, roster
  listing + hide/show, and — the priority — a **tangible purpose**, which moves P1
  (forces, the no-placebo coupling) ahead of further P0 polish. Open question put
  back to the user: which effect to make tangible first (forces / edge-visibility /
  layout).
- 2026-06-14: **P1 shipped — the force well** (`48067be`). User chose "force well"
  as the field's first job. `add_field_at` attaches a default `RepelFromMax`
  coupling over the disk peak (nodes pulled up the gradient toward the center) and
  pushes the resolved gyre `CouplingForce` into the live sim via a new
  `Physics::add_coupling_force` / `PhysicsCommand::AddCouplingForce` (no
  position-losing rebuild); `build_simulation` also folds field couplings in for a
  session reload. Default strength `5000` (the disk gradient is small). Also shipped
  **physics pause/resume** (`b2ecaaf`): Space + a toolbar pause/play button, with a
  gated `settle_physics` so a paused graph stays frozen through mutations. Testing
  surfaced the three physics issues above (weak/strength-per-field, new-node not
  captured, inertia-on-pause) → **deferred to a physics settings menu, gated on the
  window-composition plan** (the section above). No further field code until the
  user resumes the follow-ups (move/resize, box-on-interaction, roster + hide).
- 2026-06-14: **Follow-ups shipped** (user said "knock out each", ultracode on).
  Scouted the seams (4 parallel agents), implemented in the main loop, adversarial
  review (8 agents). **Box-on-interaction** (`d43f3c1`): the dashed extent box draws
  only on hover (new `fields.rs` module; `active_field`). **Roster listing + hide +
  locate** (`bfd753a`): a Fields section with per-field hide/show toggle and
  click-to-center (`hidden_fields` mirroring `hidden_edges`, `center_on_field`).
  **Play runs continuously** (`fada862`): resume settles ~forever so a well can be
  watched. **Move + resize + rebuild-on-mutation** (`7445e70`): grab a field's box
  edge (move) / corner (resize); the disk + extent follow and the **well re-aims
  live**. This required the rebuild-on-mutation fix — gyre was add-only, so it now
  keeps a replaceable `coupling_forces` list (`set_coupling_forces`) and the orrery
  re-resolves all couplings on place / move / resize **and in `reconcile_derived`,
  which also fixes the new-node-capture bug** (a node added after a field is now
  pulled). Review fixes folded in (hover-clear on viewport exit, roster sort, z-order
  comment). **Still deferred to the physics menu (post-window-composition):**
  per-field strength, response (gather/repel/wall/dampen), and the inertia/damping
  toggle — the *tuning* surface, distinct from the now-shipped *mechanics*.
