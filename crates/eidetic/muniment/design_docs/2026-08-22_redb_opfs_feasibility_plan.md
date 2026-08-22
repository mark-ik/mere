# redb over OPFS Feasibility Plan

**Date:** 2026-08-22 (revised the same day after review)
**Status:** **Two-engine, single-threaded feasibility strongly evidenced;
adoption and crash-atomic creation remain open.** Lanes 1–6 pass on native,
Chromium 151 and Firefox 153 (lane 7) from a byte-identical harness. Safari
and WKWebView are **not run**. Production muniment is unchanged.

This plan has been corrected four times — three after review found it
overclaiming, once when the benchmark's own internal control failed. Every
round is recorded in place rather than edited away, newest first:

- **Fourth round.** A built-in control (the two IndexedDB backends share
  identical *write* code, so their write phases measure only error) put the
  measurement noise floor at **2.44× on Chromium, 1.44× on Firefox**. Every
  comparison is now filtered against it; redb's read advantage clears it by
  more than an order of magnitude, and the third round's adapter-overhead
  figure did not and is withdrawn (§5.13). A genuinely idle host turned out
  not to be obtainable here (§5.10).

- **First round.** It claimed COMPLETE and recommended against adoption from
  a totals-based benchmark reading that inverted the result for read-heavy
  work (§5.10); the receipt did not identify its own build (§5.3b); the
  `Send + Sync` fail-closed claim was false as written (§5.2); and the
  creation-atomicity remedy was hypothetical (§5.4b).
- **Third round.** "Confirmed killed in flight" was not established by the
  harness (§5.4b); the aggregate provenance hash was stale because it was
  taken over inputs the same run then changed (§5.3b); the two engines
  benchmarked *concurrently* on one host, so the precise ratios are not
  decision-grade (§5.10); and the range adapter's prefix bound was wrong under
  UTF-16 ordering, exposing a genuine seam-level conflict between muniment's
  Rust-order `scan` contract and IndexedDB's UTF-16 order (§5.13b).
- **Second round.** The staged-creation lane did not test what it claimed —
  its sweep silently truncated to 40 writes and atomic promotion was never
  exercised (§5.4b); provenance still missed the compiled-in muniment source
  and mislabelled SHA-256 as blake3, and the `--locked` workaround was
  unnecessary (§5.3b); `close()` still failed *open* across realms (§5.2);
  the native verify skipped its digest check on the very route it guards
  (§5.8b); and the read benchmark measured muniment's IndexedDB adapter rather
  than IndexedDB (§5.10, §5.13).

The pattern across all three rounds is the same and worth naming: **each
defect made a result look stronger than it was, and none was visible from the
receipts alone** — every one needed reading the harness against its own
claims. Twice a claim was refuted by evidence already sitting in the receipt
(the phase data, the truncated trial list). The lanes below now say what they
measured, not what they were meant to measure.

**The withdrawn adoption recommendation is in §7.**

Receipts: [`2026-08-22_chromium_full.json`](../../../../ports/muniment-opfs-probe/receipts/2026-08-22_chromium_full.json),
[`2026-08-22_firefox.json`](../../../../ports/muniment-opfs-probe/receipts/2026-08-22_firefox.json).
The superseded first receipt
([`2026-08-22_redb_opfs_probe.json`](../../../../ports/muniment-opfs-probe/receipts/2026-08-22_redb_opfs_probe.json))
is kept as history and **should not be cited**: it predates the harness in the
tree and its `browser_to_native` field still reads "pending native verify".

## 1. Research question

Can redb use OPFS while preserving its storage contract, recovery guarantees,
and muniment semantics, without unsafe thread claims or browser-specific
authority leaking upward?

Bounded deliberately: this is a feasibility probe, run before and apart from
the production backend selection. The output is the probe, recovery fixtures,
browser receipts, benchmark results, and an adopt/fork/reject decision. The
first implementation question is whether redb's `Send + Sync` boundary can be
satisfied honestly; if it cannot, the probe stops there.

## 2. Scope boundary

- `crates/eidetic/muniment/src/` is not touched. Production `RedbBackend`
  stays on redb 2; production `IndexedDbBackend` is the browser store today.
- The executable harness lives in [`ports/muniment-opfs-probe/`](../../../../ports/muniment-opfs-probe/README.md),
  a standalone Cargo workspace (the `ports/distillery/probe` posture), so its
  redb 4.2 pin and wasm-bindgen pin never enter mere's product graph. It
  consumes muniment as a path dependency with muniment's own `redb` feature
  off, so the two redb majors never meet.
- Target: `wasm32-unknown-unknown`, wasm-bindgen, **no `atomics` feature**.
  The open-web lane cannot use SharedArrayBuffer (COOP/COEP versus
  third-party content; see the
  [cross-platform parallelism strategy](../../../../design_docs/mere_docs/research/2026-06-19_cross_platform_parallelism_strategy.md)),
  so the worker is single-threaded by construction.

## 3. Decisions taken while founding the probe (for Mark's review)

Each of these had more than one defensible answer. They are recorded here so
the picture in the tree matches the picture in the code; any can be reversed.

1. **The `Send + Sync` answer: a worker-local handle registry.** The value
   redb owns (`OpfsBackend`) holds no JS value: a **realm-qualified** token
   into a `thread_local!` table where the `FileSystemSyncAccessHandle` lives,
   a path string, and atomic counters. It is `Send + Sync` by construction
   with zero `unsafe`; a compile-time assertion guards it. A call from any
   other thread fails with `io::ErrorKind::NotConnected`, not UB.
   *(Corrected 2026-08-22: this decision originally used a bare `usize`
   index, for which that last sentence was **false** — another thread's slot
   `n` is a different file. The realm qualifier is what makes it true; see
   §5.2.)* The alternative also
   compiles: wasm-bindgen declares `unsafe impl Send/Sync for JsValue` under
   `cfg(not(target_feature = "atomics"))` (present in every pinned version
   here, 0.2.120 through 0.2.127), so a struct holding the handle directly
   would satisfy the bound too. The registry was chosen because the claim is
   then ours by construction rather than upstream's by assertion, and because
   nothing above the backend file can reach the browser handle: the only
   browser authority redb ever sees is six storage calls.
2. **License: MPL-2.0.** The 2026-08-21 license posture ruling makes MPL-2.0
   the default for everything Merely owns; no manifest has been swept yet, so
   every other `ports/` manifest still reads `MIT OR Apache-2.0`, but two
   probe crates and signalman already carry MPL-2.0. A crate founded after the
   ruling takes the ruling.
3. **wasm-bindgen pinned to 0.2.126** (js-sys/web-sys 0.3.103,
   wasm-bindgen-futures 0.4.76), the CLI installed on this machine and
   graphshell-web's pin. The distillery probe's 0.2.122 pin was a wgpu 30
   workaround that does not apply here.
4. **Web Locks are driven from the page, not from Rust.** web-sys gates
   `LockManager::request` behind `--cfg=web_sys_unstable_apis`. The
   sync-access handle's own exclusivity is the hard lock and is exercised from
   Rust; the cooperative takeover protocol (lane 4, two tabs) uses
   `navigator.locks` from the harness JS. For production this is a decision to
   make explicitly: accept the cfg, or keep lock orchestration host-side.
5. **This plan lives in muniment's in-crate `design_docs/`**, as named in the
   brief, beside the founding proposal; the repo-level `DOC_README.md` carries
   the pointer (DOC_POLICY §7).
6. **A receipt pins its own build, not a commit.** The probe stays untracked
   while it is a probe, so receipts carry SHA-256 of the probe tree, of the
   compiled-in muniment source, and of the whole lockfile (built `--locked`
   from a neutral directory), plus SHA-256 of the wasm and both JS files the
   page actually loaded (§5.3b). The alternative — commit the probe so the
   commit identifies it — is available and is Mark's call; I have not
   committed anything.
7. **A second engine is a lane, not a footnote.** Firefox runs through
   Playwright (`run-browser.mjs`) with its own receipt rather than being
   folded into the Chromium one, so a divergence stays visible (§5.11).
8. **Termination is modeled as a process kill, not a power loss.** A
   terminated worker loses nothing the browser already accepted; the fatal
   write may be torn; nothing after it exists. That is what the fault sweep's
   `cut_at_write` / `cut_at_set_len` reproduce deterministically at every call
   index, natively and over the real OPFS handle. What `flush()` buys against
   OS-level power loss is the browser's promise and cannot be tested from
   inside a worker; it is recorded as a claim boundary, not a result.

## 4. Lanes

Done-conditions, not time estimates. Any unrecoverable database is a stop
condition for the whole probe.

### Lane 1 — WASM viability

