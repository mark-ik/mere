# Shared-Engram Commons Brief

**Date:** 2026-07-24
**Status:** direction note from the 2026-07-24 application brainstorm (see
the [application prospects brief](../../2026-07-24_application_prospects_brief.md));
no code task. Names what a communal graph actually requires, which parts
exist, and the two decisions nothing currently owns.

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
   which privacy selection and byte budgets (murm-replication's
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

## Decision 1: multi-writer convergence (unowned)

Everything proven so far is single-owner or per-author-log sync. The
deletion/retention plan's direct-conversation work already gives per-author
frontier catch-up over `ConversationStore`; what no doc decides is the merge
rule when two members concurrently edit one shared *container* (graph
structure plus facets) while offline. That is deterministic merge over
per-author logs, which is p2panda's core model; before designing anything,
check how much murm-replication already inherits from p2panda's ordering
versus what chartulary's GraphJournal needs defined on top.

Done-condition: a written merge rule under which two offline members editing
the same container reconverge to one graph on sync, property-tested, before
any chat implementation slice.

## Decision 2: group keys (named in murm's remainder, not scoped)

The deletion/retention plan lists production group encryption as remaining
work, and p2panda-encryption is the planned lane (per the
[LXMF brief](2026-07-06_lxmf_key_addressed_mail_research.md)). The undecided
part is the moot-level contract: key agreement per shared space, rotation on
membership change (what a join reveals, what a leave forecloses), and the
requirement that sealed payloads ride any carrier unchanged, because iroh,
p2panda, and retinue all stay opaque. This is an MLS-shaped problem; decide
adopt-versus-blueprint the same way the LXMF brief did for mail, and decide
it once, because every content class inherits the answer.

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
boundary.

## Sequencing

Not urgent; blocking the day chat is scoped. Done-conditions:

- the two decisions above each have a dated design doc (one doc covering
  both is fine) before any chat slice lands;
- the first chat receipt is two members exchanging messages in one shared
  engram over Memory, then p2panda, then retinue, reusing the managed-network
  plan's V6/V7 admission matrix;
- the knowledge commons then adds schemas and profile vocabulary, not
  machinery.
