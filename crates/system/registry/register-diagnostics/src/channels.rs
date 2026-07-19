// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Diagnostic channel name catalog — the canonical `&'static str`
//! identifiers for every diagnostic channel the runtime emits into.
//!
//! These were originally declared as `pub(crate) const` in the shell-side
//! `shell/desktop/runtime/registries/mod.rs` (lines 213-581 pre-Slice-53)
//! but were referenced *both* by that runtime body (which calls
//! `registry.register_channel(CHANNEL_X, descriptor)`) and by the
//! canonical descriptor file at `registries/atomic/diagnostics.rs` (which
//! held the `DiagnosticChannelDescriptor` literals keyed by the same
//! constants). The cross-file pairing was the keystone blocker for
//! extracting `register-diagnostics` and ~7 other registry crates that
//! emit on these channels (`mod-loader`, `action`, `agent`, `identity`,
//! `input`, `theme`, `workflow`, `workbench-surface`).
//!
//! Slice 53 promotes the catalog into this crate. The shell-side runtime
//! re-exports the lot via `pub(crate) use register_diagnostics::channels::*;`
//! so existing call sites resolve unchanged. Visibility was widened
//! `pub(crate) → pub` per proposal §6.
//!
//! Adding a channel: add a `pub const CHANNEL_X: &str = "namespace.name";`
//! here, then register a matching `DiagnosticChannelDescriptor` in
//! [`super::descriptor`]. The `namespace:name` key form follows the
//! CLAUDE.md general code rule.

pub const CHANNEL_PROTOCOL_RESOLVE_STARTED: &str = "registry.protocol.resolve_started";
pub const CHANNEL_PROTOCOL_RESOLVE_SUCCEEDED: &str = "registry.protocol.resolve_succeeded";
pub const CHANNEL_PROTOCOL_RESOLVE_FAILED: &str = "registry.protocol.resolve_failed";
pub const CHANNEL_PROTOCOL_RESOLVE_FALLBACK_USED: &str = "registry.protocol.fallback_used";
pub const CHANNEL_VIEWER_SELECT_STARTED: &str = "registry.viewer.select_started";
pub const CHANNEL_VIEWER_SELECT_SUCCEEDED: &str = "registry.viewer.select_succeeded";
pub const CHANNEL_VIEWER_FALLBACK_USED: &str = "registry.viewer.fallback_used";
pub const CHANNEL_VIEWER_SURFACE_ALLOCATE_FAILED: &str = "registry.viewer.surface_allocate_failed";
pub const CHANNEL_VIEWER_FALLBACK_WRY_FEATURE_DISABLED: &str =
    "registry.viewer.fallback_wry_feature_disabled";
pub const CHANNEL_VIEWER_FALLBACK_WRY_CAPABILITY_MISSING: &str =
    "registry.viewer.fallback_wry_capability_missing";
pub const CHANNEL_VIEWER_FALLBACK_WRY_DISABLED_BY_PREFERENCE: &str =
    "registry.viewer.fallback_wry_disabled_by_preference";
