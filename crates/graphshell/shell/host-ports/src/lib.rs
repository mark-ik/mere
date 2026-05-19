/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Lightweight runtime-boundary vocabulary for host-independent shell code.
//!
//! The first extraction slice owns only the port traits that
//! `GraphshellRuntime::tick` actually needs today: clipboard access and toast
//! emission. Keeping that seam in a tiny crate lets later parity tests target a
//! lighter dependency surface before the rest of the host/compositor boundary
//! moves out of the main `graphshell` crate.

pub mod content_surface;
pub mod finalize_actions;
pub mod frame_inbox;
pub mod frame_projection;
pub mod portable_time;
pub mod ports;
pub mod rendering_context_producer;
pub mod system;
pub mod webview_backpressure;

// 2026-04-25 mere-host-contract extraction Slice 3: portable_now()
// re-exported at the crate root for ergonomic imports.
pub use portable_time::portable_now;

// 2026-04-25 servo-into-verso S3a: re-export the host-port trait
// surface (added in S3a) at the crate root for ergonomic imports
// from host-side adapters (egui, iced).
pub use ports::{
    BackendViewportInPixels, HostAccessibilityPort, HostInputPort, HostPaintPort, HostPorts,
    HostSurfacePort, HostTexturePort, ViewerSurfaceId,
};

pub use content_surface::{ContentSurfaceHandle, ViewerSurfaceFramePath};
pub use finalize_actions::{
    CLIPBOARD_STATUS_EMPTY_TEXT, CLIPBOARD_STATUS_FAILURE_PREFIX,
    CLIPBOARD_STATUS_MISSING_NODE_SUGGESTION_TEXT, CLIPBOARD_STATUS_SUCCESS_TITLE_TEXT,
    CLIPBOARD_STATUS_SUCCESS_URL_TEXT, CLIPBOARD_STATUS_UNAVAILABLE_TEXT, ClipboardCopyKind,
    ClipboardCopyRequest, ClipboardCopySource, NodeStatusNotice, RuntimeClipboardCopyState,
    RuntimeNodeStatusNoticeState, UiNotificationLevel, clipboard_copy_failure_text,
    clipboard_copy_missing_node_failure_text, clipboard_copy_success_text,
    drain_pending_clipboard_copy_requests, drain_pending_node_status_notices,
    emit_node_status_toast, port_error,
};
pub use frame_inbox::FrameInboxState;
pub use frame_projection::{
    AccessibilityProjectionInput, CommandPaletteProjectionInput, DialogsProjectionInput,
    FocusProjectionInput, FocusProjectionOutput, GraphSearchProjectionInput,
    OmnibarProjectionInput, SettingsProjectionInput, ToolbarProjectionInput,
    TransientFrameOutputsProjection, TransientFrameOutputsProjectionInput,
    project_accessibility_view_model, project_command_palette_view_model,
    project_dialogs_view_model, project_focus_view_model, project_graph_search_view_model,
    project_omnibar_view_model, project_settings_view_model, project_toolbar_view_model,
    project_transient_frame_outputs,
};
pub use shell_state::frame_model::{
    AccessibilityViewModel, CommandPaletteScopeView, CommandPaletteViewModel, DialogsViewModel,
    FocusRingCurve, FocusRingSettingsView, FocusRingSpec, FocusViewModel, FrameHostInput,
    FrameViewModel, GraphSearchViewModel, OmnibarProviderStatusView, OmnibarSessionKindView,
    OmnibarViewModel, SettingsViewModel, ThumbnailAspectView, ThumbnailFilterView,
    ThumbnailFormatView, ThumbnailSettingsView, ToastSeverity, ToastSpec, ToolbarViewModel,
};
pub use rendering_context_producer::RenderingContextProducer;
pub use webview_backpressure::{
    NodePaneAttachAttemptMetadata, RuntimeWebviewBackpressureMetadataSource,
    WebviewAttachRetryState, WebviewCreationBackpressureState, WebviewCreationProbeState,
};
