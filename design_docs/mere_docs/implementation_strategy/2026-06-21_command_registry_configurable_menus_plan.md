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
  same ids the palette uses. **Remaining for P3:** a11y default-actions → command id (the chrome
  buttons already route clicks via DOM, so this is mainly the orrery/pane a11y `action_routes`); the
  diagnostics audit-on-invoke (a ux-event at the host apply points; the agent harness already emits one);
  and the completeness probe.
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
  by the same ids the palette uses. a11y route-by-id + the diagnostics audit + the completeness probe
  remain for P3.

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