- Compile redb 4.2 for the browser target; run transactions in a worker
  against `InMemoryBackend`; inventory filesystem, atomics, and threading
  assumptions.
- **Done when** redb transactions run in the worker, or there is a precise
  upstream blocker.
- **Status: passed 2026-08-22** (native proof; the in-worker repeat is
  `ProbeCommand::InMemorySmoke`, lane 1 of the page). See Findings §5.1.

### Lane 2 — OPFS storage device

- Implement the minimum redb `StorageBackend` over a sync-access handle: map
  `len`, `read`, `set_len`, `sync_data`, `write`, `close`. Keep the handle
  worker-local. No shared memory, no `unsafe Send/Sync`.
- **Done when** one database creates, commits, closes, and reopens from OPFS
  (`OpfsRoundTrip` in one worker, `Reopen` in a fresh worker: integrity check
  passes, invariant holds at the committed generation, repair not invoked
  after a clean close).
- **Status: PASSED 2026-08-22 on both engines.** `src/opfs_backend.rs`; the
  reopen is clean (repair not invoked) on Chromium and Firefox alike.

### Lane 3 — Recovery and failed writes

- Inject termination around writes, resize, and flush; inject short writes,
  quota errors, and failed flushes. Verify every reopen yields either the
  preceding commit or the completed commit.
- Two halves, one invariant (`churn::verify`): generation `g` named by the
  counter, all of `g` present and correct, nothing of `g + 1` visible, nothing
  older than `g − 1` surviving, and `g ∈ {completed, completed + 1}`.
  - **Native sweep** (`cargo test`): die inside every write (fatal write
    landing none / half / all of its bytes) and before every resize, in both
    commit modes; fail every write, short-write every write, fail every sync,
    fail every resize, and every 4 KiB quota ceiling up to the final size.
  - **Browser**: the same deterministic sweep over the real OPFS handle
    (`Fault`), plus real `worker.terminate()` at random points inside real
    commits (`Churn` + kill), each followed by `Reopen` from a fresh worker.
    Termination outcomes are classified `preceding` / `completed` / `later`
    (messages lagged) / `regression` (a reported commit lost) / `unopenable`;
    the last two are stop conditions.
- **Done when** every reopen satisfies the invariant. **Any unrecoverable
  database stops the probe.**
- **Status: PASSED 2026-08-22.** Native sweep §5.5 (964+ cut and error trials,
  0 unrecoverable). Browser fault sweep §5.5 (707 trials over the real OPFS
  handle, 0 unrecoverable) and termination trials §5.5b (20 trials, forced and
  yielding, every reopen at the preceding committed generation, 0 regressions).
  The one non-recovery is the uninitialized-creation case §5.4, which loses no
  committed data; the atomicity requirement it creates is an adoption item.

### Lane 4 — Ownership and lifecycle

- Open from two tabs; kill the owning tab and worker; verify lock release and
  controlled takeover; exercise reload (background suspension is recorded,
  not forced).
- Mechanism: `createSyncAccessHandle()` is exclusive per file; a second
  worker or tab gets `NoModificationAllowedError` (mapped to
  `io::ErrorKind::WouldBlock` at the backend, reported by name in the
  receipt). Death of the holding worker or tab releases it; the page measures
  time-to-reopen by polling `TryOpen`. Controlled takeover: the holder tab
  holds `navigator.locks` `muniment-opfs:<path>`; the owner asks over a
  `BroadcastChannel`; the holder closes its database and releases the lock;
  the owner opens.
- **Done when** a second writer is reliably refused and ownership recovery
  (after kill, after clean close, after tab close, after reload) is
  deterministic, with latencies recorded.
- **Status: PASSED 2026-08-22.** Within one tab, a second worker on a live
  handle was refused `NoModificationAllowedError`; after the holder worker was
  killed the file reopened in 49 ms, and after a clean close it reopened with
  repair not invoked. Across two tabs (a real second tab, coordinated over
  `BroadcastChannel`; see the finding below on `window.open`), the second
  writer was refused, a controlled release let the owner take over in 67 ms,
  the holder re-took the handle and refused again, and closing the holder tab
  released it in 92 ms. After a full page reload the reload database reopened
  in 470 ms. Every takeover reopen invoked repair (a killed/closed holder is an
  unclean shutdown) and every one was sound.

### Lane 5 — Portability and performance

- Move one fixture between a native redb file and browser OPFS both ways:
  native `fixture write` → browser `Import` → `Reopen` + `Digest` against the
  manifest → `Churn` three more generations → `Export` → native
  `fixture verify … 8`.
- Compare representative muniment workloads against IndexedDB through one
  `Backend` trait: small slots, ordered log, atomic batches, large blobs; on
  memory, redb-on-OPFS, and `IndexedDbBackend`. The content digest after each
  workload must agree across backends. Record per-phase times, storage usage
  delta, and redb's storage-call counters.
- **Done when** native and browser reopen identical committed contents in
  both directions and the performance boundary is understood.
- **Status: PASSED 2026-08-22.** Round-trip §5.9; performance §5.10.

### Lane 6 — Staged creation (added 2026-08-22 after review)

- Lane 3 found that an interrupted *creation* leaves a file redb refuses
  (§5.4). The remedy proposed there — build under a staging name, promote by
  rename — was hypothetical, and a proposed remedy is not a result.
- Build at `<path>.staging`, commit, then promote onto `<path>` with
  `FileSystemFileHandle.move()` where the browser has it (feature-detected
  through `Reflect`, since it is a vendor extension outside the core WHATWG
  IDL and web-sys has no binding), falling back to copy + delete otherwise.
  Cut the creation at every write and resize index, fail its syncs, and cut in
  the promotion window itself.
- **Done when**, after every cut, the final path is either **absent** or a
  **sound database at the full committed generation** — never an unopenable
  stub — and any staging debris is discardable.
- **Status: staging isolation PASSED on both engines** (127 trials each, the
  sweep derived from the worker's real storage-call counts, 0 unopenable
  stubs). **Atomic promotion is NOT established**: 12 promotion-*boundary*
  kills per engine produced only the two atomic outcomes, but the harness
  cannot show a kill landed inside `move()`, and `move()` carries no spec
  crash guarantee. Evidence, not a result. §5.4b, §7.2 item 3.

### Lane 7 — Second engine (added 2026-08-22 after review)

- The first receipt was one host: Electron 42 / Chrome 148. That proves the
  engine arrangement in Chromium and settles nothing about Firefox, Safari or
  WKWebView, whose OPFS, Web Locks and worker-lifecycle behaviour are separate
  implementations. The `window.open` finding (§5.8) is itself an example of
  host variation changing the harness.
- Run the core lanes (create/reopen, ownership, portability, staged creation,
  and the benchmark) under a second independent engine via Playwright
  (`run-browser.mjs`), producing its own receipt.
- **Done when** a second engine has its own receipt, and any divergence is
  recorded rather than averaged away.
- **Status:** §5.12. Safari/WKWebView remain **not run** — no Apple host is
  driveable from this machine — and native mobile direct-file is a separate
  proof entirely.

## 5. Findings

### 5.1 redb 4.2.0 on wasm32-unknown-unknown (lane 1)

- Compiles with default features, no patches. Its `std` build uses
  `std::sync::{Mutex, RwLock, Condvar}` (fine single-threaded) and
  `std::thread::panicking`; the thread spawns in the tree are tests only. The
  file backend falls back to a `Mutex<File>` implementation on targets that
  are neither unix, windows, nor wasi: it compiles and is simply unused.
- A raw module (no wasm-bindgen) instantiates with **zero imports**: redb pulls
  no host syscalls on this target. Under Node 24, two commits (the second
  growing the store with 64 × 64 KiB values), a point read, and a range scan
  ran in 57.6 ms.
- `StorageBackend` in 4.2: `'static + Debug + Send + Sync`; `len`, `read`,
  `set_len` (zero-fill on growth), `sync_data`, `write`, defaulted `close`.
  4.2.0 calls `close()` on a failed open and on an I/O error while dropping,
  so the OPFS exclusivity ends with the `Database` on every path.
- 4.2 poisons the database after an error part-way through commit and refuses
  further write transactions until reopened (repair on reopen). The probe
  records `further_commit_refused` to confirm this over OPFS.
- Upstream's own `tests/crash_consistency.rs` uses the same wrapper-backend
  technique as the probe's `FaultBackend`, which is evidence the trait seam is
  intended for this.

### 5.2 The `Send + Sync` boundary, and the guard it needed

See decision §3.1. Two honest routes exist; the probe takes the one that makes
no claim at all.

