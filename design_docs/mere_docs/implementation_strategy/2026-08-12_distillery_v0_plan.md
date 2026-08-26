# Distillery v0 Plan

**Date**: 2026-08-12  
**Status**: D0 complete; D1 resident authority lifecycle complete. Its installed
Personae/settings binding and read-only Cambium surface are implemented pending
the focused Cargo gate, operational host composition, and Turnstone
registration. D2's configured browser embedding matrix, first exact decoder row, lease-bound
remote MiniLM row, and native ModelSession/PEFT LoRA row complete. Cooperative
cancellation, explicit browser device teardown, fresh-worker recovery, exact
remote reclaim ordering, CubeCL allocator cleanup, and real adapter numerical
parity are proven. Driver-level release is proven for the supported plain
remote profile. The immutable TrainingCorpus and EvalReport artifact contract
is ready for the first local trainer receipt. Browser-level physical GPU
allocation release remains unobservable through current browser APIs rather
than an actionable implementation gate.

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
adapter, portable remote checkpoints, tolerant comparators, or training.

## 7. D1 resident authority lifecycle

D1 in this document is Distillery-local. It does not rename the ESP
consolidation plan's D1 device-policy decision.

D1 supplies the reusable process body without hardwiring an identity profile or
an operating-system launcher.

`ResidentSettings` has no default. The embedding owner selects:

- supervisor tick cadence;
- optional maintenance cadence;
- physical blob collection cadence; and
- whether accepted maintenance releases settled mesh custody.

`ResidentStorage` opens a disk-backed collecting `BlobStore` and scopes its
custody tags to one mesh. `ResidentAuthority` rejects a host for another mesh,
owns the composed `Distillery`, p2p transport, and storage, and closes them in
that order: endpoint, joined mesh, blob-store flush.

The first restart receipt exposed a deeper lifecycle distinction: dropping a
LogSync requested actor stop but returned before its redb handle was released.
`p2panda-net::LogSync::shutdown`, `JoinedSpace::leave_and_wait`, and the
consuming mesh-host shutdown path now abort and await local job/drain tasks,
drain every topic manager, await the top-level sync actor's owning task so its
state has actually dropped, then return. Shutdown failure is carried through
the full authority chain while the remaining close operations are still
attempted. A reported clean shutdown therefore permits an immediate same-path
reopen.

The run loop emits ordered `ResidentReceipt` values for exact substrate steps,
completed maintenance, unchanged maintenance, maintenance failure, supervisor
failure, and requested stop. A maintenance refusal does not stop useful work.
That distinction is required because a checkpoint landing during a live lease
is expected to fail closed. A supervisor failure does end the run.

Cadenced maintenance uses `MeshHost::checkpoint_if_advanced`. It compares the
candidate event frontier with the latest accepted checkpoint and leaves an
empty or unchanged mesh alone. The explicit `Distillery::maintain` command
still authors whenever the owner asks.

### D1 receipt

The resident integration receipt uses a real p2panda transport, a redb-backed
`MeshStore`, and a disk-backed collecting blob store. It proves:

1. The configured tick cadence drives a V2 job to completion.
2. Scheduled maintenance releases the input and output custody tags.
3. The next maintenance turn reports an unchanged frontier without another
   checkpoint.
4. Shutdown is observed and closes the endpoint before persistent storage.
5. Reopening the redb mesh store replays the terminal job.
6. Reopening the collecting blob store preserves the custody release.

The 2026-08-20 receipt passed all three Distillery tests, including immediate
same-path redb/blob reopening, and strict Clippy for the Distillery targets and
the changed `p2panda-net` library. Shared Cargo cache contention required an
isolated manifest over the exact source files; the source and test targets were
unchanged.

## 8. Installed authority boundary

The first installed slice persists one explicit Personae `ProfileId` at
`<data-root>/distillery/settings.json`. It is a product-owned, local-only,
restart-required ordinary setting. It has no default and does not consult a
Distillery profile environment variable: an absent record is unconfigured, an
empty or unknown field is refused, and an absent selected profile fails rather
than minting a second face. The profile's mesh author derives under
`mesh::MESH_AUTHOR_SALT`; its master key supplies the transport identity. The
settings record carries neither seed nor vault secret.

Per-mesh private persistence sits at
`<data-root>/distillery/meshes/<mesh-id>/{mesh.redb,blobs}`. The reusable
`InstalledAuthority` layer opens the persisted profile, derives the author and
attestation, and binds a `ResidentAuthority` only when a caller supplies the
`MeshStore`, `HostConfig`, and `ResidentSettings` it owns. The mesh caller
therefore retains admission/retention truth; the device caller retains
resources, scheduler, conditions, and device policy. Distillery does not make
defaults for any of those facts.

`distillery-installed` is an executable bootstrap boundary, not yet a fully
operational standalone host. Its `configure` verb persists the explicit
profile; `inspect` proves that it opens from Personae and reports protection.
Starting a resident remains gated on a product composition owner supplying a
real mesh retention/admission policy and device `HostConfig`/`ResidentSettings`.
Those constructors exist through `InstalledAuthority`; the focused installed
compiler gate remains open. A CLI default here would be fabricated
scheduler/device authority.

Distillery now also homes D2's standalone browser development probe without
making it product chrome. The recovered BrowserWebGpu path now passes four
pinned embedding artifacts from 34.8 MB F16 BGE Micro through 438.0 MB F32
E5-base. Cold and warm execution pass integrity, independent numerical
reference, repeatability, GPU-error, and worker-cutoff gates. The persistent
storage request resolved `false`, so the 698 MB IndexedDB corpus remains a
reconstructible best-effort cache rather than durable product storage.

