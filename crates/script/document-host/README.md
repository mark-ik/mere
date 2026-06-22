# document-host

The DocumentScript component host (plan §11) — the native side of the
`document-core` WIT world. Owns a Wasmtime engine + a per-instance `Store`, backs
the `log` and `document-host.inspect` imports, and drives the per-turn
`handle-event` contract (§10.2) with atomic, revision-checked `apply` (§10.3).

**P2.0 status (green).** The proven probe host, transplanted into a real library,
backed by an in-memory `Doc`. Exports are invoked via `call_async`
(`exports: { default: async }`) — the fiber foundation for the future suspending
`fetch` import (§11.7-7); P2.0 has no async import, so turns never suspend yet.

Standalone for now (own `[workspace]` + a 1.93 toolchain pin) so it does not pull
the mere workspace onto rustc 1.93 before P2.5 folds it in (plan §11.7-1).

## Layout

```text
wit/world.wit        the mere:script@0.1.0 document-core contract
src/lib.rs           the host: Doc model, ScriptHost, the imports, run_turns
guest/               the shipped document-core guest (standalone; -> wasm32-wasip2)
tests/eight_turns.rs the end-to-end crate test
```

## Test

```sh
cd guest && cargo build --target wasm32-wasip2 --release && cd ..
cargo test
```

The test drives eight turns: four id-targeted mutations apply (revision 0→4), a
scoped `subtree` inspect runs as a no-op, and the stale-revision / unknown-node /
declined paths are exercised; the final tree is asserted.

## P2 roadmap (plan §11.6)

P2.0 (this) → P2.1 swap `Doc` → serval `ScriptedDom` → P2.2 engine config (epoch +
StoreLimits) → P2.3 linker / capability policy → P2.4 `WasmModRuntime` bridge →
P2.5 meerkat wiring + workspace fold-in (MSRV bump) → P2.6 AOT. The sync-WIT
`fetch` import (fiber async, §11.7-7) lands as a later step on this same
`call_async` foundation.
