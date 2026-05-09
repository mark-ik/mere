# probes

Disposable thin-slice integration probes. Each probe answers one question
about how Mere's portable crates connect to a candidate host or library,
without committing the rest of the workspace to that direction.

This directory is a **separate Cargo workspace**, excluded from the parent
mere workspace via `exclude = ["crates/probes"]` in the root `Cargo.toml`.
Probes can pull heavy host-shaped dependencies (gpui, parley, servo) without
poisoning mere's normal build/lock.

## Lifecycle

A probe is born to answer a question, lives long enough to answer it, and
either gets deleted or graduates into a real crate. Don't grow features in
probes — when a probe outgrows its scope, fork the useful parts into a
proper crate (under `crates/`, in the mere workspace) and delete the probe.

## Probes

| Probe | Question | Status |
| --- | --- | --- |
| [markdown-v0](markdown-v0/) | Can `inker::EngineDocument` (produced by `nematic::markdown`) be rendered through Glass-HQ/gpui without `platen` / `verso-tile` / `PlatformSurface`? | bring-up |
