# document-host

The native side of the `document-core` WIT world. Owns a Wasmtime engine and a
per-instance `Store<ScriptHost>`, backs the `log` / `caps` / `net` /
`document-host` imports over a live genet `ScriptedDom`, and drives the per-turn
`handle-event` contract with atomic, revision-checked apply.

The WIT lives at `crates/script/wit/world.wit`, not in this crate, so the
browser (jco) path consumes the same contract. Exports and imports are bound
async (`exports: { default: async }`), so a turn runs on a fiber and the
`net.fetch` import can suspend it without blocking the host thread.

A member of the mere workspace, edition 2021, `publish = false`. It declares
`rust-version = "1.93.0"` because wasmtime 45 requires it; the workspace
toolchain (`rust-toolchain.toml`) is 1.97.1.

## Public API

`dom_view` and `runtime` are public modules, so their items are reached through
them (`document_host::dom_view::snapshot`). `script`, `capabilities`, `host`,
and `net` are private and glob re-exported, so their items are reached at the
crate root (`document_host::DocumentScript`).

| Item | Module | What it is |
| --- | --- | --- |
| `DocumentScript` | `script` | A live script attached to a caller-provided `ScriptedDom`: `attach`, `deliver_event`, `dom`, `revision`, `detach` |
| `Quota` | `script` | `mem_bytes`, `epoch_deadline_ticks`, `max_fetches_per_turn`. Default 64 MiB, 200 ticks (5 ms each), 32 fetches |
| `TurnOutcome` | `script` | `Applied(u64)`, `Conflict(u64)`, `UnknownNode(u64)`, `Refused(String)` |
| `Grant`, `CapPermission` | `capabilities` | Per-interface `Allow` / `Prompt` / `Deny` over `log`, `document`, `net`. Built by `Grant::from_authority`, `allow_all`, or `deny_document`; `granted_names` is what `caps.granted()` reports |
| `run_guarded`, `Guarded` | `capabilities` | One turn under an epoch deadline plus a `StoreLimits` memory cap; reports `Completed` or `Trapped(String)` |
| `precompile_to_cwasm` | `capabilities` | Ahead-of-time compile a component to `.cwasm` bytes |
| `ScriptHost`, `TurnLog`, `run_turns` | `host` | The store data, and a driver that instantiates over a seeded DOM and runs a list of `(kind, payload)` turns |
| `NetFetcher`, `NetResponse` | `net` | The blocking network backend an embedder injects at `attach`; `fetch` returns `Err(String)` to the guest as `result::err` |
| `dom_view::{snapshot, snapshot_subtree, apply}` | `dom_view` | Project a `ScriptedDom` into the WIT `document-view`, and apply a mutation batch back |
| `DocumentScriptRuntime` | `runtime` | `register-mod-loader`'s `WasmModRuntime` implemented here: `new(allowed)`, `permissive()`, `active_ids`, `is_active` |

Grants are enforced by linking: a denied interface is not added to the linker,
so a component that imports it fails to instantiate. `caps` is always linked
because it reports the grant rather than being one. `net` is additionally gated
per instance by an origin allowlist (exact hosts or `*.suffix` globs, empty
denies everything) and by the per-turn fetch cap.

## Layout

```text
src/lib.rs        bindgen, the seeded DOM, module wiring
src/script.rs     Quota, TurnOutcome, DocumentScript
src/capabilities.rs  Grant model, grant-scoped linking, engine + component loading
src/host.rs       ScriptHost, the host-trait impls, full_linker, run_turns
src/net.rs        NetFetcher seam and origin matching
src/dom_view.rs   ScriptedDom <-> WIT document-view
src/runtime.rs    the WasmModRuntime bridge
guest/            the document-core guest (standalone workspace -> wasm32-wasip2)
guest-bomb/       a deliberately misbehaving guest for the quota tests
tests/            caps, document_script, eight_turns, grants, guarded, runtime
```

## Test

`build.rs` builds both guests with `cargo build --target wasm32-wasip2
--release` in their own workspaces, so `cargo test` is enough on a checkout that
has the `wasm32-wasip2` target installed. If the target is missing the build
warns and the affected tests fail naming the command to run. Artifact paths can
be overridden with `DOC_HOST_GUEST_WASM` and `DOC_HOST_BOMB_WASM`.

## Dependencies

`wasmtime` 45 and `wasmtime-wasi` 45; `genet-scripted-dom` and `layout-dom-api`
from the genet git repo (`branch = "main"`); `servitor` for the
`AuthorityProvider` / `Subject` types `Grant::from_authority` reads;
`register-mod-loader` by path for the `WasmModRuntime` trait; `pollster` to
drive the async exports from the sync surface. Tests use `tokio`.
