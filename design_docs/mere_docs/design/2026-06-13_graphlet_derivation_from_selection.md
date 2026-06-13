# Graphlet Derivation from Selection

A multi-selection is a question: *what structure do these nodes already share?* This
is the UX that answers it — select nodes, reveal the latent edges among them,
read the shape they form, and optionally crystallize it into a graphlet. It is
the read-side sibling of manual edge creation (`assert_selected_relation`): one
*asserts* a new relation, this *reveals and derives* from the relations that are
already there.

The satisfying part is how much substrate already exists. The forme graphlet
model has the nine canonical shapes (Ego, Corridor, Component, Loop, Frontier,
Facet, Bridge, WorkbenchCorrespondence, Session), the projection sourcing
(`SelectionOverride { seed_nodes }` is the highest-precedence binding, beating
`GraphViewOverride` beating `GraphDefault` — a selection *is* a projection
override), the `EdgeProjectionSpec` for which edges count, and the binding +
reconciliation machinery (session vs linked-to-truth, member deltas, the four
reconcile choices). The one real gap is a **shape classifier** (induced subgraph
over a chosen edge projection → which of the nine shapes, ranked by fit) plus the
canvas reveal/derive/crystallize choreography. Everything downstream of "here is a
shape" already exists.

## The interaction

1. **Select, and the wiring reveals itself.** On a multi-selection the canvas dims
   everything except the selection, then fades in the latent edges *among* the
   selected nodes that aren't normally drawn — the semantic, containment,
   traversal, and provenance relations they already share. Edges leaving the
   selection stay ghosted at the boundary. Each revealed edge takes its family's
   colour/style (the `RelationKind` tag already carries this), with a small family
   legend. This is the literal answer to "reveal edges": the selection lights up
   the structure it already has.

2. **A derivation strip reads the shape.** Anchored to the selection (or the radial
   menu's outer ring), a strip of ranked chips names what the revealed edges form:
   "Corridor · A→B→C→D (traversal)", "Ego · hub C, radius 1", "Component ·
   connected via Cites", "Bridge · joins {P-cluster} and {Q-cluster}". Hovering a
   chip highlights its sub-structure; clicking crystallizes. With no clean match:
   "Loose set · 3 of 5 connected", pointing at the frontier move.

3. **The projection toggle is the heart.** A family filter on the strip (Semantic /
   Traversal / Containment / Provenance / Imported / Arrangement) *is* the
   `EdgeProjectionSpec`. Flip Traversal off and Cites on, and both the revealed
   edges and the derived shape change live. The same five nodes are a Corridor
   under navigation history and a Component under citations. This is what makes it
   a tool, not a guess: the shape is a function of the projection you pick, and you
   steer it.

4. **Frontier ghosts complete the shape.** Beside the chips, the canvas ghosts the
   one-hop frontier — nodes just outside the selection that would close a clean
   shape if included ("add these two to complete the Ego"). The difference between
   classifying what you selected and deriving the graphlet that explains it.

5. **Crystallize, with a choice of permanence.** Clicking a chip binds the
   selection as a graphlet:
   - **Session** (default): groups them now as a tile-group or astroid, no link to
     graph truth. Ephemeral, like a sticky grouping.
   - **Linked**: the roster tracks the derivation rule. A Linked Ego(C) grows as C
     gains neighbours, and reconciliation proposes the delta ("3 new nodes match
     this Ego, add them?") with the existing apply / keep-as-session / fork /
     cancel choices.
   - **Astroid**: for a hub shape, offer to collapse it to the astroid glyph (a
     single node that expands) — the hub-collapse vocabulary, earned by Ego
     detection.

## Why it stays cheap and safe

Steps 1-4 are pure read-side projection over graph truth. Nothing is committed
while you explore, so you can sweep selections and flip projections freely. Only
crystallize writes, and it writes exactly one thing: a `GraphletRef` plus either
an Arrangement edge (session, tile-group) or a Containment edge (linked,
CollectionMember). The graph changes only when you commit a graphlet. This is the
same discipline as the edge-creation work — the canvas is a read until an explicit
gesture writes.

## The build

Small and well-aimed:

1. **The shape classifier** — the nine detectors over an induced subgraph, ranked
   by fit and edge strength. The genuinely new logic.
2. **The canvas choreography** — dim/reveal, the chip strip, the projection
   toggle, frontier ghosts, crystallize. Composes the existing projection,
   binding, and reconciliation.

The projection sourcing, the shapes, the binding, and the reconciliation are
already built.

## Decisions

- **Default projection for a fresh selection: all families, but auto-rank so the
  dominant shape is pre-highlighted.** Honest (you see every relation you actually
  have) without making you do the work (the best shape is offered, one toggle from
  refining). Picking *for* the user on first impression hides structure; showing
  everything but pointing at the winner does not. (2026-06-13.)

## Open questions

- **Strength for ranking.** The classifier ranks by fit and edge strength, but
  there is no unified strength number yet (traversal has counts, semantic has
  decay, arrangement has durability). See the edge-spine "strength projection"
  thread in the [edge system audit](../research/2026-06-13_edge_system_audit.md);
  per the configurability rule it likely wants to be a setting, not a constant.
- **Containment vs Arrangement on crystallize.** Session → Arrangement
  (tile-group), Linked → Containment (CollectionMember). The same "group these"
  gesture writes a different family by permanence; confirm that mapping is the
  sharp rule (also a spine thread).
