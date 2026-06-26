# Federation interop: engram schema lenses + capability-scoped sharing

**Date**: 2026-06-26
**Status**: Scoped, not started. Two federation-interop mechanisms borrowed from the
[borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md), scoped now
(ahead of the knot-editor resume) because both are load-bearing for federation and
painful to retrofit once peers are exchanging data. Implementation deferred.

Both answer "how do two peers that are not identical still interoperate": one over
**schema drift** (their engrams are shaped differently), one over **access control**
(who may read which slice). They are independent mechanisms sharing a context, so this
plan scopes both and keeps them separable.

## 1. Engram schema lenses (Cambria)

**Gap.** Engrams are schema plus data (`SchemaRef` + a JSON payload: mere-native,
json-schema, or json-ld). The [alembic core](2026-06-24_alembic_implementation_plan.md)
shipped 2026-06-24 with no schema-evolution path, so two coalitions on different
engram-schema versions cannot read each other's engrams, and lockstep upgrades across a
federation are not realistic.

**Borrow.** Cambria (Ink & Switch): bidirectional, composable **lenses** between schema
versions, built from primitive ops (rename, hoist, wrap, add-with-default, remove,
head). A lens reads old data as new and writes new as old, so a peer applies a lens
chain to read an engram authored under a schema it does not hold.

**Fit.** Payloads are JSON, which is exactly Cambria's domain, so the lens ops apply
directly. A lens registry keyed by `(from_schema, to_schema)` lives beside the engram
schema registry (alembic). It is consulted at the boundary where engrams cross peers
(sync / projection), not in the hot graph path: on receiving an engram whose `SchemaRef`
is not the local version, resolve a lens chain and project it to the local shape;
bidirectionality lets a write flow back.

**Decisions to settle.**
- Lens authorship: hand-written per schema bump, derived from a schema diff, or both.
- Storage: lenses as their own engram kind (so they federate like any other content) vs
  a side registry.
- Failure mode when no lens chain resolves: render the engram inert (the received-content
  rule), not a hard error.

**Phase sketch.** Lens core + registry + JSON ops (engram-local, fully unit-testable)
before any network wiring; the sync-boundary hook second.

## 2. Capability-scoped subgraph sharing

**Gap.** No federation access-control primitive: nothing says "peer X may read subgraph
S, revocably," without a server. The federation tiers (moot, moothold, coalition) need
it to share a slice with a peer.

**This is the federation sense of "capability," distinct from two existing in-app
senses** that must not be conflated: the
[capability-gate catalogue](../research/2026-05-14_capability_gate_catalogue_brief.md)
(the permission spine the action bus consumes) and DocumentScript's WASM-component
confinement
([document_script_substrate](2026-06-21_document_script_substrate_plan.md)). Those gate
what local code may do; this gates what a remote peer may read or write.

**Borrow.** Object-capability tokens: Meadowcap (Willow), UCAN, or Keyhive
(Ink & Switch). p2panda-dependent (the federation substrate, per
[event_dag_substrate](2026-05-07_event_dag_substrate_brief.md)). Per Mark's
borrow-discipline, sketch the layered architecture before picking one, rather than
adopting a stack wholesale.

**Fit.**
- Scope = a subgraph. The
  [graph-cluster namespaces](../research/2026-05-10_graph_cluster_namespaces_brief.md)
  brief already maps community-derived namespaces to a capability scope, so a share is a
  grant over a namespace, not an ad-hoc node set.
- The grant dovetails with **Tessera** as the trust receipt and with the tier framework
  (a grant's reach graduates with trust).
- Revocability without a server is the hard part: capability tokens plus an epoch /
  key-rotation model (Keyhive's domain) rather than a server-side revocation list.

**Decisions to settle.**
- Which borrow (Meadowcap vs UCAN vs Keyhive), via a layered sketch: token format,
  delegation, revocation, and how each rides p2panda.
- Read-only grants first; write-capability is a later rung.
- Granularity: namespace-scoped grants first, finer (per-node) only if a need proves it.

**Phase sketch.** The layered borrow sketch and the token model first (design), then
read-only namespace grants over p2panda, then revocation, then write-capabilities.

## Sequencing

Lenses are the cheaper and more self-contained of the two (engram-local, unit-testable
without the network), so they lead. Capability sharing depends on the p2panda
integration and a borrow decision, so it follows. Neither blocks the knot-editor resume;
both are scoped here so the design is settled before federation data starts flowing.

## Cross-references

- [borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md): the source.
- [alembic implementation plan](2026-06-24_alembic_implementation_plan.md): the engram
  layer the lenses extend.
- [event_dag_substrate brief](2026-05-07_event_dag_substrate_brief.md): p2panda, the
  federation substrate.
- [graph-cluster namespaces brief](../research/2026-05-10_graph_cluster_namespaces_brief.md):
  namespace = capability scope.
- [capability-gate catalogue brief](../research/2026-05-14_capability_gate_catalogue_brief.md),
  [document_script_substrate plan](2026-06-21_document_script_substrate_plan.md): the
  in-app capability senses this is distinct from.
- [persona_transport_unlinkability](2026-06-25_persona_transport_unlinkability_plan.md),
  [actor_constellation](2026-06-03_actor_constellation_plan.md): persona and federation
  context.

## Progress

- **2026-06-26, scoped.** Created from the borrowed-ideas brief at Mark's direction
  (scope the federation-load-bearing pair before resuming the knot editor). Both
  mechanisms are net-new: the alembic plan has no schema-evolution coverage, and the
  federation capability sense is unscoped (the existing "capability" docs are the in-app
  permission spine and DocumentScript confinement, a different layer). Design-level only,
  no code.
