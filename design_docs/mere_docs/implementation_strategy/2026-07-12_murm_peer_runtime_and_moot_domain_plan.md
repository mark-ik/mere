# Murm Peer Runtime and Moot Domain Plan

**Status**: Active plan.
**Date**: 2026-07-12.
**Scope**: Recast Murm as the reusable peer-exchange family, make Moot a
governed-space domain over Murm's replication foundation, and remove p2panda
session assembly from Merecat. Preserve Mere as an offline-first graph library.
**Supersedes**:
[`2026-07-08_murm_moot_sibling_posture_plan.md`](2026-07-08_murm_moot_sibling_posture_plan.md).
That plan's completed consolidation and store work remain valid receipts. Its
purity rule and host-composition target are retired.
**Related**:

- [`../../../../merecat/design_docs/2026-07-08_merecat_founding.md`](../../../../merecat/design_docs/2026-07-08_merecat_founding.md)
  fixes Mere as the offline graph library and Merecat as a reference host.
- [`2026-07-12_deletion_retention_and_native_drop_plan.md`](2026-07-12_deletion_retention_and_native_drop_plan.md)
  defines pruning, retention, checkpoints, and asynchronous carriage over the
  replication foundation established here.
- [`../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md`](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md)
  remains useful history for direct exchange, but its claim that bilateral
  communication defines the whole Murm family is superseded here.
- [`2026-07-06_comms_gating_and_key_addressing_plan.md`](2026-07-06_comms_gating_and_key_addressing_plan.md)
  owns native sealed mail and comms-facing key addressing.
- [`2026-06-06_moot_constitution_brief.md`](2026-06-06_moot_constitution_brief.md)
  owns the distinction between constitutional authority and governed settings.

---

## 1. Decision record

1. **Murm is the peer-exchange family.** Its lower layer owns reusable peer
   transport, replicated-space processing, persistence, retention mechanics,
   and native drop carriage. Direct conversation is one Murm domain over that
   layer rather than the definition of the shared machinery.
2. **Moot is a governed-space domain over Murm replication.** Moot owns
   durable community identity, membership, roles, constitution, recognition,
   tessera, moderation, retention decisions, and community projections.
3. **The dependency is semantic, not facade-shaped.** The Moot package depends
   on `murm-replication`, not on the high-level `murm` conversation API. Mesh
   and future shared domains use the same lower crate.
4. **Mere stays offline-first.** Mere's core graph, storage, and projection
   libraries remain useful without network dependencies. Optional adapters may
   project Murm or Moot state into Mere graph facts.
5. **Merecat configures services.** It supplies persona and wallet handles,
   storage roots, settings, transport availability, executor access, and wake
   integration. It consumes domain commands, snapshots, events, and status.
   It does not build `LogSync`, drain p2panda events, or decide retention law.
6. **One accepted-operation path.** Local authoring, gossip, LogSync, and drop
   import pass through the same processor before storage and materialization.
   Each domain supplies addressing, verification, authorization, and fold
   behavior.
7. **Relationship and governance define the product boundary.** A murmur is
   invited exchange whose identity comes from its participants. A moot is a
   durable governed space whose identity survives membership changes. Member
   count does not select the layer.
8. **Promotion follows the corrected shape.** `repos/murm` becomes a
   multi-crate peer-exchange library. `repos/moot` becomes a domain library
   over `murm-replication`. Mere re-bases onto the promoted libraries only
   after their standalone tests pass.

## 2. Why the sibling posture is retired

The live dependency graph has already crossed the former purity boundary:

- `transport` sits under `crates/murm` but is consumed by mesh, Moot tests,
  eidetic Iroh fetching, and the old host.
- `mooting::MunimentStore` sits under `crates/moot` but is deliberately generic
  and is consumed by mesh.
- `mesh` depends on both families to obtain one replication path.
- the old host builds the tessera `LogSync` session and its `SyncedSpace`
  callback directly. A new Mere consumer would have to reproduce that wiring.
- `murmuring` still presents a speculative protocol abstraction whose only
  required method is `name()`, while its sole substantial implementation is
  the native p2panda conversation model formerly called Cable.

The sibling plan solved duplicated code, but placed the resulting halves under
different product domains and made the application restore the invariant. That
shape fit a monolithic host better than a reusable library ecosystem.

