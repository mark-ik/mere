# Shared-Engram Commons Brief

**Date:** 2026-07-24
**Status:** direction note from the 2026-07-24 application brainstorm (see
the [application prospects brief](../../2026-07-24_application_prospects_brief.md)).
Names what a communal graph actually requires and the two decisions the note
originally found unowned. Both decisions are now answered by executable
receipts and the [Commons profile](../design/2026-07-27_commons_profile_v1.md).

## The commons is a profile, not an engine

The substrate for a shared, encrypted, permissioned graph already exists or
is in flight: codicil holds per-writer append-only logs, chartulary holds the
container graph with the GraphJournal edit spine, muniment stores it, murm
replicates it (the
[peer runtime plan](../implementation_strategy/2026-07-12_murm_peer_runtime_and_moot_domain_plan.md)
and the
[deletion/retention plan](../implementation_strategy/2026-07-12_deletion_retention_and_native_drop_plan.md)),
retinue and p2panda carry it, personae proves authority, gemot owns
membership. A sibling product embedding a shared graph later would pull these
same crates; there is no commons crate to found.

What makes a graph communal is three declarations over that substrate:

1. **a shared vocabulary of content classes** (page, post, place, person,
   file: each an engram schema per the
   [one-node facets ruling](../technical_architecture/2026-07-18_one_node_facets_layer_map.md));
2. **a replication policy for those classes**: what syncs to whom, under
   which privacy selection and byte budgets (stickleback's
   selector/budget seam; V10 of the
   [managed-network plan](../implementation_strategy/2026-07-24_low_power_managed_network_plan.md));
3. **membership and moderation**: gemot for who belongs, personae delegation
   for what each member may do, revocation as the moderation primitive.

The managed-network plan's V5 evaluator already anticipates the hook:
`ProfileRef` names a shared vocabulary without making it locally
authoritative. The commons profile is the first real inhabitant of that
field.

## Chat is the smallest commons

Chat is not a step before the commons; it is the commons at its smallest. A
channel is a shared engram with two content classes, membership, encryption,
and replication. A call is a session negotiated inside one. The knowledge
commons is the same primitive with a richer schema set. Building chat as a
bespoke feature and a commons later would build the multi-writer and
group-key machinery twice; the ruling from the brainstorm is one spine, with
chat as its first content class.

## Decision 1: multi-writer convergence (answered 2026-07-26/27)

Before this decision, everything proven was single-owner or per-author-log
sync. The deletion/retention plan's direct-conversation work already gives
per-author frontier catch-up over `ConversationStore`; what no doc had decided
was the merge rule when two members concurrently edit one shared *container* (graph
structure plus facets) while offline. That is deterministic merge over
per-author logs, which is p2panda's core model; before designing anything,
check how much stickleback already inherits from p2panda's ordering
versus what chartulary's GraphJournal needs defined on top.

Done-condition: a written merge rule under which two offline members editing
the same container reconverge to one graph on sync, property-tested, before
any chat implementation slice.

**Answered 2026-07-26/27** by the
[commons multi-writer convergence plan](../implementation_strategy/2026-07-26_commons_multi_writer_convergence_plan.md).
Checking the substrate first, as this section instructed, changed the answer:
the replication layer already provides per-author order and a deterministic
cross-author tiebreak, so convergence needed no CRDT or wall clock. It did
need application-level causality: author-first sorting alone gives permanent
public-key priority, so each signed commons record now carries the exact
per-author operation frontier its writer observed. Materialization preserves
that happens-before relation and uses the stable key tuple only for concurrent
records. The other gap was chartulary's per-log `EdgeId` counter, which
**collided** between writers and which no ordering can repair; edge identity
is now `(writer, counter)` in chartulary 0.2.0, bound to the operation signer
and checked against a monotonic stored frontier.

The merge rules are stated and property-tested. A truly concurrent removal now
wins over an insert of the same node, while an insert that causally observes
the removal deliberately recreates it. The tracked `commons-spine` workspace package now bridges chartulary
to real p2panda LogSync: partitioned members reconverge to identical graph
fingerprints, one member can then edit the other's synced edge and reconverge
again, and an actual Redb close/reopen resumes both operation and edge
counters. The profile now carries the product-facing limits.

## Decision 2: group keys (answered 2026-07-27)

The deletion/retention plan lists production group encryption as remaining
work, and p2panda-encryption is the planned lane (per the
[LXMF brief](2026-07-06_lxmf_key_addressed_mail_research.md)). The undecided
part was the moot-level contract: key agreement per shared space, rotation on
membership change, and byte-identical carriage.

**Answered** by the
[authority, keys, and consumers plan](../implementation_strategy/2026-07-27_commons_authority_keys_consumers_plan.md)
and the Commons profile. Mere adopts the `p2panda-encryption` engine rather
than `p2panda-spaces`. Gemot's converged member set supplies its DGM seam.
Profiles choose Data Encryption for retained knowledge or Message Encryption
for forward-secure messages. The first knowledge and chat fixtures both name
Data Encryption explicitly. Authenticated DCGKA welcome supplies the initial
epoch; removing a member rotates before later writes and withholds the new
secret from that member. The key state survives serialization. Application
bytes are encrypted first and the p2panda operation signs the ciphertext, so
protected native drop and Reticulum/TCP carry the same signed bytes.

## The page format candidate: knot

Knot documents (nematic's djot engine) are the natural "page" content class:
small textual diffs make small codicil ops, which is what LoRa budgets want,
and one document renders in pelt over http and in turnstone over the mesh. The
editor-side sequencing (text-editing primitives at the cambium/genet layer)
is genet's, recorded in
[`docs/2026-07-24_pelt_knot_direction.md`](https://github.com/mark-ik/genet/blob/main/docs/2026-07-24_pelt_knot_direction.md).

## LXMF boundary posture

The 2026-07-06 LXMF brief's recommendation C stands: shared-engram spine
internally, LXMF as an interop boundary. Its 2026-07-24 addendum records what
changed: retinue's landing re-prices the bridge from a stack migration to a
small spec-based codec sibling (sennet posture), and the radio business adds
two demand arguments (day-one Sideband interop for flashed radios;
propagation as a V9 offered role). LXMF's message-shaped model stays at the
boundary. The first codec slice now lives in Retinue's `outrider` crate and
round-trips a captured LXMF 0.9.6 message without importing LXMF types into the
Commons model.

## Calls

Calls remain their own product, built inside a Commons rather than folded into
chat replication. The
[calls plan](../implementation_strategy/2026-07-27_commons_calls_plan.md)
uses a retained Commons invitation to find an admitted live Notochord session,
then keeps presence, media control, and carrier quality ephemeral. IP is the
first live-audio bearer. Reticulum and direct PHY carry invitations, terminal
state, and voice notes until a separate radio-voice profile earns stronger
claims.

## Sequencing

The software done-conditions are met:

- the two decisions have dated plans plus one product profile;
- the first chat receipt exchanges encrypted immutable messages over Memory
  and real p2panda LogSync;
- canonical signed ciphertext survives protected native drop and
  Reticulum/TCP unchanged;
- immutable edit and delete facts change the current chat projection without
  pretending received ciphertext vanished;
- graph facets merge independently, while Knot owns bounded automatic
  document-text merge;
- Outrider owns the LXMF boundary codec, and calls have a separate gated plan;
- the knowledge commons then adds schemas and profile vocabulary, not
  machinery.

Direct-PHY RF passed on real T114 and Heltec V4 hardware 2026-07-27. The
received canonical operation retained its p2panda identity and signature,
decrypted through the saved Stickleback keyring, and recovered the expected
chat event.
