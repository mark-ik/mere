// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `TileSignal` — substrate-shaped projection of masonry's `RenderRootSignal`.
//!
//! masonry produces a `RenderRootSignal` enum with cases that assume a
//! window-shaped host (cursor changes, IME area moves, window-drag,
//! window-menu, etc.). For substrate-as-host the host *is* a window, so most
//! of these route directly. For tile-as-tenant-of-substrate, some don't make
//! sense (`DragWindow`, `Minimize`, `Exit`) — those are filtered or rejected.
//!
//! This module owns the projection so the host doesn't import masonry types
//! directly.

use accesskit::NodeId as AccessKitNodeId;
use dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use kurbo::Point;
use masonry_core::app::RenderRootSignal;
use masonry_core::core::{CursorIcon, ErasedAction, ResizeDirection, WidgetId};

/// Substrate-shaped signals emitted by a [`crate::MasonryTile`] per frame.
///
/// The host drains these via [`crate::MasonryTile::drain_signals`] and routes
/// each one to the appropriate substrate sink:
///
/// | Signal class            | Substrate sink                                       |
/// | ----------------------- | ---------------------------------------------------- |
/// | `Action`                | the action bus                                       |
/// | `RequestRedraw`         | the substrate's frame scheduler                      |
/// | `RequestAnimFrame`      | the substrate's animation tick                       |
/// | `SetCursor`             | the substrate's cursor manager                       |
/// | `Ime{Start,End,Moved}`  | the OS IME bridge (per OS-plumbing audit §2.1)       |
/// | `ClipboardStore`        | the substrate clipboard (per OS-plumbing audit §2.6) |
/// | `TakeFocus`             | the substrate focus router                           |
/// | `WidgetSelected`        | dev-tooling / inspector — substrate may ignore       |
/// | `Layer{New,Remove,Repos}` | masonry's pop-up / overlay system; v0 may defer    |
/// | `Window*` (Drag, Minimize, ...) | only meaningful at substrate-as-host scope; tiles can't drive these — see [`Self::is_window_scoped`] |
#[derive(Debug)]
pub enum TileSignal {
    /// A widget emitted a typed action.
    ///
    /// The host translates `(WidgetId, ErasedAction)` into the appropriate
    /// substrate action-bus dispatch (per multiplexer brief §5.8).
    Action {
        widget_id: WidgetId,
        action: ErasedAction,
    },

    /// The tile needs to be redrawn.
    RequestRedraw,
    /// An animation frame should be scheduled.
    RequestAnimFrame,

    /// The tile wants this widget focused.
    TakeFocus,

    /// The cursor icon should change while hovering this tile.
    SetCursor(CursorIcon),

    /// IME composition session started for this tile.
    ImeStart,
    /// IME composition session ended.
    ImeEnd,
    /// IME area (caret rect) moved.
    ///
    /// Coordinates are in tile-local logical space; the host translates to
    /// physical-screen space using the tile's placement transform before
    /// routing to the OS IME bridge.
    ImeMoved {
        position: LogicalPosition<f64>,
        size: LogicalSize<f64>,
    },

    /// User interaction wrote text to the clipboard.
    ClipboardStore(String),

    /// A new masonry pop-up layer should be created.
    ///
    /// Position is in tile-local coordinates. v0 hosts may simply log this
    /// and skip; pop-up rendering is a substrate concern with several
    /// possible models (in-scene-paint vs overlay).
    NewLayerRequested { at: Point },

    /// An existing layer should be repositioned.
    RepositionLayerRequested { widget: WidgetId, at: Point },

    /// An existing layer should be removed.
    RemoveLayerRequested { widget: WidgetId },

    /// Widget inspector pick — dev tooling.
    WidgetInspectorPick(WidgetId),

