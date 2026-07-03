/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Diagnostics channel descriptor catalog + DiagnosticsRegistry.
//!
//! Extracted from `registries/atomic/diagnostics.rs` per Slice 53.
//! The keystone CHANNEL_* constants live in the sibling
//! [`super::channels`] module; this file imports them via the
//! `crate::channels::*` glob so descriptor literals continue to
//! reference channels by their original names without the
//! `shell::desktop::runtime::registries::` path.

use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::channels::*;

pub mod types;
pub use types::*;

pub mod catalog;
pub use catalog::*;

pub mod registry;
pub use registry::*;

#[cfg(test)]
mod tests;

fn normalize_channel_id(channel_id: &str) -> String {
    channel_id.trim().to_ascii_lowercase()
}

fn normalize_channel_config(config: ChannelConfig) -> ChannelConfig {
    ChannelConfig {
        enabled: config.enabled,
        sample_rate: config.sample_rate.clamp(0.0, 1.0),
        retention_count: config.retention_count.max(1),
    }
}

fn validate_runtime_channel_ownership(
    descriptor: &RuntimeChannelDescriptor,
) -> Result<(), ChannelRegistrationError> {
    match descriptor.owner.source {
        DiagnosticsChannelSource::Mod => {
            let Some(owner_id) = descriptor.owner.owner_id.as_ref() else {
                return Err(ChannelRegistrationError::InvalidOwnership {
                    channel_id: descriptor.channel_id.clone(),
                    reason: "mod channel missing owner_id".to_string(),
                });
            };
            let expected_prefix = format!("mod.{owner_id}.");
            if !descriptor.channel_id.starts_with(&expected_prefix) {
                return Err(ChannelRegistrationError::InvalidOwnership {
                    channel_id: descriptor.channel_id.clone(),
                    reason: format!("mod channels must use namespace '{expected_prefix}*'"),
                });
            }
        }
        DiagnosticsChannelSource::Verse => {
            if !descriptor.channel_id.starts_with("verse.") {
                return Err(ChannelRegistrationError::InvalidOwnership {
                    channel_id: descriptor.channel_id.clone(),
                    reason: "verse channels must use namespace 'verse.*'".to_string(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_runtime_channel_schema(
    descriptor: &RuntimeChannelDescriptor,
) -> Result<(), ChannelRegistrationError> {
    if descriptor.schema_version == 0 {
        return Err(ChannelRegistrationError::InvalidSchema {
            channel_id: descriptor.channel_id.clone(),
            reason: "schema_version must be greater than zero".to_string(),
        });
    }

    if let DiagnosticPayloadSchema::Structured(fields) = &descriptor.payload_schema
        && fields.is_empty()
    {
        return Err(ChannelRegistrationError::InvalidSchema {
            channel_id: descriptor.channel_id.clone(),
            reason: "structured payload schema must declare at least one field".to_string(),
        });
    }

    if let SamplingPolicy::SampleRate(rate) = descriptor.sampling
        && !(0.0..=1.0).contains(&rate)
    {
        return Err(ChannelRegistrationError::InvalidSchema {
            channel_id: descriptor.channel_id.clone(),
            reason: "sampling rate must be within 0.0..=1.0".to_string(),
        });
    }

    Ok(())
}

fn channel_payload_schema(channel_id: &str) -> DiagnosticPayloadSchema {
    match channel_id {
        CHANNEL_PROTOCOL_RESOLVE_STARTED
        | CHANNEL_PROTOCOL_RESOLVE_SUCCEEDED
        | CHANNEL_PROTOCOL_RESOLVE_FAILED
        | CHANNEL_PROTOCOL_RESOLVE_FALLBACK_USED => {
            DiagnosticPayloadSchema::Structured(PROTOCOL_RESOLVE_FIELDS.to_vec())
        }
        CHANNEL_VIEWER_SELECT_STARTED | CHANNEL_VIEWER_SELECT_SUCCEEDED => {
            DiagnosticPayloadSchema::Structured(VIEWER_SELECT_FIELDS.to_vec())
        }
        CHANNEL_ACTION_EXECUTE_STARTED
        | CHANNEL_ACTION_EXECUTE_SUCCEEDED
        | CHANNEL_ACTION_EXECUTE_FAILED => {
            DiagnosticPayloadSchema::Structured(ACTION_EXECUTE_FIELDS.to_vec())
        }
        CHANNEL_IDENTITY_SIGN_STARTED
        | CHANNEL_IDENTITY_SIGN_SUCCEEDED
        | CHANNEL_IDENTITY_SIGN_FAILED => {
            DiagnosticPayloadSchema::Structured(IDENTITY_SIGN_FIELDS.to_vec())
        }
        CHANNEL_NOSTR_RELAY_CONNECT_STARTED
        | CHANNEL_NOSTR_RELAY_CONNECT_SUCCEEDED
        | CHANNEL_NOSTR_RELAY_CONNECT_FAILED
        | CHANNEL_NOSTR_RELAY_DISCONNECTED => {
            DiagnosticPayloadSchema::Structured(NOSTR_RELAY_CONNECT_FIELDS.to_vec())
        }
        CHANNEL_RENDERER_ATTACH | CHANNEL_RENDERER_DETACH => {
            DiagnosticPayloadSchema::Structured(RENDERER_ATTACH_FIELDS.to_vec())
        }
        CHANNEL_UI_COMMAND_SURFACE_ROUTE_RESOLVED
        | CHANNEL_UI_COMMAND_SURFACE_ROUTE_BLOCKED
        | CHANNEL_UI_COMMAND_SURFACE_ROUTE_FALLBACK
        | CHANNEL_UI_COMMAND_SURFACE_ROUTE_NO_TARGET => {
            DiagnosticPayloadSchema::Structured(COMMAND_SURFACE_ROUTE_FIELDS.to_vec())
        }
        _ => DiagnosticPayloadSchema::FreeText,
    }
}

fn channel_retention_policy(channel_id: &str) -> RetentionPolicy {
    match channel_id {
        CHANNEL_ACTION_EXECUTE_FAILED
        | CHANNEL_IDENTITY_SIGN_FAILED
        | CHANNEL_PROTOCOL_RESOLVE_FAILED
        | CHANNEL_VIEWER_SURFACE_ALLOCATE_FAILED
        | CHANNEL_VIEWER_CAPABILITY_NONE
        | CHANNEL_NOSTR_RELAY_CONNECT_FAILED => RetentionPolicy::KeepRecent(500),
        CHANNEL_COMPOSITOR_TILE_ACTIVITY => RetentionPolicy::KeepRecent(256),
        _ => RetentionPolicy::Session,
    }
}

fn channel_sampling_policy(channel_id: &str) -> SamplingPolicy {
    match channel_id {
        CHANNEL_COMPOSITOR_OVERLAY_BATCH_SIZE_SAMPLE
        | CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_CALLBACK_US_SAMPLE
        | CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PRESENTATION_US_SAMPLE
        | CHANNEL_COMPOSITOR_DIFFERENTIAL_SKIP_RATE_SAMPLE
        | CHANNEL_COMPOSITOR_TILE_ACTIVITY => SamplingPolicy::SampleRate(0.25),
        _ => SamplingPolicy::All,
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

static GLOBAL_DIAGNOSTICS_REGISTRY: OnceLock<Mutex<DiagnosticsRegistry>> = OnceLock::new();

fn global_registry() -> &'static Mutex<DiagnosticsRegistry> {
    GLOBAL_DIAGNOSTICS_REGISTRY.get_or_init(|| Mutex::new(DiagnosticsRegistry::default()))
}

pub fn should_emit_and_observe(channel_id: &str) -> (bool, Vec<DiagnosticsInvariantViolation>) {
    let mut registry = global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned");
    let should_emit = registry.should_emit_channel(channel_id);
    let violations = registry.observe_channel_event(channel_id, current_unix_ms());
    (should_emit, violations)
}

pub fn list_channel_configs_snapshot() -> Vec<(RuntimeChannelDescriptor, ChannelConfig)> {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .list_channel_configs()
}

pub fn list_orphan_channels_snapshot() -> Vec<(String, u64)> {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .list_orphan_channels()
}

#[allow(dead_code)]
pub fn list_invariants_snapshot() -> Vec<DiagnosticsInvariant> {
    let mut invariants: Vec<DiagnosticsInvariant> = global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .invariants
        .values()
        .cloned()
        .collect();
    invariants.sort_by(|a, b| a.invariant_id.cmp(&b.invariant_id));
    invariants
}

pub fn set_channel_config_global(channel_id: &str, config: ChannelConfig) {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .set_config(channel_id, config);
}

#[allow(dead_code)]
pub fn get_channel_config_global(channel_id: &str) -> ChannelConfig {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .get_config(channel_id)
}

pub fn apply_persisted_channel_configs(configs: Vec<(String, ChannelConfig)>) {
    let mut registry = global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned");
    for (channel_id, config) in configs {
        registry.set_config(&channel_id, config);
    }
}

#[allow(dead_code)]
pub fn register_mod_channel_global(
    mod_id: &str,
    channel_id: &str,
    schema_version: u16,
    description: Option<String>,
    capabilities: &[DiagnosticsCapability],
) -> Result<bool, ChannelRegistrationError> {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .register_mod_channel(
            mod_id,
            channel_id,
            schema_version,
            description,
            capabilities,
        )
}

#[allow(dead_code)]
pub fn register_verse_channel_global(
    peer_id: &str,
    channel_id: &str,
    schema_version: u16,
    description: Option<String>,
    capabilities: &[DiagnosticsCapability],
) -> Result<bool, ChannelRegistrationError> {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .register_verse_channel(
            peer_id,
            channel_id,
            schema_version,
            description,
            capabilities,
        )
}

#[allow(dead_code)]
pub fn register_invariant_global(
    invariant: DiagnosticsInvariant,
    capabilities: &[DiagnosticsCapability],
) -> Result<bool, ChannelRegistrationError> {
    global_registry()
        .lock()
        .expect("diagnostics registry lock poisoned")
        .register_invariant(invariant, capabilities)
}
