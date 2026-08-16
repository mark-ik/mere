# Mer3ly as a projection stack consumer

Status: survey, commissioned before re-gating the adoption plan
Date: 2026-08-16
Scope: what the public site consumes from Mere's projection stack, what it built locally in place of the contract, and which adoption-plan targets it actually forces
Sources: `repos/mer3ly` at `a7a2d2e` (2026-08-12, working tree dirty in three files) and `repos/mere` at `45f2794c`

## Ruling

Mer3ly is a genuine second consumer of the frozen 0.0.3 contract, over a second dataset, deployed publicly. It is also the reason several targets in the [projection grammar adoption plan](../implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md) are mis-gated, though not for the reason the 2026-08-15 reading gave.

The site consumes the stack along **two disjoint paths**. The portable path builds a `Score`, solves it, and records a diff trace. The live path drives an interactive simulation with pins, mobility, and backdrops. Nothing joins them: a pin placed by a visitor never reaches a `Score`, and the portable artifact never carries a coordinate.

Because the seam is missing, the site built its own wire. `mer3ly.graphshell-scene-state/v1` is a versioned scene state, base64url-encoded into a URL hash and copied to the clipboard, carrying the state the portable contract does not. That is the promotion evidence the catalog's rules ask for, already written, already public, and already crossing a device boundary.

## What it consumes

Six Mere crates pinned by git rev, plus two published Genet crates from crates.io.

| Dependency | Source | Surface used |
| --- | --- | --- |
| `sceno` | git, rev `8a7ede70` | `Score`, `ScoreItem`, `SourceRef`, `Arrangement::Spiral`, `Footprint::Circle`, `Representation::Glyph`, `Placement::Ordinal`, `RoutedRelation`, `InstanceId`, `Vec2` |
| `scenomise` | git, same rev | `solve` |
| `scenotime` | git, same rev | `SceneSnapshot`, `SceneEpoch`, `Revision`, `SceneDiff`, `SceneOp::{UpdateItem, TombstoneRelation}`, `RelationId`, `apply_diff`, `validate`, `active_item`, `active_item_count` |
| `seiche` | git, same rev | `Simulation`, `AnchorSpring`, `EdgeSpring`, `NodeCollider`, `NodeExclusion`, `Boundary`, `SceneSpec`, `SceneField`, `SceneBodySpec`, `NodeKey` |
| `arrangements` | git, same rev | `LayoutRegistry`, `Layout`, `Radial`, `Timeline`, `StaticLayoutState`, `CanvasViewport`, `CanvasSceneInput`, nine `graph_layout:*` ids including `graph_layout:penrose` |
| `cartography` | git, same rev | `default_graph_reading_registry`, `default_graph_representation_registry`, `ActorScope`, `PrimitiveBody` |
| `cambium` | crates.io `0.3.2` | site views serialized to static HTML |
| `genet-scripted-dom` | crates.io `0.1.0` | scripted DOM lane |

**Drift is near zero on the surfaces that matter.** The site pins mere at `8a7ede70` (2026-08-12) and mere is 112 commits ahead as of today, but exactly one of those commits touched `crates/scenograph/`, `crates/canvas/cartography/`, or `crates/canvas/arrangements/`, and it changed a single line of a design doc. The freeze is holding, and the site is live proof of it.

## The two paths

### Portable path

`portable_projection` (`crates/repo-graph/src/lib.rs:830`) hashes the authority JSON with SHA-256, derives `score.generation` from the first eight bytes of that digest, then builds one `ScoreItem` per node with a fixed shape: `Footprint::Circle { radius: 28.0 }`, `Representation::Glyph`, `Placement::Ordinal`, `layer: 0`, `visible: true`. `scenomise::solve` produces the scene, relations are routed as straight point pairs, and `SceneSnapshot::from_dense` seals it at `SceneEpoch(generation)`.

It then walks a default trace (`lib.rs:1011`): select Turnstone, move it, select the Turnstone-hosts-Mere relation, tombstone it, select Mere, fold its dependencies, expand them again. Each step is a `ProjectionStep { label, selection, diff }`, each diff chains `base` to `revision`, and `consume_portable_projection` re-validates the artifact and returns a `ProjectionReceipt` reporting score items, initial and final revision, active items and relations, picked source, and trace step count.

This path is deterministic, authority-hashed, and portable. It has no pins, no coordinates, and one representation.

### Live path

`GraphPhysics` (`lib.rs:172`) is a wasm-exported simulation wrapper: `set_arrangement(positions, mobility)`, `set_backdrop(backdrop, tangible)`, `pin_node(id, x, y)`, `unpin_node(id)`, `is_pinned(id)`, `tick(dt)`, `frame()`. Mobility is a three-way class, and the two non-free classes carry different force semantics (`lib.rs:600`):

- `anchored` installs an `AnchorSpring` at stiffness 13.0, a soft pull toward the target;
- `frozen` calls `simulation.pin`, a hard constraint;
- manually pinned nodes are pinned individually and keep their current position rather than the arrangement's proposal.

This is Penrose's `encourage` and `ensure` distinction, implemented, shipping, and with no satisfaction record anywhere. It touches `Score` nowhere.

**Changed 2026-08-16, after this survey.** The one-control-surface rework (mer3ly `145eb41`) removed the `frozen` class outright: the interactive simulation now carries `anchored` and `free` only, `is_pinned` is manual pins alone, and frozen output is ruled a non-interactive renderer's concern. The reading above holds for the surveyed commit and the ensure/encourage finding survives, since a manual pin is still the hard class. The shared wire also dropped `physics`, the one field this survey found had no owning target on either side.

### The seam that does not exist