**Correction, 2026-08-22.** The first version of this section said a call from
another thread "fails with `NotConnected`". That was **false as stated**, and
the code has been fixed rather than the claim softened. A bare `usize` index
into a `thread_local!` vector is ambiguous across threads: thread B has its
own vector, and slot *n* there is a *different file*, so a backend that
migrated to B would have silently addressed the wrong database. Today's
single-threaded, non-atomics build cannot create that situation, but nothing
made it fail closed if one ever could.

The token is now **realm-qualified**: each thread's registry draws a distinct
id from a process-global `AtomicU64` at first use, the token carries it, and a
lookup whose realm does not match the calling thread's returns
`ErrorKind::NotConnected` — under any build, including a future `+atomics`
one.

**Second correction, same day: `close()` was still failing *open*.** It was
realm-checked, so it could not release another thread's handle — but it then
returned `Ok(())`, reporting success while the exclusive handle stayed open
and the file stayed locked. That is the one place the registry still lied,
and it undercut precisely the future-threads claim the realm qualifier exists
to support. `close()` now returns `NotConnected` naming the leak. Unreachable
in today's single-threaded build; the point is that the guard is only true if
this path reports it.

The same pass added **JS safe-integer guards** on every `u64 ↔ f64` crossing
(`read`/`write` offsets, `set_len`, `get_size`, and the byte counts they
return). `FileSystemSyncAccessHandle` speaks `double`; beyond 2^53−1 a
conversion silently rounds, and on a *write offset* that means writing to the
wrong place. redb never reaches a 9 PB database in a browser, so these are
guards rather than limits, but "the contract is complete" was not true without
them.

Both are in `src/opfs_backend.rs`; the `Send + Sync` compile-time assertion on
the struct still holds, and no `unsafe` was added.

### 5.3 OPFS mapping

| redb call | OPFS | Contract note |
|---|---|---|
| `len` | `getSize()` | f64 → u64 |
| `read(off, buf)` | `read(buf, {at})` | short read → `UnexpectedEof` (redb requires an error past the end) |
| `set_len(n)` | `truncate(n)` | grows zero-filled, as redb requires |
| `sync_data` | `flush()` | the browser's durability point |
| `write(off, data)` | `write(data, {at})` | short write → `WriteZero`; extends the file |
| `close` | `close()` | releases the exclusivity |

DOMException mapping: `QuotaExceededError` → `QuotaExceeded`;
`NoModificationAllowedError` → `WouldBlock`; `NotFoundError` → `NotFound`;
`InvalidStateError` → `BrokenPipe`; `NotAllowedError`/`SecurityError` →
`PermissionDenied`. The exception name rides inside the `io::Error` payload so
receipts report the browser's own word.

### 5.3b Provenance: what a receipt pins, and the `--locked` correction

A receipt has to name the code that produced it. The probe is deliberately
untracked while it is a probe, so the git commit alone does not: the first
receipt recorded only `commit + dirty:true`, and its `environment` block was
demonstrably from an earlier harness revision than the one in the tree. That
receipt has been superseded rather than defended.

**Corrected twice, 2026-08-22.** The first attempt hashed only `src/`,
`web/` and `Cargo.toml` — which left out the compiled-in **muniment path
dependency** (whose lockfile entry is just `0.1.1`, so only its source
identifies it), `run-browser.mjs`, `receipt-sink.py`, and the fixture
manifest. It also hashed *resolved `name version` pairs* rather than the
lockfile, discarding sources and checksums, and it labelled both fields
`blake3` while computing **SHA-256**.

What a receipt now pins:

| Field | Covers |
|---|---|
| `probe_source_sha256` | every probe file except generated `web/pkg/`, receipts and `.redb` fixtures — Rust, JS, manifest, lockfile, runner and sink scripts |
| `muniment_source_sha256` | the path dependency's `src/` and `Cargo.toml` |
| `cargo_lock_sha256` | the lockfile **whole**, sources and checksums included |
| `locked_build` | that the build ran under `--locked` |
| `wasm_sha256`, `probe_js_sha256`, `worker_js_sha256` | what the page actually loaded (SubtleCrypto, page-side) |

Field names now say SHA-256, because that is what they are.

**Third correction: the hash must be taken over *frozen* inputs.** The fixed
version still had two ordering defects, caught when the recorded
`probe_source_sha256` stopped matching the tree that produced it. The
executable hashes — muniment source, lockfile, wasm, `probe.js`, `worker.js` —
all still matched, so the evidence itself was intact and the aggregate was
simply wrong; but an aggregate that drifts for non-behavioural reasons is
worse than none, because it cannot be used to detect the case that matters.
The defects: `fixtures/portability.json` was hashed **before** the same run
regenerated it, and prose files (`README.md`, `.gitignore`) were included, so
editing documentation invalidated the hash of an unchanged build. Now
fixtures are generated **first**, the hash covers **behavioural inputs only**
(no `.md`, no `.gitignore`, no `.redb`, no receipts, no generated `web/pkg/`),
and the build runs after. A mismatch now means the executable build changed.

**And the lockfile workaround was unnecessary.** The earlier claim that
`cargo --locked` "cannot be satisfied in this workspace" was wrong in its
conclusion: cargo discovers `.cargo/config.toml` from the **working
directory**, so running from a directory outside `Code/` with
`--manifest-path` avoids the machine-local `[patch]` table entirely. Verified:
from `C:\t`, `cargo test --locked` (12/12), `cargo build --locked` for wasm,
and `cargo fmt --check` all pass. `run-probe.ps1` now builds, tests and runs
the fixture from a neutral directory with `--locked`, and `run-browser.mjs`
does the same for the native verify — so the lockfile is both honoured and
hashed. (Repo policy still gitignores `Cargo.lock`; that is separate from
whether the build is reproducible.)

### 5.4 Creation is not crash-atomic (lane 3, native sweep)

A cut between redb's initial `set_len` and its first complete header write
leaves a **non-empty, header-less file that redb 4.2 refuses to open**:
`InvalidData: Not a redb database: magic number mismatch`. In the sweep this
is exactly write #1 (any torn amount) and write #2 with nothing landed, in
both commit modes; the equivalent error faults are `fail_write #1–2`,
`short_write #1`, `fail_sync #1`. No commit had completed, so no data is
lost, but redb's error cannot tell this file from a corrupted established
store. The sweep classifies it `uninitialized`, acceptable only when creation
itself was the thing cut; the same shape with any commit completed fails the
sweep. **Adoption requirement**: make creation atomic above redb, for example
create under a staging name, commit once, then `FileSystemFileHandle.move()`
into place, so an existing file always carries a valid first commit. (Native
`Database::create` has the same property; it is a redb contract fact, not an
OPFS one.)

### 5.4b Staged creation: isolation proven, promotion open (lane 6)

The §5.4 remedy is now implemented and crash-tested rather than asserted.
`StagedCreate` builds the database at `<path>.staging`, commits, and promotes
onto `<path>`; the promotion uses `FileSystemFileHandle.move()` when present.

**Both Chromium and Firefox report `atomic_move: true`** — each implements
`move()`, so the promotion is a rename, not a copy. (The fallback path, copy
+ delete, is implemented and is *not* atomic; a receipt records which ran,
because a browser without `move()` gets a weaker guarantee. `move()` is a
vendor extension: it is not in the core WHATWG File System IDL, so two
engines agreeing is encouraging, not a guarantee — Safari is untested.)

**Correction, 2026-08-22 (second review): the first version of this lane did
not test what this section claimed.** Two defects, both now fixed:

1. **The sweep was truncated and the "every write index" claim was false.**
   The page derived the write count from `staging_len`, which is *zero* after
   a successful promotion, so it silently fell back to a hard-coded 40. Both
   earlier receipts stopped at `cut_at_write#40`. The worker now reports its
   own storage-call counters and the sweep is derived from them: **108 writes,
   3 resizes, 15 syncs on Chromium — 127 creation trials, not 51.**
2. **Atomic promotion was never exercised at all.** The only promotion trial
   was `cut_before_promote`, which cuts the moment *before* the rename and
   therefore says nothing about the rename itself. There is now a
   promotion-kill sub-lane: the worker announces and yields immediately
   before calling `move()`, and the page terminates it with staggered
   sub-millisecond jitter, aiming the kill at the rename. (The third review
   established that hitting the rename's *interior* is aimed at but not
   demonstrated — see below.)

Results per engine:

- creation sweep: **127 trials, 0 unopenable stubs at the final path**; every
  trial ended with the final path absent or sound at the full committed
  generation, and staging debris is discardable by name
- **promotion-boundary kills**: 12 trials per engine, only the two atomic
  outcomes observed (staging present / final absent, or staging absent /
  final sound), **0 final-unopenable and 0 both-names-absent**

