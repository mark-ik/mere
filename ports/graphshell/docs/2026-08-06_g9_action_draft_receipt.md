# G9 action-draft receipt

**Result:** a real browser host now projects one endpoint-advertised bounded
action form through both Cambium chrome and native semantic controls. The host
keeps the selection draft outside the endpoint protocol, submits only the
endpoint's exact payload, and remounts the endpoint's fresh snapshot after an
accepted action.

## Boundary

The G8 protocol contract remains the authority for form shape and payload
validation. `ActionDraft` owns only host-local selection, the snapshot-local
target, and a renderer-neutral semantic projection. The browser HTML form and
the Cambium detail panel consume that same projection. Neither assigns a
default, invents a choice, or infers an opaque value.

The fixture advances its own revision after accepting `fixture.inspect-tile`.
The browser host then requests and mounts the next snapshot before clearing the
draft. A host-side resnapshot is therefore observable rather than assumed.

## Checks

Focused library checks passed:

```text
cargo test -p graphshell --no-default-features --features web action_draft --offline
3 passed; 0 failed

cargo test -p graphshell --no-default-features --features web canary --offline
4 passed; 0 failed
```

The browser package compiled for Wasm and was bound for the static host:

```text
$env:CARGO_TARGET_DIR = 'target-plan-graphshell-web'
cargo build -p graphshell-web --offline --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ports/graphshell/web/pkg \
  target-plan-graphshell-web/wasm32-unknown-unknown/debug/graphshell_web.wasm
```

## Headed interaction

The local host at `http://127.0.0.1:8765/` was driven in the in-app browser:

1. Switch to **Remote mount**, open object detail, and choose **Open selected**.
2. The semantic tree exposed **Inspect map tile**, its required **Inspect**
   combobox, and only `Coast outline` and `Field coordinates` as choices.
3. Submitting before selecting surfaced
   `action form requires field inspection_scope` without an endpoint call.
4. Selecting the exact `coordinates` value and submitting produced this final
   browser state:

```text
title: GRAPHSHELL H3 READY
session: remote
actionCount: 1
actionDraftOpen: false
formHidden: true
actionStatus: Accepted · resnapshotted revision 2 · 1 invocation(s)
console errors: []
```

The headed pass found one real defect before this receipt: the frame loop was
rebuilding the semantic form on every animation frame, disrupting focus. The
host now updates that DOM projection only when the draft changes.

## Stop

This proves one fixture action form through one browser host. It does not yet
make every advertised action independently selectable in the remote scene,
persist drafts, or define a settings schema. The next consumer is Cleromancy's
A16 saved-record chooser, which must advertise only its endpoint-derived,
bounded choices and receive the same exact-value treatment.
