# Tracing Reach and Quality Plan

**Date**: 2026-06-26
**Status**: Active. T1 + T1.5 (reach gate) landed; T2 substrate-half landed; T3 leaf-library passes
(netfetcher / netrender / errand / serval-layout) + T5 dev-loop ring-dump + trace-event-quality
landed. Remaining: T2 call-site half, T3 in-tree engines + graph kernel (+ the larger scry / weld /
graft pass), T4 correlation, T5 sampling + error-chain capture.
**Spun out of**: [system diagnostics and accessibility plan](2026-06-08_system_diagnostics_and_accessibility_plan.md)
(the observability spine, Apparatus, AccessKit, and the typed agent harness, D0-D8, are
substantially landed). This plan is the follow-on: extend diagnostic/trace **reach** across the
first-party components and lift trace **quality** (structure, correlation, verbosity).

One line: **the spine is built but it is near-sighted and lossy.** The tracing-to-Apparatus bridge
forwards only three target prefixes and flattens field names to a fixed whitelist, and the actor
substrate, graph kernel, and web engines emit almost nothing. Widen the lens, sharpen the focus.

---

## Findings

Grounded against the live code (2026-06-26).

### The spine exists

`crates/meerkat/src/observability.rs` (`HostObservability`) holds bounded rings for diagnostics,
UX events, traces, probes, actors, and an a11y summary; the Apparatus pane renders them; the
`register-diagnostics` registry classifies channels (descriptors, per-channel `payload_schema`,
`sampling`, `retention`, invariants); `crates/meerkat/src/tracing_layer.rs` is the `tracing`
subscriber layer that mirrors spans/events into the diagnostics ring.

### The bridge is near-sighted

`tracing_layer.rs::interesting_target` (line 113) forwards a span/event only if its target starts
with `meerkat`, `frame`, or `uxtree`. Every other target is dropped before Apparatus sees it. So a
trace emitted from `armillary`, `inker`, `graph`, or any engine never reaches the diagnostics ring
even if the component does emit it.

### The real gate is the env filter, not the bridge (2026-06-27)

A correction to "near-sighted" above: the bridge allowlist is the *secondary* filter. `main.rs`
installs one **global** `EnvFilter` (default `meerkat=info`) *above* the Apparatus layer, so every
non-`meerkat` target is dropped before `interesting_target` ever runs. A unit test replicating the
stack proves it (`the_env_filter_is_the_real_reach_gate`): an `armillary` event, fully allowlisted
by the bridge, never arrives under `meerkat=info`. So T1's broadened allowlist and T2's armillary
spans did not reach Apparatus at all until this was fixed. The fix is per-layer filters: the RUST_LOG
env filter rides `fmt` (console only), and the Apparatus ring carries its own `LevelFilter::INFO`, so
the two consumers (a quiet RUST_LOG-governed console vs a first-party-broad ring) decouple. Grounding
for the leftover coverage question: first-party = the ~65 explicit workspace members; the vendored
donors (`blitz`/`xilem`/`masonry`/`weave`) are *not* members, and at info+ the dependency noise above
them is minimal. So a four-donor *exclusion* is now grounded, where the family-prefix *inclusion* list
silently misses most first-party crates (`aether`/`gyre`/`identity`/`register_*`/the inker engines/...).

### The bridge is lossy

`register_diagnostics::StructuredPayloadField.name` is `&'static str` (emit.rs:46). A runtime
field name is not `&'static`, so the layer routes every field name through `static_field_name`,
an 11-entry whitelist (`message`, `err`, `path`, `url`, `ticket`, `member`, `frame_id`,
`node_count`, ...), and **collapses everything else to the literal `"field"`**. The value survives
(formatted via `Debug`), but which field it was is lost. This is a type constraint, not an
oversight: lossless names require changing that field to `String` / `Cow<'static, str>`.

### The components are dark

`tracing::` call counts in the first-party crates:

