# Workbench Staging Plan (2026-06-09)

**Status**: Active (open tail spun out of the completed card-system plan).
**Spun out of**: `archive_docs/.../2026-06-07_card_system_and_staging_plan.md` (the
two-stage card, focus/activation decoupling, anchored snapshot/live cards, and
cross-session snapshots all shipped; that plan is archived as complete).
**Touches**: `meerkat` (constellation, card input, the top `⊞` commit button),
`platen` (workbench tiles), `orrery` (selection), gloss (latent graphlet surfacing).
**Related**: [`../design/2026-06-07_gloss_navigator_design.md`](../design/2026-06-07_gloss_navigator_design.md)
(the latent-graphlet surface), [`../design/2026-06-07_graph_roster_and_frame_taxonomy.md`](../design/2026-06-07_graph_roster_and_frame_taxonomy.md).

The cards are done: focus is decoupled from activation, the snapshot/live two-stage
card is anchored to its node, and cross-session snapshots render host-side from
durable state. What remains is the **deliberate staging flow** (card item #5): a
tile button that stages a node, a commit that opens the staged set into the
workbench, and the latent staging-order relation surfaced through gloss.

---

## Scope

**Staging state.** A host-owned ordered set `staged: Vec<GraphMemberId>` (order is
meaningful). A live card's **tile button** (top-right, beside the X) toggles its
node in `staged`; staged cards carry a marker (a filled tile glyph). Staging is
host UI state, not graph data.

**Commit.** The top `⊞` button commits: when `staged` is non-empty it opens the
staged nodes into the workbench as tiles **in staging order**, then clears
`staged`. When empty it keeps today's behavior (open the selection's working set).
So `⊞` becomes "commit staged, else open selection."

**Latent staging relation.** On commit, record a relation among the staged set in
staging order, **latent** (no drawn edges, no graph reorganization): one graphlet
among many, surfaced or hidden through gloss + swatches per the surfaced/hidden
edge families, tags, and arrangement rules.
- **chain** — sequential `A→B→C` (the lean default), or
- **bus** — a shared hub linking the set.

---

## Phasing

1. **Staging set + tile button.** `staged` on the host; the live card's tile
   button toggles membership; staged-state affordance on the card.
2. **Commit semantics.** `⊞` becomes "commit staged → workbench tiles in order,
   else open selection."
3. **Latent staging relation + gloss surfacing.** Record the staging-order
   graphlet; surface via gloss/swatches (chain default, bus option).

---

## Open questions

- **Where the latent relation lives.** (a) a gloss-owned graphlet store (keeps it
  out of kernel graph truth) vs (b) a kernel edge family flagged `latent`/hidden
  (reuses the existing hidden-edge machinery + `ArrangementRelation` vocabulary).
  Lean (a) for "doesn't touch graph truth"; (b) reuses real infrastructure. Mark's call.
- **chain vs bus** default (lean chain, staging order; make it a gloss/arrangement
  choice, not a hardcode).
- Whether commit clears `staged` or keeps it for re-commit.
- The tile-button glyph + staged-state affordance on the card.

---

## Progress

- 2026-06-09 — Spun out of the completed card-system plan (cards P1-#4 shipped and
  runtime-confirmed; #5 staging unbuilt). No code yet.
