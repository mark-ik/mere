# Object Card Plan: a customizable, type-scoped per-object action card

**Date**: 2026-06-21
**Status**: Planning (with Mark), building widget 1.
**Code**: `crates/meerkat/` (the focus-card slot, the context action, the drain),
`crates/orrery/` (the per-object settings the widgets bind to).

A light, in-canvas card scoped to the selected graph primitive, holding a customizable,
type-scoped set of setting and action widgets. An iOS-Control-Center for graph objects.
It renders in the **focus-card slot** (replacing the snapshot preview when summoned), and
is deliberately **not** the context menu: the menu keeps the quick gestures, the card owns
the config and actions.

Sibling / converging docs:

- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md):
  the resize-tier control (its P0 size work) is the card's first widget; the card is where
  per-node representation, size, and physical-characteristic config live.
- [settings_lane_consolidation_plan](2026-06-21_settings_lane_consolidation_plan.md): the
  card is the **compact, in-canvas face of the per-object settings lane** (the `node:<id>`
  provider). One per-object settings model, two presentations.
- [node_editor_customization_probe](../research/2026-06-21_node_editor_customization_probe.md):
  the "node facets menu" that probe scoped. This plan generalizes it past nodes to every
  graph primitive, and gives it a concrete home (the card in the focus slot).

---

## The thesis: one per-object settings model, presented as a light card

A graph object (node, edge, field) carries settings. Today those are reachable only
piecemeal (a context-menu toggle here, a hardcoded default there). The object card makes
them a first-class, scoped, customizable surface:

- **Scoped to the selection.** The card shows the settings for whatever primitive is
  selected, and swaps its contents by that primitive's **type**.
- **A preset per type.** Each primitive type has a default ordered widget set (its preset):
  a node preset might be `[size-tier, representation, engine, color, pin]`; an edge preset
  `[style, weight, direction]`; a field preset `[falloff, rule]`.
- **Customizable.** The preset is user-editable (which widgets a type's card shows, and in
  what order), so the card is "put any object-related setting on it," not a fixed panel.
- **In the focus slot, summoned, persistent.** It renders in the same anchored slot the
  snapshot preview uses, replacing the preview when summoned by a context action. It stays
  open while you work it (step a tier, flip a representation) and dismisses on Esc, a
  click-away, or selecting a different object.

## Convergence: this card unifies three threads we already had

1. **The node facet menu** (the node-editor probe). The card is that menu, generalized to
   every primitive and given a render home.
2. **The settings-lane per-object provider.** The consolidation plan already models per-object
   settings as a lane namespace (`node:<id>`, and later `edge:<id>` / `field:<id>`). The card
   is the **compact, in-canvas face** of that same data; a full settings **page** in a pelt
   tile is the other face. No second data model: a widget reads and writes the object's lane
   settings, and the control keys drain exactly as the apparatus / settings-page keys do.
3. **The focus-card slot.** `compute_focus_card` / `FocusCardKind` / `focus_card_view` already
   place an anchored card over the focused node (snapshot or unvisited). The object card is a
   third `FocusCardKind`, summoned in place of the preview.

## The model

- **Widget**: a single control bound to one object setting. It renders a small control and
  emits an activation key on interaction (the existing `on_click` -> `ShellState` vec -> host
  drain path). The resize widget is `−  ●●●○○  +` bound to the node's size tier; others bind
  to representation, engine, color, pin, edge style, field falloff, and so on.
- **Preset**: an ordered `Vec<Widget>` defaulted per primitive type. The card renders the
  preset for the selected object's type. (First cut: a single hardcoded node preset of one
  widget; the list and the per-type switch come next.)
- **Render**: a `FocusCardKind::ObjectCard` carrying the resolved widget list; `focus_card_view`
  lays the widgets out in the anchored card. Summoned by a context action that sets a
  `sizing/editing` target on the view; cleared on Esc / click-away / select-other.
- **Data binding**: a widget's value comes from the object's settings (the orrery accessors
  today, the `node:<id>` lane provider once that face is shared), and its activation key drives
  the matching setter (the resize widget calls `step_node_size_tier`).
- **Distinct from the context menu**: gestures (open, relate, add, isolate) stay in the menu;
  config and actions live on the card. This is the line the settings consolidation plan already
  drew (deep config to the lane, quick gestures to the menu).

## Phases (done-conditions, not dates)

- **P0 — Widget 1 in a minimal frame.** A context action ("Resize", later "Edit" / "Configure")
  summons an `ObjectCard` into the focus slot for the selected node, replacing the snapshot. The
  card renders an ordered widget list (seeded with the resize-tier stepper `−  ●●●○○  +`); the
  − / + keys drain to `step_node_size_tier`; the notches show `node_size_tier`. Esc / click-away
  / select-other dismisses it. Done when a node's size is steppable from the card and the card
  is the preview's replacement, not a new floating surface. **(Building now.)**
- **P1 — The widget list + more node widgets.** Generalize the seed to a real widget list;
  add representation (tile / shape), engine (the picker), color, and pin as widgets bound to
  the existing per-node setters. Done when the node preset carries several widgets.
- **P2 — Share the settings-lane provider.** Bind the widgets to the `node:<id>` lane provider
  so the card and a full settings page render from one model. Done when the card and the page
  agree by construction.
- **P3 — Type presets.** A preset per primitive type; the card swaps by the selected object's
  type (node / edge / field). Done when selecting an edge shows the edge preset.
- **P4 — Customization.** A surface to edit a type's preset (which widgets, what order). Done
  when a user can add or remove a widget from a type's card.

## Findings (code-verified 2026-06-21)

- **The focus-card slot is the render home.** `compute_focus_card` (render.rs) builds a
  `FocusCard { rect, kind }` anchored at the focused node's screen position via
  `anchored_card_rect`; `focus_card_view` (window_view.rs) renders `Snapshot` (a PNG data-URI
  `<img>`) or `Unvisited`. Adding `ObjectCard` as a third kind reuses the anchoring + the
  paint-over-nodes layering.
- **Widget-1's logic is built + tested.** `orrery::SIZE_TIERS` (`[24,36,56,84,120]`),
  `node_size_tier(key)` (the nearest notch), and `step_node_size_tier(id, delta)` (step + snap
  + clamp, returns the new tier) landed with a unit test. `set_node_size` already drives the
  collider and persists, so a tier step is durable and physical for free.
- **The click-drain pattern.** A chrome control is `on_click(el, move |s, _| s.<vec>.push(key))`;
  the host drains the vec (`take_orrery_card_selects`, the list-pane / settings-pane takes) and
  applies the key. The card's − / + reuse this with a `node_size_steps: Vec<i32>` (or a keyed
  vec) drained to `step_node_size_tier`.
- **The settings lane already has a per-object seam.** The lane's namespaces include `node:<id>`,
  and a settings render arm (settings tiles, `pelt/*` pages) is wired with the same drain keys.
  P2 binds the card to that `node:<id>` provider rather than the orrery accessors directly.

## Progress

- 2026-06-21: **Plan drafted (with Mark).** Came out of the node-rep resize work: a drag handle
  was rejected for an iOS-style discrete model, which Mark then generalized into a customizable,
  type-scoped per-object action card (this doc). The resize tier model (`SIZE_TIERS`,
  `node_size_tier`, `step_node_size_tier`) is built + unit-tested as widget 1's logic. Building
  the minimal frame (P0) next.