pub const CHANNEL_VIEWER_CAPABILITY_PARTIAL: &str = "registry.viewer.capability_partial";
pub const CHANNEL_VIEWER_CAPABILITY_NONE: &str = "registry.viewer.capability_none";
pub const CHANNEL_SURFACE_CONFORMANCE_PARTIAL: &str = "registry.surface.conformance_partial";
pub const CHANNEL_SURFACE_CONFORMANCE_NONE: &str = "registry.surface.conformance_none";
pub const CHANNEL_ACTION_EXECUTE_STARTED: &str = "registry.action.execute_started";
pub const CHANNEL_ACTION_EXECUTE_SUCCEEDED: &str = "registry.action.execute_succeeded";
pub const CHANNEL_ACTION_EXECUTE_FAILED: &str = "registry.action.execute_failed";
pub const CHANNEL_INPUT_BINDING_RESOLVED: &str = "registry.input.binding_resolved";
pub const CHANNEL_INPUT_BINDING_MISSING: &str = "registry.input.binding_missing";
pub const CHANNEL_INPUT_BINDING_CONFLICT: &str = "registry.input.binding_conflict";
pub const CHANNEL_INPUT_BINDING_REBOUND: &str = "registry.input.binding_rebound";
pub const CHANNEL_RENDERER_ATTACH: &str = "registry.renderer.attach";
pub const CHANNEL_RENDERER_DETACH: &str = "registry.renderer.detach";
pub const CHANNEL_LENS_RESOLVE_SUCCEEDED: &str = "registry.lens.resolve_succeeded";
pub const CHANNEL_LENS_RESOLVE_FAILED: &str = "registry.lens.resolve_failed";
pub const CHANNEL_LENS_FALLBACK_USED: &str = "registry.lens.fallback_used";
pub const CHANNEL_IDENTITY_SIGN_STARTED: &str = "registry.identity.sign_started";
pub const CHANNEL_IDENTITY_SIGN_SUCCEEDED: &str = "registry.identity.sign_succeeded";
pub const CHANNEL_IDENTITY_SIGN_FAILED: &str = "registry.identity.sign_failed";
pub const CHANNEL_IDENTITY_VERIFY_STARTED: &str = "registry.identity.verify_started";
pub const CHANNEL_IDENTITY_VERIFY_SUCCEEDED: &str = "registry.identity.verify_succeeded";
pub const CHANNEL_IDENTITY_VERIFY_FAILED: &str = "registry.identity.verify_failed";
pub const CHANNEL_IDENTITY_KEY_UNAVAILABLE: &str = "registry.identity.key_unavailable";
pub const CHANNEL_IDENTITY_KEY_LOADED: &str = "registry.identity.key_loaded";
pub const CHANNEL_IDENTITY_KEY_GENERATED: &str = "registry.identity.key_generated";
pub const CHANNEL_IDENTITY_TRUST_STORE_LOAD_FAILED: &str =
    "registry.identity.trust_store_load_failed";
pub const CHANNEL_DIAGNOSTICS_CHANNEL_REGISTERED: &str = "registry.diagnostics.channel_registered";
pub const CHANNEL_DIAGNOSTICS_CONFIG_CHANGED: &str = "registry.diagnostics.config_changed";
pub const CHANNEL_INVARIANT_TIMEOUT: &str = "registry.invariant.timeout";
pub const CHANNEL_MOD_LOAD_STARTED: &str = "registry.mod.load_started";
pub const CHANNEL_MOD_LOAD_SUCCEEDED: &str = "registry.mod.load_succeeded";
pub const CHANNEL_MOD_LOAD_FAILED: &str = "registry.mod.load_failed";
pub const CHANNEL_MOD_ROLLBACK_SUCCEEDED: &str = "registry.mod.rollback_succeeded";
pub const CHANNEL_MOD_ROLLBACK_FAILED: &str = "registry.mod.rollback_failed";
pub const CHANNEL_MOD_QUARANTINED: &str = "registry.mod.quarantined";
pub const CHANNEL_MOD_UNLOAD_FAILED: &str = "registry.mod.unload_failed";
pub const CHANNEL_MOD_DEPENDENCY_MISSING: &str = "registry.mod.dependency_missing";
pub const CHANNEL_STARTUP_CONFIG_SNAPSHOT: &str = "startup.config.snapshot";
pub const CHANNEL_STARTUP_PERSISTENCE_OPEN_STARTED: &str = "startup.persistence.open_started";
pub const CHANNEL_STARTUP_PERSISTENCE_OPEN_SUCCEEDED: &str = "startup.persistence.open_succeeded";
pub const CHANNEL_STARTUP_PERSISTENCE_OPEN_FAILED: &str = "startup.persistence.open_failed";
pub const CHANNEL_STARTUP_PERSISTENCE_OPEN_TIMEOUT: &str = "startup.persistence.open_timeout";
pub const CHANNEL_PERSISTENCE_RECOVER_SUCCEEDED: &str = "persistence.recover.succeeded";
pub const CHANNEL_PERSISTENCE_RECOVER_FAILED: &str = "persistence.recover.failed";
pub const CHANNEL_STARTUP_VERSE_INIT_MODE: &str = "startup.verse.init_mode";
pub const CHANNEL_STARTUP_VERSE_INIT_SUCCEEDED: &str = "startup.verse.init_succeeded";
pub const CHANNEL_STARTUP_VERSE_INIT_FAILED: &str = "startup.verse.init_failed";
pub const CHANNEL_STARTUP_SELFCHECK_REGISTRIES_LOADED: &str = "startup.selfcheck.registries_loaded";
pub const CHANNEL_STARTUP_SELFCHECK_CHANNELS_COMPLETE: &str = "startup.selfcheck.channels_complete";
pub const CHANNEL_STARTUP_SELFCHECK_CHANNELS_INCOMPLETE: &str =
    "startup.selfcheck.channels_incomplete";
