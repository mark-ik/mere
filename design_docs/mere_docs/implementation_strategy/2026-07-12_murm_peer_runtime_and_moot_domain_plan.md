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
- `murmuring` presented a speculative protocol abstraction whose only required
  method was `name()`. It has since been folded into `murm`; the native model is
  a Cable-shaped p2panda dialect owned by the conversation service.

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

The current p2panda-native conversation does not claim cabal-club Cable wire
interoperability. Keep `Cabal*` for the invitation-scoped protocol object and
Cable for the inherited cabal/channel/post grammar and namespaced Mere dialect.
Use `Conversation*` for runtime and storage mechanics and **murmur** in product
surfaces. External protocols retain their own semantics behind the existing
comms adapter boundary.

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
`MunimentStore` leaves it. The current `gemot::moot` module becomes the seed of
the public single-moot service. `moothold` is the Tier 3 package for multi-moot
holding and federation behavior.

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
| `gemot::moot` | public Moot service | It already contains the single-moot grammar and fold |
| `gemot::tessera::{concord, reciprocity}` | `moothold` | These compose facts and obligations across moots |
| host `LogSync` and accept closures | library services | Consumers should not reconstruct protocol invariants |

Compatibility re-exports may preserve current imports during the in-workspace
move. They are removed before standalone promotion.

## 5. Implementation phases

### Phase A: establish the replication crate

**Implementation status (updated 2026-07-14):** structural move landed and the
Murm, Comms, and Meerkat consumer checks are green. Compatibility re-exports in
`transport` and `mooting`, plus mesh's direct transport/p2panda dependencies,
still keep this phase from being cleanly closed.

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
backend batch. Mesh now exercises checkpoint, erasure, and prefix-prune actions;
Murm routes local authoring, gossip, LogSync, and drop import through the shared
processor. A reusable multi-domain service API, remaining domain checkpoint
laws, and peer command wiring remain.

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
- **(amended 2026-07-26)** the runtime can join, leave, resync, and report one
  space without its consumer naming a session, subscription, or topic type.
  Was "without exposing a p2panda type", which cannot hold and should not: the
  `accept` closure is handed the domain's own `Operation<E>`, and `Endpoint` +
  `Gossip` pass through from the host's `sync_parts`. **Met** by
  `JoinedSpace`; the session types now appear in two files, both inside
  `murm-replication`. Reasoning in the JoinedSpace finding below;
- injected settings control limits and transport preference.

### Phase C: rebase Murm direct exchange

Move the native conversation engine onto `murm-replication`:

- replace `PersistentCabalStore` as sync authority with the shared muniment
  store and a conversation materialization;
- route local sends and remote receipt through the common processor;
- retain a supported in-memory backend for tests;
- fold `murmuring` into `murm` unless a concrete second native protocol has
  appeared by this phase;
- keep Cable/Cabal at the explicit domain boundary, `Conversation*` in the
  machinery, and **murmur** in user-facing copy;
- preserve the high-level send, history, membership, and subscribe behavior.

Done when:

- direct exchange has no hand-rolled p2panda log index;
- durable reopen and two-peer catch-up pass on the shared store;
- membership revision and key rotation are distinct and tested;
- the comms adapter consumes only the high-level Murm service.

Implementation status, 2026-07-14: the runtime cutover is landed. Murm-owned
`ConversationEngine` uses `ConversationStore` as the single
authority for local authoring, gossip receipt, and LogSync, then materializes
history and subscription events only after `OperationProcessor` reports an
insert or payload hydration. `CabalHandle` send and ingest are async; history,
membership, and subscriptions remain synchronous views. The comms adapter now
awaits the high-level send API. The duplicate `murmuring::CableEngine`, both
legacy cabal stores, their hand-written p2panda store adapter, and the
single-implementation `BilateralProtocol` trait are retired. A configured redb
backend now reopens history and the author head; native-drop import refreshes
history and subscriptions through the same processor. The Cabal/Cable naming
boundary above is deliberate. Meerkat selects that redb backend under its
per-user comms directory. Epoch-aware native-drop key rotation preserves
`CabalId`; group authorization and key distribution remain external. The
signed-operation grammar is now part of `murm`; the `murmuring` package has been
removed from the workspace.

### Phase D: rebase Moot as a domain service

Separate Moot policy from generic machinery and expose a high-level service:

