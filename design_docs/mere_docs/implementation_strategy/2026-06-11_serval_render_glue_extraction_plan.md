# Serval Render Glue Extraction Plan

**Date**: 2026-06-11
**Status**: **Done (2026-06-11).** meerkat owns the glue (`crates/meerkat/src/serval_render.rs`);
the `pelt-live` dependency is dropped; serval is untouched. Green (110 meerkat tests).
**Scope**: Move meerkat's render glue (the `ScriptedDom -> netrender::Scene`
assembly it consumes from `pelt-live`) into a meerkat-owned module that calls
`serval-layout` + `paint_list_render` directly, and drop the `pelt-live`
dependency. serval/pelt-live is left untouched; it stays the headless probe.
**Related**: [host_cheap_path_plan](2026-06-10_host_cheap_path_plan.md) (this
removes the shared edit surface that was forcing serval-side detours, unblocking
that plan's C5-remaining and C6 serval items); the archived serval-as-host flip
plan established "keep host-coupling retargetable", which this sharpens.

## Why

meerkat depends on `pelt-live`, a leaf probe in the *serval* workspace (its own
header reads "headless host probe, Stage 1b, no window, no input"). Nothing in
serval consumes it; meerkat is its only consumer. Three problems follow.

1. **Inverted dependency.** mere depends on a serval *port* (a demo leaf) where
   it should depend on serval *components*. meerkat already depends on
   `serval-layout` (the real engine component) directly; the pelt-live glue is
   the odd coupling.
2. **A probe on the product's hot path.** `scene_from_scripted_dom`,
   `hit_test_node`, and the caret + scrollbar overlay assembly run every meerkat
   frame, yet they live in scaffolding that proved the serval spine offline.
3. **A shared edit surface across a repo boundary.** pelt-live's `render.rs` is
   actively extended on the serval side (`range_rects`, `TextRange`,
   `selection_style` landed mid-session) while meerkat consumes it, which forces
   additive-only changes and serval-side detours (the C4 session hit-test was put
   in serval-layout to avoid touching the file). Owning the glue gives meerkat a
   stable contract: serval-layout's generic, tested API.

## Key finding: the engine already lives in shared crates

The glue is thin assembly. Every piece under it is a shared crate meerkat can
call directly:

- cascade / layout / emit / query primitives: `serval-layout` (already a direct
  meerkat dep).
- the paint-list to Scene lowering: `paint_list_render::translate_paint_list`.
  pelt-live only *aliases* this crate as `paint`; the orrery and platen-view
  already lower through `paint_list_render`.
- query types (`Point`, hit-test) from `engine_observables_api`; paint types
  (`DeviceIntSize`) from `paint_list_api`; a11y from `accesskit` (already a dep).

So the extraction moves no engine code and needs **zero serval changes**. It
copies roughly 300 lines of assembly into meerkat and repoints dependencies.

## Surface (what meerkat consumes today)

Seven items, all thin: `scene_from_scripted_dom`, `scene_from_session`,
`scene_from_layout_dom`, `fragments_from_scripted_dom`, `hit_test_node`,
`caret_screen_rect` (test only), and the `TextCursor` type. Plus the internals
they share (`LaidOutDocument`, `push_scrollbars`, `paint_list_from_*`) and
`a11y::accesskit_tree` (the deferred C4c chrome-a11y-from-DOM swap will want it).

## Phases (done-conditions, not dates)

### E1: meerkat `serval_render` module

Copy the glue into `crates/meerkat/src/serval_render.rs` (under the 600-LOC
ceiling), calling serval-layout + paint_list_render directly. Lean on
serval-layout's convenience entry points (`render`, `paint_list_from_layout_dom`)
where they hide taffy/euclid, to keep the new direct-dep set minimal. **Done
when** the module builds and reproduces the seven functions.

### E2: repoint call sites

Swap meerkat's `pelt_live::X` to `crate::serval_render::X` at the consuming sites
(render.rs, input.rs, main.rs, roster.rs, card.rs, pane_session.rs, tests.rs).
Mechanical. **Done when** no `pelt_live::` reference remains in meerkat.

### E3: Cargo swap

Drop `pelt-live`; add `paint_list_render` (mirroring netrender's path dep) and
whichever of {`paint_list_api`, `engine_observables_api`, `taffy`, `euclid`} the
module names directly. The taffy/euclid pins must match serval-layout's (the
experimental taffy pin); most resolve to the already-locked transitive versions.
**Done when** meerkat builds with no `pelt-live` dependency.

### E4: verify

`cargo build` + `cargo test -p meerkat`; the C3 parity fixture follows the glue
into meerkat (asserting session render equals stateless render still holds on the
copy). **Done when** green and the on-screen shell renders unchanged.

## What does not move

pelt-live keeps everything: its `pelt-live-counter` bin and its lib tests
(`cascade_is_deterministic_off_thread_and_concurrent` guards serval thread
safety; the counter end-to-end and the xilem_serval dispatch tests guard the
serval host spine). Those stay serval-side. The duplication of the thin assembly
is the intended decoupling: the heavy, tested logic stays single-source in
serval-layout + paint_list_render; only the ScriptedDom convenience signatures
are copied, and that copy is what insulates the product from probe churn.

## Payoff

- Fixes the inverted dependency (mere depends on a serval *component*, not a
  *port*).
- C5-remaining's emit-from-`LaidOutDocument` seam becomes a meerkat-local
  function, with no pelt-live coordination.
- The C6 serval composition items stop colliding with active serval edits: the
  only shared surface left is serval-layout's stable generic API.

## Progress

- **2026-06-11** — Scoped from the pelt-live dependency question. Surface
  inventoried (seven functions, all consumers meerkat-side); confirmed
  `translate_paint_list` belongs to `paint_list_render`, not pelt-live, so the
  extraction needs no serval edits.
- **2026-06-11** — **Executed, green.** `serval_render.rs` landed. The plan
  expected a verbatim copy needing taffy/euclid; the C3/C4 session-query methods
  made it leaner: every stateless function collapses to a fresh `IncrementalLayout`
  plus a session query (`scene_from_scripted_dom` = `scene_from_session` over a
  fresh session, the exact equivalence the C3 parity fixture proved), so the only
  new direct deps are `paint_list_render` + `paint_list_api` (no taffy / euclid /
  engine_observables_api). `caret_screen_rect` was dropped (test-only; the lib test
  now calls the production `IncrementalLayout::caret_rect`). All six runtime call
  sites repointed; `pelt-live` removed from Cargo. meerkat builds without compiling
  pelt-live; 110 tests green. The one snag: `tests.rs` is a *lib* test, so it could
  not reach the bin-side `serval_render`; resolved by pointing it at the serval
  primitive instead. Behaviour preserved (meerkat only ever rendered the stateless
  panes with `cursor: None`; the chrome caret already rode the session).