**What that does and does not establish — corrected, third review.** These
were first reported as "12/12 confirmed killed in flight", which the harness
cannot show. `terminate()` resolving the pending call only proves the kill
landed before the *whole command* returned, and that command spans `move()`
**and** the post-move existence and reopen checks — so a kill may have landed
after the rename completed. They are therefore **promotion-boundary kills**:
the sampled window straddles the rename rather than being confined to it. The
inspection also raced browser state that was still settling; it now polls
both names until two consecutive observations agree (`names_stabilized` in
the receipt) before classifying.

The observed outcomes remain useful evidence: across 24 boundary kills on two
engines, no torn intermediate appeared. But that is *consistent with* a
crash-atomic rename, not a demonstration of one — the interior of `move()` may
simply be too narrow to hit, `move()` carries no spec crash guarantee, and
Safari is untested. §5.4's adoption item is therefore **partly discharged**:
**staging isolation is proven** at every real write, resize and sync index;
**atomic promotion is not**, and stays open in §7.2.

### 5.5 Native sweep results (lane 3, 2026-08-22)

Shape 8 keys × 1 KiB, 12 generations, redb 4.2.0, `SharedMemory` image:

| Mode | Cut trials | Preceding commit | Completed commit | Uninitialized (creation) | Unrecoverable |
|---|---|---|---|---|---|
| single-phase | 464 (154 writes × 3 torn + 2 resizes) | 436 | 24 | 4 | **0** |
| two-phase | 500 (166 writes × 3 torn + 2 resizes) | 469 | 27 | 4 | **0** |

Every failed write, short write, failed sync, failed resize at every call
index, and every 4 KiB quota ceiling up to the final file size: reopenable,
invariant intact, **0 unrecoverable**. Cuts that land in redb's shutdown
writes during `drop` surface no commit error (every generation had already
committed) and reopen at that generation after repair. The same workloads
digest identically on `MemoryBackend` and redb-in-memory.

### 5.5b Browser termination trials (lane 3, 2026-08-22)

Electron 42 / Chrome 148, shape 16 keys × 4 KiB, single-phase commit, one
database churned across all trials (generation 1 → 1682), each trial killing
the worker and reopening from a fresh worker, classified against the OPFS
side file:

| Mode | Trials | Kill inside a commit | Reopened at | Repair | Reopen time | Handle released after kill | Generations after `terminate()` |
|---|---|---|---|---|---|---|---|
| forced (no yield) | 10 | 9 | preceding commit, 10/10 | 10/10 | 24–32 ms | 2065–2094 ms | 154–159 |
| yielding | 10 | 0 | preceding commit, 10/10 | 10/10 | 23–28 ms | 156–188 ms | 1–3 |

No regression (a reported commit lost), no unopenable store, no corrupt
reopen. The forced rows are the mid-commit kills the lane asked for; the
yielding rows give the cooperative-app release latency: **~160–190 ms from
`terminate()` to a second worker being able to take the handle**, which a
host must wait out (retry on `NoModificationAllowedError`) rather than treat
as a second writer.

### 5.6 A killed worker's handle is released asynchronously (lanes 3 and 4)

Observed in the browser on the first termination trial: immediately after
`worker.terminate()`, a fresh worker's `createSyncAccessHandle()` on the same
file was refused with `NoModificationAllowedError: Access Handles cannot be
created if there is another open Access Handle or Writable stream associated
with the same file`. The browser tears the killed worker down and releases
its handle some time after `terminate()` returns. The same-worker path has no
such gap: `Database` drop calls `close()` synchronously, and the fault sweep
(same worker, close then reopen, 707 trials over OPFS) never saw the refusal.
So the refusal **is** the second-writer exclusion lane 4 wants, and the gap
is the "release after kill" latency lane 4 measures; a host that reopens after
a crash must retry on that exception. The harness now records the wait per
trial (`release_wait_ms`) instead of treating it as a failure. The first run
of the harness threw on it and discarded a completed fault sweep; partial
results are now persisted after every sub-step.

### 5.7 `terminate()` does not interrupt a worker inside a synchronous commit loop *on Chromium*

First browser termination trials (4 trials, single-phase commit): after the
page called `worker.terminate()` while the worker reported "committing
generation g", the worker went on committing for **2083–2093 ms** (about 170
more generations of 16 × 4 KiB) and only then died, whereupon its handle was
released. That constant is Chromium's forcible worker-termination delay
(2 s) for a worker that never returns to its event loop; the churn loop is
synchronous wasm over synchronous OPFS calls and never yields, so the
termination request is not observed until the browser forces it. Every
reopen after the forced kill was sound (integrity check passed, invariant
held, repair invoked). Consequences: (a) for an app, a write loop that never
yields will be allowed up to 2 s after a `terminate()`, and a kill then lands
at a random point, very likely inside a commit — which is the case recovery
must survive, and did; (b) the page cannot learn where the kill landed from
`postMessage`, which stops delivering the moment `terminate()` is called. A
`BroadcastChannel` did not help either: a never-yielding worker's posts never
reached the page at all (0 of ~340 in five trials, including those made
before the terminate call). The side channel that works is OPFS itself: the
churn writes `committed g` / `committing g` to a 16-byte side file with
synchronous flushed writes, and the page reads it after the kill. The trials
now run in two modes: **forced** (no yield; the browser kills at a random
point after ~2 s) and **yielding** (the worker returns to its event loop
after every commit; `terminate()` is honored promptly between commits), and
classify each reopen against the side file.

**Scope correction, after lane 7: this is Chromium policy, not a browser
fact.** Firefox 153 terminates the same non-yielding worker in ~91 ms after
0–3 further generations (§5.12). The 2-second grace period is real where it
happens and recovery survives it, but no app should be written against either
number.

### 5.8 `window.open` is not usable to open a second tab in an in-app browser

The two-tab test first tried `window.open(holderUrl, name)` from the page. In
the in-app Electron/Chromium pane it **navigated the current tab** into holder
mode instead of opening a window, destroying the running harness. Rewritten:
the page never calls `window.open`; it announces `holder-wanted` over a
`BroadcastChannel` and waits, and the holder tab is opened by the operator (or
by a real browser's own UI / a test driver's tab command). With a real second
tab the test ran green. A production ownership test in CI needs a driver that
can open tabs, not a page that opens its own. (`window.close()` from the
holder, by contrast, worked and released the handle.)

### 5.8b The native verify was not enforcing its digest

**Found in the second review.** `fixture verify` gated the content digest on
`expected_generation == manifest.generation` — and the browser→native route
*always* extends the generation (manifest 5, browser leaves 8), so the digest
comparison was disabled on the one path it exists to guard. Both receipts
happened to record matching digests, so the runs were in fact sound, but a
genuine semantic divergence would have returned `ok: true`.

The gate is removed; the digest is now always enforced. That is safe because
the digest covers the `muniment` table while the generation churn writes
`probe_meta`/`probe_data` — extending generations cannot move it. If it ever
does move, that is a real divergence and the verify should fail, which it now
will. The receipt field `muniment_digest_checked` (which could read `false`)
is replaced by `muniment_digest_ok`.

### 5.9 Portability round-trips both ways (lane 5)

- **Native → browser**: `fixture write` produced `portability.redb` (761,856
  bytes, generation 5, muniment-table digest `a74b7476…`). The browser
  `Import`ed it into OPFS, reopened it (integrity passed, invariant held at
  generation 5), and `Digest`ed the muniment table to the **same
  `a74b7476…`**.
- **Browser → native**: the browser then committed three more generations
  (to 8) over OPFS and `Export`ed the file. `fixture verify … 8` natively:
  integrity passed, invariant held at generation 8, and the muniment-table
  digest was still `a74b7476…` — the browser extended the generation tables
  while preserving the workload table's bytes exactly. Same redb file format
  read and written by native redb 4.2 and browser-OPFS redb 4.2.

### 5.10 Performance: redb pays per durable commit and wins on indexed reads

**Correction, 2026-08-22.** The first version of this section reported
per-workload *totals* and concluded "redb-on-OPFS is 1.6–5.2× slower than
IndexedDB". That reading was wrong in a way that inverted the finding for
read-heavy work, and it is retracted. Totals hide the shape; the phase data
was in the receipt all along. Two errors:

1. **Totals conflated writes and reads, which move in opposite directions.**
2. **The `ordered_log` workload is not how the shipping log writes.** It issues
   1000 individual `put`s, i.e. 1000 durable redb transactions.
   `stickleback::MunimentStore::insert_operation` writes the log entry, the id
   pointer, and the payload reference **in one `apply`** so a reader never sees
   one without the others. A `log_batched` workload with exactly that shape is
   now in the harness beside the unbatched one, so the two rows differ only in
   batching.

