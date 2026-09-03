# Keyed View Sequence Plan — identity-stable list diffing for xilem_serval

**Date**: 2026-07-02
**Status**: Implemented 2026-07-02. `Keyed<K, V>` landed in `xilem_serval` and is wired into the orrery, roster, list-pane, and gloss call sites. With the gnode-pool pivot, the orrery use is temporary; the durable scope is the row/list consumers.
**Related**: [ui_polish_plan](../2026-09-02_retired_plans/2026-07-01_ui_polish_plan.md) (finding 5, the paint-list-emission cost this plan does *not* fix), `repos/genet/components/xilem-core/src/` (`view_sequence.rs`, `view_sequences/impl_vec.rs`, `element_splice.rs`), `repos/genet/components/xilem-serval/`, `crates/meerkat/src/render/orrery_scene.rs` (`orrery_element`'s gnode list, the original motivating call site).

Spun out of a conversation about why an orrery gnode's DOM position is synced from gyre every frame rather than "the DOM object just having gyre properties" (a node/card architecture question). That thread surfaced a real, separate gap: the list of gnodes fed to `orrery_element` diffs **positionally**, not by identity.

## The gap, verified in code

`xilem-core`'s `ViewSequence` impl for `Vec<Seq>` ([`view_sequences/impl_vec.rs`](../../../../genet/components/xilem-core/src/view_sequences/impl_vec.rs)) assigns each slot's identity as `(index, generation)` — a packed `ViewId` zipped old-vs-new **by position** (`self.iter().zip(prev).zip(&mut seq_state.inner_states).enumerate()`). There is no keyed or map-backed `ViewSequence` anywhere in the crate (checked for `Keyed<K,V>`, a `HashMap`/`BTreeMap` impl — none exists). This matches real upstream Xilem's own `Vec<V>` behavior (the doc comment's "persistent allocation proportional to the longest Vec ever provided" caveat is verbatim the same design), so this is not a genet-specific oversight to report upstream against — it's the general Xilem-family tradeoff, and fixing it here is additive, not a bug fix.

**Consequence**: whenever a `Vec<ShellView>` (or `Vec<RosterView>`/`Vec<ListView>`/`Vec<GlossMinimapView>`) built by `.iter().map(view_fn).collect()` gains or loses a member anywhere except the tail, every element from that point on is diffed against the wrong previous occupant of its slot — forcing a rebuild (or worse, silently reusing retained state across what should be two unrelated elements) for items that didn't actually change, just shifted index.

**This is not a rare case for the orrery.** `Orrery::graph().nodes()` iterates petgraph's `node_indices()` (insertion-order stable), and the `filter_map` in `render_orrery_scene` (scope filter + on-screen bounds check) preserves relative order — so the *set* of surviving gnodes changes (a node pans on/off screen, gets scoped in/out) in a stable relative order, but insertion/removal at any position except the tail is exactly the routine case (panning the camera moves things in and out of view at arbitrary screen positions, unrelated to graph insertion order).

**Not orrery-only.** The same `.iter().map(..).collect::<Vec<XView>>()` shape recurs across `roster_view_graphlets.rs`, `roster_view_links.rs`, `roster_facet_view.rs`, `list_pane.rs`, `gloss_view.rs`'s minimap — every filtered/sorted list-of-rows in the shell document has the identical exposure.

## What this fix does *not* solve

The dominant perf complaint (`chrome_us` 100-145ms/frame, [ui_polish_plan finding 5](../2026-09-02_retired_plans/2026-07-01_ui_polish_plan.md)) is genet-layout's paint-list emission re-walking the *whole* box tree regardless of mutation count — 16 mutations cost the same as 203. Reducing spurious view-diff-layer rebuilds (this plan) does not touch that: paint-list emission is insensitive to how many elements structurally changed either way. This plan is a **correctness + view-diff-layer efficiency** fix (fewer wrong-element reuses, fewer needless `teardown`/`build` calls and their layout side-effects), not a fix for the frame-time complaint. Keep them as separate line items.

There is a second-order interaction worth flagging, not resolving here: `ElementSplice` ([`element_splice.rs`](../../../../genet/components/xilem-core/src/element_splice.rs)) exposes `insert` / `mutate` / `skip` / `delete` — **no move/reorder primitive**. So a keyed impl can preserve retained state across membership changes that keep the survivors in the same relative order, but it cannot literally reposition an existing DOM node to an earlier/later slot. Arbitrary reorder either needs delete+reinsert (a structural DOM change, likely off the RepaintOnly fast path, and not state-preserving through today's public trait) or a real move primitive. Whether that structural churn matters is unknown until finding 5's engine fix lands and per-mutation cost is actually visible again; premature to add a move primitive before that.

## Scope: no xilem-core changes required

`ViewSequence`, `ElementSplice`, and `AppendVec` are all `pub`, and `ViewSequence` is implementable for any local type without touching the defining crate (Rust's orphan rule is satisfied by a new wrapper type, not a new impl of a foreign type). So this can ship as a new type + `impl ViewSequence for Keyed<K, V>` with **no xilem-core edit**. Recommended home: **`xilem_serval`** (a sibling module to `tags.rs`/`key.rs`), since the roster/gloss/list-pane call sites above are equally exposed and equally in-scope to migrate later. That is still a `repos/genet` edit; the meerkat-only alternative avoids a genet edit but would have to be duplicated or re-exported before roster/gloss/list-pane can share it.

## Phases

- **P1 — `Keyed<K, V>` primitive.** Done 2026-07-02. `Keyed<K, V>` now lives in `xilem_serval` as a keyed `ViewSequence` wrapper with unique-key enforcement and retained-state tracking by key rather than slot.

  V1 should be scoped to **insert/delete with preserved relative order**: the orrery culling case, roster filtering, and append/remove row churn. With the current `ElementSplice` cursor (`insert` / `mutate` / `skip` / `delete`, no jump/move), an arbitrary reorder cannot both keep an existing DOM child and mutate it at its new earlier slot. For a moved key, V1 should either treat it as teardown+build or reject/log that case; preserving retained state across arbitrary reorder belongs with P4's move primitive, not with the first keyed wrapper.
- **P2 — First consumer: the orrery gnode list.** Done 2026-07-02 as the first bridge. The gnode list now uses `Keyed<GraphMemberId, _>`. This consumer is temporary and is expected to disappear if the gnode-pool plan lands.
- **P3 — Roll out to the other call sites.** Done 2026-07-02 for roster tables, list panes, and gloss rows/minimap. These remain the durable scope even if the orrery consumer is retired.
- **P4 (deferred, gated on finding-5 data) — `ElementSplice` move primitive.** Only worth scoping once genet-layout's paint-list cost is fixed and delete+reinsert's actual per-reorder cost is separately visible and non-trivial. Not started; not a done-condition of this plan.

## Open questions

- **OQ-1**: `Vec<(K,V)>` linear key lookup vs a small hash index — at orrery/roster row counts (tens, maybe low hundreds of nodes) a linear scan per rebuild is probably fine; profile before reaching for a hash map.
- **OQ-2**: whether `Keyed<K,V>` should require unique keys (panic/log on duplicate) or tolerate collisions by falling back to positional behavior for the colliding subset.
