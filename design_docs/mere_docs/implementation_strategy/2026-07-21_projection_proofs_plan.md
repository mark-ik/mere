# Projection Proofs Plan

**Date**: 2026-07-21
**Status**: P3/P4/P5 code slices are landed in the working tree. P3 has a
green two-process headed receipt, including score restore after restart. Live
radio facts remain correctly deferred because no radio product exposes them.
Executes the five-proof sequence from the
[projection_engine_prior_art_brief](../research/2026-07-21_projection_engine_prior_art_brief.md)
§9. Proof 1 landed same-day. The scenograph family repo is founded
([mark-ik/scenograph](https://github.com/mark-ik/scenograph), commit `5a730e1`:
`sceno` core / `scenomise` choreography / `scenotime` runtime / `scenograph`
facade, MIT/Apache ed2024, name-holding; crates.io publication is Mark's step).

## Plan

- **P1 — wire the analytic catalog through turnstone (DONE 2026-07-21).** The
  smoke test: one strategy exposed as a palette action, projected through the
  canvas's recompute gate, applied per frame, headed receipt. Done condition
  met: `RESULT ok` +
  `testing/turnstone/images/scenarios/proof1_projection/{01_force_baseline,02_phyllotaxis,03_force_restored}.png`.
- **P2 — the portable scene contract.** Point, rectangle, and polygon
  footprints; instance ids; coordinate spaces; representation slots (brief
  §5), landing in `sceno` with mere as first consumer. This is where the
  `cartography::Projection` point+radius ceiling lifts and where P1's overlap
  finding gets its fix.
- **P3 — pane spiral.** Move the kernel-neutral Spiral solver into
  `scenomise`; keep the graph-to-scene adaptation in `mere-cartography`.
  Persist one product-free score and prove that its measurements give
  footprint-aware placement. Recency drives scale; the realization traverses
  glyph, card, snapshot, and focused live-pane LOD. **Implemented
  2026-07-22:** score + solver + graph adapter, durable revisit timestamps,
  session-sidecar save/rebind, and a two-process headed scenario pair. The
  score proves selected LOD; a richer host realization of those slots remains
  separate presentation work, not a type-system claim.
- **P4 — Isometry consumes that same contract** for its overmap and one
  tile-board projection. Delete `Overmap::layout` rather than wrapping it;
  Isometry's adapter keeps campaign truth, authored pins, and paint local.
  The receipt must serialize the same score/scene vocabulary as P3, and an
  audit of `sceno`/`scenomise` types must contain neither `mere` nor
  `isometry`.
- **Boundary consolidation — after the two-product proof.** Finish moving
  the generic Spiral out of Mere; tighten `mere-cartography` and
  `mere-canvas` around the graph-specific remainder. Done for the proved
  shape: every canvas Spiral dispatch now passes through the shared score and
  solver, while `arrangements` remains only for its distinct nonportable
  catalog entries. No Cambium/Sprigging API was extracted: the two consumers
  presently agree only on the existing `GraphCanvasSwatch` contract. This is a
  boundary map, not a cleanup queue.
- **Graphshell — begin only after the second-consumer boundary is real.**
  Its first work consumes the consolidated Scenograph vocabulary; it does not
  create a competing abstraction while P3/P4 are still proving one.
- **P5 — fixture-driven geographic projection** over that consolidated
  boundary is implemented as a serialized coastal-map fixture: an ordinary
  low-layer map underlay plus geographic source facts, preserved through
  score-to-scene realization. Live Retinue/Tulle/Sennet location facts wait
  for an actual radio fact surface; none exists in those products today.

**Done overall** = one persisted, product-free score vocabulary drives the
Mere pane spiral, the Isometry overmap and board, and the geographic fixture;
neither portable crate nor serialized scene type mentions either product.

## Findings

- **P1 confirmed the "separate lanes" diagnosis exactly.** The canvas strategy
  seam (`set_layout_strategy` / `needs_strategy_recompute` /
  `apply_strategy_positions` / `note_strategy_computed`) was complete,
  documented, and host-ready; the entire gap was the host loop. The turnstone
  wiring is ~60 lines: an `Action::SetLayoutStrategy(Option<&'static str>)`
  variant + two palette entries (`action.rs`), a dispatch arm + an
  `App::drive_layout_strategy(w, h)` host loop (`app.rs`), the drive call
  before both canvas `frame()` sites (`shell.rs`), and two scenario-runner
  verbs (`script.rs`: `layout_phyllotaxis` / `layout_force`).
- **The footprint channel is now an empirical finding, not a prediction.**
  `phyllotaxis.default` at 15 nodes packs the spiral into heavy node overlap
  (capture 02): the projection emits positions only, node extents are ~40px
  canvas squares the strategy cannot see, and nothing scales spacing to
  extent. The failure is missing placement *data* (footprints / spacing
  awareness), not missing wiring — the exact distinction P1 existed to make.
  Configurability note: per-strategy scale/spacing settings are the cheap
  palliative; the real fix is P2's extent-aware contract.
- **`project_canvas_strategy` discards all but positions** (radius, overlays,
  bounds dropped), as the review warned; the gloss lens path
  (`project_canvas_lens`) keeps overlays. Acceptable for P1; the boundary
  lift is P2's job, not a patch here.
- **The scenario lane absorbed the new action for free.** `act <palette
  label>` + self-capture ran unchanged; automation reach cost two mapping
  lines. Receipt of the one-tool-vocabulary doctrine paying rent.
- **Owed follow-ups**: persist the active strategy as view-intent (the canvas
  doc expects the host to; not wired), a checkmarked layout picker surface
  (palette-only today), and the remaining catalog ids as palette entries
  (grid / penrose / lsystem / timeline / kanban / radial / spectral all
  dispatch already).

## Arrangement naming (plain vocabulary, stack-consistent)

The Merely brand's projection grid (mosaic / spiral / atlas / hulls / workbench)
names the arrangements in plain product words. Adopting them requires honoring a
distinction the brand blurs: **the stack has two name registers, and they must
not collide.**

- **Surface register** (what hosts content, code-verified heavy use): orrery,
  workbench, gloss, roster, inspector, apparatus, trail, steward. Off-limits as
  arrangement names. The brand's "orrery" and "workbench" cells are *surfaces*,
  not arrangements — orrery's default arrangement is force-directed; workbench's
  behavior is symmetric splits.
- **Arrangement register** (how nodes are placed inside a surface): the plain
  display names below. The `graph_layout:*` id stays the persistence key; only
  `display_name` carries the plain word.

| Arrangement id | Plain name | Status | Note |
|---|---|---|---|
| `graph_layout:phyllotaxis` | **Spiral** | done (registry + palette) | brand "spiral"; age-shrink is P3 |
| (force-directed / seiche `None`) | Force-directed | done (palette) | orrery's native arrangement; keep honest name |
| `graph_layout:grid` | Grid | plain already | |
| `graph_layout:radial` | Radial | plain already | volvelle = a *moot* radial, different concept |
| `graph_layout:timeline` | Timeline | plain already | |
| `graph_layout:kanban` | Kanban | proposed → Board? | jargon; ratify |
| `graph_layout:penrose` | Penrose | keep | aperiodic tiling; **not** Mosaic (proper noun ok) |
| `graph_layout:lsystem` | L-System | proposed → Fractal? | ratify |
| `graph_layout:semantic_embedding` | Semantic | keep | |
| (spectral) | Spectral | keep | jargon; no clean plain word |
| *(future, P4)* adjacency tiling | **Mosaic** | reserved (free) | brand "mosaic"; data-driven tiles, distinct from Penrose |
| *(future, P5)* geographic | **Atlas** | reserved (free) | brand "atlas"; distinct from isometry's *overmap* surface |
| *(future)* burn-kinship regions | **Hulls** | reserved | brand "hulls"; ties to the fields/hulls primitive |
| *(future)* joint-constrained bodies | **Armature** | **ratified 2026-07-31, DISPLAY NAME ONLY** | positions fixed by attachment frames, never force-settled — the nested-graph-scale sibling of orrery. First consumer: the games wing's live body swatch (a critter's body is a nested graph; see `mesocosm/design_docs/2026-07-30_body_pipeline_and_host_probe_plan.md` §3). **Never a crate name**: `armature` is taken on crates.io (an event-driven stateful actor framework, 0.1.1, 2021, dormant), and its domain is adjacent to **armillary**, so publishing under it would collide twice over. The `graph_layout:*` id stays the persistence key, as for every arrangement |

Collision checks (2026-07-22): `mosaic` zero hits stack-wide; `atlas` only a
"font/SVG atlas" texture ref in graph-kernel; `spiral` only an internal grid
traversal + phyllotaxis config word (Spiral-the-display-name is unambiguous at
the picker level). Isometry's `overmap` is a *surface* (its travel map), the
same surface-vs-arrangement split as orrery-vs-force-directed; when isometry
consumes scenograph (P4) it keeps "overmap" and may render via the Atlas
arrangement. Only Phyllotaxis→Spiral is renamed now (wired + greenlit); the
`Board`/`Fractal` proposals await Mark's ratification, not applied unilaterally.
The proper consistency end-state is the checkmarked layout picker deriving its
labels from the registry `display_name` (owed follow-up), so turnstone stops
hardcoding labels; today the hardcoded palette label is kept *matching* the
registry.

## Progress

- 2026-07-21: **P1 landed.** Scenograph family founded and pushed
  (`5a730e1`). Turnstone wiring per Findings; `cargo check` green (bin, own
  cwd); headed receipt via the self-drive scenario lane
  (`scenarios/proof1_projection.scn`, fresh `TURNSTONE_ROOT` profile):
  force-directed baseline → phyllotaxis applied (physics halted, analytic
  layout holds still) → revert re-settles. `RESULT ok`, three captures,
  whole-frame check clean. Turnstone changes left uncommitted for Mark's
  review.
- 2026-07-22: **Merely brand read + plain-vocabulary rename (with Mark).** The
  brand doc's animated mark *is* this engine (one graph re-projecting between
  force-directed / mosaic / spiral / atlas; "the projection decides what a node
  *is*"), and its projection grid supplies plain product names. Ruled: brand
  palette + NODE_SHEET re-color are deferred to tinct theming / stylesheet
  customization (not hardcoded now). Applied the arrangement rename
  Phyllotaxis→**Spiral** consistently — registry `display_name`, turnstone palette
  label, scenario `act` label, script verb `layout_spiral` — after the
  stack-consistency survey that produced the two-register model + naming table
  above. Registry rename is display-only (id/tags unchanged); `Board`/`Fractal`
  proposals recorded but not applied. Re-ran the scenario to confirm the renamed
  `act Layout: Spiral` label resolves.
- 2026-07-22: **P2 first slice landed in sceno** (scenograph `c2719ab`, 12
  tests): geometry (similarity `Transform2`), `Footprint`
  (Point/Circle/Rect/Polygon/Path + bounds — the proof-1 fix's type),
  `Scene` (interned `SourceRef`s making source-vs-instance structural;
  `Space` chains with cycle-safe `to_world` — the frames ruling as data;
  `ProjectedItem` with representation slot + hit shape; `RoutedRelation`;
  `Region` absorbing ClusterHalo and serving the hulls lane), and
  `Measurements` (the representation-measures input half). Decisions + open
  questions in `scenograph:design_docs/2026-07-22_scene_contract_note.md`,
  written for Mark's review **before** mere wires on. Deferred on purpose:
  Score types, intents, diffs, per-item emphasis channels, 3D variants.
  Next slice: mere consumes (scene-shaped strategy output beside
  `cartography::Projection`; phyllotaxis reads `Measurements`; re-run
  proof-1 scenario showing the spiral clear its cards).
- 2026-07-22: **P2 consumption slice landed — mere consumes sceno, and the
  spiral clears its cards (headed receipt).** Dependency: mere consumes
  `sceno` as a git sibling (`scenograph.git` branch=main in the workspace
  block; local checkout via the gitignored `.cargo/config.toml` patch, the
  armillary pattern). Extent lane: `ViewIntent.extents` (per-node `(w, h)`,
  view-config like `axis_values`, additive contract) →
  `CartographySceneOptions.extents` → dispatch → `PhyllotaxisAdapter`
  effective scale = `config.scale.max(max_side × 1.6)` (√2 axis-aligned
  clearance ÷ ~0.9 Vogel neighbor compression), locked by a
  no-pair-overlaps unit test; `Canvas::strategy_extents()` measures from
  `node_size` (per-node overrides + degree/importance channels ride along);
  turnstone's drive loop passes them. Scene lane: `cartography::scene_out`
  (`scene_from_projection` + `MERE_GRAPH_ADAPTER`): positioned nodes →
  items with Rect-from-measurement / Circle-from-radius / Point footprints,
  caller-supplied stable ids (Uuid, never NodeKey), edges → routed
  relations with weight + path, `ClusterHalo` → `Region`,
  `ImportanceScale` → transform scale; 5 tests; no live consumer yet
  (stated honestly — the swatch/cambium consumer is a later slice). Label
  consistency: `CANVAS_LAYOUT_STRATEGIES` "Phyllotaxis" → "Spiral" (the
  second display-name table, now matching the registry). Tests: cartography
  16 + arrangements 88 + canvas 137 green; turnstone rebuilt; scenario re-run
  fresh-profile `RESULT ok` — capture 02 now shows a legible golden-angle
  spiral with every 36px box clear (the P1 overlap capture is overwritten
  on disk; the finding text stands as its record). **Follow-ups surfaced**:
  `needs_strategy_recompute` does not key on extents (a node resize under
  an active analytic layout waits for a revision/viewport/focus change to
  re-space; add an extents fingerprint to the cache inputs); the subgraph
  path passes no extents (NodeKeys do not survive the re-add; thread ids if
  the gloss wants measured re-layout).
- 2026-07-22: **P3a landed — recency drives scale + the Spiral's ordinal.**
  Two channels, both deterministic-tested. **Ordinal**: `SpiralOrdering`
  on `PhyllotaxisAdapter` (kernel-aware layer, not the portable config) with
  `RecentFirst` sorting by `last_visited` desc (stable, epoch fallback) so
  ordinal 0 — the `Outward` center — is the newest node; the meristem /
  brand "age shrinks what you leave behind" reading. **Scale**: a
  `size_by_recency` toggle on `Canvas`, mirroring size-by-degree/importance
  — `recompute_recency` normalizes `last_visited` across the graph
  (`0..=1`, newest = cap; a single-timestamp graph reads uniform, not
  collapsed), refreshed each `push_node_geometry` (cheap, no dirty flag),
  and `node_size` maps it DEFAULT..MAX (36..88) below a manual override and
  size-by-importance, above size-by-degree. Wiring: canvas dispatch takes
  `recent_first`, driven by `canvas.size_by_recency()`; turnstone
  `Action::ToggleSizeByRecency` (+ palette + `toggle_size_by_recency` verb)
  re-selects the active strategy to drop its cache. Added `Action::FitView`
  (+ palette + `fit_view` verb) exposing the shipped `fit_to_content` —
  analytic layouts land anywhere in world space and the extent-aware Spiral
  spreads wide, so the view needs an explicit fit. Tests: `arrangements`
  recency-ordering + `mere-canvas` recency-size unit tests (89 + 138 green);
  headed `proof3_recency` scenario `RESULT ok` (uniform → size gradient →
  newest-at-center Spiral, fitted). **Findings, honest**: (1) **`visit()`
  does not refresh `last_visited`** on an existing node (input.rs:284 just
  selects), so live recency currently reflects creation/ingest order, not
  last interaction — the receipt's gradient is the sample graph's
  construction order, and closing this is a precondition for recency as a
  real UX feature. (2) **The DOM gnode face is a fixed 36px** (build.rs
  `GNODE_BOX`); `node_size` drives underlay rects (demoted nodes),
  colliders, edge-trims, rings, and hit-testing, but not the primary painted
  face — so *every* size channel (degree/importance/recency) is only
  partially visible. Making the gnode face track `node_size` is the
  rendering follow-up that would make P3a (and its siblings) fully legible.
  (3) camera does not auto-fit analytic layouts (hence `FitView`). P3b (LOD:
  representation degrades card→glyph with recency, focus stays live) is the
  remaining half of P3.
- 2026-07-22: **P3/P4/P5 boundary proof implementation.** `sceno::Score`
  now serializes the neutral `Spiral`, `Board`, and `Geographic` arrangements,
  measured footprints, selected representation slots, opaque source refs, and
  generation. `scenomise::solve` realizes it footprint-aware. Mere's graph
  adapter maps durable visit recency to score order, selects
  glyph/card/snapshot/live-pane slots, persists `projection-score.json`, and
  rebinds its source UUIDs into the canvas strategy buffer on session restore.
  Existing-node `visit()` now stamps the replayable visit delta, closing P3a's
  recorded honesty gap. The old arrangements Phyllotaxis adapter no longer
  drives *any* canvas Spiral path.

  Isometry deleted `Overmap::layout` and its force solver. Its views adapt
  authored pins to a geographic score or unpinned sites to the portable
  spiral, fit only the realized scene to the Cambium viewport, and emit a
  tactical ground board through the same score/scene types. The Isometry tests
  serialize both shared types and assert their type paths originate in
  `sceno`; opaque `isometry.*` adapter strings remain local return addresses.

  P5 adds `scenomise/fixtures/coastal_map.json`, containing a low-layer map
  underlay plus geographic facts and LOD selections. The fixture verifies
  disclosed coordinates, generation, LOD, and layer behavior. There is no
  Retinue/Tulle/Sennet location-fact API to adapt yet, so none was invented.
  Focused receipts: `sceno` 13 tests, `scenomise` 4 tests,
  `mere-cartography` 17 tests, `mere-canvas` 140 tests, and
  `isometry-views` 37 tests, and the Turnstone session sidecar test 3 tests.
  The headed P3 create/save run and independent restart/restore run both
  report `RESULT ok`.
- 2026-07-23: **The visible size channel landed, and verifying the receipts
  found the restore silently broken.** (1) **Gnode face fix**: the `.gnode`
  class baked `width/height: 36px` and the per-frame style set only the
  transform, so `node_size` drove colliders, edge trims, rings, and
  hit-testing but never the painted face — *every* size channel (per-node
  override, size-by-degree / -importance / -recency) was invisible on the
  node itself. The inline per-node style now carries width/height from
  `node_size`; the centring half generalizes `NODE_HALF * z` to
  `face * 0.5 * z` (identical at the default size); the favicon inset scales
  off the resolved face. Receipt: `proof3_recency` 02/03 now show the newest
  node markedly largest and the Spiral seating it largest-at-centre with age
  shrinking outward — P3a's claim is finally legible rather than inferred.
  (2) **Restore was recomputing, not restoring.** `restore_projection_score`
  set `last_strategy_inputs = None`, so the host's very next
  `needs_strategy_recompute` reported stale and rebuilt the arrangement from
  *live* inputs (recency off ⇒ uniform extents + graph-order ordinals),
  discarding the saved score before it painted. The verify scenario still
  reported `RESULT ok` because `assert nodes >= 14` / `assert visible` pass
  for any non-empty canvas. Fixed with a `restored_score_hold`
  `(strategy id, graph revision)` claim that the gate honours until the graph
  changes, the user picks a layout, or a recompute is recorded; pinned by
  `a_restored_score_survives_the_hosts_next_recompute_check`. The restart
  capture now reproduces the saved spiral position-for-position.
  **Lesson**: a headed `RESULT ok` is only as strong as its asserts — the
  deterministic unit test, not the scenario, is what guards this invariant.
  (3) **Receipt audit**: `Overmap::layout` is genuinely deleted (not
  wrapped); `isometry-views` calls `scenomise::solve`; `isometry-core` 37 +
  `isometry-views` 56 green; the portability audit finds `mere`/`isometry` in
  `sceno`/`scenomise`/`scenotime` only in doc-comment examples and one test
  fixture string — no types, deps, or code paths. **Open**: the restored
  score carries footprints, but the face still paints from live `node_size`,
  and `size_by_recency` is not persisted — so a restored spiral comes back
  uniformly sized. Either persist the size channel or let the restored
  score's footprints drive the face.
- 2026-07-23: **Physics promoted from a layout mode to a global capability
  (ruled with Mark), and the restored score now carries its sizes.**
  (1) **Footprint → face.** `restore_projection_score` writes each item's
  measured footprint onto the per-node size channel, so a restored spiral
  returns at the size it was saved with rather than uniformly. The score is
  the persisted truth for *both* placement and measured extent. Tradeoff
  recorded: restored sizes land in `node_sizes` (the manual-resize slot), so
  a later size-*mode* toggle needs `clear_node_size` to take over.
  (2) **Physics is a capability, not a mode.** Mark: "whether a given graph is
  affected by physics should be a global question that can apply to all
  layouts and arrangements... the idea of 'the mode with the physics' feels
  like a squandering." The code agreed: `physics_paused` already existed as a
  proper global gate (`settle_physics` is "the single gate every settle
  trigger routes through") but was **unexposed in the host**, while
  `set_layout_strategy` bypassed it with a direct `physics.halt()` — welding
  physics to the arrangement. Now: `set_physics_paused` is the one global
  control; picking an arrangement pauses *by default* through that visible,
  reversible flag rather than a hidden halt; `apply_strategy_positions` seeds
  the physics **world** (`physics.seed`, which already existed) so an
  arrangement is a real initial condition; `apply_strategy_to_view` re-asserts
  positions only while paused, so playing does not pin the nodes. The full 2×2
  is now reachable and receipted (`proof3_physics_capability`): Spiral paused
  (crisp analytic placement) → Spiral played (sim relaxes from the seed, the
  arrangement still selected) → free graph frozen (the cell that was
  unreachable while physics was welded to the mode). "Force-directed" is
  demoted to what it always was: *no* analytic arrangement with physics
  running. Host: `Action::TogglePhysics` + palette "Play/pause physics" +
  `toggle_physics` verb. Pinned by
  `physics_is_global_and_composes_with_any_arrangement`; canvas 142 green.
  **Next rung (Mark's framing)**: arrangements plug into the broader capability
  set (size, colour, shape, physics), and *any* graph surface should be able to
  run physics — "even swatches should be able to become physics-directed
  graphs... a gesture, something like knocking twice on the graph's canvas."
  Swatch physics is a real build (the swatch contract is a static position map
  today; it would need a sim per swatch or a shared one) and wants its own
  slice. The deeper form is **arrangement-as-attractor**: rather than a
  one-shot seed, an arrangement's targets become spring anchors, which is
  exactly the coupling/field model — the same mechanism hulls use. That is
  where arrangements, fields, and physics become one design instead of three.
- 2026-07-23: **Arrangement-as-attractor landed — arrangements, fields, and
  physics are now one mechanism.** `seiche::AnchorSpring` (conatus `82ac70f`)
  gives each node a spring toward the slot its arrangement chose, applied
  after the graph's own forces so stiffness reads as "how much does the
  arrangement win". It is deliberately the same shape as `CouplingForce` — a
  target, a response, a strength — which is what collapses the arrangement
  lane and the field/hull lane into one design instead of two. Canvas side:
  `arrangement_pull` (default `DEFAULT_ANCHOR_STIFFNESS`) with
  `set_arrangement_pull`, and `sync_anchor_force` installing anchors only
  while *playing* (paused, the buffered positions are asserted directly, so no
  force is needed). At zero pull the arrangement is a pure initial condition —
  the seed-only reading — and the graph's forces take over entirely. Pinned by
  `a_playing_arrangement_pulls_as_a_field_not_an_override`; canvas 143 green.
  Receipt: the played Spiral now keeps its recognizable shape while relaxing,
  where the seed-only version dissolved into an unrelated blob.
  **Build-topology finding (cost of the sibling split)**: turnstone could not see
  the new force at all — mere patches numen/quint/seiche to the local conatus
  checkout, turnstone patched none of them, so it silently built against the
  published crates. Patching *one* is worse than none: local seiche path-deps
  local numen/quint, so a partial patch yields two copies of the same types
  ("expected `numen::coupling::CouplingResponse`, found `CouplingResponse`").
  The family patches together or not at all; turnstone's gitignored
  `.cargo/config.toml` now mirrors mere's set exactly. Generalizes the
  cargo-cwd lesson: after extracting a crate, every consumer needs the whole
  family patched, and a physics change now costs either a publish or a
  patch-sweep.
- 2026-07-23: **Swatch physics — the capability reaches every graph surface.**
  Mark: "even swatches should be able to become physics-directed graphs... a
  gesture, something like knocking twice on the graph's canvas." The blocker
  was that a swatch is a `GraphCanvasSubgraph` snapshot with no sim, and the
  obvious fixes were both wrong: rapier does not belong in Cambium (a widget
  toolkit) nor in the portable projection family (a contract crate). So
  `scenomise::relax` is dependency-free and deterministic — repulsion between
  placed items scaled by their measured footprints, springs along routed
  relations, and a pull back toward the arrangement's own slots. That last
  term is the `AnchorSpring` idea at swatch scale, so a swatch reads with the
  *same vocabulary* as the canvas (arrangement as participant, not authority)
  while the heavyweight sim stays where it belongs. Cost is stated honestly in
  the docs: `O(steps · n²)`, right for tens of nodes, wrong for thousands —
  a surface with a real sim keeps using it. **Consumer proof**: isometry's
  `overmap_positions_relaxed` loosens the realized scene before the viewport
  fit, locked by a test asserting relaxation changes placement but never
  membership, stays inside the normalized viewport, and is deterministic
  (`isometry-views` 38 green; workspace green under `--all-features` per that
  repo's guard). `None` keeps the analytic path, so the default is unchanged.
  **Gesture deliberately not invented**: "knock twice" is a good instinct but
  double-click collides with create-on-canvas in most tools, so the affordance
  wants its own ruling rather than an inherited default. The capability is
  reachable by API today; the gesture is a separate, deliberate choice.
  **Review sweep**: scenograph 29, seiche 53, `mere-canvas` 143,
  `mere-cartography` 17, `arrangements` 89, `isometry-views` 38 — all green.
