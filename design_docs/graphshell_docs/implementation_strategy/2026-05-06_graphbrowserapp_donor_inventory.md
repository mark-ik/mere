# GraphBrowserApp Donor Inventory

**Date**: 2026-05-06
**Status**: Seed inventory / migration gate
**Scope**: Classify old `repos/graphshell` `GraphBrowserApp` methods before any concrete `GraphBrowserApp` import into Mere.

This document is intentionally skeptical: old methods are evidence, not destiny. A method should migrate only when its durable responsibility fits a Mere owner and when the target boundary keeps portable crates free of concrete host, renderer, storage, or task-runtime handles.

## Classification Rules

| Class | Meaning | Migration rule |
| --- | --- | --- |
| Reducer | Mutates `GraphWorkspace` or graph truth through typed intents/deltas | Move only the pure state transition; emit `WorkspaceEffect` for services |
| Service glue | Calls persistence, journals, settings, Mnem, sync, route, diagnostics, or task traits | Prefer `app_state::services` or a narrow service module; keep concrete implementations outside app state |
| Host adapter | Translates webview/window/widget events into intents, or applies surface commands to host resources | Keep in future Graphshell host crates; never mutate graph truth directly |
| Platen projection | Converts workbench/frame/pane arrangement into plain renderable packets or graph-arrangement snapshots | Move plain projection/snapshot types to `platen`; graph mutation still goes through reducers/deltas |
| Inker policy | Chooses engines or surface contracts from URI/content/runtime context | Move policy to `inker`; Graphshell should ask through `EngineRouter` |
| Verso-Tile lifecycle | Allocates, focuses, retires, or acknowledges rendering surfaces | Move identity/lifecycle vocabulary to `verso-tile`; hosts apply commands |
| Obsolete residue | Exists only for old renderer ownership, compatibility plumbing, or direct desktop coupling | Do not migrate; replace with the new owner boundary |

## Seed Inventory

| Donor area | Representative methods | Classification | Mere direction |
| --- | --- | --- | --- |
| `app/runtime_lifecycle.rs` host-open and webview-created flow | `handle_host_open_request`, `plan_webview_created`, `apply_webview_created_plan`, `handle_webview_created`, `position_for_host_open` | Host adapter plus reducer/service split | Keep event parsing and host IDs in a host crate. Move only pure planning data if needed; node creation becomes graph-runtime intent(s), pane presentation becomes `WorkspaceEffect::RequestSurface`, and engine choice goes through `inker` route policy. |
| `app/runtime_lifecycle.rs` webview URL/history/scroll/title/crash flow | `plan_webview_url_change`, `apply_webview_url_change_plan`, `plan_webview_history_change`, `apply_webview_history_change_plan`, `plan_webview_scroll_change`, `apply_webview_scroll_change_plan`, `plan_webview_title_change`, `apply_webview_title_change_plan`, `plan_webview_crashed`, `apply_webview_crashed_plan` | Host adapter plus reducer | Preserve the plan/apply split, but replace webview-specific mutation with typed runtime intents over `GraphWorkspace`. Crash/blocking policy can become service/runtime state only after its portable fields are identified. |
| `app/runtime_lifecycle.rs` webview-node maps | `map_webview_to_node`, `unmap_webview`, `get_node_for_webview`, `get_webview_for_node`, `webview_node_mappings` | Host adapter / Verso-Tile lifecycle | Do not place renderer IDs in `GraphWorkspace`. Model active surfaces with `SurfaceHostId`, `SurfaceCommand`, and future `verso-tile` surface allocation state. |
| `app/composition/arrangement_graph_bridge.rs` | `ArrangementSnapshot`, `ArrangementGraphDelta`, `apply_arrangement_snapshot`, `outgoing_membership_nodes`, frame/group reconcilers | Platen projection plus reducer | Keep the good idea: plain snapshots crossing the boundary. Frame arrangement snapshot vocabulary now lives in `platen::workbench`, and hosted members project into `verso_tile::surface::SurfacePlacementPlan`; any future graph reconciliation should consume those snapshots through Graphshell reducers/journal, not private `GraphBrowserApp` helpers. |
| `app/persistence/persistence_facade.rs` persistence health and snapshots | `persistence_health_summary`, `check_periodic_snapshot`, `set_snapshot_interval_secs`, `take_snapshot`, `save_tile_layout_json`, `load_tile_layout_json` | Service glue | Existing Mere `WorkspaceRepository`, `SettingsStore`, `GraphMutationJournal`, and Mnem seams cover the durable direction. Add typed operations only when a donor method maps to a durable document, not for facade parity. |
| `app/persistence/persistence_facade.rs` sync/storage handles | `set_sync_command_tx`, `set_client_storage_manager`, `set_storage_interop_coordinator`, `request_sync_all_trusted_peers` | Service glue / obsolete residue | Do not import channels or manager handles into app state. Rebuild sync/storage interop around Mere protocol crates and service traits. |
| `app/app_ux/clip_capture.rs` clip inspector state | `open_clip_inspector`, `close_clip_inspector`, `update_clip_inspector_pointer_stack`, `clip_inspector_step_stack` | Reducer / app UX | Pure modal/action state belongs in `app_state::app_ux`; host capture payloads should enter as typed intents/effects. |
| `app/app_ux/clip_capture.rs` clip node creation | `create_clip_node_from_capture`, `create_clip_nodes_from_captures`, `create_clip_node_at_position` | Reducer plus service glue | Migrate only after clip payload/document shape is portable. Node creation goes through graph-runtime reducers; capture storage goes through Mnem or typed persistence. |
| `app/history.rs` and `app/history_runtime.rs` | undo/redo checkpointing, history preview cursors, timeline/archive queries | Reducer plus Mnem/persistence | Keep durable navigation memory on the typed mutation/journal lane. Archive/query surfaces belong behind Mnem/persistence traits, not direct store calls. |

## Current Migration Gate

Before importing any donor `GraphBrowserApp` method, add a row above or update an existing row with:

1. the donor method name and source module;
2. the intended owner crate/module;
3. whether it is reducer, service glue, host adapter, projection, route policy, surface lifecycle, or obsolete residue;
4. the test that proves the new Mere-side contract.

The preferred migration shape is to move narrow vocabulary and tests first, then wire service/host implementations later. Copying an old method body is a last resort, not the default path.