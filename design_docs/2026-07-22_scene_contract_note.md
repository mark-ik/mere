# Scene Contract Note — the first sceno slice

**Date**: 2026-07-22
**Status**: Landed (sceno 0.0.1 working tree, 12 tests). Design rationale for
the P2 type sketch, written for review before mere wires onto it. The proof
sequence and findings live in mere's
`design_docs/mere_docs/implementation_strategy/2026-07-21_projection_proofs_plan.md`;
the direction record is the same repo's
`2026-07-21_projection_engine_prior_art_brief.md`.

## What landed

- `geometry`: `Vec2` / `Size2` / `Rect` / `Transform2` (similarity: uniform
  scale → rotate → translate). Dependency-free wire shapes; hosts convert at
  the boundary.
- `footprint`: `Point | Circle | Rect | Polygon | Path` in item-local space,
  with `bounds()`. The proof-1 fix: solvers can now see what they must clear.
- `scene`: `SourceRef` (adapter + opaque id) interned into `sources`;
  `Space` chain with `to_world()` (cycle-safe); `ProjectedItem` (source ix,
  space, transform, footprint, representation slot, layer, visibility, hit
  override); `RoutedRelation` (full polyline + open kind + weight);
  `Region` (members + optional contour + confidence).
- `measure`: `Measurement` / `Measurements` — the input half of "the
  representation measures content; the projection places it."

## Decisions, with reasons

1. **String `SourceRef`, interned, not a generic `Scene<Id>`.** Heterogeneous
   scenes (a mere node beside a hocket phrase) need refs that carry their
   source lane, and serialized scenes need concrete types. The allocation
   cost is bounded by interning (`sources` is deduped; items carry a `u32`),
   and scenes rebuild on change, not per frame (scenotime's job). This also
   makes the source-vs-instance separation *structural*: one interned source,
   many items pointing at it.
2. **Identity is an index** (the stack's data-oriented doctrine): dense
   vectors, `u32` id newtypes indexing them, world space at index 0 by
   construction.
3. **Uniform-scale similarity transforms.** Non-uniform scale distorts
   footprint shapes and is a representation concern; placement gets
   translate/rotate/uniform-scale. Revisit only if a real solver needs
   shear/stretch.
4. **Footprints are item-local, centered on the anchor.** The transform
   carries them into a space; `Point` is the degenerate pre-P2 case so
   position-only strategies stay expressible.
5. **Spaces are the frame primitive.** Hull groups, geographic ground
   planes, fretboards, nested maps: all `Space` entries; dragging a group is
   one transform edit. This is the frames ruling from the brief landing as
   data.
6. **Representation is a slot with a recognized LOD core**
   (Glyph/Card/Sprite/Snapshot/LivePane) **plus an open tail** — the same
   recognized-core-plus-open-tail shape as the coupling vocabulary. Scenes
   never carry rendered content.
7. **Overlay migration**: cartography's `EdgeWeight` → `RoutedRelation.weight`;
   `ClusterHalo` → `Region`; `ImportanceScale` folds into assigned
   footprints/transforms; `ActivityHeat`/`BridgeEmphasis` await the channel
   question below.

## Deliberately deferred

- **Score types** (serialized projection settings): the schema question is
  open in the brief; it lands when the serialized-settings done condition is
  exercised (proof 4 era), not before.
- **Action intents**: the reverse path is scenotime's proof; the contract
  slot is reserved conceptually (hit shapes exist) but no intent types yet.
- **Scene diffs**: scenotime's whole reason to exist; the snapshot lands
  first so there is something to diff.
- **Per-item emphasis channels** (heat, bridge rings): may be a
  `channels: Vec<(String, f32)>` open map per item, or host-side signal
  reads; decide when a consumer forces it rather than guessing.
- **Volumes / 3D footprints**: per the brief's task-and-display gating; the
  types are shaped so a z-bearing variant extends rather than migrates.

## Open questions for review

- Does `Region` want a `SpaceId`-only form (a pure frame group with no
  members list) or is members-with-space the right single shape?
- Should `RoutedRelation` endpoints allow attaching to *regions* (hull-to-
  hull relations) or stay instance-to-instance until a consumer asks?
- `Measurements` is a flat vec keyed by `SourceIx`; fine at canvas scale,
  revisit the lookup shape if a consumer measures thousands.

## Next slice (P2 continues)

Mere consumes: a `sceno`-emitting adapter beside `cartography::Projection`
(the strategy dispatch gains a scene-shaped output path), phyllotaxis reads
`Measurements` for extent-aware spacing, and the proof-1 scenario re-runs to
show the spiral clearing its cards. Isometry follows as the second consumer
(proof 4).