pub const CHANNEL_UI_HISTORY_MANAGER_LIMIT: &str = "ui.history_manager.limit_applied";
pub const CHANNEL_HISTORY_TRAVERSAL_RECORDED: &str = "history.traversal.recorded";
pub const CHANNEL_HISTORY_TRAVERSAL_RECORD_FAILED: &str = "history.traversal.record_failed";
pub const CHANNEL_HISTORY_ARCHIVE_DISSOLVED_APPENDED: &str = "history.archive.dissolved_appended";
pub const CHANNEL_HISTORY_ARCHIVE_CLEAR_FAILED: &str = "history.archive.clear_failed";
pub const CHANNEL_HISTORY_ARCHIVE_EXPORT_FAILED: &str = "history.archive.export_failed";
pub const CHANNEL_HISTORY_TIMELINE_PREVIEW_ENTERED: &str = "history.timeline.preview_entered";
pub const CHANNEL_HISTORY_TIMELINE_PREVIEW_EXITED: &str = "history.timeline.preview_exited";
pub const CHANNEL_HISTORY_TIMELINE_PREVIEW_ISOLATION_VIOLATION: &str =
    "history.timeline.preview_isolation_violation";
pub const CHANNEL_HISTORY_TIMELINE_REPLAY_STARTED: &str = "history.timeline.replay_started";
pub const CHANNEL_HISTORY_TIMELINE_REPLAY_SUCCEEDED: &str = "history.timeline.replay_succeeded";
pub const CHANNEL_HISTORY_TIMELINE_REPLAY_FAILED: &str = "history.timeline.replay_failed";
pub const CHANNEL_HISTORY_TIMELINE_RETURN_TO_PRESENT_FAILED: &str =
    "history.timeline.return_to_present_failed";
pub const CHANNEL_UI_CLIPBOARD_COPY_FAILED: &str = "ui.clipboard.copy_failed";
pub const CHANNEL_UI_GRAPH_CAMERA_REQUEST_BLOCKED: &str = "runtime.ui.graph.camera_request_blocked";
pub const CHANNEL_UI_GRAPH_KEYBOARD_ZOOM_BLOCKED: &str = "runtime.ui.graph.keyboard_zoom_blocked";
pub const CHANNEL_UI_GRAPH_CAMERA_FIT_BLOCKED_ZERO_VIEW: &str =
    "runtime.ui.graph.camera_fit_blocked_zero_view";
pub const CHANNEL_UI_GRAPH_FIT_SELECTION_FALLBACK_TO_FIT: &str =
    "runtime.ui.graph.fit_selection_fallback_to_fit";
pub const CHANNEL_UI_GRAPH_FIT_GRAPHLET_FALLBACK_TO_FIT: &str =
    "runtime.ui.graph.fit_graphlet_fallback_to_fit";
pub const CHANNEL_UI_GRAPH_CAMERA_FIT_BLOCKED_NO_BOUNDS: &str =
    "runtime.ui.graph.camera_fit_blocked_no_bounds";
pub const CHANNEL_UI_GRAPH_CAMERA_FIT_DEFERRED_NO_METADATA: &str =
    "runtime.ui.graph.camera_fit_deferred_no_metadata";
pub const CHANNEL_UI_GRAPH_SELECTION_AMBIGUOUS_HIT: &str =
    "runtime.ui.graph.selection_ambiguous_hit";
pub const CHANNEL_UI_GRAPH_WHEEL_ZOOM_NOT_CAPTURED: &str =
    "runtime.ui.graph.wheel_zoom_not_captured";
pub const CHANNEL_UI_GRAPH_KEYBOARD_ZOOM_BLOCKED_NO_METADATA: &str =
    "runtime.ui.graph.keyboard_zoom_blocked_no_metadata";
pub const CHANNEL_UI_GRAPH_CAMERA_ZOOM_DEFERRED_NO_METADATA: &str =
    "runtime.ui.graph.camera_zoom_deferred_no_metadata";
pub const CHANNEL_UI_GRAPH_WHEEL_ZOOM_DEFERRED_NO_METADATA: &str =
    "runtime.ui.graph.wheel_zoom_deferred_no_metadata";
pub const CHANNEL_UI_GRAPH_LASSO_BLOCKED_NO_STATE: &str = "runtime.ui.graph.lasso_blocked_no_state";
pub const CHANNEL_UI_GRAPH_EVENT_BLOCKED_NO_STATE: &str = "runtime.ui.graph.event_blocked_no_state";
pub const CHANNEL_UI_GRAPH_LAYOUT_SYNC_BLOCKED_NO_STATE: &str =
    "runtime.ui.graph.layout_sync_blocked_no_state";
