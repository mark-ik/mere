# Device Resident V1 Receipt

**Date:** 2026-08-22
**Code:** `228213fe`
**Status:** Automated cone and Turnstone headed edit/close/restart complete.
Physical two-device and the remaining standalone/evidence-headed receipts stay
open.

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

## Headed Turnstone evidence

The headed lane now has a real first-party composition receipt rather than a
sample-graph launch. Mere `4565d040` adds a receipt fixture that authors
`knot://vault/field-note` through `StartupUnlockedPersonalVault` and selects
that persona through Graphshell's ordinary owner settings. Turnstone
`02772e0`, `952f0df`, and `8637abe` add the scenario key/IME routing and the two
checked-in edit and reopen scenarios. Genet `9d3f2bd3031` makes the shared Probe
selector recognize a textarea's native `textbox` role.

The final run used fresh persona `00000000-0000-0000-0000-00000000a502`, fresh
stores, and a private first-party named pipe. One hidden
`graphshell_device_host` process logged `resident Knot route open` and
`door="first-party"`. A headed Turnstone process then:

1. opened the resident `knot` route;
2. focused the painted editor through Turnstone's ordinary pointer path;
3. inserted `headed edit` through the same IME seam as native input;
4. saved with the same `Ctrl+S` key seam as native input;
5. captured the saved editor;
6. closed the live content pane and asserted that its surface was absent.

Its app-authored sentinel reported `RESULT ok`. Graphshell's original resident
process was still live after Turnstone exited. A second, fresh Turnstone process
then reopened `field-note`, asserted `Resident V1 headed edit` and `saved`, and
wrote a second `RESULT ok` sentinel. The app-authored frames are
`01_resident_edit.png`, `02_resident_after_close.png`, and
`03_resident_reopened.png` in the isolated receipt archive at
`C:\t\knot-v1-headed-run-20260822-a502`.

The shared semantic selector exposed one bounded drift. Genet Probe re-derives
layout and aimed the textarea role at x621, outside the narrower textarea that
Turnstone actually painted. The receipt therefore records its fixed 1024 x 600
painted point, x480/y65, and still routes that point through the ordinary
pointer lifecycle. Moving selector resolution onto Turnstone's retained,
painted layout is a separate Genet/Turnstone automation contract change. It is
not resident authority work.

## Remaining evidence

The Rust receipt simulates two device identities, two stores, real endpoints,
document-before-artifact delivery, verified fetch, restart, offline read, and
post-unpair refusal. It does not substitute for the plan's physical two-device
run.

The remaining headed work is narrower now. There is no standalone Knot
executable yet, only the reserved `knot-editor` package name, so standalone sync
status, the standalone edit/restart/evidence-open flow, and directory-only
editing with the desktop resident stopped are not runnable claims. Turnstone's
resident route also has no evidence-open scenario yet. Those are product/UI
composition lanes, not reasons to reopen the resident document authority that
this receipt exercised.

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