Medians of 3 repeats, headless Chrome 151, ms. Grouped as write phases vs
read phases, because they move in opposite directions:

| Workload | writes: IDB → redb | reads: IDB → redb |
|---|---|---|
| small slots (200 × 256 B) | 123 → 1454 (**11.8× slower**) | 75.5 → 4.2 (**18× faster**) |
| ordered log (1000 unbatched puts) | 300 → 3646 (**12.2× slower**) | 369 → 5.9 (**63× faster**) |
| **log batched (the shipping shape)** | 412 → 4201 (**10.2× slower**) | 945 → 10.9 (**87× faster**) |
| atomic batches (100 × 8-op apply) | 93 → 511 (5.5× slower) | 180 → 6.3 (**29× faster**) |
| large blobs (24 MiB) | 170 → 398 (2.3× slower) | 221 → 261 (1.2× slower) |

Individual phases, same runs: 50 window scans **167 ms → 0.4 ms** (ordered
log) and **384 ms → 0.3 ms** (log batched); whole-store digest **186 → 4.7**
and **548 → 9.7**; 200 point gets **39.7 → 1.5**.

The real shape: **redb pays a durable flush per commit; the read comparison
depends on which IndexedDB you compare against.** Every durable-write phase
favours IndexedDB.

**Correction, 2026-08-22 (second review): the read numbers above measure
muniment's adapter, not IndexedDB.** Production `IndexedDbBackend::scan` and
`::list` call `getAllKeys()` with **no query** and filter the whole key set in
Rust — a deliberate, documented choice ("correct for stores of the size a
browser tab holds"). So "redb reads 20–86× faster than IndexedDB" was
comparing a real index against a full-table scan, and it characterised the
shipping adapter rather than the storage engine. The honest comparison needs
IndexedDB asked for the range it wants.

The probe therefore now benchmarks a **third browser backend**,
`IndexedDbRangeBackend` (in the probe, not in muniment): identical in every
respect except that `scan` and `list` use `IDBKeyRange` + `getAllKeys(range)`.
Every workload runs on all three, and receipts carry both ratios —
`redb_over_indexeddb` (versus what ships) and `redb_over_indexeddb_range`
(versus what IndexedDB can do) — plus
`shipping_adapter_read_overhead`, how much of the shipping backend's read cost
is the adapter rather than the engine. **The `_range` column is the one an
adoption decision should use**; the shipping column says what muniment would
gain by fixing its own adapter, which is a cheaper change than swapping
storage engines. Numbers in §5.13.

**The scaling is the part that matters for a selection.** `log_batched` writes
the same 1000 operations as `ordered_log` but 2–3 keys each, so ~2.5× the key
count. IndexedDB's read cost tracked that increase almost linearly (369 →
945 ms); redb's barely moved (5.9 → 10.9 ms). So the gap is not a constant
factor to trade off once — **it widens in redb's favour as the key set grows,
and in IndexedDB's favour as writes get smaller and less batched.**

**And the write penalty differs by engine**, consistently across runs: much
larger on Chromium than Firefox (final runs: 6–37× against 2–3×). The
*direction* is stable; the *magnitude* is not pinned, because it moved
substantially between runs and the noise floor is 2.44×/1.44× (§5.13). Treat
it as "Chromium punishes redb's per-commit flush considerably harder than
Firefox does", not as a number. A selection made from one engine's figures
would be made from the least favourable one.

Levers not pulled, for the selection rather than the probe:
`Durability::None` for commits that may be lost, and a larger redb cache
(the probe caps at 32 MiB).

### 5.11 Browser scope: what "the browser passed" covers

A receipt covers **one engine on one host**, and this section says which.

| Engine | Host | Status |
|---|---|---|
| Chromium | Electron 42 / Chrome 148 (in-app pane), and headless Chrome 151 via Playwright | all lanes pass |
| Firefox | Playwright Firefox 153, headless | all lanes pass (§5.12) |
| Safari / WebKit | — | **not run.** No Apple host is driveable from this machine; the iMac is on a separate network and headed Safari is not scriptable from here. WebKit's OPFS and worker-termination behaviour are unproven. |
| WKWebView, iOS/Android WebView | — | **not run**, and not implied by any desktop result |
| Native mobile direct-file | — | out of scope; a different storage path entirely |

Two reasons this matters beyond diligence, both already visible in the
evidence: the forcible-termination delay (§5.7) and the handle-release latency
(§5.6) are **engine policies, not spec guarantees**, and `move()` (§5.4b) is a
vendor extension one engine may simply lack, which downgrades staged creation
from atomic to best-effort. Nothing here should be read as "browsers do X".

### 5.12 Firefox, the second engine (lane 7)

Playwright Firefox 153, headless, **byte-identical harness**: its
`probe_source_sha256` and `wasm_sha256` match the Chromium receipt's, so the two
receipts differ only by engine. Results in
[`2026-08-22_firefox.json`](../../../../ports/muniment-opfs-probe/receipts/2026-08-22_firefox.json).

Capabilities from inside a Firefox dedicated worker: `sync_access_handle`,
`storage_manager`, `web_locks`, and — a vendor extension outside the core
WHATWG IDL — **`atomic_move: true`**. Staged creation gets its atomic
promotion on Firefox too. Two engines, still not a spec guarantee.

**Everything the probe calls a result replicated:**

| | Chromium 151 | Firefox 153 |
|---|---|---|
| OPFS fault sweep | 707 trials, **0 unrecoverable** | 707 trials, **0 unrecoverable** |
| Termination trials | 20, all sound | 20, all sound |
| Second-writer refusal | `NoModificationAllowedError` | `NoModificationAllowedError` |
| Two-tab takeover / reload | passed, 178 ms | passed, 164 ms |
| Staged creation | 127 trials, **0 stubs** | 127 trials, **0 stubs** |
| Promotion-boundary kills | 12, **0 torn** (5 before / 7 after rename) | 12, **0 torn** (7 / 5) |
| Round-trip both ways | exact | exact |
| Stop condition | none | none |

**And two things diverged, both of which were assumed universal and are not:**

1. **The forcible-termination delay is Chromium policy, not browser
   behaviour.** §5.7 measured ~2.1 s and ~110 further generations after
   `terminate()` on a non-yielding worker. Firefox kills the same worker in
   **~91 ms after 0–3 further generations** — near-immediate. So the "a write
   loop gets up to 2 s of grace" caveat is Chromium-only, and an app must not
   rely on either number.
2. **Handle-release latency differs in shape.** Chromium: 2116 ms median after
   a forced kill but 89 ms after a cooperative one. Firefox: **~85–91 ms
   either way** — it does not distinguish, because it does not let the worker
   run on.

**The performance ratio is engine-dependent by roughly 3×**, which is on its
own enough to retire any single-number claim. redb's write penalty versus
IndexedDB, from the final sequential runs, **filtered by each engine's
measured noise floor** (§5.13); unresolvable cells are named rather than
filled with a number:

| Workload | Chromium write | Firefox write |
|---|---|---|
| small slots | 8.5× slower | 2.2× slower |
| ordered log (unbatched) | 37× slower | 3.2× slower |
| log batched (shipping shape) | 14× slower | *unresolvable* |
| atomic batches | 6.3× slower | *unresolvable* |
| large blobs | *unresolvable* | *unresolvable* |

redb's read advantage holds on both engines and survives the fair baseline by
a wide margin: **26–112× faster than range-query IndexedDB on Chromium,
37–79× on Firefox**, spreads nowhere near overlapping (§5.13). The
engine difference on writes is directionally clear — Chromium punishes redb
much harder than Firefox — but its magnitude is not pinned by this data.
Whatever the eventual selection, it cannot be made from one engine's numbers.

**One harness bug this engine exposed, which matters beyond the probe:**
`navigator.storage.persist()` **never settles in headless Firefox** — it
awaits a permission prompt with no UI to answer it, so the promise simply
hangs. Chromium auto-denies and resolves `false`. The harness deadlocked on
it before every lane, in the receipt-setup step, and the symptom (lane 1
hanging) pointed nowhere near the cause. Storage-manager calls are now raced
against a 5 s deadline and a timeout is recorded as a fact
(`persistence_request.granted: null`) rather than swallowed. Any production
code that awaits `persist()` on a background path should assume it may never
resolve.

### 5.13 The fair IndexedDB baseline (added after the second review)

Three browser backends, same workloads, same harness: redb-on-OPFS, the
**shipping** `IndexedDbBackend` (full key fetch, filter in Rust), and
`IndexedDbRangeBackend` (`IDBKeyRange` + `getAllKeys(range)`).

