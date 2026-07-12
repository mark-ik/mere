# Deletion, Retention, and Native Drop Plan

**Status**: Active plan.
**Date**: 2026-07-12.
**Scope**: Give Mere's p2panda-backed spaces one deletion and retention law,
then define a transport-independent native drop format for moving the same
accepted operations, checkpoints, proofs, and payloads over Iroh, Retinue,
files, or other store-and-forward paths.
**Related**:

- [`2026-07-08_murm_moot_sibling_posture_plan.md`](2026-07-08_murm_moot_sibling_posture_plan.md)
  owns the shared `MunimentStore`, `SyncedSpace`, and murm/moot boundary this
  plan extends.
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

1. **Current state is a checkpoint plus a tail.** A space's accepted current
   state is the latest authorized checkpoint and the operations after its
   frontier. Older history is useful evidence, not permanent application truth.
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
| Shared persistence | `mooting::MunimentStore` | Implements `OperationStore`, `LogStore`, and `TopicStore`; can strip a payload or prune a log prefix | Pruning is callable but not governed by one application law |
| Live receive | `transport::SyncedSpace` plus domain `accept` closures | Signature, address, and insert checks differ by consumer | No common prune-aware ingest processor |
| Murmur history | `PersistentCabalStore` | Header and payload are bundled; `prune_entries` is a deliberate no-op | Sibling-plan S2b must separate its domain view from the muniment sync store |
| Materialization | Domain folds; generic graph snapshots elsewhere | Several areas already use snapshot plus tail replay | No shared checkpoint contract for replicated spaces |
| Off-grid bytes | feature-gated `ReticulumTransport`; future Retinue resources | Bilateral streams work; sync and blobs stay Iroh-only | No delay-tolerant application bundle |
| Group security | constitution/capability plans; p2panda encryption is a candidate | Structural caps and live key state are not production-wired | Private drops must stay gated until a real protector is injected |

Ownership after this plan:

- **Domain crates** define what a checkpoint means, what may be withdrawn, and
  who may authorize it.
- **`mooting`** owns the generic muniment-backed ingest, retention, checkpoint,
  and drop machinery. Its store is already intentionally useful outside Moot.
- **The host** composes domain policy with the generic processor and store.
- **`transport`** moves live bytes and exposes endpoints. It does not interpret
  retention or drop records.
- **Retinue** moves an opaque drop as a resource or a small framed payload. It
  does not learn p2panda or Mere policy.

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
    pub availability_floor: KeepBound,
    pub privacy_ceiling: KeepBound,
    pub checkpoint_rule: CheckpointRule,
    pub payload_rule: PayloadRule,
    pub audit_rule: AuditRule,
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
- a moot uses the constitution revision named by the checkpoint;
- a mesh may retain terminal job bodies for a configured audit window and then
  keep only compact results or receipts;
- a voluntary host must surface when local settings make it ineligible for the
  advertised retention promise.

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
- Mere must report `body erased` separately from `history pruned`.

Arbitrary physical removal of one operation from the middle of a signed log is
not a v0 promise. Its live effect can be withdrawn and its body erased, but its
header leaves only when a later prefix prune covers it.

### 3.5 Checkpoint contract

Every replicated domain checkpoint must bind at least:

