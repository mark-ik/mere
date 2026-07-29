# Graphshell H7a personal-sync core receipt

Date: 2026-07-29

Status: the bounded native sync core is implemented. The later
[H7b closure receipt](2026-07-29_h7b_personal_sync_closure_receipt.md) closes
the durable reopen, device-host/browser composition, physical-device
convergence, and blob-availability gates.

## Implemented boundary

`ports/graphshell/src/personal_sync.rs` gives Graphshell a secret-free event
grammar for continuous personal-device metadata sync. It composes p2panda
operations and storage with Stickleback's policy-before-insert processor and
LogSync join/drain machinery.

The selected sync surface includes:

- node creation and removal, titles, tags, and relations;
- user-selected graph facets;
- immutable access records with their original device and time;
- saved scenes and handler preferences when separately selected.

Personae supplies the stable public writer binding and roster roots. The
generic lane categorically rejects credential and secret-bearing facets,
`LocalOnly` access records, and Moot-scoped access records. Vault roots, seeds,
credential slots, decrypted payloads, and private epochs never enter an event.

Arbitrary Mere facets now use the same `GraphDelta` capture and replay spine as
the rest of the durable graph. Graphshell no longer writes those facets around
the graph journal.

## Promoted shared seam

Stickleback now owns the stable writer-subject binding derived from a public
Personae root and optional attestation. Commons-spine uses that implementation.
Knot keeps its causal text fold because its merge semantics are not generic
graph semantics.

Graphshell and the existing consumers therefore share causal authoring,
policy-before-storage intake, LogSync drain, and storage contracts without
sharing a domain fold.

## Executable proof

The focused Graphshell test creates two independent in-memory stores and two
in-process p2panda transports. Each device authors while partitioned. After
joining the same LogSync space, both projections converge byte-for-byte while
retaining:

- both concurrent tags and one relation;
- two access records ordered by original time and still attributed to their
  source devices;
- a selected arbitrary facet;
- a saved scene and handler preference;
- admitted stable-writer receipts.

A second test creates concurrent scalar title edits and verifies that the
projection exposes the unresolved target instead of silently selecting one
value. It also proves secret-bearing facets and local-only access records are
refused before authoring.

Verification:

```text
cargo test -p mere-kernel --lib
280 passed; 0 failed

cargo test -p graphshell --lib --features personal-sync
85 passed; 0 failed

cargo test -p stickleback --lib
56 passed; 0 failed

cargo test -p commons-spine --lib
40 passed; 0 failed

cargo test -p knot --lib sync::tests::
14 passed; 0 failed

cargo check -p graphshell --target wasm32-unknown-unknown \
  --no-default-features --features web
passed

cargo clippy -p stickleback --lib --no-deps -- -D warnings
cargo clippy -p commons-spine --lib --no-deps -- -D warnings
cargo clippy -p graphshell --lib --features personal-sync --no-deps -- \
  -D warnings -A clippy::too_many_arguments
passed
```

The complete Knot package is not green in the live baseline: 35 tests pass and
17 content-class tests fail because Knot consumes Eidetic with default features
disabled while those tests require its JSON-schema validator. All 14 Knot sync
tests pass, including durable author-head/frontier reopen and real p2panda
LogSync convergence. The validator mismatch is outside the promoted H7 seam.

## H7 closure

The H7b slice adds Redb reopen, resident device-host ownership, admitted
browser projection, separate blob-availability metadata, and the physical
Windows-to-Q-PC convergence receipt. Personae carry, enrollment, secret
wrapping, revocation, and recovery remain on their separate high-assurance
lane.
