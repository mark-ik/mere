/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::channels::*;
use super::super::{DiagnosticChannelDescriptor, ChannelSeverity};

pub(super) const PHASE3B: &[DiagnosticChannelDescriptor] = &[
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
