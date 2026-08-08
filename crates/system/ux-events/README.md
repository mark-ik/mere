# ux-events

Portable chrome-event taxonomy for the [mere](https://crates.io/crates/mere)
browser. A host emits a `UxEvent` whenever a chrome surface opens or dismisses
and whenever a host intent is dispatched; observers record those events, probes
assert invariants over them, and a bridge forwards them to a diagnostics-channel
registry.

It sits alongside two other event layers: code-level `tracing` spans, and the
`register-diagnostics` channel registry.

Depends on `chrome` (for `routing::ToolSurfaceReturnTarget`), `kernel` (for
`accessibility::SurfaceId`, `actions::ActionId`, `graph::NodeKey`,
`pane::PaneId`), and `serde`.

## Modules

| Module | Contents |
| --- | --- |
| `ux_observability` | `UxEvent`, `DismissReason`, `UxObserver`, `UxObservers`, `CountingObserver`, `RecordingObserver`; re-exports `kernel::accessibility::SurfaceId` |
| `ux_diagnostics` | `DiagnosticsSeverity`, `ChannelEmission`, `DiagnosticsChannelSink`, `event_channel`, `all_channel_ids`, `UxChannelObserver`, `NoopChannelSink`, `RecordingChannelSink` |
| `ux_probes` | `UxProbe`, `ProbeFailure`, `probe_as_observer`, and the four canonical probes |
| `command_surface_telemetry` | `CommandSurfaceTelemetry` plus its snapshot and counter types |

The crate root also exports `VERSION` and `STAGE`.

## UxEvent

```rust
pub enum UxEvent {
    SurfaceOpened { surface: SurfaceId },
    SurfaceDismissed { surface: SurfaceId, reason: DismissReason },
    ActionDispatched { action_id: ActionId, target: Option<NodeKey> },
    OpenNodeDispatched { node_key: NodeKey },
}
```

`DismissReason` is `Confirmed`, `Cancelled`, `Superseded`, or `Programmatic`.

Hosts register observers on a `UxObservers` collection and call
`UxObservers::emit`; every registered observer sees the event in registration
order. `UxObserver::observe` takes `&self`, so observers own their interior
mutability and are `Send + Sync`.

## Probes

| Probe | Invariant |
| --- | --- |
| `MutualExclusionProbe` | At most one modal-like surface open at a time |
| `OpenDismissBalanceProbe` | Every `SurfaceOpened` is eventually paired with a `SurfaceDismissed` |
| `ProductiveSelectionProbe` | A `Confirmed` dismissal is followed by a productive event, per a list of `ProductiveRule`s with `ProductiveOutcome` targets |
| `DestructiveActionGateProbe` | An `ActionDispatched` for a configured-destructive `ActionId` is preceded by a `Confirmed` `ConfirmDialog` dismissal |

Each implements `UxProbe` (`name`, `observe`, `drain_failures`) and is
registered on a `UxObservers` through `probe_as_observer(Arc<dyn UxProbe>)`.
`drain_failures` returns the `ProbeFailure`s recorded since the last call.

## Diagnostics bridge

`event_channel(&UxEvent) -> ChannelEmission` is the canonical mapping to a
`(channel_id, severity, note)` triple. Channel ids follow
`"ux.<surface>.<opened|dismissed>"`, plus `"ux.action.dispatched"` and
`"ux.open_node.dispatched"`. `all_channel_ids()` lists the ids a host
pre-registers at startup. A host implements `DiagnosticsChannelSink` over its
own registry and registers `UxChannelObserver::new(sink)`; `NoopChannelSink` and
`RecordingChannelSink` ship for hosts with no registry and for tests.

## Command-surface telemetry

`CommandSurfaceTelemetry` is a per-instance sink (no global) holding the latest
published `CommandSurfaceSemanticSnapshot` and a
`CommandSurfaceEventSequenceMetadata` counter block. Widgets call
`publish_snapshot` / `clear_snapshot` and the `note_route_*` /
`note_omnibar_mailbox_*` counters; readers call `latest_snapshot` and
`latest_event_sequence_metadata`. Counters use `saturating_add`.

## History

These four modules were extracted from `shell-state` on 2026-05-10.
`shell-state` re-exports each of them, so `shell_state::ux_observability::*`
still resolves; new code imports from `ux_events::*`.
