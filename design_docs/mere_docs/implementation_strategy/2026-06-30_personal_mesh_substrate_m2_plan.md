# Personal Mesh Substrate M2 Plan

**Date**: 2026-06-30

**Status**: Active next slice; re-scoped 2026-08-09 after ESP consolidation.

**Related**:
[`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md),
[`../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md`](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](2026-06-30_mesh_lease_scheduler_plan.md),
[`2026-08-08_esp_consolidation_plan.md`](2026-08-08_esp_consolidation_plan.md)

M2 turns the M1 convergence proof into a bounded execution substrate. Its whole
claim is one versioned job namespace, one resource registry, and one useful
resource running through both. Leases, GPU scheduling, kith grants, and markets
remain later layers.

---

## 1. Current state

`crates/mesh/mesh` currently has:

- signed `MeshEvent` operations over LogSync;
- a deterministic `JobBoard` fold;
- inline-payload `JobPosted`, `JobClaimed`, and `JobDone` events;
- pure `Echo` and `Blake3` executors;
- retention checkpoints and history pruning; and
- a two-peer rehearsal path.

The checkout has no `JobSpec`, `JobNamespace`, `MeshResource`, or resource
registry. M1 proves transport, storage, and convergence. It does not yet prove
that a worker sees only the inputs and outputs granted to one job.

---

## 2. Ownership boundary

- **Mesh wire and board** own signed job facts and deterministic projection.
- **The host** resolves authorization, constructs the restricted namespace,
  advertises local capabilities, selects work, and supplies cancellation.
- **A resource adapter** prepares and executes only through that namespace. It
  does not read the mesh store, inspect the OS, choose a device, or mutate the
  board.
- **ESP** owns embedding behavior. The mesh adapter calls `esp::embed`; it does
  not reproduce a provider.

A `BlobRef` is an address and integrity commitment, not an authorization token.
The signed spec can name a blob, but only the local host can grant a resource a
reader for it. This is the difference between a namespace-shaped manifest and
an enforced namespace.

---

## 3. M2a: versioned job wire

Do not change the fields of the existing signed CBOR variants. Stored M1
operations must remain decodable and replayable. Add new variants, provisionally
named `JobPostedV2` and `JobDoneV2`; after the cutover, new writes use V2 while
the fold accepts both generations.

The first wire shape is deliberately smaller than the 2026-06-30 sketch:

```rust
pub struct JobSpec {
    pub resource: ResourceId,
    pub inputs: Vec<JobInput>,
    pub output: OutputGrant,
    pub requirements: ResourceRequirements,
    pub determinism: DeterminismClass,
    pub checkpoint: CheckpointClass,
}

pub struct JobInput {
    pub name: String,
    pub blob: BlobRef,
}

pub struct OutputGrant {
    pub name: String,
    pub max_bytes: u64,
}
```

`ResourceId` is an extensible, validated identifier such as
`esp.embed.lexical/v1`, not a closed enum. Registering a new resource must not
require a wire or board edit. Weights are named inputs when a resource needs
them. Scratch, metrics, arbitrary host calls, and cross-ring privacy grants wait
for a consumer rather than becoming speculative M2 vocabulary.

`JobDoneV2` records a content-addressed output plus the resource id and
implementation identity needed to verify it. Result bytes do not return inline.

Done for M2a:

- legacy events still decode and reproduce the same board;
- V2 specs and results survive CBOR and signed-operation round trips;
- malformed or duplicate input names, invalid resource ids, and oversized
  grants are rejected before mutation; and
- mixed M1/V2 replicas converge in a property or permutation test.

---

## 4. M2b: restricted namespace and execution control

The host constructs a `JobNamespaceView` only after checking the job and local
authority. It exposes named reads and the granted output writer. It does not
expose a raw `muniment::Backend`, filesystem handle, network client, or ambient
blob resolver.

Required negative receipts:

- reading a blob not named by the job fails;
- writing another output name fails;
- exceeding `max_bytes` fails without committing a partial output; and
- a digest mismatch fails before the adapter receives bytes.

Execution receives a host-owned control handle with cooperative cancellation.
The first adapter may finish too quickly to exercise preemption, but the seam
must not require a breaking rewrite when M3 owner reclaim arrives. Blob access
is asynchronous; the object-safe resource interface must therefore admit
asynchronous preparation/execution rather than hiding `block_on` inside an
adapter.

---

## 5. M2c: one registry, including M1

The registry maps `ResourceId` to an adapter descriptor and executor. It lives
above wire/store/sync and below the host actor.

Conceptually, each resource supplies:

- stable resource and implementation identities;
- capability matching against host-advertised facts;
- preparation through the restricted namespace;
- execution under the control handle; and
- a declared verification class.

Move `Echo` and `Blake3` behind minimal adapters before adding ESP. The old M1
wire compatibility path may translate its inline payload into an ephemeral
one-input namespace, but the executors themselves should have one route. The
board remains ignorant of adapter types.

---

## 6. M2d: lexical embedding resource

The first useful resource is `esp.embed.lexical/v1`, backed by
`esp::embed::LexicalEmbeddingProvider`:

- input: a canonical batch of UTF-8 texts plus configurable dimensions;
- output: vectors in input order, their dimensions and cosine metric, encoded
  canonically;
- dependencies: ESP without Burn, a GPU, model weights, or tokenizer assets;
  and
- product value: a cheap shared-vocabulary signal suitable for clustering and
  recall rehearsals.

`StubEmbeddingProvider` remains a test double. It hashes whole strings and does
not produce useful similarity; the earlier plan's claim that the hashed
provider was product-useful was wrong.

The verification class must be earned. Run the canonical output on native and
wasm. If the `f32` normalization is bit-identical, record the vectors as an
exact deterministic receipt. If it is not, use an explicit tolerance or add a
canonical integer/quantized result. A content hash proves which bytes were
stored; it does not by itself prove that every device should have produced the
same bytes.

Two-peer receipt:

1. One peer stores and posts a blob-backed text batch.
2. A second peer advertises and claims `esp.embed.lexical/v1`.
3. The host constructs a restricted namespace and the adapter executes.
4. The output is committed by `BlobRef` and `JobDoneV2` converges to both peers.
5. A local rerun verifies it under the declared verification class.
6. A deliberately ungranted blob remains unreadable.

---

## 7. Non-goals and stop rule

M2 does not implement:

- lease expiry, heartbeat, owner reclaim, or reputation;
- OS device discovery or foreground policy;
- Burn, WGPU, remote tensor execution, or training;
- kith grants, bounty escrow, or public verification markets;
- arbitrary Wasm execution; or
- generalized scratch, metrics, or host-call mounts.

Stop after the lexical two-peer receipt and compatibility coverage. The next
serial slice is the revised mesh lease scheduler plan. A long-running, GPU, or
remote resource may not land before that plan proves cancellation and owner
reclaim.

## 8. Done conditions

- V2 jobs are signed, versioned, content-addressed, and backward-readable with
  M1 history.
- Namespace enforcement has positive and negative access tests.
- One registry owns M1 compatibility adapters and the lexical ESP adapter.
- Adding a test resource changes neither `wire.rs` nor `JobBoard::fold`.
- The two-peer namespace-backed job and verification receipt pass.
- Native and wasm lexical result behavior is classified honestly.

## 9. Progress

- **2026-06-30**: split out of the merged resource-coordination brief as the
  namespace manifest plus first-adapter slice.
- **2026-08-09**: re-scoped against the live M1 wire and newly consolidated
  ESP. Preserved signed-wire compatibility with V2 events; replaced the closed
  job-kind direction with an extensible resource id; separated blob addressing
  from host-enforced namespace access; added async/cancellation requirements;
  corrected the first useful adapter from the whole-string stub to the lexical
  provider; and made exact-versus-tolerance verification an explicit receipt.