**Methodology, forced by the host-noise problem (§5.10 item 2).** The two
IndexedDB backends share **byte-identical write code** — only `scan`/`list`
differ. Their write phases are therefore a **built-in control**: any
difference between them is measurement error. On the final sequential runs
that control reports a **noise floor of 2.44× on Chromium** (log_batched,
spreads not even overlapping) and **1.44× on Firefox**. Nothing below those
figures is resolvable by this harness on this machine, and the table below
reports only comparisons that clear the floor *and* whose min–max spreads do
not overlap. Everything else is marked unresolvable rather than quoted.

| Workload | redb READ vs range | redb WRITE vs range |
|---|---|---|
| small slots | **112× faster** (Ch) / **56× faster** (Fx) | 8.5× slower (Ch) / 2.2× slower (Fx) |
| ordered log | **49×** / **37×** | 37× slower / 3.2× slower |
| log batched | **36×** / **58×** | 14× slower / *unresolvable* |
| atomic batches | **26×** / **79×** | 6.3× slower / *unresolvable* |
| large blobs | *unresolvable* / 1.6× faster | *unresolvable* on both |

**What survives, and what I have to withdraw.**

- **Survives, robustly:** on the four key-oriented workloads, **redb reads
  25–112× faster than IndexedDB used properly**, on both engines, with
  spreads nowhere near touching. Across three separate benchmark runs the
  direction never changed and the order of magnitude never changed. This is
  the finding the decision can lean on.
- **Survives, directionally:** redb writes are slower. The magnitude is not
  pinned — Chromium 6–37×, Firefox 2–3×, and it moved substantially between
  runs.
- **Withdrawn:** the earlier claim that a range-query adapter buys
  "**1.6–1.8× on scan-heavy workloads**". That figure was **below the noise
  floor** and should not have been quoted. On the final runs the same
  comparison is resolvable *only* on Chromium's two scan-heavy workloads
  (2.6× and 3.7×) and is unresolvable everywhere else, including on every
  Firefox workload. The honest statement is: **fixing the adapter helps
  scan-heavy workloads by an amount this harness cannot pin down, and does
  not measurably help the others.**
- **Withdrawn:** large blobs as "the workload where the two nearly meet".
  On Chromium that comparison is now unresolvable in both directions.

Why: even a proper range query returns its keys through the structured-clone
boundary, and — under this seam — every value still costs an individual
`get()` round-trip, where redb walks a B-tree in linear memory. Large blobs
remain the exception in both directions: the one workload where IndexedDB
reads faster on Chromium.

### 5.13b The range adapter is only correct for ASCII keys

Found in the third review, and it is a **seam** finding, not just a bug.

The first range implementation bounded a prefix scan with
`prefix + U+10FFFF`, reasoning that nothing sorts above it. That is false
under IndexedDB, which orders strings by **UTF-16 code unit**: a
supplementary character is a surrogate pair beginning below `U+FFFF`, so
`prefix + U+FFFF` sorts *above* that bound and was **silently omitted**.
Verified directly in JS before fixing.

The bound is now the prefix's immediate successor, which is exact. But the
deeper problem does not go away:

```text
Rust (muniment's scan contract): "\u{FFFF}"  <  "\u{10000}"
IndexedDB (UTF-16 code units):   "\u{FFFF}"  >  "\u{10000}"
```

**No choice of bounds reproduces muniment's specified ordering on IndexedDB
for keys outside the BMP.** A production range adapter would have to restrict
keys, encode them order-preservingly, or read through a cursor and re-sort in
Rust — and that last option gives back much of the range query's advantage.
This is a real cost line for option 2 in §7.2b, and it did not exist while
`scan` was a full-key-set filter (which, whatever else is wrong with it, sorts
in Rust and is therefore *correct*).

The probe takes the narrow honest option: `IndexedDbRangeBackend` declares an
**ASCII key contract** and returns an error for anything else, so it can never
silently return the wrong set. Every probe workload is ASCII, so the benchmark
is unaffected. The bound arithmetic, the carry past `0x7F`, the contract
check, and the ordering divergence itself are covered by native tests in
`src/idb_keys.rs` (17 tests total now).

**Two scope limits on these numbers, both from the third review.**

1. **They are measured under muniment's current `Backend` contract**, not
   against IndexedDB's ceiling. The range backend fixes `scan`/`list`, but the
   trait still hands out one `get(key)` at a time and one transaction per
   call, so every value read is a separate round-trip where redb walks a
   B-tree in linear memory. A `get_many`/cursor-shaped seam would move these
   numbers, possibly a lot. What is measured is "redb versus IndexedDB **as
   muniment asks for data today**" — which is the right comparison for a
   drop-in swap, and the wrong one for "how fast can IndexedDB go".
2. **The precise ratios need an idle host, and getting one took two goes.**
   The first fair-baseline run had both engines benchmarking *concurrently*
   (start timestamps 4.4 s apart, lane 5 lasting 47 s and 59 s), so they
   contended. The direction and order of magnitude survive that — a 20–40×
   read gap is not a scheduling artifact — but the exact ratios and the
   cross-engine difference are not.

   Making the runs sequential was not sufficient on its own: a Playwright
   browser killed mid-run **leaks its process tree**, and 44 stray Firefox
   processes from earlier aborted runs were still resident, which slowed the
   next fault sweep about 5× (≈1 trial/s against ≈5.6). Those were reaped.

   **But a genuinely idle host was not achieved, and cannot be here.** This
   is a daily-driver machine: with the strays gone, CPU was still saturated
   by the owner's own applications, and the same lane that took 125 s in one
   run took several times that in another. Killing those to get a clean
   measurement is not a call the probe gets to make. So the benchmark's
   **precision is bounded by host noise**, and the receipts record
   `total_spread_ms` (min/max across the 3 repeats) per row precisely so the
   noise is visible rather than averaged away. **Read the spread before
   quoting a ratio**; where min and max straddle a comparison, that
   comparison is not decision-grade, and a quiet machine or a CI runner is
   needed to settle it.

   Two process-hygiene notes worth keeping: lane 3 prints nothing between its
   start and finish lines, so silence is not evidence of a stall (it was
   misread as one here), and `TaskStop` on the runner does not reap the
   browser.

Two things follow:

- **For the adoption case:** redb's read advantage survives the fair
  comparison intact, and survives the noise floor by more than an order of
  magnitude. Correcting the baseline changed the *number* but not the
  *conclusion*.
- **The cheap fix is no longer priced.** It was, wrongly, at 1.6–1.8×; that
  was noise. This harness can say only that it helps scan-heavy workloads
  and not the rest. Pricing it properly needs a quiet machine — and §5.13b
  shows it also carries a correctness bill, so "cheap" was doing a lot of
  work in that sentence.

### 5.14 Adoption considerations (outside the probe's pass/fail)

- Production muniment is on redb 2.6; the probe is on 4.2. redb 3.0 changed
  the file format and 4.x reports `DatabaseError::UpgradeRequired` on older
  files, so adopting 4.x on desktop carries an upgrade path for existing
  stores. Not a probe concern; a selection concern.
- redb's default cache is 1 GiB; the probe sets 32 MiB in the worker.
- A freshly created store has no allocator-state table, so `Database::new`
  takes its "not shutdown cleanly → repair" path once at creation (three
  repair callbacks). `repair_invoked` is therefore meaningful only on a
  reopen; a clean close stores the allocator state and the next open skips
  repair entirely (lane 2: zero callbacks). After a kill, a **full repair**
  walks every page; its cost grows with the store, which is the recovery-time
  boundary to watch in lane 3's `open_ms`. `set_quick_repair(true)` (two-phase
  commit plus allocator state per commit) trades commit cost for near-instant
  recovery, a knob for the selection, not the probe.
- `check_integrity` needs `&mut Database`, so it runs before the muniment
  adapter wraps the database in an `Arc`.

## 6. Progress

- **2026-08-22** — Brief received. Grounded against the tree: muniment on
  redb 2 with `RedbBackend` + `IndexedDbBackend`; redb 4.2.0 already in the
  registry (iroh-blobs); `ports/distillery/probe` as the standalone headed
  probe pattern; wasm-bindgen CLI 0.2.126 installed; no atomics anywhere in
  the workspace. **Lane 1 passed**: redb 4.2.0 compiled for
  wasm32-unknown-unknown and ran transactions under Node with zero imports.
  Probe crate founded at `ports/muniment-opfs-probe/` (MPL-2.0): OPFS backend,
  fault injection with native sweeps, generation invariant, redb-4.2 muniment
  adapter, workloads, worker body, page harness, fixture binary, runner
  script.
