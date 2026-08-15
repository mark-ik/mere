# Scenograph Freeze Plan

**Status (2026-07-24): planned; implementation has not started.**

The [scene contract note](../../../crates/scenograph/design_docs/2026-07-22_scene_contract_note.md)
ends with four items standing between the family and a frozen 0.0.3, and the
[projection proofs plan](2026-07-21_projection_proofs_plan.md) has finished
the proving half. Woodshed gates its own scene-contract work on this freeze
(`woodshed/design_docs/2026-07-11_stage_set_tools_plan.md`: "what remains is
the freeze, not the proof"), so this is the cheapest unblock available.

This plan answers the four with code evidence rather than deliberation, per
the note's own standard: a question is answered by the consumer that forces
it, and unforced surface does not ship.

```text
F1 intents (delete question)  F2 measure (delete module)
F3 channels (add, forced)     F4 pick (add, forced)   -> 0.0.3 freeze
```

## Findings (verified against the code 2026-07-24)

**Baseline.** 29 family tests green: sceno 13, scenomise 9, scenotime 7.
Family is 2,113 LOC across four crates at 0.0.2. Live consumers are mere (by
path), isometry and turnstone (git, `merely-made/mere` branch `main`), and mere's
own `ports/graphshell`. The archived `repos/graphshell` still pins
`scenograph.git`, a repository the 2026-07-23 consolidation absorbed; that
copy is dead (its last commit is "Archived: moved in the 2026-07-23 repo
consolidation") and is not a freeze blocker, but no live work should be done
against it.

**F1 evidence.** `chirograph` carries a complete intent vocabulary:
`IntentReference`, `IntentEffect` (`Curation` / `DomainTruth` /
`ExternalEffect`), `AdvertisedAction` (intent, label, explanation,
payload_schema, effect), `IntentInvocation` (session, `target: InstanceId`,
`observed_epoch`, `observed_revision`, intent, opaque payload), and
`IntentResult` (`Accepted` / `Rejected` / `Stale`). It already binds to the
family's identities without sceno owning an intent type: instance identity
from `sceno`, epoch and revision from `scenotime`. The `Stale` variant is
only expressible because the protocol sees epoch and revision together,
which a sceno-side intent type would have to duplicate.

**F2 evidence.** Nothing in the workspace imports `sceno::measure`
(ripgrep over all Rust sources, excluding targets). `ScoreItem.footprint`
carries the measured extent, and `cartography::scene_out` takes an
`extent_of` closure and stamps footprints directly. Two further findings the
note did not have:

- `Measurement.min` (smallest legible size) is the one datum `Footprint` does
  not carry. It would matter only for representation degradation under
  pressure, which no solver does; graphshell instead answers content sizing
  host-side with `BoundsRelationship` (`FillFootprint` /
  `FitWithinFootprint` / `IntrinsicWithinFootprint`).
- `Measurements.by_source: Vec<(SourceIx, Measurement)>` is keyed by
  **source**, which contradicts the contract's founding separation. One
  source may be placed as many instances at different representation rungs,
  and a source-keyed map cannot express two rungs for one source. The module
  is not merely unused; it is shaped against the commitment it was written to
  serve.

**F3 evidence.** `cartography::overlay::Overlay` still holds `ActivityHeat`
and `BridgeEmphasis` unmigrated, while `scene_out` migrates `ClusterHalo` to
`Region`, `ImportanceScale` into `transform.scale`, and `EdgeWeight` into
`RoutedRelation.weight`, dropping heat and bridge deliberately. The decisive
argument is graphshell: it ships scenes over a wire to a client that has no
source access, so any emphasis kept as a host-side signal read is invisible
to a remote viewer. Per-item emphasis must be in the scene or it cannot
cross the wire.

Woodshed's pressure is real but lands elsewhere than the note expected. Its
requirement is that one chord pair carry diatonic, shared-tone,
voice-leading, and practiced-after at once without deduplicating "into one
winning reason", which is a **relation** question, not a per-item one:
`RoutedRelation.kind` is a single `Option<String>`. That case needs no
contract change, because mere already ruled multi-edge is truth and the
collapse is an experience setting (the cells-as-edges ruling in the
[swatch primitive design](../design/2026-06-27_swatch_primitive_design.md),
live in `canvas::edge_cells` with a fanned-parallel-cells test). Woodshed
emits one `RoutedRelation` per reason.

**F4 evidence.** `ProjectedItem.hit` is `None` at every construction site in
the workspace: sceno's own tests, `scenomise::solve` and `::relax`,
`scenotime::diff`, `cartography::scene_out`, isometry, and graphshell's
client, canary, and resume. Nothing populates it and nothing reads it. Real
hit resolution is host-side and does not consult the scene at all: mere's
canvas runs `view.hit_test(screen_to_world(cursor))` against seiche colliders
plus `edge_cell_hit_test` for edge cells. `scenotime` exports only
`diff`, `ids`, and `snapshot`, with no spatial query.

The forcing consumer is again the remote graphshell client: it must resolve a
click to an `InstanceId` to fill `IntentInvocation.target`, and it has no
seiche world to ask. Today every remote client would reimplement picking.

## Decisions

### D1. Sceno ships no intent vocabulary

The reverse path stays where graphshell built it. The seam is stated once and
does not move: **sceno owns instance identity, scenotime owns epoch and
revision identity, the protocol owns the intent triple (advertise, invoke,
result), and the participant gate owns authority.** A hit shape plus a
product-owned action id is the whole engine-side story.

This closes the note's question in the direction its own evidence pointed,
and forecloses a second intent vocabulary that would have to agree with the
first.

### D2. `sceno::measure` is deleted, not documented

Delete the module rather than shipping unused surface. The principle it
encoded ("the representation measures content; the projection places it")
survives in `ScoreItem.footprint`, which is where hosts already stamp
measurements.

If representation degradation later needs a legibility floor, it arrives as
an optional field on `ScoreItem` beside the footprint that is already there,
per item and therefore per instance. It does not return as a source-keyed
map.

### D3. Per-item emphasis channels land as an open map

`ProjectedItem` gains `channels: Vec<(String, f32)>`: an open map with
documented recognized names rather than an enumerated set, matching the
recognized-core-plus-open-tail shape `Representation` already uses. Empty is
the common case and costs one empty vec.

Relations do not gain channels. Multi-reason relations fan into one
`RoutedRelation` per reason, consistent with the cells-as-edges ruling.

### D4. Picking belongs to scenotime, and the host may ignore it

`scenotime` gains a default pick over its slot tables: topmost-first by
layer, honoring `hit` when present and falling back to `footprint`, skipping
invisible items, resolving through the space chain to world. Hosts with a
better spatial index (mere, via seiche) continue to use it and are
unaffected.

This makes `hit` meaningful for the first time: it is the shape picking uses
when an item's clickable area differs from the extent solvers clear.

## Build order

### S1. Delete `measure` and state the intent seam

**Files:** `sceno/src/lib.rs`, `sceno/src/measure.rs` (removed)

Remove the module, its two re-exports, and its test. Reword the lib-level
contract commitment so the measure principle points at `ScoreItem.footprint`.
Add a short doc block recording D1: intents are deliberately absent, and
where they live instead.

**Done when:** `sceno` has no unconsumed public module, and a reader learns
from the crate docs why there is no intent type here.

### S2. Emphasis channels through to a remote client

**Files:** `sceno/src/scene.rs`, `cartography/src/scene_out.rs`,
`cartography/src/overlay.rs` doc comment

Add `channels` to `ProjectedItem`. Map `ActivityHeat` to a `"heat"` channel
and `BridgeEmphasis` to a `"bridge"` channel in `scene_out`, and delete the
"await the channels decision" caveat from its module doc. Document the two
recognized names in `sceno` without closing the set.

**Tests:** heat and bridge overlays reach the scene as channels; an unknown
channel name survives a serde round trip; an item with no overlays carries an
empty channel list.

**Done when:** a graphshell snapshot carries per-item heat to a client with
no source access, which is the case host-side signal reads cannot serve.

### S3. Default picking in scenotime

**Files:** new `scenotime/src/pick.rs`, `scenotime/src/lib.rs`

Layer-respecting topmost-first pick over `SceneTables`, space-chain aware,
honoring `hit` then `footprint`, skipping invisible items and tombstoned
slots.

**Tests:** overlapping items pick the higher layer; an item with a `hit`
override smaller than its footprint rejects a point inside the footprint but
outside the override; an invisible item never picks; a point in a nested
space picks through the transform chain; a tombstoned slot never picks; a
miss returns `None` rather than a nearest match.

**Done when:** a client with only a snapshot can resolve a point to the same
`InstanceId` a source-owning host would.

### S4. Freeze and cut 0.0.3

Confirm the family's crates.io publish state before cutting (at `0.0.x`
every version is semver-incompatible in Cargo, so a removed module needs no
special handling beyond consumers re-resolving). Bump the four crates, update
the scene contract note's open-questions section to record the four rulings,
and update `sceno`'s package description, which currently promises that
"scores and action intents arrive with later proofs" and is now wrong on the
second half.

Re-resolve isometry and turnstone against the new commit, since both track
`merely-made/mere` branch `main`.

**Done when:** the note has no open questions, `woodshed` can adopt against a
stated contract, and the freeze list in its stage-set plan is answered rather
than deferred.

## Non-goals

- An intent vocabulary in `sceno`, now or later, unless the protocol's own
  design fails on a case it cannot express
- Channels on relations (multi-reason relations fan instead)
- Replacing mere's seiche-backed picking with the scenotime default
- Volumes, 3D footprints, or non-uniform transforms, which the note already
  gates on task-and-display evidence
- Reviving the archived `repos/graphshell` copy or its stale `scenograph.git`
  pin

## Verification wall

```text
cargo test -p sceno -p scenomise -p scenotime
cargo test -p cartography
cargo test -p chirograph -p graphshell-client
cargo fmt --all -- --check
git diff --check
```

Baseline before any change: 29 family tests green (13 / 9 / 7).

## Progress

### 2026-07-24 — S1 through S3 landed; S4 local half done, publish pending

Written and executed the same day, after the
[application prospects brief](../../2026-07-24_application_prospects_brief.md)
ranked this the highest unlock-per-effort item on the board.

**S1.** `sceno::measure` deleted with its re-exports and test. The lib-level
commitment now points at `ScoreItem.footprint`, and a "what is deliberately
absent" section records D1 in the crate docs where a reader meets it.

**S2.** `ProjectedItem.channels` landed as an open `Vec<(String, f32)>`.
Cartography maps `ActivityHeat` to `"heat"` and `BridgeEmphasis` to
`"bridge"` behind exported `HEAT_CHANNEL` / `BRIDGE_CHANNEL` constants, which
completes the overlay migration. Two tests: both emphases reach the item, and
an unemphasized node carries none.

**S3.** Picking landed, but the factoring moved during the work. Sceno gained
the two pieces it should have owned all along, `Transform2::inverse` and
`Footprint::contains` (circle by radius, rect by half-extent, polygon by
even-odd ray cast, path by distance to segment), so `scenotime::pick` holds
only traversal: layer then order then slot, `hit` over `footprint`, skipping
invisible items and tombstones, carrying the point back through the space
chain. Ten pick tests plus seven for the new sceno primitives.

`Footprint::Point` contains nothing, so a zero-extent item is unpickable
without a `hit` shape. That is the case `hit` exists for, and it is now
tested and documented rather than implied.

**S4 (local half).** All four crates bumped to 0.0.3. `sceno`'s published
description promised "scores and action intents arrive with later proofs",
which D1 makes permanently wrong, so it and `scenotime`'s were rewritten, as
was the facade's module doc. The scene contract note's open questions are
replaced by a Rulings section.

**Verified.** 45 family tests green, up from 29 (sceno 19, scenomise 9,
scenotime 17), plus mere-cartography, graphshell, graphshell-client and
chirograph. `cargo fmt --check` clean across the touched crates,
which also required normalizing pre-existing failures in `scenomise/relax.rs`
(4 hunks) and `cartography/scene_out.rs` (3): both already failed at HEAD
before this work, verified by stashing.

**Two findings worth keeping.**

1. `sceno` 0.0.2 was published to crates.io earlier the same day, so deleting
   `measure` is a real published break and the 0.0.3 cut is required rather
   than cosmetic. At `0.0.x` Cargo treats every version as incompatible, so
   consumers need only re-resolve.
2. Neither isometry nor turnstone constructs a `ProjectedItem` anywhere, so the
   new field costs them nothing beyond that re-resolve. Every construction
   site is inside mere.

**Remaining:** ~~publish the four crates~~ **published 2026-07-24 23:45 UTC**
(all four 0.0.3s went up in dependency order four minutes after the freeze
commit; verified 2026-07-25 against the registry — the published sceno has no
`measure.rs` and the published scenotime carries `pick.rs`, so the artifacts
are this cut. A second publish attempt on 2026-07-25 correctly errored
"already exists"; nothing was missing). ~~Still remaining: re-resolve isometry
and turnstone against 0.0.3, and tell woodshed the contract is frozen.~~
**Closed 2026-08-10**: both locks verified at 0.0.3 for sceno, scenomise,
and scenotime (each tracks `merely-made/mere` main and had re-resolved along
the way), and the freeze notification is delivered as a banner on woodshed's
stage-set plan plus the
[scenograph expansion brief](../research/2026-08-10_scenograph_expansion_brief.md),
whose L1 lane owns woodshed adoption.

**History note.** A concurrent session's `git add -A` swept most of S1-S3
into two unrelated commits, `4b8d875f` ("document-host: derive the wasm
import grant from typed capabilities") and `6d2014b3` ("Consolidation plan:
document-host closed..."). The content is intact and tested; the history is
misleading, and was left alone rather than rewritten because it is shared.
