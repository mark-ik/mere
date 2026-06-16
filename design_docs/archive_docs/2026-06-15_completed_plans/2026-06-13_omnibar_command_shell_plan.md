# Omnibar Command Shell Plan

> **ARCHIVED 2026-06-15** (DOC_POLICY §8). The privileged omnibar command shell
> (S0–S4) is **shipped and on-screen verified**: a `>`-expression evals against a
> read-only `ShellContext` snapshot and emits `Command`s drained through the existing
> run path, with ghost-text completion and the `inspect()` query. The **sandboxed
> knot-note `rhai eval` lane** (the other half of the two-tier trust model in this
> doc) is a *different* plan's scope and remains unwired: tracked in
> `knot_evaluation_export_plan` and the in-the-wings audit's Tier B. Original
> location: `mere_docs/implementation_strategy/`.

The omnibar becomes a programmable command line over the existing `Command`
spine: type an address to navigate (today), type a `>`-prefixed expression to run
a script that drives the shell. One language (rhai) on two surfaces (sandboxed
knot notes; privileged omnibar), the same `Command` verbs reached four ways
(palette, agent harness, accesskit actions, and now a typed expression).

This is the "realization of the graphshell premise" framing: scripting that spans
the orrery graph and the content shell from one line, gated by a clean trust
boundary.

## Findings

The four pieces already exist; the shell is assembly on top of them, not new
machinery.

- **The resolution chokepoint** is [`nav::classify`](../../../crates/meerkat/src/nav.rs)
  returning `NavTarget::{Url, Search}`. `nav.rs` already comments the seam
  ("becomes configurable when the omnibar session lands"). The shell adds a third
  arm.
- **The spine** is [`Command`](../../../crates/meerkat/src/command.rs): 20 verbs,
  pure data, each mapped to a `Chrome::run_command` mutation, with
  `is_host_action()` already separating chrome-level history verbs from
  host-drained intents (`ConnectPeer`, node/edge ops, pane toggles).
- **Automation already reuses the spine.** `AgentAction::InvokeCommand(Command)`
  in [`agent_harness`](../../../crates/meerkat/src/agent_harness.rs) runs a
  Command and closes. The shell is the same shape with a rhai front end.
- **The sandboxed lane** is [`RhaiEvaluator`](../../../crates/script/rhai/src/lib.rs):
  the standard rhai engine (no file/network builtins), `set_max_operations`
  op-budget, `set_max_call_levels` / `set_max_expr_depths` caps, `on_print` /
  `on_debug` silenced. It registers **no** host bindings, and only runs note
  sources that `EvaluationPolicy::for_own_notes` admits.

### The architectural key: snapshot in, Commands out

