/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scripting hook types for scene object scripts.
//!
//! These are pure data types that define the contract between graph-canvas and
//! a host-side script runtime (e.g. Wasmtime/Extism). graph-canvas defines the
//! types; the host implements the runtime and packs script outputs into
//! `CanvasSceneInput.scene_objects`.
//!
//! Scripts produce draw items, hit shapes, and overlay items for their scene
//! objects. They cannot mutate graph truth directly — they emit presentation
//! and interaction data that flows through the same canvas packet and action
//! model as nodes and edges.

use euclid::default::Vector2D;
use serde::{Deserialize, Serialize};

use crate::packet::SceneDrawItem;

// ── Identifiers ───────────────────────────────────────────────────────────

/// Opaque identifier for a scripted scene object.
///
/// Assigned by the host. The canvas uses these to reference scene objects in
/// hit proxies, actions, and interaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneObjectId(pub u64);

// ── Hit shapes ────────────────────────────────────────────────────────────

/// Interaction surface shape for a scene object.
///
/// Defines the clickable/hoverable area around the object's position.
/// The canvas uses this to generate `HitProxy::SceneObject` entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneObjectHitShape {
    /// Circular hit area centered at the object position.
    Circle { radius: f32 },
    /// Rectangular hit area centered at the object position.
    Rect { half_extents: Vector2D<f32> },
}

// ── Capabilities ──────────────────────────────────────────────────────────

/// Capability flags declaring what a script is allowed to produce or read.
///
/// The host sets these based on the script's manifest. graph-canvas can use
/// them to validate scene object content and emit diagnostics for
/// capability-blocked operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptCapability {
    /// Script may produce draw items (visual content).
    pub emit_draw_items: bool,
    /// Script may produce a hit proxy (interaction surface).
    pub emit_hit_proxy: bool,
    /// Script may produce overlay items (drawn on top of the scene).
    pub emit_overlays: bool,
    /// Script may read node positions from the scene context.
    pub read_node_positions: bool,
    /// Script may read the current scene mode.
    pub read_scene_mode: bool,
}

impl ScriptCapability {
    /// All capabilities enabled.
    pub const fn all() -> Self {
        Self {
            emit_draw_items: true,
            emit_hit_proxy: true,
            emit_overlays: true,
            read_node_positions: true,
            read_scene_mode: true,
        }
    }

    /// No capabilities enabled (fully sandboxed).
    pub const fn none() -> Self {
        Self {
            emit_draw_items: false,
            emit_hit_proxy: false,
            emit_overlays: false,
            read_node_positions: false,
            read_scene_mode: false,
        }
    }
}

impl Default for ScriptCapability {
    fn default() -> Self {
        Self::all()
    }
}

// ── Diagnostics ───────────────────────────────────────────────────────────

/// Severity level for script diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Info,
    Warn,
    Error,
}

/// Diagnostic emitted when a capability is blocked or a script misbehaves.
///
/// The host surfaces these through its diagnostics channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source_id: SceneObjectId,
}

// ── Script output ─────────────────────────────────────────────────────────

/// Per-frame output from a scripted scene object.
///
/// The host runs the script, collects this output, and packs it into a
/// `CanvasSceneObject` for inclusion in `CanvasSceneInput.scene_objects`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectOutput {
    /// Draw items produced by the script (world-layer content).
    /// Positions are relative to the scene object's position.
    pub draw_items: Vec<SceneDrawItem>,
    /// Interaction surface shape. `None` means the object is not interactive.
    pub hit_shape: Option<SceneObjectHitShape>,
    /// Overlay items drawn on top of the scene (e.g. tooltips, badges).
    /// Positions are relative to the scene object's position.
    pub overlay_items: Vec<SceneDrawItem>,
    /// Diagnostics emitted during this frame.
    pub diagnostics: Vec<ScriptDiagnostic>,
}

impl Default for SceneObjectOutput {
    fn default() -> Self {
        Self {
            draw_items: Vec::new(),
            hit_shape: None,
            overlay_items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::Color;
    use euclid::default::Point2D;

    #[test]
    fn scene_object_id_equality() {
        assert_eq!(SceneObjectId(1), SceneObjectId(1));
        assert_ne!(SceneObjectId(1), SceneObjectId(2));
    }

    #[test]
    fn script_capability_all_enables_everything() {
        let cap = ScriptCapability::all();
        assert!(cap.emit_draw_items);
        assert!(cap.emit_hit_proxy);
        assert!(cap.emit_overlays);
        assert!(cap.read_node_positions);
        assert!(cap.read_scene_mode);
    }

    #[test]
    fn script_capability_none_disables_everything() {
        let cap = ScriptCapability::none();
        assert!(!cap.emit_draw_items);
        assert!(!cap.emit_hit_proxy);
        assert!(!cap.emit_overlays);
        assert!(!cap.read_node_positions);
        assert!(!cap.read_scene_mode);
    }

    #[test]
    fn script_capability_default_is_all() {
        assert_eq!(ScriptCapability::default(), ScriptCapability::all());
    }

    #[test]
    fn scene_object_output_default_is_empty() {
        let output = SceneObjectOutput::default();
        assert!(output.draw_items.is_empty());
        assert!(output.hit_shape.is_none());
        assert!(output.overlay_items.is_empty());
        assert!(output.diagnostics.is_empty());
    }

    #[test]
    fn serde_roundtrip_scene_object_id() {
        let id = SceneObjectId(42);
        let json = serde_json::to_string(&id).unwrap();
        let back: SceneObjectId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn serde_roundtrip_hit_shape() {
        let shapes = vec![
            SceneObjectHitShape::Circle { radius: 16.0 },
            SceneObjectHitShape::Rect {
                half_extents: Vector2D::new(20.0, 10.0),
            },
        ];
        let json = serde_json::to_string(&shapes).unwrap();
        let back: Vec<SceneObjectHitShape> = serde_json::from_str(&json).unwrap();
        assert_eq!(shapes, back);
    }

    #[test]
    fn serde_roundtrip_script_capability() {
        let cap = ScriptCapability {
            emit_draw_items: true,
            emit_hit_proxy: false,
            emit_overlays: true,
            read_node_positions: false,
            read_scene_mode: true,
        };
        let json = serde_json::to_string(&cap).unwrap();
        let back: ScriptCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, back);
    }

    #[test]
    fn serde_roundtrip_script_diagnostic() {
        let diag = ScriptDiagnostic {
            severity: DiagnosticSeverity::Warn,
            message: "capability blocked: emit_hit_proxy".into(),
            source_id: SceneObjectId(7),
        };
        let json = serde_json::to_string(&diag).unwrap();
        let back: ScriptDiagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(diag, back);
    }

    #[test]
    fn serde_roundtrip_scene_object_output() {
        let output = SceneObjectOutput {
            draw_items: vec![SceneDrawItem::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                fill: Color::new(1.0, 0.0, 0.0, 1.0),
                stroke: None,
            }],
            hit_shape: Some(SceneObjectHitShape::Circle { radius: 12.0 }),
            overlay_items: vec![],
            diagnostics: vec![ScriptDiagnostic {
                severity: DiagnosticSeverity::Info,
                message: "script loaded".into(),
                source_id: SceneObjectId(1),
            }],
        };
        let json = serde_json::to_string(&output).unwrap();
        let back: SceneObjectOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output, back);
    }
}