pub const CHANNEL_UI_GRAPH_WHEEL_ZOOM_BLOCKED_INVALID_FACTOR: &str =
    "runtime.ui.graph.wheel_zoom_blocked_invalid_factor";
pub const CHANNEL_UI_GRAPH_CAMERA_COMMAND_BLOCKED_MISSING_TARGET_VIEW: &str =
    "runtime.ui.graph.camera_command_blocked_missing_target_view";
pub const CHANNEL_UI_GRAPH_VIEW_REGION_MUTATION_APPLIED: &str =
    "runtime.ui.graph.view_region_mutation_applied";
pub const CHANNEL_UI_GRAPH_VIEW_TRANSFER_SUCCEEDED: &str =
    "runtime.ui.graph.view_transfer_succeeded";
pub const CHANNEL_UI_GRAPH_VIEW_TRANSFER_BLOCKED: &str = "runtime.ui.graph.view_transfer_blocked";
pub const CHANNEL_UI_COMMAND_BAR_COMMAND_PALETTE_REQUESTED: &str =
    "runtime.ui.command_bar.command_palette_requested";
pub const CHANNEL_UI_COMMAND_BAR_WORKBENCH_COMMAND_REQUESTED: &str =
    "runtime.ui.command_bar.workbench_command.requested";
pub const CHANNEL_UI_COMMAND_BAR_WORKBENCH_COMMAND_EXECUTED: &str =
    "runtime.ui.command_bar.workbench_command.executed";
pub const CHANNEL_UI_COMMAND_BAR_WORKBENCH_COMMAND_BLOCKED_BY_FOCUS: &str =
    "runtime.ui.command_bar.workbench_command.blocked_by_focus";
pub const CHANNEL_UI_COMMAND_SURFACE_ROUTE_RESOLVED: &str =
    "runtime.ui.command_surface.route_resolved";
pub const CHANNEL_UI_COMMAND_SURFACE_ROUTE_BLOCKED: &str =
    "runtime.ui.command_surface.route_blocked";
pub const CHANNEL_UI_COMMAND_SURFACE_ROUTE_FALLBACK: &str =
    "runtime.ui.command_surface.route_fallback";
pub const CHANNEL_UI_COMMAND_SURFACE_ROUTE_NO_TARGET: &str =
    "runtime.ui.command_surface.route_no_target";
pub const CHANNEL_UI_COMMAND_BAR_NAV_ACTION_REQUESTED: &str =
    "runtime.ui.command_bar.nav_action.requested";
pub const CHANNEL_UI_COMMAND_BAR_NAV_ACTION_BLOCKED: &str =
    "runtime.ui.command_bar.nav_action.blocked";
pub const CHANNEL_UI_COMMAND_BAR_NAV_ACTION_NO_TARGET: &str =
    "runtime.ui.command_bar.nav_action.no_target";
pub const CHANNEL_HOST_WEBDRIVER_BROWSER_ACTION_REQUESTED: &str =
    "runtime.host.webdriver.browser_action.requested";
pub const CHANNEL_HOST_WEBDRIVER_BROWSER_ACTION_MISSING_WEBVIEW: &str =
    "runtime.host.webdriver.browser_action.missing_webview";
pub const CHANNEL_HOST_WEBDRIVER_LOAD_URL_REQUESTED: &str =
    "runtime.host.webdriver.load_url.requested";
pub const CHANNEL_HOST_WEBDRIVER_LOAD_URL_MISSING_WEBVIEW: &str =
    "runtime.host.webdriver.load_url.missing_webview";
pub const CHANNEL_HOST_WEBDRIVER_LOAD_STATUS_BLOCKED: &str =
    "runtime.host.webdriver.load_status.blocked";
pub const CHANNEL_UI_OMNIBAR_PROVIDER_MAILBOX_REQUEST_STARTED: &str =
    "runtime.ui.omnibar.provider_mailbox.request_started";
pub const CHANNEL_UI_OMNIBAR_PROVIDER_MAILBOX_APPLIED: &str =
    "runtime.ui.omnibar.provider_mailbox.applied";
pub const CHANNEL_UI_OMNIBAR_PROVIDER_MAILBOX_FAILED: &str =
    "runtime.ui.omnibar.provider_mailbox.failed";
