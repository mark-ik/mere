# Commons Authority, Keys, and Consumers Plan

**Date:** 2026-07-27
**Status:** implemented 2026-07-27. C1 through C7 have executable software
receipts. The direct-PHY RF carriage named by C6 remains a hardware receipt.

**Companions:** the
[shared-engram commons brief](../research/2026-07-24_shared_engram_commons_brief.md),
the
[multi-writer convergence plan](2026-07-26_commons_multi_writer_convergence_plan.md),
the [Knot port plan](2026-07-25_knot_port_plan.md), and the
[deletion, retention, and native-drop plan](2026-07-12_deletion_retention_and_native_drop_plan.md).

## 1. Rulings

### 1.1 Knot owns document sync semantics, not a replication engine

Knot remains the owner of document events, format writes, document conflict
policy, and the conflict UI. It does not grow another p2panda reconciliation
runtime. Its existing `KnotSyncStore` already reuses Stickleback's operation
store, processor, and joined-space ceremony; the duplicated part is causal
projection bookkeeping.

Knot therefore pulls the neutral causal machinery into Stickleback:

- exact observed-frontier parents;
- deterministic topological ordering;
- author head recovery;
- bounded parent and payload admission;
- pending-parent diagnostics that do not hide unrelated state.

Knot keeps its sealed `Put` and `Delete` document grammar and folds those
ordered facts into a document projection. A document touched by multiple
writers remains an explicit conflict until Knot has a document-grained merge
policy. The conflict does not make unrelated documents unavailable. No
chartulary batch is manufactured merely to move a document.

This is both the architectural and efficiency boundary: one signed operation,
one ciphertext body, one retained log. Knot materializes document state from
that log without duplicating transport storage or translating through a graph
format it does not use.

### 1.2 Authority is evaluated over retained facts

Structural admission verifies only self-contained claims: operation integrity,
space address, bounded grammar, signer binding, and per-author continuity. It
retains a structurally valid operation even if the authority facts that make it
effective have not arrived.

Each communal operation binds its signing key to a stable Personae root:

- a root key may bind directly to itself;
- a derived per-space key carries a `DerivedKeyAttestation` verified against
  the space-specific derivation salt.

The operation's edits deterministically imply typed Servitor capabilities.
The materializer asks the converged Moot/Personae authority about the stable
root and every required capability. It reports each operation as:

- **effective**, and applies it;
- **pending**, retained but not currently applied;
- **revoked**, retained and withdrawn from the current projection.

Revocation is retrospective for the live communal projection: it withdraws
historical contributions while preserving their signed records for audit and
possible later authority changes. It does not rewrite the replicated log.
Transport peer identity never participates in this decision.

### 1.3 Encryption mode is a profile setting

Mere adopts `p2panda-encryption`'s engine and supplies Gemot membership,
Personae key material, and the signed causal order. It does not adopt the
`p2panda-spaces` bundle or a second authorization system.

Every shared profile chooses one of two explicit modes:

- **Data Encryption** for durable documents and knowledge spaces. A joining
  member receives retained group secrets and may read history, subject to the
  profile's retention rule.
- **Message Encryption** for a chat or other ephemeral lane that chooses
  per-message forward secrecy. A joining member does not gain old message
  plaintext, and loss of ratchet state has the corresponding recovery cost.

There is no global default hidden in the engine. A profile must state its
mode. The first knowledge/Knot communal profile uses Data Encryption. The
first chat profile chooses durable Data Encryption with eight retained epochs.
A future forward-secure profile must choose Message Encryption explicitly and
carry its own recovery contract.

Group control and welcome messages are signed operations in dedicated log
classes. Membership removal rotates the key before later application writes.
The host persists the current encryption state and retained decryption epochs.
The application event is encrypted into the operation body, then the p2panda
header signs that ciphertext and its visible addressing metadata. Those exact
operation bytes ride LogSync, native drop, and Retinue unchanged.

### 1.4 Missing history yields a partial projection

A missing causal parent blocks its operation and descendants, not the entire
space. Materialization returns the complete causally closed prefix plus
diagnostics naming each pending operation and unresolved parent. Cycles and
malformed stored facts still fail closed.

Checkpoint and pruning authority remains domain-owned. A checkpoint commits a
projection plus the causal, author-sequence, edge-counter, and authority
frontiers needed to validate and continue the retained tail. Pruning cannot
erase the last proof needed to authorize, decrypt, or continue a writer.

### 1.5 Chat is the second consumer

The smallest Commons chat profile has two content classes:

- `commons.channel`: channel metadata and membership-facing settings;
- `commons.message`: an immutable authored message with body, sent-at fact,
  channel, and optional reply target.

