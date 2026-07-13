# Deletion, Retention, and Native Drop Plan

**Status**: Active plan.
**Date**: 2026-07-12.
**Scope**: Give Mere's p2panda-backed spaces one deletion and retention law,
then define a transport-independent native drop format for moving the same
accepted operations, checkpoints, proofs, and payloads over Iroh, Retinue,
files, or other store-and-forward paths.
**Related**:

- [`2026-07-12_murm_peer_runtime_and_moot_domain_plan.md`](2026-07-12_murm_peer_runtime_and_moot_domain_plan.md)
  owns the shared `murm-replication` foundation and the corrected Murm/Moot
  boundary this plan extends. It supersedes the earlier sibling posture.
- [`2026-06-30_commitment_proof_interface_plan.md`](2026-06-30_commitment_proof_interface_plan.md)
  owns typed digests, commitments, and proof domains.
- [`../technical_architecture/2026-06-19_statement_kernel_brief.md`](../technical_architecture/2026-06-19_statement_kernel_brief.md)
  owns per-predicate lifecycle and GC.
- [`2026-06-29_reticulum_transport_plan.md`](2026-06-29_reticulum_transport_plan.md)
  keeps Reticulum bilateral while sync and blobs remain Iroh-only.
- [`../../../../retinue/design_docs/2026-07-06_retinue_v0_plan.md`](../../../../retinue/design_docs/2026-07-06_retinue_v0_plan.md)
  owns Reticulum packet, link, and resource transfer.