Implementation status, 2026-07-14: the first governance substrate is landed.
`gemot::constitution` now owns signed genesis and amendment events, a
deterministic founder-only fold, memory/redb stores over `MunimentStore`, and
the governed authorization seam. The accepted constitution revision and signer
set construct Moot checkpoint authority. `MootGovernance` supplies plain
commands and snapshots, and its constitution log has a live late-peer test.
The declaration/roster lane now also uses `MunimentStore` and the shared
processor, with separate event/checkpoint logs, monotone snapshot frontiers,
constitution-bound checkpoint admission, and authorized prefix pruning.
Broader membership/capability policy remains.
Federation-level concord composition and reciprocity now live in the distinct
`moothold` package; `MootId` remains owned by `gemot` and is shared across that
boundary. `moothold 0.1.0` also owns a founder-signed aggregate with explicit
member terms, deterministic revision folding, and a durable redb-backed store.

- keep declaration, roster, recognition, constitution, tessera, and moderation
  with Moot;
- register Moot validation, authorization, checkpoint authority, and folds
  with `murm-replication`;
- keep the landed Moot object lane on the shared muniment backend and processor;
- expose commands and snapshots for declare, join, leave, share, amend,
  withdraw, checkpoint, and inspect retention status;
- bind retention settings to a governed policy revision authorized by the
  constitution, rather than treating the settings as constitutional clauses.

Done when:

- **(met 2026-07-26, under ruling (b))** a consumer joins a Moot through one
  domain API and imports no p2panda crate. The constitution and delegation
  convergence proofs are the standing receipt. "One API" reads as one per
  layer: the gemot store plus `JoinedSpace::join`. A single `Moot::join` would
  put the ceremony in gemot, which the transport-ownership ruling declined and
  no consumer has asked for;
- the same Moot converges through live sync and native drop;
- unauthorized membership, checkpoint, retention, and prune operations fail
  before mutation;
- tessera and constitution remain separate policy rings with explicit inputs.

Implementation status, 2026-07-14: constitution and Moot retention are landed.
Signed genesis and amendment events feed a deterministic muniment-backed fold,
converge over LogSync, and project checkpoint authority from the accepted
revision. `MootGovernance` exposes founding, amendment, snapshots, durable
reopen, and checkpoint authorization without p2panda types. The Moot object
lane now shares the muniment processor and has checkpoint-plus-tail roster
replay, durable redb reopen, separate checkpoint logs, and authorized event
prefix pruning. Checkpoint v1 deliberately permits one active signer; key
rotation replaces that signer through a new constitution revision. The
aggregate `Moot` service now composes governance and roster commands, durable
snapshots, checkpoint authoring, pruning, and rotation-safe checkpoint ancestry
behind a p2panda-free command API. Aggregate drops now carry canonical critical
constitution evidence, admit it before object records, and can bootstrap a
fresh peer through a rotated checkpoint chain. Plaintext/local and injected
protected carriage share that dependency order and idempotent import receipt.
`Moot::record_tessera` returns a lane-tagged receipt whose recovered operation
is the explicit host publication seam; the retiring in-workspace host uses the
same sign-store-publish shape for its starter Tessera log. As of 2026-07-16,
the constitution also accepts a frozen-electorate member quorum, and carries
durable founder-governed grants that narrow the injected live capability
decision. `gemot::moot::group` now adapts verified p2panda-auth membership to
that seam and advances a host-owned p2panda-encryption group-secret epoch when
the resolved member set changes.

Independent delegation began 2026-07-17. Personae now owns the generic signed
certificate and revocation grammar, binds resource-scoped derived signing keys
to identity roots, and verifies path/action/time/depth attenuation. Gemot folds
those certificates beneath constitutional grant roots, cascades certificate or
root revocation, and intersects delegated authority with the existing live
membership/capability and admission checks. Graph statements remain inspection
projections, never authority. The signed p2panda lane now retains valid pending
statements, materializes chains independent of arrival order, and survives redb
reopen. Its live two-peer catch-up proof now runs over upstream p2panda 0.7 and
Iroh 1.0. Aggregate native drops carry the signed lane as critical capability
evidence and refresh delegated authority on import. Gemot exposes deterministic
read-only participant projections plus revocation-derived scope-key epochs; the
host binds those epochs to p2panda-encryption secrets and distributes them.

### Phase E: move the remaining consumers

Rebase mesh first, then other shared domains such as Isometry collaboration.
Each keeps its event grammar and materializer while deleting custom session and
store assembly.

