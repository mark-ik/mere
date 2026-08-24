# muniment OPFS probe

The executable half of the
[redb-over-OPFS feasibility plan](../../crates/eidetic/muniment/design_docs/2026-08-22_redb_opfs_feasibility_plan.md):
can redb 4.2 run over an OPFS sync-access handle as a muniment backend, inside
a dedicated browser worker, keeping redb's storage contract and recovery
guarantees, without an `unsafe` thread claim and without browser authority
leaking above the seam?

Production `muniment` is untouched. This is a standalone Cargo workspace, like
`ports/distillery/probe`, so its redb 4.2 pin and wasm-bindgen pin never reach
mere's product graph; muniment is consumed as a path dependency with its own
`redb` (2.x) feature off.

## What is where

| Path | Role |
|---|---|
| `src/opfs_backend.rs` | redb `StorageBackend` over one sync-access handle, kept in a **realm-qualified** worker-local registry (the `Send + Sync` answer, fail-closed across threads), plus JS safe-integer guards and staged-creation promotion via `move()`. |
| `src/fault.rs` | Fault injection around any backend: short/failed writes, failed sync and resize, quota, and cuts (process death inside write `k` / before resize `k`). Native sweep tests. |
| `src/churn.rs` | The generation workload and the recovery invariant every reopen is checked against. |
| `src/redb_backend.rs` | muniment `Backend` over a redb 4.2 `Database` (the production adapter on 4.2). |
| `src/workload.rs` | Representative muniment workloads over the `Backend` trait, with a content digest as the cross-backend oracle. |
| `src/worker.rs` | The wasm body: one `run_command` per probe command. |
| `src/bin/fixture.rs` | Native: write and verify the lane-5 portability fixture. |
| `web/` | The page (`probe.js`) that drives workers, kills them, coordinates a second tab, reloads itself, and assembles the receipt. |
| `run-browser.mjs` | Playwright driver: runs the lanes in a named engine (`--engine firefox`), opens the second tab lane 4 needs, verifies the browser-written database natively, and writes a per-engine receipt. |
| `receipt-sink.py` | Accepts the page's POSTed receipt and exported database when driving the page by hand. |
| `receipts/` | Dated receipts that substantiate a boundary. Each covers **one engine**. |

## Run

Native halves first (fault sweep, invariant, workloads, adapter):

```powershell
$env:CARGO_TARGET_DIR = 'C:\t\muniment-opfs-probe'
cargo test --manifest-path ports/muniment-opfs-probe/Cargo.toml
```

Browser halves, from the mere root:

```powershell
ports/muniment-opfs-probe/run-probe.ps1
```

This builds the wasm, writes `fixtures/portability.redb` natively, runs
wasm-bindgen 0.2.126 (the CLI on this machine; pass `-WasmBindgen` for another
path), and serves the mere root on port 8733. Open the printed URL in a headed
Chromium and press **Run every lane**, or drive it:

```js
await window.munimentOpfsProbe.runAll();
window.munimentOpfsProbe.receipt();
window.munimentOpfsProbe.exportedBase64(); // lane 5: the browser-written file, for `fixture verify`
```

With `python receipt-sink.py` running beside the static server (port 8734),
`postReceipt("<date>_<name>.json")` writes the receipt under `receipts/` and
`postExport("browser-export.redb")` writes the lane-5 export under
`fixtures/`, where `cargo run --bin fixture -- verify fixtures/browser-export.redb fixtures/portability.json 8`
checks it natively.

Lane 4 reloads the page once (the reload step) and resumes from
`sessionStorage`; lane 4 also opens a second tab, which a popup blocker can
refuse (recorded as `two_tabs: "blocked"`, not a failure).

### A second engine

```bash
npm i playwright && npx playwright install firefox
```

```bash
PLAYWRIGHT_MODULE=/abs/path/to/node_modules/playwright node run-browser.mjs --engine firefox --lanes 1,2,3,4,5,6 --out receipts/firefox.json
```

`--engine` takes `chromium`, `firefox` or `webkit`; `--headed true` to watch.
Each run writes its own receipt, because a receipt covers one engine.

**WebKit cannot run this probe.** Playwright WebKit 26.5 exposes no
`navigator.storage` at all — no OPFS, in page or worker — so every lane fails
at the same call and the runner records an `outcome: "unsupported"` receipt
with a capability probe instead. This is a property of that WebKit build, not
of Safari (which has had OPFS since 15.2). It does **not** substitute for
Safari coverage.

## Provenance

