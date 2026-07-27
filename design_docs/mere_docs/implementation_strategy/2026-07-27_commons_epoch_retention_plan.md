# Safe Commons Epoch Retention

**Date:** 2026-07-27  
**Status:** active. E0-E1 are complete locally; E2 is next.

## 1. Problem

`commons.chat.v1` keeps p2panda Data Encryption history and names eight
retained epochs. Stickleback persists those epochs and can forget one exact
epoch after a domain has authorized it. The number eight is a retention floor,
not proof that the ninth-oldest secret is safe to erase.

The missing bridge is a reviewable proposal computed from:

- chronological epoch identity;
- a durable domain checkpoint and its authority revision;
- decryption reachability after that checkpoint;
- facts whose authority may still change the projection;
- author-continuation state for the retained tail;
- the Commons policy for members that remain offline through rotations.

The current Commons receipt has no checkpoint log. The current keyring exposes
its ids in lexical order, which is intentionally not rotation order. Either
absence must block a proposal rather than invite a guess.

## 2. Ownership

- **Stickleback** records neutral epoch chronology, calculates a proposal from
  caller-supplied facts, and erases only exact ids after a domain decision. It
  does not infer membership, authority, checkpoint validity, or offline policy.
- **Commons** owns the checkpoint grammar, current projection, pending and
  revoked facts, decryption-reachability inventory, and message-history policy.
- **Gemot/Personae** supplies the converged authority revision and the governed
  offline-member rule.
- **Knot** supplies its own checkpoint and reachability facts. It does not
  translate documents through chartulary or Commons chat events.
- **The host** persists the accepted checkpoint, keyring, and authorized
  forgetting receipt atomically.

## 3. Invariants

1. The current epoch is never proposed for forgetting.
2. The profile count is a minimum retained suffix, not the only safety gate.
3. Legacy keyrings without proven rotation order remain usable for decryption
   but cannot produce a pruning proposal.
4. Missing checkpoints, stale checkpoint authority, or an incomplete
   author-continuation frontier block every candidate.
5. An epoch covering a causally incomplete fact, a fact awaiting authority,
   or state promised to an offline member remains held.
6. A projection checkpoint may retire an old ciphertext only if replay from
   checkpoint plus retained tail preserves every future authority outcome the
   profile still permits.
7. A proposal is a dry-run artifact. Execution revalidates its basis under
   current authority and records the exact ids erased.
8. Epoch erasure cannot retract plaintext or keys already received elsewhere.

## 4. Sequence

### E0. Boundary audit — DONE 2026-07-27

Receipt:

- `DataKeyring::forget_authorized` erases an exact caller-approved id;
- `commons.chat.v1` records an eight-epoch floor;
- Commons graph/chat currently has one data log and no checkpoint lane;
- Knot has a projection checkpoint and retained-tail receipt, but its
  checkpoint is a pruning prerequisite rather than pruning authority;
- Mesh and Moot already prove domain-owned checkpoint authority and monotone
  frontiers.

### E1. Chronology and neutral proposal engine — DONE 2026-07-27

Files:

- `crates/stickleback/src/group_crypto.rs`
- `crates/stickleback/src/epoch_retention.rs`
- `crates/stickleback/src/lib.rs`

Done when:

- new keyrings persist exact installation order through reopen;
- legacy unordered keyrings decrypt normally but return an explicit
  incomplete-order blocker;
- the proposal retains the current epoch, the configured suffix, and every
  caller-held epoch;
- missing/stale checkpoint authority and missing continuation proof block the
  whole proposal;
- the result is serializable and names every hold reason and exact candidate.

Receipt: `DataKeyring` now persists validated installation order separately
from lexical secret ids. Existing version-1 keyrings without that added field
remain decryptable and produce `IncompleteEpochOrder`, with every present key
retained. `propose_epoch_pruning` emits no destructive candidate unless the
mode is Data Encryption, chronology is complete, checkpoint authority is
current, author continuation is ready, and every domain hold names a present
epoch. Stickleback's 55 unit tests, 5 boundary tests, and doctests pass.

### E2. Authorized Commons checkpoint

Files:

- `crates/probes/commons-spine/src/chat.rs`
- the promoted Commons domain selected by its first product consumer

Done when:

- a separately addressed signed checkpoint commits the chat projection,
  causal and author frontiers, current authority revision, epoch inventory,
  and a previous-checkpoint link;
- the checkpoint snapshot is protected under a retained current epoch;
- checkpoint-plus-tail reproduces full replay;
- pending/revoked facts either remain decryptably reachable or explicitly
  block the checkpoint from retiring their epoch;
- stale, foreign, forged, and rewinding checkpoints fail before mutation.

The probe may establish the grammar. Authorized pruning does not remain
probe-only: the consumer that promotes Commons must own the production
checkpoint store and policy.

### E3. Two consumer proposals

Done when:

- Commons chat produces a proposal from its authorized checkpoint, live
  operation inventory, and offline-member policy;
- communal Knot produces the same neutral proposal from its Knot checkpoint
  and tail without translating document events;
- both refuse to forget an epoch needed by a pending causal or authority fact.

### E4. Authorized execution and reopen

Done when:

- current domain authority revalidates the proposal basis;
- keyring persistence, proposal receipt, and exact epoch erasures commit as
  one host transaction;
- stale proposals and changed checkpoints fail without erasing anything;
- restart restores the reduced keyring and the same checkpoint-plus-tail
  projection;
- a member inside the offline recovery window can resume, while a member
  outside it receives the profile's explicit rejoin/bootstrap outcome.

## 5. Stop rules

- Never select an epoch by lexical secret id.
- Never infer checkpoint authority from session admission or transport peer
  identity.
- Never call a retained ciphertext deleted merely because the current
  projection hides it.
- Never prune a pending or revoked fact while the profile still permits a
  later authority change to make it effective.
- Never make destructive execution an automatic consequence of elapsed time
  in the first version.