## 3. Target architecture

```text
personae / wallet
       |
       v
repos/murm
  peer transport       Iroh, Retinue adapter, tickets, blobs
       |
  murm-replication      endpoint/session lifecycle, processor, muniment store,
       |                retention mechanics, checkpoints, native drop
       |
       +----------------+------------------+
       |                |                  |
  direct exchange   repos/moot          mesh and other domains
  conversations     governed spaces     jobs, collaboration, sharing
       |                |
       +--------+-------+
                |
        optional Mere projections
                |
             Merecat
        settings, UI, lifecycle
```

### 3.1 `murm-replication`

This is the shared foundation. It owns:

- the existing p2panda endpoint and sync-session lifecycle;
- the existing `SyncedSpace` drain behavior;
- the muniment-backed `OperationStore`, `LogStore`, and `TopicStore` adapter;
- one policy-before-insert operation processor;
- atomic topic, operation, payload, checkpoint, and prune mutations;
- sync status and resync control;
- retention and prune mechanics, with domain policy injected;
- the native drop logical stream and staged importer;
- carrier-neutral transfer commands and progress events.

It does not assign social meaning. The domain contract supplies, at minimum:

```rust
pub trait ReplicatedDomain<E> {
    type SpaceId;
    type Materialized;

    fn address(&self, operation: &Operation<E>) -> Result<Self::SpaceId, Reject>;
    fn verify(&self, operation: &Operation<E>) -> Result<(), Reject>;
    fn authorize(
        &self,
        operation: &Operation<E>,
        context: &AuthorityContext,
    ) -> Result<(), Reject>;
    fn apply(
        &self,
        state: &mut Self::Materialized,
        operation: &Operation<E>,
    ) -> Result<(), Reject>;
}
```

This signature is illustrative. The implementation may split validation,
authorization, checkpoint authority, and materialization into smaller traits.
The fixed rule is that the runtime owns sequencing and mutation while domains
own meaning and permission.

### 3.2 Murm direct exchange

Murm's public conversation service owns:

- invitations and participant-derived conversation identity;
- signed message, topic, membership, withdrawal, and key-change events;
- conversation history and subscriptions;
- native sealed mail and direct attachment references;
- private-group policy and key rotation;
- adapters for Misfin or other exchange protocols at the edge.

The current p2panda-native conversation is not Cable interoperable. Its domain
types should migrate from `Cabal*` and `Cable*` toward `Murmur*` and
`Conversation*` as touched. External protocols retain their own semantics
behind the existing comms adapter boundary.

### 3.3 Moot

Moot owns:

- `MootId`, declaration, lifecycle, roster, roles, and fauna;
- constitution and amendment evaluation;
- governed configuration, including retention policy revisions;
- recognition and tessera facts;
- capability and membership authorization;
- moderation and withdrawal semantics;
- deterministic community materializations and graph projections;
- policy for voluntary hosting and federation.

The current `mooting::RecognitionContext` stays in this family. The generic
`MunimentStore` leaves it. The current `moothold::moot` module becomes the seed
of the public single-moot service. The `moothold` name is reserved for actual
multi-moot holding or federation behavior once such behavior exists.

### 3.4 Mere and Merecat

Mere may provide graph-facing adapters such as `MootProjection` or
`ConversationProjection`. They consume stable domain snapshots and events.
They do not own peer sessions or group policy.

Merecat owns service lifecycle and product interaction:

- enable, disable, and configure peer services;
- select persona, storage root, transport preference, and retention settings;
- display connectivity, sync, retention, and deletion status;
- lower UI actions into Murm or Moot commands;
- project returned domain state into Mere through the optional adapters.

## 4. Package movement

| Current item | Target | Reason |
|---|---|---|
| `crates/murm/transport` | Murm peer-transport crate | Already the shared endpoint and carrier layer |
| `transport::SyncedSpace` | `murm-replication` | It is replicated-space processing, not byte transport |
| `mooting::MunimentStore` | `murm-replication` | Generic persistence used outside Moot |
| new retention/checkpoint/drop machinery | `murm-replication` | Shared mechanism for every replicated domain |
| `mooting::RecognitionContext` | Moot policy module | Community authorization semantics |
| `murmuring` native conversation implementation | `murm` domain modules | One real native protocol does not justify a plugin crate |
| `BilateralProtocol` | retire unless a second native implementation needs it | The comms adapter is the real cross-protocol boundary |
| `moothold::moot` | public Moot service | It already contains the single-moot grammar and fold |
| `moothold` federation name | future multi-moot package | Keep the word aligned with its product meaning |
| host `LogSync` and accept closures | library services | Consumers should not reconstruct protocol invariants |

