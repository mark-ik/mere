# Node Body & Face Model — decoupling shape from texture, authoring physical bodies

**Date**: 2026-06-23
**Status**: Planning (with Mark). Successor to the representation half of the
[node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md),
which is substantially complete (P0 cues, P1 per-node form, P2-static sprite faces + the
sprite-alpha hull collider, P3/P4 done) and whose representation axis this plan re-bases. The
arrangement half already spun out to
[graph_signals_layer_plan](2026-06-22_graph_signals_layer_plan.md) (Decision 7). This plan
collects the still-open representation follow-ons so they are not orphaned.
**Code**: `crates/orrery/orrery` (the representation + sprite + hull maps, `node_collider`),
`crates/orrery/gyre` (`NodeCollider`, the body-spawn material params), `crates/meerkat`
(`window_view.rs` face pick, `swatch.rs` + `input.rs` the shape editor).

## Why this plan exists

Mark's reframe (2026-06-23): a node's **body shape** is meant to be a way to give it real
physical character, and a sprite should not be the only way to define it. Today a sprite is
load-bearing for two unrelated things at once, and that coupling is the thing to break. The
terminology settles as **tile** = a standard node, **shape** = a node whose body has been
customized; and structurally that becomes two orthogonal axes, **Body** and **Face**.

A **card** is not a node. A card is a preview or config surface scoped to a node (a snapshot of
its content, or a facet of its settings). This plan does not touch cards; it is about the node's
own body and the texture on its face. (The node-vs-card line was already sharpened in the
predecessor plan; restated here so the vocabulary holds.)

## Current state (code-verified 2026-06-23)

The data model already keeps texture and shape in separate maps, but two sites collapse them
into one mutually-exclusive `Representation` choice:

- **`Representation`** ([orrery types.rs](../../../crates/orrery/orrery/src/types.rs)) is one axis:
  `Tile` (favicon + caption), `Shape` (bare content-typed face, no texture), `Sprite` (an
  imported image fills the face). `NodeShape` (Square / Rounded / Circle) is a separate
  content-type silhouette axis.
- **Face pick** ([window_view.rs](../../../crates/meerkat/src/window_view.rs) `gnode_view`):
  `match representation { Sprite => sprite, Tile => favicon, Shape => None }`. A sprite
  *categorically replaces* the favicon. There is no way to have a custom-bodied node that still
  shows a favicon, nor a sprite face on the default body.
- **Collider pick** ([orrery lib.rs](../../../crates/orrery/orrery/src/lib.rs) `node_collider`):
  a custom hull is consulted **only when the representation is `Sprite`**; otherwise the collider
  is the content-type silhouette primitive. So a tailored body (a hull) is reachable only by
  importing a sprite. This is the coupling Mark named: "only sprites have meaningfully tailored
  hulls."
- **Physical material is near-fixed.** Node bodies spawn with `restitution 0.0`, `friction 0.0`,
  `density 0.001` (mass approx 1.0), `angular_damping 4.0`; only `linear_damping` is per-node
  tunable (`set_linear_damping` re-applies live). The machinery to set per-body restitution /
  friction already exists and is used by the scene-body path, just not exposed for a node's own
  body.

So a sprite currently owns both the face *and* the only path to a tailored body + collider, and
material character is not authored at all.

## The model: Body × Face, with tile/shape as body presets

Split the single representation axis into the two layers the data already implies.

- **Body** = the node's physical form: its collider geometry (a content-type silhouette
  *primitive*, or a custom *hull* authored directly) plus its material parameters (mass,
  restitution, friction, damping). Independent of any texture.
- **Face** = what is painted on the body: nothing, the favicon, or a sprite image. Independent of
  the body.

`tile` and `shape` then fall out as presets over the Body axis, not exclusive forms:

- **tile** = the standard node: a content-type silhouette body + the favicon face. The default.
- **shape** = a customized node: a directly-authored body (hull + material), with whatever face
  the user wants (favicon, sprite, or none).

A sprite becomes *one way to seed a shape* (trace its alpha into the hull, optionally use it as
the face), not the only way and not bundled with the face choice. The shape editor (the swatch)
is where a body is authored, sprite or no sprite.

## Phases

Done-conditions, not dates. B0 is the structural unlock; B1 redefines the taxonomy on top of it;
B2 + B3 give bodies real character and a real editor; B4 brings the remaining forms to parity.

### B0 — Decouple the hull from the sprite

