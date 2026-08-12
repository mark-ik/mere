# Distillery v0 Plan

**Date**: 2026-08-12  
**Status**: Complete for D0, the authority projection and retention sweep.

## 1. Purpose

Distillery is Mere's model-works port. D0 proves the port by making it the first
real consumer of `mere-mesh-host`, rather than founding another scheduler or
starting with ornamental views.

The port owns product composition. Mesh owns job, lease, checkpoint, and safe
retention truth. `mere-transport` owns physical blob storage. Distillery drives
the host and decides when owner-governed maintenance runs.

## 2. D0 contract

`Distillery<B>`:

- owns a `MeshHost<B>` and drives one non-blocking supervisor tick at a time;
- returns the host's exact `Step` receipts;
- owns configurable retention behavior, with collection off by default;
- explicitly authors an owner-governed checkpoint before collecting;
- asks `MeshStore` for the safe blob set rather than reconstructing mesh truth;
- releases only this mesh's custody tags; and
- reports the resulting `RetentionEffect::BlobCollected`.

The maintenance report distinguishes candidates from tags actually released.
That distinction matters because repeated custody release is idempotent and a
different mesh or subsystem may retain the same physical content.

## 3. Findings forced by the implementation

### Shared hashes cross job boundaries

The original H2 query considered only terminal jobs in the accepted checkpoint.
That was insufficient for content-addressed storage. A job posted after that
checkpoint can reuse the same input hash, so deleting the older job's input
would also delete the newer job's input. `collectable_blobs` now compares the
checkpoint with the current board tail, protects every reference not settled
by that checkpoint, and deduplicates the release set.

### Custody is not physical ownership

The transport store is shared infrastructure. Anonymous permanent tags could
never be released, while deleting bytes by hash would erase every subsystem's
copy. The store now supports stable named custody tags and collecting memory or
disk modes. Mesh tags include the mesh id and content hash. Removing one tag
does not remove an Eidetic, other-mesh, or other-subsystem claim; garbage
collection removes physical bytes only after the last tag is gone.

## 4. Files

- `ports/distillery/src/authority.rs`: product authority and maintenance report.
- `ports/distillery/tests/authority.rs`: real joined-mesh D0 receipt.
- `crates/mesh/host/src/host.rs`: explicit checkpoint operation for a product host.
- `crates/mesh/host/src/courier.rs`: mesh-scoped transport custody.
- `crates/mesh/mesh/src/retention.rs`: checkpoint plus current-tail safety rule.
- `crates/murm/transport/src/blobs.rs`: named tags and collecting stores.

## 5. Acceptance receipt

The Distillery integration test joins a real one-host mesh over the p2panda
transport, stages an input in the transport's blob store, posts and executes
`mesh.blake3/v1`, and proves:

1. Distillery drives the host to completion.
2. Completion alone retains both input and output.
3. Explicit maintenance authors and accepts a checkpoint.
4. The maintenance report names two candidates and two released custody tags.
5. Store garbage collection removes the unowned input.

Lower-level receipts prove a shared hash remains while another subsystem's tag
exists, a post-checkpoint job protects a reused hash, the disk-backed collecting
constructor actually reclaims released bytes, and existing host blob delivery
still completes across disjoint stores.

Verified from `C:\Users\mark_\Code\repos\mere` with an isolated
`CARGO_TARGET_DIR=C:\t\distillery-v0`:

- `cargo test -p distillery`: 1 integration receipt passed.
- `cargo test -p mere-transport --lib`: 39 tests passed.
- `cargo test -p mere-mesh --lib`: 117 tests passed.
- `cargo test -p mere-mesh-host`: 10 unit and 2 real-host integration tests
  passed.
- strict all-target Clippy passed for Distillery and the three touched substrate
  crates with `--no-deps -- -D warnings`.

## 6. Stop boundary

D0 does not create the resident process or lifecycle, Cambium views, the model
manifest browser, streaming console, Burn resource migration, Burn Remote
adapter, portable remote checkpoints, tolerant comparators, or training. The
stable Burn migration remains the next serial mesh-host gate; the lease-bound
remote adapter follows it.

The next Distillery-native slice should be a resident authority process with a
disk-backed collecting store and owner-configurable retention settings. Views
should follow that authority and render its receipts rather than inventing a
parallel state model.
