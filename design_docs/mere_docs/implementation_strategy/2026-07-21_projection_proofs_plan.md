# Projection Proofs Plan

**Date**: 2026-07-21
**Status**: Active. Executes the five-proof sequence from the
[projection_engine_prior_art_brief](../research/2026-07-21_projection_engine_prior_art_brief.md)
§9. Proof 1 landed same-day. The scenograph family repo is founded
([mark-ik/scenograph](https://github.com/mark-ik/scenograph), commit `5a730e1`:
`sceno` core / `scenomise` choreography / `scenotime` runtime / `scenograph`
facade, MIT/Apache ed2024, name-holding; crates.io publication is Mark's step).

## Plan

- **P1 — wire the analytic catalog through merecat (DONE 2026-07-21).** The
  smoke test: one strategy exposed as a palette action, projected through the
  canvas's recompute gate, applied per frame, headed receipt. Done condition
  met: `RESULT ok` +
  `testing/merecat/images/scenarios/proof1_projection/{01_force_baseline,02_phyllotaxis,03_force_restored}.png`.
- **P2 — the portable scene contract.** Point, rectangle, and polygon
  footprints; instance ids; coordinate spaces; representation slots (brief
  §5), landing in `sceno` with mere as first consumer. This is where the
  `cartography::Projection` point+radius ceiling lifts and where P1's overlap
  finding gets its fix.
- **P3 — browser nodes as pane slots in a configurable phyllotaxis spiral.**
  Recency drives scale and LOD; focused content stays live; small items
  degrade to snapshots / cards / glyphs. The "representation measures,
  projection places" proof.
- **P4 — isometry consumes the same contract** for its overmap and one
  tile-board projection, deleting its hand-rolled force layout
  (`isometry-core/src/overmap.rs:123`).
- **P5 — fixture-driven geographic projection** (facts from fixtures, map
  underlay), then live retinue/tulle/sennet location facts when they exist.

**Done overall** = the same serialized score drives a mere pane spiral and an
isometry map, with neither portable crate depending on either product.

## Findings

- **P1 confirmed the "separate lanes" diagnosis exactly.** The canvas strategy
  seam (`set_layout_strategy` / `needs_strategy_recompute` /
  `apply_strategy_positions` / `note_strategy_computed`) was complete,
  documented, and host-ready; the entire gap was the host loop. The merecat
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

Collision checks (2026-07-22): `mosaic` zero hits stack-wide; `atlas` only a
"font/SVG atlas" texture ref in graph-kernel; `spiral` only an internal grid
traversal + phyllotaxis config word (Spiral-the-display-name is unambiguous at
the picker level). Isometry's `overmap` is a *surface* (its travel map), the
same surface-vs-arrangement split as orrery-vs-force-directed; when isometry
consumes scenograph (P4) it keeps "overmap" and may render via the Atlas
arrangement. Only Phyllotaxis→Spiral is renamed now (wired + greenlit); the
`Board`/`Fractal` proposals await Mark's ratification, not applied unilaterally.
The proper consistency end-state is the checkmarked layout picker deriving its
labels from the registry `display_name` (owed follow-up), so merecat stops
hardcoding labels; today the hardcoded palette label is kept *matching* the
registry.

## Progress

- 2026-07-21: **P1 landed.** Scenograph family founded and pushed
  (`5a730e1`). Merecat wiring per Findings; `cargo check` green (bin, own
  cwd); headed receipt via the self-drive scenario lane
  (`scenarios/proof1_projection.scn`, fresh `MERECAT_ROOT` profile):
  force-directed baseline → phyllotaxis applied (physics halted, analytic
  layout holds still) → revert re-settles. `RESULT ok`, three captures,
  whole-frame check clean. Merecat changes left uncommitted for Mark's
  review.
- 2026-07-22: **Merely brand read + plain-vocabulary rename (with Mark).** The
  brand doc's animated mark *is* this engine (one graph re-projecting between
  force-directed / mosaic / spiral / atlas; "the projection decides what a node
  *is*"), and its projection grid supplies plain product names. Ruled: brand
  palette + NODE_SHEET re-color are deferred to tinct theming / stylesheet
  customization (not hardcoded now). Applied the arrangement rename
  Phyllotaxis→**Spiral** consistently — registry `display_name`, merecat palette
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
  merecat's drive loop passes them. Scene lane: `cartography::scene_out`
  (`scene_from_projection` + `MERE_GRAPH_ADAPTER`): positioned nodes →
  items with Rect-from-measurement / Circle-from-radius / Point footprints,
  caller-supplied stable ids (Uuid, never NodeKey), edges → routed
  relations with weight + path, `ClusterHalo` → `Region`,
  `ImportanceScale` → transform scale; 5 tests; no live consumer yet
  (stated honestly — the swatch/cambium consumer is a later slice). Label
  consistency: `CANVAS_LAYOUT_STRATEGIES` "Phyllotaxis" → "Spiral" (the
  second display-name table, now matching the registry). Tests: cartography
  16 + arrangements 88 + canvas 137 green; merecat rebuilt; scenario re-run
  fresh-profile `RESULT ok` — capture 02 now shows a legible golden-angle
  spiral with every 36px box clear (the P1 overlap capture is overwritten
  on disk; the finding text stands as its record). **Follow-ups surfaced**:
  `needs_strategy_recompute` does not key on extents (a node resize under
  an active analytic layout waits for a revision/viewport/focus change to
  re-space; add an extents fingerprint to the cache inputs); the subgraph
  path passes no extents (NodeKeys do not survive the re-add; thread ids if
  the gloss wants measured re-layout).