Add a per-node body store (`node_colliders` / `node_hulls` + `set_node_collider`) consulted by
`node_collider` *regardless of representation*, falling back to the content-type silhouette.
Sprite import keeps tracing the alpha but now only *seeds* this shared body. Make the Face pick
independent of the Body (favicon by default, sprite as an override, none), so a custom-bodied node
can still show a favicon and a sprite face can sit on the default silhouette.

Done when a node's body shape and its face texture are set independently: a custom hull holds with
a favicon face, a sprite face holds on a default body, and sprite-import seeds but no longer owns
the body.

### B1 — tile/shape as body presets (the taxonomy)

Re-base `Representation` onto the Body × Face model: tile = standard body + favicon, shape =
customized body + any face. The existing `Sprite` nodes migrate cleanly (body = the traced hull,
face = the sprite image). The user picker (the context-menu Form/Face entries, and the object-card
widgets owned by [object_card_plan](2026-06-21_object_card_plan.md)) sets Body and Face as two
choices, not one.

Done when tile and shape are body presets, Body and Face are set independently in the picker, and
the migration leaves existing sprite nodes unchanged in appearance.

### B2 — Per-node material properties

Add per-node restitution / friction / mass(density) / damping overrides on the orrery, pushed to
gyre via setters mirroring `set_linear_damping` and `set_node_tangibility` (the scene-body path
already sets these per body). Default-preserving: the current constants are the defaults, so an
unconfigured graph is unchanged. Persist via the cartography sidecar, where sizes / sprites /
hulls already ride.

Done when a node can be made heavy / bouncy / slippery, the change is live and persists across
reload, and it composes with the [physics_scenes_and_tangibility_plan](2026-06-22_physics_scenes_and_tangibility_plan.md)
tangibility lever (a tangible, custom-material node interacts physically with a scene).

### B3 — The generalized shape editor (the swatch as body designer)

Grow the swatch from "drag a sprite-traced hull's vertices" (Stage B, shipped) into a body
designer: add / remove / move vertices, snap to primitives (ball / box / rounded / polygon), set
the material parameters (sliders), with an optional sprite underlay as a tracing aid. This is the
[gloss_navigator_design](../design/2026-06-07_gloss_navigator_design.md) §2b swatch editing its
scoped element, where the element is now the node's body. It surfaces in the `node:<id>` facet
pane ([settings_lane_consolidation_plan](../../archive_docs/2026-07-13_superseded_plans/2026-06-21_settings_lane_consolidation_plan.md)) and
compactly as object-card widgets.

