# Scene Contract Note — the first sceno slice

**Date**: 2026-07-22 (updated 2026-07-24)
**Status**: Landed and consumed. Written as rationale for the P2 type sketch,
before mere wired onto it; mere, isometry, and graphshell now all consume the
contract, so the review question is no longer "is this the right shape to
build" but "which parts did the consumers actually force". Current: sceno
0.0.2, 29 family tests (sceno 13, scenomise 9, scenotime 7), verified
2026-07-24.

The family moved on 2026-07-23: the standalone `scenograph` repo was absorbed
into mere at `crates/scenograph`, so the proof sequence and findings now live
in this repo beside it,
[projection_proofs_plan](../../../design_docs/mere_docs/implementation_strategy/2026-07-21_projection_proofs_plan.md);
the direction record is the
[prior-art brief](../../../design_docs/mere_docs/research/2026-07-21_projection_engine_prior_art_brief.md).

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
  representation measures content; the projection places it." See the open
  question below: this is the one module no consumer has used.
- `score` (landed 2026-07-22, after this note's first draft): `Score`,
  `Arrangement` (`Spiral` | `Board` | `Geographic`), `ScoreItem`, `Placement`,
  `SCORE_VERSION`. The deferral below expected these at the proof-4 era; the
  P3/P4/P5 boundary proof pulled them forward, because a product-free
  *persisted* vocabulary is what makes two adapters comparable.

Beside it in the family: `scenomise::solve` realizes a score footprint-aware
(the spiral grows its spacing to clear the largest measured item),
`scenomise::relax` gives any scene dependency-free repulsion/spring/anchor
relaxation, and `scenotime` wraps a dense scene in epoch-scoped stable slots
with transactional diffs (see the
[scenotime note](2026-07-22_scenotime_epoch_diff_note.md)).

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

- ~~**Score types**~~ **landed 2026-07-22** in `sceno::score`, earlier than
  this note projected. Proof 4 arrived the same day and needed a persisted
  score to compare two adapters against, so the schema question was answered
  by exercise rather than by deliberation: version, arrangement, ordered
  measured items, generation.
- **Action intents**: still absent from `sceno`, but no longer unexercised.
  Graphshell shipped its own reverse path at the protocol layer instead:
  `IntentReference`, `AdvertisedAction`, `IntentInvocation`, `IntentResult`,
  with the payload deliberately opaque at G1. So the question has changed
  shape. It is no longer "when do intent types land in the contract" but
  "does the contract need them at all, or is a hit shape plus a
  product-owned action id the whole engine-side story, with authorization and
  dispatch staying in the protocol and the participant gate?" Answer this
  before adding a second intent vocabulary that has to agree with the first.
- ~~**Scene diffs**~~ **landed 2026-07-22** in `scenotime`: epoch-scoped stable
  slots, tombstones that survive serialization, transactional revision
  transitions, idempotent replay.
- **Per-item emphasis channels** (heat, bridge rings): may be a
  `channels: Vec<(String, f32)>` open map per item, or host-side signal
  reads; decide when a consumer forces it rather than guessing.
- **Volumes / 3D footprints**: per the brief's task-and-display gating; the
  types are shaped so a z-bearing variant extends rather than migrates.

## Open questions, and what two consumers ruled

Verified against the code 2026-07-24, after mere, isometry, and graphshell had
all wired on.

- **`Region`: does it want a members-free, `SpaceId`-only form?** Unforced, so
  hold the single shape. Only `cartography::scene_out` emits regions at all
  (`ClusterHalo` → `Region`); isometry emits none. The pure-frame-group case
  that would have wanted the alternative is already served by `Space`, which
  is what makes the members list non-redundant. Revisit when hull authoring
  produces a group before it has members.
- **Should `RoutedRelation` endpoints attach to regions?** Hold
  instance-to-instance. Both emitters (`cartography::scene_out`,
  `isometry-graphshell`) emit instance pairs, and `scenomise::relax` now reads
  relations as springs between *placed items*, so a region endpoint would owe
  a relaxation rule as well as a stroke. Two lanes to define, still zero
  consumers asking.
- **`Measurements` lookup shape** — the question is moot, and its answer is
  worth more than the question. Nothing consumes `sceno::measure`: not mere,
  not isometry, not merecat, not graphshell. The extent lane shipped in two
  other places instead. Hosts measure and stamp `ScoreItem.footprint`, and
  mere carries per-node extents through `ViewIntent.extents` →
  `CartographySceneOptions.extents` → the adapter. "The representation
  measures, the projection places" held up as a *principle*; the module that
  encoded it did not, because a score that already carries a measured
  footprint per item leaves `Measurements` with nothing to say. Decide before
  0.0.3 whether `measure` earns its place, folds into the score, or ships as
  documented-but-unused surface. Publishing dead contract surface is the one
  outcome to avoid.

## Where it went

P2's next slice, and everything the original version of this section
anticipated, is done. Mere consumes the contract through
`cartography::scene_out` + `spiral_score`, and the proof-1 scenario re-ran
with every card clear. Isometry (proof 4) deleted `Overmap::layout` and its
force solver, adapting authored pins to a geographic score and unpinned sites
to the portable spiral. P5's coastal fixture exercises the geographic path
from serialized facts. On 2026-07-23 `scenomise::relax` extended the
capability to swatch-scale surfaces without a physics dependency, and
graphshell G2 took `scenotime`'s snapshot/diff pair as its remote replay
contract.

Remaining before a freeze can be claimed: the intent question and the
`measure` question above, per-item emphasis channels, and whether hit
resolution belongs to `scenotime` or the host.

The likely third consumer is **woodshed**, which gates its own scene-contract
work on this family being proved and frozen (see that repo's
`2026-07-11_stage_set_tools_plan.md`). The proving half is finished, so the
freeze list above is what it is actually waiting on. It brings two inputs the
first two consumers could not: a non-graph frame (a fretboard is a `Space`
mapping (string, fret) to screen, with notes as point footprints and
fingerings as paths), and a source whose relations are dense,
multi-family, and deterministic on day one, with no authoring step.