The first D2 decoder row now passes a pinned 269,060,552-byte BF16 SmolLM2
artifact in headed Chromium. Cold and warm workers reproduce the independent
Transformers and ESP NdArray token sequence exactly, stream every fragment to
the page, reopen matching IndexedDB content, stay below the configured frame
p95 bound, and report no WebGPU validation errors. The row forced Llama's
split-half rotary rule and async browser token readback into ESP. The lifecycle
receipt now cancels before the next token crosses ESP's observer boundary,
destroys the worker's tracked `GPUDevice`, terminates cleanly, and reproduces
the exact output in a fresh worker. Physical GPU-allocation release remains
unobservable. The upper embedding and decoder boundaries do not need larger
rows until a consumer asks for them.

The first Cambium surface now follows `ResidentReceipt` after the installed
authority selects its Personae profile and settings location. It renders only
the selected profile/protection, mesh-local paths, bound resident settings, and
the latest exact observer receipt; it owns no scheduler, retention, device,
job-board, or session copy. Broad job/device/session projection waits for each
authority to expose an equally truthful snapshot. The prerelease Burn 0.22
migration has executed for ESP and Quint; stable repinning and package closure
remain release-gated. The lease-bound Burn Remote source audit and targeted
session-close seam are complete, including live-request interruption,
stop-before-reclaim ordering, zero sessions, and fresh-lease recovery.

The allocator sidequest is complete through the patched CubeCL
`ComputeClient::memory_usage()` seam. Both plain-WGPU MiniLM sessions rose from
zero to 101 live allocations and 90,261,504 bytes in use, then returned exactly
to the zero baseline before their reclaim facts. Reserved bytes are recorded
but remain outside the allocator gate. A separate Windows driver receipt now
measures 604,114,944 and 637,603,840 dedicated bytes released across the two
plain-profile cycles, zero retained growth across reclaim baselines, NVIDIA GPU
0 attribution, and counter disappearance after process exit. Burn Remote now
keeps draining sessions visible until worker cleanup, propagates worker failure
through Distillery, and wakes detached writers on close.

The bounded feature matrix passes every local row and remote plain. Remote
Fusion plus autotune completes inference but retains five live allocations;
remote autotune-only becomes timing-sensitive through inference/reclaim, while
remote Fusion-only stalls during first provider load. This is an upstream
Burn/CubeCL remote-backend compatibility lane, not a Distillery lease-adapter
gate. Plain WGPU remains the supported remote profile.

Model manifest browsing, streaming console, portable remote checkpoints,
tolerant comparators, a real stacked-adapter row, and training remain separate
slices.
## 9. Training artifact boundary

The first local trainer vertical needs three immutable Eidetic artifacts:
`ModelAdapterManifest`, `TrainingCorpus`, and `EvalReport`. The first is
already the ModelSession provenance envelope. `TrainingCorpus` now separates
non-empty, strictly manifest-id-ordered training and held-out evaluation source
partitions; overlap is rejected before the artifact is stored. `EvalReport` is
a fixed-corpus baseline-versus-adapter `RecallAt` or `RankingAt` receipt using
integer passes and total cases. It rejects zero limits, impossible tallies, and
comparisons over different case counts; it also validates the adapter manifest
itself and then checks its model, adapter, and corpus links.

This is Eidetic's artifact contract only. It does not create a trainer, pick a
runtime or optimizer, assign a Mesh resource, record a lease/checkpoint, or
make Distillery infer device policy. Those facts remain respectively ESP,
Mesh, and the host scheduler's authorities.

### Next forcing task

Run one local, deterministic recall or ranking fixture that materializes a
canonical `TrainingCorpus`, publishes an adapter manifest with that corpus as
its provenance, then writes an `EvalReport` showing the adapter strictly beats
the unchanged baseline on the same fixed cases. That receipt decides the first
trainer resource input/output shape; no trainer framework is justified before
it exists.

### Progress

- **2026-08-26, installed authority/settings slice**: Distillery now stores an
  explicit Personae profile selection under its private data root and refuses
  a missing or malformed selection. `InstalledAuthority` opens that existing
  profile from Personae, derives the stable mesh author and attestation, uses
  the profile master for p2p transport, names private mesh paths, and composes
  a resident only from caller-owned mesh and device facts. The
  `distillery-installed` binary is the configure/inspect bootstrap boundary;
  it records the deliberate resident-start gate instead of hiding it behind a
  permissive device or cadence default. Formatting and diff checks passed;
  focused compilation and Clippy remain open after package-cache contention.
- **2026-08-26, artifact foundation**: Eidetic gained typed `TrainingCorpus`
  and `EvalReport` payloads with fixed schema references, validating
  serialization, and typed-store round trips. The corpus uses separate,
  disjoint training and evaluation partitions; the report deliberately records
  only deterministic ranking/recall counts and validates a well-formed adapter
  plus the provenance links already reserved by `ModelAdapterManifest`. No
  training, evaluation runtime, Mesh job, lease, checkpoint, or Distillery
  authority was added.
- **2026-08-26, read-only contributed surface**: The
  `distillery.installed.v1` descriptor and erased Cambium session consume the
  installed authority's profile/path projection and only resident facts the
  caller supplies from the real resident (`ResidentSettings` plus its latest
  `ResidentReceipt`). It opens no authority and has no mutable controls. This
  is the second product half of the Knot contribution seam; a Turnstone
  registration/admission receipt remains P0 work, along with generic AccessKit
  projection and a full-shell build. Focused surface compilation and Clippy are
  pending the same shared Cargo package-cache lock.