Done when:

- **(met, verified 2026-07-26)** mesh depends on `murm-replication` rather than
  both `transport` and `mooting`. `mooting` is absent from mesh's manifest and
  `transport` is dev-only. The same check dropped mesh's now-unreferenced
  `p2panda-sync` and murm's `p2panda-net` + `p2panda-sync`, which the
  `JoinedSpace` extraction had made dead;
- one backend and processor implementation serves conversation, Moot, and mesh;
- each domain's two-peer tests cover online exchange, offline catch-up,
  withdrawal, checkpoint, and late-peer return.

### Phase F: keep host protocol assembly from being built

**Restated 2026-07-26. Both of this phase's original subjects are gone.**
It was written to replace `meerkat/src/sync.rs`, and meerkat was deleted
2026-07-18; merecat has one manifest naming no p2panda, murm, gemot, or
mooting dependency, so it has no peer lane to clean up. There is no host
protocol assembly left to remove.

So this is now a forward constraint rather than a removal task, which is the
cheaper direction anyway: when merecat grows a peer lane it consumes services
from the start and never assembles protocol. The app provides configuration
and receives typed status and events.

Done when:

- **(vacuously true today, and to be held)** Merecat has no direct
  `p2panda-net`, `p2panda-sync`, or store-trait dependencies. True now because
  the lane does not exist; the criterion earns its keep by still holding once
  it does. The reference for what the lane consumes instead is
  `murm-replication::JoinedSpace` plus the domain store;
- connect, disconnect, resync, import, export, and status are ordinary service
  commands;
- the app can run fully offline with peer services disabled;
- a headed receipt shows honest connected, catching-up, retained, body-erased,
  and prefix-pruned states.

### Phase G: promote the corrected families (WITHDRAWN 2026-07-23)

**This phase is void, and nothing replaces it.** The
[repo consolidation ruling](2026-07-23_repo_consolidation_plan.md) settled that
Mere is the platform and its extracted families stay its components, with the
bar for a separate repository being coherent identity apart from the six
primaries. It withdraws the murm/moot promotion by name. `repos/murm` and
`repos/moot` will not be founded, so the standalone-clone done-conditions
below describe repositories that will never exist.

Marked here rather than deleted because Phases A through F were sequenced
"promote only after", and a reader needs to know that gate was lifted rather
than left unmet. The parts worth keeping outlived the phase and are ordinary
requirements now: **Mere builds offline without peer features**, and
**committed manifests contain no local paths**.

The original text, for the record:

> Promote only after Phases A through F establish the public seams:
> `repos/murm` (peer transport, `murm-replication`, direct exchange);
> `repos/moot` (governed-space domain over the branch-tracked Murm
> dependency); Mere (optional projection adapters, peer dependencies absent
> from its default core); Merecat (direct dependencies on Mere, Murm, Moot,
> Personae, and Serval). Done when fresh standalone clones build and test,
> committed manifests contain no local paths, Mere builds offline without peer
> features, and Merecat performs the same two-peer scenario through the
> promoted libraries.

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

### 2026-07-26: the done-conditions were audited against the tree, and three had drifted

Prompted by asking what is actually left in this lane. Checking the phase
criteria against the code rather than against memory found three in a state no
reader could infer, so the phase text now carries current truth and this entry
carries why. Rule applied, per DOC_POLICY §3: correct a live plan in place,
never leave a criterion pointing at something that no longer exists.

- **Phase E #1 was already met** and had been for some time. `mooting` is
  absent from mesh's manifest; `transport` is dev-only. Nobody had said so.
- **Phase F's subjects are both gone.** It was written to replace
  `meerkat/src/sync.rs`, and meerkat was deleted 2026-07-18. merecat, its other
  subject, has one manifest naming no p2panda, murm, gemot, or mooting
  dependency, so there is no peer lane to clean up either. Restated as a
  forward constraint: when merecat grows that lane it consumes services from
  the start. Its first criterion is true vacuously today and earns its keep by
  still holding once the lane exists.
- **Phase G is void.** The 2026-07-23 consolidation ruling withdrew the
  murm/moot promotion by name, so its standalone-clone conditions describe
  repositories that will never be founded. Marked withdrawn rather than
  deleted, because Phases A-F were gated on "promote only after" and a reader
  needs to know that gate was lifted rather than unmet. Two requirements
  survive it as ordinary ones: Mere builds offline without peer features, and
  committed manifests carry no local paths.

