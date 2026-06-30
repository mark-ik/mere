# Notification Subsystem Plan — Steward-accounted notifications + transient toasts

**Date**: 2026-06-27
**Status**: **Phase 0 (foundation) DONE + tested; Phases 1-4 planned.** Spun from the
[tear-out gestures plan](2026-06-24_tearout_gestures_plan.md)'s ambiguous-drag toast item,
on Mark's reframe: notifications are a **subsystem accounted for by the Steward**, with
toasts as their transient view, not a one-off chrome widget.
**Lane / conflict posture**: meerkat `observability` + `chrome`. No kernel / orrery changes.
**Consumers**: the tear-out ambiguous-drag prompt (actionable), the diagnostics-dump path
(already wants a toast it cannot make, `command_drain.rs:72`), failed fetches, crawl/sync
discrete events, save / export / clip outcomes.

---

## Thesis

User-facing events scatter today. There are ~37 `record_diagnostic` sites (a dev log the
Apparatus shows), a comms `send_status` line, two live status **chips** (sync, crawl), and at
least one place (`command_drain.rs:72`) that wants to "surface the path as a toast" but has
nowhere to put it. There is no unified user-facing notification layer.

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

A toast queue in `Chrome` (`Vec<Toast>` or a small ring) with `Toast { severity, title, body,
created_at, ttl, dismissable }`. A `recent_notifications` accessor on `HostObservability` plus
a "last drained" cursor; the host folds new transient notifications into the queue each frame
(`chrome_update`). Render a corner-anchored, stacked chrome element (modelled on the branch
chip / context menu), styled by severity, auto-dismissing after `ttl` and click-to-dismiss.

Done when: `record_notification(_, _, _, true)` shows a toast that auto-dismisses; manual
dismiss works; the diagnostics-dump path (`command_drain.rs:72`) uses it instead of its
absent-toast comment.

### Phase 2 — Actionable notifications — interactive, headed

Give a toast optional `actions: Vec<NotificationAction { label, verb }>`, rendered as buttons;
a click drains the verb as a host intent (the `pending_command` pattern, generalized). The
tear-out **ambiguous-drag** is the first consumer: a no-modifier orrery-node **drag-out** fires
an actionable notification offering **Keep-leaf / Branch / Fork**, toasted + logged; each verb
queues its command (`TearOut { node, from }` / `BranchNode` / `ForkNode`). (The drag-out vs
pin-drag gesture is the tear-out plan's piece; this phase owns the actionable-toast half.)

Done when: the ambiguous-drag prompt offers the three verbs and each runs its command; the
choice is also in the Steward log.

### Phase 3 — Producers (consolidate the scatter)

Route **discrete** user-facing events through `record_notification` (transient where the user
should see it now): failed fetches (`Severity::Error`), crawl done, sync connected / failed,
save / export / engram / clip outcomes. Keep the **continuous** chips (sync, crawl) as live
status; notifications are for discrete moments. Audit the ~37 `record_diagnostic` sites:
dev-only ones stay diagnostics; user-facing ones *also* `record_notification`.

Done when: a failed fetch toasts + logs; the Steward shows recent notifications; the sync /
crawl chips are unchanged; no event is double-counted as both a chip pulse and a toast.

### Phase 4 — Severity routing, dismissal, a11y

Severity-driven behaviour: Info auto-dismiss short, Warn longer, Error sticky until dismissed.
Per-notification dismiss; a "clear" affordance in the Steward. A11y: announce via AccessKit (a
live region) so notifications are not vision-only, and actionable toasts are focus-reachable
for keyboard activation. Persistence across restart is out of scope for v0 (the log is
in-memory bounded; durable-important notifications are a later call).

---

## Findings (verified against the code, 2026-06-27)

- **Foundation exists** (Phase 0): `NotificationRecord` + `record_notification` +
  `notification_rows` + the Steward surface, unit-tested.
- **~37 user-facing event sites** route only to `record_diagnostic` (dev log / Apparatus);
  none toast. `command_drain.rs:72` explicitly wants a toast it cannot make.
- The chrome already folds live state via `chrome_update` (e.g. `comms.set_send_status`); the
  toast drain rides that pattern, no new plumbing.
- Live **chips** (`SyncIndicator`, `CrawlIndicator` in `Chrome`) are continuous status,
  distinct from discrete notifications; they stay.
- **No toast element exists** today (confirmed by grep).

---

## Design decisions

- **Two homes, one record.** The log (observability, Steward) is the truth; the toast (chrome)
  is a transient view drained from it. Not two parallel systems.
- **Actions in the chrome, not observability.** Keeps `observability` free of `ShellCommand`;
  reuses the `pending_command` intent seam.
- **Chips stay; notifications are discrete.** Continuous status (sync / crawl progress) is not
  a notification; a state *transition* (connected, crawl done, fetch failed) is.
- **Steward is the center for v1.** A dedicated notification-center surface is deferred until
  the Steward section feels cramped.

---

## Progress

- **2026-06-27** — Plan created. **Phase 0 foundation built + tested** in the tear-out pass
  (`NotificationRecord` log in `HostObservability`, `record_notification`, Steward
  `notification_rows`; `notifications_log_and_surface_for_the_steward`). Grounded the plan
  against the code: ~37 `record_diagnostic` producers, the `command_drain.rs:72` wants-a-toast
  site, the `chrome_update` fold seam, and the continuous sync / crawl chips. Phases 1-4
  (toast view, actionable notifications, producer consolidation, severity / dismissal / a11y)
  planned; the interactive phases want headed verification.