The first slice does not expose message editing or deletion. Existing
`Murm::Post` remains the bilateral direct-conversation protocol. A read adapter
may project compatible text/topic posts into the chat view, but no existing
post is rewritten and the two wire grammars are not silently declared equal.

After Knot and chat both use Stickleback's causal machinery, the shared module
is founded by two consumers and the old probe-local implementation is removed.

### 1.6 Concurrent remove versus insert is remove-wins

For the same node identity:

- a causally later insert recreates a removed node;
- a causally later remove removes the edited node;
- a truly concurrent remove and insert resolves to removed.

This avoids a permanent tombstone: deliberate recreation is expressible by
observing the removal first. It also prevents an unseen concurrent edit from
undoing a deletion. The rule must be executable before a Commons profile
exposes edit/delete.

## 2. Sequence and receipts

### C1. Shared causal module — DONE 2026-07-27

Move causal ordering, frontier recovery, bounds, and partial-projection
diagnostics from `commons-spine` into Stickleback. Commons remains green and
the probe-local duplicate is deleted.

Receipt: Stickleback owns bounded causal admission, author-head and frontier
recovery, topological projection, happens-before queries, and pending-root
diagnostics. Both Knot and Commons chat consume it.

### C2. Authority-effective fold — DONE 2026-07-27

Add stable-root claims and a typed authority classifier. Prove:

- an operation arriving before its certificate is pending, then becomes
  effective without reinsertion;
- revocation withdraws it without deleting the operation;
- a derived signing key cannot claim another Personae root;
- a relay delivering another author's operation has no authority effect.

Receipt: `CommonsAuthority` retains structurally valid operations and
re-projects them through pending, effective, and revoked states. Personae
derived-key attestations bind each writer to a stable root; Servitor supplies
typed capability checks.

### C3. Group-key engine — DONE 2026-07-27

Wire the p2panda data scheme first, with durable group state, welcome delivery,
removal rotation, and historical-key retention. Keep the mode enum and
message-scheme boundary explicit; the message implementation must use the same
causal operation ordering rather than an in-memory side queue.

Receipt: Stickleback persists versioned data-encryption keyrings and retained
epochs. A real p2panda DCGKA test authenticates a welcome, removes a member,
rotates the key, preserves old-epoch reads, and withholds later plaintext from
the removed member.

### C4. Knot consumer pull and durable projection — DONE 2026-07-27

Knot authors signed causal parents, reopens over Redb, reports conflicts and
pending facts without hiding unrelated documents, and restores its author
frontier. Add a projection checkpoint plus tail receipt before pruning.

Receipt: five Knot sync tests cover Memory and real LogSync convergence,
same-document conflict reporting, partial projection under missing history,
Redb reopen, checkpoint persistence, and retained-tail recovery.

### C5. Chat consumer — DONE 2026-07-27

Two members exchange immutable `commons.message` facts over Memory and real
p2panda and converge on one channel view. The chosen encryption mode is part of
the profile and test fixture.

Receipt: `commons.channel` and immutable `commons.message` use the shared
causal seam and the durable-data chat profile. Partitioned replicas converge
through both Memory acceptance and real p2panda LogSync.

### C6. Carrier identity — SOFTWARE RECEIPT DONE 2026-07-27

Export the same signed encrypted operations through a protected native drop
and the Reticulum/TCP carrier. Compare canonical operation bytes before and
after each carriage. Direct-PHY RF remains a hardware receipt, not a software
substitute.

Receipt: one encrypted, signed operation record survives a protected native
drop and Reticulum/TCP byte-for-byte; the recovered p2panda signature verifies.
The direct-PHY RF receipt remains deliberately open.

### C7. Product policy and profile — DONE 2026-07-27

Execute remove-wins for truly concurrent remove/insert, publish the first
Commons profile document with authority states, encryption mode, limits,
conflict behavior, and retention rules, and update the brief/index/Knot plan.

Receipt: the Commons fold executes concurrent remove-wins and permits a
causally later insert to recreate. The first Commons profile records authority,
encryption, limits, conflict, missing-history, durability, and retention
contracts.

## 3. Stop rules

- Session admission never becomes operation authority.
- Knot does not translate documents through chartulary merely to reuse a lane.
- A causal total order never masquerades as text merge.
- A missing parent does not hide unrelated causally complete state.
- Pruning never outruns the authority, decryption, or author-continuation
  frontier.
- Native drop and Retinue carry operation bytes; they do not re-encrypt or
  reinterpret domain payloads.
- Chat edit/delete does not ship before concurrent remove/insert has an
  executable rule.
