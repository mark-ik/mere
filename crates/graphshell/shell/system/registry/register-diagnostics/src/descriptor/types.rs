/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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
            super::channel_payload_schema(&channel_id),
            super::channel_retention_policy(&channel_id),
            super::channel_sampling_policy(&channel_id),
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
            super::channel_payload_schema(&channel_id),
            super::channel_retention_policy(&channel_id),
            super::channel_sampling_policy(&channel_id),
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
            super::channel_payload_schema(&channel_id),
            super::channel_retention_policy(&channel_id),
            super::channel_sampling_policy(&channel_id),
        )
    }

    pub fn from_contract(descriptor: DiagnosticChannelDescriptor) -> Self {
        Self::new(
            descriptor.channel_id,
            descriptor.schema_version,
            DiagnosticsChannelOwner::core(),
            None,
            descriptor.severity,
            super::channel_payload_schema(descriptor.channel_id),
            super::channel_retention_policy(descriptor.channel_id),
            super::channel_sampling_policy(descriptor.channel_id),
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