Compatibility re-exports may preserve current imports during the in-workspace
move. They are removed before standalone promotion.

## 5. Implementation phases

### Phase A: establish the replication crate

**Implementation status (2026-07-12):** structural move landed. The new crate
and its own tests are green; full workspace consumer checks remain pending the
workspace resolver issue recorded in Progress.

Create `murm-replication` in the current workspace. Move, rather than copy:

1. `SyncedSpace`, `SyncStatus`, and `SyncRound` from `transport`;
2. `MunimentStore` and its p2panda store implementations from `mooting`;
3. the p2panda-core/store/net/stream dependency surface needed by both;
4. shared test fixtures for signed operations and two-peer convergence.

Keep temporary re-exports in `transport` and `mooting`.

Done when:

- mesh, murm, and both Moot lanes compile against `murm-replication`;
- `transport` contains byte transport, endpoint, blobs, and carrier adapters,
  but no operation drain;
- `mooting` contains recognition and Moot policy, but no generic store;
- existing convergence tests pass without a host-local processor copy.

### Phase B: one processor and runtime service

Build the shared accepted-operation processor and a reusable runtime service.
The runtime takes injected persona/wallet, muniment backend factory, transport
configuration, and executor access. Domains register their operation extension
and policy implementation.

Implementation status, 2026-07-13: the shared processor foundation is landed.
Mesh and Tessera local authoring and LogSync receipt now share it. Tessera's
current admission checks its signed Moot address and event wire grammar; it
does not claim Moot membership or constitutional authorization. The upstream
prune proof passes against `MunimentStore`, and the production processor now
accepts an authorized prune action, enforces a strictly advancing retained
frontier, and commits the surviving operation plus prefix removal in one
backend batch. Mesh continues to request `Keep`. The reusable runtime service,
checkpoint law, production domain flag wiring, gossip receipt, and drop import
remain.

Order operations as follows:

1. decode and bound resource use;
2. verify header, signature, and space address;
3. evaluate capability and domain authorization;
4. validate backlink, frontier, checkpoint, and prune law;
5. commit operation, topic, payload, checkpoint, and prune effects atomically;
6. apply or refresh the deterministic materialization;
7. emit a typed domain event and sync-status change.

Done when:

- local authoring, LogSync receipt, gossip receipt, and drop import return the
  same decision for one operation corpus;
- unauthorized or stale operations leave the live store unchanged;
- the runtime can join, leave, resync, and report one space without exposing a
  p2panda type to its consumer;
- injected settings control limits and transport preference.

### Phase C: rebase Murm direct exchange

Move the native conversation engine onto `murm-replication`:

- replace `PersistentCabalStore` as sync authority with the shared muniment
  store and a conversation materialization;
- route local sends and remote receipt through the common processor;
- retain a supported in-memory backend for tests;
- fold `murmuring` into `murm` unless a concrete second native protocol has
  appeared by this phase;
- migrate user-facing and touched code from Cable/cabal vocabulary;
- preserve the high-level send, history, membership, and subscribe behavior.

Done when:

- direct exchange has no hand-rolled p2panda log index;
- durable reopen and two-peer catch-up pass on the shared store;
- membership revision and key rotation are distinct and tested;
- the comms adapter consumes only the high-level Murm service.

### Phase D: rebase Moot as a domain service

Separate Moot policy from generic machinery and expose a high-level service:

- keep declaration, roster, recognition, constitution, tessera, and moderation
  with Moot;
- register Moot validation, authorization, checkpoint authority, and folds
  with `murm-replication`;
- replace the current SQLite Moot store with the shared backend;
- expose commands and snapshots for declare, join, leave, share, amend,
  withdraw, checkpoint, and inspect retention status;
- bind retention settings to a governed policy revision authorized by the
  constitution, rather than treating the settings as constitutional clauses.

