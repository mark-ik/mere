# Meerkat onto one state, N windows: the migration plan

**Date**: 2026-07-06
**Status**: Plan. The framework half (serval `ServalMultiRunner`) landed
2026-07-06; this is the meerkat consumer migration onto it. No meerkat code
changed yet.
**Related**: [one_state_n_windows_design](../design/2026-07-05_one_state_n_windows_design.md)
(the design this executes step 2's second half + steps 3-4 of),
[movebefore plan](../../../../serval/docs/2026-07-05_movebefore_dom_standard_plan.md)
(S1-S5 landed; `PortableKeyed` is the tile-move mechanism this eventually
consumes), [multi_window_plan](2026-06-10_multi_window_plan.md) (built the
N-runner shape this replaces).

## The finding: the framework is ready, no extension needed

`ServalMultiRunner<State, Logic, V>` takes `Logic: FnMut(&State) -> V`. A
`Box<dyn FnMut(&AppState) -> ShellView>` satisfies that, so each window's
projection is a **boxed closure capturing its projection index**, reading
`&app.windows[i]`. The design doc's `FnMut(&AppState, &WindowLocal) -> V` lens
is sugar over exactly this: one `AppState` that *contains* the per-window
locals, each projection's closure indexing its own.

`ShellView` is already `Box<dyn AnyView<…>>` (erased `V`); the only type change
is `ShellLogic`, from `fn(&ShellState) -> ShellView` to
`Box<dyn Fn(&AppState) -> ShellView>`. Nothing in `ServalMultiRunner` changes.

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
ServalMultiRunner<AppState, Box<dyn Fn(&AppState) -> ShellView>, ShellView>
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
because they share the `Rc`. Per-window `ServalAppRunner`s stay. **Done
when**: the sync/crawl fan-out in `handler_user.rs` is gone, both chips still
update live across two windows in a headed drive, build + `-- gloss
pane_session` green. This seam is what Slice 3 lifts: the `Rc<RefCell<
SharedChrome>>` becomes `AppState.shared`, owned by the multi-runner instead
of shared by `Rc`.

**Slice 1 — the rest of the shared fields.** comms identity/status, sessions,
shellbar_*, physics_paused onto `SharedChrome` the same way; delete their
per-frame mirroring. **Done when**: no `chrome_update` call writes a shared
field; the only per-frame chrome sync left is genuinely per-window.

**Slice 2 — WindowLocal split.** Factor `ShellState` into `WindowLocal` (the
per-window remainder) so the two halves are named types, still owned by the
per-window runner. Pure refactor, no behavior change. **Done when**:
`ShellState = { shared_handle, local: WindowLocal }` and the view reads both.

**Slice 3 — flip to `ServalMultiRunner`.** `Shell` owns
`ServalMultiRunner<AppState, BoxedLogic, ShellView>`; `AppState.shared`
absorbs the `Rc<RefCell<SharedChrome>>` contents; `AppState.windows` holds the
`WindowLocal`s; `WindowView` drops `runner`; the ~30 dispatch/focus/state
sites and `render_chrome_scene` reroute through the projection id; spawn/close
become `push_projection`/`remove_projection`. **Done when**: no
`ServalAppRunner` remains in meerkat, one `update` diffs every window, primary
+ a torn-out leaf both drive correctly in a headed run.

**Slice 4 — source-first rebuild order.** Order `push_projection` / the
rebuild loop so a tear-out's source window rebuilds before its target, so a
future `PortableKeyed` tile move preserves (per the design's ordering caveat).
**Done when**: documented + a test window ordering exists; unblocks the tiles
slice.

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
- **Headed verification is the real gate** and the 13x13-window blocker
  (workspace brief) may still obstruct it; each slice's done condition names a
  drive that is owed if the blocker clears.
- **Keep files under 600 LOC**: `AppState` / `WindowLocal` want their own
  module, not a bloated `window_view/mod.rs`.