`Score`, `Placement`, `scenomise::solve`, and `SceneSnapshot` appear only in the portable path. `Simulation`, pins, and mobility appear only in the live path. A visitor who pins three repositories and shares the result is sharing state the portable contract cannot express, so the site does not try.

## The site's own scene wire

`sceneState()` (`assets/graph-sandbox.js:824`) serializes ten fields.

| Field | Portable contract coverage |
| --- | --- |
| `schema: "mer3ly.graphshell-scene-state/v1"` | site-local, versioned |
| `dataset` | site-local |
| `source { source, commit, committed_at }` | revision cursor; `SceneEpoch` exists but is not the same thing |
| `reading` | `cartography` reading id, portable |
| `arrangement` | `graph_layout:*` id, portable |
| `motion` | **absent from the contract** (free / anchored / frozen) |
| `backdrop { kind, collidable }` | **absent from the contract** |
| `physics` | **absent from the contract** |
| `selection` | **absent from the contract** (single id, no resolution) |
| `pins: [{ id, x, y }]` | **absent from the score**; `Placement::Coordinate` exists, the site never reaches it |

That state is base64url-encoded into `#graphshell-scene=`, written to the address bar, and copied to the clipboard (`graph-sandbox.js:844`, `:1059`). A second person opening that link restores dataset, reading, arrangement, motion, backdrop, physics, selection, and pins.

Two things follow. First, the expansion brief's L3 entrance gate, "the first consumer that asks for the same arrangement or filter from a second window, device, or peer", is already met in production. Second, five of the ten fields are state the portable contract does not carry, which is exactly the promotion argument the catalog demands: a consumer built the surface locally because the contract lacked it.

A related tell sits inside the portable path. `visibility_diff` (`lib.rs:1144`) marks a folded root by pushing a channel literally named `"fold"` with value 1.0, because there is no typed field for it. Untyped named channels are the pressure valve the contract currently offers.

## Target-by-target verdict

| Target | Verdict |
| --- | --- |
| A1 placement satisfaction | **Strongest evidence yet, and a forcing consumer once one precondition is met.** The site ships the ensure/encourage split and shares pins to a second device, but its pins never enter a `Score`. It forces A1 the moment a pinned arrangement needs to survive as a portable artifact. The plan names isometry or the canvas intent; mer3ly is closer and public. |
| A2 selection clauses | **Entrance gate open, forcing consumer partial.** `selection` crosses the wire and `ActorScope::FocusAndNeighbors` scopes a reading, but there is one view, one selected id, and no resolution strategy. Mosaic's clause shape is unforced; the serialization question is not. |
| A3 LOD rungs | **Unforced; the plan's premise stands.** `representation_registry()` is exported to JS for the live path's UI, while the portable path hardcodes `Representation::Glyph`. Fold and expand are a manual toggle over `visible` plus a `"fold"` channel, not a measure, threshold, or hysteresis. |
| A4 transition specs | **Record shape already exists; the spec does not.** The default trace is a labeled, revision-chained diff sequence, which is most of what L5 needs for playback. What is missing is the Gemini-style authored spec: duration, easing, stagger. |
| A5 gap anatomies | Unaffected. The site exercises no scales, guides, facets, or set-scoped constraints. |
| B1 accessible frozen realization | **The site is the argument for B1, not a counterexample.** The sandbox is JavaScript-only: `data-graph-interface` ships `hidden`, the node list is populated at runtime, and the no-script status reads "Graphshell sandbox not initialized". Accessibility is served instead by a separate authority-derived static index (`tests/m5_repo_graph.rs:230` asserts fallback, then hidden interface, then index, with one `data-repository-id` per authority repository). The site works around B1's absence by rendering a second page from the authority. |
| B2 probe-drivable projections | Adjacent. The sandbox carries DOM identity throughout (`data-sandbox-*`, `data-repository-id`), so it is a natural driving target, but it is a plain web page plus a wasm canvas rather than a Genet realization. |
| B3 livery classing check | Unaffected. |
| C1 satisfaction chrome | Precedent only. The sandbox has pin controls and an `aria-live` status region, in a web page rather than Cambium host chrome. |
| C2 transition playback | Precedent. The sandbox already steps through history with a source-time control. |
| C3 backdrop classing | **Two properties already shipping and already on the wire.** `set_backdrop(backdrop, tangible)` and `backdrop { kind, collidable }` are evidence for L2's minimum property set: identity plus collidable, ahead of visible, hit, or provenance. |

## Corrections to the 2026-08-15 reading

Four claims from the first pass over this site do not survive the survey.

1. A1 is not "already shipped". The pin model exists but never reaches a score, which is a different and more useful finding.
2. A3's premise was not falsified. The exported registry serves the live path's UI, not portable rung selection.
3. B1 is not already served. The sandbox is not readable without JavaScript, and the accessible artifact is authority-derived and separate.
4. `graph_layout:penrose` does exist, at `crates/canvas/arrangements/src/registry.rs:358`, as a `LayoutCapability` id distinct from the adapter's `penrose.default` projection id. The site uses the registry id directly. An earlier claim that it was absent came from a truncated grep and has been reverted in the plan and the catalog.

## Questions for Mark

1. Does mer3ly count as authority-grade, a consumer whose asks open gates, or donor-only the way graphshell is? It differs from graphshell in being live, public, and pinned to a real rev.
2. Should the missing seam be a target in its own right? Letting the live path's placement reach a `Score` is the precondition for A1, A2, and C3 having one forcing consumer instead of three hypothetical ones.
3. Is `mer3ly.graphshell-scene-state/v1` a promotion candidate to be lifted, or a local convenience to leave alone until a second consumer wants the same fields?