**The pattern worth naming.** All three drifted the same way: the plan was
right when written, and the *world* moved (a crate deleted, a promotion
withdrawn, a dependency quietly satisfied). A criterion is a claim about the
tree, and it decays exactly like the manifest comment this lane caught
yesterday. Auditing them costs one pass and is worth doing whenever a phase is
declared complete.

**Two dead dependencies fell out of the same pass**, both made dead by
`JoinedSpace`: mesh's `p2panda-sync` and murm's `p2panda-net` + `p2panda-sync`
were declared but unreferenced. Removed with Mark's go-ahead; 86 tests green
after (mesh 29, murm 57). This is the done-condition recorded with the
transport ruling, now measured: the consumers' direct p2panda surface shrank
to what the drain cannot carry, and for murm it vanished.

### 2026-07-25: JoinedSpace landed; the seven ceremonies are one call

Executes the (b) ruling below, same day. `murm_replication::JoinedSpace<E>` is
generic over extensions only: the store and log-id types are erased at
construction (the session is kept alive as an opaque `Send + Sync` box, so the
erasure murm hand-rolled is now the crate's implementation detail), so a
holder names one type and the drop order (drain → stream handle → session
actor) is fixed in one place instead of by field position at every site. The
one cost of erasure: a store implementing `LogStore` for every log id cannot
pin `L` by inference, so each call site carries one turbofish
(`JoinedSpace::join::<_, u64, _, _>`).

All seven call sites migrated. mesh's `SyncedMesh` collapsed three
drop-order-sensitive fields into one and publishes through
`JoinedSpace::publish`; murm's `SyncedCabal` deleted its `Box<dyn Any>`
keepalive and holds `Option<JoinedSpace<CabalExt>>` (the no-transport mode
composes as ruled); gemot's four lane proofs and the `moot-peer` example each
collapsed to one call.

**The done-condition, measured rather than asserted.** The first write-up of
this entry claimed the gemot proofs imported no p2panda crate; a grep proved
that false: `join` took a `Topic`, so every site still imported
`p2panda_core`. Fixed at the source rather than in the prose: `join` now takes
`impl Into<Topic>`, callers pass their raw 32-byte space id, and the claim is
true by construction. Verified after the change:

- `LogSync`, `SyncHandle`, `TopicLogSyncEvent`, and `SyncSubscription` are
  imported in exactly two files workspace-wide, both inside
  `murm-replication`. No consumer names a session type.
- Three files are now fully p2panda-free, one of them **production**: murm's
  `gossip_sync.rs`, plus gemot's constitution and delegation proofs.
- What remains is honest rather than residual. `Operation` / `SigningKey` /
  `Hash` stay where a domain signs and verifies its own operations (mesh
  authoring, the tessera and records fixtures, the example); `Endpoint` +
  `Gossip` stay as the pass-through values those join signatures carry from
  the host's `sync_parts`.

`leave()` was added alongside, completing Phase B's named vocabulary (join,
leave, resync, report). It is exactly what dropping does, named so a
deliberate leave reads differently from a value going out of scope.

**The posture conflict that motivated the ruling did not exist.** The finding
below reads gemot's manifest comment ("gemot owns no p2panda-net ... p2panda-net
is dev-only") and mesh's opposite choice, and calls that a conflict needing a
boundary ruling. `cargo tree -p gemot -e normal -i p2panda-net` says otherwise:

```text
p2panda-net v0.7.0
└── murm-replication v0.1.0
    └── gemot v0.1.0
```

p2panda-net has been in gemot's **production** graph since murm-replication
became a normal dependency there. The manifest comment was describing an
intent the graph already contradicted, and reading the comment plus the
dev-dependency block, without the transitive check, is how it went
unnoticed. gemot's manifest now states the version that is true.

This does not change the ruling; it changes its reason. (b) is right because
murm-replication is where the mechanics belong, not because it keeps a
dependency out of gemot that was never out. And the posture worth holding is
about **source**: gemot's code names no session type, which is now enforced by
the two-file import result above, rather than about a build graph that stopped
being clean some time ago. The general lesson is the [cambium-winit
one](2026-07-23_repo_consolidation_plan.md) in a new place: a manifest comment
is a claim about the graph, and only `cargo tree` settles it.

