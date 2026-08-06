# Browser storage persistence, verified in a browser

**Date:** 2026-08-06
**Result:** the reference host asks the browser whether its storage is kept,
and reports a refusal honestly. Confirmed against a running Chromium, not
inferred from a type check.

## Cut

`storage_status` already reached the DOM as `data-storage`, but it named which
store was open rather than whether the browser would keep it. "IndexedDB
reopened" reads as durable and is not.

`graphshell::browser_storage` holds the decision with no browser in it, because
`web.rs` has no test harness and cannot grow one without a browser. Three
states rather than two: `Granted`, `Refused`, and `Unknown(reason)`. An
insecure context, an older browser, and a call that rejected all mean *we could
not ask*, which is a different fact from *the browser said no*. `is_durable()`
is true only for `Granted`.

## Evidence

```text
cargo test -p graphshell --features personal-sync --lib browser_storage
cargo build -p graphshell-web --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ports/graphshell/web/pkg \
  target/wasm32-unknown-unknown/debug/graphshell_web.wasm
python -m http.server 8731   # from ports/graphshell/web
```

Loaded `http://localhost:8731/index.html` in a browser and read the live DOM:

```json
{
  "title": "GRAPHSHELL H3 READY",
  "storage": "IndexedDB seeded · not persistent, may be evicted",
  "persistence": "refused",
  "secure": true,
  "hasStorageMgr": true
}
```

Cross-checked against the browser's own answer rather than the host's report:

```json
{ "persistedNow": false, "quotaMB": 5849, "usageKB": 18, "attr": "refused" }
```

The browser refused. That is the ordinary case for a fresh origin with no
engagement, and it is the case the honesty requirement was written for: the
host says "may be evicted" instead of reporting a store it cannot keep as
though it were safe. `refused` rather than `unknown` also confirms both calls
answered, so the two states are distinguished by evidence and not by guess.

## Boundary

This verified the reported state, not eviction itself. Provoking a real
eviction needs storage pressure a page cannot create on demand.

Nothing here makes browser storage durable. The resident host's blob store is
what makes a refusal survivable, and that is why refusing is reported rather
than treated as a failure to start.

## Stop rule

There is no automated browser harness in this repo: no `wasm-bindgen-test`, no
Playwright, no headless driver. This receipt was taken by hand. Before the next
browser-facing claim, decide whether that stays a manual step or becomes a
wired one; "web.rs has no test harness" is a gap, and it should not keep being
cited as a reason.