pub const CHANNEL_UI_OMNIBAR_PROVIDER_MAILBOX_STALE: &str =
    "runtime.ui.omnibar.provider_mailbox.stale";
pub const CHANNEL_RUNTIME_CACHE_HIT: &str = "runtime.cache.hit";
pub const CHANNEL_RUNTIME_CACHE_MISS: &str = "runtime.cache.miss";
pub const CHANNEL_RUNTIME_CACHE_INSERT: &str = "runtime.cache.insert";
pub const CHANNEL_RUNTIME_CACHE_EVICTION: &str = "runtime.cache.eviction";
pub const CHANNEL_UI_GRAPH_KEYBOARD_PAN_BLOCKED_FIT_LOCK: &str =
    "runtime.ui.graph.keyboard_pan_blocked_fit_lock";
pub const CHANNEL_UI_GRAPH_KEYBOARD_PAN_BLOCKED_INACTIVE_VIEW: &str =
    "runtime.ui.graph.keyboard_pan_blocked_inactive_view";
pub const CHANNEL_VERSE_PREINIT_CALL: &str = "verse.preinit.call";
pub const CHANNEL_VERSE_SYNC_UNIT_SENT: &str = "verse.sync.unit_sent";
pub const CHANNEL_VERSE_SYNC_UNIT_RECEIVED: &str = "verse.sync.unit_received";
pub const CHANNEL_VERSE_SYNC_INTENT_APPLIED: &str = "verse.sync.intent_applied";
pub const CHANNEL_VERSE_SYNC_ACCESS_DENIED: &str = "verse.sync.access_denied";
pub const CHANNEL_VERSE_SYNC_CONNECTION_REJECTED: &str = "verse.sync.connection_rejected";
pub const CHANNEL_VERSE_SYNC_IDENTITY_GENERATED: &str = "verse.sync.identity_generated";
pub const CHANNEL_VERSE_SYNC_CONFLICT_DETECTED: &str = "verse.sync.conflict_detected";
pub const CHANNEL_VERSE_SYNC_CONFLICT_RESOLVED: &str = "verse.sync.conflict_resolved";
pub const CHANNEL_NOSTR_CAPABILITY_DENIED: &str = "mod.nostrcore.capability_denied";
pub const CHANNEL_NOSTR_SIGN_REQUEST_DENIED: &str = "mod.nostrcore.sign_request_denied";
pub const CHANNEL_NOSTR_RELAY_PUBLISH_FAILED: &str = "mod.nostrcore.relay_publish_failed";
pub const CHANNEL_NOSTR_RELAY_SUBSCRIPTION_FAILED: &str = "mod.nostrcore.relay_subscription_failed";
pub const CHANNEL_NOSTR_RELAY_CONNECT_STARTED: &str = "mod.nostrcore.relay_connect_started";
pub const CHANNEL_NOSTR_RELAY_CONNECT_SUCCEEDED: &str = "mod.nostrcore.relay_connect_succeeded";
pub const CHANNEL_NOSTR_RELAY_CONNECT_FAILED: &str = "mod.nostrcore.relay_connect_failed";
pub const CHANNEL_NOSTR_RELAY_DISCONNECTED: &str = "mod.nostrcore.relay_disconnected";
pub const CHANNEL_NOSTR_INTENT_REJECTED: &str = "mod.nostrcore.intent_rejected";
pub const CHANNEL_NOSTR_SECURITY_VIOLATION: &str = "mod.nostrcore.security_violation";
pub const CHANNEL_COMPOSITOR_GL_STATE_VIOLATION: &str = "compositor.gl_state_violation";
pub const CHANNEL_COMPOSITOR_CONTENT_PASS_REGISTERED: &str = "compositor.content_pass_registered";
pub const CHANNEL_COMPOSITOR_OVERLAY_PASS_REGISTERED: &str = "compositor.overlay_pass_registered";
pub const CHANNEL_COMPOSITOR_PASS_ORDER_VIOLATION: &str = "compositor.pass_order_violation";
pub const CHANNEL_COMPOSITOR_INVALID_TILE_RECT: &str = "compositor.invalid_tile_rect";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_CHAOS: &str = "diagnostics.compositor_chaos";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_CHAOS_PASS: &str = "diagnostics.compositor_chaos.pass";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_CHAOS_FAIL: &str = "diagnostics.compositor_chaos.fail";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PROBE: &str = "diagnostics.compositor_bridge_probe";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PROBE_FAILED_FRAME: &str =
    "diagnostics.compositor_bridge_probe.failed_frame";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_CALLBACK_US_SAMPLE: &str =
    "diagnostics.compositor_bridge_probe.callback_us_sample";