rhai's `Engine::register_fn` closures must be `'static` (and `Send + Sync`), so a
binding cannot borrow `Chrome` or the host state live. That constraint is a gift:
it forces the shell into the same eval-against-a-snapshot, emit-Commands-after
shape the agent harness already uses.

- Before eval, the host builds a **`ShellContext` snapshot** (read-only values:
  current url, history, can-back/forward, later the focused node and a tile/
  content report). Query bindings read the snapshot.
- Action bindings **enqueue a `Command`** into a shared buffer rather than
  mutating anything.
- `eval` returns a **`ShellOutcome { text, commands: Vec<Command> }`**. The host
  drains `commands` through the existing `run_command` / host-intent path (the
  `InvokeCommand` route), and echoes `text` (a return value, or an error) in the
  omnibar.

So the engine can only *read* a snapshot and *emit* Commands; it never touches
`Chrome`. That makes the whole shell unit-testable headlessly (snapshot in,
(text + command list) out, no live host) and keeps the trust boundary a property
of the binding set rather than of runtime guards.

## Trust model: two tiers, one language

Privilege is the binding set, gated by provenance (who authored the script), not
by a second language.

| Lane | Engine | Bindings | Runs on | Policy |
|---|---|---|---|---|
| Knot note block | `RhaiEvaluator` (script/rhai) | none | a note's `rhai eval` fence | `EvaluationPolicy::for_own_notes` admits the source |
| Omnibar shell | `CommandShell` (meerkat) | Command verbs + read-only queries | text the user types into their own omnibar | the user's keyboard is the provenance |

A peer's note cannot call `delete_node()` because the function is not registered
in the note engine. The privileged engine only ever sees text the local user
typed. The two lanes share the sandbox primitive (caps, no stdout); they differ
only in the binding set.

## Binding surface (slice 1)

Action verbs, each enqueuing the named `Command`. Zero-arg, acting on the
focused/selected target as the palette equivalents do:

- nav: `back()`, `forward()`, `home()`
- panes: `settings()`, `comms()`, `roster()`, `gloss()`, `apparatus()`,
  `inspector()`, `steward()`, `workbench()`, `compat_view()`
- graph/actors: `delete_node()`, `background_node()`, `hide_edge()`,
  `show_all_edges()`, `retry()`, `stop()`, `pin()`
- nav with an argument: `go(url)` (the `Url` arm as a callable), `search(text)`
  (the `Search` arm), `connect_peer(ticket)`

Read-only queries over the snapshot:

- `current_url() -> String`, `history() -> Array`, `can_back() -> bool`,
  `can_forward() -> bool`

Ergonomic sugar: a bare single identifier that names a zero-arg verb auto-calls,
so `>back` and `>back()` are equivalent. Anything else evals as rhai, so
`>for u in history() { print(u) }` and `>if can_forward() { forward() }` work.

Pulled into slice 1 (decision 2026-06-13): read-only **graph queries**
`focused_node()` and `nodes()`, so a typed expression can iterate the orrery
immediately (`>for n in nodes() { ... }`). This realizes the cross-to-orrery
reach in the first cut rather than deferring it, at the cost of the snapshot
carrying graph state. Still deferred to a later slice: the richer per-node
`inspect()` returning the serval `ContentReport` as a map (it converges with the
pelt/serval inspector work and wants the laid-out-document query seam).

## Placement

- **Shared sandbox primitive** to `script/rhai`: factor `base_engine()` out of
  `RhaiEvaluator::new` (the caps + silenced stdout). The note lane uses it bare;
  the shell lane layers bindings on top. No behavior change to the note path.
- **`CommandShell`** is meerkat-side (it references `Command` and host state, so
  it cannot live in `script/rhai` without inverting the dep). New module
  `meerkat/src/shell_eval.rs`: builds the engine, registers bindings against a
  shared command buffer, exposes `eval(source, &ShellContext) -> ShellOutcome`.
  Pure relative to the live host (snapshot in, outcome out), so it is unit-tested
  without a winit loop and stays well under the 600-LOC ceiling.
- **Resolution branch**: extend `NavTarget` with a `Command(String)` arm and
  teach `classify` the `>` sigil. The omnibar submit handler routes that arm to
  `CommandShell::eval`, drains `ShellOutcome.commands`, surfaces `.text`.

The pelt parallel: pelt's `TileShell` is the same drive surface scoped to tiles.
If pelt grows an omnibar, the pattern lifts (a `script/shell` crate could host
the engine builder), but meerkat is the first and only consumer now, so it lives
there until a second consumer pulls it out.

## Phases

- **S0** ✅ Extract `script_rhai::base_engine()` (the shared sandbox config) +
  re-export the rhai surface a privileged lane needs. Note-lane tests unchanged.
- **S1** ✅ `NavTarget::Command(String)` + `classify_with(input, sigil)` (the
  default `classify` passes `>`), so a leading `>` routes to the command arm.
- **S2** ✅ `CommandShell` + `ShellContext` + the slice-1 bindings (the 20
  `Command` verbs, the nav/history queries, **and** the graph queries
  `focused_node()` / `nodes()`) + `eval -> ShellOutcome`. Bare-verb / bare-query
  sugar (`>back`, `>current_url`). Eight pure unit tests (sugar, sequencing,
  query echo, conditional gating, graph-loop, op-budget runaway, unknown ident,
  empty no-op).
- **S3** ✅ Wire the omnibar Enter path: a `>`-expression with no highlighted
  suggestion routes to `submit_omnibar_command`, which builds the snapshot, evals,
  and drains each emitted command through the same per-interaction routine
  `chrome_activate` runs (`pending_command` is a single slot, so it drains between
  commands). The result text / error echoes via `show_location`; every eval is
  recorded as a `meerkat.omnibar.command` diagnostic. One driven host test
  (`>roster` toggles a Roster leaf, no navigation).
- **S4** ✅ `inspect()` returns the focused node's inspection rows (the Inspector
  pane's `(label, value)` pairs: node metadata, fetch state, content structure) as
  a rhai map, queryable by label (`inspect()["Title"]`). `ShellContext` carries the
  rows as plain `(String, String)` (no cross-crate dep); the bin populates them
  from `inspector_rows` over the focused node, `CommandShell` surfaces the map.
  (Broader than the originally-scoped serval `ContentReport`, and reuses the
  existing inspector logic — closes the omnibar↔inspector loop.)

## Decisions (2026-06-13)

1. **Sigil:** `>`, exposed as a setting with `>` as the default (configurability
   stance). `classify` reads a leading `>` as the command arm.
2. **Slice-1 binding scope:** the 20 Commands + nav/history queries **plus**
   read-only graph queries (`focused_node()`, `nodes()`). Richer per-node
   `inspect()` (the serval `ContentReport`) stays deferred.

## Progress

- 2026-06-13: Plan written. Grounded against `nav.rs`, `command.rs`, `suggest.rs`,
  `agent_harness.rs`, `script/rhai`.
- 2026-06-13: **S0-S3 shipped.** `script_rhai::base_engine()` extracted (note lane
  unchanged, 8 tests green); `NavTarget::Command` + `classify_with` sigil (2 new
  nav tests); `meerkat/src/shell_eval.rs` `CommandShell` (8 unit tests, snapshot-in
  / commands-out, the 20 verbs + nav/history/graph queries + bare-call sugar);
  Enter-path wiring in `input.rs` + `submit_omnibar_command` / `shell_context` in
  `frame_ops.rs`, draining each command through the `chrome_activate` routine and
  echoing via `show_location` + a `meerkat.omnibar.command` diagnostic (1 driven
  host test). Full meerkat suite green (54 lib + 72 bin). On-screen verify of a
  live `>`-command and the S4 `inspect()` query remain.
- 2026-06-13: **polish + unification + S4 shipped.** Palette/omnibar single source
  (`Command::verb()`; bindings + completion derive from `Command::ALL`); ghost-text
  autocomplete (serval `TextInput` ghost suffix + `accept_ghost`, `shell_eval::complete`,
  → / Tab accept; `f12f013`, `e6b459646da`); caret/selection/Ctrl+A input polish;
  `>relate` / `>unrelate` edge commands (`d4f7787`); S4 `inspect()` (`f4eee77`).
  All on-screen verified. Suite: 59 lib + 76 bin.
