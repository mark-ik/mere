# Meerkat CLI Tooling Plan

**Date**: 2026-06-29
**Status**: Complete for the local tooling slice.
**Code**: `scripts/meerkat.ps1`, `scripts/check-meerkat.ps1`,
`scripts/test-roster.ps1`, `scripts/drive-meerkat.ps1`; local ignored config in
`.cargo/config.toml`, `C:\Users\mark_\Code\.cargo\config.toml`,
`C:\Users\mark_\Code\repos\woodshed\.cargo\config.toml`, and
`C:\Users\mark_\Code\repos\strophe\.cargo\config.toml`.

This slice cleans up the local Meerkat build loop after the roster/card work
exposed two avoidable sources of doubt: the target cache was named
`graphshell-target`, and Cargo emitted path-override warnings before every
otherwise-useful check. The goal is not a new build system. The goal is a quiet,
named, repeatable local CLI for the Meerkat binary and the focused roster/card
checks.

Related docs:

- [external_deps_topology_brief](../../2026-05-24_external_deps_topology_brief.md)
  owns the `Code/` workspace split and cross-repo local override convention.
- [graph_object_roster_detail_cards_plan](2026-06-29_graph_object_roster_detail_cards_plan.md)
  is the slice whose headed verification exposed the stale target path.

---

## Plan

### T1 - Rename the stale target cache

Use a Meerkat-named target directory for local builds:

`C:\t\meerkat-target`

Done when local scripts and repo-local Cargo config resolve the Meerkat binary
from that directory, and the old `C:\t\graphshell-target` cache is no longer the
normal path.

**Status**: Done 2026-06-29. The existing directory was renamed from
`C:\t\graphshell-target` to `C:\t\meerkat-target`. The process-level
`CARGO_TARGET_DIR` inherited by this Codex session still exists outside the repo,
so the scripts set the target dir explicitly.

### T2 - Replace broad Cargo `paths` overrides

The old local `.cargo/config.toml` used Cargo's global `paths = [...]` override
to redirect git dependencies to sibling checkouts. That kept cross-repo edits
live, but Cargo warned whenever a local checkout had a dependency list that did
not match the resolved git manifest.

Use source-specific `[patch."https://github.com/mark-ik/<repo>.git"]` entries
instead. This names the git source and package explicitly and keeps the local
edit loop without the path-override warning storm.

**Status**: Done 2026-06-29. Mere's ignored `.cargo/config.toml` now patches:

- `serval`: `layout-dom-api`, `pelt-core`, `pelt-desktop`,
  `script-engine-boa`, `serval-extract`, `serval-layout`,
  `serval-scripted-dom`, `serval-static-dom`, `serval-winit-host`,
  `xilem-serval`, and `xilem_core`.
- `netrender`: `netrender`, `netrender_device`, `netrender_text`,
  `paint_list_api`, and `paint_list_render`.
- `netfetcher`: `netfetcher`.
- `errand`: `errand`.
- `wgpu-scry`: `scrying`.
- `wgpu-graft`: `grafting`.

The inherited parent config at `C:\Users\mark_\Code\.cargo\config.toml` was also
cleared of `paths` entries. Those entries were intended for Woodshed/Strophe, but
Cargo inherited them into Mere and forced `xilem_core` from
`C:\Users\mark_\Code\crates\xilem-woodshed`. Woodshed and Strophe now carry those
local overrides in their own ignored repo configs.

### T3 - Add one blessed Meerkat runner

Add a PowerShell runner that owns the normal local commands and always prints or
uses the resolved Meerkat executable path.

Done when one script can check, build, run, drive, and run the focused roster
tests without relying on `target/debug/meerkat.exe`.

**Status**: Done 2026-06-29. `scripts/meerkat.ps1` supports:

- `check`: `cargo check -p meerkat --bin meerkat --message-format=short`
- `build`: build the Meerkat binary and print the exe path.
- `run`: build and run the exe in the foreground.
- `drive`: build and launch the exe with `Start-Process`, printing the PID.
- `test-roster`: run the focused roster/card/swatch/session tests.

### T4 - Add focused command wrappers

The roster/card/swatch lane is bin-module shaped, so `cargo check -p meerkat
--lib` is not a useful guard. Keep short wrapper names for the checks that match
the current module shape.

**Status**: Done 2026-06-29:

- `scripts/check-meerkat.ps1`
- `scripts/test-roster.ps1`
- `scripts/drive-meerkat.ps1`

---

## Findings

- **The stale Graphshell name was target-cache naming, not app identity.**
  `crates/meerkat/Cargo.toml` still names the package and bin `meerkat`, and the
  native window title still starts with `Meerkat`.
- **The noisy Cargo warnings had two causes.** Mere's repo-local `paths` override
  caused most warnings. The last `xilem_core` warning came from the parent
  `Code/.cargo/config.toml`, which Cargo inherited into every repo under `Code/`.
- **Parent Cargo config should stay boring.** Local dependency overrides should
  live at the repo that needs them. A parent `paths` entry is effectively global
  across all sibling projects and can silently change unrelated graphs by
  package name and version.
- **PowerShell is the right first CLI surface here.** The local environment is
  Windows, the existing cross-repo smoke is PowerShell, and the runner can set
  `CARGO_TARGET_DIR` reliably without asking the caller to remember it.

---

## Progress

### 2026-06-29 - Local CLI cleanup

Landed:

- Renamed `C:\t\graphshell-target` to `C:\t\meerkat-target`.
- Rewrote Mere's ignored `.cargo/config.toml` from broad `paths` overrides to
  source-specific local patches and `build.target-dir = "C:/t/meerkat-target"`.
- Cleared inherited `paths` entries from `C:\Users\mark_\Code\.cargo\config.toml`.
- Moved the Woodshed/Strophe `xilem-woodshed` overrides into ignored local config
  files for those repos.
- Added `scripts/meerkat.ps1` and the short wrappers:
  `check-meerkat.ps1`, `test-roster.ps1`, and `drive-meerkat.ps1`.

Verification:

- `cargo metadata --no-deps --format-version 1` with `CARGO_TARGET_DIR` removed
  reports `target_directory: C:/t/meerkat-target`.
- `cargo tree -p meerkat -i xilem_core --edges normal` resolves `xilem_core`
  from `C:\Users\mark_\Code\repos\serval\components\xilem-core`.
- `powershell -ExecutionPolicy Bypass -File scripts\check-meerkat.ps1` passed.
- `powershell -ExecutionPolicy Bypass -File scripts\test-roster.ps1` passed:
  graphlet card tests, active-tab test, fanned relation-cell test, and hidden
  relation restore test.
- `powershell -ExecutionPolicy Bypass -File scripts\meerkat.ps1 build` passed and
  printed `C:\t\meerkat-target\debug\meerkat.exe`.
- The path-override warning storm is gone; remaining output is ordinary existing
  unused-code warnings.

Open:

- The current Codex process still inherits a stale process-level
  `CARGO_TARGET_DIR`. The repo scripts override it, and a fresh shell without
  that inherited variable uses the repo config.
