# SessionServiceRunner — implementation plan

**Date**: 2026-05-14
**Status**: Implementation plan — v0a trait + null runner landed; v0b real workers pending

> **Reconcile note (2026-07-03 archive pass):** the code moved past the status line —
> `session_service_runner.rs` now also ships an `InMemoryRunner` (`impl SessionServiceRunner`)
> and the misfin server (`crates/murm/misfin/src/server.rs` *(historical citation)* <!-- doc-audit: historical-path -->) runs as a
> `SessionServiceRunner` worker. v0b is at least partially real; the remaining question is
> which of the §-listed worker kinds (fetcher pool, embedder, indexer, …) still lack runners.
> File paths below are 2026-05-14-era; verify before use.
**Scope**: Let sessions declare background workers (fetcher pool, embedder, indexer, intelligence-signal producer, …) that run with no attached client. Per the framing brief §5.7, the kernel stays a pure data layer; networking and GPU/model runtimes live behind a `SessionServiceRunner` capability the host implements. Land the trait + a no-op runner + the worker-status vocabulary now; per-worker implementations land as their workloads materialise.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) §5.7 — the framing brief.
- [`crates/system/session-runtime/src/manifest.rs`](../../../crates/system/session-runtime/src/manifest.rs) *(historical citation)* <!-- doc-audit: historical-link --> — `WorkerKind` enum (currently `None`-only); `GraphSessionManifest.active_workers: Vec<WorkerKind>` is already in place.
- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) §5.9 — single-process logical daemon framing. v0 runs in-process; v1+ may relocate to a separate service. The trait is designed so the relocation doesn't rewrite session semantics.

---

## 1. Goal + done conditions

**Goal:** A `SessionServiceRunner` contract that:

1. Names the worker classes a session can declare (`WorkerKind`).
2. Reports each worker's current state (`WorkerStatus`).
3. Lets the host start, stop, and enumerate workers per session.

The trait is portable (lives in `system/session-runtime`, wasm-clean) so a future split into a separate runner service crate doesn't have to re-export everything.

**v0a done when (this turn):**

- `session_service_runner` module in `system/session-runtime` exposes `SessionServiceRunner` trait + `WorkerHandle` + `WorkerStatus` + `WorkerState` + extended `WorkerKind` variants.
- `NullRunner` no-op implementation lets `HostRoot` thread a runner reference through even before any real worker exists.
- An `InMemoryRunner` test double records start/stop calls — basis for v0b real-worker tests too.
- Trait tests cover start → stop → list lifecycle.

**v0b done when (per worker):**

- A concrete runner implementation (e.g. an `InProcessRunner` that owns Tokio tasks or threads) takes `NullRunner`'s place in `HostRoot`.
- Each real worker (FetcherPool first since intelligence work depends on it) implements its `start`/`stop` against the runner.
- `GraphSessionManifest.active_workers` actually steers which workers spin up when a session opens.

## 2. Trait shape

```rust
pub trait SessionServiceRunner {
    fn start_worker(
        &mut self,
        session_id: SessionId,
        kind: WorkerKind,
    ) -> Result<WorkerHandle, WorkerStartError>;

    fn stop_worker(&mut self, handle: WorkerHandle) -> Result<(), WorkerStopError>;

    fn workers_for(&self, session_id: SessionId) -> Vec<WorkerStatus>;
}
```

`WorkerHandle` is an opaque newtype (`u64`) the runner mints — callers don't construct them. `WorkerStatus { session_id, handle, kind, state }` mirrors the brief's signature.

Error types are unit-like enums for v0a (`WorkerStartError::Unsupported`, `WorkerStartError::AlreadyRunning`, etc.) so a runner can refuse a request without freeform strings. Add fields when a concrete refusal needs context.

## 3. WorkerKind variants

Today: `WorkerKind::None` (placeholder).

v0a additions (each a placeholder variant; the brief §5.7 names these as the first real worker classes):

```rust
pub enum WorkerKind {
    None,
    FetcherPool,
    Embedder,
    Indexer,
    IntelligenceSignalProducer,
}
```

Adding variants now is honest because:

- They're documented in the framing brief as the v1 worker set; the names are stable.
- The manifest already has `active_workers: Vec<WorkerKind>` so persisted manifests forward-compatibly serialise the new variants (default field tolerates unknown serde variants on read; current implementations write only what they emit).
- A `WorkerKind` value doesn't imply an implementation exists — the runner returns `Unsupported` if it can't service the kind.

## 4. Why an in-memory test runner

The `InMemoryRunner` (in `#[cfg(test)]` plus exported for downstream test crates) tracks start/stop calls and ledgers a worker handle map. Two roles:

1. v0a contract tests verify the trait's expected lifecycle without needing a real worker.
2. v0b real runners can share the test-double shape — workers are themselves testable against `InMemoryRunner` before being wired to a process/thread.

## 5. Test plan

**v0a (this turn):**

- `InMemoryRunner` start returns a stable handle; stop releases it; `workers_for` reflects current set.
- Stopping a stale handle is `Err(WorkerStopError::UnknownHandle)`, not a panic.
- Starting a kind the runner doesn't support returns `Err(WorkerStartError::Unsupported)`.
- Two sessions can run the same kind side-by-side — handles are per-session-unique.
- `NullRunner::start_worker` always errors `Unsupported`; `workers_for` is empty.
- `WorkerKind` serde round-trip works for every variant (so manifests can carry them).

**v0b (per worker):**

- Each real implementation has its own test surface; the trait test stays in v0a.

## 6. Manifest integration

`GraphSessionManifest.active_workers` is already a `Vec<WorkerKind>` with `#[serde(default)]`. v0a doesn't change the manifest. v0b adds a host-side reconcile step: on session open, the host walks `active_workers` and calls `runner.start_worker(session_id, kind)` for each. On session close (or app exit), the host iterates `runner.workers_for(session_id)` and stops every entry.

## 7. Process boundary forward-compat (§5.9)

The trait is intentionally synchronous and accepts owned ids — both choices keep it remotable. A future `RemoteRunner` impl over IPC matches this shape directly. Errors are values not panics for the same reason: remote-edge failures must be expressible without unwinding across the IPC boundary.

## 8. Open questions

1. **Async vs sync.** v0a keeps the trait synchronous. Real workers are long-running; the runner likely orchestrates async tasks internally but exposes a sync trait to the host (start returns once the task is spawned, not when it completes). When/if a worker's start has to await something, switch the trait to `async fn` or expose `start_async`. Defer until a real worker hits the constraint.
2. **Per-worker config.** Some workers want config (e.g. fetcher concurrency limits). v0a's `start_worker(session_id, kind)` doesn't carry config; v0b adds either per-kind config in the manifest or a `WorkerConfig` enum parameter to the start signature. Decide when a real worker needs more than its `WorkerKind`.
3. **Crash / restart semantics.** What happens when a worker dies? `WorkerStatus::state` can grow `Crashed { error }`; the runner decides restart policy. v0a leaves this to v0b — the test runner doesn't model crashes.
