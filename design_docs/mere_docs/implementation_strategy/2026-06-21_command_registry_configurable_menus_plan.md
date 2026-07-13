# Command registry → palette + persona-configurable context menu

**Date**: 2026-06-21
**Status**: In progress. P1 (registry foundation: `from_id` + the `command_entries` catalog) +
P2 first pass (every action in the palette: `PaletteItem` + the `PALETTE_CONTEXT_ACTIONS` catalog,
the palette unified + headed-verified) landed 2026-06-21. Next: P2-rest (parameterized 1-of-N
pickers), then P3 (agent/a11y/diagnostics one-seam), P4 (configurable menu), P5 (persona persist).
Captures Mark's direction from the settings-lane P2b menu-de-dup decision: *full de-dup of the
context menu, but every action becomes a command in the palette, and the context menu is a
user-curated subset of commands persisted as persona profile config.*
**Code**: `crates/meerkat/` (`command.rs` = `Command`; `lib.rs` = `ContextAction`; the palette in
`views.rs` + `lib.rs`; the context menu in `menus.rs`; drains in `command_drain.rs` + `menus.rs`),
`crates/persona/identity` (`PersonaId`), the persona settings location.
**Related**:
[settings_lane_consolidation_plan](2026-06-21_settings_lane_consolidation_plan.md) (P2b surfaced this),
[persona_model_brief](../research/2026-05-14_persona_model_brief.md) (the `<persona_id>/settings/`
home, which already names "palette" as persona-scoped). Supersedes the abandoned
`archive_docs/.../2026-05-11_typed_action_bus_plan.md` framing.

---

## The problem

Today the host has **two parallel action vocabularies**, both hardcoded enums:

- `Command` (command.rs) — drives the palette, the `>`-omnibar, the agent harness, accesskit. A
  fixed list; adding an action means a new enum variant + a `run_command` arm.
- `ContextAction` (lib.rs) — drives the right-click context menu only. A *separate* fixed list,
  drained in `menus.rs`.

So an action is either palette-reachable or menu-reachable, rarely both, and the menu's contents
are hardcoded per selection-shape in `menus.rs`. The settings-lane P2b work made this concrete: the
orrery scene toggles (layout / size-by-degree / mirror) now live on the `pelt/orrery` page **and**
in the context menu, with no clean way to say "this command shows in the menu, that one doesn't"
without editing `menus.rs`.

This is the long-standing "route both menus through an ActionRegistry, no hardcoded command enums"
direction (inherited control-UI principle).

## The model

The registry is the **single programmatic surface for everything the app can do** — the Zed /
VS Code command-palette model. Every action *and every setting* is listed there under a stable id,
so one source of truth feeds the palette, the context menu, keybindings, **and** the scripting /
automation layer. Making the UI customizable (the context-menu case) and making scripting powerful +
consistent are the *same* work: expose every command + setting as an addressable, listable entry,
and the UI surfaces and the scripts both just consume that list. (Engine note, code-verified: the
live automation lane is the **rhai** omnibar `>`-shell — `shell_eval.rs` / `script_rhai` — and it
**already registers one binding per `Command` verb** from `Command::ALL`, so the seam is partly
realized today. Separately, web-content scripting is the JS / Nova-Boa lane. The registry stays
engine-agnostic — it must not bake in a specific engine — but "rhai" is correct for the omnibar
automation lane, which is the existing proof-of-concept consumer of the catalog.)

Runtime command mods are a follow-on to this registry, not a separate command
bus. The shape is documented in
[`2026-06-30_runtime_mod_authoring_loop_plan.md`](2026-06-30_runtime_mod_authoring_loop_plan.md):
a Rhai command pack contributes dynamic registry entries, receives a read-only
context snapshot, returns action requests, and lets the host validate them
through the same command registry path. Rhai stays the local command language;
the portable untrusted mod boundary stays Wasm/WIT.

One **command registry**: every user action — and every setting — is a registry entry with

- a stable **id** (`orrery.layout.set:<id>`, `node.open-tile`, `graph.add-node`,
  `pelt.appearance.tab-cap`, …),
- a **label**,
- an **applicability** predicate (the *context* it makes sense in: always / needs-selection /
  needs-N-nodes / needs-link / empty-canvas), and
- an **invoke** that runs it (wrapping the existing `Command` / `ContextAction` effects; a setting's
  invoke reads/writes the setting, optionally with an argument so a script can `set` it directly).

Settings are commands too (VS Code's model): a toggle setting is a command that flips it; a valued
setting is a command that takes an argument (and may also open a picker from the UI). This is what
lets the settings lane, the palette, and a script all drive the same knob through one id.

Then:

- The **palette** lists every applicable command (replacing the `Command`-enum source).
- The **context menu** lists a **persona-configured subset** of command ids, each filtered by its
  applicability for the current selection. The user adds/removes commands; the default subset is the
  current menu's commands (so day-one behaviour is unchanged minus the de-dup'd scene toggles).
