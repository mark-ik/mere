# Meerkat automation: two subsystems, one vocabulary

**Date**: 2026-07-07
**Status**: Assessment + proposal. Some of the "what ails it" fixes landed this session
(prompted by the Slice 3 headed check); the unification is proposed, not yet built.
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
from the one `ServalMultiRunner` (leaf slim, primary full, both on the shared graph through
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

## Proposed: a self-driven headed scenario mode

The headed layer's real fragility is **OS synthetic input** (focus races, timing). The fix
that also delivers the unification: let the app **drive itself** from a scenario file instead
of receiving OS input.

- **`MEERKAT_SCENARIO=<path>`** (env or arg): at boot, the shell reads a scenario of registry
  command ids + capture markers and runs it on the event loop (each step on a frame boundary),
  invoking the **same registry seam** `agent_invoke` uses and writing captures through the
  existing `MEERKAT_CAPTURE_DIR` hook. No `SendKeys`, no cursor warping, no focus race — the
  app injects its own commands and captures its own frames. Deterministic, headed, and
  expressed in the same vocabulary as the headless harness.
- **Scenario source of truth**: a small scenario format (or Rhai, reusing the mod-authoring
  loop's snapshot-in/action-out shape) that both `agent_harness` (assert mode) and the headed
  app (drive+capture mode) consume. A regression scenario is written once and gives both a
  headless assertion and a headed screenshot.
- **`mk-harness` shrinks to launch + capture-collection**: it launches with `MEERKAT_SCENARIO`
  set, waits for the app to finish the scenario (a sentinel file / log line), and collects the
  PNGs. The 130 one-off drivers become ~N scenario files; the broken boilerplate is retired.

Multi-window is the immediate motivator: a headed multi-window scenario (spawn, act in window
A, assert it reflects in window B) is exactly what the Slice 3 flip wants to keep honest, and
OS-input drivers make that especially racy across two windows — a self-driven scenario does
not.

## Migration / next

1. **Landed**: `mk-harness.ps1` base; the partitioned-mode capture fix; target-dir back to
   `repos/mere/target`; the Slice 3 headed check on the new base. **Also landed 2026-07-07**:
   the three media folders (`screenshots\` / `screenrecordings\` / `scry-shots\`) were unified
   into `Code\testing\<repo>\{images,videos,scripts}` (repo = `mere` / `isometry` / `serval`;
   `_unsorted\` = the ~700 untagged raw captures + recordings; `_archive\scripts\` = the ~138
   retired one-off drivers). `mk-harness` + `drive-s3c` moved to `testing\mere\scripts\` and
   write shots to `testing\mere\images\`; the ephemeral cruft (448 logs, the `forget-profile`
   test DB, capture working dirs, locks) was purged.
2. **Near-term (proposed, needs sign-off)**: the `MEERKAT_SCENARIO` self-drive mode + a
   scenario format shared with `agent_harness`; port a handful of the highest-value one-offs
   (settings, find, multi-window) from `_archive\scripts\` to scenarios as the pattern.
3. **Cleanup**: the orphaned `C:\t\meerkat-target` build tree can still be deleted.

The end state: one scenario vocabulary (registry ids), two runners (headless assert / headed
self-drive+capture), one PS base (`mk-harness`) that only launches and collects.