pub const CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PRESENTATION_US_SAMPLE: &str =
    "diagnostics.compositor_bridge_probe.presentation_us_sample";
pub const CHANNEL_COMPOSITOR_FOCUS_ACTIVATION_DEFERRED: &str =
    "compositor.focus_activation.deferred";
pub const CHANNEL_COMPOSITOR_OVERLAY_STYLE_RECT_STROKE: &str =
    "compositor.overlay.style.rect_stroke";
pub const CHANNEL_COMPOSITOR_OVERLAY_STYLE_CHROME_ONLY: &str =
    "compositor.overlay.style.chrome_only";
pub const CHANNEL_COMPOSITOR_OVERLAY_MODE_COMPOSITED_TEXTURE: &str =
    "compositor.overlay.mode.composited_texture";
pub const CHANNEL_COMPOSITOR_OVERLAY_MODE_NATIVE_OVERLAY: &str =
    "compositor.overlay.mode.native_overlay";
pub const CHANNEL_COMPOSITOR_OVERLAY_MODE_EMBEDDED_HOST: &str =
    "compositor.overlay.mode.embedded_host";
pub const CHANNEL_COMPOSITOR_OVERLAY_MODE_PLACEHOLDER: &str = "compositor.overlay.mode.placeholder";
pub const CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_INTERACTION_MENU: &str =
    "compositor.overlay.native.suppressed.interaction_menu";
pub const CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_HELP_PANEL: &str =
    "compositor.overlay.native.suppressed.help_panel";
pub const CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_RADIAL_MENU: &str =
    "compositor.overlay.native.suppressed.radial_menu";
pub const CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_TILE_DRAG: &str =
    "compositor.overlay.native.suppressed.tile_drag";
pub const CHANNEL_COMPOSITOR_REPLAY_SAMPLE_RECORDED: &str = "compositor.replay.sample_recorded";
pub const CHANNEL_COMPOSITOR_REPLAY_ARTIFACT_RECORDED: &str = "compositor.replay.artifact_recorded";
pub const CHANNEL_COMPOSITOR_DIFFERENTIAL_CONTENT_COMPOSED: &str =
    "compositor.differential.content_composed";
pub const CHANNEL_COMPOSITOR_DIFFERENTIAL_CONTENT_SKIPPED: &str =
    "compositor.differential.content_skipped";
pub const CHANNEL_COMPOSITOR_DIFFERENTIAL_FALLBACK_NO_PRIOR_SIGNATURE: &str =
    "compositor.differential.fallback_no_prior_signature";
pub const CHANNEL_COMPOSITOR_DIFFERENTIAL_FALLBACK_SIGNATURE_CHANGED: &str =
    "compositor.differential.fallback_signature_changed";
pub const CHANNEL_COMPOSITOR_DIFFERENTIAL_SKIP_RATE_SAMPLE: &str =
    "compositor.differential.skip_rate_basis_points";
pub const CHANNEL_COMPOSITOR_TILE_ACTIVITY: &str = "compositor:tile_activity";
pub const CHANNEL_COMPOSITOR_OVERLAY_LIFECYCLE_INDICATOR: &str =
    "compositor:overlay_lifecycle_indicator";
pub const CHANNEL_COMPOSITOR_LENS_OVERLAY_APPLIED: &str = "compositor:lens_overlay_applied";
pub const CHANNEL_COMPOSITOR_CONTENT_CULLED_OFFVIEWPORT: &str =
    "compositor.content.culled_offviewport";
pub const CHANNEL_COMPOSITOR_DEGRADATION_GPU_PRESSURE: &str = "compositor.degradation.gpu_pressure";
pub const CHANNEL_COMPOSITOR_DEGRADATION_PLACEHOLDER_MODE: &str =
    "compositor.degradation.placeholder_mode";
pub const CHANNEL_COMPOSITOR_RESOURCE_REUSE_CONTEXT_HIT: &str =
    "compositor.resource_reuse.context_hit";
pub const CHANNEL_COMPOSITOR_RESOURCE_REUSE_CONTEXT_MISS: &str =
    "compositor.resource_reuse.context_miss";
pub const CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_SHARED_WGPU: &str =
    "compositor.viewer_surface_path.shared_wgpu";
