# Facets as a Signaling Medium: Interior Signaling, Decay, and Control Loops

**Date:** 2026-08-16
**Status:** design round (Mark, 2026-08-16). Gives open question 4 of the
[one node, atomic facets layer map](2026-07-18_one_node_facets_layer_map.md)
("facet grants") a definite shape, and names one verified gap in the servitor
reactive substrate. Targets 1 through 3 landed 2026-08-18; target 4 remains
open behind a real-consumer gate.

Companion to the one-node ruling (which established facets as the metadata
mechanism) and to the participant gate work (which owns the authority half).
Where the one-node doc asked what a node *carries*, this doc asks what
participants *say to each other* through what nodes carry.

**Related:** the
[graph behaviors plan](../implementation_strategy/2026-08-13_graph_behaviors_plan.md)
owns the watch and cascade substrate section 6 reasons over (its slices W0
through W5 landed 2026-08-13 and its actuation deadband landed 2026-08-18); the
[capability model plan](../implementation_strategy/2026-07-23_capability_model_plan.md)
owns the capability algebra section 7 proposes extending.

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
  forks, archives, and replays as one body with the nested graph. This was checked
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
which is why the intuition that a nested graph affords more structure than the open
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
    (`crates/eidetic/scholia/src/project.rs` *(historical citation)* <!-- doc-audit: historical-path -->). A decaying tag would publish a
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

**The gap verified 2026-08-16: depth was bounded, frequency was not.**
`CascadeBudget` counted rounds within one cascade, and servitor contained no rate,
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

The owning plan was equally silent. The graph behaviors plan carried no rate,
thrash, frequency, or oscillation language either, so it was a hole in the
design rather than a known deferral someone parked. Target 3 closed that hole on
2026-08-18 at actuation: a behavior declares minimum change and interval,
supplies one signed output in its own stable units, and moves its persisted
baseline only after an attributed graph commit lands.

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

Homes, since none of this work lands here: target 1 belongs to the
capability model plan, targets 3 and 4 to the graph behaviors plan, and target 2
to chartulary with a mere-side reader.

1. **DONE 2026-08-18: facet-namespace capability kind**, with the gate checking
   it alongside scope. A `web.` grant permits `web.viewer` while refusing
   `denizen.binding` on the same node. **Sequencing ruling:** leaf first, then
   the kind in the same pass. `mere-capability` now owns `Capability`, `Cap`,
   `ScopePath`, `Mode`, and `FacetNamespace`; servitor and gemot depend on it
   directly, while Servitor keeps compatibility re-exports.
2. **DONE 2026-08-18: expiring facets in read-time-predicate form.** Chartulary's
   generic `ExpiringFacet<T>` envelope names the first stale graph revision;
   Pandect's `read_expiring_facet` takes the revision explicitly, returns no
   value at or beyond the boundary, and never mutates the stored envelope. A
   recorded cascade crosses the boundary and its before/after reads replay
   identically from the two journal prefixes.
3. **DONE 2026-08-18: deadband declaration** on a behavior (minimum change,
   minimum interval). It is enforced at actuation against one behavior-defined
   signed scalar and a host-fed instant. A slow limit cycle is named before it
   writes journal history, and accepted state survives restart.
4. **Watch predicate dimension** (threshold crossing), after target 3 and gated
   on a real consumer.

## 9. Open questions

1. **CLOSED 2026-08-18: actuation.** Suppressing a watch cannot bound scheduled
   or hand-run writers. The gate checks the behavior's declared output and the
   host-fed instant before commit, then records the accepted baseline only after
   that subject's graph entry lands. A refused or stale petition therefore
   cannot move either baseline.
2. **CLOSED 2026-08-18: revision-indexed expiry.** The target's replay
   done-condition rules here: `expires_at_revision` is deterministic at every
   journal prefix. A quiet graph deliberately does not advance the shelf life.
   Wall-clock expiry remains a different, unbuilt policy and must not be smuggled
   into this reader later.
3. **Does a signal want a reserved namespace**, so a reader can distinguish
   signal from state without a schema lookup?
