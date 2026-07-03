/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::super::{ChannelSeverity, DiagnosticChannelDescriptor};
use crate::channels::*;

pub(super) const PHASE3A: &[DiagnosticChannelDescriptor] = &[
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
];
