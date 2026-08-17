# Facets as a Signaling Medium: Interior Signaling, Decay, and Control Loops

**Date:** 2026-08-16
**Status:** design round (Mark, 2026-08-16). Gives open question 4 of the
[one node, atomic facets layer map](2026-07-18_one_node_facets_layer_map.md)
("facet grants") a definite shape, and names one verified gap in the servitor
reactive substrate. No code changed by this doc.

Companion to the one-node ruling (which established facets as the metadata
mechanism) and to the participant gate work (which owns the authority half).
Where the one-node doc asked what a node *carries*, this doc asks what
participants *say to each other* through what nodes carry.

## 1. The question, and its correction

The round opened on whether facets are the beginning of an internodal
communication substrate, a codec by which nodes become aware of their
neighbors.

The codec half is right and already built. The internodal half is not, and the
correction is load-bearing rather than pedantic: a `Container` is passive data
and cannot be aware of anything. The things that act are **participants** (apps,
denizens, scripts, peers), all of which pass through one gate. So facets are not
node-to-node communication. They are **participant-to-participant communication
through nodes**, which is the blackboard and tuple-space lineage, and which the
stigmergy metaphor describes accurately.

Neighbor-awareness proper already has three homes, none of them facets: edges
carry internodal statements (the semantic ring), fields and couplings carry
literal neighbor response in arrangement space (numen, quint, seiche), and
signals derives structural awareness globally. A facet is keyed to one node id.
Payloads may reference other nodes, and `denizen.binding` already does, so
cross-references ride the medium; nothing evaluates one node's facets in
another's context.

## 2. What the tree already provides

Verified against code during the round:

- **The codec.** Namespaced `FacetId` plus schema engram is the registry; the
  opaque payload is the frame. Unknown facet ids are preserved untouched across
  load and re-save (`crates/eidetic/chartulary/src/facet.rs`), which is the
  forward-compatibility rule that separates a medium from private state. A
  medium must not destroy messages it cannot read.
- **The wire.** Facet writes are attributed journal edits (`SetFacet` /
  `RemoveFacet` in both `EditSpec` and `GraphEdit`), so facet traffic is
  ordered, attributed, and replayable. The code calls them *independently
  mergeable*, and that phrase is doing real work: concurrent facet writes to one
  node do not contend.
- **The membrane.** `FacetStore` is a field of `GraphLog` and part of the
  compacted snapshot (`spine.rs`), not a sidecar beside it. A nested graph
  therefore carries its own facets *inside its own log*, so interior signaling
  forks, archives, and replays as one body with the subgraph. This was checked
  by looking for the opposite: `archive_nested` moves only the log and snapshot
  slots, which would orphan a facet sidecar if one existed. There is none to
  orphan.
- **Actuation and authority.** A petition is a scope-claimed batch against a
  nested graph, and `SetFacet` / `RemoveFacet` appear in the gate's
  `touched_nodes`, so facet writes are already scope-checked by node id.
- **Sensing.** `servitor/src/watch.rs` provides standing subscriptions with a
  containment law (a watch must sit inside what its subject may already read),
  no self-waking, and cursors rather than replays.
- **Loop bounding.** `servitor/src/cascade.rs` bounds how far one wake travels,
  with a budget that names the subjects still waking each other instead of
  truncating silently.

## 3. Interior and exterior signaling

The asymmetry has a consistency cause, not merely a convenience one.

**Inside one graph** there is a single serialization point: one journal, one
revision counter, revision-checked atomic batches. That is enough to support
protocol, ordered exchange, handshakes, and control loops.

**Across graphs** there is no shared clock and no shared revision, since each
log commits independently. Exterior marks must therefore be idempotent,
self-describing, and tolerant of being read late or never. Unknown-facet
preservation is precisely that guarantee.

Interior can afford conversation; exterior can only afford deposits. The
endocrine and pheromone metaphors land on opposite sides of a real boundary,
which is why the intuition that a subgraph affords more structure than the open
graph is correct.

## 4. Signals are not tags

A signal is tag-shaped at a glance, and the tree already contains one hand-rolled
instance: the grant projection encodes `cap`, `mode`, `subject`, and `expires`
as prefixed tag strings with a fail-closed reader. Decay still belongs on facets
rather than tags, for two verified reasons.

1. **There is no tag edit.** `EditSpec` and `GraphEdit` offer InsertNode,
   RemoveNode, Connect, Disconnect, Derive, SetFacet, and RemoveFacet. Node
   payload changes only by whole-node upsert, so a decaying tag would rewrite the
   entire node into the journal on every tick, and two participants signalling on
   one node would contend on the whole payload.
2. **Tags export.** A node's tags project to RDF as `schema:keywords`
   (`crates/eidetic/scholia/src/project.rs`). A decaying tag would publish a
   transient interior signal as a permanent public descriptor.

The resulting rule is compact. A **tag** says what a node *is*: durable,
descriptive, exported as public vocabulary. A **signal** says what is happening
to it *now*: transient, coordinating, confined to the membrane. Those properties
travel together, so decay and non-projection are one design decision seen from
two sides. Anything that should fade is also something that should never have
entered the linked-data ring.

## 5. Decay must not break replay

The spine guarantees that live mutation and replay produce identical state, and
cascade rounds run in stable order so that a recorded cascade replays. Implicit
wall-clock expiry that mutates the store would break both.

