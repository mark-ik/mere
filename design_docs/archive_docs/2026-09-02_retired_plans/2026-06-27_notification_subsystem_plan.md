# Notification Subsystem Plan — Steward-accounted notifications + transient toasts

**Date**: 2026-06-27
**Status**: **Phase 0 (foundation) DONE + tested; Phases 1-4 planned, tightened
pre-Phase-1 (2026-07-01).** Spun from the
[tear-out gestures plan](2026-06-24_tearout_gestures_plan.md)'s ambiguous-drag toast item,
on Mark's reframe: notifications are a **subsystem accounted for by the Steward**, with
toasts as their transient view, not a one-off chrome widget.
**Lane / conflict posture**: meerkat `observability` + `chrome`. No kernel / orrery changes.
**Consumers**: the tear-out ambiguous-drag prompt (actionable), the diagnostics-dump path
(already writes a transient notification with no toast to show it, `command_drain.rs:81`),
failed fetches, crawl/sync discrete events, save / export / clip outcomes.

---

## Thesis

User-facing events scatter today. There are ~37 `record_diagnostic` sites (a dev log the
Apparatus shows), a comms `send_status` line, two live status **chips** (sync, crawl), and at
least one place (`command_drain.rs:81`) that already writes a transient notification
(`record_notification(_, _, _, true)`) with no toast drain to render it. There is no unified
user-facing notification layer.

The subsystem gives one: a notification is a first-class record; the **Steward** accounts for
the log (it is the center); **transient** notifications surface as **toasts**; **actionable**
ones carry verbs. The continuous status **chips** (sync, crawl) stay as they are; they show
ongoing state, not discrete events.

---

## The model

A notification has two homes:

1. **The log** (durable record, Steward-surfaced): `NotificationRecord` in
   `HostObservability`, beside `diagnostics`. The record of what happened. **Built.**
2. **The transient toast queue** (ephemeral view), in `Chrome`, drained from recent
   `transient` notifications, rendered stacked, auto-dismissing (Info / Warn) or sticky until
   dismissed (Error / actionable).

`record_notification(severity, title, body, transient)` writes the log; the host drains new
`transient` ones into the chrome toast queue each frame via `chrome_update` (the existing
live-state fold, e.g. `comms.set_send_status`).

**Where actions live.** Actionable notifications (buttons → host verbs) are held in the
**chrome**, where commands are reachable; `observability` is a lower layer that must not know
`ShellCommand`. The action carries a verb the host dispatches through the existing
`pending_command` / intent seam.

**Split rationale.** The log is the Steward's (accountability + history); the toast is the
chrome's (transient view); actions ride with the toast (commands). The Steward is the
notification **center** for v1; a dedicated center surface is a later option.

---

## Phases

### Phase 0 — Foundation — DONE (2026-06-27), tested

`NotificationRecord` (severity / title / body / time / `transient`) logged in
`HostObservability` beside `diagnostics`; `record_notification(severity, title, body,
transient)`; `HostObservability::notification_rows` surfaced by the **Steward** (`steward_rows`
shows a count + the recent few). Unit-tested `notifications_log_and_surface_for_the_steward`.
Built in the tear-out pass.

### Phase 1 — The transient toast view (chrome) — interactive, headed

**Prerequisite: give `NotificationRecord` an id.** It currently has `severity/title/body/at/
transient` and no identifier (`observability/mod.rs:69`). Add a monotonic `NotificationId` —
the drain cursor below, manual dismiss, clear, and Phase 2's action-result correlation all need
to address one specific notification.

A toast queue in `Chrome` (`Vec<Toast>` or a small ring) with `Toast { id, severity, title,
body, created_at, ttl, dismissable }`. A `recent_notifications` accessor on `HostObservability`
plus a "last drained" cursor keyed past `NotificationId`; the host folds new transient
notifications into the queue each frame (`chrome_update`). Render a corner-anchored, stacked
chrome element (modelled on the branch chip / context menu), styled by severity,
auto-dismissing after `ttl` and click-to-dismiss.

**Name the toast scope.** `HostObservability` lives on `SharedState`, shared across every
window (`app_state.rs:42`); `Chrome` is per-window on `WindowView` (`window_view/mod.rs:529`).
A single shared drain cursor means whichever window renders first eats the toast; an unscoped
per-window cursor means every window toasts it. Decide a `ToastScope` (current window /
graph-session / global) as part of this phase rather than picking implicitly.

