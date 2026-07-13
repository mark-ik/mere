# Slice 3 execution sub-plan: the multi-runner flip

**Date**: 2026-07-06
**Status**: **COMPLETE 2026-07-07 — archived** (with the parent plan, to `archive_docs/2026-07-07_one_state_migration/`). Execution sub-plan for Slice 3 of the meerkat one-state migration. Written
before touching code because the flip is far larger than the parent plan's estimate
(see the scope finding below). Grounded in a code survey this session, not doc-to-doc.
**Parent**: [meerkat_one_state_migration_plan](2026-07-06_meerkat_one_state_migration_plan.md)
(S0-S2 landed; this executes its Slice 3).
**Framework**: `genet/components/xilem-serval/src/multi.rs` (`GenetMultiRunner`, the
target runner) and `runner.rs` (`RunnerTree`, per-projection guts).

## The scope finding (why this is not a quick slice)

The parent plan estimated Slice 3 at "~30 `view.runner` reroutes". The real surface,
measured this session:

- **73** `view.runner.*` dispatch/focus/dom sites (40 non-test) across the input,
  a11y, nav, and render paths.
- **~215** `view.chrome()` / `view.set_orrery()` / pane+gloss setter call sites across
  **32 files** — because `WindowView` owns its view-state *through* its runner
  (`state_impl.rs`'s ~25 accessor methods do `self.runner.state().local.X`), so
  removing the runner forces those accessors to relocate, changing every caller.

There is **no cheap facade.** Holding each `WindowLocal` in an `Rc<RefCell<>>` (the S0
`SharedChrome` pattern) to keep the accessors working on `&WindowView` breaks the
per-window **lens routing**: `shell_view`'s sub-state lenses (`lens(make_roster,
to_roster)` with `to_roster: &mut ShellState -> &mut RosterState`) require the
per-window state to live *inside* the runner's `State`. That is exactly why the design
specs `AppState.windows: Vec<WindowLocal>` — the lens becomes `|app| &mut
app.windows[i].roster`. So the accessor relocation is unavoidable.

The flip is atomic in state ownership (N per-window `GenetAppRunner`s become one
`GenetMultiRunner`), so it cannot be kept green in a single pass. The sub-slices below
push the bulk of the churn (the 215 accessor callers) into a **green** step so the
atomic core (S3c) is small.

## Target architecture

```rust
// meerkat (bin)
struct AppState {
    shared: SharedChrome,            // owned now, not Rc<RefCell<>> — the flip absorbs S0's cell
    windows: Vec<WindowLocal>,       // one per OS window; the lens routes into windows[i]
}
type BoxedLogic = Box<dyn FnMut(&AppState) -> ShellView>;
// Shell owns:
GenetMultiRunner<AppState, BoxedLogic, ShellView>
// projection i pushed at spawn (captures its index):
//   Box::new(move |app: &AppState| shell_view(&app.shared, app, i))
```

- `WindowView` **drops `runner`**, **gains `projection_id: ProjectionId`**. It keeps its
  `dom` handle (the projection renders into the same `Rc<RefCell<ScriptedDom>>`), its
  `chrome_session` (per-window incremental layout), and all host bookkeeping (surface,
  caches, drags, workbench, gnode_pool).
- `WindowCtx` gains `multi: &mut GenetMultiRunner<AppState>`. Borrow model is clean:
  `view` (from `Shell.windows`) and `multi` (from `Shell.multi`) are disjoint `Shell`
  fields, so `WindowCtx { view: &mut WindowView, multi: &mut GenetMultiRunner<...>,
  shared: &mut SharedState, ... }` borrows both mutably (like today's bundle in
  `shell_access.rs`).
- `shell_view` signature becomes `shell_view(shared: &SharedChrome, app: &AppState, i:
  usize) -> ShellView`; its lenses capture `i`: `move |app: &mut AppState| &mut
  app.windows[i].roster`. `lens`'s `Fn` bound already accepts capturing closures
  (verified in `xilem-core/src/views/lens.rs`; S0 relied on the same for the crawl
  closure). The crawl chip reads `app.shared.crawl` directly (no `.borrow()`; the Rc
  is gone).

**Render is largely unaffected**: `render_chrome_scene` (`render/cards.rs`) drains
mutations from `self.view.dom` and lays out via `self.view.chrome_session`, both of
which stay on `WindowView`. The multi-runner's rebuild emits into the projection's dom,
which *is* `view.dom` (same handle). So render keeps draining `view.dom` as today.

## Per-window vs shared rebuilds (the `update_local` addition)

`GenetMultiRunner::update(f)` rebuilds **every** projection. The per-window setters
(`set_orrery`, `set_roster`, `set_list_pane`, the gloss setters — called per frame per
window) must rebuild **only their window**, or a single-window snapshot update churns
N windows every frame. So genet gains:

```rust
// multi.rs
pub fn update_local(&mut self, id: ProjectionId, f: impl FnOnce(&mut State)) {
    f(&mut self.state);
    let Self { state, projections, .. } = self;
    if let Some(Some(p)) = projections.get_mut(id.0) {
        p.tree.rebuild(&mut p.logic, state);
    }
}
```

Setters that touch `app.windows[pid].X` use `update_local(pid, …)`; a shared-chrome
change (sync/crawl chip) uses `update(…)` so every window's next diff reflects it. This
also *replaces* S0's per-window `refresh_shared_chrome` nudge loop: one `multi.update`
does it.

## Sub-slices (green checkpoints)

**S3a — genet `update_local` (isolated, green).** Add the method above to `multi.rs`;
a unit test in `xilem-serval/src/tests.rs::multi` (mutate state, assert only the target
projection's dom changed). **Done when**: `cargo test -p xilem-serval` green. No meerkat
change yet.

**S3b — accessor relocation to `WindowCtx`, still runner-backed (green).** Add a
`WindowCtx` accessor for each of the ~25 `WindowView` view-state methods, delegating for
now (`fn chrome(&self) -> &Chrome { self.view.chrome() }`). Migrate the ~215 call sites:
`self.view.chrome()` → `self.chrome()` inside `WindowCtx` methods (the common case);
audit the bare callers (`app.view().chrome()` in tests, Shell-level methods) and leave
them on the `WindowView` accessors, which stay. This is the big churn, done **green** by
delegation, so the atomic core (S3c) changes only the ~25 accessor *bodies*, not 215
callers. **Done when**: `meerkat --all-targets` + full test suite green; the only
remaining `self.view.chrome()`-style calls are the audited bare callers.

**S3c — the structural flip (atomic core, now small).** Define `AppState`; give `Shell`
the `GenetMultiRunner` and give `WindowView` a `projection_id` (drop `runner`);
`shell_view` becomes the index-capturing projection logic; the ~25 `WindowCtx` accessor
bodies switch from `self.view.runner.state().local.X` to `self.multi.state().windows[
self.view.projection_id].X` (reads) and `self.multi.update_local(pid, |app| app.windows[
pid].X = …)` (writes); the **73** dispatch/focus sites reroute
`self.view.runner.dispatch_*` → `self.multi.dispatch_*(pid, …)`; spawn → `push_projection`,
close → `remove_projection`, boot (`pending_view` → primary) pushes the primary
projection; the bare-WindowView callers reroute to `Shell.multi`; `SharedChrome`'s
`Rc<RefCell<>>` collapses into owned `AppState.shared`; `shell_runner` + the per-window
`ShellRunner` type are deleted. **Done when**: no `GenetAppRunner` remains in meerkat;
one `multi.update` diffs every window; build + full test suite green; a headed run drives
primary + a torn-out leaf. **DONE 2026-07-07** — build + 302 tests green; the headed drive
(mk-harness) rendered primary + slim leaf from the one runner, no crash.

**S3d — source-first rebuild order.** Order the projection vector / rebuild loop so a
tear-out's source window rebuilds before its target (design step 2 ordering caveat,
`multi.rs` module docs), unblocking the later `PortableKeyed` tile move. **Done when**:
documented + a window-order test. **HOLDS BY CONSTRUCTION 2026-07-07** — spawn appends the
target after its source and ids never reuse, so source-before-target holds with no code
change; the dedicated test + the `PortableKeyed` payoff defer to the forest-dom slice.

## Risks / watch-items

- **The bare-WindowView accessor callers** (tests + any Shell-level method holding a
  `WindowView` without a `WindowCtx`) are the S3b/S3c seam that needs a real audit; they
  cannot use `WindowCtx` methods and must reach `Shell.multi` in S3c.
- **All-projections-rebuild on a shared edit** is correct (an unchanged window diffs to
  zero mutations) but is N x the diff work; `update_local` confines the per-window
  setters, and single-window sessions (the common case) are N=1.
- **The `chrome_session` / dom pairing**: each `WindowView` keeps the dom the multi-runner's
  projection renders into. `push_projection(dom, logic)` must be handed `view.dom` so the
  projection and the window agree on the target. Verify the projection's dom handle and
  `view.dom` stay the same `Rc` across spawn.
- **`ShellLogic` is currently a bare `fn`**; `BoxedLogic` is `Box<dyn FnMut>`. The parent
  plan already confirmed the multi-runner accepts a boxed per-window closure, so no
  `GenetMultiRunner` change beyond `update_local`.
- **Keep files < 600 LOC**: `AppState` / the projection wiring want their own module, not
  a bloated `window_view/mod.rs` or `shell_new.rs`.

## Progress

**S3a landed 2026-07-06** — `GenetMultiRunner::update_local(id, f)` + a
`update_local_rebuilds_only_that_projection` test; `xilem-serval` 88 tests green (1.95.0).
**S3b landed 2026-07-07** — the `WindowCtx` delegating accessors (`window_view/
ctx_accessors.rs`) + 169 `self.view.X` → `self.X` caller sites migrated; `meerkat
--all-targets` green.

**S3c landed 2026-07-07 — the atomic flip is done and verified.** `AppState { shared:
SharedChrome, windows: Vec<WindowLocal> }` (new `window_view/projection.rs`) held by
`Shell.multi: GenetMultiRunner<AppState, BoxedLogic, ShellView>`; `WindowView` dropped
`runner`, gained `projection_id: ProjectionId`; `WindowCtx` gained `multi`. `shell_view`
became the top-level per-window lens `lens(|wl| window_local_view(wl, &crawl), |app| &mut
app.windows[i])` — so the inner builders operate on one `WindowLocal` and the only
`i`-capturing site is that one lens (no `i` threaded through orrery/card/widget builders).
The 29 `ctx_accessors` bodies read `multi.state().windows[pid.0].X` / write
`multi.update_local(pid, …)`; the dispatch/focus sites reroute to `multi.<m>(pid, …)`;
boot/spawn push a projection (index-aligned, `WindowLocal` appended before
`push_projection` so `windows[i]` exists for the logic), close tombstones via
`remove_projection`; the S0 `Rc<RefCell<SharedChrome>>` collapsed into owned
`AppState.shared`, and the crawl/sync writers became one `multi.update` (the per-window
nudge loop is gone). `shell_runner` / `ShellRunner` / `ShellLogic` / `ShellState` deleted.
Genet change: `ProjectionId(pub usize)` (the slot index is the host's `windows` index —
the intended alignment). **Verified**: `cargo check -p meerkat --all-targets` 0 errors;
`cargo test -p meerkat` 302 tests pass (72 + 230); `xilem-serval` 89 tests pass.

**Build-infra note (cost this session):** meerkat resolves the local genet checkout only
through `repos/mere/.cargo/config.toml`'s `[patch."…genet.git"]`, which cargo finds from
the **cwd**, not `--manifest-path`. Run mere cargo commands from `repos/mere` (a subshell
`cd` keeps the tool cwd stable) — else both that patch (→ falls back to git genet, no
`multi.rs`) and `rust-toolchain.toml` (→ default toolchain) are silently bypassed.

**S3d — the source-first rebuild-order invariant holds by construction (no code change).**
A tear-out appends the target projection *after* its source exists, and `ProjectionId`s
never reuse (close tombstones in place), so a target always has a higher index than its
source; `GenetMultiRunner` rebuilds in insertion order, so the source rebuilds before the
target. The dedicated window-order test + the actual `PortableKeyed` cross-window survival
are deferred to the forest-dom slice (design step 3): until then a cross-window tile move
degrades to fresh-build regardless (see "Not in scope"), so the test would assert an
invariant with no live consumer. Re-confirm the invariant when the forest dom lands.

## S3c execution notes (deepened recon, 2026-07-07)

Verified against the live `Shell` / `WindowCtx` / spawn-close code, for a clean flip:

- **Two per-window collections, index-aligned.** `Shell.windows: HashMap<WindowId,
  WindowView>` (host bookkeeping: surface, caches, drags, dom, chrome_session) stays;
  the multi-runner owns `AppState.windows: Vec<WindowLocal>` (the view state that was
  `runner.state().local`). `WindowView` gains `projection_id: ProjectionId`; its
  `WindowLocal` moves into `AppState.windows[projection_id.0]`. The multi-runner's
  `projections` Vec and `AppState.windows` Vec must stay index-aligned: **append
  together at spawn, never pop, tombstone on close** (`remove_projection` already keeps
  slots stable; leave the `WindowLocal` slot in place). `ProjectionId(i)` from
  `push_projection` therefore equals the `windows` index `i`.
- **The projection push relocates.** `build_window_view` is `&self` and cannot push a
  projection (needs `&mut multi`). Build the `WindowView` + its initial `WindowLocal`
  there (WindowView carries the local until adoption), then in `create_window` /
  `spawn_window_with_view` (`&mut self`): `let i = multi.state().windows.len();
  multi.update(|app| app.windows.push(local)); let pid = multi.push_projection(view.dom.clone(),
  boxed_logic(i)); view.projection_id = pid;`. Boot (`shell_new`) does the same for the
  primary before `pending_view` is set. Close: `multi.remove_projection(view.projection_id)`
  alongside `windows.remove(&id)`.
- **`WindowCtx` gains `multi: &mut GenetMultiRunner<AppState>`** — a disjoint `Shell`
  field from `windows`, so `ctx()` / `window_ctx()` (`shell_access.rs`) borrow `view`
  (`&mut self.windows[id]`) and `multi` (`&mut self.multi`) together without conflict.
- **The 29 accessor bodies** (in `ctx_accessors.rs`) swap delegation for: reads →
  `&self.multi.state().windows[self.view.projection_id.0].X`; per-window writes →
  `self.multi.update_local(self.view.projection_id, |app| app.windows[pid.0].X = …)`;
  `chrome_update` → `update_local` over `windows[pid].chrome`; `refresh_shared_chrome`
  disappears (a shared-chrome change now uses `multi.update`, rebuilding every window —
  the S0 nudge loop in `handler_user.rs` collapses to one `multi.update`).
- **The 73 dispatch reroutes are per-method, not a single regex** — inserting
  `projection_id` as the first arg breaks zero-arg calls. Two shapes:
  `self.view.runner.focus()` → `self.multi.focus(self.view.projection_id)` (wrap), and
  `self.view.runner.dispatch_key(e)` → `self.multi.dispatch_key(self.view.projection_id, e)`
  (prepend). Do the ~13 method names as targeted replaces.
- **Order to minimize the red window**: (1) `AppState` + `Shell.multi` + `WindowView.
  projection_id` + `WindowCtx.multi` (structural); (2) boot/spawn/close push/remove; (3)
  the 29 accessor bodies; (4) the 73 dispatch reroutes; (5) bare-`WindowView` callers →
  `Shell.multi`; (6) delete `shell_runner` / `ShellRunner`; (7) compiler-driven cleanup.

## Not in scope (later design steps)

Forest dom (design step 3: one `ScriptedDom`, N window-roots) and portable tiles (step 4:
`PortableKeyed` + `move_before`) follow this. This slice keeps **N doms** (one per
projection); cross-window tile moves still degrade to fresh-build until the forest dom.
