# Isometric Orrery Camera Plan: 2.5D as one parameterized planar camera

**Date**: 2026-06-22
**Status**: Planning (with Mark). The "2.5D isometric rung" of the
[orrery physics environments research](../research/2026-06-22_orrery_physics_environments_research.md):
the cheap, in-stack dimensional mode (an isometric, orbitable camera with fake height) over the
existing 2D rapier physics, with no rapier3d and no new render lane. Full 3D stays the gated long
pole in its own plan.
**Code**: `crates/orrery/orrery` (`CameraView`, `frame.rs`, `lib.rs` camera math, `input.rs`
gestures), `crates/platen/platen/src/scene_paint.rs` (the ground `Camera` + `PushTransform`),
`crates/system/session-runtime` (`view_intent_store` camera persistence), `crates/meerkat`
(the mode command + the focused content card anchor).

**Related**:

- [orrery_physics_environments_research](../research/2026-06-22_orrery_physics_environments_research.md)
  — the parent probe; this is its 2.5D rung. The dimensional hotswitch's full-3D mode is a separate
  future plan (it needs rapier3d + a 3D render lane via `compose_external_texture`).
- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md) — node
  cards / tiles / sprites are the **billboards** this plan keeps upright; the per-node size / face
  geometry (Decision 5) is what a billboard scales and what a ground shadow sits under.
- [command_registry_configurable_menus_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md)
  — the projection mode is a registry command (`view.projection.set`), palette- and script-reachable.