    /// Window-scoped signals masonry can produce that don't make sense
    /// for a tile.
    ///
    /// A tile cannot drag/resize/minimize/exit the host's window. The host
    /// should log a warning if it sees these — they indicate a widget is
    /// using window-decoration affordances that don't map to tile semantics.
    /// At substrate-as-host scope, these *do* make sense and may be re-projected
    /// upward by the host into substrate-window operations.
    WindowScopedRejected(WindowScopedSignal),
}

impl TileSignal {
    /// Project a masonry [`RenderRootSignal`] into a [`TileSignal`].
    ///
    /// Lossless for tile-meaningful signals; window-scoped signals are
    /// wrapped in [`TileSignal::WindowScopedRejected`] for the host to
    /// decide whether to re-project (substrate-as-host scope) or warn (tile
    /// scope).
    pub fn from_render_root_signal(signal: RenderRootSignal) -> Self {
        use RenderRootSignal as S;
        match signal {
            S::Action(action, widget_id) => Self::Action { widget_id, action },
            S::RequestRedraw => Self::RequestRedraw,
            S::RequestAnimFrame => Self::RequestAnimFrame,
            S::TakeFocus => Self::TakeFocus,
            S::SetCursor(c) => Self::SetCursor(c),
            S::StartIme => Self::ImeStart,
            S::EndIme => Self::ImeEnd,
            S::ImeMoved(position, size) => Self::ImeMoved { position, size },
            S::ClipboardStore(s) => Self::ClipboardStore(s),
            S::NewLayer(_layer_type, _new_widget, point) => {
                // TODO(wiring): NewLayer carries the widget itself; we drop it
                // for now since substrate-side pop-up handling isn't designed
                // yet. Once the substrate has a pop-up model, project the
                // widget into a child SceneNode.
                Self::NewLayerRequested { at: point }
            }
            S::RepositionLayer(widget, point) => {
                Self::RepositionLayerRequested { widget, at: point }
            }
            S::RemoveLayer(widget) => Self::RemoveLayerRequested { widget },
            S::WidgetSelectedInInspector(widget) => Self::WidgetInspectorPick(widget),

            // Window-scoped signals — wrap and let host decide.
            S::SetSize(s) => Self::WindowScopedRejected(WindowScopedSignal::SetSize(s)),
            S::SetTitle(t) => Self::WindowScopedRejected(WindowScopedSignal::SetTitle(t)),
            S::DragWindow => Self::WindowScopedRejected(WindowScopedSignal::DragWindow),
            S::DragResizeWindow(d) => {
                Self::WindowScopedRejected(WindowScopedSignal::DragResizeWindow(d))
            }
            S::ToggleMaximized => Self::WindowScopedRejected(WindowScopedSignal::ToggleMaximized),
            S::Minimize => Self::WindowScopedRejected(WindowScopedSignal::Minimize),
            S::Exit => Self::WindowScopedRejected(WindowScopedSignal::Exit),
            S::ShowWindowMenu(p) => Self::WindowScopedRejected(WindowScopedSignal::ShowWindowMenu(p)),
        }
    }

    /// Whether this signal makes sense at tile scope (true) or only at
    /// substrate-as-host scope (false).
    pub fn is_window_scoped(&self) -> bool {
        matches!(self, Self::WindowScopedRejected(_))
    }
}

/// Window-decoration / window-control signals masonry can produce.
///
/// Wrapped in [`TileSignal::WindowScopedRejected`]; meaningful at
/// substrate-as-host scope (where the host *is* a window) but rejected at
/// tile scope.
#[derive(Debug)]
pub enum WindowScopedSignal {
    SetSize(PhysicalSize<u32>),
    SetTitle(String),
    DragWindow,
    DragResizeWindow(ResizeDirection),
    ToggleMaximized,
    Minimize,
    Exit,
    ShowWindowMenu(LogicalPosition<f64>),
}

// Suppress an unused-import warning on a type the public surface doesn't
// reference yet but tile.rs will once accessibility wiring lands.
#[allow(dead_code)]
const _: Option<AccessKitNodeId> = None;