- **2026-08-22, native** — `cargo test --release`: 12/12. The OPFS backend
  compiles for wasm32-unknown-unknown against redb's `Send + Sync` bound with
  zero `unsafe` (the compile-time assertion holds). Lane-3 native sweep
  recorded in §5.5; the creation-atomicity finding in §5.4. Two harness
  corrections on the way: cuts that fall in redb's shutdown writes produce no
  commit error by design, and `createSyncAccessHandle` is exposed only in
  dedicated workers, so the capability is recorded from inside one.
  Portability fixture written natively (761,856 bytes, generation 5, muniment
  digest `a74b7476…`). Browser receipts follow.
- **2026-08-22, browser (Electron 42 / Chrome 148, in-app pane)** — **Lane 1
  passed in the worker**: redb transactions against `InMemoryBackend`, 64 ×
  64 KiB values, 37.9 ms. **Lane 2 passed**: one worker created
  `muniment-probe/lane2.redb` over the OPFS sync-access handle, committed 8
  generations (192 writes, 15 flushes, 2 resizes, 1.25 MB written, 226 ms),
  closed; a fresh worker reopened it in 13.7 ms with the integrity check
  passing, the invariant holding at generation 8, and repair **not** invoked
  (the clean shutdown was recognized); 462,848 bytes on disk. Harness
  corrections: `createSyncAccessHandle` is recorded from inside a worker;
  kill delays busy-wait and reopen polling yields via `MessageChannel`, so a
  hidden pane's timer throttling cannot stretch them.
- **2026-08-22, all lanes green.** Lane 3 browser: 707-trial OPFS fault sweep
  with 0 unrecoverable, then 20 termination trials (10 forced, 10 yielding),
  every reopen at the preceding committed generation, 0 regressions (§5.5b).
  Progress-under-kill solved with an OPFS side file after `postMessage` and
  `BroadcastChannel` both failed to survive a forced kill (§5.7). Lane 4:
  second-writer refusal, controlled two-tab takeover, and reload recovery all
  passed, once the `window.open` self-navigation trap was replaced by an
  operator-opened holder tab (§5.8). Lane 5: portability round-trips both ways
  with matching digests (§5.9), and the IndexedDB benchmark gives redb-on-OPFS
  at 1.6–5.2× IndexedDB (§5.10). Receipt at
  [`receipts/2026-08-22_redb_opfs_probe.json`](../../../../ports/muniment-opfs-probe/receipts/2026-08-22_redb_opfs_probe.json)
  (the full run) and the browser-written fixture verified natively at
  generation 8. **Probe complete; the decision is §7.**
- **2026-08-22, review pass.** A review found six problems and every one held
  up on inspection. Two were blocking and are the reason the earlier
  "COMPLETE / do not adopt" framing is withdrawn:
  - **The benchmark was misread (§5.10).** Reporting totals hid that redb and
    IndexedDB move in *opposite* directions on writes and reads; the claimed
    "2–5× slower" inverted the result for read-heavy work (window scans are
    ~400× faster on redb). The `ordered_log` workload also did not model the
    shipping write path: `stickleback::insert_operation` batches 2–3 keys
    through one `apply`, so a `log_batched` workload with that exact shape was
    added and is now the row a decision should turn on.
  - **The receipt did not identify its own build (§5.3b).** It recorded only
    `commit + dirty`, its `environment` block came from an earlier harness
    revision than the tree, and `browser_to_native` still read "pending native
    verify". Receipts now carry source/dependency/wasm/JS hashes, and
    `run-browser.mjs` runs the native verify itself and records the real
    result instead of a placeholder.
  Three more were real code or coverage gaps, now closed: the fail-closed
  cross-thread claim was **false as written** and the token is now
  realm-qualified, with JS safe-integer guards added (§5.2); the
  creation-atomicity remedy was hypothetical and is now lane 6, crash-tested
  with 0 unopenable stubs (§5.4b); and "browser" meant one Chromium host, so
  lane 7 adds Firefox with its own receipt (§5.12) — **Safari/WKWebView
  remain unrun and are the standing gap** (§5.11). The sixth was cleanup:
  `cargo fmt` is applied and enforced by `run-probe.ps1`, and the `--locked`
  failure turned out to be the machine-local patch table, not source drift
  (§5.3b). Firefox also exposed a harness deadlock worth keeping —
  `navigator.storage.persist()` never settles headless (§5.12).

  Per DOC_POLICY §8 the plan stays **active** rather than moving to
  `archive_docs/`: the decision it exists to inform is open again, not closed.
- **2026-08-22, two-engine receipts.** Chromium 151 and Firefox 153, same
  `probe_source_sha256` and `wasm_sha256`, all six lanes each. **Every correctness
  result replicated** (707 fault trials → 0 unrecoverable, 20 termination
  trials sound, 51 staged-creation trials → 0 stubs, identical refusal name,
  exact round-trip both ways with the native verify run inside the pass).
  Two behaviours diverged and both had been treated as universal: **Chromium's
  ~2.1 s forcible-termination grace is Chromium's alone** (Firefox: ~91 ms,
  0–3 further generations), and **the redb/IndexedDB write penalty is much
  larger on Chromium than Firefox** (the specific ratios quoted at the time
  were later found to be below the measurement noise floor; the direction
  held across runs, the magnitude did not — §5.13). Both are reasons the
  selection cannot be made from one engine.
  Receipts: `2026-08-22_chromium_full.json`, `2026-08-22_firefox.json`.
- **2026-08-22, second review pass.** Five more findings, all confirmed
  against the tree, all fixed; the status line is downgraded accordingly.
  - **Lane 6 did not test what it claimed.** The sweep silently truncated to
    40 writes (it derived the count from `staging_len`, which is 0 after a
    successful promotion), and atomic promotion was never exercised — the
    only promotion trial cut *before* the rename. Now: counters come from the
    worker (**108 writes, 3 resizes, 15 syncs → 127 creation trials**, both
    engines), and a promotion-kill sub-lane terminates the worker around
    `move()`: 12 trials per engine, **0 unopenable, 0 both-absent** (Chromium
    5 before / 7 after the rename; Firefox 7 / 5). *(This entry originally
    read "12/12 confirmed killed in flight"; the third review showed the
    harness cannot establish that, and the claim is withdrawn — see below and
    §5.4b.)*
  - **Provenance missed the compiled-in muniment source**, `run-browser.mjs`,
    the sink and the fixture manifest; hashed resolved `name version` pairs
    instead of the lockfile; and called SHA-256 "blake3". All fixed. **And
    the `--locked` workaround was unnecessary** — cargo reads
    `.cargo/config.toml` from the *working directory*, so building from `C:\t`
    with `--manifest-path` avoids the patch table entirely. The build now runs
    `--locked` from a neutral directory and hashes the real lockfile. §5.3b.
  - **`close()` still failed open** across realms, reporting success while
    leaving the file locked — the last place the registry lied, and exactly
    the future-threads claim the realm guard exists for. §5.2.
  - **The native verify skipped its digest check** on the browser→native
    route, the only route it guards. Always enforced now. §5.8b.
  - **The read benchmark measured muniment's adapter, not IndexedDB.** A
    third backend (`IndexedDbRangeBackend`, `IDBKeyRange` + `getAllKeys`) now
    runs every workload. The correction moves the baseline but not the
    conclusion: redb stays tens of times faster than IndexedDB used properly.
    *(The adapter-overhead figure quoted here at the time, 1.6–1.8×, was
    withdrawn in the fourth round as below the noise floor — §5.13.)*
    §5.13, §7.2b.
- **2026-08-22, third review pass.** Four cleanup items, all confirmed, all
  fixed; the promotion and provenance claims are downgraded accordingly.
  - **"Confirmed killed in flight" was not shown.** `terminate()` resolving
    the call only proves the kill preceded the *whole command*, which also
    contains the post-move checks. Renamed to **promotion-boundary kills**,
    the claim removed, and inspection now waits for both names to stabilize
    (an immediate read can race browser state still settling). The observed
    outcomes stand as evidence. §5.4b.
  - **The aggregate provenance hash was already stale** (`ec6c…` recorded,
    `eeb7…` current) while every executable hash still matched — because the
    script hashed the fixture manifest *before* regenerating it and included
    prose files. Fixtures are now generated first and only behavioural inputs
    are hashed, so a mismatch means the build really changed. §5.3b.
  - **The benchmark receipts were concurrent** — both engines ran at once on
    one host (4.4 s apart, 47 s and 59 s lanes), so they contended. Runs are
    now sequential; earlier cross-engine ratios are indicative only. The
    conclusion is also now phrased as holding **under muniment's current
    `Backend` contract**, since the range backend still uses one transaction
    and one `get()` per key. §5.10, §5.13.
  - **The range adapter was only correct for ASCII** — and worse, this turned
    out to be a **seam** finding. `prefix + U+10FFFF` sorts *below*
    `prefix + U+FFFF` under IndexedDB's UTF-16 ordering, so such keys were
    silently omitted; and no bound choice can reproduce muniment's Rust-order
    `scan` contract outside the BMP, because the two orders genuinely
    disagree. Bounds fixed to the exact successor, an **ASCII key contract**
    added that refuses rather than mis-selects, and the divergence itself
    asserted in tests (`src/idb_keys.rs`, 17 native tests now). This puts a
    real correctness bill on §7.2b's option 2. §5.13b.