- [cartography_aether_layout_seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
  — the camera is a render/projection concern; gyre's world coordinates do not change.

---

## Thesis: one camera, 2D is the degenerate iso

Today the orrery maps world to screen with `screen = world * zoom + offset` (orrery `CameraView`,
types.rs:17; the platen `Camera`, scene_paint.rs:85; the inverse `screen_to_world`, lib.rs:1421).
That is the special case of an **isometric ground projection** at orbit yaw 0 and tilt 1 (no
foreshorten).

Generalize the camera to `{ offset, zoom, yaw, tilt }`:

- `yaw` rotates the ground plane about the vertical (the orbit angle).
- `tilt` foreshortens the vertical axis (1.0 = top-down 2D, ~0.55 = a classic isometric squash).
- `offset`, `zoom` are unchanged (screen-space pan, uniform scale).

So the 2D / 2.5D "dimensional hotswitch" is a continuous tween of `yaw` and `tilt`, not a branch.
2D mode is `(yaw = 0, tilt = 1)` and reduces, bit for bit, to the current map. This is why the rung
is cheap: no new physics, no new renderer, one projection type the whole orrery already funnels
through (once P0 consolidates it).

## The two consumers: ground transform vs billboard projector

The frame already composites two relevant layers (frame.rs:254-274): a **ground** layer (the
underlay edges, field regions, demoted dots) painted inside the camera `PushTransform`, and an
**on-screen** layer (the gnode DOM cards plus the favicon quads). Isometry treats them differently,
and the existing split is exactly the seam:

- **Ground elements shear with the iso affine, and that is correct.** Edges, field-region extents,
  the optional floor, and the tiny demoted dots lie *on* the ground plane, so applying the full iso
  affine in the `PushTransform` (rotate yaw, scale Y by tilt, scale zoom, translate offset) is the
  right look. This is the trivial matrix change.
- **Node cards / sprites stay upright billboards.** A node's card, tile, favicon, selection ring,
  and the focused content card must not shear into parallelograms. Each is positioned at its ground
  anchor `to_screen(world)` and drawn upright, scaled by `zoom` (closer-is-bigger is fine; shear is
  not). This is the meatier change, but it touches only the on-screen layer.

So the camera exposes two derivations of the same `{offset, zoom, yaw, tilt}`: a `ground_transform`
(a `LayoutTransform` for the `PushTransform`) and a point projector `to_screen` / `to_world` (for
billboard anchors and picking). A billboard always sits at its ground anchor, so the two stay
consistent.

## Findings (code-verified 2026-06-22)

The world<->screen math is open-coded in one inverse site and several forward sites. Funnelling
them through one projector is the prerequisite, and is a clean refactor regardless of isometry.

- **Two camera structs**, both `screen = world*zoom+offset`: orrery `CameraView` (host-facing /
  serialized, types.rs:17) and platen `Camera` (the ground transform, scene_paint.rs:85). The
  orrery's internal `self.camera` is passed straight into the underlay paint
  (`orrery_paint_list_demoted_from_arrangement(.., self.camera, ..)`, frame.rs:114).
- **The inverse is already centralized**: `screen_to_world` (lib.rs:1421), used by drag re-pin
  (frame.rs:52), `world_viewport` cull (lib.rs:1429), and the input gestures. One site to make
  iso-aware.
- **The forward map is scattered** and each must route through `to_screen`:
  - the gnode DOM **stage transform** `translate(offset) scale(zoom)` (frame.rs:146-153) — this is
    the **shear locus**: it wraps the whole gnode pool, so putting iso here would shear every card.
    The billboard fix moves projection to per-gnode anchors and keeps the per-gnode transform a
    *uniform* `translate(to_screen) scale(zoom)`;
  - per-gnode position `translate(pos - NODE_HALF)` (frame.rs:170-175);
  - the favicon quad corners, explicit `world*zoom+offset` (frame.rs:232-235);
  - `focused_node_screen`, the floating-card anchor (lib.rs:1196-1205);
  - `zoom_at`, anchor-preserving zoom (lib.rs:1411-1417);
  - field centering (fields.rs:173-175).
- **`world_viewport` cull breaks under rotation** (lib.rs:1429): it maps two screen corners to
  world. Under yaw the screen rectangle is a rotated world quad, so the cull AABB must be the bounds
  of all four inverse-projected corners (a small, correctness-required change).
- **The gnode transform/class updates are attribute-only (RepaintOnly)** (frame.rs:198-201), so
  adding a per-gnode `z-index` (for depth order) and the iso anchor transform stays on the cheap
  paint path, no relayout.
- **Physics is untouched.** Drag pins via `screen_to_world` into world coords (frame.rs:52); gyre's
  `hit_test` is world-space. With flat billboards (height 0) the inverse projection makes pick and
  drag correct with no physics change.
- **The `PushTransform` already takes a general `LayoutTransform`** (`scale(..).then(translation)`,
  scene_paint.rs:155), so the iso ground affine composes in directly *if* `LayoutTransform` exposes a
  2D rotation / shear builder; confirming that (or building it from raw matrix components) is a P1
  check, not an assumption.

## Phases (done-conditions, not dates)

### P0 — Consolidate the projection (no visual change)

Introduce one projector (extend the existing camera, do not add a third struct): `CameraView` and
the platen `Camera` gain `yaw` and `tilt` (default `0.0` / `1.0`), and grow `to_screen(world) ->
(f32,f32)`, `to_world(screen) -> world`, and `ground_transform() -> LayoutTransform`. Route every
forward site in Findings through `to_screen` and the inverse through `to_world`. Fix `world_viewport`
to bound four corners. Persist defaults so a pre-iso `view_intent` loads unchanged.

Done when all world<->screen math routes through the one projector, every existing test passes, and
the orrery is pixel-identical at `(yaw=0, tilt=1)`.

### P1 — Isometric ground + upright billboards (height 0, no orbit yet)

Add a projection mode that sets a fixed isometric `tilt` (and yaw 0 to start). Apply the iso
`ground_transform` to the underlay `PushTransform` (ground elements shear correctly). Reposition the
on-screen layer as billboards: each gnode at `to_screen(anchor)` with a uniform `scale(zoom)`
(drop the single stage shear), favicons at `to_screen`, the focused content card anchored via
`focused_node_screen` (now iso-aware). Depth-sort billboards by projected ground depth via per-gnode
`z-index` (attribute-only). Drag, pick, marquee, and zoom keep working through `to_world`.

Done when toggling to isometric renders the graph as an isometric ground with upright, zoom-scaled
node cards in correct front-to-back order, and grab / drag / fling / select / zoom are unchanged
(verified by a headed drive).

### P2 — Free cam (orbit + tilt) and the mode as a persisted setting

Make `yaw` and `tilt` live: an orbit gesture (a modifier-drag or right-drag in `input.rs`, kept off
the node-grab path) rotates `yaw`; a tilt control sweeps `tilt`. The 2D/2.5D toggle becomes a tween
of `(yaw, tilt)` between `(0, 1)` and the iso preset, exposed as the registry command
`view.projection.set` (TopDown / Isometric, per the command-registry plan) and on the orrery scene
settings. Persist `yaw` / `tilt` in `view_intent_store` alongside `offset` / `zoom` (extend
`CameraView` + the `CameraSnapshot`), serde-defaulted so old sidecars load.

Done when you can orbit and tilt the view freely, the 2D<->2.5D toggle animates between presets, and
the projection persists per scene across a reload.

### P3 — Fake height (the "2.5D" depth) and height-aware interaction

A per-element height `h`: billboards raised by `h * height_scale` in screen-Y, a ground-shadow
ellipse drawn at the un-raised anchor, depth order already handled by P1's z-index. Height source is
a setting (default flat; options: selected / hovered raised, or by-degree, mirroring size-by-degree).
Make pick and drag height-aware: hit-test against the raised screen rects (the host already computes
per-node screen boxes for hover), and subtract the height offset before the inverse on a raised-node
drag.

Done when nodes stand off the ground with shadows, taller / selected nodes read as nearer, and
picking / dragging a raised node lands on the right node.

## Open questions

- **Orbit gesture binding.** Right-drag, middle-drag (currently pan), or modifier-drag? Pan is
  middle-drag today (input.rs:51); orbit wants its own without stealing the node grab. Lean
  modifier-drag (e.g. Alt+drag) for orbit, keep middle-drag for pan.
- **Tilt range and iso preset.** A true isometric is `tilt ≈ 0.577` (30-degree); a softer 0.6-0.7
  reads less game-like. Expose as a setting with an iso default.
- **Does zoom stay uniform, or foreshorten with tilt?** Uniform zoom keeps the math simple and cards
  legible; only `tilt` foreshortens. Recommend uniform.
- **Height source.** Flat by default; the compelling auto-source (selected raised? by-degree, so
  hubs stand tall?) is a P3 setting decision, not a constant.
- **Billboard scaling vs the underlay.** The demoted underlay dots are tiny, so leaving them on the
  ground (sheared) is fine; confirm no visible seam between a demoted dot and the same node's
  billboard when it crosses the cull boundary.
- ~~**`LayoutTransform` rotation API.**~~ **Resolved (2026-06-22, netrender now local):**
  `paint_list_api::LayoutTransform` is `euclid::Transform3D<f32, LayoutPixel, LayoutPixel>`
  (`repos/netrender/paint_list_api/src/primitives.rs:36`), so `rotation(0,0,1, Angle)` / `scale` /
  `translation` / `then` are all available and flow through `layout_transform_to_scene` (4x4
  column-major). P2's yaw is a Z-rotation composed into `ground_transform`; it only needs
  `euclid::Angle` (a one-line `euclid` dep add to platen, already transitive). Tilt-only P1 did not
  need it.

## Grounding (code-verified 2026-06-22)

- `CameraView { offset, zoom }` (orrery types.rs:17, `screen = world*zoom+offset`); platen
  `Camera { offset, zoom }` (scene_paint.rs:85); the `PushTransform` is a general `LayoutTransform`
  (scene_paint.rs:155).
- Single inverse `screen_to_world` (lib.rs:1421); forward sites at frame.rs:146-153 / 170-175 /
  232-235, lib.rs:1196-1205 / 1411-1417, fields.rs:173-175; cull `world_viewport` (lib.rs:1429,
  two-corner, needs four under yaw).
- Frame composites ground (underlay, inside the camera transform) and on-screen (gnode DOM +
  favicon) as separate layers (frame.rs:254-274) — the ground-vs-billboard seam already exists.
- gnode transform/class updates are RepaintOnly (frame.rs:198-201); z-index + anchor transform stay
  on that path.
- Physics in world coords, untouched: drag pins via `screen_to_world` (frame.rs:52), gyre `hit_test`
  is world-space.

## Progress

- 2026-06-22: **Plan written (with Mark), code-verified against the orrery render + camera path.**
  Picked the 2.5D isometric rung from the physics-environments probe. Key finding: 2D is the
  degenerate isometric case `(yaw=0, tilt=1)`, so one parameterized planar camera unifies both and
  the mode toggle is a scalar tween, not a branch; and the existing ground-layer vs on-screen-layer
  split in `frame.rs` maps onto ground-shear (edges / fields / dots) vs upright billboards (cards /
  favicons), so a blanket iso matrix is right for the ground and wrong for the cards. Physics is
  untouched (rapier2d, world coords). Sequenced P0 consolidate the projector (funnel ~8 forward
  sites + the one inverse through `to_screen` / `to_world`, fix the cull to four corners) → P1 iso
  ground + upright billboards at height 0 → P2 free-cam orbit/tilt + the persisted mode command →
  P3 fake height + shadows + height-aware pick. No code yet.
- 2026-06-22: **P0 landed + verified (no behavior change).** Added `yaw` / `tilt` (default `0.0` /
  `1.0`) plus `to_screen` / `to_world` / `ground_transform` to the platen `Camera`
  (`scene_paint.rs`), and routed the orrery's world<->screen sites through them: `screen_to_world`
  and `focused_node_screen` and `zoom_at` (now anchor-preserving via `to_world` + `to_screen`,
  correct for any projection) and the `frame.rs` favicon corners (via `to_screen`); `world_viewport`
  now bounds four corners (yaw-safe). `scene_paint`'s `PushTransform` uses `camera.ground_transform()`.
  At the default camera every map is bit-identical: `camera_round_trips_and_guards_bad_zoom`,
  `screen_to_world_inverts_the_camera`, and `zoom_at_keeps_the_anchor_world_point_fixed` all pass
  (44 orrery + 77 platen tests, 0 failed; baseline was green). **Scope refinements**: `CameraView`
  stays `{offset, zoom}` (yaw/tilt + persistence deferred to P2, where they become live); the
  `fields.rs` field-centering math and the gnode `.stage` CSS transform are correct-at-default and
  deferred to P1 (the `.stage` transform becomes the ground transform; per-gnode positioning becomes
  billboard anchors). Left uncommitted for Mark (his sprite/node-editor work is in flight in the same
  crate, different regions).
- 2026-06-22: **P1 landed (isometric ground + upright billboards), tilt-only.** `ground_transform`
  foreshortens the vertical by `tilt` (`scale(zoom, tilt*zoom).then(translate)`); the gnode cards
  and favicons reposition as **upright billboards** at their `to_screen` anchors with the `.stage`
  container set to identity (the foreshorten lives in each node's anchor, not a stage shear).
  `Orrery::set_isometric` / `is_isometric` toggle the preset (`ISO_TILT = 0.55`); the standalone
  `orrery` bin's `i` key flips it for headed checks. At `tilt == 1` everything is unchanged (the
  top-down camera tests pass, plus a new `camera_top_down_is_plain_scale_translate`); a new
  `camera_isometric_foreshortens_y_and_round_trips` proves the foreshorten + `to_world` inversion.
  orrery 44 + platen 79 green, no new warnings. **Scope call**: the `yaw` orbit (a Z-rotation) is
  held for P2; tilt-only needs just `scale` + `translate`, matches the plan's "yaw 0 to start", and
  is recognizably 2.5D (the ground reclines, cards stand upright). **Deferred**: depth-sort (unneeded
  under tilt-only, vertical order is preserved); screen-space billboard picking (a click on the top
  of a tall card maps outside the foreshortened ground collider, so clicks near a node's base are
  most reliable; kept world-space pick via `to_world` to avoid editing Mark's in-flight `input.rs`).
  **Headed-verified** (scry-shots/iso-00..02 via `drive-iso.ps1`): pressing `i` foreshortens the
  ground (the graph reclines, compressing toward the camera's vertical center) while the node cards
  stay upright squares (billboards, not sheared) and edges foreshorten along the floor; pressing `i`
  again reverts to top-down. No render errors in the bin log. Left uncommitted.
- 2026-06-22: **P2 core landed + headed-verified (free-cam yaw orbit); meerkat tail deferred.**
  `ground_transform` now builds the full isometric affine via `LayoutTransform::new` (rotate `yaw` +
  foreshorten `tilt` + scale + translate), constructed to match `to_screen` exactly so edges meet
  the upright billboards under an orbit. A new `ground_transform_matches_to_screen_under_yaw` test
  pins that equality (no `euclid` dep and no rotation-sign-convention risk, since the matrix is
  derived straight from `to_screen`'s `cos`/`sin`). The orrery gained `orbit_by` / `set_yaw` /
  `set_tilt` / `yaw` / `tilt`; the gnode billboards now carry a `z-index` depth from the projected
  ground depth (post-yaw "north"), so an orbit paints front-to-back (stayed RepaintOnly, no relayout
  warning in the log). The standalone bin gained free-cam keys (`q` / `e` orbit, `[` / `]` tilt; `i`
  still toggles the iso preset). **Headed-verified** (scry-shots/iso2-00..02 via `drive-iso2.ps1`):
  `i` foreshortens, `e`x6 rotates the ground under the upright cards with edges still meeting them;
  no errors. orrery 44 + platen 80 green. **Deferred (Mark's in-flight files)**: persistence
  (`CameraView` / `CameraSnapshot` yaw+tilt + the meerkat save/restore, so yaw/tilt reset to
  top-down on reload for now), the `view.projection` registry command + the in-app orbit gesture
  (meerkat), and screen-space billboard picking (orrery `input.rs`). Left uncommitted.
- 2026-06-22: **P3 core landed + headed-verified (fake height: float + stems).** Added `node_height`
  (0, else `16*degree` capped 80 when height-by-degree is on) + `set_height_by_degree` /
  `height_by_degree`, mirroring size-by-degree (one struct field, purely visual — it does not move
  the gyre body). `frame.rs` raises each billboard card by `height * zoom` in screen-y and draws a
  stem (a thin translucent rect) from the card down to its ground anchor, composited under the cards;
  the P2 `z-index` depth already orders them. The bin's `h` key toggles it. **Headed-verified**
  (scry-shots/iso3-01 flat vs iso3-02 height via `drive-iso3.ps1`): with height on, cards float
  above the ground on stems while edges meet at the stem bases (ground anchors), hubs riding taller
  stems; composes with the iso tilt (P1-verified). orrery 44 + platen 80 green. **Deferred**:
  per-node height overrides + a "selected / hovered raised" source (the object-card / meerkat
  surface); a soft ground-shadow blob (stem-only for now); and height-aware picking (a floated card
  maps above its ground collider, so picking targets the base — part of the meerkat / `input.rs`
  screen-space-pick tail). Left uncommitted.
- 2026-06-22: **Screen-space billboard picking landed (the orrery-side half of the deferred tail;
  the meerkat-proper tail stays deferred per Mark, since command.rs is mid command-registry
  refactor).** Added `Orrery::pick_at`: the exact world-space collider pick under top-down
  (unchanged — shape-aware, including sprite hulls), switching to a screen-space billboard-rect pick
  (front-most by projected ground depth) when iso / fake-height lift the cards off their ground
  colliders. `pointer_down` (grab + select) and `node_at_screen` (sprite-drop) both route through it.
  orrery 44 green; the top-down branch is the prior code, so no regression. **Headed**: the input
  path is live under height (clicks register; an empty-gap click picks the nearest edge), but the
  non-deterministic force-directed settle made blind automated clicks land on edges/gaps rather than
  a targeted raised card, so precise click-on-card is an interactive spot-check. Doable cleanly
  because the WIP checkpoint committed `orrery/input.rs`. Left uncommitted. **Still deferred to the
  post-command-registry meerkat pass**: persistence (`CameraView`/`CameraSnapshot` yaw+tilt +
  save/restore), the `view.projection` command, and the in-app orbit gesture.
- 2026-06-23: **Camera work committed (`715203a`); deferred meerkat tail landed (command +
  persistence), build + 176 tests green; orbit gesture still deferred.** Committed the orrery/platen
  camera work (P0-P3 + picking) with an explicit pathspec (Mark's "commit only my paths"; meerkat
  untouched). Then, with meerkat compiling on Mark's refactor, the two free pieces went in (left in
  his dirty meerkat for his own commit, since they intermix with the command-registry refactor):
  (1) a **`ToggleProjection` command** (palette "Projection (toggle 2.5D isometric)" / `>projection`),
  mirroring `ToggleRoster` across `command.rs` (enum / `ALL` / `is_host_action` / `menu_scope` /
  `verb` / `label`), the `lib.rs` host-intent group, and a `command_drain` arm that flips
  `orrery.set_isometric`; (2) yaw/tilt **persistence with no schema change** — `camera_to_snapshot` /
  `snapshot_to_camera` now encode/decode the full camera into the existing six affine coefficients
  (`a=cos*zoom, b=sin*tilt*zoom, c=-sin*zoom, d=cos*tilt*zoom`) plus a new `snapshot_yaw_tilt` decode;
  an old top-down snapshot (`b=c=0, a=d=zoom`) reads back as `(yaw 0, tilt 1)` for free, so pre-iso
  sessions load unchanged. Save (`session_ops`) passes `orrery.yaw()/tilt()`; both restores (boot
  `main.rs` + session-switch `session_ops`) set them. **Still deferred**: the Alt+drag in-app orbit
  gesture (involved `input.rs`; the command + the bin's q/e/[/] keys cover driving it). Headed
  palette / reload spot-check is a quick confirm in the app (meerkat is Mark's in-flight, not
  auto-driven).
- 2026-06-24: **Alt+drag orbit gesture landed (committed `70a486f`); the plan is fully complete.** The
  last deferred piece, now that the command-registry refactor settled and the tree was clean. Wired as
  a first-class orrery gesture beside the middle-drag pan: an `alt` flag (`set_alt`, mirroring
  set_ctrl / set_shift) + an `orbit_drag` state; Alt+left-press starts an orbit (skipping node-pick /
  field-grab / marquee), `cursor_moved` yaws by the horizontal delta (`orbit_by`) and reclines tilt by
  the vertical (`set_tilt` clamps, so no flip), `pointer_up` ends it; two sensitivity consts (a ~30px
  drag ≈ the q/e keystep). Lives in the orrery, so the bin gets it too; both hosts push `set_alt` in
  ModifiersChanged. orrery 63 tests green incl. a gesture test (Alt+drag yaws + reclines; release ends
  it; a non-Alt left drag does not orbit). Headed-verified (scry-shots/isoorb-a vs -b): an Alt+left-drag
  reclines + rotates the field, cards upright. The in-app orbit now matches the q/e/[/] keys + the
  `>projection` toggle. **Nothing deferred remains.**