pub const CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_CALLBACK_FALLBACK: &str =
    "compositor.viewer_surface_path.callback_fallback";
pub const CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_MISSING_SURFACE: &str =
    "compositor.viewer_surface_path.missing_surface";
pub const CHANNEL_COMPOSITOR_OVERLAY_BATCH_SIZE_SAMPLE: &str =
    "compositor.overlay.batch_size_sample";
pub const CHANNEL_UX_DISPATCH_STARTED: &str = "ux:dispatch_started";
pub const CHANNEL_UX_DISPATCH_PHASE: &str = "ux:dispatch_phase";
pub const CHANNEL_UX_DISPATCH_CONSUMED: &str = "ux:dispatch_consumed";
pub const CHANNEL_UX_DISPATCH_DEFAULT_PREVENTED: &str = "ux:dispatch_default_prevented";
pub const CHANNEL_UX_NAVIGATION_TRANSITION: &str = "ux:navigation_transition";
pub const CHANNEL_UX_NAVIGATION_VIOLATION: &str = "ux:navigation_violation";
pub const CHANNEL_UX_ARRANGEMENT_PROJECTION_HEALTH: &str = "ux:arrangement_projection_health";
pub const CHANNEL_UX_ARRANGEMENT_MISSING_FAMILY_FALLBACK: &str =
    "ux:arrangement_missing_family_fallback";
pub const CHANNEL_UX_ARRANGEMENT_DURABILITY_TRANSITION: &str =
    "ux:arrangement_durability_transition";
pub const CHANNEL_UX_FOCUS_CAPTURE_ENTER: &str = "ux:focus_capture_enter";
pub const CHANNEL_UX_FOCUS_CAPTURE_EXIT: &str = "ux:focus_capture_exit";
pub const CHANNEL_UX_FOCUS_RETURN_FALLBACK: &str = "ux:focus_return_fallback";
pub const CHANNEL_UX_FOCUS_REALIZATION_MISMATCH: &str = "ux:focus_realization_mismatch";
pub const CHANNEL_UX_EMBEDDED_FOCUS_RECLAIM: &str = "ux:embedded_focus_reclaim";
pub const CHANNEL_UX_STRUCTURAL_VIOLATION: &str = "ux:structural_violation";
pub const CHANNEL_UX_CONTRACT_WARNING: &str = "ux:contract_warning";
pub const CHANNEL_UX_TREE_BUILD: &str = "ux:tree_build";
pub const CHANNEL_UX_OPEN_DECISION_PATH: &str = "ux:open_decision_path";
pub const CHANNEL_UX_OPEN_DECISION_REASON: &str = "ux:open_decision_reason";
pub const CHANNEL_UX_RADIAL_OVERFLOW: &str = "ux:radial_overflow";
pub const CHANNEL_UX_RADIAL_LAYOUT: &str = "ux:radial_layout";
pub const CHANNEL_UX_RADIAL_LABEL_COLLISION: &str = "ux:radial_label_collision";
pub const CHANNEL_UX_RADIAL_MODE_FALLBACK: &str = "ux:radial_mode_fallback";
pub const CHANNEL_UX_TREE_SNAPSHOT_BUILT: &str = "ux:tree_snapshot_built";
pub const CHANNEL_UX_SNAPSHOT_WRITTEN: &str = "ux:snapshot_written";
pub const CHANNEL_UX_PRESENTATION_BOUNDS_MISSING: &str = "ux:presentation_bounds_missing";
pub const CHANNEL_UX_LAYOUT_GUTTER_DETECTED: &str = "ux:layout_gutter_detected";
pub const CHANNEL_UX_LAYOUT_OVERLAP_DETECTED: &str = "ux:layout_overlap_detected";
pub const CHANNEL_UX_LAYOUT_CONSTRAINT_CONFLICT: &str = "ux:layout_constraint_conflict";
pub const CHANNEL_UX_LAYOUT_CONSTRAINT_DRIFT: &str = "ux:layout_constraint_drift";
pub const CHANNEL_UX_CONFIG_MODE_ENTERED: &str = "ux:config_mode_entered";
pub const CHANNEL_UX_FIRST_USE_PROMPT_SHOWN: &str = "ux:first_use_prompt_shown";
pub const CHANNEL_COMPOSITOR_PAINT_NOT_CONFIRMED: &str = "compositor.paint_not_confirmed";
pub const CHANNEL_COMPOSITOR_NATIVE_OVERLAY_RECT_MISMATCH: &str =
    "compositor.native_overlay_rect_mismatch";
