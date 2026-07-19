// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::channels::*;

#[test]
fn diagnostics_registry_seeds_phase_contract_channels() {
    let registry = DiagnosticsRegistry::default();
    assert!(registry.has_channel(CHANNEL_PROTOCOL_RESOLVE_STARTED));
    assert!(registry.has_channel(CHANNEL_ACTION_EXECUTE_STARTED));
    assert!(registry.has_channel(CHANNEL_IDENTITY_SIGN_STARTED));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_GL_STATE_VIOLATION));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_CONTENT_PASS_REGISTERED));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_OVERLAY_PASS_REGISTERED));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_PASS_ORDER_VIOLATION));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_INVALID_TILE_RECT));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_SHARED_WGPU));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_OVERLAY_STYLE_RECT_STROKE));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_OVERLAY_MODE_NATIVE_OVERLAY));
    assert!(registry.has_channel(CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_INTERACTION_MENU));
    assert!(registry.has_channel(CHANNEL_VIEWER_FALLBACK_WRY_FEATURE_DISABLED));
    assert!(registry.has_channel(CHANNEL_VERSE_SYNC_UNIT_SENT));
}

#[test]
fn diagnostics_registry_declares_gl_state_violation_as_warn_severity() {
    let descriptor = phase3_required_channels()
        .iter()
        .find(|entry| entry.channel_id == CHANNEL_COMPOSITOR_GL_STATE_VIOLATION)
        .expect("phase3 channels must include compositor.gl_state_violation");

    assert_eq!(descriptor.severity, ChannelSeverity::Warn);
    assert_eq!(descriptor.schema_version, 1);
}

#[test]
fn diagnostics_registry_config_roundtrip() {
    let mut registry = DiagnosticsRegistry::default();
    let channel = CHANNEL_VIEWER_SELECT_STARTED;

    let updated = ChannelConfig {
        enabled: true,
        sample_rate: 0.5,
        retention_count: 32,
    };
    registry.set_config(channel, updated.clone());

    let loaded = registry.get_config(channel);
    assert_eq!(loaded.enabled, updated.enabled);
    assert!((loaded.sample_rate - updated.sample_rate).abs() < f32::EPSILON);
    assert_eq!(loaded.retention_count, updated.retention_count);
}

#[test]
fn diagnostics_registry_supports_dynamic_runtime_channel_registration() {
    let mut registry = DiagnosticsRegistry::default();
    let created = registry
        .register_runtime_channel(
            RuntimeChannelDescriptor::info(
                "agent.think.started",
                1,
                DiagnosticsChannelOwner {
                    source: DiagnosticsChannelSource::Agent,
                    owner_id: Some("agent:planner".to_string()),
                },
                Some("planner think loop started".to_string()),
            ),
            ChannelRegistrationPolicy::RejectConflict,
        )
        .expect("runtime channel registration should succeed");

    assert!(created);
    assert!(registry.has_channel("agent.think.started"));
}

#[test]
fn diagnostics_registry_tracks_auto_registered_orphan_channels() {
    let mut registry = DiagnosticsRegistry::default();

    assert!(registry.should_emit_channel("runtime.unknown.channel"));
    assert!(registry.should_emit_channel("runtime.unknown.channel"));

    let orphan_channels = registry.list_orphan_channels();
    assert_eq!(orphan_channels.len(), 1);
    assert_eq!(orphan_channels[0].0, "runtime.unknown.channel");
    assert_eq!(orphan_channels[0].1, 1);
}

#[test]
fn diagnostics_registry_rejects_conflicting_schema_on_reject_policy() {
    let mut registry = DiagnosticsRegistry::default();
    let result = registry.register_runtime_channel(
        RuntimeChannelDescriptor::info(
            CHANNEL_PROTOCOL_RESOLVE_STARTED,
            7,
            DiagnosticsChannelOwner::core(),
            None,
        ),
        ChannelRegistrationPolicy::RejectConflict,
    );

    assert!(matches!(
        result,
        Err(ChannelRegistrationError::Conflict { .. })
    ));
}

#[test]
fn diagnostics_registry_mod_namespace_enforcement_blocks_invalid_channel() {
    let mut registry = DiagnosticsRegistry::default();
    let result = registry.register_mod_channel(
        "planner",
        "agent.think.started",
        1,
        None,
        &[DiagnosticsCapability::RegisterChannels],
    );

    assert!(matches!(
        result,
        Err(ChannelRegistrationError::InvalidOwnership { .. })
    ));
}