The probe is untracked while it is a probe, so a receipt cannot be identified
by a commit alone. `run-probe.ps1` bakes three SHA-256 hashes into the wasm —
`probe_source_sha256` (every probe file bar generated output and receipts),
`muniment_source_sha256` (the compiled-in path dependency, whose lockfile
entry is only a version), and `cargo_lock_sha256` (the lockfile whole,
sources and checksums included) — and the page adds SHA-256 of the wasm,
`probe.js` and `worker.js` it actually loaded. Compare
`environment.build.probe_source_sha256` across receipts to know two runs used
the same code.

The build runs from a **neutral working directory** (`-NeutralDir`, default
`C:\t`) with `--manifest-path` and `--locked`. Cargo discovers
`.cargo/config.toml` from the working directory, so building from outside
`Code/` avoids the machine-local `[patch]` table whose unused entries
otherwise churn the lockfile — which means `--locked` holds and the hashed
lockfile is provably the one that was used. `run-browser.mjs` does the same
for the native verify (`NEUTRAL_DIR`).

## Claim boundary

The fault sweep models worker termination as a process kill: every write
before the kill is in the browser's file, the fatal write may be torn, nothing
after it exists. Power loss underneath `flush()` is the browser's promise and
is out of reach from inside a worker. Termination trials kill the worker at
real random points inside real commits; the fault sweep cuts deterministically
at every write and resize index over the real OPFS handle.

Lane 6 proves **staging isolation** at every real write/resize/sync index (the
sweep is derived from the worker's own storage-call counters, not guessed).
Atomic **promotion is not established**: the trials are
*promotion-**boundary*** kills — the harness terminates the worker around
`move()`, but cannot show a kill landed inside it, because the terminated
command also spans the post-move checks. Only atomic outcomes have been
observed, which is evidence; `move()` carries no spec crash guarantee.

`IndexedDbRangeBackend` declares an **ASCII key contract** and errors on
anything else, on **every key-bearing operation** — not just `scan`/`list`.
Enforcing it only at the range query was not enough: a non-ASCII key admitted
through `put` or `apply` sits in the store and surfaces through `list("")`,
which has no bounds to validate, in IndexedDB's UTF-16 order. `apply`
prevalidates the whole batch so a refusal stays all-or-nothing. This is not
fussiness: IndexedDB orders strings by UTF-16 code unit while
`muniment::Backend::scan` is specified in Rust's code-point order, and the two
disagree outside the BMP, so no range query can honour the contract for such
keys. See `src/idb_keys.rs` (native tests) and lane 5's `ascii_contract`
(browser test).

Read-performance numbers must cite the `indexed_db_range` backend, not
`indexed_db`. The latter is muniment's shipping adapter, which fetches every
key and filters in Rust; comparing redb against it measures the adapter as
well as the engine. Both are in every receipt. All of it holds **under
muniment's current `Backend` contract** — one transaction and one `get()` per
key — not against IndexedDB's ceiling.

Run engines **sequentially, on as quiet a host as you can get**. Two browsers
benchmarking at once contend badly enough to make the ratios unusable — and
sequential is not sufficient by itself: a Playwright browser killed mid-run
leaks its whole process tree (Playwright chromium is `chrome-headless-shell`,
not `chrome`), and strays from an aborted run slowed a later fault sweep ~5×.
Before a benchmarking run:

```powershell
# Playwright's browsers ONLY — filter by path. Playwright's Firefox and your
# own share the process name `firefox`, so a name-only kill closes your
# browser too. (Learned the hard way: an earlier version of this command was
# `Get-Process chrome-headless-shell,firefox,node | Stop-Process -Force`,
# which cannot tell the two apart and would also kill unrelated `node`.)
Get-Process chrome-headless-shell,firefox -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -like '*ms-playwright*' } |
  Stop-Process -Force
```

To check what that would reap before running it, swap `Stop-Process -Force`
for `Select-Object Id,Path`.

Reaping strays is necessary but not sufficient on a daily-driver machine:
ordinary desktop applications alone can move lane timings several-fold.
**Quote worst-observed ratios, not just medians.** The two IndexedDB backends
share identical *write* code, so their write phases are a built-in control on
how much variation the host is injecting — it has been seen as high as 2.44×
between identical code. Every summary row carries `envelopes` with the
observed min/max per backend, the worst-observed ratio, and whether every
repeat favoured the same side. A median that a single outlier can move 7× (it
happened: 48.8× median, 6.7× worst) is not a number to put in a decision.

Note that lane 3 prints nothing between its start and finish lines; a long
silence there is normal, not a stall.

Each receipt covers **one engine**. Safari and WKWebView have never been run.