pub const CHANNEL_UX_PROBE_REGISTERED: &str = "ux:probe_registered";
pub const CHANNEL_UX_PROBE_DISABLED: &str = "ux:probe_disabled";
pub const CHANNEL_UX_FACET_FILTER_APPLIED: &str = "ux:facet_filter_applied";
pub const CHANNEL_UX_FACET_FILTER_CLEARED: &str = "ux:facet_filter_cleared";
pub const CHANNEL_UX_FACET_FILTER_INVALID_QUERY: &str = "ux:facet_filter_invalid_query";
pub const CHANNEL_UX_FACET_FILTER_TYPE_MISMATCH: &str = "ux:facet_filter_type_mismatch";
pub const CHANNEL_UX_FACET_FILTER_EVAL_FAILURE: &str = "ux:facet_filter_eval_failure";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_PUBLISHED: &str = "register.signal_routing.published";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_UNROUTED: &str = "register.signal_routing.unrouted";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_FAILED: &str = "register.signal_routing.failed";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_QUEUE_DEPTH: &str = "register.signal_routing.queue_depth";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED: &str = "register.signal_routing.lagged";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_MOD_WORKFLOW_ROUTED: &str =
    "register.signal_routing.mod_workflow_routed";
pub const CHANNEL_REGISTER_SIGNAL_ROUTING_SUBSYSTEM_HEALTH_PROPAGATED: &str =
    "register.signal_routing.subsystem_health_propagated";
pub const CHANNEL_WORKBENCH_SURFACE_PROFILE_ACTIVATED: &str =
    "registry.workbench_surface.profile_activated";
pub const CHANNEL_CANVAS_PROFILE_ACTIVATED: &str = "registry.canvas.profile_activated";
pub const CHANNEL_CANVAS_FRAME_AFFINITY_CHANGED: &str = "registry.canvas.frame_affinity_changed";
pub const CHANNEL_PHYSICS_PROFILE_ACTIVATED: &str = "registry.physics_profile.activated";
pub const CHANNEL_LAYOUT_COMPUTE_STARTED: &str = "registry.layout.compute_started";
pub const CHANNEL_LAYOUT_COMPUTE_SUCCEEDED: &str = "registry.layout.compute_succeeded";
pub const CHANNEL_LAYOUT_COMPUTE_FAILED: &str = "registry.layout.compute_failed";
pub const CHANNEL_LAYOUT_FALLBACK_USED: &str = "registry.layout.fallback_used";
pub const CHANNEL_LAYOUT_DOMAIN_PROFILE_RESOLVED: &str = "registry.layout_domain.profile_resolved";
pub const CHANNEL_PRESENTATION_PROFILE_RESOLVED: &str = "registry.presentation.profile_resolved";
pub const CHANNEL_THEME_ACTIVATED: &str = "registry.theme.activated";
pub const CHANNEL_AGENT_SPAWNED: &str = "registry.agent.spawned";
pub const CHANNEL_AGENT_INTENT_DROPPED: &str = "registry.agent.intent_dropped";
pub const CHANNEL_WORKFLOW_ACTIVATED: &str = "registry.workflow.activated";
pub const CHANNEL_KNOWLEDGE_INDEX_UPDATED: &str = "registry.knowledge.index_updated";
pub const CHANNEL_KNOWLEDGE_TAG_VALIDATION_WARN: &str = "registry.knowledge.tag_validation_warn";
pub const CHANNEL_KNOWLEDGE_PLACEMENT_ANCHOR_SELECTED: &str =
    "registry.knowledge.placement_anchor_selected";
pub const CHANNEL_KNOWLEDGE_CLASSIFICATION_CLUSTERING_APPLIED: &str =
    "registry.knowledge.classification_clustering_applied";
pub const CHANNEL_INDEX_SEARCH: &str = "registry.index.search";

pub const CHANNEL_SYSTEM_TASK_BUDGET_BACKPRESSURE: &str = "system:task_budget:backpressure";
pub const CHANNEL_SYSTEM_TASK_BUDGET_WORKER_SUSPENDED: &str = "system:task_budget:worker_suspended";
pub const CHANNEL_SYSTEM_TASK_BUDGET_WORKER_RESUMED: &str = "system:task_budget:worker_resumed";
pub const CHANNEL_SYSTEM_TASK_BUDGET_QUEUE_DEPTH: &str = "system:task_budget:queue_depth";