**Receipts:** 228 tests green across the four crates, run before and after the
Topic change: murm-replication 40, gemot 102 (including the four two-peer
convergence proofs over live loopback), mesh 29 (both two-peer network
tests), murm 57. `cargo fmt` clean (the pass also normalized pre-existing
unformatted hunks in five gemot files from the concurrent commons-capability
work; verified format-only).

**Where this leaves Phase B done-when #3** ("the runtime can join, leave,
resync, and report one space without exposing a p2panda type to its
consumer"). The four verbs exist on one type. The session and topic types are
erased. Two things are still p2panda-shaped at the seam, and both should
stay:

- `Endpoint` + `Gossip` are pass-through values from the host's `sync_parts`.
  Erasing them means murm-replication depending on `transport`, inverting the
  layer for two opaque arguments a caller never constructs.
- `accept` is handed an `Operation<E>`. That IS the domain's operation. A
  lane that verifies and signs its own operations should see them, and
  wrapping it would be ceremony rather than a boundary.

So #3 is met for the space lifecycle and deliberately unmet for the
operation payload. **The criterion should read: *without naming a session,
subscription, or topic type*, the things that are p2panda's business. That
version is now enforced, since those types appear in exactly two files, both
inside this crate.**

**Phase D done-when #1** ("a consumer joins a Moot through one domain API and
imports no p2panda crate") holds under (b): the constitution and delegation
proofs join a Moot lane, import no p2panda crate, and are the standing
receipt. What is honestly still two calls rather than one is `gemot` store +
`JoinedSpace::join`; a single `Moot::join` would need gemot to own the
ceremony, which the ruling deliberately declined. Either the criterion means
"one API per layer" (met) or Phase D wants a gemot-side wrapper (not built,
and no consumer has asked for one).

### 2026-07-25: the reusable runtime service needs a ruling on who owns transport