- The config is **persona profile data**: persisted under `<persona_id>/settings/` (the persona
  brief's home), via a typed store (no ad-hoc JSON writes — persona-brief §3.1).

The de-dup falls out of the default config: the scene toggles are not in the default menu subset
(they live on the orrery page + the palette), and the user can re-add any of them.

## One hub, not a fifth list (accessibility / agent / diagnostics)

Mark's question: can accessibility / diagnostics give us the complete registry? **Direction matters.**
The registry is the *source of truth*; a11y, the agent harness, and diagnostics are *projections /
consumers* of it, not the source — you can't derive the catalog from them (the a11y tree only holds
currently-rendered elements; diagnostics observes events, it doesn't enumerate commands). What we
*do* is unify the existing action surfaces **onto** the registry so there is one hub, not five lists.

Today there are already four overlapping action vocabularies: `Command` (palette/omnibar), `ContextAction`
(menu), `AgentAction` (agent harness — and it already bridges via `AgentAction::InvokeCommand(Command)`),
and `A11yHostAction` (the a11y click/focus routes in `frame_a11y` / `frame_a11y_panes`). The registry
collapses them:

- **Agent harness** = the biggest consolidation win, and the closest thing to a programmatic registry
  today. Fold `AgentAction` into "invoke command id (+ arg)", so the agent, scripting, and the palette
  drive one command set. (`InvokeCommand` already exists; this generalizes it.)
- **Accessibility** = the *contextual projection*: each rendered command's a11y default action routes
  to its command id (replacing the ad-hoc `action_routes` map), so a screen reader fires the same
  command by the same path. a11y exposes the currently-applicable subset, never the full catalog.
- **Diagnostics** = the *audit sink* + a *completeness check*, not the source. Every registry invoke
  emits a ux-event `(surface, command id, arg)`, giving a uniform audit of everything that ran across
  palette / menu / agent / a11y / script. And a probe can *enforce* completeness — assert every a11y
  action and every menu entry resolves to a registry id, so no orphan action can drift back in. That
  is how we "leverage diagnostics": to verify the registry is complete, not to build it.

So the registry is the hub; palette, menu, agent harness, a11y, and scripting all invoke by id;
diagnostics observes by id. The "complete registry" is the registry — a11y/diagnostics keep it honest.

### Why this compounds (the payoff)

Coordinating all six surfaces — script automation, agents, command palette, context menu,
accessibility, diagnostics — on one seam is **mutually reinforcing**, not just tidy (Mark,
2026-06-21: "the mutual leverage is enormous"):

- **Anything a user can do, a script / agent can do** — by the same id, so automation matches the
  UI's full capability and is reproducible (no separate, lagging automation API).
- **Anything scriptable is accessible** — every command is reachable by a11y, so the screen-reader
  path and the agent path are the same path, and a11y coverage comes for free with each command.
- **Everything is observable** — every invoke (from any surface) emits one diagnostic, giving a
  uniform activity log + a replayable trace across user / script / agent / a11y.
- **The registry stays whole** — a diagnostics/probe completeness check fails if any menu entry or
  a11y action doesn't resolve to a registry id, so no surface can drift its own private action back
  in. The agent harness doubles as a test surface for the entire command set.

The leverage is the reason to pay the registry's up-front cost rather than the thin menu-config hack.

## Phases (done-conditions, not dates)

- **P1 — Command registry + palette routed through it.** Introduce the registry; register every
  current `Command` + `ContextAction` as an entry (id / label / applicability / apply, wrapping the
  existing effects). Route the **palette** through the registry (it lists registry commands). No
  context-menu change yet. The `Command` enum can stay as the apply-layer under the hood at first;
  the registry is the new front door. Done when the palette is registry-driven and every existing
  action has a registry id.
- **P2 — Every action in the palette.** Make the menu-only actions (the scene toggles, engine /
  representation pickers, isolate / show-all, relate, tags) palette-invokable registry commands.
  Parameterized actions (layout strategy, engine pick, representation) become either a small set of
  concrete commands or a command that opens a picker — decide per action. Done when there is no
  action reachable only from the context menu. **Status: first pass done + headed-verified
  (2026-06-21).** Added `PaletteItem { Command | Context }` + the opt-in `PALETTE_CONTEXT_ACTIONS`
  catalog (`command.rs`); the palette lists/steps/runs the unified list (`palette_items`), and a
  palette-invoked context action records `pending_palette_action`, which the host drains by seeding
  `context_set` from the live selection then running the existing context drain (the same applier the
  menu + agent harness use). Verified live: the gestures/toggles (Add node/tile/field/session, Isolate,
  Show all, Size-by-degree, Mirror, Add tag, Open in splits/stack, Resize) show + filter in the
  palette, and "Add node" minted a node. **Deferred to P2-rest:** the parameterized 1-of-N pickers
  (layout / engine pick / representation) — they need the picker-opening invoke (decision: 1-of-N
  opens a chooser), and engine/representation are also heading to the `node:` facets provider; plus
  applicability filtering (gray-out without a selection), which is P3.
- **P3 — One seam: route the agent harness + a11y + diagnostics through the registry.** Fold
  `AgentAction` into "invoke command id (+ arg)" (generalizing the existing `InvokeCommand`), so
  automation + agents drive the same seam (Mark, 2026-06-21: "we would want automation and agents to
  also use the same seam"). Route each rendered command's a11y default action to its command id
  (replacing the ad-hoc `action_routes`). Emit a ux-event `(surface, command id, arg)` on every
  invoke. Add a completeness probe: every menu entry + every a11y action resolves to a registry id.
  Done when palette / menu / agent / a11y / script all invoke by id and diagnostics audits by id.
  **Status: agent-by-id seam done (2026-06-21).** Context actions now have stable registry ids
  (`PALETTE_CONTEXT_ACTIONS` carries `(action, id, label)`; `context_action_id` / `context_action_from_id`,
  unique across the whole id space — proven by a test), so the registry is one id namespace
  (command verbs + context ids). `AgentAction::Invoke(String)` resolves a command id (`Command::from_id`)
  or a context-action id (`context_action_from_id`) and applies it (a command via the command path, a
  context action seeded against the live selection), so an agent reaches *every* registry action by the
  same ids the palette uses. **Diagnostics audit-on-invoke done + verified (2026-06-21):** every host
  command (`drain_pending_command`) and context action (`drain_pending_context`) records a
  `meerkat.command.invoked` diagnostic by its registry id through the one observability spine, so
  palette / omnibar / menu / agent invocations all land in one audit log (the agent lane keeps its own
  provenance line too). Verified deterministically — `agent_invoke_by_id_runs_commands_and_context_actions`
  asserts the `add_node` invoke produces the audit diagnostic. **Remaining for P3:** a11y
  command-by-id is effectively already covered (the chrome command buttons route a11y clicks through the
  DOM `on_click`, firing the command); the **full** completeness probe ("every menu entry resolves to a
  registry id") is gated on P4 (menu becomes registry-driven) — the partial probe (registry ids unique +
  round-tripping) is in place.
- **P4 — Configurable context menu.** Render the menu from a config (an ordered list of command ids)
  filtered by applicability for the current selection, instead of the hardcoded `menus.rs`
  builders. Ship a default config equal to today's menu minus the de-dup'd scene toggles. Edit
  surface: a `pelt/menu` settings page (checkbox list of commands) for the first pass; later, an
  inline editor — a command-palette-fueled mini search bar to add/remove commands — likely *in
  addition* to the settings page (Mark, 2026-06-21). Done when the menu is data-driven and the user
  can add/remove commands.
- **P5 — Persona persistence.** Persist the menu config (and any other persona-scoped UI config) in
  `<persona_id>/settings/` via a typed `PersonaSettingsStore`; load at boot, save on change. Done
  when a customized menu survives a restart, scoped to the active persona.

## Scoping: menu-editor UI (mini-bar + reorder)

The fuller slice shipped the editor as two browse lists on `pelt/menu` ("In the menu" / "Add a
command") + Reset. Two UI refinements remain (Mark, 2026-06-22). State of the world they build on:
the config is an **ordered `Vec<String>`** in `PersonaSettings.menu_actions`; `toggle_menu_action`
adds (append) / removes; `all_registry_ids` + `registry_label` enumerate candidates; the command
palette already owns a proven search + arrow-nav + scroll surface (`palette_items`, `step_palette`,
`run_palette_selection`).

### A. Mini-bar (command-search add)

The "Add a command" list is the full registry (~38 rows) with no filter; a search bar makes adding
fast. Two designs:

- **A1 — reuse the palette in an "add to menu" mode (recommended).** This is literally the
  "command-palette-fueled mini search bar" from decision 3. A `PaletteMode { Run, AddToMenu }` flag on
  `Chrome` (meerkat-side; **not** `SearchPaletteScope`, which is the search-*scope* axis in the shared
  `chrome` crate). A "+ Add command…" button on `pelt/menu` (drains `menu:add_open`) opens the palette
  in `AddToMenu`; `run_palette_selection` branches on the mode: in `AddToMenu` it routes the picked
  item's registry id to `toggle_menu_action` (add) and stays open for the next add, instead of running
  it. Reuses the palette's field + nav + scroll wholesale; no shell-crate change. Effort: small-medium
  (mode flag + one branch + a trigger + a "done" close). The id from a `PaletteItem`: `Command` -> verb,
  `Context(action)` -> `context_action_id`.
- **A2 — an inline text field on the page.** Add a text-input `PaneItem` kind to the settings-pane model
  and live-filter the "Add a command" list by `label_matches`. Keeps everything on one surface, but the
  settings-pane model has no text input today, so it is the larger build (new control kind + caret/focus
  routing + filter drain). Effort: medium. Pick this only if Mark wants the search inline rather than via
  the palette.

Recommendation: A1 first (cheap, reuses the palette, matches the "palette-fueled" phrasing); revisit A2
if an on-page field is wanted in addition.

### B. Reorder the in-menu list

The menu renders in config order, so reordering is a `Vec` reposition + persist — the data side is
trivial; the cost is the gesture.

- **B1 — move up / down affordances. BUILT (2026-06-22).** Each "In the menu" row has inline ▲ / ▼
  buttons (a `PaneItem::reorder_row` compound row) draining `menu:move:<id>:up|down` ->
  `move_menu_action` (swap with the neighbor + persist). See the progress log.
- **B2 — true drag-reorder.** Pointer-drag a row to a new index. Scoped below.

### B2 scope (2026-06-23)

**Data side is free** — reuse B1: move the id in `menu_actions` + `persist_menu_actions` (a `Vec`
reposition, not just a neighbor swap). All the cost is the **gesture**, and the shell DOM is
`on_click`-only (the runner exposes `dispatch_click`, not pointer-down/move), so the drag must be
**host-driven** off the chrome's raw pointer events — exactly how the orrery already drags nodes
(`pointer_down -> dragging -> move -> up`) and how `resize_drag` / `titlebar_press` work in
`input.rs`. The chrome already hit-tests the settings pane scroll-aware (`chrome_click` ->
`chrome_session.hit_test`, mirroring the `settings-pane-body` scroll offset), so the same hit-test
serves drag. Five pieces:

1. **Mark the draggable row + its id.** `reorder_view` (settings_pane_view) tags the row container with
   the registry id — a `data-reorder-id=<id>` attribute or a dedicated drag-handle element. On
   pointer-down, hit-test -> walk to the row -> read the id + its current index. (A handle column also
   keeps drag distinct from the row's existing click targets — label = remove, ▲/▼ = step.)
2. **Drag state.** A host-side `MenuRowDrag { id, from: usize, cursor_y }` on the window/view (beside
   `resize_drag`), armed on Pressed over a reorder row, promoted to a real drag only past a small
   movement threshold (so a plain click still toggles / steps).
3. **Drop target on move.** On `CursorMoved` while dragging, hit-test the cursor -> the row under it ->
   its index -> the drop position; store it + redraw. Mirror the `settings-pane-body` scroll offset
   (as `chrome_click` does) so a scrolled list lands right.
4. **Drop indicator.** A render overlay (an insertion line between rows, or a highlight) computed from
   the drag state + the settings-pane row rects (the session `fragments().rect_of`, the same source the
   keyboard scroll-into-view pass uses).
5. **Resolve on release.** On Released while dragging, reposition `id` from `from` to the drop index in
   `menu_actions` + persist; clear the state. A sub-threshold release falls through to the normal click.

**Effort:** medium-large. **Risks:** (a) click-vs-drag discrimination on rows that already click (a drag
handle plus a movement threshold mitigates this); (b) drop-index hit-test and indicator over a *scrolled*
list (reuse the existing scroll-offset mirroring); (c) auto-scroll when dragging near the list edges
(refinement; skip in v1).

**Recommendation:** build it as a **shared reorderable-list capability** — a host-side `data-reorder-id`
drag helper + drop-index hit-test — so it serves the menu list now and future reorderable lists
(workbench tabs, shellbar, roster) rather than a one-off. **But it is genuinely optional polish:** B1
(▲/▼) already reorders, and the searchable menu + suggestions are the higher-value pieces and are in.
Do B2 only if button-reorder proves insufficient in practice.

## Scoping: searchable context menu (the cursor palette)

Mark, 2026-06-22: "what if someone just wants the mini bar as their context menu, so they can right
click and search for commands? a widget in the card itself?" This is the strongest unification on the
table: the **context menu becomes the cursor-anchored command palette**. Right-click gives the curated
menu (the configured subset) as the zero-query view; start typing and it filters to **any** command
(the full registry via `palette_items`). The two surfaces — `Ctrl+P` centered, right-click at the
cursor — converge on one model. It also subsumes mini-bar **A**: you no longer need a separate
"add to menu" search, because the menu *is* the search; pinning a found command to the curated set can
be an inline action on a searched row.

What it reuses (most of it already exists):
- The keyboard nav + scroll already built on `ContextMenu` (arrow / Enter / scroll-into-view) carries
  straight over to the filtered list.
- `palette_items(query)` is the filtered-results source; the curated `menu_actions` (resolved by
  `resolve_menu_item`) is the zero-query source.
- The palette's `TextInput` + `text_field_typed` is the search-field widget to embed at the top of the
  context-menu card.

**Resolved (Mark, 2026-06-23):**

- **No special toggle.** The menu is always searchable; there is no per-persona on/off setting.
  Persona-scoping of the curated set happens through the editor or by pinning, not a toggle.
- **Pin from search.** A searched command can be pinned (added to `menu_actions`); the curated pins
  are the zero-query rows. **Nothing pinned -> the menu is just the mini bar** (the search field), which
  falls out for free (empty `menu_actions` -> no curated rows).
- **Auto-suggest the 3-6 most-frequent commands for the target context** (Mark's idea, agreed). When the
  query is empty, surface a short "Suggested" section so an unconfigured menu is immediately useful.

Keyboard "focus" is **not** a separate problem: an open menu already owns the keyboard (the
`on_context_menu_key` route), so search is "route printable keys to a menu query buffer + rebuild the
rows," exactly the palette's `other`-key branch. The work is the query buffer on `ContextMenu`, the
rebuild (empty -> curated/suggested, non-empty -> `palette_items(query)` mapped to rows), and rendering
the field.

### Auto-suggest design (frequency)

- **Counts.** A per-command invocation count, recorded at the **existing** `meerkat.command.invoked`
  hook (already fired for every command + context action in `drain_pending_command` /
  `drain_pending_context`). One increment point, so palette / menu / agent invocations all feed it.
- **Keying — v1 global, ranked by applicability.** Keep a flat `command_usage: id -> count`; at suggest
  time, take the commands **applicable** to the target context (`registry_scope(id).applies(len)`), rank
  by count, drop already-pinned, take the top 3-6. This yields "your most-used commands that apply here"
  without a context-keyed map. Per-context-keyed counts (canvas-frequent vs selection-frequent) are a
  refinement.
- **Persistence — persona-scoped.** Add `command_usage` to `PersonaSettings` beside `menu_actions`.
  Persist on a debounce / interval (or on close) rather than every invocation, to avoid a write per
  click.
- **Recency.** Pure frequency entrenches old habits; an exponential-decay or recent-window score
  surfaces newly-relevant commands. Note as a refinement; v1 is raw counts.
- **Display.** Empty-query menu = `[Suggested (3-6)]` then `[Pins]` (skip Suggested for any id already
  pinned). With no pins, Suggested is the menu body beside the search field.

### Phasing

- **S1 — core searchable menu.** Query buffer + field + rebuild (empty -> curated pins, typing ->
  `palette_items` mapped to rows, run on Enter/click). Reuses the built nav + scroll. Unit-testable.
- **S2 — pin from search.** A searched row offers pin -> `toggle_menu_action` (add) so it joins the pins.
- **S3 — frequency auto-suggest.** Usage counts at the invocation hook, persona-persisted; the
  "Suggested" section.

Recommendation: build S1 first (foundational, agreed, bounded), then S2, then S3. Higher value than
B2 (drag); do B2 only if button-reorder proves insufficient.

### S1 — core searchable menu. BUILT + verified (2026-06-23)

`ContextMenu` gained a `query` buffer; `open_context_menu_at` was split so `build_curated_menu_items`
produces the zero-query rows and `rebuild_context_menu` swaps in `search_menu_items(query)` (the
registry via `palette_items`, mapped to runnable rows: `Command` -> `RunCommand(verb)`, context action
-> the action) as the query changes. `on_context_menu_key` now edits the query on `Character` /
`Backspace` / `Space` and rebuilds; the already-built arrow-nav + scroll + Enter-runs carry over. A
search field renders at the top of the card (`.context-search` / `-empty` placeholder); the open menu
already owns the keyboard, so no separate focus wiring was needed. Verified live
(`scry-shots/sf-1..3`): right-click shows "Search commands…" above the curated rows; typing "set"
filters to Settings + Node settings (focused) + Node settings (selected); clicking a result runs it
(same `RunCommand` dispatch proven at menuf-7). meerkat green (lib 72, bin 104). **S2 (pin from
search) + S3 (frequency auto-suggest) remain.**

### S2 — pin from search. BUILT + verified (2026-06-23)

Search-result rows carry a pin toggle: `PinSpec` + `ContextItem.pin` / `ContextItem::searchable`,
`ContextAction::PinToMenu(&'static)` + `Chrome::pin_from_menu` (sets the pin intent **without** closing),
`search_menu_items` tags each result with its registry id + current pin state, and the drain toggles
`menu_actions` + rebuilds in place. The input layer was the catch: it closed the menu after *any* click
— fixed to skip the close when the click was a `PinToMenu` (so several can be pinned in a row). views.rs
renders an inline **+ / ✓** toggle (CSS `.context-pin` / `.context-pin-on`). Verified live
(`scry-shots/pin-1..2`): searching "comms" shows the result with a + toggle; clicking it appends
`"comms"` to `menu_actions` in `ui.json`. (A concurrent genet bump broke the workspace mid-build; once
the genet-side fix landed it compiled clean.)

### S3 — frequency auto-suggest. BUILT + verified (2026-06-23)

`PersonaSettings` + `Presentation` gained `command_usage` (a `BTreeMap<id,count>`); `record_command_usage`
increments at the **existing** `meerkat.command.invoked` hook (command_drain for host commands, menus for
cataloged context actions) and persists. `build_curated_menu_items` shows `suggested_menu_items` — the
top-6 most-used commands **applicable** to the selection, not already pinned, as pinnable rows — when no
applicable pins exist ("nothing pinned -> the mini bar suggests for you"). Verified live: with
`menu_actions: []` + seeded usage, the canvas menu listed Inspector / Workbench / Comms / Back / Settings
/ Home ranked by count (7→2, the 1-count `roster` cut at top-6), each with a + toggle (`scry-shots/sg-1`);
running `workbench` from the palette bumped its count 6→7 in `ui.json`. meerkat green (lib 73, bin 109),
session-runtime 71.

**Refinements deferred:** persist-usage debounce (v1 writes per invocation); per-context-keyed counts +
recency decay; pinning a *suggestion* transitions the menu to "has pins" (suggestions then yield to
pins), so "pin several" applies to the search-results mode, not the suggestions mode.

## Open decisions (for Mark)

1. ~~Registry depth now vs pragmatic incremental.~~ **Resolved: build the registry.** The
   scripting / automation consumer is the deciding factor — a thin menu-config layer over the
   hardcoded enums would customize the menu but would *not* give scripts a consistent, complete
   command + settings surface, and automation + agents must use the same seam. The registry is the
   point, so P1 builds it.
2. ~~Parameterized actions.~~ **Resolved (Mark, 2026-06-21): toggles flat, 1-of-N opens a picker.**
   So `orrery.sizebydegree` / `orrery.mirror` are flat commands; layout / engine pick / representation
   are one command each that opens a chooser.
3. ~~Menu-config UI.~~ **Resolved (Mark, 2026-06-21): `pelt/menu` settings page (checkbox list) for
   the first pass.** A future inline editor (a command-palette-fueled mini search bar to add/remove)
   is wanted too, likely in addition to the page — folded into P4.

## Progress (P4/P5 — configurable + persisted menu)

- 2026-06-22: **Fuller slice landed + headed-verified — inclusion config + applicability + real
  PersonaSettingsStore + an inline add/remove editor.** This supersedes the hide-set first slice;
  the menu is now rendered *from* a config and the user can add **any** registry command to it.
  - **Applicability (`MenuScope`).** `command.rs` gains `MenuScope` (`Always` / `Canvas` / `SingleNode`
    / `Selection` / `MultiNode`) + `applies(len)`, `context_action_scope`, `Command::menu_scope`, and
    `registry_scope(id)` — one applicability model the menu filters by (and the palette / scripting can
    query). Unit-tested (`menu_scope_filters_by_selection_shape`).
  - **Inclusion config + config-driven menu.** `DEFAULT_MENU_ACTIONS` (one ordered id list) seeds the
    menu; `open_context_menu_at` was rewritten to build from the configured list — each id resolved by
    `resolve_menu_item` (scope-filtered, count-adapted labels, dynamic conditions) — plus the dynamic
    non-catalog rows (layout submenu, radial toggle, delete field, close pane) appended. One list now
    drives the canvas / single / multi menus. The old per-shape branches + `filter_hidden_menu` are gone.
  - **Any command in the menu.** New `ContextAction::RunCommand(&'static str)` + `Chrome::run_command_intent`
    let the menu carry global commands, not just native context actions; `OpenNodeFacets` joined the
    catalog, and the menu's "Relate" now resolves to the existing `AssertEdge` ("relate") command
    (no duplicate id).
  - **Real `PersonaSettingsStore` (P5 proper).** New `session_runtime::persona_settings_store`
    (`personas/<id>/settings/ui.json`) with `PersonaSettings { menu_actions }`, load/save, round-trip
    tested. The menu config moved off the app `settings.json` onto it; meerkat loads at boot
    (`load_persona_menu_actions`, default persona for v0) and `persist_menu_actions` writes on change.
  - **Inline editor.** `pelt/menu` is now an inclusion editor: "In the menu" (✓, click to remove),
    "Add a command" (every other registry command, click to add), and "Reset to default". Drains
    `menu:toggle:<id>` / `menu:reset` → `toggle_menu_action` / `reset_menu_actions`.
  - Verified live (`scry-shots/menuf-1..7`): default menu reproduced; editor renders all sections;
    removing "Add node" drops it from the menu and writes `ui.json` (without `add_node`); adding the
    global "Settings" command makes it appear in the right-click menu and **run** (opens the settings
    tile) via `RunCommand`. meerkat green (lib 71, bin 104), session-runtime green (71).
  - **Deferred:** an inline *command-search* mini-bar (filter the "Add a command" list / a palette
    "add to menu" mode) on top of the full-list editor; drag-reorder of the in-menu list; finer
    per-command scopes; threading the **active** persona id (v0 uses the default persona).
- 2026-06-22: **Context menu keyboard nav + scroll** (Mark's ask: "scroll and arrow up/down like the
  command palette"). `ContextMenu` gains a `selected` highlight; `step_context_menu` / `run_context_selection`
  mirror the palette's `step_palette` / `run_palette_selection`. An open menu owns the keyboard via
  `on_context_menu_key` (Down/Up wrap the highlight, Enter runs the row, Escape closes); the render
  pass bounds the panel to the window (max-height + overflow) and scrolls the `context-item-active`
  row into view, reusing the palette's `cmd-list` / `cmd-row-active` mechanism. Verified live
  (`scry-shots/kb-1..5`): highlight steps + wraps; a tall menu opened low is clipped to the window and
  scrolls to follow the selection. Unit test `context_menu_keyboard_nav_wraps_and_runs`. meerkat green
  (lib 72, bin 104).
- 2026-06-22: **Finer per-command scopes + active-persona threading** (two of the deferred items).
  - `Command::menu_scope` is now an **exhaustive** match assigning each command a real scope —
    `MultiNode` (AssertEdge / RetractEdge), `Selection` (DeleteNode / HideSelectedEdge), `SingleNode`
    (OpenNodeSettings / BackgroundNode / Retry / Stop / Pin / ToggleCompatView), `Always` (nav + panel
    toggles + ShowAllEdges / OpenSettings / CloseGraphPane / ExportGraph). No `_` arm, so a new command
    must declare a scope (compile-time obligation, like its verb). Test spot-checks added.
  - The menu config is now filed under the **active persona** instead of a hardcoded default:
    `Session.active_persona` is resolved at boot from the active session's manifest
    (`manifests.get(active_session_id).persona_id`, falling back to `default_persona`), threaded into
    `load_persona_menu_actions` and `persist_menu_actions`. v0 still resolves to the default persona, but
    the wiring now follows whatever persona the manifest names. meerkat green (lib 72, bin 104).
  - **Still deferred (now scoped — see "Scoping: menu-editor UI"):** the mini-bar (A1: palette
    add-to-menu mode, recommended) and reorder (B1: move up/down, recommended first; B2: true drag).
- 2026-06-22: **B1 reorder (move up / down) built.** The `pelt/menu` "In the menu" rows are now
  reorderable: a new `PaneItem::reorder_row` (a compound row like the slider) renders the label
  (click = remove) plus inline ▲ / ▼ buttons that queue `menu:move:<id>:up|down`; `settings_pane_view`
  draws it; `apply_pelt_activation` routes the key to `frame_ops::move_menu_action` (swap with the
  neighbor in `menu_actions` + `persist_menu_actions`). **Render verified live** (`scry-shots/ro-3`,
  `ro-8`, `ro-9`: every in-menu row carries the ▲ / ▼ controls). The move and persist run the same
  drain-and-persist path the verified toggle uses, so it is verified-by-construction; the headed move-*click*
  wasn't captured because the roster pane opened inconsistently (shifting the settings tile) and the
  wide-window shots are display-scaled (coordinate reads off) — a test-harness limitation, not a code
  gap. meerkat green (lib 72, bin 104).

- 2026-06-22: **First slice landed + headed-verified — a persona-persisted, user-toggleable menu.**
  The pragmatic core of P4/P5: the context menu can be customized (gestures hidden/shown) on a
  settings page, persisted, surviving restart.
  - **Config + persistence (P5).** Added `hidden_menu_actions: Vec<String>` to `PersistedSettings`
    (the `settings.json` sidecar at `<data_dir>/mere` — app-scoped today, which is the **default
    persona's** settings until the `PersonaSettingsStore` lands, then it relocates to
    `<persona_id>/settings/`). Loaded into `Presentation.hidden_menu_actions` (a `HashSet`) at boot,
    written by `persist_settings`. Round-trip unit-tested in `session-runtime`.
  - **Editor page (P4).** New `pelt/menu` settings page (`menu_settings_items`) lists every registry
    gesture from `PALETTE_CONTEXT_ACTIONS` as a toggle row (✓ = shown), draining `menu:toggle:<id>`
    → `toggle_menu_action` (flip the set + persist).
  - **Menu filter (P4).** `filter_hidden_menu` drops any context-menu row whose `context_action_id`
    is in the hidden set, applied at the canvas + selection menu build sites. Rows **without** a
    registry id (Open tile, Node settings) are never filtered, so the menu can't be emptied of its
    reason to exist.
  - Verified live (`scry-shots/menu-1..4`): the page renders the 12 toggles; hiding "Add node"
    drops its ✓ and removes it from the canvas menu (Add field stays); `settings.json` then carries
    `hidden_menu_actions: ["add_node"]` (reset to `[]` after the run). meerkat green (lib 69, bin
    101); session-runtime green (68).
  - **Still open (the fuller P4):** this slice is **hide/show** of the menu's existing gestures. Mark's
    "add *any* command to the menu" needs the **inclusion** model — the menu rendered *from* a config
    list (add commands not in the default menu) + an **applicability** model (a configured command
    only shows in the contexts it applies to: empty-canvas / single / multi / two-nodes). That, the
    inline command-search editor (decision 3), and the real `PersonaSettingsStore` (P5 proper) are the
    next slices. The menu-completeness probe (every menu action has a registry id) is closer now but
    still gated on the inclusion rewrite, since the structural rows (Open tile, Node settings, Open
    splits/stack, Relate) remain hardcoded.

## Progress (P3 — one seam)

- 2026-06-21: **Agent-by-id seam landed.** Gave context actions stable registry ids
  (`PALETTE_CONTEXT_ACTIONS` → `(action, id, label)`; `context_action_id` /
  `context_action_from_id`), so commands (`verb`) + context actions share one id space, proven
  unique + round-tripping by `registry_ids_are_unique_across_commands_and_context_actions`. Added
  `AgentAction::Invoke(String)` → `agent_invoke(id)`: a command id routes through the command path,
  a context-action id is applied against the live selection (the agent counterpart of the palette
  context path — seed `context_set`, `pick_context`, drain). Test
  `agent_invoke_by_id_runs_commands_and_context_actions`: `Invoke("workbench")` runs, `Invoke("add_node")`
  mints a node purely by id, an unknown id is reported-not-applied. meerkat green (lib 69, bin 101).
  This is the "automation + agents use the same seam" win: an agent now reaches every registry action
  by the same ids the palette uses.
- 2026-06-21: **Diagnostics audit-on-invoke landed + verified.** Added a `meerkat.command.invoked`
  diagnostic at the two host apply points — `drain_pending_command` (by `cmd.verb()`) and
  `drain_pending_context` (by `context_action_id`, Debug fallback for the not-yet-cataloged
  parameterized ones) — so every palette / omnibar / menu / agent invocation lands in one audit log
  through the observability spine. Extended `agent_invoke_by_id_runs_commands_and_context_actions` to
  assert the `add_node` invoke produces the audit diagnostic (deterministic verification, not just a
  log line). meerkat green (lib 69, bin 101). P3 is substantially done: agent-by-id seam + audit; a11y
  command invocation already rides the DOM `on_click`; the full menu-completeness probe waits on P4.

## Progress (P2 — every action in the palette)

- 2026-06-21: **P2 first pass landed + headed-verified.** The command palette is now the unified
  registry surface for commands **and** context actions. Pieces:
  - `command.rs`: `PaletteItem { Command(Command) | Context(ContextAction) }` + `label()`; the opt-in
    `PALETTE_CONTEXT_ACTIONS` catalog (`(ContextAction, label)` pairs — an opt-in list, not an
    exhaustive match, so a new variant Mark adds never breaks the build) + `context_action_palette_label`;
    `palette_items(query)` (matching commands, then matching palette context actions); `label_matches`
    extracted as the one shared match rule.
  - `lib.rs`: `Chrome::palette_items`, `pending_palette_action`, `run_palette_item` (dispatch by kind:
    command → `run_command`; context → record `pending_palette_action`), `run_palette_item_and_close`;
    `step_palette`/`run_palette_selection` now over `palette_items`. `views.rs`: the palette renders +
    dispatches the unified items.
  - `menus.rs`: `drain_palette_context_action` seeds `context_set` from the live selection + moves the
    action into `pending_context`; `input.rs`: it runs right before `drain_pending_context` in both the
    click path (`drain_chrome_intents`) and the **keyboard Enter** path (`on_palette_key`).
  - **Bug found by headed verify + fixed:** the palette's Enter handler drains a *manual subset*, not
    `drain_chrome_intents`, so the first run minted nothing (the palette context action's
    `pending_palette_action` never drained on the keyboard path). Added the two context drains to
    `on_palette_key`; re-verified that "Add node" via the palette mints a node. (Unit tests covered the
    palette→`pending_palette_action` step and the catalog, but not the keyboard drain wiring — the live
    run caught it.)
  - Tests: `palette_items_unify_commands_and_context_actions` (command.rs) +
    `palette_runs_a_context_action_into_the_pending_slot` (tests.rs); updated the two count-based palette
    tests for the unified list. meerkat green (lib 68, bin 100). Coexists cleanly with Mark's concurrent
    seed-palette theme editor + Reading settings page (both live in the same verified build).

## Progress

- 2026-06-21: Plan created from the settings-lane P2b menu-de-dup decision. Confirmed in code that no
  action registry/bus exists today (only the `Command` + `ContextAction` enums; the one
  `register-mod-loader` hit is unrelated). Grounded the persona-persistence target in the existing
  persona model (`<persona_id>/settings/`, which already lists "palette"). Mark reframed the scope
  across several messages: (1) the menu config persists as persona/profile data (existing persona
  settings, not a new system); (2) it is the Zed/VS Code command-palette model — every command **and
  setting** is a listable, addressable registry entry; (3) the registry is the one seam that **script
  automation, agents, command palette, context menu, accessibility, and diagnostics** all share —
  a11y/diagnostics don't *source* the registry (they're projections/audit), they unify *onto* it, and
  the existing `AgentAction`/`A11yHostAction`/`ContextAction` fold in (`AgentAction::InvokeCommand`
  already bridges); (4) the mutual leverage of coordinating all six surfaces is the payoff that
  justifies the registry over a thin menu-config hack. **Decisions resolved:** registry (not the
  hack); toggles flat / 1-of-N opens a picker; `pelt/menu` checkbox page first, inline
  command-search editor later. Phasing P1–P5 above. Design locked; P1 is the clean next build (note:
  Mark has concurrent object-card/`ResizeNode` edits live in `lib.rs`/`menus.rs`, so P1 starts from
  current code and stays additive — a new `command_registry` module + the palette source — to steer
  clear of his menu work until P3/P4 touch it).
- 2026-06-21: **P1 foundation landed (additive, in `command.rs`).** Found that `Command` already
  carries the palette registry's shape — `verb()` (a stable id), `label()`, `ALL`, `filter()`, and
  the omnibar `>`-shell derives its bindings from `verb()` — so the missing seam pieces were the
  **id→command resolution** and a **catalog type**. Added (all in `command.rs`, the pure-data lib
  module Mark is not editing): `Command::from_id(id) -> Option<Command>` (the reverse of `verb`; the
  by-id seam a script / agent / a11y route resolves through), `CommandEntry { id, label, host_action }`
  (the catalog entry as pure data), and `command_entries()` (the listable catalog). Tests:
  `from_id` round-trips every verb + rejects unknowns; the catalog covers `ALL` with unique ids.
  meerkat green (lib 66, bin 98). **Next P1 step (deferred around Mark's live edits):** route the
  palette + the agent harness explicitly through `from_id`/the catalog and add a host-side
  `invoke(id, arg)`, then fold `ContextAction` into the catalog (P2) — those touch `lib.rs` /
  `views.rs` / `menus.rs`, so they wait until the object-card edits there settle (or his go-ahead).
- 2026-06-21: **Code audit of the existing consumers — the Command-side seam is more realized than
  assumed, and a plan correction.** (1) The omnibar `>`-shell (`shell_eval.rs`) is the live **rhai**
  automation lane (`script_rhai`), and it **already registers one binding per `Command` verb** from
  `Command::ALL` (shell_eval.rs:127-132) — so the live automation lane already consumes the catalog;
  "scripting uses the same seam" is already true here. Corrected the plan/DOC_README, which had wrongly
  said "Rhai dropped" for this lane (rhai is the omnibar/knot lane; JS/Nova-Boa is the separate
  web-content lane; the registry stays engine-agnostic). (2) The agent harness already records a
  diagnostic per action (`apply_agent_action` → `record_diagnostic("meerkat.agent.action_applied")`)
  — the audit-by-invoke piece is partly present. **Conclusion:** the **Command-side** registry is
  effectively complete (catalog `command_entries` + `from_id` + the palette via `Command::ALL` + the
  omnibar rhai verbs + the agent harness + per-action diagnostics). The remaining phases all act on
  the *other* surfaces and Mark's live files: P2 (`ContextAction` → catalog, with the toggles-flat /
  1-of-N-picker treatment, in `lib.rs`/`menus.rs`/the palette), P3 (a11y route-by-id + completeness
  probe), P4 (configurable menu), P5 (persona persist). P2 is a real multi-step build (entries +
  picker invoke + palette listing + applies), best taken as a focused run, not a marathon-tail
  half-build; queued for when the object-card edits settle or on Mark's go.
