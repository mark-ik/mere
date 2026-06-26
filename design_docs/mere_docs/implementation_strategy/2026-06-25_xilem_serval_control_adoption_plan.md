# xilem-serval control-set adoption

**Date**: 2026-06-25
**Status**: Planned. From the [serval capability-misuse sweep](2026-06-25_overlay_primitive_adoption_plan.md)
(2026-06-25).
**Owner**: meerkat (consumes xilem-serval `controls`).

## Problem

`xilem-serval` exports a full reusable control set (`button`, `checkbox`, `toggle`, `radio_group`,
`select`, `slider`, `text_field` in `controls.rs` / `radio.rs` / `select.rs` / `slider.rs`). meerkat uses
exactly one of them, `text_field` (correctly, for the omnibar / palette / find / comms fields). Every other
control is hand-built from `div`s + an active CSS class, which also omits the ARIA roles the primitives
stamp. The a11y bridge already ignores stamped ARIA per the grand audit (`grand_audit.md:94`), so the
host hand-rolled rows have no role/state for a screen reader.

Specifics:
- **`button`** — 15 chrome buttons in [views.rs](../../../crates/meerkat/src/views.rs) hand-roll
  `on_click(el("button", label), handler)`, which is verbatim what `xilem_serval::button` wraps
  (`controls.rs:685`, whose own doc says it is "the ergonomic form of" that pattern).
- **`radio_group`** — every single-selection settings picker (node Face, theme Harmony, body/code font,
  orrery Layout, gloss lens, crawl Scope/Depth/Page-cap) is a list of `PaneItem::button` rows with the
  chosen one carrying `app-btn-active` + a check glyph. A radio group, minus `role="radio"`/`aria-checked`.
- **`checkbox`/`toggle`** — engine on/off and orrery/crawl boolean switches are buttons toggling
  `app-btn-active` (the host closures are even named `toggle`). No switch/checkbox role.

## The structural obstacle

meerkat's `PaneItem` model is deliberately **state-decoupled**: a row carries a string activation key the
host drains (`PaneItem::button(class, text, key)`), it does not bind a lensed `&mut RadioGroup` / `&mut
bool`. The xilem-serval primitives flip lensed state. So `radio_group` / `toggle` / `checkbox` do not drop
in without an adapter. This is why these are **underuse, not misuse**. `button` is the exception (it is
stateless, a near drop-in).

## Plan

**P1 — `button` adoption (near drop-in).**
- Replace the 15 `on_click(el("button", ...), h)` sites with `xilem_serval::button(label, h)`, keeping the
  `.attr("class", ...)` on the returned view. Confirm the class-on-button path works; if not, extend
  `button` to take a class or wrap once locally.
- Done: no bare `el("button")` in views.rs; one helper.

**P2 — ARIA stamping on the picker/toggle rows (the bankable a11y win).**
- The immediate value is not the lensing but the role/state the primitives emit. Two routes:
  (a) add `role="radio"` + `aria-checked` (and `role="switch"`/`aria-checked` for toggles) directly to the
  `PaneItem` rows that represent radio groups / switches, via a `PaneItem` variant or an attr pass; or
  (b) a thin adapter that renders `radio_group` / `toggle` driven by the PaneItem's selected index / bool
  while still emitting the host's activation key.
- Coordinate with the a11y bridge so the stamped ARIA is actually surfaced (the grand audit notes it is
  currently dropped).
- Done: settings pickers expose `role="radio"`/`aria-checked`; toggles expose switch semantics; a screen
  reader reads the selection.

**P3 — evaluate full primitive adoption (gated).**
- If a lensed `PaneItem` path emerges (or settings panes move off the key-drain model), adopt
  `radio_group` / `toggle` / `checkbox` directly and delete the hand-rolled class-toggle code.

## Non-goals (verified, do not force-fit)

- **`select`** — no natural consumer. The omnibar suggestions list is a richer autocomplete; the engine/
  face/layout pickers are intentionally context-menu / settings-pane rows, not form selects.
- **`slider`** — meerkat's `PaneItem` segmented slider is a discrete cell-picker with a hue-rainbow mode,
  a genuinely different widget from the continuous drag `slider`. Keep it.

## Progress

- 2026-06-25: Drafted from the capability sweep. Not started.
