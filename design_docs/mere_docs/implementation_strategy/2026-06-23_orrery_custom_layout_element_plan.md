# Orrery Custom-Layout Element Plan (cond 1)

**Parked / deferred, 2026-06-23.** Spun out of the
[unified document host plan](2026-06-17_unified_document_host_plan.md) as its Phase-2 cond 1, the
one remaining engine-native piece after Phase 1 + the four pressing slices landed. cond 1 makes the
orrery a real serval **custom-layout element** whose `gyre`-positioned children carry their position
in the layout fragments, so every consumer (overlays, a11y bounds, text selection, hit-test) reads
correct geometry with no transform special-casing. It is **deferred by design**: the interim
transform-aware focus ring + the slice-4 DOM-sourced a11y bounds hold the visible behaviour correct
without it. Un-park only when host-driven transform-setting becomes a perf or correctness problem.

## Why parked (the interim holds)

Today `orrery_element` is a host-positioned `<div>` whose card children are
`position:absolute; transform:translate(gyre.x, gyre.y)`. serval lays them out and the `translate`
shifts only the **paint** (the verified `RepaintOnly` transform path), not the box geometry. The
fragments therefore carry each card at its *pre-transform* slot, and every paint-side consumer has
to add the transform back:

- the focus ring does, via `IncrementalLayout::accumulated_translate` (serval `a2d91ddc`, mere `7181206`);
- the orrery a11y bounds do, via the same `accumulated_translate` (slice 4, 2026-06-23).

So the correctness gap is **closed for the two live consumers**. cond 1 is the structural form that
makes the transform special-casing unnecessary for *all* future consumers (text selection, IME
carets, find-in-page rects inside the orrery), not a fix for a present-day bug.

## Mechanism A (the form) — lifted from the unified plan's cond-1 design

**Mechanism B (absolute `left`/`top`) is rejected.** Setting each card's `left`/`top` from `gyre`
and letting taffy flow them would put the positions in the fragments, but a `left`/`top` change is
layout-tier: every physics frame would relayout, the "orrery freeze" the transform / `RepaintOnly`
path was built to avoid (regression guard, serval `incremental.rs:1232`). B reintroduces it.

**Mechanism A (custom-layout concern) is the form.** Three serval-layout pieces:

1. **A custom-layout mode** for the orrery element, recognized by a marker attribute the way
   external-texture is recognized by `external_texture_key_of` (not a new CSS `display`): its
   children are measured to their natural size by taffy, then placed at host-supplied per-node
   positions rather than flowed.
2. **A per-child position concern:** a `child-node -> (x, y)` map the host supplies into layout each
   frame, analogous to the external-texture key / scroll offsets but per child.
3. **A position-only incremental path:** when only the position concern changes (DOM, styles, child
   sizes unchanged), update the children's fragment locations without re-measuring, so a gyre frame
   stays cheap. This is the layout-side analog of the `RepaintOnly` transform path, and the piece
   that keeps A as cheap as the transform it replaces; without it A is just B.

**Host migration (meerkat).** `orrery_element` marks the container custom-layout and drops the
per-card `transform: translate`; `render.rs` feeds gyre's per-node positions into the concern each
frame instead of into transforms. `accumulated_translate` then returns 0 for the cards (their
fragments carry the real positions) so the interim ring + a11y fixes become harmless no-ops.

## Scope + engine ask

A real multi-subsystem serval-layout feature: a box-tree layout mode + the per-child position concern
plumbing + a new incremental damage class / path. It warrants its own focused effort and is the
engine-side work the unified plan said "belongs in serval's own
`docs/2026-05-27_serval_as_host_xilem_serval_plan.md`"; this plan is the meerkat-side consumer view +
the documented serval ask. Not a tail change; the interim ring + a11y fixes hold until it is built.

## Trigger to un-park

Revisit when any of: (a) a new orrery-interior consumer needs correct geometry and the per-consumer
`accumulated_translate` add-back becomes error-prone or is forgotten (e.g. text selection / IME
carets / find rects inside the orrery); (b) host-driven per-frame transform-setting shows up as a
perf cost; (c) the secondary-orreries / side-by-side work (tearout) wants the cleaner element form.

## Cross-references

- [unified document host plan](2026-06-17_unified_document_host_plan.md) — origin (Phase 2 cond 1);
  its closing entry maps this and the other Phase-2-tail threads to their homes.
- [node representation / arrangement plan](2026-06-18_node_representation_arrangement_plan.md) — owns
  the node body / sprite whose geometry cond 1 makes fragment-correct (the card is one sprite).
- [tearout composability plan](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md) — the secondary-orreries
  consumer that may want the element form.

## Progress

- **2026-06-23 (parked).** Extracted from the unified-document-host plan on its core-complete
  closeout. No code; deferred by design until a trigger above fires. The Mechanism-A design + the
  rejected Mechanism B are the load-bearing record carried forward.