```rust
pub struct SpaceCheckpoint {
    pub version: u16,
    pub space_id: [u8; 32],
    pub schema_id: String,
    pub policy_revision: [u8; 32],
    pub frontier: Vec<LogFrontier>,
    pub snapshot: BlobRef,
    pub snapshot_digest: [u8; 32],
    pub key_epoch: Option<[u8; 32]>,
}

pub struct LogFrontier {
    pub author: [u8; 32],
    pub log_id: Vec<u8>,
    pub seq_num: u64,
    pub operation: [u8; 32],
}
```

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
    Proof {
        proof_type: String,
        bytes: Vec<u8>,
        critical: bool,
    },
}
```

`Proof` carries capability chains, checkpoint authorization, or key envelopes
without teaching the container their semantics. A checkpoint is an ordinary
operation plus its referenced blob. A withdrawal and a prune point are ordinary
operations. There is deliberately no drop-only deletion record.

Headers and bodies stay separable, matching `MunimentStore` and allowing a
compact drop to carry signed history without large or privacy-expired content.
Chunks are content-addressed and independently deduplicated.

### 4.3 Manifest, protection, and physical frames

The semantic body is deterministic CBOR, matching the current operation stack.
A canonical manifest lists each record's kind, BLAKE3 digest, byte length, and
whether it is critical. `DropId` is BLAKE3 over the canonical manifest and lives
inside the protected body. It identifies semantic content independently of
compression, encryption, frame sizing, or carrier.

The physical encoding has two layers:

1. A small clear **cover**: magic, format version, protection-suite id,
   protected-body length, frame count, a transfer id, and protected-body digest.
2. A protected **body**: manifest plus records, optionally compressed before it
   is encrypted.

The protected body is split into self-delimiting frames for streaming and
resume. Each frame carries the format version, transfer id, frame index, total
frames, payload length, payload bytes, and a BLAKE3 checksum. The transfer id is
derived from the protected bytes, so frames can be grouped before decryption
without exposing the semantic `DropId` of a private drop. File concatenation and
out-of-order store-and-forward delivery use the same codec. Exact field widths
and magic bytes are frozen only after golden-vector tests.

Private drops require an injected protector. The intended production protector
wraps a fresh drop content key to a p2panda-encryption epoch or explicit
recipients. Until that layer is live, plaintext drops are limited to public data
and explicit local test/export flows. The UI must refuse a private export rather
than imply encryption.

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

Each profile exposes byte budget, chunk size, compression, expiry, inline-body
threshold, blob policy, and priority ordering as settings. Retinue's resource
layer owns reliable segmentation. Before Retinue R4, only a drop that fits the
available bilateral stream or packet budget may use that lane.

### 4.5 Import law

Import is staged and transactional:

1. Parse the cover and enforce configured byte, frame, record, nesting, and
   decompression limits before allocating the claimed sizes.
2. Reassemble, verify, decrypt, and decompress the protected body.
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

Build a focused `mooting` test extension containing a space id and optional
`PruneFlag`. Compose `MunimentStore`, p2panda-core's prune-aware backlink
validation, and p2panda-stream's `LogPrune` without any network dependency.

Done when:

- adding a defaulted, skipped `PruneFlag` preserves the bytes, hashes, and
  signatures of pre-field fixtures;
- operations 0 through 2 disappear after operation 3 carries a valid prune
  flag;
- operation 3 validates with its predecessor absent;
- replaying operation 0 after the store has advanced is rejected;
- an unapproved prune flag is rejected before store mutation.

### Phase D1: one ingest pipeline

Add a generic prune-aware processor beside `MunimentStore`, with injected
domain verification and authorization callbacks. Route the mesh vertical slice
through it first. Then route LogSync and local authoring through the same API.
Murm waits for sibling-plan S2b, which removes its coupled redb sync index.

Done when:

- local, LogSync, and drop-shaped byte input produce the same accept/reject
  result for one operation corpus;
- the processor, not each `SyncedSpace` closure, owns backlink/frontier/prune
  validation;
- store mutation is atomic across operation pointer, log index, topic,
  checkpoint, and prune effects;
- mesh two-peer convergence remains green.

### Phase D2: checkpoint and policy contract

Implement `SpaceCheckpoint`, `LogFrontier`, retention settings, and a small
domain adapter trait. Use mesh first because `JobBoard` is deterministic and its
terminal jobs give an honest retention case. Add a checkpoint event and fold
support without teaching `mooting` mesh semantics.

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

Implement the cover, canonical manifest, record vocabulary, frame codec, and
configured resource limits in `mooting::drop`. Start with plaintext public/test
drops and an injected protector trait.

Done when:

- golden vectors freeze canonical encoding, semantic `DropId` derivation, and
  protected transfer-id derivation;
- a multi-frame drop round-trips when frames arrive in order, out of order, and
  with duplicates;
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
  and produce identical import reports even when framing or protection changes;
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
- Do not optimize radio framing until the catch-up drop works through the file
  carrier and the importer proves anti-resurrection.

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
translate them into Willow Entries. The shared muniment/p2panda adapter in
`mooting` is the existing neutral home. Iroh and Retinue remain opaque carriers;
domain folds remain the semantic owners.

## Progress

### 2026-07-12

- Audited the live p2panda operation extensions, `SyncedSpace` accept seam,
  `MunimentStore` payload deletion and prefix pruning, murm's no-op prune,
  snapshot-plus-tail plans, Reticulum limitation, and Retinue resource roadmap.
- Verified upstream `PruneFlag`, `validate_prunable_backlink`, and `LogPrune`
  against p2panda 0.6.1, the version pinned in Mere.
- Chose the checkpoint plus tail law, transient withdrawals, separate payload
  erasure, upstream prefix pruning, and one native drop carrier.
- Planning only. No runtime code or tests changed in this pass.