| Crate | calls | | Crate | calls |
|---|---|---|---|---|
| `armillary` (actor substrate) | **0** | | `inker` | 10 |
| `graph` (kernel) | **0** | | `murm` (comms) | 5 |
| `intel` (embed/RAG) | **0** | | `persona` | 4 |
| `mesh` | **0** | | `orrery` | 3 |
| `moot` | **0** | | `forme` | 2 |
| `verso-scry` / `verso-serval` (engines) | **0** | | `shell` | 1 |
| `import`, `eidetic` | **0** | | | |

The async actor substrate, the graph kernel, and the web engines, the exact places where faults
hide and time is spent, emit nothing. Meerkat infers actor state from the kernel inbox drain, not
from the actors themselves, so a fault inside an actor reaches Apparatus only as an after-the-fact
inbox message with no causal detail.

### The registry is rich but under-used for live channels

`register-diagnostics` already supports structured payload schemas, per-channel sampling
(`SamplingPolicy::SampleRate`), retention policies, and invariants. The structured schemas that
ship today are the inherited graphshell catalog (protocol/viewer/action/identity/nostr/renderer);
the live `meerkat.*` channels are mostly `FreeText`. The descriptor layer is the natural driver
for "which targets/channels are interesting" and "at what verbosity," instead of the hardcoded
prefix list in the bridge.

### Where the actor spans go

`crates/armillary/src/actor.rs`: `spawn<C, U, F>(wake, run)` / `spawn_on(pool, wake, run)` run a
subsystem's `run(commands, emitter)` loop on its own thread / a pool worker. Lifecycle and
per-command spans belong around that loop (in armillary, and at the call sites that build the
content / fetch / sync / comms actors).

### Startup fault surfaced by the ring-dump (2026-06-29)

The first real use of the ring-dump (T5) immediately surfaced a startup fault that was previously
invisible. The **address-book store crashes a background worker while executing migrations** at
launch, and both subsystems that depend on it fail downstream:

- `meerkat::sync` (warn): `p2p sync disabled: transport bind: backend error: address book: while
  executing migrations: attempted to communicate with a crashed background worker`
- `meerkat::comms_host` (warn): `murm cabal unavailable; misfin only` (same root error)

So p2p sync was off and comms degraded to misfin-only on every launch, silently, until the ring-dump
showed it. The twist: this reach + quality work both *revealed* the fault and, it turned out,
*caused* it.

**Root cause + FIX (2026-06-29, `6df8fb2`).** Not redb, and not stale data (an earlier note here
guessed both; both were wrong). The address book is p2panda-net's **in-memory SQLite** store
(`SqliteStoreBuilder::new().build()` via sqlx-sqlite), so a *fresh* migration crashed every launch.
The captured stderr backtrace pinned it: sqlx-sqlite's connection worker thread panicked inside
`tracing-subscriber`'s registry (`extensions.rs:88: assertion failed: self.replace(val).is_none()`,
then a poisoned RwLock). The culprit is ours: `ApparatusTracingLayer::on_enter` inserted a
`SpanStart` extension unconditionally, but a span can be **re-entered** (an async future polled
repeatedly) and `ExtensionsMut::insert` panics on a duplicate. The migration enters/exits a span per
statement on the sqlx workers; the second enter double-inserts, panics, poisons the shared registry,
and the cascade kills the migration, the address-book spawn, and with it both p2p sync and the murm
cabal (they share the one AddressBook). Fixed by recording the start idempotently (insert only if
absent); headed-verified zero panics with p2p sync + murm cabal up (`cabal=true`). Exposed by this
plan's own reach work: the broadened first-party allowlist (T1) plus the per-layer ring that captures
regardless of RUST_LOG (T1.5) first brought re-entrant spans through `on_enter` by default.

**Lesson (feedback loop).** Instrumentation must never be able to panic the app it observes. Span
enter/exit fire multiple times per span, so any per-span extension write in a layer has to be
idempotent (`insert` asserts uniqueness; use a presence check or `replace`).

---

## Goals

Two axes, weighted equally per the brief:

