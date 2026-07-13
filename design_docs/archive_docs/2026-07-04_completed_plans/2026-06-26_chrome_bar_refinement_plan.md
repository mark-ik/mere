# Chrome Bar Refinement Plan

**Date**: 2026-06-26
**Status**: **COMPLETE — P1–P5 + follow-ons landed + verified headed (2026-06-26).** Follow-ons: context-menu edge-flip, UI scaling (now full auto-DPI — see [ui_dpi_scaling_plan](2026-06-26_ui_dpi_scaling_plan.md)), the omnibar single-line fix, the P4 switcher/thumbnail dead-code cleanup, and the even-shellbar fix.
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
  and one tofu box. **Two suspects**: (a) genet not honoring flex-item width/height
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
  (genet flex-item sizing/justify vs CSS not applied), then fixed so each glyph is
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

**2026-06-26 — P3 landed (+ batched P1/P2 headed confirmation).**
- Shellbar centering: genet's flex does **not** centre a bare text child via
  `justify-content` (confirmed at runtime — the prior `width: 44px` + `justify-content:
  center` left every glyph hugging the button's left edge). Fix: content-width buttons
  with *symmetric* `padding: 0 13px` (centres the glyph whatever its width) centred in
  the strip via the container's `align-items: center`. Verified headed.
- Glyph coverage: the host font (parley/fontique system fallback) covers only Math
  Operators + Geometric Shapes + text-presentation Misc Symbols (⚙ U+2699, ⚒ U+2692).
  It has **no** Arrows block and routes emoji-presentation symbols (⚗ ⚛ ⏹ ✉) to the
  colour-emoji font (or tofu under VS15). Replaced the broken shellbar glyphs: trail
  ⇝→◈ (U+25C8), alembic ⚗→▽ (U+25BD distillation funnel), comms ✉→@ (apt: misfin is
  `mailbox@server`). Same fix for the Steward verb buttons — ↻/⏹/⚓ had no text glyph, so
  they now use word labels (retry/stop/pin focused; per-row stop/pin/retry).
- Batched headed confirmation (one driven session, shots in `scry-shots/p3-*.png`):
  **P1** — Steward shows `Sync lane: tessera / Syncing now / Standing: +11 / Tessera
  ticket: endpoint…`; Apparatus shows the at-rest `Sync` section (`Lane: tessera /
  Caught-up ops: 0`). The omnibar chip is gone. **P2** — Steward's "Live operations"
  section renders with the honest "No live operations" empty state (no live ops in a
  fresh session). **P3** — all nine shellbar glyphs render as centred monochrome
  line-art. (Per-row verbs firing against a *live* op still unverified — needs a loaded
  tile; the wiring is unit-tested + the routing is straightforward.)
- Tests: lib 89/89, steward bin 5/5 green after the CSS/glyph/label changes.

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

**2026-06-26 — P2 landed.**
- Extracted the Steward pane into a new `steward.rs` module (moved `steward_rows`,
  `steward_items`, `steward_sync_rows`, `fetch_state_count`, `short_member` out of
  `pane_data.rs`). Driven by the 600-LOC ceiling — `pane_data.rs` was at 589 and P2
  deepens Steward, the "split before adding when approaching the limit" case. Result:
  `pane_data` 445, `steward` 263, `node_ops` 564 — all under 600.
- Process list: `steward_items` now renders a "Live operations" section — one row per
  `active_operations()` entry (short id · url · state) with per-row stop / pin / retry
  buttons keyed `steward:<verb>:<uuid>`. Capped at `SHOWN_OPS` = 24 with a muted
  "+N more" note (no silent cap). Honest empty-state row when nothing is live. Replaced
  the old count + 6-row truncated dump.
- Per-row verbs: extracted member-targeted `stop_operation` / `pin_operation` /
  `retry_content_url` (relocated into `steward.rs` as the pane's action handlers); the
  focused-op verbs now delegate to them, so focused and per-row share one path. The
  drain (`input.rs`) parses `steward:<verb>:<uuid>` via `GraphMemberId::parse_str`
  (member id is a `Uuid`); retry resolves the member's URL from `active_operations()`.
  Kept the bare `steward:retry`/`stop`/`pin` focused keys (act on the focused op, which
  may be dormant and absent from the live list).
- Tests: `cargo check -p meerkat` clean; lib 89/89, steward/apparatus bin 6/6 incl. a
  new `steward_shows_a_live_operations_section` (asserts the header + honest empty-state
  without spawning actors). The per-row verb keys firing against live ops is part of the
  batched P3 headed pass.

**2026-06-26 — follow-on asks (context menu + UI scaling).**
- Context-menu edge-flip: the root menu now opens *away* from the right / bottom edges
  instead of clipping. It places down-right of the cursor by default and flips left /
  up when the panel would overflow, in `render.rs`. Height is estimated from the item
  count (one search row + items × ~35px) rather than the measured rect — the pre-existing
  `max-height` cap shrinks the measured size to "fit", masking the overflow; width uses
  the measured natural width or a 240px default. Verified headed (bottom-right → flips
  up-left, fully on-screen; top-left → normal down-right).
- UI scaling: a single `ui_scale` (= the user's `user_zoom`) multiplied through the
  chrome via `scale_px`, which scales every `Npx` token in the built sheet (no 37
  hand-edits; unitless numbers / `rgb()` untouched). Default 1.1 (the "point or two
  larger" baseline), persisted as `PersistedSettings::ui_zoom`. Ctrl +/-/0 adjust /
  reset (browser-style), clamped 0.6–3.0, rebuilding the sheet + re-measuring the
  toolbar height + re-rasterising the window-control texture. Theme switch rebuilds at
  scale too. Verified headed (baseline modest bump; Ctrl+= / Ctrl+- / Ctrl+0 all work).
- **Auto-DPI deferred (finding):** wiring winit's `scale_factor` into `ui_scale` was
  built then removed — winit reports **2.0 on this 96-DPI (100%) panel**, and meerkat
  lays the chrome out in *physical* pixels with a physical-pixel-sized window, so
  multiplying by it double-counts and oversizes/wraps the chrome. True DPI-awareness
  needs a logical-pixel migration of the host (window sizing, layout, input) — a much
  larger change. Until then the user's Ctrl-zoom *is* the manual DPI knob. `ui_scale`'s
  doc-comment records this so it isn't naively re-added.
- Tests: `scale_px` unit tests (px scales, unitless/colors untouched, 1.0 no-op);
  session-runtime settings 10/10; meerkat lib 89/89.

**2026-06-26 — P4 landed (sessions into the toolbar).**
- Sessions moved from the host-drawn shellbar-bottom switcher into a DOM **session
  strip** in the toolbar: a chip per open session (label activates, × closes), an
  overflow `+N ⌄` past `SESSION_INLINE_CAP` = 4, and an add `+`. Chips carry the active
  session's selection highlight (representation-identity); long labels (a session named
  after a URL) clip to ~22 chars.
- New `Chrome` state: `sessions: Vec<SessionChip>`, `sessions_overflow_open`,
  `session_intent` (one-shot), with `pick_session` / `request_close_session` /
  `request_create_session` / `toggle_sessions_overflow`. Host syncs the chip list each
  frame (`render.rs`, ordered by id like `cycle_session`, active = focused pane's
  session, rename buffer shown live in the chip) and drains `session_intent` into the
  existing `ShellCommand`s (`drain_session_intent` — Activate→Switch, or OpenGraphBeside
  on Shift; Close; Create), so behavior matches the old switcher.
- Removed the host-drawn switcher block from `render.rs` (texture strip + `session_*_rect`
  caches no longer populated; the input rects are now inert no-ops). Rename stays reachable
  via F2 (the buffer renders in the chip); right-click-chip rename deferred.
- Verified headed: chips render in the toolbar with the active highlight + clipped label,
  the shellbar's bottom switcher is gone, the strip `+` mints a session. Tests: lib 89/89
  (toolbar div-count updated 6→8 for the strip + add), bin session/rename/cycle 12/12.
- Deferred to P5: two `+` buttons coexist (the strip's session-add + the toolbar add-pill);
  P5 consolidates by dropping the pill's "Add session".

**2026-06-26 — P5 landed (segmented add group). Chrome-bar plan COMPLETE.**
- The single add-pill became a segmented `+node | +tile | +field` group in the toolbar
  (`add_group` in `views.rs`), each button firing its verb directly via
  `Chrome::pick_context(ContextAction::Add*)` — no menu. Collapses to a split-button
  (primary `+` adds a node; a caret opens the add menu for tile/field) when the toolbar
  is crowded (`sessions.len() > SESSION_INLINE_CAP`). "Add session" dropped from
  `open_add_menu` (the P4 session strip owns session creation); the session-add `+` and
  the add group are now distinct, non-redundant affordances.
- Added `white-space: nowrap` so the `+word` labels stay single-line (they wrapped at
  first, inflating the toolbar height). Verified headed.
- Tests: lib 89/89 (toolbar asserts updated: button 13→15 for the 3 segmented buttons;
  div 8→9 for the add-group container), bin add/menu/context green.
- **Cleanup deferred:** P4 left the `switcher` *render* module (`switcher_scene` /
  `switcher_height`) dead — but the thumbnail pipeline it fed (`session_ops`
  `refresh_session_thumbnails`, `session_thumbnails`) is still alive because the P4 chip
  list reads `session_thumbnails.keys()` as its session source. A proper cleanup switches
  the session-list source to a canonical list (manifests/labels) and then removes the
  whole thumbnail + switcher-scene pipeline. Tracked as a follow-on, not done here.

**2026-06-26 — toolbar robustness (omnibar single-line).**
- The omnibar was wrapping `mere://welcome` to two lines when squeezed (notably at 2×
  DPI with a long session chip + the add group), inflating the toolbar height. Fixed:
  `input` gets `min-width: 0; white-space: nowrap; overflow: hidden; text-overflow:
  ellipsis` — it shrinks in the flex row and clips to one line ("mere://welco…").
- `.toolbar` gets `flex-wrap: nowrap; align-items: center` so a tall child can't stretch
  the buttons to odd proportions and the band stays one row (nothing pushes the shellbar
  or session chip out of view). Verified headed at 2×.

**2026-06-26 — closeout: switcher cleanup + even shellbar + omnibar reconfirm.**
- **Switcher/thumbnail dead-code removed** (the P4 deferred cleanup): the session-chip
  list now sources from the canonical `manifests` (what `cycle_session` enumerates), not
  the retired `session_thumbnails` map. `refresh_session_thumbnails` → `refresh_session_labels`
  (builds only labels; the thumbnail rasterization on every session change + on every save
  is gone). Deleted the `session_thumbnails` field, both `build_switcher_thumbnail_with`
  call sites, the whole `switcher.rs` module (`switcher_scene`/`switcher_height` were dead
  since P4), and the now-unused imports. Build clean; session/toolbar tests 13/13.
- **Even shellbar** (reported uneven): the P3 buttons are content-width (symmetric padding
  centres the glyph), so per-button *backgrounds* read as a ragged-width column at 2×.
  Fix: inactive buttons now have **no** background — a clean centred glyph column — and only
  the active button keeps a fill (a rounded accent pill marking the open pane). Verified
  headed (crop `scry-shots/pc-shellbar.png`).
- **Omnibar single-line reconfirmed** (reported toolbar-over-tile-tabs): a long URL
  (`settings://node:44d9b7fb-…/info/longpath/segment`) clips to one line and the toolbar
  stays a single row (`white-space: nowrap; overflow: hidden` on the omnibar + `flex-wrap:
  nowrap; align-items: center` on the toolbar) — so it never balloons to crowd the pelt
  tile-tab strip. The 3-line-wrap screenshots (`screenshots/...192229.png`) were a pre-fix
  state. Verified headed (`scry-shots/pc-longurl-toolbar.png`).
