// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::sync::OnceLock;

use super::*;
use crate::channels::*;

mod phase3a;
mod phase3b;

pub const PROTOCOL_RESOLVE_FIELDS: [PayloadField; 2] = [
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

pub const VIEWER_SELECT_FIELDS: [PayloadField; 2] = [
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

pub const ACTION_EXECUTE_FIELDS: [PayloadField; 2] = [
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

pub const IDENTITY_SIGN_FIELDS: [PayloadField; 2] = [
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

pub const NOSTR_RELAY_CONNECT_FIELDS: [PayloadField; 2] = [
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

pub const RENDERER_ATTACH_FIELDS: [PayloadField; 2] = [
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

pub const COMMAND_SURFACE_ROUTE_FIELDS: [PayloadField; 4] = [
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

pub const INVARIANT_VERSE_SYNC_RECEIVED_COMPLETES: &str = "invariant.verse.sync.received_completes";
pub const INVARIANT_VERSE_SYNC_SENT_COMPLETES: &str = "invariant.verse.sync.sent_completes";
pub const PHASE5_INVARIANT_IDS: [&str; 2] = [
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
    static COMBINED: OnceLock<Vec<DiagnosticChannelDescriptor>> = OnceLock::new();
    COMBINED.get_or_init(|| {
        let mut v: Vec<DiagnosticChannelDescriptor> = Vec::with_capacity(170);
        v.extend_from_slice(phase3a::PHASE3A);
        v.extend_from_slice(phase3b::PHASE3B);
        v
    })
}

pub fn phase5_required_channels() -> &'static [DiagnosticChannelDescriptor] {
    &PHASE5_CHANNELS
}

#[allow(dead_code)]
pub fn phase5_required_invariant_ids() -> &'static [&'static str] {
    &PHASE5_INVARIANT_IDS
}