- **Reach (coverage).** Every meaningful subsystem can emit a diagnostic/trace that actually
  reaches Apparatus: the actor substrate, content/fetch/sync/comms actors, the graph kernel, the
  web engines, inker, orrery.
- **Quality.** Traces are structured, correlated, timed, and at the right verbosity, so Apparatus
  (and a human, and the agent harness) can answer "what just happened, in what order, how long did
  it take, and why did it fail" without tailing terminal logs.

Non-goals: a second metrics system (the ring is the store); per-frame flood (sampling governs hot
paths); changing app authority (the ring stays an observation cache, per the 06-08 plan).

---

## Trace quality (the dimension, called out)

What "quality" means here, concretely:

1. **Lossless structured fields.** Change `StructuredPayloadField.name` to an owned/`Cow` string so
   a span's real field names survive. Retire the `static_field_name` whitelist.
2. **Typed channels for the load-bearing events.** Map the high-value component spans to registered
   channels with a `Structured` `payload_schema` (actor lifecycle, fetch, parse, layout), so
   Apparatus renders fields as columns and the registry's invariants/sampling apply, instead of a
   single generic `meerkat.tracing.event` free-text bucket.
3. **Correlation.** Thread an op/origin id (and use `tracing`'s native span parent/child) so events
   chain: `fetch(url) -> parse(url) -> render(url) failed` reads as one causal thread, not three
   unrelated rows. Apparatus gains a "by op" grouping.
4. **Timing.** Spans already carry enter/exit `duration_us`; extend to the phases that matter
   (cascade vs layout vs paint; fetch vs decode) and surface latency on the record.
5. **Level discipline + sampling.** `info` for lifecycle, `warn`/`error` for faults, `debug`/`trace`
   for verbose interior; hot paths declare a sample rate via the registry so they never flood.
6. **Error context.** Capture the full error chain (source/cause) on `failed` events, not just the
   top-line message.
7. **Ergonomics.** Adopt `#[tracing::instrument]` on actor/engine entry points (zero usages in the
   first-party crates today) so spans + arg fields are one attribute, consistent and cheap.

---

## Phases

### T1 - Bridge fix (DONE 2026-06-26)

- **Field names are already lossless with no type change.** `tracing::Field::name()` returns
  `&'static str` (compile-time metadata), so the bridge feeds the real name straight into
  `StructuredPayloadField.name` (still `&'static str`); the `static_field_name` whitelist (which
  collapsed unknown names to the literal `"field"`) is deleted, and typed `record_str/i64/u64/bool`
  keep natural values. The feared `&'static str -> Cow` ripple through `register-diagnostics` did
  **not** apply.
- `interesting_target` now checks a default first-party component prefix allowlist
  (`armillary`/`graph`/`inker`/`intel`/`orrery`/`mesh`/`moot`/`murm`/`persona`/`verso`/`serval`/...),
  overridable per dev session via `MEERKAT_TRACE_TARGETS` (comma-separated prefixes) with no rebuild.
- The `meerkat.tracing.event` generic channel stays the catch-all; T2/T5 promote the hot ones to
  schema'd channels.

Done: a `tracing::info!(target: "armillary", custom_field = 7)` reaches the ring with `custom_field`
intact; the whitelist is gone. 2 tests; 167 bin + 89 lib green. (Rode into `ac43edd` via a
concurrent-commit collision; the change is intact.)

### T2 - Actor substrate instrumentation (highest value)

- **Substrate lifecycle span: DONE 2026-06-26** (`armillary` `61c1bf5`). `spawn` delegates to a new
  `spawn_named` (+ `spawn_named_on`) that wraps the run loop in an `info` `armillary` lifecycle span
  (`actor = name`) bracketed by `actor started` / `actor finished` (`lifetime_ms`). Every actor
  (fetch/sync/comms/content/crawl/find_worker/community-lane) now reports its wall-clock lifetime
  with **zero call-site changes** (generic name `"actor"` by default).
- **Pending** (touches meerkat hot files; deferred until they settle): give each actor a specific
  name at its `spawn`->`spawn_named` call site, and add per-operation `started -> succeeded | failed`
  spans with an op id + the error chain inside the content/fetch/sync/comms `run` loops (the
  per-command semantics armillary can't see). Register the matching `meerkat.actor.*` channels with
  `Structured` schemas.

Done when an actor fault shows in Apparatus with the op id, elapsed time, and error chain, sourced
from the actor itself rather than inferred from an inbox message. (Substrate half lands the lifetime;
the call-site half lands the per-operation detail.)

### T3 - Engine + content passes

**Leaf libraries DONE (2026-06-28).** The four sibling libraries on the load path now emit a per-op
`debug` completion + `warn` fault, runtime-verified end-to-end (headed) and committed in their own
repos: `netfetcher` fetch (`url`/`status`/`elapsed_ms`, `65721fd`), `netrender` paint
(`op_count`/`viewport`/`scale`, per-frame, `6820eed95`), `errand` smolweb fetch
(`scheme`/`status`/`byte_len`, `4c82b5f`), `serval-layout` `lay_out_content`
(`fragment_count`/`image_count`/`elapsed_ms`, `868abf3`). The bridge allowlist gained
`netfetcher`/`netrender`/`errand`, and the Apparatus ring filter became an `EnvFilter`
(`info,netfetcher=debug,errand=debug,serval_layout=debug`, `f4af6d8`) so per-op completions reach the
ring while `netrender`'s per-frame `debug` stays out (only its faults pass). This is the
level/sampling discipline T5 calls for, applied at the ring rather than per-crate.

Remaining: the in-tree engines (`verso-scry` / `verso-serval` / `inker`) and `graph` kernel spans
below; the external web engines `scry` / `weld` / `graft` are the larger follow-on (same per-op +
fault shape, plus the engine-neutral `SurfaceFrame` seam).

- `verso-scry` / `verso-serval` / `inker`: cascade / layout / paint (and fetch / decode) spans with
  timing. This doubles as the per-pass perf signal the parallelism work will want.
- `graph` kernel: mutation / snapshot / query spans at `debug`, sampled.

Done when Apparatus can show per-pass timing for a page load, and a slow pass is visible without a
profiler.

### T4 - Correlation

- Thread the op/origin id through the actor + engine spans; rely on `tracing` span parentage for
  hierarchy; add a "group by op" read to the Apparatus Tracing/Events sections.

Done when a single page interaction reads as one ordered causal chain in Apparatus.

### T5 - Quality polish + dev-loop

- **Dev-loop escape hatch: DONE (2026-06-28, `0e03c91`).** Ctrl+Shift+D writes the full ring (every
  buffer up to capacity, far past the pane's recent window) to `<mere_root>/diagnostics-dump.txt`,
  toasts the path, and echoes it to stderr. `HostObservability::dump_report` formats
  diagnostics / traces / actors / probes / notifications / invariants + the a11y summary. This is what
  surfaced the startup fault in Findings.
- **Trace events modeled as log lines, not message-receipts: DONE (2026-06-29, `633dc8a`).**
  `register-diagnostics` gained `DiagnosticEvent::Event { target, level, message, fields }`; the
  bridge emits it instead of overloading `MessageReceivedStructured` (which had forced a synthetic
  channel + fake `latency_us: 0`). The consumer maps `level -> severity` (so a `warn`/`error` fault
  reads as a fault, not flattened to `info`) and renders `target: message (fields)`. A step toward
  quality #2 (typed channels) short of per-channel schemas.
- **Level discipline (partial): DONE.** a11y tree rebuilds log a diagnostic only when *degraded* (the
  healthy per-interaction rebuild was crowding the recent window; `d17619b`); the ring's per-target
  `=debug` opt-in (T3) keeps per-frame paint out of the ring.
- Remaining: per-channel sampling via the registry for the hot paths; full error-chain capture on
  `failed` events (today only the top-line message).

Done when sustained browsing does not flood the ring, failures carry their cause, and the ring is
dumpable without a rebuild. (Dumpable plus level-discipline for current sources: done; registry
sampling and error-chain capture: open.)

---

## Risks / gotchas

- ~~`StructuredPayloadField.name` ripple.~~ **Resolved in T1: no change needed.**
  `tracing::Field::name()` is already `&'static str`, so the bridge feeds the real name straight
  through and the type stays as-is. The `descriptor/types.rs:24` schema field is a separate
  compile-time declaration, also untouched.
- **Chrome-hot concurrency.** Mark's shellbar / graph-signals / illume work has `views.rs`,
  `render.rs`, `menus.rs`, `lib.rs`, `main.rs`, plus `graph-kernel` dirty. The instrumentation
  touches some of these. Commit with explicit pathspec per the alembic-tail handoff's gotcha, and
  prefer instrumenting the leaf crates (armillary, engines) that are not chrome-hot first.
- **Over-instrumentation.** Spans on a per-frame or per-DOM-op hot path will flood the ring; T3/T5
  gate those behind `debug`/`trace` + sampling from the start, not retroactively.
- **Target vs channel.** A `tracing` target is a module path; a diagnostic channel is a registered
  id. T1/T2 must settle the mapping (a small target -> channel table, or a convention) rather than
  leave two vocabularies drifting.

---

## Open decisions

1. **Target -> channel mapping.** A registered descriptor per component span (rich, typed,
   sampled), or a generic per-target trace channel with structured fields (cheap, uniform)? Lean:
   generic for T1 reach, promote the load-bearing ones to typed channels in T2/T5.
2. **Correlation id source.** Per-actor, per-request/op, or per-origin? Lean: per-op id minted at
   the actor command boundary, carried in span fields.
3. **`Cow` vs `String`** for the payload field name. Lean: `Cow<'static, str>` so the donor's
   `&'static` literals stay zero-alloc and runtime names own. **Resolved in T1: moot (`Field::name()`
   is `&'static str`).**
4. **Ring scope: four-donor exclusion vs complete inclusion.** Now that per-layer filters put the
   scope decision on the Apparatus layer (T1.5), should `interesting_target` *exclude* the four
   vendored donors (`blitz`/`xilem`/`masonry`/`weave`, the only non-member crates emitting at info+)
   or *include* a complete list of the ~65 workspace members? Lean: exclusion (small, grounded,
   self-maintaining; the family-prefix inclusion list already silently drops most first-party crates).

---

## First slice

T1 (the bridge fix): lossless field names + registry/config-driven target allowlist. It is small,
self-contained to `register-diagnostics` + `meerkat/tracing_layer.rs`, and it lights up every
component the later slices instrument.

---

## Progress

- 2026-06-26: Plan written. Grounded against `tracing_layer.rs` (narrow `interesting_target` +
  `static_field_name` whitelist), `register-diagnostics` (rich descriptor/schema/sampling registry;
  `StructuredPayloadField.name: &'static str`), `observability.rs` (the spine), `armillary::actor`
  (spawn/run shape), and a first-party tracing-coverage survey (armillary/graph/intel/mesh/moot/
  verso-* at zero). Spun out of the 06-08 diagnostics plan (D0-D8 landed). No code yet.
- 2026-06-26: **T1 (bridge fix) landed.** `tracing_layer.rs`: dropped the `static_field_name`
  whitelist (`tracing::Field::name()` is already `&'static str`, so the real field name flows
  straight into `StructuredPayloadField.name` with no `register-diagnostics` change), added typed
  `record_str/i64/u64/bool` for natural values, and replaced the `meerkat`/`frame`/`uxtree` target
  filter with a default first-party prefix allowlist plus a `MEERKAT_TRACE_TARGETS` env override
  (no rebuild). So the actor substrate + engines reach the diagnostics ring with intact structured
  fields once they emit. 2 tests; 167 bin + 89 lib green. The `&'static str -> Cow` ripple gotcha
  and open decision #3 are therefore moot. Landed inside `ac43edd` (a concurrent broad commit swept
  the staged file into Mark's browse-trace commit; the change is intact). Next: T2 (actor substrate).
- 2026-06-26: **T2 substrate half landed** (`armillary` `61c1bf5`). `armillary::spawn`/`spawn_on`
  delegate to span-wrapped `spawn_named`/`spawn_named_on`; every actor now emits an `info` lifecycle
  span + `actor started`/`actor finished` (`lifetime_ms`) on target `armillary` (which T1's
  broadened bridge captures), with no call-site change. `tracing` is armillary's first dep
  (host-neutral facade). 1 new test; 8 armillary green. Committed cleanly via `--only` (no sweep).
  The call-site half (specific names + per-operation `started->succeeded|failed` + op id + error
  chain in the content/fetch/sync/comms loops) is deferred while meerkat is chrome-hot under Mark's
  concurrent browse-trace work (meerkat is currently red on his mid-edit `TraceEvent.candidates`,
  unrelated to this change). Next: the call-site half once meerkat settles, or T3 (engine passes) on
  another leaf.
- 2026-06-27: **Reach gate found and fixed (T1.5).** Examining the actual subscriber install (prompted
  by Mark's "examine the crates before just excluding") showed the *global* `EnvFilter` default
  `meerkat=info` was dropping every non-meerkat target before the bridge ran, so T1/T2 reached nothing
  by default. Proven by `the_env_filter_is_the_real_reach_gate` (committed `3a3a5d0`). Fixed with
  per-layer filters in `main.rs` (`fmt` carries the env filter; the Apparatus ring carries its own
  `LevelFilter::INFO`); `cargo check` green; `per_layer_split_feeds_the_ring_while_the_console_stays_quiet`
  proves an armillary event now reaches the ring while the console stays scoped (test committed
  `c8c4f19`; the ~4-line `main.rs` hunk rides the concurrent switcher refactor in that file rather
  than being carved out). This makes T1/T2/T3 actually observable. Leftover: open decision #4 below.
- 2026-06-28/29: **T3 leaf libraries + T5 dev-loop + trace-event quality landed.** Instrumented the
  four load-path sibling libraries (netfetcher / netrender / errand / serval-layout) with a per-op
  `debug` completion + `warn` fault, runtime-verified headed (example.com fetch then layout pulse,
  plus a bad-URL fault, all in the Apparatus pane under default RUST_LOG); committed in their own
  repos (`65721fd` / `6820eed95` / `4c82b5f` / `868abf3`). Made the Apparatus ring an `EnvFilter` so
  per-op `debug` completions reach it without per-frame flood (`f4af6d8`). Added
  `DiagnosticEvent::Event` so trace events carry real severity and read as `target: message (fields)`
  rather than fake message-receipts (`633dc8a`), de-noised a11y to degraded-only (`d17619b`), and
  shipped the Ctrl+Shift+D ring-dump (`0e03c91`). Headed verification of all this also surfaced (and
  fixed, separately) two unrelated chrome bugs: window keyboard-focus-on-show (`7f984ff`) and the
  omnibar dropdown stacking over the shellbar (`19fd35d`); plus the address-book startup fault now in
  Findings. Next: T2 call-site half, or T3 in-tree engines + graph kernel, or the scry / weld / graft
  pass.
- 2026-06-29: **Fixed the startup crash the ring-dump surfaced (`6df8fb2`).** Traced it from the
  captured stderr backtrace to our own `ApparatusTracingLayer::on_enter`: it inserted `SpanStart`
  non-idempotently, so a re-entered span double-inserted, `ExtensionsMut::insert` panicked, and the
  poisoned registry RwLock cascaded into a crash that killed the p2panda in-memory-SQLite
  address-book migration on sqlx's worker threads, disabling p2p sync + the murm cabal. A regression
  exposed by this plan's reach work (T1 allowlist + T1.5 per-layer ring). Recorded the start
  idempotently; headed-verified zero panics with sync + cabal up (`cabal=true`) and the first-party
  pulse intact; new re-entry test. See the corrected Findings entry. Lesson: a tracing layer must
  never panic, and per-span extension writes must be idempotent (enter/exit fire repeatedly).
