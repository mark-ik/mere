# apparatus

Apparatus domain module for the [mere](https://crates.io/crates/mere)
browser — the peripheral system-inspector strip.

In scholarly use, an *apparatus* (apparatus criticus) is the textual /
critical machinery alongside a primary text: variants, footnotes,
source citations. mere's apparatus panel is the same shape applied to
the system itself — diagnostics, register-diagnostics channel taps, the
live uxtree dump, accesskit inspector, performance metrics, profiler
traces. Distinct from `system` (which *changes* the machine);
apparatus *examines* it.

## What it produces (v0)

A placeholder [`project_skeleton`] that emits a stub apparatus subtree
with sections for the inspector lanes that will eventually populate it:

```text
apparatus (Group "Apparatus")
  ├─ tracing events (Group, empty)
  ├─ register-diagnostics channels (Group, empty)
  ├─ uxtree (Group, empty)
  └─ accesskit (Group, empty)
```

Real content lands as each lane is wired:

- **tracing events** — pulled from a tracing-subscriber bridge
  installed at host startup
- **register-diagnostics channels** — bridged in via
  `register_diagnostics::install_global_sender`
- **uxtree** — a snapshot of the running application uxtree
  (the inspector overlay you already have in `mere-host`)
- **accesskit** — node walk of the current accesskit tree

## Status

Pre-1.0. Skeleton with placeholder section nodes. Each section grows
real content as the corresponding host bridge lands.
