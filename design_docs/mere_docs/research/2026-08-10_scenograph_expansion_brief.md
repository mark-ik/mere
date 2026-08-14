# Scenograph Expansion Brief

**Date**: 2026-08-10
**Status**: research brief, commissioned by Mark: scenes, arrangements, and
backdrops need thorough, organized expansion and broad utility, for woodshed
before it releases and broadly ("we're not using even 10% of our capabilities
in that regard"). The brief organizes the space so generalization arrives
prepared rather than improvised. Every lane keeps the family's proven
standard: a question is answered by the consumer that forces it, and unforced
surface does not ship.

**Related**:
[scenograph_content_catalog](2026-08-13_scenograph_content_catalog.md)
(the content pass this brief prepared for, 2026-08-13),
[scenograph_freeze_plan](../implementation_strategy/2026-07-24_scenograph_freeze_plan.md)
(0.0.3 frozen and published 2026-07-24), the scene contract note
(`crates/scenograph/design_docs/2026-07-22_scene_contract_note.md`),
[projection_proofs_plan](../implementation_strategy/2026-07-21_projection_proofs_plan.md),
woodshed's stage/set/tools plan
(`woodshed/design_docs/2026-07-11_stage_set_tools_plan.md`),
[swatch_primitive_design](../design/2026-06-27_swatch_primitive_design.md)
(the cells-as-edges ruling that multi-edge is truth).

## 1. Where the family stands

- **0.0.3 is frozen and published**: sceno (instance identity, footprints,
  transforms, `contains`, open emphasis channels), scenomise (solve/relax),
  scenotime (epoch/revision identity, diff, snapshot, default pick),
  scenograph facade. 45 family tests.
- **Consumers**: mere by path; isometry and turnstone by git (branch `main`);
  `ports/graphshell` including the remote client, which is the standing proof
  that scenes cross a wire to a viewer with no source access.
- **Loose ends the freeze recorded and nobody picked up**: isometry and
  turnstone still need re-resolving against 0.0.3, and woodshed was never
  told the contract froze; its stage-set plan gates on exactly that ("what
  remains is the freeze, not the proof"). This brief is that notification.
- **The leverage reading.** The family proved contract discipline: unused
  surface was deleted, additions were forced through consumers. What it has
  not yet proven is breadth of use. One projection style is consumed (graph
  canvas), there is no environment layer, no motion, and snapshots serve as
  transport only. The 90% Mark is pointing at is real and enumerable.

## 2. Vocabulary: the stagecraft tiers

The family name promises a scenery workshop; today it ships placement. Three
tiers make the gaps addressable:

- **Scene** (frozen): placed instances, footprints, channels, routed
  relations, pick. sceno and scenotime own it.
- **Arrangement** (exists, mere-side only): the composition spine's state:
  which members, projected how. `canvas/arrangements` holds it; workbench
  (tree) and orrery (cartography) are its two projections. It is not yet a
  portable concept: it lives as mere library state, invisible to remote or
  embedded consumers.
- **Backdrop** (missing): the environment behind and beneath placed content.
  The word appears exactly once in the corpus (the orrery's background
  palette in the apparatus theme plan); every host paints its own background
  ad hoc, and nothing behind content can cross the wire, because the
  contract has no place for it.

**Product register (recorded 2026-08-14, Mark)**: above these contract
tiers, *scene* in product speech means a reusable total projection regime:
every lever (arrangement, backdrop, representations, motion, grammar) flexed
toward one intent, portable across datasets, obeying the orrery's laws. That
is the brand mark's "the projection decides what a node *is*" made a design
unit. The [content catalog](2026-08-13_scenograph_content_catalog.md) deck 2
holds that register; the contract's Scene type is what a regime emits when
applied to a dataset.

## 3. Lanes, each with its forcing consumer

### L1. Woodshed adoption (open now; the release gate)

The stage-set plan's model is ready-made for the frozen contract: the Set
projected as a graph (each staged Card occurrence a numbered node, Set order
a typed `Next` edge), one `RoutedRelation` per reason (diatonic, shared-tone,
voice-leading, practiced-after) per the cells-as-edges ruling, expansion
state on the projection, Card edits landing on the one Set. First acts:
re-resolve isometry and turnstone to 0.0.3, then build Stage's projection
through the `scene_out` shape. **Done when** Stage renders a Set graph
through the frozen contract with fanned relations, before woodshed's
release.

### L2. Backdrops (two consumers already waiting)

Isometry's map *is* a backdrop: a pixel-art VTT scene is mostly environment.
Woodshed's stage floor / fretboard context is the second. Graphshell remote
is the discipline: a backdrop must be scene data or a remote viewer sees
content floating on nothing. Research questions for the first slice: backdrop
as an item kind versus a separate table; layering (always behind, or
interleaved); whether `Region` generalizes or a new type is forced; hit
transparency (backdrops mostly never pick, except VTT maps that do, which is
what `hit` overrides are for). **Entrance gate**: prototype against
isometry's map and woodshed's stage; ship whichever shape both force,
nothing either fails to force.

### L3. Portable arrangements

The tear-out trichotomy and one-app-state-N-windows already treat a window
as a lens over an arrangement. Missing is arrangement identity that
*travels*: persist one (muniment), share it across windows (proven), then
across devices and peers (graphshell remote, commons). This is the lane
where "a graph-shaped session" becomes a durable, addressable object.
**Entrance gate**: the first consumer that asks for the same arrangement by
name from a second window, device, or peer.

### L4. Scenes as documents

scenotime snapshots already cross a wire; nothing stores one. A snapshot as
an eidetic artifact yields projection save/restore, VTT session records
(isometry), shareable set lists (woodshed), and replay for the games wing.
Cheap because the encoding exists; the work is identity and retention, which
eidetic owns. **Entrance gate**: the first consumer that wants yesterday's
projection back.

### L5. Motion (the deepest of the 90%)

`scenotime::diff` computes what changed between snapshots, and every
consumer snaps. Tweening diff output (position, scale, opacity over host
time) is the largest experience upgrade per line of code available in the
family, and it belongs at the scenotime layer precisely so every consumer
(canvas, remote client, woodshed's rehearsal filmstrip) inherits it rather
than reimplementing it. Care: the host owns the clock, the same seam
discipline `mere-mesh-host` uses for `Clock`. **Entrance gate**: the first
continuous re-projection a consumer ships (rehearsal filmstrip or a canvas
projection switch, whichever lands first).

## 4. Method

Mark's framing is the method: the unexpected generalizations will come from
the content, so this brief's job is preparation, not invention. Each lane
names its forcing consumer up front; content-driven surprises get placed
against §2's tiers (is this scene, arrangement, or backdrop?) before any new
module ships. The freeze plan's standard stands.

## 5. Sequence

L1 now; it is woodshed's release gate and costs mostly wiring. L2 opens the
moment isometry or woodshed touches environment work. L4 may leapfrog L3 if
a save-my-projection ask arrives first. L5 rides the first continuous
re-projection. None of this unfreezes 0.0.3: L2, L4, and L5 are additive
(0.0.4 material), and L3 stays mere-side until portability forces contract
surface.