#[test]
fn diagnostics_registry_invariant_watchdog_times_out_when_terminal_missing() {
    let mut registry = DiagnosticsRegistry::default();
    let _ = registry.register_invariant(
        DiagnosticsInvariant {
            invariant_id: "invariant.test.compute_finishes".to_string(),
            start_channel: "layout.compute_started".to_string(),
            terminal_channels: vec![
                "layout.compute_succeeded".to_string(),
                "layout.compute_failed".to_string(),
            ],
            timeout_ms: 10,
            owner: DiagnosticsChannelOwner::core(),
            enabled: true,
        },
        &[DiagnosticsCapability::RegisterInvariants],
    );

    let started_at = 100;
    let _ = registry.observe_channel_event("layout.compute_started", started_at);
    let violations = registry.sweep_invariants(started_at + 20);

    assert_eq!(violations.len(), 1);
    assert_eq!(
        violations[0].invariant_id,
        "invariant.test.compute_finishes".to_string()
    );
}

#[test]
fn diagnostics_registry_registers_phase5_sync_watchdog_invariants() {
    let registry = DiagnosticsRegistry::default();

    assert!(
        registry
            .invariants
            .contains_key(INVARIANT_VERSE_SYNC_RECEIVED_COMPLETES)
    );
    assert!(
        registry
            .invariants
            .contains_key(INVARIANT_VERSE_SYNC_SENT_COMPLETES)
    );
}

#[test]
fn diagnostics_registry_phase5_received_watchdog_clears_on_terminal_channel() {
    let mut registry = DiagnosticsRegistry::default();
    let started_at = 100;

    let _ = registry.observe_channel_event(CHANNEL_VERSE_SYNC_UNIT_RECEIVED, started_at);
    let _ = registry.observe_channel_event(CHANNEL_VERSE_SYNC_INTENT_APPLIED, started_at + 10);
    let violations = registry.sweep_invariants(started_at + 2_000);

    assert!(violations.is_empty());
}

#[test]
fn diagnostics_registry_phase5_sent_watchdog_times_out_without_terminal() {
    let mut registry = DiagnosticsRegistry::default();
    let started_at = 100;

    let _ = registry.observe_channel_event(CHANNEL_VERSE_SYNC_UNIT_SENT, started_at);
    let violations = registry.sweep_invariants(started_at + 2_100);

    assert!(
        violations
            .iter()
            .any(|entry| entry.invariant_id == INVARIANT_VERSE_SYNC_SENT_COMPLETES)
    );
}

#[test]
fn diagnostics_registry_attaches_structured_schema_to_high_value_contract_channels() {
    let registry = DiagnosticsRegistry::default();
    let channels = registry.list_channel_configs();

    for channel_id in [
        CHANNEL_PROTOCOL_RESOLVE_STARTED,
        CHANNEL_VIEWER_SELECT_SUCCEEDED,
        CHANNEL_ACTION_EXECUTE_FAILED,
        CHANNEL_IDENTITY_SIGN_FAILED,
        CHANNEL_RENDERER_ATTACH,
        CHANNEL_UI_COMMAND_SURFACE_ROUTE_RESOLVED,
        CHANNEL_UI_COMMAND_SURFACE_ROUTE_BLOCKED,
        CHANNEL_UI_COMMAND_SURFACE_ROUTE_FALLBACK,
        CHANNEL_UI_COMMAND_SURFACE_ROUTE_NO_TARGET,
    ] {
        let descriptor = channels
            .iter()
            .find_map(|(descriptor, _)| (descriptor.channel_id == channel_id).then_some(descriptor))
            .expect("channel should be registered");
        assert!(matches!(
            descriptor.payload_schema,
            DiagnosticPayloadSchema::Structured(ref fields) if !fields.is_empty()
        ));
    }
}

#[test]
fn diagnostics_registry_rejects_empty_structured_payload_schema() {
    let mut registry = DiagnosticsRegistry::default();
    let result = registry.register_runtime_channel(
        RuntimeChannelDescriptor::new(
            "runtime.invalid.schema",
            1,
            DiagnosticsChannelOwner::runtime(),
            Some("invalid".to_string()),
            ChannelSeverity::Info,
            DiagnosticPayloadSchema::Structured(Vec::new()),
            RetentionPolicy::Session,
            SamplingPolicy::All,
        ),
        ChannelRegistrationPolicy::RejectConflict,
    );

    assert!(matches!(
        result,
        Err(ChannelRegistrationError::InvalidSchema { .. })
    ));
}