4. **Scripts as participants.** Scripts have no graph surface today (the rhai
   lane is the knot block evaluator plus the privileged omnibar shell), so the
   lane is open. The shape follows from the gate: a script should petition, never
   hold a graph handle, exactly as denizens do.

## Progress

- **2026-08-18 (target 3 complete):** added Servitor's persisted behavior
  deadband and the two-phase `Gate::petition_behavior` path. Each declaration
  has a positive minimum change and minimum interval; each run supplies one
  signed scalar in behavior-defined units plus a host-fed instant. Refusal names
  the subject and every failing dimension. Turnstone accepts `-- @deadband
  <minimum-change> <minimum-interval-ms>`, exposes `mere.output(value)`, shows
  the declaration during install review, and applies one actuation check to
  manual, watch and scheduled resident runs before graph-action lowering. The
  accepted baseline persists beside watches and is removed on uninstall. The
  slow-cycle receipt uses long-separated `0/1` outputs and leaves graph revision
  unchanged; a separate receipt proves a stale commit does not consume the
  interval. The Turnstone receipt writes once, reloads, then refuses the next
  actuation without another journal entry. Servitor is 65/65 and passes Clippy
  with warnings denied; Turnstone is 316 passed with 4 explicit endpoint
  ignores. No headed target-3 scenario was added. Target 4 remains gated on a
  real threshold-crossing consumer.

- **2026-08-18 (target 2 complete):** added chartulary's generic
  `ExpiringFacet<T>` envelope and Pandect's fail-closed `read_expiring_facet`.
  The expiry boundary is the first stale graph revision, so the read is live
  only while `revision < expires_at_revision`; neither the live path nor replay
  removes the stored bytes. The receipt records a one-round Servitor cascade:
  its answer advances a chartulary `GraphLog` from revision 2 to 3, expiring a
  signal at revision 3, then replays the prefix on each side and obtains the
  same `[Some("pulse"), None]` reads. Focused sets are Chartulary 10/10 and
  Pandect facet-store 7/7; their full library suites are 54/54 and 274/274.
  Chartulary passes Clippy with warnings denied, and Pandect Clippy reports no
  diagnostic in the changed module; crate-wide warning denial is already
  blocked by 43 existing diagnostics elsewhere. The signal-namespace question
  remains open. At that checkpoint, targets 3 and 4 remained open.

- **2026-08-18 (target 1 complete):** extracted the dependency-free
  `mere-capability` leaf, added dot-segment `Cap::Facet`, carried its order
  losslessly through personae's signed delegation path, and made the gate
  require facet authority for both set and remove. The done-condition receipt
  grants `web.` and writes `web.viewer`, then refuses `denizen.binding` on the
  same node; the Gemot receipt asks the same three questions through its typed
  authorization adapter. `mere-capability` 8/8, servitor 54/54, gemot 112/112
  (including typed authorization 9/9), Turnstone's focused signed-install /
  uninstall-revocation test 1/1, and touched-package Clippy with warnings
  denied. At that checkpoint, targets 2 through 4 remained open.

- **2026-08-16 (open-work sweep, same day):** checked this round's findings
  against the plans that own them. Three results. **The frequency gap is
  unclaimed** by the graph behaviors plan as well as by the code, so it is now
  named in that plan's status header and Progress. **One claim made and
  withdrawn:** the capability model plan's C3 and C4 read as deferred, but that
  bullet sits inside its historical 2026-07-23 entry, and the 2026-07-24 entry
  records C3, C3' and C4 all landing. What *is* still pending there is D3b's
  algebra extraction, which is why target 1 carries a sequencing note; the
  facet-namespace request is filed in that plan under 2026-08-16. **And this
  doc's own first draft** used the bare word "subgraph" three times, which
  TERMINOLOGY.md and the gate plan both reserve for "nested graph"; corrected
  here and in the one-node Progress entry.

- **2026-08-16:** Design round with Mark, recorded here. Every claim above was
  checked against the tree rather than against prior docs. One correction earned
  during the round is preserved deliberately: an earlier claim that facet writes
  were unobservable was wrong, because `watch.rs` and `cascade.rs` already
  provide standing subscriptions and cascade bounding. The habit that produced
  the error was reasoning from the facet layer alone without opening the
  servitor modules next to it.