Two safe forms:

1. **Read-time predicate.** Store a valid-until with the value and filter on
   read. The store never changes behind replay.
2. **Explicit journaled sweep.** Emit `RemoveFacet`, so the removal is itself an
   entry and replays with everything else.

Expiry indexed by revision rather than wall clock is the most replay-friendly of
all. Note that only the read-time form composes with the cascade replay property,
so the two features constrain each other toward the same answer.

**Two meanings of decay, kept apart.** A discrete shelf life (this signal is
stale after N) is facet-shaped and cheap. A falling concentration you can follow
up a gradient, as a real trail, is continuous dynamics and belongs to the field
layer with numen, quint, and seiche. Both are honestly called decay; only the
first belongs in facet space. Building the second there would re-derive seiche.

## 6. Control loops

The loop is already closed in code: a watch senses, a host-supplied runner
computes, a gate petition actuates, and the resulting commit wakes the next
watch. Three of cascade's four stated properties are stability properties in
disguise. No self-waking kills the trivial feedback path structurally rather than
by budget. Cursors mean a loop cannot re-chew its own history. Stable round order
means a controller's entire history is reproducible, which is unusual and worth
protecting: every actuation is an attributed journal entry, so a control loop
here can be replayed to see why it acted.

**The gap, verified by search: depth is bounded, frequency is not.**
`CascadeBudget` counts rounds within one cascade, and servitor contains no rate,
throttle, debounce, hysteresis, cooldown, or period anywhere. A behavior that
settles and re-triggers a second later, forever, never exhausts the budget,
because each individual cascade is short. It is a slow limit cycle and the budget
cannot see it.

This matters more here than in an ordinary control system. Normally instability
wastes energy and wobbles visibly. In an event-sourced substrate, instability
**writes history**. The graph is the replay of the journal, so an oscillating
behavior permanently inflates load and replay cost, and that damage outlives the
fix: deleting the misbehaving denizen does not shrink the journal it wrote. That
argues for deadband as first-class machinery a behavior declares (a minimum
change, a minimum interval) rather than discipline every modder must reinvent
correctly.

The metaphor prescribes the same engineering. Endocrine control loops are slow,
graded, decaying, and setpoint-seeking, and every one of those adjectives is a
stability property; fast, undamped, non-decaying feedback in a body is a seizure.
Deadband and decay are one prescription arriving from two directions.

**One thin spot for setpoint controllers.** A `WatchEvent` carries a sequence
number, an author, and the scopes touched, but not what changed to what. A
controller therefore wakes on any touch in scope and must re-read to sense a
value, so threshold crossing cannot be expressed in the watch itself. That is the
right call for keeping the matcher tier-agnostic across the two journals, and a
predicate dimension is where it would grow, alongside the main-graph region
vocabulary the module header already flags as missing.

## 7. Facet grants (one-node open question 4)

Today a facet write is scope-checked by node id, so a write grant on `trail/`
permits writing *any* facet id on nodes under it, including `denizen.binding` or
`web.*`. The gate governs which tissues may be touched, not which hormones may be
secreted.

The missing axis is the facet namespace, and it fits the existing shape rather
than straining it. `Cap` is already a multi-kind enum with per-kind coverage laws
(Power by equality, Scope by segment prefix, cross-kind always false). Facet ids
are dot-namespaced by convention, so a facet kind with prefix coverage is the
natural third member: a grant on `web.` covers `web.viewer` and never covers
`denizen.binding`.

## 8. Targets

1. **Facet-namespace capability kind**, with the gate checking it alongside
   scope. Done when a grant can permit `web.*` while refusing `denizen.*` on the
   same nodes.
2. **Expiring facets in read-time-predicate form.** Done when a recorded cascade
   replays identically across an expiry boundary.
3. **Deadband declaration** on a behavior (minimum change, minimum interval).
   Done when a slow limit cycle is refused or named rather than silently writing
   journal history.
4. **Watch predicate dimension** (threshold crossing), after target 3 and gated
   on a real consumer.

## 9. Open questions

1. **Does deadband belong to sensing or actuation?** On the watch it suppresses
   the wake; on the gate it suppresses the write. The actuation side also bounds
   journal growth from writers that were never woken, which is the failure this
   is meant to prevent.
2. **Revision-indexed or wall-clock expiry.** Revision indexing replays
   perfectly, but a graph with no writes never expires anything. Wall clock needs
   read-time evaluation to stay replay-safe.
3. **Does a signal want a reserved namespace**, so a reader can distinguish
   signal from state without a schema lookup?
4. **Scripts as participants.** Scripts have no graph surface today (the rhai
   lane is the knot block evaluator plus the privileged omnibar shell), so the
   lane is open. The shape follows from the gate: a script should petition, never
   hold a graph handle, exactly as denizens do.

## Progress

- **2026-08-16:** Design round with Mark, recorded here. Every claim above was
  checked against the tree rather than against prior docs. One correction earned
  during the round is preserved deliberately: an earlier claim that facet writes
  were unobservable was wrong, because `watch.rs` and `cascade.rs` already
  provide standing subscriptions and cascade bounding. The habit that produced
  the error was reasoning from the facet layer alone without opening the
  servitor modules next to it.
