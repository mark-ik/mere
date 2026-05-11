# mere-ux-events

Portable chrome-event taxonomy for the [mere](https://crates.io/crates/mere)
browser. The **third pillar** between:

- **code-level events** (`tracing` spans + events emitted from inker /
  nematic / platen / etc.)
- **registry-level events** (`register-diagnostics` channels emitted
  from registry crates)
- **chrome / UX events** (this crate) — `UxEvent` whenever an overlay
  opens/dismisses, a command palette executes a row, a focus authority
  releases, etc.

Every host (gpui today, iced fallback, future Stage-G/Stage-H hosts)
emits the same `UxEvent` taxonomy when chrome surfaces interact. That
shared contract lets:

- The apparatus pane subscribe via `UxObserver` and surface chrome
  events alongside tracing events.
- Automation tests register `UxProbe`s (active invariant-checkers) and
  assert that, e.g., no two modal-like surfaces are open at the same
  time.
- The `register-diagnostics` channel registry receive a stable
  `(channel_id, severity, payload)` triple per UxEvent via
  [`UxChannelObserver`].

## Modules

| Module | Role |
| --- | --- |
| `ux_observability` | `UxEvent` taxonomy, `UxObserver` trait, `CountingObserver`, `RecordingObserver` |
| `ux_diagnostics` | Bridge from `UxEvent` to a diagnostics-channel sink (the seam `register-diagnostics` plugs into) |
| `ux_probes` | Active invariant-checkers: `MutualExclusionProbe`, `OpenDismissBalanceProbe`, `ProductiveSelectionProbe`, `DestructiveActionGateProbe` |
| `command_surface_telemetry` | Publish-latest-snapshot cell + sequence counter for command-bar / omnibar / palette widgets |

## History

These modules previously lived in `graphshell-shell-state` (a kitchen-
sink portable session-state crate). They were extracted to this crate
on 2026-05-10 because:

1. They form a coherent unit (ux_diagnostics + ux_probes both pivot
   on `UxEvent` defined in ux_observability).
2. They're broadly useful, not graphshell-specific — every host emits
   the same taxonomy.
3. They bridge to `register-diagnostics` (registry events) and feed
   `mere-host-apparatus` (host-side inspector). Sitting under a single
   shell-state crate understated the scope.

`graphshell-shell-state` re-exports each module for backward compat;
new code imports from `mere_ux_events::*` directly.

## Status

Pre-1.0. Initial extraction with all four modules. Future work: more
canonical probes; richer apparatus integration so the live event
stream surfaces in-app.
