# Meerkat automation: two subsystems, one vocabulary

**Date**: 2026-07-07 (self-drive mode landed 2026-07-08)
**Status**: Assessment + the unification, now built and complete for the whole session. The
"what ails it" fixes landed with the Slice 3 headed check; the `MEERKAT_SCENARIO` self-drive
mode + shared scenario vocabulary landed 2026-07-08 (multi-window + settings verified headed),
and the `navigate` + `key` verbs for flows outside the registry landed the same day (find
verified headed). Only pointer gestures remain outside a scenario (Migration item 4).
**Scope**: the two ways meerkat is driven under automation, what ails the headed one, and a
scheme to let one *scenario* run either way.
**Related**: the [meerkat one-state migration (archived)](../../archive_docs/2026-07-07_one_state_migration/2026-07-06_meerkat_one_state_migration_plan.md)
(whose Slice 3 headed check surfaced this); the mod-authoring loop's
[snapshot-in/action-out automation](../implementation_strategy/2026-06-30_runtime_mod_authoring_loop_plan.md)
(user-facing Rhai packs — adjacent, not the dev harness).

## The finding: two automation subsystems, different layers

Meerkat is driven under automation two ways, and they verify **different layers**:

1. **In-process `agent_harness`** (`crates/meerkat/src/agent_harness.rs`, `#[cfg(any(test,
   feature = "agent-harness"))]`). Constructs a real `Shell`, drives it by **registry
   command / context ids** (`agent_invoke(id)` → `agent_invoke_command` /
   `agent_invoke_context`, plus `agent_select_node_by_url`, `agent_set_theme`, synthetic
   key/pointer events through a `WindowCtx`), and asserts on state. Headless: no OS window,
   no GPU present — the render path builds scenes but never reaches the swapchain. **Fast,
   deterministic, the source of truth for logic/behaviour** (302 tests, incl. the 230 that
   drive the full input/render path headlessly).

2. **External Win32 PowerShell drivers** (`scry-shots/*.ps1`). Launch the real
   `meerkat.exe`, drive it via **OS synthetic input** (`SendKeys` / `mouse_event` /
   `keybd_event`), and capture pixels (the in-app GPU self-capture, or ffmpeg `ddagrab`).
   The **only** way to verify actual on-screen rendering, GPU present, DPI, and real
   multi-window OS behaviour — but slow, non-deterministic (focus races, timing), and
   assertion-poor (a human eyeballs screenshots).

Neither can do the other's job: the in-process harness can't prove a frame presents to the
screen; the PS drivers can't cheaply assert internal state. So they are genuinely two
layers, not duplicates. The problem is not that both exist — it is that the **headed layer
has no working canonical base and no shared vocabulary with the headless one**.

## What ails the headed harness (and what landed this session)

The Slice 3 headed check found the PS layer broken in four ways. Three were fixed this
session; the fourth is the proposal below.

