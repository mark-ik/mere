# Chrome Bar Refinement Plan

**Date**: 2026-06-26
**Status**: Planning; no code yet.
**Related**: [shellbar_plan (F2, in progress)](2026-06-09_shellbar_plan.md), [apparatus_pane_and_theme_switcher_plan](2026-06-08_apparatus_pane_and_theme_switcher_plan.md), [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md), `crates/meerkat/` (`views.rs`, `render.rs`, `pane_data.rs`, `apparatus.rs`, `main.rs`)

A polish pass over the toolbar (omnibar band), the shellbar, and the Steward /
Apparatus split. Five moves: rehome the tessera status out of the omnibar into
Steward, sharpen the live-vs-at-rest split across Steward and Apparatus (with a
real process list in Steward), move the session switcher out of the shellbar into
the toolbar, turn the `+` pill into a segmented add group, and fix shellbar button
centering + missing glyphs.

---

## Decisions (locked 2026-06-26)

- **Tessera home**: Steward (the live / async / actionable plane). Apparatus keeps
  the at-rest sync trace / record. Honors the static-vs-live axis from the
  [peripheral panes doc §Runtime authority](../technical_architecture/2026-06-06_peripheral_panes_architecture.md).
- **Sessions in the toolbar**: hybrid — inline chips up to a cap, then a `+N ⌄`
  overflow dropdown. Session add lives with the strip.
- **`+` create affordance**: a segmented add group (`+node | +tile | +field`),
  always visible; collapses to a split-button when the bar is crowded. The old
  menu's "Add session" item is dropped (the session strip owns session creation).

---

## Findings (grounded)

