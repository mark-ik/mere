# mere-host

The gpui host crate for [mere](https://crates.io/crates/mere).

## What lives here

The host crate is the apex of mere's stack — the integration point where:

- **`gpui::Application`** is bootstrapped, windows are opened, the event
  loop runs.
- **`tracing-subscriber`** is installed (consuming `tracing` events from
  every mere portable crate).
- **`PlatformSurface`** impls for non-mac platforms land
  (`DirectXSurface` on Windows, `WaylandSubsurface` / `X11ChildWindow`
  on Linux). On macOS we use Glass-HQ's existing `GpuiSurface`.
- **`accesskit_winit`** bridges uxtree subtrees from `mere-workbench`,
  `mere-orrery`, etc. into the OS accessibility APIs.
- **mere-domain modules** are wired together. The host stitches the
  workbench, orrery, gloss, system, etc. uxtree contributions under a
  single application-root accesskit node.

## Why a separate workspace

This crate has its own `[workspace]` declaration and is excluded from
the parent mere workspace via `exclude = ["crates/mere-host"]` in the
root `Cargo.toml`. Reason: gpui pulls a heavy native dep tree (wgpu,
font-kit, directx_renderer, direct_write on Windows). Excluding it
keeps `cargo build` / `cargo test` in the mere workspace fast for the
portable crates. When the host is built, all the gpui machinery
compiles.

This is the same pattern as `crates/probes/` — host code that depends
on host frameworks lives in a sub-workspace until the parent is ready
to absorb it.

## Status

v0 — scaffold only. Opens a gpui window and renders a hardcoded
`EngineDocument` (the markdown sample from probe-markdown-v0) to prove
end-to-end wiring. Subsequent rounds add:

- mere-workbench projection rendered alongside the document
- `tracing-subscriber` → `register-diagnostics::install_global_sender`
  bridge (rule 9: only registry crates emit through register-diagnostics
  but the host is the place that bridges)
- `accesskit_winit` integration consuming uxtree
- `PlatformSurface` impls for Windows/Linux

## Run

```pwsh
cd c:\Users\mark_\Code\repos\mere\crates\mere-host
cargo run
```

Glass-HQ/gpui must be checked out at `repos/glass-gpui/` (sibling to
`repos/mere/`).