Done when:

- a consumer joins a Moot through one domain API and imports no p2panda crate;
- the same Moot converges through live sync and native drop;
- unauthorized membership, checkpoint, retention, and prune operations fail
  before mutation;
- tessera and constitution remain separate policy rings with explicit inputs.

### Phase E: move the remaining consumers

Rebase mesh first, then other shared domains such as Isometry collaboration.
Each keeps its event grammar and materializer while deleting custom session and
store assembly.

Done when:

- mesh depends on `murm-replication` rather than both `transport` and `mooting`;
- one backend and processor implementation serves conversation, Moot, and mesh;
- each domain's two-peer tests cover online exchange, offline catch-up,
  withdrawal, checkpoint, and late-peer return.

### Phase F: remove host protocol assembly

Replace `meerkat/src/sync.rs` and future Merecat equivalents with service
adapters. The app provides configuration and receives typed status/events.

Done when:

- Merecat and the retiring in-workspace host have no direct `p2panda-net`,
  `p2panda-sync`, or store-trait dependencies;
- connect, disconnect, resync, import, export, and status are ordinary service
  commands;
- the app can run fully offline with peer services disabled;
- a headed receipt shows honest connected, catching-up, retained, body-erased,
  and prefix-pruned states.

### Phase G: promote the corrected families

Promote only after Phases A through F establish the public seams:

- `repos/murm`: peer transport, `murm-replication`, and direct exchange;
- `repos/moot`: governed-space domain over the branch-tracked Murm dependency;
- Mere: optional projection adapters, with peer dependencies absent from its
  default core;
- Merecat: direct dependencies on Mere, Murm, Moot, Personae, and Serval.

Done when fresh standalone clones build and test, committed manifests contain
no local paths, Mere builds offline without peer features, and Merecat performs
the same two-peer scenario through the promoted libraries.

## 6. Deletion and native-drop consequences

The related deletion plan changes owner, but not its core law:

- `murm-replication` owns the processor, prune mechanics, checkpoint storage,
  native drop logical stream, and staged import;
- domains own withdrawal semantics, checkpoint authority, governed retention
  policy, and materialization;
- Retinue owns link/resource segmentation and resume;
- Personae/wallet owns key custody, recovery, enrollment, and capability slots;
- blob-domain owners decide liveness of their own content before collection.

The native drop v0 freezes a cover, canonical manifest, protected body, and
self-delimiting record stream. It does not define Retinue-style resumable
fragments. Proof and checkpoint records use the shared typed commitment and
digest vocabulary rather than raw arrays and string proof kinds.

Plan ordering is explicit:

1. Phase A here creates the destination crate.
2. Deletion D0 proves upstream pruning inside that crate.
3. Phase B here and deletion D1 are the same processor milestone.
4. Deletion D2 establishes retention checkpoints before Murm and Moot expose
   destructive retention commands in Phases C and D.
5. Deletion D3 and D4 add file carriage and staged import after one domain has
   moved through the processor.
6. Deletion D5 waits for Retinue's resource layer and real private protection.

## 7. Stop rules

- Do not move generic replication machinery into Moot under a community-shaped
  name.
- Do not make `moot` depend on the direct-conversation Murm facade.
- Do not leave operation verification or insertion callbacks in Merecat.
- Do not preserve `murmuring` solely for a hypothetical future protocol.
- Do not make Mere's default graph library depend on network runtimes.
- Do not combine key custody, carrier framing, social authorization, and blob
  liveness in the replication crate.
- Do not promote the current folder layout and repair its public boundary later.

## Findings

### 2026-07-12: library extraction changes the useful purity rule

The former `murm has no store` and `moot has no socket` rules forced reusable
mechanism into two product families and required the application to join it
again. That is the wrong cost placement once Mere is a library and Merecat is
one consumer among several. The durable purity rule is now: the replication
foundation owns mechanism, domains own meaning and authority, and applications
own settings and interaction.

### 2026-07-12: the live tree already contains the migration map

`SyncedSpace` is the shared receive loop, `MunimentStore` is the shared durable
adapter, and mesh proves that both are useful outside their current family
folders. The first phase is therefore a move and API consolidation, not a new
protocol design.

## Progress

### 2026-07-12

