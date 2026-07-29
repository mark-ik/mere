# Graphshell H6c transfer-core receipt

Date: 2026-07-29

Status: local two-store transfer core complete. The physical carrier gate is
closed by the H6d receipt.

## Implemented boundary

`ports/graphshell/src/transfer.rs` composes the existing Graphshell selection
export, Eidetic typed envelopes and authorities, Muniment blob storage, and
Mere's cross-graph copy primitive.

One versioned transfer manifest now carries:

- a schema-typed, immutable graph-selection engram;
- source and destination graph, persona, and device references;
- the replicate or copy operation;
- the selected route;
- independently addressed BLAKE3 blob descriptors;
- source AccessRecords only under the selected access-history policy.

Local file locations are removed unconditionally. File bytes remain outside
the engram. Preparation verifies their byte count, media type, and existing
SHA-256 content facet before staging them as Muniment blobs. Application
verifies BLAKE3 and byte count before graph mutation and skips a source fetch
when the destination already holds the verified blob.

Replicate requires one persona at both endpoints and preserves Container ids.
Copy uses stable-per-transfer UUIDs, remaps the closed relation set and scene,
and delegates node creation to Mere's `copy_node_from_with_id`, retaining a
`CopiedFrom` derivation for every copied object. Copy does not carry source
visit, import, arrangement, or access-cache facets.

Every imported destination object receives a new typed AccessRecord. Included
source AccessRecords remain immutable. A typed transfer receipt records the
source, destination, route, authorization grant, manifest and selection
hashes, blob hashes, id map, counts, destination AccessRecord ids, and result.
The receipt is written only after graph persistence and authority writes
succeed.

## Executable proof

The five focused tests use two independent `MemoryBackend` stores and an
actual temporary file written and read through the OS filesystem. They prove:

- one URL and one file node retain their tags and `Cites` relation;
- replicate preserves both ids;
- copy mints both ids and retains source graph and node provenance;
- the destination stores and addresses the exact file bytes by BLAKE3;
- source history is policy-carried and destination imports append their own
  AccessRecords;
- a revoked grant returns before blob or graph mutation;
- retry preserves the copy id map and AccessRecord cardinality;
- an already-verified destination blob completes after the source blob is made
  unavailable.

Verification:

```text
cargo test -p graphshell --all-features
83 passed; 0 failed

cargo check -p graphshell --no-default-features --features web \
  --target wasm32-unknown-unknown
passed

cargo clippy -p graphshell --all-features --all-targets -- \
  -A clippy::too-many-arguments
passed

cargo fmt -p graphshell -- --check
passed
```

## Physical follow-on

This receipt proves the carrier-ready transfer contract and two-store behavior.
It does not itself claim a physical-device transfer. The
[H6d physical transfer receipt](2026-07-29_h6d_physical_transfer_closure_receipt.md)
composes this contract with the admitted p2panda/Iroh carrier between Windows
and Q-PC, including a transfer interrupted between manifest and blob,
destination application, and live intent revocation.

Source retirement remains outside this slice. `Move` still means a completed
replicate or copy followed by a separately authorized retirement operation.