- **2026-08-22, fourth pass: the benchmark had to grade itself.** Re-running
  sequentially surfaced two things beyond the fix list.
  - **`TaskStop` on the runner leaks the browser process tree** (Playwright
    chromium is `chrome-headless-shell`, not `chrome`). 44 stray Firefox
    processes were slowing the fault sweep ~5×. Reaped — but with them gone
    the host was *still* saturated by ordinary desktop applications, and
    **a genuinely idle host is not obtainable on this machine**. Also: lane 3
    prints nothing between its start and finish lines, and I misread ten
    minutes of that silence as a stall. §5.10 item 2.
  - **A control was available and it failed.** The two IndexedDB backends
    share byte-identical *write* code, so their write phases measure only
    error. That control reports a **noise floor of 2.44× (Chromium) and
    1.44× (Firefox)** — on log_batched the identical code differed by 2.44×
    with *non-overlapping* spreads. Every comparison is now filtered against
    it. **redb's 26–112× read advantage clears it by more than an order of
    magnitude and is the finding to lean on; the 1.6–1.8× adapter-overhead
    figure from the third pass did not clear it and is withdrawn**, as is
    "large blobs nearly meet". §5.13.

  Correctness results were unaffected: 707 fault trials, 127 creation trials
  and 0 unopenable stubs per engine, round-trip exact both ways with the
  digest enforced, on both engines, from one `probe_source_sha256`.

## 7. Decision

**Revised 2026-08-22, across three review rounds.** The first version of this
section recommended not adopting redb-on-OPFS, on the strength of a
totals-based reading of the benchmark. That reading was wrong (§5.10) and the
recommendation built on it is withdrawn. What follows separates what is
proven from what is still open, because they have different answers — and the
"open" list has grown, not shrunk, as the harness got more honest.

### 7.1 Proven, on two independent engines

**Stock redb 4.2 can run over OPFS in a single-threaded worker, with no redb
change, no Rust `unsafe`, strong crash-recovery evidence, and
native-compatible database contents — replicated on Chromium 151 and Firefox
153 from a byte-identical harness** (matching `probe_source_sha256` and
`wasm_sha256`).

- **Feasibility and the `Send + Sync` gate: yes, honestly.** The value redb
  owns holds no JS value; the token is realm-qualified so a cross-thread
  lookup fails closed under any build, and the JS safe-integer crossings are
  guarded (§5.2). The compile-time `Send + Sync` assertion holds with no
  `unsafe`.
- **Recovery: 964 native + 727 browser trials per engine, 0 unrecoverable on
  either.** Every reopen yielded the preceding or the completed commit with
  the invariant intact (§5.5, §5.5b, §5.12).
- **Creation staging: proven.** 127 trials per engine, the sweep derived from
  the worker's actual storage-call counts, **0 unopenable stubs** (§5.4b).
  Atomic *promotion* is **not** proven — 24 boundary kills produced only
  atomic outcomes, but in-flight termination is not established (§7.2 item 3).
- **Ownership: deterministic on both** — same refusal
  (`NoModificationAllowedError`), controlled takeover, release after
  kill/close/reload, with measured latencies (lane 4).
- **Portability: exact, both directions**, same redb file read and written by
  native and browser; the browser-written database is now verified by the
  native `fixture` binary inside the run, not by a placeholder (§5.9).
- **muniment semantics preserved**: the adapter passes the same cross-backend
  content-digest oracle as `MemoryBackend` and `IndexedDbBackend`.

### 7.2 Open, and therefore not decidable yet

1. **Engine coverage.** Two engines agree on every correctness result, which
   is real reassurance. But **Safari/WKWebView are not run at all** and cannot
   be from this machine, and lane 7 proved the point of asking: the
   forcible-termination delay differs by ~23× between the two engines tested,
   and `move()` is a vendor extension whose absence would downgrade staged
   creation to best-effort (§5.11, §5.12).
2. **Performance, and it is not a scalar.** The axis is read/write mix and
   batching. redb wins **26–112× on indexed reads** against a fair
   range-query baseline — robust across three runs, two engines, and well
   clear of the noise floor — and pays on durable writes by an amount that is
   directionally certain but **not pinned** (much worse on Chromium than
   Firefox; §5.13). A decision needs a named consumer's mix, a target-engine
   weighting, and — for anything finer than an order of magnitude — a quieter
   machine than this one (§5.10 item 2).
3. **Crash-atomic promotion.** `move()` is a vendor extension with no spec
   crash guarantee. 24 promotion-*boundary* kills across two engines produced
   only atomic outcomes — but the harness cannot establish that any kill
   landed inside the rename (§5.4b), so this is weaker than it first read.
   Closing it needs an upstream guarantee, a harness that can prove in-flight
   termination, or a design that does not depend on rename atomicity at all.
4. **Levers unpulled**: `Durability::None`, cache size, and batching at the
   consumer.

### 7.2b A third option the two-column benchmark hid

Because the read comparison was against muniment's own full-scan adapter
(§5.13), the decision was implicitly framed as "redb-on-OPFS or the status
quo". It is not. The options are:

1. **Keep IndexedDB as it is.** Ships, passes, no work.
2. **Give `muniment::IndexedDbBackend` an `IDBKeyRange` `scan`/`list`.** A
   small, contained change to a shipping backend — no new engine, no new file
   format, no creation-atomicity machinery.
3. **Adopt redb-on-OPFS.** Everything this probe built, in exchange for the
   read advantage plus real transactions and native format portability, at
   the cost of durable writes.

Option 2 is **not yet priced**, and my two previous attempts to price it were
both wrong — first by assuming it captured most of redb's read advantage, then
by quoting 1.6–1.8× from below the measurement noise floor (§5.13). What can
be said: it helps scan-heavy workloads by an unpinned amount, does not
measurably help the rest, and is nowhere near redb's 25–112×.

It also carries a correctness bill that option 1 does not (§5.13b): a range
query orders in UTF-16, muniment's `scan` is specified in Rust's order, and
the two disagree outside the BMP. Today's full-key-set filter is slow but
*correct*, so option 2 must additionally buy an ASCII key restriction, an
order-preserving key encoding, or a cursor-plus-re-sort that surrenders much
of the gain. It is a real design decision, not a free win — and it is not a
substitute for option 3 if the read gap is what a consumer needs.

### 7.3 Where that leaves the call

**Chromium feasibility is proven; the adoption decision is open** — and it is
not the binary the probe was originally framed around. On the evidence now in
hand, redb-on-OPFS is a *credible* browser backend whose profile is the
opposite of IndexedDB's: better indexed reads, worse unbatched durable writes.
Which one wins depends on a consumer's mix, which is a product question.

**Retaining IndexedDB as the incumbent remains sensible** — it ships, it
passes, and nothing here forces a change. **Rejecting redb-on-OPFS as the
eventual default is not supported by this benchmark**, and I withdraw the
earlier recommendation to that effect.

Before the selection is made, the tail worth finishing (in order):

1. Freeze source, deps and wasm in a fresh receipt — **done** (§5.3b): probe
   tree, muniment source, whole lockfile, `--locked` build, plus page-side
   wasm/JS hashes. The superseded receipts are history only.
2. Thread and offset guards — **done**, including `close()` failing closed
   (§5.2).
3. Prove staging creation — **staging proven; atomic promotion evidenced,
   open** (§5.4b, §7.2 item 3).
4. Phase-level, repeated, batched-shape benchmarks **against a fair
   IndexedDB** — **done** (§5.10, §5.13). Read `log_batched` and the
   `vs_range` column, not totals and not `vs_shipping`.
5. Firefox — **done** (§5.12). **Safari/WKWebView remain the real gap.**
6. Price the range-query `scan` option — **still open**. Two attempts to
   price it were wrong (§7.2b), the second because the figure sat below the
   noise floor. It needs a quiet machine, and §5.13b's ordering problem
   costed alongside it.
7. Then decide default versus feature, against a named consumer's read/write
   mix and a target-engine weighting rather than in the abstract.

Still Mark's calls, not mine: whether to commit the probe so a commit
identifies it; the Web Locks posture (§3.4); and whether Safari coverage is a
precondition for the selection or can trail it.