- **Tessera chip** = the `sync-chip` `div` in [views.rs:126](../../../crates/meerkat/src/views.rs#L126),
  rendering `c.sync.summary()` (`"tessera: idle"`), pushed to the toolbar's right by
  the omnibar flex-grow. CSS at [main.rs:210](../../../crates/meerkat/src/main.rs#L210).
- **Steward already carries** the live ops + sync + standing + ticket via
  `steward_rows` / `sync_summary` ([pane_data.rs:348](../../../crates/meerkat/src/pane_data.rs#L348),
  [:462](../../../crates/meerkat/src/pane_data.rs#L462)) and act-on verbs (retry /
  stop / pin) in `steward_items` ([:430](../../../crates/meerkat/src/pane_data.rs#L430)).
  So the chip is largely redundant; the work is delete-from-toolbar + confirm the
  pane is complete.
- **Process list**: `self.shared.content.constellation.active_operations()` is read
  in `steward_rows` but only surfaced as a count plus 6 truncated `Operation …`
  rows. Steward is the actor / process monitor per the architecture doc; this should
  become a real list.
- **Apparatus** ([apparatus.rs:139](../../../crates/meerkat/src/apparatus.rs#L139))
  is read-only diagnostics (Overview / UX Events / Actors / Accessibility /
  Diagnostics / Tracing / Registry / Probes). It has no dedicated at-rest **sync
  trace** section yet; the at-rest half of the split lands here.
- **Session switcher** = a host-drawn texture strip at the shellbar's bottom,
  Left/Right edges only ([render.rs:2063](../../../crates/meerkat/src/render.rs#L2063)),
  with hit-rects cached on `session_row_rects` / `session_close_rects` /
  `session_add_rect`. Moving it into the toolbar means a DOM widget in `chrome_view`
  and click routing through the chrome runner instead of cached rects.
- **`+` pill** = `add-pill` button → `open_add_menu` ([views.rs:140](../../../crates/meerkat/src/views.rs#L140),
  [lib.rs:976](../../../crates/meerkat/src/lib.rs#L976)); verbs in [menus.rs:614+](../../../crates/meerkat/src/menus.rs#L614).
- **Shellbar render**: `shellbar_view` builds 9 buttons ([views.rs:702](../../../crates/meerkat/src/views.rs#L702)).
  CSS *already* sets `justify-content: center` + 44×44 sizing ([main.rs:471](../../../crates/meerkat/src/main.rs#L471)),
  flex-direction set inline per edge ([render.rs:660](../../../crates/meerkat/src/render.rs#L660)).
  Yet the 2026-06-24 screenshot shows glyphs left-stuck, no 44×44 button grounds,
  and one tofu box. **Two suspects**: (a) serval not honoring flex-item width/height
  or `justify-content` on these buttons; (b) font lacks coverage for some symbol
  glyphs (`⇝` U+21DD, `⚗` U+2697, `✉` U+2709, `⚒` U+2692). **Runtime verification
  required — do not resolve by static tracing.**
- **Test coupling**: [tests.rs:27-30](../../../crates/meerkat/src/tests.rs#L27) asserts
  exact toolbar button/div counts ("back + forward + pause + add-pill + 9 shellbar",
  "chrome + toolbar + branch-chip + sync-chip + crawl-chip + suggestions + shellbar").
  Removing the sync-chip and reshaping the add affordance will move these counts.

---

## Phases

### P1 — Pane split: tessera → Steward, at-rest trace → Apparatus
Done when:
- The `sync-chip` is gone from `chrome_view` + its CSS removed from `main.rs`; the
  toolbar tuple and the `tests.rs` div/button asserts updated.
- Steward shows the full live sync readout it already computes (label / syncing /
  ops / standing) plus the dialable ticket, as first-class rows (not just folded
  into one `Sync:` line) — verified against a running sync lane, no placebo
  (per the real-sync-feedback rule).
- Apparatus gains an at-rest **Sync** section: the sync trace / last-synced record
  drawn from the observability spine (the at-rest half), distinct from Steward's
  live actionable rows.
- The live-vs-at-rest axis is documented inline so the two panes don't re-converge.

### P2 — Steward process list
Done when:
- `active_operations()` renders as a real per-process list in Steward (id / url /
  state / background / recovering), not a count + 6-row truncation, with the
  existing retry / stop / pin verbs bound per-row where feasible (today they target
  only the focused op).
- The list is bounded sanely and logs what it drops if truncated (no silent cap).

### P3 — Shellbar centering + glyph coverage (self-contained; runtime-verify first)
Done when:
- The cause of the left-stick + missing button grounds is identified at runtime
  (serval flex-item sizing/justify vs CSS not applied), then fixed so each glyph is
  centered in its 44×44 cell and the button column is centered in the strip.
- Every shellbar glyph renders (no tofu): either swap the offenders for
  font-covered glyphs or add a symbol-font fallback to the host text stack. Confirm
  with a fresh headed scry-shot.

### P4 — Sessions into the toolbar (hybrid chips + overflow)
Done when:
- The host-drawn session strip is removed from `render.rs` (Left/Right) and its
  `session_*_rect` caches retired.
- `chrome_view` renders a session strip in the toolbar band: inline chips (carrying
  each session's node color + selection highlight per the representation-identity
  rule), capped at N, then a `+N ⌄` overflow dropdown; the omnibar shares the row.
- Click / close / rename / add route through the chrome runner (DOM), not cached
  rects; cycle order matches `cycle_session`.
- Works on all shellbar edges (the old strip was Left/Right only).

### P5 — Segmented add group
Done when:
- The `add-pill` is replaced by a segmented `+node | +tile | +field` group; the
  "Add session" verb is removed (the P4 strip owns it).
- The group collapses to a split-button (primary `+node` + caret) when the toolbar
  is crowded by sessions (P4), so the two toolbar additions don't fight for space.

---

## Batched headed verification

P1 and P2 are wired + unit-tested but their *rendering* (the sync rows, the process
list, the per-row verbs firing) is confirmed in the **P3 headed pass**, which spins
up the harness anyway. So P3's done-condition grows: confirm (1) P1 Steward/Apparatus
sync rows, (2) P2 process list + per-row retry/stop/pin, (3) shellbar centering +
glyph coverage — all in one driven session. (Decided 2026-06-26.)

## Sequencing notes

P1 → P2 are the pane half (P2 builds on P1's Steward). P3 is independent and
self-contained — good to land early for a clean render baseline. P4 is the largest
(host-draw → DOM migration) and P5 depends on P4's space budget, so P5 last.

## Progress

**2026-06-26 — P1 landed.**
- Removed the toolbar `sync-chip` (`views.rs` construction + tuple element) and its
  `.sync-chip` CSS (`main.rs`). The host still folds the real `SyncStatus` into
  `c.sync`; the panes read it from there.
- Steward (live plane): replaced the single folded `Sync:` row + inline ticket with
  first-class rows via `steward_sync_rows` — `Sync lane`, `Syncing now`, `Standing`
  (when a ledger has folded), `Tessera ticket`. Retired the now-unused `sync_summary`.
- Apparatus (at-rest record): new `Sync` section via `apparatus_sync_rows` +
  `unix_age` (Unix-epoch `last_activity_ms`, distinct from the monotonic `Instant`
  the observability `age` helper takes) — `Lane`, `Caught-up ops`, `Last activity`.
  Threaded a `sync_rows` param through `apparatus_items` (`apparatus.rs`) and its
  `render.rs` caller.
- Placement note: kept both row builders (`steward_sync_rows` + `apparatus_sync_rows`)
  together in `pane_data.rs` rather than splitting one into `frame_ops.rs` (already at
  621 LOC, over the 600 ceiling) — the two halves of the split read as siblings there.
- Tests: `cargo check -p meerkat` clean; `meerkat` lib 89/89, bin steward/apparatus/sync
  9/9 (incl. `steward_exposes_clickable_action_verbs`, `..._live_graph_count`,
  `agent_can_open_apparatus_switch_theme_and_open_roster`). Updated the toolbar div-count
  assert (7 → 6; button count unchanged — the chip was a `div`).
- Honesty caveat: the row wiring + item-building are verified by tests and read from
  the real folded `SyncIndicator`; a live two-peer sync round was not driven, so the
  non-default values (ops > 0, standing, ticket present) are unverified at runtime. A
  headed pass to confirm rendering folds into P3 (which needs the harness anyway).

Pre-existing (not P1): `views.rs` has unused-import warnings (`ShellbarEdge`,
`textarea_typed`) untouched here; `views.rs` is chrome-hot, left for its owner.