Done when a user can author a node's hull from scratch (not only by tracing a sprite), pick a
primitive, and set its material, all in the editor, with the collider and physics updating live.
Note (carried from the swatch review): the stored hull is the raw edited polygon while parry
reconvexifies for the collider, so a concave edit diverges visually from the physics body; a
convexity constraint or a compound/decomposed collider (the probe's concave tier) is the
fidelity step here.

### B4 — Representation-form parity: in-scene rendering + the interactive / scripted forms

This is mostly *wiring already-built substrate into the form hook*, not new mechanism. The
substrate owners are named; this plan owns the Representation variant + dispatch.

- **In-scene sprites + custom bodies.** Today sprites render on the focused orrery's cards only;
  secondary panes still draw the tile form ([orrery frame.rs](../../../crates/orrery/orrery/src/frame.rs)
  renders favicon RGBA, not sprite data). Bring sprite / custom-body rendering to the in-scene
  path so secondary panes match.
- **The interactive node-body form (a live surface as a body).** Re-verified 2026-06-23: the live
  WebView case is **built, not blocked** — the scry path already does off-window WebView2 capture,
  wgpu import, composite-under-chrome, and mouse / keyboard / IME forwarding by API, shipping in
  pelt tiles. Putting it on a node body is a Representation variant dispatching to that path with
  the node's rect. The compositing + input substrate is owned by
  [native_surface_compositing_plan](../../archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md). The
  canvas / arbitrary-producer-behind-an-`<external-texture>` subcase is the one genuine gap (the
  texture-local input bridge), owned by [tearout_composability_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md)
  C2.
- **The decorative scripted form.** A custom-drawn / data-driven face is substrate-available now
  via DOM (the swatch proves node-scoped interactive chrome-DOM). The *behavioral* scripted form
  (forces / rules) stays field-regions territory; rhai survives narrowly as the field-rule
  language ([scriptable_field_regions_plan](2026-06-13_scriptable_field_regions_plan.md)), so the
  predecessor plan's "rhai substrate" reference was accurate, not stale.

Done when sprites and custom bodies render on secondary panes; a live-surface form is selectable
and rides the scry path; the decorative scripted face is wired; and each blocked subcase is
explicitly attributed to its owner.

## Collected follow-ons (so they are not orphaned)

Open representation threads, each with its home, so closing the predecessor plan loses nothing:

- **Styling lens (Decision 3).** One styling lens over node shape + **edge style + field style**
  (the NODE_SHEET pattern widened), not three hardcoded paths. The node-shape half exists; the
  edge / field extension is tracked here as the node-appearance styling concern. Relates to but is
  not owned by the theme system ([seed_palette_theme_system_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-22_seed_palette_theme_system_plan.md),
  which owns color) or the command registry.
- **Label density (Decision 4).** Off / terse / full as a per-scene setting, ellipsized to the
  face width at the current zoom. A node-appearance setting; surfaces via the settings-lane
  `pelt/orrery` page.
- **Sprite blob externalization.** Sprite + hull data-URIs ride the cartography sidecar inline;
  fine for a handful, but externalizing the blobs (a blob store keyed by hash) is the scaling
  step. Noted optimization, not yet owned.
- **Sprite / body federation identity (probe Q5 remainder).** Whether a node's sprite + hull +
  material data serializes and travels with the node across federation sync (the moot / moothold
  tier), not just the local cartography sidecar. Defers to the moot-tier federation design; tracked
  so it is not lost.
- **Scene-wide representation defaults (Decision 6).** The scene-wide form-by-content-type mapping,
  default arrangement, and edge / field styling, distinct from the per-node overrides. The "scene
  pane" of Decision 6 is the settings-lane global page; owned by
  [settings_lane_consolidation_plan](../../archive_docs/2026-07-13_superseded_plans/2026-06-21_settings_lane_consolidation_plan.md) (`pelt` /
  `pelt/orrery`), with per-node overrides on the `node:<id>` provider.
- **Facets-panel structure + scope (probe Q4).** Which per-node facets the deep editor exposes and
  in what order, and which scattered context-menu toggles it consolidates (Form / Face / size /
  material / tags / engine pin / relations). The panel is owned by
  [settings_lane_consolidation_plan](../../archive_docs/2026-07-13_superseded_plans/2026-06-21_settings_lane_consolidation_plan.md)'s `node:<id>`
  provider; the compact in-canvas form is [object_card_plan](2026-06-21_object_card_plan.md). This
  plan supplies the Body / Face / material setters those surfaces bind.
- **Arrangement surface + persistence (Decision 7, P3).** Per-scene arrangement *persistence* is
  already shipped (the picker writes `ViewIntent.strategy` + `save_session`, restored on boot /
  switch); the remaining per-scene picker UI consolidation rides the settings-lane scene page, and
  the signal derivation behind analytic arrangements rides
  [graph_signals_layer_plan](2026-06-22_graph_signals_layer_plan.md). Noted so the arrangement-axis
  handoff is explicit, not silently dropped.
- **Edge-trim on routed paths (P0 residual).** Straight edges already trim to the node face
  (shipped 2026-06-21); routed-path edges (explicit geometry) still attach at the collider center.
  Trimming them to the face endpoints is a cosmetic refinement that follows the body's outline, so
  it sits with this axis. Low priority.
- **Dormant analytic arrangements + signal-driven encodings** (centrality to size, community to a
  ring/halo) are owned by [graph_signals_layer_plan](2026-06-22_graph_signals_layer_plan.md);
  cross-ref only.
- **The per-object widget surface** (Body / Face / material widgets in the in-canvas card) is
  owned by [object_card_plan](2026-06-21_object_card_plan.md); this plan supplies the underlying
  setters it binds.
- **The `node:<id>` facet pane** (the deep editor's home) is owned by
  [settings_lane_consolidation_plan](../../archive_docs/2026-07-13_superseded_plans/2026-06-21_settings_lane_consolidation_plan.md).

This plan is the **represent** layer of the
[interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md),
succeeding the predecessor plan's representation half. It resolves the
[node_editor_customization_probe](../research/2026-06-21_node_editor_customization_probe.md) open
questions: Q6 (representation relationship) = **orthogonal**, the Body × Face split; Q3 (collider
fidelity) = a directly-authored hull, convex now, concave/compound as the B3 fidelity step; Q7
(scripted / live forms) = unwired, not blocked, attributed to their substrate owners above.

## Findings (code-verified 2026-06-23, via a 4-reader workflow + targeted reads)

- **Texture and body-shape are already separable in data, coupled only in two pick sites.**
  `node_sprites`, `node_sprite_hulls`, and `node_shape` are independent maps; the conflation is
  the `gnode_view` face match and the `node_collider` "Sprite gate" on the hull (above). So
  the decoupling (B0) is a small structural change, not a data-model rework.
- **The collider lowering is production-ready for authored hulls.** `gyre::NodeCollider`
  (Ball / Square / RoundedSquare / Hull) lowers a hull via parry `convex_hull` with a `< 3`-point
  guard and a ball fallback; no reconvexification logic beyond the degenerate guard. An authored
  hull (B3) reuses this directly, the same path the sprite-traced hull already uses.
- **Per-body material setters already exist on the scene-body path.** gyre spawns node bodies with
  fixed restitution / friction / density and only `linear_damping` per-node; the scene-body path
  sets restitution / friction per body, and `set_node_tangibility` re-masks a live collider. So
  B2's per-node setters mirror an existing shape (`set_linear_damping` / `set_node_tangibility`),
  not new machinery.
- **The interactive node-body form is built, not blocked.** Scry does off-window WebView2 capture
  -> wgpu import -> `compose_external_texture` under chrome, with mouse / keyboard / IME forwarded
  by API, verified live in pelt tiles ([scrying_host.rs](../../../crates/meerkat/src/scrying_host.rs),
  [render.rs](../../../crates/meerkat/src/render.rs)). The `<external-texture>` element composites
  but does not yet forward input (the one real gap, tearout C2). The swatch demonstrates an
  interactive node-scoped chrome-DOM surface today.
- **Scripting substrate.** Rhai was dropped as a general first-party placement (2026-06-10,
  scripting map = Rust + JS), but is retained narrowly for knot-block eval and field-region rules
  ([aether rhai_bindings.rs](../../../crates/orrery/aether/src/rhai_bindings.rs)). A decorative
  scripted node face needs no rhai (DOM substrate); a behavioral one is field-regions.

## Progress

- 2026-06-23: **Plan written (with Mark).** Spun out of the node-representation plan's
  representation half on Mark's Body × Face / tile-shape reframe, after a ground-truth workflow
  (4 readers + a re-run) corrected the predecessor plan's "P2-interactive / scripted are blocked"
  framing: the live-surface form is built-and-unwired (scry path), the decorative scripted form is
  DOM-substrate-available, and only the canvas external-texture-input bridge is genuinely unbuilt
  (tearout C2). Collected the open representation follow-ons (styling lens, label density, in-scene
  sprites, blob externalization) and attributed the cross-plan ones (arrangements -> graph_signals,
  widgets -> object_card, facet pane -> settings_lane, compositing -> native_surface_compositing,
  input bridge -> tearout, behavioral scripting -> field_regions, scene physics -> physics_scenes).
  Resolves the node-editor probe's Q3 / Q6 / Q7. Predecessor plan marked superseded (kept in place,
  since active siblings cite it) in the same pass.
- 2026-07-09: **Favicon face inset landed (status-communication design space, with Mark).**
  Turnstone's favicon-on-node slice surfaced that the cover-fit `Face::Favicon` quad hid the node
  accent behind the icon — activation state / selection amber survived only at the icon's corner
  cutouts, breaking the representations-carry-node-identity read. Mark's call: the icon now insets
  within the face (`FAVICON_INSET` 0.72 in orrery's lib.rs, a settings-candidate knob) so the
  accent reads as a frame around it (receipt:
  `testing/turnstone/images/2026-07-09_favicon_inset_frame.png`). Cover-fit stays available as
  `Face::Sprite` (the "alive graph" form is deliberately full-bleed). Alternatives tracked, not
  chosen: a state-colored caption chip (loud; long titles become big colored bars), a hover ring
  (state on demand only — loses the at-a-glance constellation read the status-cluster glyphs and
  gloss dots lean on), and a status card (a summonable detail layer — composes with the inset
  rather than replacing it; belongs to the interaction-model spine if wanted). Revisit when the
  styling-lens follow-on lands, where per-state face treatments become lens material.