> **RESOLVED the same day — see [the JoinedSpace
> entry](#2026-07-25-joinedspace-landed-the-seven-ceremonies-are-one-call)
> above.** Transport ownership was ruled **(b)** and `JoinedSpace` landed in
> `murm-replication`, collapsing what this survey counted as four ceremonies
> and the executing lane counted as seven. Read that entry, not this one, for
> the current state; this stays for the survey evidence.
>
> **And correct one premise below before reusing it.** Point 2 reports gemot's
> manifest as keeping p2panda-net "dev-only by design". The *direct* entry is
> indeed dev-only, but the comment it quoted was itself wrong about the build
> graph: `cargo tree -p gemot -e normal -i p2panda-net` shows p2panda-net in
> gemot's production graph transitively through `murm-replication`. So the
> "posture conflict" this survey framed as the blocker was softer than stated
> — option (b) was available all along. The posture that actually holds is
> about SOURCE (gemot's code names no session type), not the dependency graph.
> Fixed at the source in `617ff648`.

Surveyed toward Phase B's remaining "reusable multi-domain service API". Three
things checked; two are real, and the third is a non-finding worth recording
because I was one commit from "fixing" it.

**1. The join ceremony is hand-written four times.** Every consumer of
`SyncedSpace` repeats the same sequence — `LogSync::builder(store.sync_store(),
endpoint, gossip).spawn()` → `.stream(Topic::from(id), true)` → `.subscribe()`
→ `SyncedSpace::drive(sub, accept)` → hold `_log_sync` + `_handle` for
liveness, drop-order-sensitive. It appears in mesh's `SyncedMesh::join`
(production), murm's `SyncedCabal` (production), gemot's constitution
convergence test, and gemot's `moot-peer` example. The domain-specific part is
three values: the store handle, the topic, and the accept closure.

**2. The blocker is not code, it is a posture conflict.** gemot's manifest
states p2panda-net is **dev-only by design** — "It provides the stores + folds
+ `verify`; the host builds the ... p2panda-net / p2panda-sync / tokio-stream /
tokio are dev-only". mesh made the opposite choice and depends on p2panda-net
for its production `sync` module. So Phase D's done-when — "a consumer joins a
Moot through one domain API and imports no p2panda crate" — cannot be satisfied
as written without one of:

  - **(a) gemot takes the dependency**, gaining a `SyncedMoot` mirroring
    `SyncedMesh`, and its "host builds the transport" note is retired;
  - **(b) the shared service lives in `murm-replication`** as a host-facing
    `JoinedSpace { store_handle, topic, accept }`, so gemot stays
    transport-free and the *consumer* imports the replication crate rather
    than p2panda directly — which satisfies the criterion's letter and keeps
    gemot's posture;
  - **(c) the criterion is wrong for Moot** and joining is explicitly a host
    concern, in which case Phase D should say so.

**(b) is the shape the four call sites point at** — the ceremony is identical
and the variation is exactly the three values a `JoinedSpace::join` would take.
But this is a boundary ruling about who owns transport, not an implementation
detail, so it is recorded rather than guessed. Note that murm would hold
`Option<JoinedSpace>` (it supports a no-transport mode), which composes.

**RULED (b), Mark, 2026-07-25**: `JoinedSpace` lands in `murm-replication`,
subject to a deeper look at that crate's goals before building. Two facts
found while recording the ruling strengthen it. First, `murm-replication`
already production-depends on `p2panda-net` 0.7.0, so the crate already owns
the p2panda surface and (b) changes no dependency posture anywhere; it
completes the crate's own charter sentence ("owns the reusable p2panda
receive drain") by owning the drain's construction too. Second, the ceremony
count was undercounted: gemot carries it in FOUR `#[cfg(test)]` sync modules
(constitution, delegation, records, tessera) plus the `moot-peer` example,
alongside mesh's and murm's production copies, for seven hand-written sites.
knot K5 would be the eighth. A done-condition worth adding when this builds:
mesh's and murm's direct `p2panda-net` imports shrink to types the drain
cannot carry, or vanish.

**3. NON-finding: murm's `SyncStatus` / `SyncRound` are not stale duplicates.**
They look like copies of `murm_replication`'s and are not: murm's carry
`posts_received`, and its own doc states the gossip counters are "merged with
the shared LogSync drain's `murm_replication::SyncStatus`" because "the LogSync
side has no notion of gossip posts". They are documented domain supersets that
compose the shared type. Recorded because the resemblance is close enough to
invite exactly the well-meaning unification the
[intake-divergence test](#2026-07-25-verification-is-restored-and-phase-bs-first-criterion-is-false-as-written)
was just written to prevent one section above — and I nearly did it here.


### 2026-07-25: verification is restored, and Phase B's first criterion is false as written

**The 07-13/07-14 resolver stall is repaired.** Those entries record that
`cargo test -p murm-replication` "again stalled during workspace dependency
resolution before compilation and was stopped", leaving Murm, Moot, and mesh
package tests pending "a normal workspace rerun after that resolver state is
repaired". The [repo consolidation](2026-07-23_repo_consolidation_plan.md) did
repair it — every one of them now runs from a normal workspace command:

| package | result |
| --- | --- |
| `murm-replication` | 40 passed |
| `murm` | 57 passed |
| `mere-mesh` | 29 passed |
| `mooting` | 5 passed |
| `gemot` | 101 passed |

So the isolated-manifest workaround the plan leaned on is no longer needed,
and the pending consumer verification it names is done.

**Phase B done-when #1 does not hold, and should not.** It asks that "local
authoring, LogSync receipt, gossip receipt, and drop import return the same
decision for one operation corpus". `MootPolicy` carries
`allow_historical_checkpoint_authority` and `allow_historical_prune`, both
`false` for live processing and plain-drop import and both **`true`** for
`import_drop_records`, the aggregate carrier path. That divergence is
deliberate and this plan explains why elsewhere: an aggregate drop bootstraps
a fresh peer *through a rotated checkpoint chain*, so it must accept
checkpoints that were authoritative at their time and that live intake
correctly rejects as stale.

The criterion should therefore read: **the intake paths return the same
decision for one operation corpus, except that aggregate-drop import accepts
historical checkpoint authority and prune ancestry which live intake rejects
as stale — and that exception is pinned by a test rather than left as two
booleans someone might "fix".**

**Pinned 2026-07-25** (gemot 102 tests):
`aggregate_import_admits_the_historical_prune_that_live_intake_rejects` builds
a rotated checkpoint chain, constructs ONE prune operation naming the
superseded checkpoint (via `to_prune_operation_seed`, so it is built without
being accepted), then puts that same operation through both paths against the
same store: `accept` refuses it `checkpoint-stale` and leaves the operation
count unchanged, `import_drop_records` admits it (`accepted == 1`). The
divergence is now a property with a stated reason rather than a discrepancy
waiting to be tidied away.

### 2026-07-25: refinement — the read-time rule does not cover destructive operations

The [commons ruling](#2026-07-24-a-moots-commons-is-unauthorized-and-there-is-no-shared-graph-record)
above states that authority belongs at read because authority state converges
separately from the operations it authorizes. That is right for
**contributions**, and it is stated too absolutely. gemot already authorizes
checkpoints and prunes **at admission**, against convergent constitutional
state, and is correct to:

> A share that is filtered at read can become visible later when its
> certificate lands. **A prune cannot be un-pruned.** For destructive
> operations there is no later state in which to re-decide, so authority must
> be established before the mutation, accepting the risk that a legitimately
> early-arriving prune is rejected and must be re-sent.

The full rule, then: *admission validates what one operation can prove about
itself, plus authority for anything irreversible; the fold and its projections
decide what converged authority makes effective for everything else.* Phase D's
"unauthorized membership, checkpoint, retention, and prune operations fail
before mutation" is consistent with this for the destructive three.

Membership is the open edge: `MootEvent::Joined` is admitted by the same
catch-all arm as `Shared`, so a roster's `members` is who *claims* to be
present. The authority is `MootGroup` (verified p2panda-auth membership) at the
authorization seam, not the roster projection — worth stating explicitly
wherever the roster is consumed, since "members" reads like a decision and is
not one.


### 2026-07-24: a moot's commons is unauthorized, and there is no shared-graph record

Surveyed on entering this lane from the [capability model
round](2026-07-23_capability_model_plan.md), which had just made a moot peer's
petition authorizable (`gemot::MootAuthority`). Two structural facts, both
verified against the tree rather than inferred:

1. **`MootEvent::Shared` is admitted with no authorization.** `MootPolicy::admit`
   handles `RetentionCheckpoint` and `HistoryPruned` explicitly and falls
   through to `_ => Ok(Admission::keep(target))` for everything else, checking
   only the prune flag. `MootRoster`'s fold then pushes the entry into `fauna`
   unconditionally. So **any author whose operation reaches the store can place
   a reference in a moot's commons** — no membership check, no capability
   check. This plan already says as much for Tessera ("does not claim Moot
   membership or constitutional authorization"); it is equally true of the
   roster lane, and the delegation machinery to close it now exists.

2. **No moot record carries a graph edit.** The event vocabulary is
   `Declared` / `Joined` / `Shared` / `RetentionCheckpoint` / `HistoryPruned`:
   governance, plus *references* to content (`Shared` names a manifest id).
   A moot replicates who-may-decide and what-is-pointed-at; it does not
   replicate live graph petitions. So "a peer's petition arrives over the wire
   and applies through the gate" — the connective slice the capability round
   left dangling — **has no substrate today**. It would need a new event kind
   carrying an edit batch, which is a product decision about whether moots
   host live shared graphs at all, not an implementation detail to invent.

**RULED 2026-07-24 (Mark): filter at read.** And the reason is stronger than
consistency with `chain_is_live` — it is a correctness requirement of a
replicated log. Authority state (delegation certificates, constitution
amendments) converges *separately* from the operations it authorizes, so an
operation can legitimately arrive before the certificate that authorizes it:
out-of-order sync, a late-joining peer, a drop import. Refusing at admission
would permanently discard operations that become authorized moments later,
with no retry, because the operation is gone. Read-time evaluation cannot fail
that way: the entry becomes visible when its certificate lands and vanishes
when it is revoked.

The division of labour this settles, worth holding to elsewhere in the lane:

> **Admission validates what one operation can prove about itself** —
> signature, moot address, wire grammar, prune flag, all self-contained.
> **The fold and its projections decide what converged authority makes
> effective.**

**Landed** (gemot, 101 tests): `records::fauna_cap()` (the typed scope
`moot/fauna`), `MootRoster::authorized_fauna(&impl AuthorityProvider)` — the
commons as converged authority sees it — and `Moot::authorized_fauna(at_ms)`
composing rules + delegations + roster through `MootAuthority`. Both views stay
available on purpose: the unfiltered `roster.fauna` is still the convergent
record, so a surface can show an unauthorized contribution as *pending* rather
than making it vanish. Receipts: a delegated sharer's entry counts and an
ungranted identity's does not; revoking the sharer's certificate withdraws
their contribution **with no change to stored operations** — authority decided,
not the log.

Remaining in this thread: no surface reads the authorized view yet (the product
call of which view a UI shows), and admission still accepts any well-formed
`Shared`, which is now deliberate rather than an oversight.


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
  `murmuring`, `mooting`, `gemot`, mesh, and the old host sync lane.
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
  with compact results retained. Shared commitment/blob types and generic blob
  collection are now landed; complete domain reference tracing remains.
- Landed the first native drop slice in `murm-replication`: the plaintext/public
  cover, canonical manifest identity, operation/payload/blob/evidence records,
  bounded staged visitation, and golden/error vectors. This is carrier framing
  plus an injected protected-suite seam. The D4 operation selector/importer is
  also landed. Verified records now stage durably by `DropId`; operations,
  authorized prefix pruning and payload erasure, stage cleanup, and
  a cached receipt commit in one backend batch. Digest-checked blob/payload
  assembly is landed, including later matching of headers and payloads without
  reopening erased bodies. Callers can list, retry, or discard staged corpora
  according to their own storage settings.
- Added file carriage as a thin D4 edge: explicit plaintext/public and injected
  protected exports flush the same native bytes to disk, while file imports
  enter the existing staged processor path. Fresh-store transfer and repeated
  receipt behavior are covered. Archive/radio priority remains domain policy.
- Added the receipt-coordination seam below the peer runtime. Atomic local
  completion markers encode as bounded, digest-checked control frames; remote
  statements are retained only under an authenticated-peer scope with explicit
  cleanup. They remain advisory and cannot stand in for local import or domain
  admission. Peer command wiring still belongs to Phase C.
- Completed the generic D4 selector. Domains provide omit/header/full decisions
  and settings-derived priority; replication orders them deterministically,
  applies an optional canonical-record byte budget, and reports why records
  were omitted. Private file export requires this policy input. Concrete
  catch-up/archive/radio mappings remain with each domain.
- Added the first concrete mapping in mesh. Its catch-up selector binds the
  latest accepted checkpoint and per-log tail; archive and radio use explicit
  input/result privacy and settings-owned event priorities. The mesh selector
  drives the shared exporter directly.
- Added the direct-conversation mapping in `murm`. It uses the actual
  per-author sequence frontier for catch-up and settings-owned privacy and
  post-kind priorities for archive/radio. Message and topic headers may remain
  after their bodies are excluded.
- Added `ConversationStore`, the muniment-backed direct-conversation substrate.
  It admits through `OperationProcessor`, rejects cross-conversation replay
  before mutation, is idempotent, exposes the shared LogSync store, and drives
  the native-drop exporter.
- Added Murm-owned `ConversationEngine` and cut the live in-memory runtime over
  to that store. Local sends, gossip receipt, and LogSync now share the same
  processor and operation store; post history and broadcast subscriptions are
  derived views. The legacy `murmuring::CableEngine` is no longer Murm's live
  authority. The Moot mapping remains.
- Retired the legacy direct runtime completely: `CableEngine`,
  `PersistentCabalStore`, `InMemoryCabalStore`, the redb LogSync adapter, and
  the name-only `BilateralProtocol` trait are deleted. Folded the remaining
  signed-operation grammar and validation coverage into `murm`, then removed
  the `murmuring` package. All fifty-seven Murm tests pass, including two-peer
  gossip and LogSync.
- Added configurable memory/redb conversation storage. Redb reopen rebuilds
  retained history and restores the local author sequence before the handle is
  returned. Native-drop import now rebuilds the same derived view and emits
  newly visible posts exactly once.
- Added an epoch-aware native-drop keyring using p2panda-encryption's XChaCha20
  primitive. Rotation preserves the stable cabal identity and old epochs remain
  recoverable until the caller forgets them. Personae or a group-state adapter
  still owns authorization, key distribution, and persisted key history.
- Added the native Moot constitution producer: signed founder-rooted genesis and
  amendment events, canonical rule commitments, a muniment-backed store, a
  deterministic prior-rule fold, and checkpoint authority projected from the
  accepted revision. A real two-peer LogSync test proves late-peer constitution
  catch-up and identical authority on both peers. `MootGovernance` now provides
  the plain application boundary and durable reopen; aggregate Moot service and
  retention-event cutover remain.
- Closed the header-before-body gap in the shared processor. A later full
  operation hydrates an already-retained header and reports `HydratedPayload`;
  atomic imports expose their hydration count. Removed payload references still
  prevent post-retention resurrection.
- Added the constitution producer behind Moot checkpoint authority. Signed
  founder-rooted genesis and amendments fold through a muniment-backed store;
  the accepted revision and governed signer set construct the authority. It
  still intentionally cannot infer authority from roster membership.
