# Device Resident V1 Receipt

**Date:** 2026-08-22
**Code:** `228213fe`
**Status:** Automated cone complete. Physical two-device and headed receipts
remain open.

## What this closes

The final audit found two claims that the earlier focused receipts did not yet
make:

1. Personal unpairing had changed live authority, but the two-device artifact
   flow had not attempted a new evidence fetch after unpairing.
2. Communal authority had been materialized from Gemot certificates, but a live
   Knot host had not applied a signed grant and signed revocation while all
   three consumers were running.

`paired_peers_replicate_djot_then_fetch_and_reopen_verified_evidence` now
retains a second artifact, removes Device B from the personal authority, and
proves all three facts together: exact-hash serving is denied, a new source
fetch fails, and Device A keeps its retained bytes.

`signed_gemot_grant_and_revocation_update_every_live_consumer` begins with an
empty communal authority, accepts signed Gemot certificates for independent
`document`, `evidence/read`, and `evidence/source` capabilities, applies their
materialized revision to one live host, and then accepts signed revocations.
Writing, exact-hash serving, source selection, and route hints move on the same
revision. Revocation removes all three rights without deleting locally retained
bytes.

The full Graphshell run also exposed a shutdown defect. `PersonalSyncHost` was
dropping `JoinedSpace` and polling redb, even though `JoinedSpace` already owns a
waited shutdown that joins the drain and LogSync actor. With another peer still
live, the old path exhausted its database-lock retry. `close` now calls
`leave_and_wait` before closing the endpoint and probing the store. The original
three-host test passes without changing its shutdown order.

## Automated evidence

The following commands passed on the compatible p2panda 0.7.0 source:

```text
cargo test -p knot --lib --offline
101 passed; 0 failed

cargo test -p mere-transport --lib --offline
45 passed; 0 failed

cargo test -p titulus --lib --offline
12 passed; 0 failed

CARGO_PROFILE_TEST_DEBUG=0 cargo test -p graphshell \
  --features personal-sync --lib --offline
236 passed; 0 failed
```

The Graphshell command was repeated from a detached `228213fe` worktree using
the workspace's committed p2panda source. During verification, the adjacent
local p2panda checkout advanced from 0.7.0 to 0.7.1. Loading that checkout makes
Stickleback fail to compile because the new `Header` no longer supplies the
serialization and `to_bytes` API the current adapter expects. That integration
drift is outside this V1 change and is not counted as a test failure.

## Remaining evidence

The Rust receipt simulates two device identities, two stores, real endpoints,
document-before-artifact delivery, verified fetch, restart, offline read, and
post-unpair refusal. It does not substitute for the plan's physical two-device
run.

The headed receipt also remains distinct. Turnstone's checked-in lockfile is
behind the source that introduced `AppRouteCarrier`: `--locked` refuses a lock
update, and resolving the old Git graph leaves that type absent. A disposable
Turnstone worktree redirects the Git dependencies to the live Mere checkout and
passes `cargo check --offline`. A normal debug link then failed with an invalid
Turnstone rlib, while a fresh build with development debug information disabled
linked and launched successfully. That headed run used Turnstone's sample graph,
not the resident Knot route, so the plan's headed edit, restart, and
evidence-open composition remains required for acceptance.

## Command-palette lag

The reported lag while Turnstone's command palette is open is recorded as a
Turnstone chrome performance lane. Code reading shows a plausible hot path: the
open palette enlarges the chrome subtree, while chrome synchronization and
scene production run on redraw. A disposable unoptimized build reproduced the
whole-frame delta at 1024 x 600: two closed redraws took 25.9 ms and 26.7 ms,
while four open-palette redraws took 239.3 ms through 250.7 ms. This small
development sample confirms the symptom but does not attribute it.

The next receipt should compare one release-build scene and profile with the
palette closed and open, publish median and p95 frame time plus
input-to-present latency, and attribute the delta among suggestion computation,
chrome cascade/layout/paint, and background resident work. The fix is done when
merely leaving the palette open causes no repeated work and the responsible
stage has an executable regression check. This observation does not widen the
resident lane unless profiling demonstrates resident work on the hot path.