**Auto-dismiss needs a clock path.** The render loop only self-schedules a redraw while the
orrery is settling / gliding / dragging (`render/paint.rs:518`); nothing currently wakes the
frame for a TTL expiry. Either request a redraw every frame while live toasts exist, or
schedule a wake for the nearest expiry.

Done when: `record_notification(_, _, _, true)` shows a toast that auto-dismisses on its own
(no unrelated input needed); manual dismiss works; the diagnostics-dump path
(`command_drain.rs:81`) uses it instead of sitting un-rendered.

### Phase 2 — Actionable notifications — interactive, headed

Give a toast optional `actions: Vec<NotificationAction { label, verb }>`, rendered as buttons;
a click drains the verb as a host intent. **Not a generalized `pending_command`:** that slot
carries `Command` (`lib.rs:130`), and `Command` (`command.rs:19`) is entirely unit variants with
no payload fields, so it can't represent `TearOut { node, from }`. Add a payload-bearing
`NotificationIntent` (or `ToastIntent`) queue on `Chrome`, drained by `WindowCtx` the same way
`pending_connect` / `comms_intent` already are, rather than stretching `Command`.

The tear-out **ambiguous-drag** is the first consumer: a no-modifier orrery-node **drag-out**
fires an actionable notification offering **Keep-leaf / Branch / Fork**, toasted + logged; each
verb queues its command (`TearOut { node, from }` / `BranchNode` / `ForkNode`). (The drag-out
vs pin-drag gesture is the tear-out plan's piece; this phase owns the actionable-toast half.)

**Keyboard reachability ships with this phase, not Phase 4.** Actionable toast buttons need
normal chrome activation/a11y routing (focus-reachable, keyboard-activatable) as part of the
same change that makes them clickable — a mouse-only action prompt is not an acceptable
intermediate state. Live-region announcement (screen readers hearing the toast at all) can
still wait for Phase 4.

Done when: the ambiguous-drag prompt offers the three verbs, each is reachable and
activatable by keyboard, and each runs its command; the choice is also in the Steward log.

### Phase 3 — Producers (consolidate the scatter)

Route **discrete** user-facing events through `record_notification` (transient where the user
should see it now): failed fetches (`Severity::Error`), crawl done, sync connected / failed,
save / export / engram / clip outcomes. Keep the **continuous** chips (sync, crawl) as live
status; notifications are for discrete moments. Audit the ~37 `record_diagnostic` sites:
dev-only ones stay diagnostics; user-facing ones *also* `record_notification`.

**Dedupe and rate-limit before the broad rollout.** Failed fetches, crawl, sync retries, and
background jobs can spam the toast queue once ~37 sites feed it. Give `NotificationRecord` a
`source` and `category`, an optional dedupe key (collapse repeats into one updating toast with
a count), and user settings for severity routing / TTL / max-visible — land this alongside the
first few producers, not after all 37 are wired.

Done when: a failed fetch toasts + logs; the Steward shows recent notifications; the sync /
crawl chips are unchanged; no event is double-counted as both a chip pulse and a toast; a
repeated failure (e.g. a retry loop) collapses to one updating toast instead of a stack.

### Phase 4 — Severity routing, dismissal, a11y

Severity-driven behaviour: Info auto-dismiss short, Warn longer, Error sticky until dismissed.
Per-notification dismiss; a "clear" affordance in the Steward. A11y: announce via AccessKit (a
live region) so notifications are not vision-only. (Actionable-toast keyboard/focus
reachability moved to Phase 2 — those buttons ship activatable from the start, not retrofitted
here.) Persistence across restart is out of scope for v0 (the log is in-memory bounded;
durable-important notifications are a later call).

---

## Findings (verified against the code, 2026-06-27)

- **Foundation exists** (Phase 0): `NotificationRecord` + `record_notification` +
  `notification_rows` + the Steward surface, unit-tested.
- **~37 user-facing event sites** route only to `record_diagnostic` (dev log / Apparatus);
  none toast. `command_drain.rs:81` already writes a transient `record_notification`
  (diagnostics-dump path); it has nowhere to render.
- The chrome already folds live state via `chrome_update` (e.g. `comms.set_send_status`); the
  toast drain rides that pattern, no new plumbing.
- Live **chips** (`SyncIndicator`, `CrawlIndicator` in `Chrome`) are continuous status,
  distinct from discrete notifications; they stay.
- **No toast element exists** today (confirmed by grep).

---

## Pre-Phase-1 tightening (2026-07-01)

A review pass before Phase 1 starts, verified line-by-line against the current code, not just
re-read against this doc:

- **Identity.** `NotificationRecord` (`observability/mod.rs:69`) has no id. Folded into Phase 1
  as a prerequisite — the drain cursor, dismissal, clear, and Phase 2's action-result
  correlation all need to address one specific notification.
- **Window scope.** `HostObservability` sits on `SharedState`, shared across every window
  (`app_state.rs:42`); `Chrome` is per-window on `WindowView` (`window_view/mod.rs:529`). This
  is live architecture, not a hypothetical — a naive shared drain cursor starves every window
  but the first to render, and a naive per-window one re-toasts in each. Folded into Phase 1 as
  a named `ToastScope` decision.
- **No payload-bearing intent exists yet.** `pending_command` carries `Command` (`lib.rs:130`),
  and `Command` (`command.rs:19`) is entirely unit variants. Phase 2's first consumer
  (`TearOut { node, from }` / `BranchNode` / `ForkNode`) needs a new payload-bearing intent
  queue, not a generalization of `pending_command`. Folded into Phase 2.
- **No redraw clock for TTL expiry.** `render/paint.rs:518` only self-schedules a redraw while
  the orrery is settling / gliding / dragging. A toast TTL would sit stale past expiry until
  unrelated input arrives. Folded into Phase 1.
- **Keyboard reachability moved up.** Actionable toasts need focus/keyboard routing in the
  same phase they ship clickable (Phase 2), not deferred to Phase 4's a11y pass.
- **Dedupe / rate-limit before the broad producer rollout.** Folded into Phase 3, ahead of
  wiring all ~37 `record_diagnostic` sites.

Net: the model (log in `HostObservability`, transient view in `Chrome`, actions out of
observability, chips staying separate from discrete events) holds. These are seam-exactness
gaps in the unbuilt phases, not a rethink of the split.

---

## Design decisions

- **Two homes, one record.** The log (observability, Steward) is the truth; the toast (chrome)
  is a transient view drained from it. Not two parallel systems.
- **Actions in the chrome, not observability.** Keeps `observability` free of `ShellCommand`;
  rides the same intent-queue pattern as `pending_command` / `pending_connect` / `comms_intent`
  (a new payload-bearing queue — `Command` itself is unit-variant-only, see Phase 2).
- **Chips stay; notifications are discrete.** Continuous status (sync / crawl progress) is not
  a notification; a state *transition* (connected, crawl done, fetch failed) is.
- **Steward is the center for v1.** A dedicated notification-center surface is deferred until
  the Steward section feels cramped.

---

## Progress

- **2026-06-27** — Plan created. **Phase 0 foundation built + tested** in the tear-out pass
  (`NotificationRecord` log in `HostObservability`, `record_notification`, Steward
  `notification_rows`; `notifications_log_and_surface_for_the_steward`). Grounded the plan
  against the code: ~37 `record_diagnostic` producers, the `command_drain.rs:81`
  transient-notification-with-no-toast site, the `chrome_update` fold seam, and the continuous
  sync / crawl chips. Phases 1-4 (toast view, actionable notifications, producer consolidation,
  severity / dismissal / a11y) planned; the interactive phases want headed verification.
- **2026-07-01** — Pre-Phase-1 tightening: an agent review of the plan, then verified against
  the code rather than the doc. The split holds; folded six gaps into Phases 1-3 (notification
  identity, a named `ToastScope` for the shared-observability/per-window-chrome split, a typed
  `NotificationIntent` instead of stretching `pending_command`, a redraw clock for TTL expiry,
  keyboard reachability moved from Phase 4 into Phase 2, and dedupe/rate-limits ahead of the
  Phase 3 producer sweep). Corrected the doc's stale `command_drain.rs:72` "wants a toast"
  framing to match Phase 0's actual state. No implementation this pass.