- [p2panda `PruneFlag`](https://docs.rs/p2panda-core/0.6.1/p2panda_core/prune/struct.PruneFlag.html)
  and [p2panda-stream `LogPrune`](https://docs.rs/p2panda-stream/0.6.1/p2panda_stream/log_prune/struct.LogPrune.html)
  are the upstream log-pruning primitives.
- The [Willow data model](https://willowprotocol.org/specs/data-model/index.html)
  and [Willow Drop Format](https://willowprotocol.org/specs/drop-format/index.html)
  are donors for destructive current-state semantics and asynchronous carriage,
  not new Mere substrates.

---

## 1. Decision record

1. **Prunable current state is a retention checkpoint plus a tail.** An
   ordinary materialization checkpoint remains a replay optimization whose log
   stays authoritative. An authorized `RetentionCheckpoint` is a distinct
   trust-root transition after which an eligible prefix may disappear. This
   plan does not rewrite Codicil or local undo/history semantics.
2. **No generic permanent tombstone.** This plan adds no `Tombstone` record to
   the store or drop format. Immediate shared withdrawal still requires a
   signed negative event until a checkpoint records the resulting absence.
   Once that checkpoint is accepted, the withdrawal event can leave with the
   rest of the pruned prefix.
3. **Use p2panda's prune law.** Each domain extension gains p2panda-core's
   optional `PruneFlag`. An author sets it only on an operation that names an
   authorized checkpoint or a domain-approved history boundary. The surviving
   flagged operation is the compact anti-resurrection fact for that author's
   log. Do not invent a second cut certificate.
4. **Separate four acts that older prose called delete.** Withdrawing a fact
   from the live fold, erasing an operation body, collecting an unreferenced
   blob or encryption key, and pruning signed log headers are distinct. Product
   status must say which happened.
5. **Retention has a floor and a ceiling.** A hosting promise may require a
   minimum availability window. Privacy policy may impose a maximum retention
   window. If the floor exceeds the ceiling, the node stops claiming the
   hosting role; it does not retain private data to satisfy an incompatible
   promise.
6. **The drop is a carrier, not a second sync engine.** It packages the same
   p2panda operations and content-addressed payloads accepted by live sync. It
   introduces no alternate authorship, deletion, membership, or ordering
   semantics.
7. **One ingest path.** Gossip, LogSync, local authoring, and drop import all
   pass through the same verification, authorization, checkpoint, and pruning
   pipeline before they reach a domain fold.
8. **BLAKE3-native first.** Drop and record identities use the stack's current
   BLAKE3 vocabulary. Literal Willow Drop interoperability, including its
   Willow Entry and Bab/WILLIAM3 requirements, remains a separate adapter.

## 2. Current state and seams

| Concern | Live owner | Current fact | Gap this plan closes |
|---|---|---|---|
| Signed replication | p2panda-core operations in murmuring, moothold, mesh | Per-author `seq_num` and `backlink`; LogSync reconciles them | Domain extensions do not carry `PruneFlag` |
| Shared persistence | `mooting::MunimentStore`, moving to `murm-replication` | Implements `OperationStore`, `LogStore`, and `TopicStore`; can strip a payload or prune a log prefix | Pruning is callable but not governed by one application law |
| Live receive | `transport::SyncedSpace`, moving to `murm-replication`, plus domain `accept` closures | Signature, address, and insert checks differ by consumer | No common prune-aware ingest processor |
| Murmur history | `PersistentCabalStore` | Header and payload are bundled; `prune_entries` is a deliberate no-op | The Murm direct-exchange rebase must separate its domain view from the shared muniment sync store |
| Materialization | Domain folds; generic graph snapshots elsewhere | Several areas already use snapshot plus tail replay | No shared checkpoint contract for replicated spaces |
| Off-grid bytes | feature-gated `ReticulumTransport`; future Retinue resources | Bilateral streams work; sync and blobs stay Iroh-only | No delay-tolerant application bundle |
| Group security | constitution/capability plans; p2panda encryption is a candidate | Structural caps and live key state are not production-wired | Private drops must stay gated until a real protector is injected |

Ownership after this plan:

- **Domain crates** define withdrawal meaning, checkpoint authority, governed
  retention policy, and materialization.
- **`murm-replication`** owns generic muniment-backed ingest, retention and
  prune mechanics, checkpoint storage, and native drop machinery.
- **Murm and Moot services** compose their domain policy with the replication
  foundation and expose typed commands, snapshots, events, and status.
- **Merecat** supplies settings and resources. It does not assemble p2panda
  sessions or operation callbacks.
- **Murm peer transport** moves live bytes and exposes endpoints. It does not
  interpret retention or drop records.
- **Retinue** moves an opaque drop as a resource or a small framed payload. It
  does not learn p2panda or Mere policy.
- **Personae/wallet** owns key custody, enrollment, recovery, and capability
  slots. A drop carries only opaque envelopes produced by that layer.
- **Blob-domain owners** decide whether their content is live before generic
  storage collects it.

## 3. The deletion and retention law

### 3.1 The five retained things

Keep these separate in APIs and diagnostics:

1. **Live fact**: whether a statement, post, job, member, or resource appears in
   the current fold.
2. **Operation header**: signed authorship, address, sequence, backlink,
   timestamp, payload length, and payload hash.
3. **Operation body**: the application event bytes bound by that header.
4. **Referenced blob**: a larger content-addressed object named by an event or
   checkpoint.
5. **Decryption material**: the key epoch or object key needed to read encrypted
   bodies and blobs.

A live fact can disappear while its signed header remains. A body can be erased
while its hash remains. A blob can be collected only when no retained live state
or checkpoint references it. Erasing a group epoch is valid only when every
object under that epoch is eligible; sensitive objects therefore need their own
content key wrapped by the group epoch rather than sharing the epoch key as the
only erasure unit.

### 3.2 Retention policy

The shared vocabulary should be settings-friendly rather than a collection of
hardcoded durations:

```rust
pub struct RetentionPolicy {
    pub availability: AvailabilityPolicy,
    pub erasure: ErasurePolicy,
    pub checkpoint_rule: CheckpointRule,
    pub audit_rule: AuditRule,
}

pub struct AvailabilityPolicy {
    pub promised_floor: KeepBound,
}

pub struct ErasurePolicy {
    pub privacy_ceiling: KeepBound,
    pub payload_rule: PayloadRule,
}

pub enum KeepBound {
    Forever,
    ForAge(Duration),
    LastCount(u64),
    ThroughCheckpoint,
    None,
}
```

The exact type names may change during implementation. The required semantics
do not:

- personal `LocalOnly` data uses the owner's settings;
- a murmur uses its negotiated conversation policy, with each author retaining
  authority over their own log;
- a moot names a governed `RetentionPolicy` revision; its constitution
  separately proves who may adopt or amend that policy;
- a mesh may retain terminal job bodies for a configured audit window and then
  keep only compact results or receipts;
- a voluntary host must surface when local settings make it ineligible for the
  advertised retention promise;
- availability lapse does not itself authorize erasure. It ends a hosting or
  pinning promise, while privacy withdrawal or the erasure ceiling governs
  destructive action;
- local browsing history, Eidetic memory tiers, and Athanor image collection
  keep their existing owners. This policy applies when their data enters a
  replicated space or native drop.

### 3.3 Withdrawal, checkpoint, and prune

The normal lifecycle is:

1. An authorized `Withdraw` or domain-specific delete/retract event enters the
   log. The live fold immediately excludes its target.
2. Cooperative stores may erase the target body or unreferenced blob as soon as
   policy permits. They retain enough signed structure to reject malformed log
   continuations.
3. A domain checkpoint captures the resulting current state and a frontier over
   every included `(author, log_id)`.
4. The checkpoint is accepted under the policy revision it names. For a private
   space this can be one owner signature. For a moot it requires the
   constitution's authorization rule.
5. Each cooperating author may emit an operation referring to that checkpoint
   with `PruneFlag(true)`. p2panda's prune-aware backlink validation permits the
   flagged operation to survive without its predecessor header, and rejects a
   stale lower sequence after the log has advanced.
6. `p2panda-stream::LogPrune` calls the store's existing `prune_entries` for the
   prefix. The flagged operation and later tail remain.

This avoids one permanent tombstone per deleted object. It does not make the
distributed-systems constraint disappear: before an accepted checkpoint, a
specific shared absence needs a retained withdrawal event or an old peer can
reintroduce the positive fact.

### 3.4 Multi-author limit

Only an author can sign a prune point in their own log. A constitution can say
that compliant members should checkpoint and prune, and cooperative hosts can
erase bodies they are no longer allowed to retain, but the group cannot forge a
departed author's prune flag.

Therefore:

- an active author's prefix can disappear completely after their prune point;
- an inactive author's old bodies can be erased under accepted space policy;
- their small signed headers may remain until the protocol has a checkpoint-aware
  log-root transition accepted upstream;
- service and product status must report `body erased` separately from
  `history pruned`.

Arbitrary physical removal of one operation from the middle of a signed log is
not a v0 promise. Its live effect can be withdrawn and its body erased, but its
header leaves only when a later prefix prune covers it.

### 3.5 Checkpoint contract

Every replicated domain retention checkpoint must bind at least:

```rust
pub struct RetentionCheckpoint {
    pub version: u16,
    pub space_id: SpaceId,
    pub schema_id: String,
    pub policy_revision: Digest,
    pub authority_revision: Digest,
    pub frontier: Vec<LogFrontier>,
    pub snapshot: BlobRef,
    pub snapshot_commitment: Commitment,
    pub key_epoch: Option<Digest>,
}

pub struct LogFrontier {
    pub author: [u8; 32],
    pub log_id: Vec<u8>,
    pub seq_num: u64,
    pub operation: Digest,
}
```

`Digest` and `Commitment` are the shared typed proof vocabulary. The snapshot
uses the storage-checkpoint commitment domain. A p2panda operation hash remains
an operation identity and is not silently reused as an application commitment.
An ordinary materialization checkpoint may reuse the snapshot and frontier
shape, but it does not carry prune authority and never advances the retained
history floor by itself.

The checkpoint operation remains an ordinary signed p2panda operation. Its
backlink commits to the prior author chain even after those bytes are gone. The
snapshot is content-addressed in muniment. A consumer must prove:

- the checkpoint is authorized for the named space and policy revision;
- its snapshot digest matches its bytes;
- replaying the retained tail from that snapshot equals the live fold;
- no imported operation at or below the accepted frontier is applied again;
- a newer checkpoint advances, and never rewinds, each included frontier.

Historical verification becomes impossible once every copy of old bodies is
gone. Accepting a checkpoint is therefore accepting a new compact trust root,
not merely running a database optimization. The authorization UI and audit
receipts must say so.

## 4. Mere native drop format

### 4.1 Purpose and non-goals

A **drop** is an idempotent, bounded package of retained Mere data that can be
created without a live peer session and ingested through the normal application
pipeline later. It supports files, removable media, email-like attachment
paths, Iroh blobs, and Retinue resources.

The native sealed mail object remains the comms format. A drop may carry that
object or a reference to it, but does not define another message envelope.

It is not:

- a replacement for LogSync's efficient online reconciliation;
- a Reticulum routing or fragmentation protocol;
- a new operation type or authorization system;
- proof that every record in the sender's store was included;
- permission to bypass local interest, capability, privacy, or retention rules.

### 4.2 Logical records

The first format version needs only four record kinds:

```rust
pub enum DropRecord {
    Operation {
        header: Vec<u8>,
        inline_body: Option<Vec<u8>>,
    },
    PayloadChunk {
        payload_hash: [u8; 32],
        offset: u64,
        bytes: Vec<u8>,
    },
    BlobChunk {
        blob_hash: [u8; 32],
        offset: u64,
        bytes: Vec<u8>,
    },
    Evidence {
        kind: EvidenceKind,
        subject: Digest,
        bytes: Vec<u8>,
        critical: bool,
    },
}
```

`EvidenceKind` is a registered numeric discriminant, not an arbitrary string.
It distinguishes typed commitment proofs, capability chains, checkpoint
authorization, and opaque wallet-produced key envelopes while allowing unknown
optional kinds to be skipped. A checkpoint is an ordinary operation plus its
referenced blob. A withdrawal and a prune point are ordinary operations. There
is deliberately no drop-only deletion record.

Headers and bodies stay separable, matching `MunimentStore` and allowing a
compact drop to carry signed history without large or privacy-expired content.
Chunks are content-addressed and independently deduplicated.

### 4.3 Manifest, protection, and physical stream

The semantic body is deterministic CBOR, matching the current operation stack.
A canonical manifest lists each record's kind, BLAKE3 digest, byte length, and
whether it is critical. `DropId` is BLAKE3 over the canonical manifest and lives
inside the protected body. It identifies semantic content independently of
compression, encryption, transfer segmentation, or carrier.

The physical encoding has two layers:

1. A small clear **cover**: magic, format version, protection-suite id,
   protected-body length, and protected-body digest.
2. A protected **body**: manifest plus records, optionally compressed before it
   is encrypted.

The body is a self-delimiting record stream, so a decoder can enforce limits and
stage records without loading the entire plaintext into memory. The drop format
does not define indexed transfer fragments or resume state. Iroh and Retinue
segment, retry, and resume the opaque protected stream. File and removable-media
carriers write the stream directly. Exact field widths and magic bytes are
frozen only after golden-vector tests.

Private drops require an injected protector. The intended production protector
wraps a fresh drop content key to a p2panda-encryption epoch or explicit
recipients. Until that layer is live, plaintext drops are limited to public data
and explicit local test/export flows. The UI must refuse a private export rather
than imply encryption. Personae/wallet supplies the key and capability
envelopes; the drop codec does not define device enrollment, persona recovery,
or wallet backup semantics.

An optional exporter signature authenticates the completeness claim of this
particular package. It never grants authority to the contained operations, which
must verify independently.

### 4.4 Configurable export profiles

Profiles are named settings bundles, not protocol constants:

- **Catch-up**: latest accepted checkpoint, retained tail, required proofs, and
  selected small payloads.
- **Archive**: all locally retainable operations and referenced blobs permitted
  by the space's privacy ceiling.
- **Radio**: an explicit byte budget and priority classes, normally key updates,
  invitations, withdrawals, votes, short murmurs, checkpoints, and compact
  receipts before bulk content.

Each profile exposes byte budget, record chunk size, compression, expiry, inline-body
threshold, blob policy, and priority ordering as settings. Retinue's resource
layer owns reliable segmentation and resume. Before Retinue R4, only a drop that
fits the available bilateral stream or packet budget may use that lane.

### 4.5 Import law

Import is staged and transactional:

1. Parse the cover and enforce configured byte, record, nesting, and
   decompression limits before allocating the claimed sizes.
2. Verify, decrypt, and decompress the protected body.
3. Verify the manifest, record digests, and optional exporter signature.
4. Group operations by `(space, author, log_id)`, order them by sequence, and
   select the highest admissible prune point before considering an older prefix.
5. Feed every operation through the same prune-aware ingest pipeline used by
   gossip and LogSync: header signature, space address, strictly advancing or
   prunable backlink, capability/policy authorization, checkpoint validity, and
   idempotent store insertion.
6. Stage payload and blob chunks only when an accepted operation or checkpoint
   references them and local policy permits retention.
7. Commit the accepted operation, topic, payload, blob, checkpoint, and prune
   changes through one muniment batch where the backend supports it.
8. Return a structured report: accepted, duplicate, unauthorized, stale below
   frontier, body omitted, body erased, prefix pruned, missing key, corrupt, and
   over-budget.

Malformed optional records are skipped with a report. An unknown critical
record or protection suite rejects the drop. Re-importing the same `DropId` is
safe and should avoid repeating record verification when the receipt cache
still retains its manifest digest.

## 5. Implementation phases and done-conditions

### Phase D0: upstream prune proof

Build a focused `murm-replication` test extension containing a space id and
optional `PruneFlag`. Compose `MunimentStore`, p2panda-core's prune-aware
backlink validation, and p2panda-stream's `LogPrune` without a network session.

Implementation status, 2026-07-13: complete. The focused proof preserves
pre-field encoding and identity, removes operations 0 through 2 beneath a
flagged operation 3, validates that survivor without its predecessor, rejects
replay below the retained frontier, and rejects an unapproved flag before
mutation.

`LogPrune` delegates to `LogStore::prune_entries` after ingestion. It proves the
upstream deletion semantics, but does not make operation insertion and prefix
removal one transaction. D1 must compose those writes inside the muniment
backend batch or introduce an equivalent backend transaction boundary.

Done when:

- adding a defaulted, skipped `PruneFlag` preserves the bytes, hashes, and
  signatures of pre-field fixtures;
- operations 0 through 2 disappear after operation 3 carries a valid prune
  flag;
- operation 3 validates with its predecessor absent;
- replaying operation 0 after the store has advanced is rejected;
- an unapproved prune flag is rejected before store mutation.

### Phase D1: one ingest pipeline

Add the generic prune-aware processor to `murm-replication`, beside
`MunimentStore`, with injected domain verification and authorization. Route the
mesh vertical slice through it first. Then route LogSync, gossip, local
authoring, and drop import through the same API. Murm direct exchange migrates
after its coupled redb sync index is replaced by the shared store.

Implementation status, 2026-07-13: the processor and atomic retention-effect
path are landed. Domain admission returns `Keep`, an authorized
`PruneBeforeCurrent`, and any authorized payload erasures. The processor applies p2panda's
prune-aware backlink law, separately rejects any operation at or below the
retained frontier, and commits prefix deletion with the surviving operation,
topic, log entry, operation pointer, and body erasure in one muniment backend
batch. Mesh local authoring, LogSync receipt, checkpoint acceptance, and
authorized pruning share this path. The D0 proof is complete. Gossip receipt
and drop import remain.

Done when:

- local, LogSync, and drop-shaped byte input produce the same accept/reject
  result for one operation corpus;
- the processor, not each `SyncedSpace` closure, owns backlink/frontier/prune
  validation;
- store mutation is atomic across operation pointer, log index, topic,
  checkpoint, and prune effects;
- mesh two-peer convergence remains green.

### Phase D2: checkpoint and policy contract

Implement `RetentionCheckpoint`, `LogFrontier`, separate availability and
erasure settings, and the replication domain adapter. Use mesh first because
`JobBoard` is deterministic and its terminal jobs give an honest retention
case. Add a checkpoint event and fold support without teaching
`murm-replication` mesh semantics.

Implementation status, 2026-07-13: the mesh vertical slice is landed. Mesh has
separate event and checkpoint logs, governed policy revisions, monotone
per-author event frontiers, canonical snapshot digests, checkpoint-plus-tail
folding, flagged prune authoring, and atomic terminal-input erasure. Keeping the
checkpoint in its own log lets the event prefix disappear without deleting the
trust root that authorizes the cut. The policy vocabulary separates an
availability floor from an erasure ceiling, and process outcomes distinguish
body erasure from prefix pruning.

The general D2 contract is still partial. The mesh snapshot is inline and uses
a mesh-local `CheckpointDigest`; it must move to the shared typed
`Commitment`/`BlobRef` vocabulary when that proof interface lands. Blob
reference tracing and collection are also absent. The current configured
authority is one key, suitable for the personal mesh, while Moot still needs
constitution-authorized checkpoint adoption.

Done when:

- full replay equals checkpoint plus tail replay;
- the accepted frontier never rewinds;
- a terminal job body is erased after its configured ceiling while its compact
  result remains;
- a false snapshot digest, foreign space, stale policy revision, or unauthorized
  checkpoint is rejected;
- diagnostics distinguish live withdrawal, body erasure, blob GC, and full
  prefix prune.

### Phase D3: native drop codec

Implement the cover, canonical manifest, self-delimiting record stream, and
configured resource limits in `murm_replication::drop`. Start with plaintext
public/test drops and an injected protector trait.

Implementation status, 2026-07-13: the plaintext/public framing is landed. A
fixed `MEREDRP\0` cover binds version, protection-suite id, protected length,
and body digest. The protected body contains a canonical manifest followed by
self-delimiting record frames. `DropId` is BLAKE3 over the canonical manifest,
and each manifest entry binds kind, criticality, length, and record digest. The
writer makes bounded per-record passes rather than buffering the full body; the
reader visits one verified record at a time through an explicitly staging-only
callback. Configurable limits are checked before manifest or record allocation.

This is a partial D3 landing. Suite `0` is deliberately plaintext and suitable
only for public data and local tests. The injected protector and compression
layers are not yet implemented, so private export remains unavailable and
decompression-bomb handling remains a future protected-suite concern.

Done when:

- golden vectors freeze canonical encoding, semantic `DropId` derivation, and
  protected-body digest derivation;
- a drop streams and round-trips without loading its full plaintext body;
- bit corruption, truncation, wrong lengths, decompression bombs, unknown
  critical records, and unsupported protection suites fail closed;
- unknown optional records are reported and skipped;
- headers can arrive without bodies and later accept matching chunks.

### Phase D4: staged import and export

Add export selection over `MunimentStore` and a temporary muniment staging
namespace for import. Apply accepted records through D1, never directly to
domain tables.

Done when:

- export never includes data beyond the privacy ceiling or below the retained
  frontier;
- import is idempotent by record hash and the post-decryption `DropId`;
- a corrupt or unauthorized record leaves the live store unchanged;
- importing checkpoint plus tail produces the same fold as live LogSync;
- an old drop cannot resurrect a withdrawn and checkpointed fact;
- file export/import works between two fresh profiles.

### Phase D5: carriers, security, and domain rollout

Carry the same bytes through Iroh blob transfer and Retinue R4 resources. Wire a
real protector only when p2panda group encryption or an equivalent Mere trait is
production-ready. Then migrate murm, moot/tessera, and remaining mesh records,
with each domain supplying its own authority and checkpoint fold.

Done when:

- file, Iroh, and Retinue resource carriage preserve the same semantic `DropId`
  and produce identical import reports even when carrier segmentation or
  protection changes;
- transport loss/resume changes delivery only, never application semantics;
- private export is impossible without a configured protector;
- key rotation does not change `SpaceId` or checkpoint identity;
- each domain has a two-peer test for withdrawal, checkpoint, late-peer return,
  payload erasure, and eventual prefix prune;
- stale docs that prescribe permanent federation tombstones are corrected.

## 6. Stop rules and explicit non-promises

- Do not custom-build prefix pruning while p2panda's `PruneFlag`,
  `validate_prunable_backlink`, and `LogPrune` fit.
- Do not put drop semantics in `transport` or Retinue.
- Do not allow a checkpoint merely because its snapshot decodes; authorization
  is the trust-root transition.
- Do not describe body erasure as full history deletion while signed headers
  remain.
- Do not promise deletion from hostile peers, backups, screenshots, or devices
  that previously held plaintext. Report cooperative-replica scope plainly.
- Do not use a group encryption epoch as the stable namespace id.
- Do not ship private plaintext drops as a temporary product fallback.
- Do not optimize Retinue radio segmentation until the catch-up drop works
  through the file carrier and the importer proves anti-resurrection.

---

## Findings

### 2026-07-12: p2panda already supplies the missing anti-resurrection primitive

The first instinct was to invent a signed multi-log cut. Source and API review
showed that p2panda-core 0.6.1 already carries a compact, default-feature
`PruneFlag`, its alternative backlink validator explicitly accepts a surviving
flagged operation whose predecessor is gone, and p2panda-stream coordinates the
store prefix deletion. Mere's `MunimentStore::prune_entries` already satisfies
the physical store half. The missing work is application checkpoint authority,
extension wiring, and a common ingest path.

### 2026-07-12: tombstone-free means compacted absence, not absent evidence

A permanent negative record per object is unnecessary. An immediate shared
withdrawal must nevertheless remain visible until an accepted checkpoint makes
the absence part of the new current-state root. The prune point then prevents a
late peer's old prefix from becoming current again. This is the useful Willow
lesson expressed in the p2panda-native log model.

### 2026-07-12: the drop belongs above transport and below domains

Willow's useful donor idea is one idempotent package that can travel by any
means. Mere should package its own operations and BLAKE3 content rather than
translate them into Willow Entries. The shared muniment/p2panda adapter is
moving from `mooting` into `murm-replication`, the corrected neutral home. Iroh
and Retinue remain opaque carriers; domain folds remain the semantic owners.

## Progress

### 2026-07-12

- Audited the live p2panda operation extensions, `SyncedSpace` accept seam,
  `MunimentStore` payload deletion and prefix pruning, murm's no-op prune,
  snapshot-plus-tail plans, Reticulum limitation, and Retinue resource roadmap.
- Verified upstream `PruneFlag`, `validate_prunable_backlink`, and `LogPrune`
  against p2panda 0.6.1, the version pinned in Mere.
- Chose the checkpoint plus tail law, transient withdrawals, separate payload
  erasure, upstream prefix pruning, and one native drop carrier.
- Reconciled the plan with the Murm peer-runtime reframe: generic machinery now
  targets `murm-replication`; retention checkpoints are distinct from ordinary
  materialization snapshots; typed commitments replace raw proof identifiers;
  availability and erasure policies are separate; wallet and blob-GC ownership
  stay outside replication; Retinue retains transfer segmentation and resume.
- Planning only. No runtime code or tests changed in this pass.

### 2026-07-13

- Landed the shared non-prune `OperationProcessor` in `murm-replication` with
  injected mesh admission, signature and operation-id checks, idempotence, and
  ordinary backlink continuity.
- Routed mesh local authoring and LogSync receipt through that single path.
  Rejection-before-mutation is covered directly, and the then-eighteen-test
  mesh suite passed against the real source through an isolated manifest.
- Batched topic association, log indexing, and the operation pointer into one
  backend apply. Checkpoint and prune effects remain absent from the production
  processor.
- Completed D0 against p2panda 0.6.1. Eleven `murm-replication` tests now prove
  default-field wire compatibility, authorized prefix removal, missing-
  predecessor validation at the prune point, stale-prefix rejection, and
  rejection-before-mutation for an unauthorized flag.
- Confirmed that upstream `LogPrune` performs prefix removal through a separate
  `LogStore::prune_entries` call. D1 therefore owns the stronger atomic
  insertion-plus-prune boundary.
- Added `Admission` and `HistoryAction` to the production processor. An
  authorized `PruneBeforeCurrent` now permits a surviving operation whose
  predecessor is absent while a separate retained-frontier check rejects stale
  ordinary and flagged operations.
- Added the muniment atomic prune path: old log entries and operation pointers
  are deleted in the same `Backend::apply` batch that writes the surviving
  operation, topic association, and new pointer. `ProcessOutcome` reports the
  removed-entry count.
- Focused workspace verification passes thirteen `murm-replication` tests and all
  twenty-seven mesh tests, including two-peer convergence, governed checkpoint
  rejection, checkpoint-plus-tail equivalence, atomic terminal-input erasure,
  checkpoint survival across event-prefix pruning, and stale-prefix rejection.
- Landed the mesh D2 vertical slice: `MeshRetentionPolicy`,
  `RetentionCheckpoint`, monotone `LogFrontier`, canonical snapshot digest,
  separate `Events` and `Checkpoints` logs, snapshot-plus-tail `JobBoard`
  replay, checkpoint and prune authoring helpers, and structured retention
  effects.
- Extended shared admission with authorized payload erasures. Muniment now
  strips those bodies in the same backend batch that inserts the checkpoint;
  the retained header still binds the erased body hash, while the checkpoint
  keeps the terminal job result.
- The first workspace mesh run finished compiling just as the command timeout
  closed its test pipe. An immediate rerun passed. The only migration residue
  in these package graphs is unused-patch warnings for Genet's `graft-engine`
  and `weld-engine`.
- D2 remains partial at the cross-domain boundary: mesh still uses an inline
  snapshot and mesh-local digest pending the shared commitment interface;
  referenced-blob tracing/GC and Moot constitution authorization remain.
- Landed the D3 plaintext/public native drop framing in `murm-replication`:
  fixed cover, canonical semantic manifest, BLAKE3 `DropId`, four record kinds,
  self-delimiting frames, bounded streaming visit, and structured errors.
- Golden cover/identity vectors and failure tests cover bit corruption,
  truncation, false lengths, allocation limits, unsupported protection suites,
  unknown optional skipping, unknown critical rejection, and header-first body
  carriage. The shared replication suite now passes twenty tests.
- D3 remains partial until a real injected protector and compression suite land.
  The D4 staged importer/export selector remains unimplemented.