- Reframed Murm and Moot against the current Mere library and Merecat host
  boundary.
- Audited current dependency edges and the public APIs of `transport`, `murm`,
  `murmuring`, `mooting`, `moothold`, mesh, and the old host sync lane.
- **Phase A structural move landed.** Created `murm-replication`; moved the
  complete `SyncedSpace`/`SyncStatus`/`SyncRound` drain and
  `MunimentStore` implementation into it; left compatibility re-exports in
  `transport` and `mooting`; rewired Murm, both Moot test lanes, mesh, and the
  old host to import the new crate directly. Mesh no longer depends on
  `mooting` or normal-dep `transport` for replication.
- Updated crate READMEs and module ownership prose. Workspace metadata confirms
  the intended normal/dev dependency directions, and a stale-import audit finds
  no direct consumer of the compatibility paths.
- Verification: the real moved source compiled in an isolated manifest and its
  three store tests passed (ordered log replay, topic resolution, prefix prune);
  rustfmt check over every touched Rust file and `git diff --check` passed.
  Normal workspace package commands repeatedly remained in full-workspace
  source resolution until timeout before spawning rustc. Offline verification
  exposed the unrelated root patch problem: the locked Serval `ipc-channel`
  source was not cached, followed by a local Stylo package-name mismatch. Murm,
  Moot, mesh, and old-host package checks therefore still need a normal
  workspace rerun after that resolver state is repaired.

### 2026-07-13

- Accepted the peer-runtime/domain direction as current architecture and audited
  the existing Phase A move rather than recreating it.
- Confirmed the workspace dependency graph: mesh now takes replication directly;
  `transport` and `mooting` retain compatibility re-exports; Murm, both Moot
  lanes, and the old host import `murm-replication` explicitly.
- Targeted rustfmt over every moved or rewired Rust file and `git diff --check`
  pass. Stale sibling-phase wording was removed from the deletion plan, which
  now also states that native drop carries the separately owned sealed mail
  object rather than defining another message envelope.
- A fresh `cargo test -p murm-replication`, including an offline retry, again
  stalled during workspace dependency resolution before compilation and was
  stopped. The prior isolated three-test receipt remains the available runtime
  verification; consumer package tests remain pending the resolver repair.
- Added `OperationProcessor` with injected domain admission, structural and
  operation-id validation, idempotence, ordinary backlink continuity, and
  typed outcomes. Rejections occur before mutation.
- Made topic association, log indexing, and operation-pointer storage one
  `MunimentStore` backend batch.
- Routed mesh local authoring and LogSync receipt through the same processor.
  Mesh retains its body grammar and addressed-mesh policy.
- Focused workspace verification now passes all thirteen
  `murm-replication` tests and all twenty-seven mesh tests, including late-joiner
  catch-up, two-peer work exchange, rejection-before-mutation, wire-compatible
  optional `PruneFlag`, prefix removal, and anti-resurrection. The first mesh
  run reached the test binary as its timeout closed the pipe; an immediate
  rerun passed. The remaining workspace noise is limited to unused Serval patch
  warnings for `graft-engine` and `weld-engine` in these package graphs.
- The D0 proof confirms a boundary correction: p2panda-stream's `LogPrune`
  invokes `prune_entries` separately after ingestion. The production processor
  must put accepted-operation and prune effects behind one muniment transaction
  rather than treating the upstream processor as an atomic ingest primitive.
- Landed that atomic boundary in the production processor. `Admission` carries
  `Keep` or `PruneBeforeCurrent`, plus authorized body erasures; the processor
  combines prune-aware backlink validation with a strict retained-frontier
  check; muniment applies body stripping, prefix deletes, and the surviving
  indexed operation in one batch.
- Landed the mesh retention vertical slice over that boundary: separate event
  and checkpoint logs, policy-bound checkpoints, monotone frontiers,
  snapshot-plus-tail replay, flagged prune points, and terminal-input erasure
  with compact results retained. Shared commitment/blob references, blob GC,
  and Moot checkpoint governance remain later work.
- Landed the first native drop slice in `murm-replication`: the plaintext/public
  cover, canonical manifest identity, operation/payload/blob/evidence records,
  bounded staged visitation, and golden/error vectors. This is carrier framing
  only. Protected suites and the D4 importer/export selector remain open.