- **Dead shared base.** ~130 `drive-*.ps1` / `verify-*.ps1` scripts each begin
  `. C:\Users\mark_\Code\pelt-shots\harness.ps1` — but `pelt-shots\` was deleted 2026-06-15.
  Every script is broken at line 1. **Fixed**: a single canonical base,
  `testing\mere\scripts\mk-harness.ps1` (its `scry-shots\` home was retired in the folder
  reorg below), folds in the reliable primitives each script re-derived ad hoc (see below).
  New headed checks dot-source it; the one-offs are superseded.
- **Stale exe path + target-dir override.** Scripts pointed at `repos\mere\target\debug`,
  but a `.cargo/config.toml` `[build] target-dir = "C:/t/meerkat-target"` override moved
  the real exe. **Fixed**: the override is removed (builds land in `repos/mere/target/`
  again), and `mk-harness` reads from there. (The stale `C:\t\meerkat-target` tree can be
  deleted; it is no longer written.)
- **Unreliable window + capture.** winit's `MainWindowHandle` is a 13×13 stub, not the app
  window; `CopyFromScreen` silently captures the wrong window under this session's focus
  race. **Fixed in `mk-harness`**: an `EnumWindows` "largest visible top-level for pid"
  finder (`Find-MkWindows`, primary first, then spawned leaves by area), plus both reliable
  capture paths — the GPU **self-capture** (nudge one window, read its chrome texture off
  the GPU, compositor-independent) and **ddagrab** (full-desktop truth, orrery canvas
  included) with the `AttachThreadInput` foreground-force in the same step.
- **Self-capture dead in partitioned mode** (the render-perf note's "capture harness dead in
  partitioned mode"). The self-capture only fired on `ChromeRasterPlan::Full` frames; once
  the shell settles into `ChromeRasterPlan::Partitioned` (base + orrery reused), the code
  passed `_chrome_tex = None` and the capture silently no-opped. **Fixed** in
  `render/paint.rs`: fall back to the cached `chrome_base_tex` (which holds exactly the
  chrome-minus-orrery the chrome-layer capture targets), so capture works on any frame.

With those, the Slice 3 headed check ran clean: primary + a Ctrl+Shift+N leaf both rendered
from the one `GenetMultiRunner` (leaf slim, primary full, both on the shared graph through
their own cameras), no crash. Shots: `testing\mere\images\s3c-{1-primary,3-both}.png`.

## The unification seam: the registry command id

The in-process harness already drives by **registry command ids** — the same stable ids the
palette and context menu use (`agent_invoke("theme.set")`, `context.addnode`, …). That id
is the natural shared vocabulary. A *scenario* is then a named, ordered list of steps over
that vocabulary plus capture/assert markers:

```
navigate mere://welcome
invoke  window.spawn            # or the Ctrl+Shift+N chord, headed
assert  window_count == 2       # headless: state assert; headed: EnumWindows count
capture primary chrome
capture desktop both
```

The same scenario can run **headless** (agent_harness executes each `invoke`, evaluates each
`assert` against state, skips `capture`) or **headed** (the app executes each `invoke`,
`capture` writes a PNG via the self-capture hook, `assert` degrades to a logged check). One
description, two layers — that is the unification, and it does not collapse the layers that
genuinely differ.

## Landed: the self-driven headed scenario mode

The headed layer's real fragility is **OS synthetic input** (focus races, timing). The fix
that also delivers the unification: let the app **drive itself** from a scenario file instead
of receiving OS input. This shipped 2026-07-08.

- **The vocabulary** (`crates/meerkat/src/scenario/mod.rs`): a line-oriented format over the
  registry-id space plus the host verbs, the two non-registry-flow verbs, and the harness
  markers a driven session needs — `invoke <id>`, `navigate <url>`, `key <chord>`,
  `theme <id>`, `spawn`, `capture <name>`, `settle [<frames>]`, `assert windows <op> <n>`,
  `log <text>`. A trailing `@<n>` on a windowed verb targets a specific window (0 = primary).
  Pure data + parser, unit-tested; no `Shell` dependency. (`navigate` / `key` detailed under
  Migration item 3.)
- **`MEERKAT_SCENARIO=<path>`** at boot loads a scenario (`ScenarioRunner::from_env`); the
  shell then runs it on the event loop, one step per `about_to_wait` tick (`pump_scenario`),
  under `ControlFlow::Poll` so ticks progress without OS input. Each `invoke` routes through
  the **same** registry seam the palette / menu / `agent_invoke` use
  (`WindowCtx::scenario_invoke`, the twin of `agent_invoke_command` / `agent_invoke_context`);
  each `capture` writes the target through the existing `MEERKAT_CAPTURE_DIR` self-capture
  hook. No `SendKeys`, no cursor warping, no focus race. On completion the runner writes a
  `scenario.done` sentinel (first line `RESULT ok` / `RESULT fail`, then the step log) and
  exits.
- **Per-window capture** is deterministic: the capture request carries an optional projection
  index, and the self-capture hook (`maybe_dump_chrome_capture`, now passed
  `self.view.projection_id.0`) only fires on the matching window, leaving the request in place
  for the intended one. So `capture leaf @1` captures the spawned leaf even while the primary
  is also rendering — verified: the leaf shot is slim chrome, the primary shot is full, not a
  re-capture.
- **The executors are shared** so headless and headed run the *same* step through the *same*
  code: `Shell::scenario_invoke` / `scenario_theme` / `scenario_window_count` are always
  compiled and drive the real host seams. The genuinely-differing layers diverge only where
  they must — spawn needs the event loop, capture needs the GPU. A `#[cfg(test)]`
  `run_scenario_headless` consumes the identical parsed steps and asserts state, proving the
  claim (`scenario/runner.rs` tests).
