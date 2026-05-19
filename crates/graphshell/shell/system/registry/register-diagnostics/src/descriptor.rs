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

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::channels::*;

/// Severity tier for diagnostic channel prioritization in the diagnostics pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelSeverity {
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticFieldType {
    String,
    Integer,
    Float,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadField {
    pub name: &'static str,
    pub field_type: DiagnosticFieldType,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticPayloadSchema {
    FreeText,
    Structured(Vec<PayloadField>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionPolicy {
    KeepRecent(usize),
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SamplingPolicy {
    All,
    SampleRate(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticChannelDescriptor {
    pub channel_id: &'static str,
    pub schema_version: u16,
    pub severity: ChannelSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsChannelSource {
    Core,
    Mod,
    Verse,
    Agent,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsChannelOwner {
    pub source: DiagnosticsChannelSource,
    pub owner_id: Option<String>,
}

impl DiagnosticsChannelOwner {
    pub fn core() -> Self {
        Self {
            source: DiagnosticsChannelSource::Core,
            owner_id: None,
        }
    }

    pub fn runtime() -> Self {
        Self {
            source: DiagnosticsChannelSource::Runtime,
            owner_id: None,
        }
    }

    pub fn mod_owner(mod_id: &str) -> Self {
        Self {
            source: DiagnosticsChannelSource::Mod,
            owner_id: Some(mod_id.to_ascii_lowercase()),
        }
    }

    pub fn verse_owner(peer_id: &str) -> Self {
        Self {
            source: DiagnosticsChannelSource::Verse,
            owner_id: Some(peer_id.to_ascii_lowercase()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeChannelDescriptor {
    pub channel_id: String,
    pub schema_version: u16,
    pub owner: DiagnosticsChannelOwner,
    pub description: Option<String>,
    pub severity: ChannelSeverity,
    pub payload_schema: DiagnosticPayloadSchema,
    pub retention: RetentionPolicy,
    pub sampling: SamplingPolicy,
}

impl RuntimeChannelDescriptor {
    pub fn new(
        channel_id: impl Into<String>,
        schema_version: u16,
        owner: DiagnosticsChannelOwner,
        description: Option<String>,
        severity: ChannelSeverity,
        payload_schema: DiagnosticPayloadSchema,
        retention: RetentionPolicy,
        sampling: SamplingPolicy,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            schema_version,
            owner,
            description,
            severity,
            payload_schema,
            retention,
            sampling,
        }
    }

    pub fn info(
        channel_id: impl Into<String>,
        schema_version: u16,
        owner: DiagnosticsChannelOwner,
        description: Option<String>,
    ) -> Self {
        let channel_id = channel_id.into();
        Self::new(
            channel_id.clone(),
            schema_version,
            owner,
            description,
            ChannelSeverity::Info,
            channel_payload_schema(&channel_id),
            channel_retention_policy(&channel_id),
            channel_sampling_policy(&channel_id),
        )
    }

    pub fn warn(
        channel_id: impl Into<String>,
        schema_version: u16,
        owner: DiagnosticsChannelOwner,
        description: Option<String>,
    ) -> Self {
        let channel_id = channel_id.into();
        Self::new(
            channel_id.clone(),
            schema_version,
            owner,
            description,
            ChannelSeverity::Warn,
            channel_payload_schema(&channel_id),
            channel_retention_policy(&channel_id),
            channel_sampling_policy(&channel_id),
        )
    }

    pub fn error(
        channel_id: impl Into<String>,
        schema_version: u16,
        owner: DiagnosticsChannelOwner,
        description: Option<String>,
    ) -> Self {
        let channel_id = channel_id.into();
        Self::new(
            channel_id.clone(),
            schema_version,
            owner,
            description,
            ChannelSeverity::Error,
            channel_payload_schema(&channel_id),
            channel_retention_policy(&channel_id),
            channel_sampling_policy(&channel_id),
        )
    }

    pub fn from_contract(descriptor: DiagnosticChannelDescriptor) -> Self {
        Self::new(
            descriptor.channel_id,
            descriptor.schema_version,
            DiagnosticsChannelOwner::core(),
            None,
            descriptor.severity,
            channel_payload_schema(descriptor.channel_id),
            channel_retention_policy(descriptor.channel_id),
            channel_sampling_policy(descriptor.channel_id),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelRegistrationPolicy {
    RejectConflict,
    ReplaceExisting,
    KeepExisting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelRegistrationError {
    InvalidChannelId,
    InvalidSchema {
        channel_id: String,
        reason: String,
    },
    Conflict {
        channel_id: String,
        existing_schema_version: u16,
        requested_schema_version: u16,
    },
    InvalidOwnership {
        channel_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsCapability {
    RegisterChannels,
    RegisterInvariants,
    #[allow(dead_code)]
    ConfigureChannels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsInvariant {
    pub invariant_id: String,
    pub start_channel: String,
    pub terminal_channels: Vec<String>,
    pub timeout_ms: u64,
    pub owner: DiagnosticsChannelOwner,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsInvariantViolation {
    pub invariant_id: String,
    pub start_channel: String,
    pub deadline_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingInvariantToken {
    start_channel: String,
    deadline_unix_ms: u64,
}

const PROTOCOL_RESOLVE_FIELDS: [PayloadField; 2] = [
    PayloadField {
        name: "uri",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "scheme",
        field_type: DiagnosticFieldType::String,
        required: false,
    },
];

const VIEWER_SELECT_FIELDS: [PayloadField; 2] = [
    PayloadField {
        name: "uri",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "viewer_id",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
];

const ACTION_EXECUTE_FIELDS: [PayloadField; 2] = [
    PayloadField {
        name: "action_id",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "latency_us",
        field_type: DiagnosticFieldType::Integer,
        required: false,
    },
];

const IDENTITY_SIGN_FIELDS: [PayloadField; 2] = [
    PayloadField {
        name: "identity_id",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "payload_bytes",
        field_type: DiagnosticFieldType::Integer,
        required: false,
    },
];

const NOSTR_RELAY_CONNECT_FIELDS: [PayloadField; 2] = [
    PayloadField {
        name: "relay_url",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "attempt",
        field_type: DiagnosticFieldType::Integer,
        required: false,
    },
];

const RENDERER_ATTACH_FIELDS: [PayloadField; 2] = [
    PayloadField {
        name: "pane_id",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "renderer_id",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
];

const COMMAND_SURFACE_ROUTE_FIELDS: [PayloadField; 4] = [
    PayloadField {
        name: "source_surface",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "command_id",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "target_kind",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
    PayloadField {
        name: "route_detail",
        field_type: DiagnosticFieldType::String,
        required: true,
    },
];

const PHASE0_CHANNELS: [DiagnosticChannelDescriptor; 15] = [
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PROTOCOL_RESOLVE_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PROTOCOL_RESOLVE_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PROTOCOL_RESOLVE_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PROTOCOL_RESOLVE_FALLBACK_USED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_SELECT_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_SELECT_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_FALLBACK_USED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_SURFACE_ALLOCATE_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_FALLBACK_WRY_FEATURE_DISABLED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_FALLBACK_WRY_CAPABILITY_MISSING,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_FALLBACK_WRY_DISABLED_BY_PREFERENCE,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_CAPABILITY_PARTIAL,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VIEWER_CAPABILITY_NONE,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_RENDERER_ATTACH,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_RENDERER_DETACH,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
];

const PHASE2_CHANNELS: [DiagnosticChannelDescriptor; 10] = [
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_ACTION_EXECUTE_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_ACTION_EXECUTE_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_ACTION_EXECUTE_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_INPUT_BINDING_RESOLVED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_INPUT_BINDING_REBOUND,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_INPUT_BINDING_MISSING,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_INPUT_BINDING_CONFLICT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LENS_RESOLVE_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LENS_RESOLVE_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LENS_FALLBACK_USED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
];

const PHASE3_CHANNELS: [DiagnosticChannelDescriptor; 170] = [
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_SIGN_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_SIGN_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_SIGN_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_KEY_UNAVAILABLE,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_VERIFY_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_VERIFY_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_VERIFY_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_KEY_LOADED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_KEY_GENERATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_IDENTITY_TRUST_STORE_LOAD_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_NOSTR_RELAY_CONNECT_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_NOSTR_RELAY_CONNECT_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_NOSTR_RELAY_CONNECT_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_NOSTR_RELAY_DISCONNECTED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_CHANNEL_REGISTERED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_CONFIG_CHANGED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_INVARIANT_TIMEOUT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_LOAD_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_LOAD_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_LOAD_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_ROLLBACK_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_ROLLBACK_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_QUARANTINED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_UNLOAD_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_MOD_DEPENDENCY_MISSING,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_CONFIG_SNAPSHOT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_PERSISTENCE_OPEN_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_PERSISTENCE_OPEN_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_PERSISTENCE_OPEN_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_PERSISTENCE_OPEN_TIMEOUT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PERSISTENCE_RECOVER_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PERSISTENCE_RECOVER_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_VERSE_INIT_MODE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_VERSE_INIT_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_VERSE_INIT_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_SELFCHECK_REGISTRIES_LOADED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_SELFCHECK_CHANNELS_COMPLETE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_STARTUP_SELFCHECK_CHANNELS_INCOMPLETE,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_HISTORY_MANAGER_LIMIT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_BAR_WORKBENCH_COMMAND_REQUESTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_BAR_WORKBENCH_COMMAND_EXECUTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_BAR_WORKBENCH_COMMAND_BLOCKED_BY_FOCUS,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_SURFACE_ROUTE_RESOLVED,
        schema_version: 2,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_SURFACE_ROUTE_BLOCKED,
        schema_version: 2,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_SURFACE_ROUTE_FALLBACK,
        schema_version: 2,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_COMMAND_SURFACE_ROUTE_NO_TARGET,
        schema_version: 2,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TRAVERSAL_RECORDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TRAVERSAL_RECORD_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_ARCHIVE_DISSOLVED_APPENDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_ARCHIVE_CLEAR_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_ARCHIVE_EXPORT_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_PREVIEW_ENTERED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_PREVIEW_EXITED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_PREVIEW_ISOLATION_VIOLATION,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_REPLAY_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_REPLAY_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_REPLAY_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_HISTORY_TIMELINE_RETURN_TO_PRESENT_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UI_CLIPBOARD_COPY_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_RUNTIME_CACHE_HIT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_RUNTIME_CACHE_MISS,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_RUNTIME_CACHE_INSERT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_RUNTIME_CACHE_EVICTION,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_NAVIGATION_TRANSITION,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_NAVIGATION_VIOLATION,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_ARRANGEMENT_PROJECTION_HEALTH,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_ARRANGEMENT_MISSING_FAMILY_FALLBACK,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_ARRANGEMENT_DURABILITY_TRANSITION,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FOCUS_CAPTURE_ENTER,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FOCUS_CAPTURE_EXIT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FOCUS_RETURN_FALLBACK,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FOCUS_REALIZATION_MISMATCH,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_EMBEDDED_FOCUS_RECLAIM,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_STRUCTURAL_VIOLATION,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_CONTRACT_WARNING,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_TREE_BUILD,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_TREE_SNAPSHOT_BUILT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_SNAPSHOT_WRITTEN,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_WORKBENCH_SURFACE_PROFILE_ACTIVATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_CANVAS_PROFILE_ACTIVATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PHYSICS_PROFILE_ACTIVATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LAYOUT_COMPUTE_STARTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LAYOUT_COMPUTE_SUCCEEDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LAYOUT_COMPUTE_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LAYOUT_FALLBACK_USED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_LAYOUT_DOMAIN_PROFILE_RESOLVED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_PRESENTATION_PROFILE_RESOLVED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_THEME_ACTIVATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_AGENT_SPAWNED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_AGENT_INTENT_DROPPED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_KNOWLEDGE_INDEX_UPDATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_KNOWLEDGE_TAG_VALIDATION_WARN,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_INDEX_SEARCH,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_WORKFLOW_ACTIVATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_PROBE_REGISTERED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_PROBE_DISABLED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FACET_FILTER_APPLIED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FACET_FILTER_CLEARED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FACET_FILTER_INVALID_QUERY,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FACET_FILTER_TYPE_MISMATCH,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FACET_FILTER_EVAL_FAILURE,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_RADIAL_OVERFLOW,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_RADIAL_LAYOUT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_RADIAL_LABEL_COLLISION,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_PUBLISHED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_UNROUTED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_FAILED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_QUEUE_DEPTH,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_LAGGED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_MOD_WORKFLOW_ROUTED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_REGISTER_SIGNAL_ROUTING_SUBSYSTEM_HEALTH_PROPAGATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_PREINIT_CALL,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_SURFACE_CONFORMANCE_PARTIAL,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_SURFACE_CONFORMANCE_NONE,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_GL_STATE_VIOLATION,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_CONTENT_PASS_REGISTERED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_PASS_REGISTERED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_PASS_ORDER_VIOLATION,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_INVALID_TILE_RECT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_CHAOS,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_CHAOS_PASS,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_CHAOS_FAIL,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PROBE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PROBE_FAILED_FRAME,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_CALLBACK_US_SAMPLE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_DIAGNOSTICS_COMPOSITOR_BRIDGE_PRESENTATION_US_SAMPLE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_FOCUS_ACTIVATION_DEFERRED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_STYLE_RECT_STROKE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_STYLE_CHROME_ONLY,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_MODE_COMPOSITED_TEXTURE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_MODE_NATIVE_OVERLAY,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_MODE_EMBEDDED_HOST,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_MODE_PLACEHOLDER,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_INTERACTION_MENU,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_HELP_PANEL,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_RADIAL_MENU,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_TILE_DRAG,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_REPLAY_SAMPLE_RECORDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_REPLAY_ARTIFACT_RECORDED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DIFFERENTIAL_CONTENT_COMPOSED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DIFFERENTIAL_CONTENT_SKIPPED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DIFFERENTIAL_FALLBACK_NO_PRIOR_SIGNATURE,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DIFFERENTIAL_FALLBACK_SIGNATURE_CHANGED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DIFFERENTIAL_SKIP_RATE_SAMPLE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_TILE_ACTIVITY,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_LIFECYCLE_INDICATOR,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_LENS_OVERLAY_APPLIED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_CONTENT_CULLED_OFFVIEWPORT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DEGRADATION_GPU_PRESSURE,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_DEGRADATION_PLACEHOLDER_MODE,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_RESOURCE_REUSE_CONTEXT_HIT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_RESOURCE_REUSE_CONTEXT_MISS,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_SHARED_WGPU,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_CALLBACK_FALLBACK,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_VIEWER_SURFACE_PATH_MISSING_SURFACE,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_OVERLAY_BATCH_SIZE_SAMPLE,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_RADIAL_MODE_FALLBACK,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_SYSTEM_TASK_BUDGET_BACKPRESSURE,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_SYSTEM_TASK_BUDGET_WORKER_SUSPENDED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_SYSTEM_TASK_BUDGET_WORKER_RESUMED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_SYSTEM_TASK_BUDGET_QUEUE_DEPTH,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_PRESENTATION_BOUNDS_MISSING,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_LAYOUT_GUTTER_DETECTED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_LAYOUT_OVERLAP_DETECTED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_LAYOUT_CONSTRAINT_CONFLICT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_LAYOUT_CONSTRAINT_DRIFT,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_CONFIG_MODE_ENTERED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_UX_FIRST_USE_PROMPT_SHOWN,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_PAINT_NOT_CONFIRMED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_COMPOSITOR_NATIVE_OVERLAY_RECT_MISMATCH,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
];

const PHASE5_CHANNELS: [DiagnosticChannelDescriptor; 8] = [
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_UNIT_SENT,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_UNIT_RECEIVED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_INTENT_APPLIED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_ACCESS_DENIED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_CONNECTION_REJECTED,
        schema_version: 1,
        severity: ChannelSeverity::Error,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_IDENTITY_GENERATED,
        schema_version: 1,
        severity: ChannelSeverity::Info,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_CONFLICT_DETECTED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
    DiagnosticChannelDescriptor {
        channel_id: CHANNEL_VERSE_SYNC_CONFLICT_RESOLVED,
        schema_version: 1,
        severity: ChannelSeverity::Warn,
    },
];

const INVARIANT_VERSE_SYNC_RECEIVED_COMPLETES: &str = "invariant.verse.sync.received_completes";
const INVARIANT_VERSE_SYNC_SENT_COMPLETES: &str = "invariant.verse.sync.sent_completes";
const PHASE5_INVARIANT_IDS: [&str; 2] = [
    INVARIANT_VERSE_SYNC_RECEIVED_COMPLETES,
    INVARIANT_VERSE_SYNC_SENT_COMPLETES,
];

pub fn phase0_required_channels() -> &'static [DiagnosticChannelDescriptor] {
    &PHASE0_CHANNELS
}

pub fn phase2_required_channels() -> &'static [DiagnosticChannelDescriptor] {
    &PHASE2_CHANNELS
}

pub fn phase3_required_channels() -> &'static [DiagnosticChannelDescriptor] {
    &PHASE3_CHANNELS
}

pub fn phase5_required_channels() -> &'static [DiagnosticChannelDescriptor] {
    &PHASE5_CHANNELS
}

#[allow(dead_code)]
pub fn phase5_required_invariant_ids() -> &'static [&'static str] {
    &PHASE5_INVARIANT_IDS
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub enabled: bool,
    pub sample_rate: f32,
    pub retention_count: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_rate: 1.0,
            retention_count: 100,
        }
    }
}

pub struct DiagnosticsRegistry {
    channels: HashMap<String, RuntimeChannelDescriptor>,
    configs: HashMap<String, ChannelConfig>,
    sample_counters: HashMap<String, u64>,
    orphan_channels: HashMap<String, u64>,
    invariants: HashMap<String, DiagnosticsInvariant>,
    pending_invariants: HashMap<String, VecDeque<PendingInvariantToken>>,
}

impl Default for DiagnosticsRegistry {
    fn default() -> Self {
        let mut registry = Self {
            channels: HashMap::new(),
            configs: HashMap::new(),
            sample_counters: HashMap::new(),
            orphan_channels: HashMap::new(),
            invariants: HashMap::new(),
            pending_invariants: HashMap::new(),
        };

        registry.register_batch(phase0_required_channels());
        registry.register_batch(phase2_required_channels());
        registry.register_batch(phase3_required_channels());
        registry.register_batch(phase5_required_channels());
        registry.register_default_invariants();

        registry
    }
}

impl DiagnosticsRegistry {
    pub fn register(&mut self, descriptor: DiagnosticChannelDescriptor) {
        let runtime = RuntimeChannelDescriptor::from_contract(descriptor);
        self.configs.entry(runtime.channel_id.clone()).or_default();
        self.channels.insert(runtime.channel_id.clone(), runtime);
    }

    pub fn register_batch(&mut self, descriptors: &[DiagnosticChannelDescriptor]) {
        for descriptor in descriptors.iter().copied() {
            self.register(descriptor);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn get_config(&self, channel_id: &str) -> ChannelConfig {
        self.configs
            .get(&normalize_channel_id(channel_id))
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_config(&mut self, channel_id: &str, config: ChannelConfig) {
        self.configs.insert(
            normalize_channel_id(channel_id),
            normalize_channel_config(config),
        );
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_channel(&self, channel_id: &str) -> bool {
        self.channels
            .contains_key(&normalize_channel_id(channel_id))
    }

    pub fn list_channel_configs(&self) -> Vec<(RuntimeChannelDescriptor, ChannelConfig)> {
        self.channels
            .values()
            .cloned()
            .map(|descriptor| {
                let config = self
                    .configs
                    .get(&descriptor.channel_id)
                    .cloned()
                    .unwrap_or_default();
                (descriptor, config)
            })
            .collect()
    }

    pub fn list_orphan_channels(&self) -> Vec<(String, u64)> {
        let mut entries: Vec<(String, u64)> = self
            .orphan_channels
            .iter()
            .map(|(channel_id, count)| (channel_id.clone(), *count))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    pub fn register_runtime_channel(
        &mut self,
        descriptor: RuntimeChannelDescriptor,
        policy: ChannelRegistrationPolicy,
    ) -> Result<bool, ChannelRegistrationError> {
        if descriptor.channel_id.trim().is_empty() {
            return Err(ChannelRegistrationError::InvalidChannelId);
        }

        let normalized_id = normalize_channel_id(&descriptor.channel_id);
        let mut normalized_descriptor = descriptor.clone();
        normalized_descriptor.channel_id = normalized_id.clone();

        validate_runtime_channel_ownership(&normalized_descriptor)?;
        validate_runtime_channel_schema(&normalized_descriptor)?;

        if let Some(existing) = self.channels.get(&normalized_id) {
            if existing.schema_version != normalized_descriptor.schema_version {
                match policy {
                    ChannelRegistrationPolicy::RejectConflict => {
                        return Err(ChannelRegistrationError::Conflict {
                            channel_id: normalized_id,
                            existing_schema_version: existing.schema_version,
                            requested_schema_version: normalized_descriptor.schema_version,
                        });
                    }
                    ChannelRegistrationPolicy::KeepExisting => return Ok(false),
                    ChannelRegistrationPolicy::ReplaceExisting => {}
                }
            } else if !matches!(policy, ChannelRegistrationPolicy::ReplaceExisting) {
                return Ok(false);
            }
        }

        self.channels.insert(
            normalized_descriptor.channel_id.clone(),
            normalized_descriptor.clone(),
        );
        self.configs
            .entry(normalized_descriptor.channel_id)
            .or_default();
        Ok(true)
    }

    pub fn register_mod_channel(
        &mut self,
        mod_id: &str,
        channel_id: &str,
        schema_version: u16,
        description: Option<String>,
        capabilities: &[DiagnosticsCapability],
    ) -> Result<bool, ChannelRegistrationError> {
        if !capabilities.contains(&DiagnosticsCapability::RegisterChannels) {
            return Err(ChannelRegistrationError::InvalidOwnership {
                channel_id: channel_id.to_string(),
                reason: "missing RegisterChannels capability".to_string(),
            });
        }

        self.register_runtime_channel(
            RuntimeChannelDescriptor::info(
                channel_id,
                schema_version,
                DiagnosticsChannelOwner::mod_owner(mod_id),
                description,
            ),
            ChannelRegistrationPolicy::RejectConflict,
        )
    }

    pub fn register_verse_channel(
        &mut self,
        peer_id: &str,
        channel_id: &str,
        schema_version: u16,
        description: Option<String>,
        capabilities: &[DiagnosticsCapability],
    ) -> Result<bool, ChannelRegistrationError> {
        if !capabilities.contains(&DiagnosticsCapability::RegisterChannels) {
            return Err(ChannelRegistrationError::InvalidOwnership {
                channel_id: channel_id.to_string(),
                reason: "missing RegisterChannels capability".to_string(),
            });
        }

        self.register_runtime_channel(
            RuntimeChannelDescriptor::info(
                channel_id,
                schema_version,
                DiagnosticsChannelOwner::verse_owner(peer_id),
                description,
            ),
            ChannelRegistrationPolicy::RejectConflict,
        )
    }

    pub fn should_emit_channel(&mut self, channel_id: &str) -> bool {
        let normalized = normalize_channel_id(channel_id);
        if !self.channels.contains_key(&normalized) {
            log::warn!("diagnostics: emit to unregistered channel '{normalized}'");
            let _ = self.register_runtime_channel(
                RuntimeChannelDescriptor::info(
                    normalized.clone(),
                    1,
                    DiagnosticsChannelOwner::runtime(),
                    Some("Auto-registered runtime channel".to_string()),
                ),
                ChannelRegistrationPolicy::KeepExisting,
            );
            *self.orphan_channels.entry(normalized.clone()).or_insert(0) += 1;
        }

        let config = self.configs.get(&normalized).cloned().unwrap_or_default();
        if !config.enabled {
            return false;
        }
        if config.sample_rate >= 1.0 {
            return true;
        }
        if config.sample_rate <= 0.0 {
            return false;
        }

        let counter = self.sample_counters.entry(normalized).or_insert(0);
        *counter = counter.saturating_add(1);
        let gate = (1.0f32 / config.sample_rate.max(0.0001)).ceil() as u64;
        gate <= 1 || (*counter % gate == 0)
    }

    pub fn register_invariant(
        &mut self,
        invariant: DiagnosticsInvariant,
        capabilities: &[DiagnosticsCapability],
    ) -> Result<bool, ChannelRegistrationError> {
        if !capabilities.contains(&DiagnosticsCapability::RegisterInvariants) {
            return Err(ChannelRegistrationError::InvalidOwnership {
                channel_id: invariant.start_channel,
                reason: "missing RegisterInvariants capability".to_string(),
            });
        }

        if invariant.invariant_id.trim().is_empty() || invariant.timeout_ms == 0 {
            return Err(ChannelRegistrationError::InvalidChannelId);
        }

        let invariant_id = invariant.invariant_id.trim().to_ascii_lowercase();
        if self.invariants.contains_key(&invariant_id) {
            return Ok(false);
        }

        self.invariants.insert(
            invariant_id,
            DiagnosticsInvariant {
                invariant_id: invariant.invariant_id.trim().to_ascii_lowercase(),
                start_channel: normalize_channel_id(&invariant.start_channel),
                terminal_channels: invariant
                    .terminal_channels
                    .iter()
                    .map(|entry| normalize_channel_id(entry))
                    .collect(),
                timeout_ms: invariant.timeout_ms,
                owner: invariant.owner,
                enabled: invariant.enabled,
            },
        );
        Ok(true)
    }

    pub fn observe_channel_event(
        &mut self,
        channel_id: &str,
        now_unix_ms: u64,
    ) -> Vec<DiagnosticsInvariantViolation> {
        let normalized = normalize_channel_id(channel_id);

        for invariant in self.invariants.values() {
            if !invariant.enabled {
                continue;
            }

            if invariant.start_channel == normalized {
                self.pending_invariants
                    .entry(invariant.invariant_id.clone())
                    .or_default()
                    .push_back(PendingInvariantToken {
                        start_channel: normalized.clone(),
                        deadline_unix_ms: now_unix_ms.saturating_add(invariant.timeout_ms),
                    });
            }

            if invariant
                .terminal_channels
                .iter()
                .any(|entry| entry == &normalized)
                && let Some(queue) = self.pending_invariants.get_mut(&invariant.invariant_id)
            {
                let _ = queue.pop_front();
            }
        }

        self.sweep_invariants(now_unix_ms)
    }

    pub fn sweep_invariants(&mut self, now_unix_ms: u64) -> Vec<DiagnosticsInvariantViolation> {
        let mut violations = Vec::new();

        for invariant in self.invariants.values() {
            if !invariant.enabled {
                continue;
            }

            let Some(queue) = self.pending_invariants.get_mut(&invariant.invariant_id) else {
                continue;
            };

            while let Some(front) = queue.front() {
                if front.deadline_unix_ms > now_unix_ms {
                    break;
                }
                let expired = queue.pop_front().expect("queue front just checked");
                violations.push(DiagnosticsInvariantViolation {
                    invariant_id: invariant.invariant_id.clone(),
                    start_channel: expired.start_channel,
                    deadline_unix_ms: expired.deadline_unix_ms,
                });
            }
        }

        violations
    }

    fn register_default_invariants(&mut self) {
        let _ = self.register_invariant(
            DiagnosticsInvariant {
                invariant_id: "invariant.registry.protocol.resolve_completes".to_string(),
                start_channel: CHANNEL_PROTOCOL_RESOLVE_STARTED.to_string(),
                terminal_channels: vec![
                    CHANNEL_PROTOCOL_RESOLVE_SUCCEEDED.to_string(),
                    CHANNEL_PROTOCOL_RESOLVE_FAILED.to_string(),
                ],
                timeout_ms: 500,
                owner: DiagnosticsChannelOwner::core(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );

        let phase5_terminal_channels = vec![
            CHANNEL_VERSE_SYNC_INTENT_APPLIED.to_string(),
            CHANNEL_VERSE_SYNC_ACCESS_DENIED.to_string(),
            CHANNEL_VERSE_SYNC_CONNECTION_REJECTED.to_string(),
        ];

        let _ = self.register_invariant(
            DiagnosticsInvariant {
                invariant_id: INVARIANT_VERSE_SYNC_RECEIVED_COMPLETES.to_string(),
                start_channel: CHANNEL_VERSE_SYNC_UNIT_RECEIVED.to_string(),
                terminal_channels: phase5_terminal_channels.clone(),
                timeout_ms: 1_000,
                owner: DiagnosticsChannelOwner::core(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );

        let _ = self.register_invariant(
            DiagnosticsInvariant {
                invariant_id: INVARIANT_VERSE_SYNC_SENT_COMPLETES.to_string(),
                start_channel: CHANNEL_VERSE_SYNC_UNIT_SENT.to_string(),
                terminal_channels: phase5_terminal_channels,
                timeout_ms: 2_000,
                owner: DiagnosticsChannelOwner::core(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            registry.has_channel(CHANNEL_COMPOSITOR_OVERLAY_NATIVE_SUPPRESSED_INTERACTION_MENU)
        );
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
                .find_map(|(descriptor, _)| {
                    (descriptor.channel_id == channel_id).then_some(descriptor)
                })
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
}
