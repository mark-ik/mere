# Meerkat onto one state, N windows: the migration plan

**Date**: 2026-07-06
**Status**: **COMPLETE 2026-07-07 — archived** (to `archive_docs/2026-07-07_one_state_migration/`). All slices landed + verified headless (302 meerkat + 89 xilem-serval tests) and headed (the mk-harness primary+leaf drive). The framework half (genet `GenetMultiRunner`) landed
2026-07-06. Consumer migration: **Slices 0-3 landed** (0-2 on 2026-07-06; **Slice 3 —
the atomic multi-runner flip — on 2026-07-07**: `cargo check -p meerkat --all-targets`
0 errors, `cargo test -p meerkat` 302 tests pass (72 + 230), `xilem-serval` 89 tests
pass). Slice 1 was reassessed and trimmed (see below). **Slice 4** (source-first
rebuild order) holds by construction — see its entry. Workspace toolchain bumped to
**1.96.0** this session (a dep in the graph now needs rustc >= 1.95;
`rust-toolchain.toml` pinned to 1.96).
**Related**: [one_state_n_windows_design](../design/2026-07-05_one_state_n_windows_design.md)
(the design this executes step 2's second half + steps 3-4 of),
[movebefore plan](../../../../genet/docs/2026-07-05_movebefore_dom_standard_plan.md)
(S1-S5 landed; `PortableKeyed` is the tile-move mechanism this eventually
consumes), [multi_window_plan](2026-06-10_multi_window_plan.md) (built the
N-runner shape this replaces).

## The finding: the framework is ready, no extension needed

`GenetMultiRunner<State, Logic, V>` takes `Logic: FnMut(&State) -> V`. A
`Box<dyn FnMut(&AppState) -> ShellView>` satisfies that, so each window's
projection is a **boxed closure capturing its projection index**, reading
`&app.windows[i]`. The design doc's `FnMut(&AppState, &WindowLocal) -> V` lens
is sugar over exactly this: one `AppState` that *contains* the per-window
locals, each projection's closure indexing its own.

`ShellView` is already `Box<dyn AnyView<…>>` (erased `V`); the only type change
is `ShellLogic`, from `fn(&ShellState) -> ShellView` to
`Box<dyn Fn(&AppState) -> ShellView>`. Nothing in `GenetMultiRunner` changes.

Consequence for message-time mutation: xilem threads **one** `&mut State`
through `View::message`, and shared truth is one object across N windows, so it
cannot live inside N per-window states. It lives in the one `AppState` the
multi-runner owns; a handler in any window mutates `AppState` (reaching its own
window's local via a `lens`, or the shared part directly), and every
projection rebuilds against the new truth. That *is* the synced-panels
behavior: no mirror, no fan-out.

## Target architecture

```rust
struct AppState {
    shared: SharedChrome,        // sync, crawl, comms identity/status,
                                 // sessions, shellbar_*, physics_paused
    windows: Vec<WindowLocal>,   // ≈ today's ShellState, minus the shared fields
}
// WindowLocal: chrome-local (omnibar/palette/find/knot buffers, context menu,
// per-window history + toolbar + content_location), orrery snapshot, roster,
// panes, gloss, settings, object_card_keys.

// Shell owns:
GenetMultiRunner<AppState, Box<dyn Fn(&AppState) -> ShellView>, ShellView>
// per window i, pushed at spawn:
//   |app| shell_view(&app.shared, &app.windows[i])
```

`WindowView` loses its `runner` field. Render and input reach the multi-runner
by `ProjectionId`: `render_chrome_scene` reads the projection's retained layout
session; the ~30 `self.view.runner.{dispatch_*,focus,set_focus,state}` sites
become `self.multi.{…}(projection_id, …)`. The `WindowCtx` borrow bundle,
which today bundles `view.runner`, instead carries the window's `ProjectionId`
plus a borrow of the shared multi-runner.

## The Chrome carve (shared vs window-local)

Fanned-out today (→ `SharedChrome`): `sync`, `crawl`, `comms` identity/status,
`sessions`, `sessions_overflow_open`? (no — per-window), `shellbar_panes`,
`shellbar_edge`, `shellbar_hidden`, `physics_paused`. These are the fields the
`handler_user.rs` fan-out loop and `build_window_view_for` spawn-seeding
copy into every window because they are the same for all.

Stays window-local: `toolbar`, `omnibar`, `history`, `suggest*`, `palette*`,
`find*`, `context_menu`, `tear_ghost`, `branch_label`, `content_location`
(windows navigate independently), the comms compose buffers, the knot editor,
`slim`, `sessions_overflow_open`, and every `pending_*` intent (drained
per-window by the host).

`content_location` and `toolbar` look shared but are not: a leaf window shows a
different page than the primary. Only the genuinely-identical-across-windows
fields move.

## Slices (done conditions, not order-of-magnitude)

**Slice 0 — SharedChrome seam, fan-out deleted (strangler, keeps per-window
runners).** Give `ShellState` a field `shared: Rc<RefCell<SharedChrome>>`;
every window's `ShellState` holds a **clone of the same `Rc`** (all point at
one `SharedChrome` owned by `SharedState`). `ShellLogic` stays `fn(&ShellState)
-> ShellView` — no closure change, since the view reads `s.shared.borrow()`
directly. Wrinkle (confirmed in `views/mod.rs`): the toolbar view reads
`c.crawl` and the Steward/Apparatus panes read `c.sync` off the `Chrome` lens,
so those ~3 view functions take the shared value as a **param** (read from
`s.shared` in `shell_view` and threaded down) rather than off `Chrome`; move
`sync` + `crawl` out of `Chrome`. The fan-out loop + spawn seeding collapse to
a single `shell.shared_chrome.borrow_mut().sync = …` — every window sees it
because they share the `Rc`. Per-window `GenetAppRunner`s stay. **Done
when**: the sync/crawl fan-out in `handler_user.rs` is gone, both chips still
update live across two windows in a headed drive, build + `-- gloss
pane_session` green. This seam is what Slice 3 lifts: the `Rc<RefCell<
SharedChrome>>` becomes `AppState.shared`, owned by the multi-runner instead
of shared by `Rc`.

**Slice 1 — reassessed and trimmed (LANDED 2026-07-06).** Recon against the code
showed the plan's field list (comms, sessions, `shellbar_*`, physics_paused) is
mostly shared+local *mixes*, not clean shared truth, so most of it does **not**
belong in the shared cell:

- `sessions`: the chip data is shared, but the "active" highlight is
  `Some(id) == focused` per each window's own `focused_graph`, so the rendered list
  differs per window.
- `comms`: the conversation inbox/thread/identity are shared (actor-drained), but the
  dock open/focus and the compose buffers are per-window.
- `physics_paused`: read from `orrery().physics_paused()`, which is per-Orrery (per
  focused graph), so a leaf on a different graph legitimately differs.
- `shellbar_panes`: computed per-window from each window's own frame layout.

Their shared/local split is the lens at Slice 3 (`|app| shell_view(&app.shared,
&app.windows[i])`), where each window derives its own projection (active highlight,
dock, pause) from shared truth — not by cramming a mixed field into a shared cell. The
one genuinely-clean action was **deleting the vestigial `Chrome.shellbar_edge` mirror**:
the shellbar's dock geometry (flex direction) is applied host-side each render via
`set_attribute` from `presentation.shellbar_edge`, so the view never read the `Chrome`
copy and its only effect was a no-op chrome rebuild on redock. `shellbar_hidden` stays a
per-window mirror for now (its rebuild is triggered by per-window `setup_and_sync_chrome`
change-detection using the `Chrome` copy as the last-rendered marker; relocating that
needs the Shell-level nudge that is S3's job). **Landed**: `Chrome.shellbar_edge` +
its setup sync removed; `meerkat --all-targets` green. **Deferred to S3**: the comms
fan-out, and the sessions / physics / shellbar_hidden projections.

**Slice 2 — WindowLocal split (LANDED 2026-07-06).** `ShellState` factored into
`ShellState { shared: Rc<RefCell<SharedChrome>>, local: WindowLocal }`, where
`WindowLocal` holds the per-window remainder (chrome, orrery snapshot, roster, the
folded list panes + gloss lenses, settings tiles, drained intents). Pure refactor: ~40
accessor sites rewritten (`s.X` → `s.local.X`) across `window_view/{mod,views,
state_impl}.rs` plus the direct external reads (`frame_a11y_panes`, `shell_access`,
`shell_new`, `agent_harness/tests`, `roster_action_tests`); the shared cell reads stay
`s.shared`. **Landed**: `ShellState = { shared, local: WindowLocal }`, the view reads
both, `meerkat --all-targets` green. `WindowLocal` is the type Slice 3's multi-runner
holds one-per-window in `AppState.windows`.

**Slice 3 — flip to `GenetMultiRunner`.** `Shell` owns
`GenetMultiRunner<AppState, BoxedLogic, ShellView>`; `AppState.shared`
absorbs the `Rc<RefCell<SharedChrome>>` contents; `AppState.windows` holds the
`WindowLocal`s; `WindowView` drops `runner`; the dispatch/focus/state sites
reroute through the projection id; spawn/close become
`push_projection`/`remove_projection`. **Scope correction (2026-07-06)**: this is
~250 sites across 30+ files, **not** the ~30 first estimated — removing
`WindowView.runner` forces the ~25 view-state accessor methods off `WindowView`,
changing their **~215** callers, on top of the **73** `view.runner` dispatch
sites; the lens routing rules out a cheap `Rc` facade. A dedicated execution
sub-plan decomposes it into green checkpoints (genet `update_local` → accessor
relocation while runner-backed → the atomic structural flip):
[2026-07-06_slice3_multirunner_flip_subplan](2026-07-06_slice3_multirunner_flip_subplan.md).
**Done when**: no `GenetAppRunner` remains in meerkat, one `update` diffs every
window, and a primary plus a torn-out leaf both drive correctly in a headed run.
**Landed 2026-07-07**: `Shell.multi: GenetMultiRunner<AppState, BoxedLogic,
ShellView>`; `WindowView.projection_id` replaced `runner`; the 29 accessor bodies +
dispatch/focus sites route through the projection id; spawn/close →
`push_projection`/`remove_projection` (index-aligned dual collections); the S0
`Rc<RefCell<SharedChrome>>` collapsed into owned `AppState.shared`;
`ShellState`/`ShellRunner`/`ShellLogic`/`shell_runner` deleted; genet gained
`ProjectionId(pub usize)`. Build + 302 meerkat tests + 89 `xilem-serval` tests green.
**Headed check done 2026-07-07**: on the rebuilt exe, the primary + a Ctrl+Shift+N leaf both
render from the one `GenetMultiRunner` (leaf slim, primary full, both on the shared graph
through their own cameras), no crash — driven by the unified
`testing/mere/scripts/mk-harness.ps1` after fixing the partitioned-mode self-capture in
`render/paint.rs`. Shots: `testing/mere/images/s3c-{1-primary,3-both}.png`. All four slices
are now verified headless (302 tests) and headed.

**Slice 4 — source-first rebuild order (holds by construction).** A tear-out appends
the target projection after its source exists and `ProjectionId`s never reuse, so a
target always has a higher index than its source; `GenetMultiRunner` rebuilds in
insertion order, so the source rebuilds before the target with **no code change**. The
dedicated window-order test + the actual `PortableKeyed` cross-window survival are
deferred to the forest-dom slice (until then a cross-window tile move degrades to
fresh-build regardless). See the sub-plan's S3d note.

Steps 3 (forest dom) and 4 (portable tiles) of the design doc follow, as their
own plans, once the runner flip lands.

## Risks

- **The `WindowCtx` borrow re-architecture (Slice 3) is the load-bearing
  risk.** ~30 sites reach `self.view.runner`; they must move to a
  `(multi_runner, projection_id)` shape without the borrow checker fighting the
  disjoint `shared` / `windows[i]` split. Slice 2 names the halves first to
  de-risk it.
- **All-projections-rebuild-per-local-edit** (an omnibar edit in window 2
  rebuilds window 1). Correct (window 1's unchanged view diffs to zero DOM
  mutations) and cheap, but if it ever bites, add a targeted
  `update_local(id, …)` that rebuilds one projection. Not built up front.
- **Headed verification** (was the real gate; the 13x13-window blocker is handled by
  mk-harness's `EnumWindows` largest-for-pid finder) — **cleared 2026-07-07**: the headed
  primary-plus-leaf drive ran clean on the unified harness.
- **Keep files under 600 LOC**: `AppState` / `WindowLocal` want their own
  module, not a bloated `window_view/mod.rs`.