- **`mk-harness` shrank to launch + collect** (`Run-Scenario`): it launches with
  `MEERKAT_SCENARIO` + `MEERKAT_CAPTURE_DIR` set, waits for the sentinel, and collects the
  PNGs. No synthetic input in the path.

Multi-window was the immediate motivator, and it verified clean: the shipped
`scenarios/multi_window.scn` self-drives `capture primary` -> `invoke roster` (roster pane
opens on the primary) -> `spawn` -> `assert windows == 2` (PASS, two live OS windows) ->
`capture leaf @1` (the slim leaf), all from one `GenetMultiRunner`, no OS input, no focus
race.

## Migration / next

1. **Landed 2026-07-07**: `mk-harness.ps1` base; the partitioned-mode capture fix; target-dir
   back to `repos/mere/target`; the Slice 3 headed check on the new base. The three media
   folders (`screenshots\` / `screenrecordings\` / `scry-shots\`) were unified into
   `Code\testing\<repo>\{images,videos,scripts}` (repo = `mere` / `isometry` / `genet`;
   `_unsorted\` = the ~700 untagged raw captures + recordings; `_archive\scripts\` = the ~138
   retired one-off drivers). `mk-harness` + `drive-s3c` moved to `testing\mere\scripts\` and
   write shots to `testing\mere\images\`; the ephemeral cruft (448 logs, the `forget-profile`
   test DB, capture working dirs, locks) was purged.
2. **Landed 2026-07-08**: the `MEERKAT_SCENARIO` self-drive mode + the shared scenario format
   (above); the `mk-harness` `Run-Scenario` launch+collect; two seed scenarios
   (`crates/meerkat/scenarios/multi_window.scn`, `settings.scn`), both verified headed
   (`RESULT ok`).
3. **Landed 2026-07-08**: the two verbs for flows outside the registry, so the vocabulary now
   covers the whole session, not just registry commands:
   - **`navigate <url>`** routes through the omnibar-submit path (`WindowCtx::scenario_navigate`
     -> seed the address bar -> `submit_omnibar` classify/resolve/history -> the host's
     `sync_orrery` load), the same route Enter in the omnibar takes. Async, so follow with
     `settle`. URL `#fragments` survive parsing (only a whitespace-preceded `#` is a comment).
   - **`key <chord>`** (`ctrl+f`, `ctrl+shift+n`, `enter`, `f5`, `escape`, a bare char)
     dispatches through the real `on_key_pressed` path (`WindowCtx::scenario_key` sets the
     modifier state the handler reads, presses, restores). This reaches the chords that are
     not registry commands: Ctrl+F find, and typing into the find bar / omnibar char by char.
     Verified headed: `scenarios/find.scn` opens find with `key ctrl+f` and types a query.
4. **Cleanup**: the orphaned `C:\t\meerkat-target` build tree can still be deleted.

The end state is now reached across the whole session, not just the registry-id core: one
scenario vocabulary (registry ids + `navigate` + `key`), two runners (headless assert / headed
self-drive+capture), one PS base (`mk-harness`) that only launches and collects. The only
things still outside a scenario are pointer gestures (drag / click-at-coord); those remain the
`Click`/`Dda-Capture` province of `mk-harness` and are a later verb if a scenario ever needs
them.
