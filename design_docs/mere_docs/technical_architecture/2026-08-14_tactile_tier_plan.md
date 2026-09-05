# Tactile Tier Plan (2026-08-14)

**Status:** founded 2026-08-14; T1-T3 landed the same day. The tactile
half of the two-tier ruling (field system extraction doc, amendments of
2026-08-13): CPU rapier, the bodies a hand manipulates, the source of
commitment events. Its doctrine is already ruled and this plan builds
the vocabulary on it:

- **Physics proposes, the record disposes** (P1, spatial compute plan):
  seiche answers geometric questions; the host mints facts at explicit
  commitments. Nothing here adds a fact type to seiche.
- **Every physical parameter is a data binding** (the projection
  ruling's amendment): materials, sieves, and springs are tunable
  per-mere configuration, and the *mapping* from data to parameter is
  the host's metaphor to choose. seiche carries the levers, never the
  meanings.
- The failure mode to refuse is decorative physics (BumpTop): each
  gate below must move something a workflow can read, not just look
  physical.

## Gates

### T1. Materials as data (landed 2026-08-14)

`NodeMaterial { density, friction, restitution, gravity_scale }`,
settable per node at runtime and remembered across body re-syncs. The
gravity lever matters most: nodes are weightless by design (layout does
not fall), so *weight is opt-in per node*, which is what lets a mere
say "old documents are heavy" without every node dropping off the
canvas.

**Done when:** a material read back off the live body matches what was
set; a weighted node falls under world gravity while a default node
does not move; and restitution visibly changes what a fall does (a
bouncy node rebounds higher than a dead one off the same floor).

### T2. The sieve (landed 2026-08-14)

A scene body that blocks some kinds of node and passes the rest: a
collision predicate, felt as a wall. Nodes carry `Kinds` (a 16-bit
band of host-defined meaning); a sieve declares which kinds it blocks;
rapier's interaction groups do the rest, with no per-frame work.

The bit algebra, recorded because it is the whole trick: nodes carry
their kinds in *memberships* and the whole kind band in *filter*;
a sieve carries its blocked kinds in both. The pairwise test then
reduces to `kinds ∩ blocks ≠ ∅`, independent of the tangibility lever,
and a kindless node passes every sieve (blocked-ness is opt-in).
Node-node exclusion and ordinary scene tangibility are untouched.

**Done when:** over the same fixed floor declared as a sieve, a node of
a blocked kind lands and rests while a node of another kind falls
straight through, in one run.

### T3. Support proposals (landed 2026-08-14)

`supports_of(node)`: which bodies hold this one up, read from rapier's
live contact graph (contact normal pointing upward from the supporter,
contact actually touching). Read-only, like `containments_of`: a
proposal the host may promote to a fact ("resting on", "stacked on") at
a commitment, never a stored relation.

**Done when:** the T2 scene's blocked node names the floor as its
support and the passed node names nothing.

### T4. Piles (open)

Proximity clusters as recoverable informal groupings: union-find over
the node-node contact graph, surfaced as a derived query. The one
BumpTop idea worth keeping. Open until a host wants it; the contact
graph read in T3 is most of the work.

### T5. The mere profile (open)

The host-side half: a mere's physics profile as data (which data facts
map to which material parameters, which facets define kinds, which
gestures commit which proposals), wired into a real canvas. This is
where the metaphors become configurable, and it belongs to the canvas
host rather than to conatus.

## Stop rules

- No fact types, logs, or callbacks in seiche: proposals are reads.
- No second dynamics engine; rapier is the tactile substrate
  (place-graph plan §0.10 reserve clause unaffected).
- Kinds mean nothing to seiche. The host defines the vocabulary; a
  `Kinds` bit is as opaque as a `NodeKey`.

## Findings

- **2026-08-14 (T1):** `NodeMaterial` already existed (restitution /
  friction / density, from the node-rep work), so T1 was an extension,
  not a creation: `gravity_scale` (the weight opt-in), per-node memory
  so a re-synced body comes back tuned, and a live read-back
  (`node_material`) off rapier state so the receipt asserts what
  actually runs, not what was remembered. One consumer edit in
  mere-canvas (a struct literal grew `..Default::default()`).
- **2026-08-14 (T2):** the sieve needed no new collision machinery,
  only a bit-allocation discipline: rapier's 32-group space splits into
  the two existing groups and a 16-bit kind band, and the And-mode
  pairwise test computes the predicate. The tangibility lever survives
  unchanged because kinds ride in different bits than the scene group.
  Consolidation fell out: the old scene-tier `remask_node` and the
  sieve remask collapsed into one function in `sift`, the single place
  node groups are computed, with per-node tangibility read back off the
  live collider filter so the two axes compose instead of clobbering.
- **2026-08-14 (T3):** rapier's manifold normal is world-space and
  points from `collider1` toward `collider2`, so "the supporter pushes
  up" is one orientation flip plus a dot with the gravity direction
  (threshold −0.5). No gravity, no supports: holding-up is only
  meaningful against a pull. Receipts for all three gates run in
  `tests/tactile.rs`; the sieve and support receipts share one run, as
  the plan's done-conditions asked.
