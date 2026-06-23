# Layout Phase-Split Probe Plan (slice 5)

**Planned, 2026-06-23.** Spun out of the
[unified document host plan](2026-06-17_unified_document_host_plan.md) as its pressing slice 5
(documented-only there, never built). Build the **cascade-vs-box-tree-vs-shaping phase-split probe**
in serval-layout: a native-release timing harness that times each phase of a cold layout and reports
the split. It is the measurement prerequisite the
[parallelism strategy research](../research/2026-06-19_cross_platform_parallelism_strategy.md) §0
names for the whole parallel-cascade thesis, and it gates goal 2 (reach gpui-level baseline
performance).

## Why it gates everything downstream

There is no timing instrumentation in serval-layout today (no `Instant` / `bench` anywhere in the
crate), so the ~100 ms / 578 KB cold-layout cost is a single opaque number. The parallel-cascade
thesis assumes the **cascade** is the dominant, parallelizable share, but **box-tree build is
sequential**, so it caps the achievable win. Until the split is measured, every parallelism decision
(SAB-Rayon on pelt, the off-main-thread ordering, whether the cascade is even worth parallelizing) is
guesswork. Measure first.

## Build

A native-release timing harness, **cfg-gated out of wasm and the hot path**, that times each phase
(cascade / box-tree build / text shaping / layout) of a representative cold layout and reports the
breakdown. Small-Medium, pure measurement; no behaviour change. Done: a `cargo` invocation (a bench
or a gated test) prints the per-phase split on the reference page set, so the parallel-cascade share
is a known number, not an assumption.

## Wasmtime-async lane (per Mark)

The phase split also bounds the win for the non-browser **wasmtime lane** the parallelism research
doc parks (serval-on-Wasmtime / Spin for server-side async/parallel layout, the SSR/edge lane),
distinct from the browser's Web-Worker path. WASI 0.3 async (shipped 2026-06-11) is its substrate
today, wasi-threads later; the wasmtime-async work is where async/parallel layout can run off the
main thread server-side, and this probe's measurement is the same prerequisite for that lane. Unlike
the browser lane it is **not** COOP/COEP-gated (cross-ref the research doc, which keeps the browser
SAB lane app-lane-only).

## Cross-references

- [cross-platform parallelism strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  — §0 owner of the strategy this probe gates; cross-reference, do not restate (honor its confidence
  levels: parallel-cascade-on-wasm is unproven / borrowed).
- [unified document host plan](2026-06-17_unified_document_host_plan.md) — origin (pressing slice 5);
  its closing entry maps the Phase-2-tail threads.

## Progress

- **2026-06-23 (planned).** Extracted from the unified-document-host plan's pressing-slices section
  on its core-complete closeout. Documented-only to date; this plan is the build target. No code yet.
