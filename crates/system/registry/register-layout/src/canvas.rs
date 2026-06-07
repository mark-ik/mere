use std::fmt;
use std::str::FromStr;

use super::SurfaceSubsystemCapabilities;
use super::profile_registry::{ProfileRegistry, ProfileResolution};

pub const CANVAS_PROFILE_DEFAULT: &str = "canvas:default";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasTopologyPolicy {
    pub policy_id: String,
    pub directed: bool,
    pub cycles_allowed: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasLayoutAlgorithmPolicy {
    pub algorithm_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasNavigationPolicy {
    pub zoom_and_pan_enabled: bool,
    pub wheel_zoom_requires_ctrl: bool,
    #[serde(default = "default_keyboard_zoom_step")]
    pub keyboard_zoom_step: f32,
    #[serde(default = "default_keyboard_pan_step")]
    pub keyboard_pan_step: f32,
    #[serde(default = "default_wheel_zoom_impulse_scale")]
    pub wheel_zoom_impulse_scale: f32,
    #[serde(default = "default_wheel_zoom_inertia_damping")]
    pub wheel_zoom_inertia_damping: f32,
    #[serde(default = "default_wheel_zoom_inertia_min_abs")]
    pub wheel_zoom_inertia_min_abs: f32,
    #[serde(default = "default_camera_fit_padding")]
    pub camera_fit_padding: f32,
    #[serde(default = "default_camera_fit_relax")]
    pub camera_fit_relax: f32,
    #[serde(default = "default_camera_focus_selection_padding")]
    pub camera_focus_selection_padding: f32,
}

fn default_keyboard_zoom_step() -> f32 {
    1.1
}

fn default_camera_fit_padding() -> f32 {
    1.1
}

fn default_keyboard_pan_step() -> f32 {
    12.0
}

fn default_camera_fit_relax() -> f32 {
    0.5
}

fn default_camera_focus_selection_padding() -> f32 {
    1.2
}

fn default_wheel_zoom_impulse_scale() -> f32 {
    0.012
}

fn default_wheel_zoom_inertia_damping() -> f32 {
    0.86
}

fn default_wheel_zoom_inertia_min_abs() -> f32 {
    0.00035
}

macro_rules! impl_display_from_str {
    ($ty:ty { $($variant:path => $value:literal),+ $(,)? }) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $($variant => f.write_str($value),)+
                }
            }
        }

        impl FromStr for $ty {
            type Err = ();

            fn from_str(raw: &str) -> Result<Self, Self::Err> {
                match raw.trim() {
                    $($value => Ok($variant),)+
                    _ => Err(()),
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CanvasLassoBinding {
    RightDrag,
    ShiftLeftDrag,
}

impl_display_from_str!(CanvasLassoBinding {
    CanvasLassoBinding::RightDrag => "right-drag",
    CanvasLassoBinding::ShiftLeftDrag => "shift-left-drag",
});

impl Default for CanvasLassoBinding {
    fn default() -> Self {
        Self::ShiftLeftDrag
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasInteractionPolicy {
    pub dragging_enabled: bool,
    pub node_selection_enabled: bool,
    pub node_clicking_enabled: bool,
    pub lasso_binding: CanvasLassoBinding,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasStylePolicy {
    pub labels_always: bool,
    /// When `true`, frame-affinity backdrop regions are rendered and the
    /// soft centroid-attraction force is applied for frame members.
    ///
    /// Spec: `layout_behaviors_and_physics_spec.md §4.3`
    #[serde(default)]
    pub frame_affinity_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EdgeLodPolicy {
    Full,
    SkipLabels,
    Hidden,
}

/// Rendering performance and quality policy controls.
///
/// These toggles gate Phase 1 performance optimizations so behavior remains
/// policy-driven rather than hardcoded in render callsites.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasPerformancePolicy {
    /// When true, only nodes within the visible viewport are submitted to the
    /// graph renderer each frame.
    pub viewport_culling_enabled: bool,
    pub label_culling_enabled: bool,
    pub edge_lod: EdgeLodPolicy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CanvasSurfaceProfile {
    pub profile_id: String,
    pub topology: CanvasTopologyPolicy,
    pub layout_algorithm: CanvasLayoutAlgorithmPolicy,
    pub navigation: CanvasNavigationPolicy,
    pub interaction: CanvasInteractionPolicy,
    pub style: CanvasStylePolicy,
    pub performance: CanvasPerformancePolicy,
    /// Folded subsystem conformance declarations for this canvas surface.
    #[serde(flatten)]
    pub subsystems: SurfaceSubsystemCapabilities,
}

impl CanvasSurfaceProfile {
    /// Whether frame-affinity backdrop rendering and soft-force are active.
    ///
    /// Spec: `layout_behaviors_and_physics_spec.md §4.3`
    pub fn zones_enabled(&self) -> bool {
        self.style.frame_affinity_enabled
    }

    pub fn should_capture_wheel_zoom(&self, ctrl_pressed: bool) -> bool {
        !self.navigation.wheel_zoom_requires_ctrl || ctrl_pressed
    }

    pub fn allows_background_pan(
        &self,
        no_hovered_node: bool,
        pointer_inside: bool,
        primary_down: bool,
        lasso_primary_drag_active: bool,
        _radial_open: bool,
        right_button_down: bool,
    ) -> bool {
        let is_tree_topology = self.topology.policy_id == "topology:tree";
        pointer_inside
            && no_hovered_node
            && primary_down
            && !lasso_primary_drag_active
            && self.interaction.dragging_enabled
            && !right_button_down
            && !is_tree_topology
    }
}

pub type CanvasSurfaceResolution = ProfileResolution<CanvasSurfaceProfile>;

pub struct CanvasRegistry {
    profiles: ProfileRegistry<CanvasSurfaceProfile>,
}

impl CanvasRegistry {
    pub fn register(&mut self, profile_id: &str, profile: CanvasSurfaceProfile) {
        self.profiles.register(profile_id, profile);
    }

    pub fn resolve(&self, profile_id: &str) -> CanvasSurfaceResolution {
        self.profiles.resolve(profile_id, "canvas")
    }
}

impl Default for CanvasRegistry {
    fn default() -> Self {
        let mut registry = Self {
            profiles: ProfileRegistry::new(CANVAS_PROFILE_DEFAULT),
        };
        registry.register(
            CANVAS_PROFILE_DEFAULT,
            CanvasSurfaceProfile {
                profile_id: CANVAS_PROFILE_DEFAULT.to_string(),
                topology: CanvasTopologyPolicy {
                    policy_id: "topology:free".to_string(),
                    directed: false,
                    cycles_allowed: true,
                },
                layout_algorithm: CanvasLayoutAlgorithmPolicy {
                    algorithm_id: "graph_layout:force_directed".to_string(),
                },
                navigation: CanvasNavigationPolicy {
                    zoom_and_pan_enabled: true,
                    wheel_zoom_requires_ctrl: false,
                    keyboard_zoom_step: default_keyboard_zoom_step(),
                    keyboard_pan_step: default_keyboard_pan_step(),
                    camera_fit_padding: default_camera_fit_padding(),
                    wheel_zoom_impulse_scale: default_wheel_zoom_impulse_scale(),
                    wheel_zoom_inertia_damping: default_wheel_zoom_inertia_damping(),
                    wheel_zoom_inertia_min_abs: default_wheel_zoom_inertia_min_abs(),
                    camera_fit_relax: default_camera_fit_relax(),
                    camera_focus_selection_padding: default_camera_focus_selection_padding(),
                },
                interaction: CanvasInteractionPolicy {
                    dragging_enabled: true,
                    node_selection_enabled: true,
                    node_clicking_enabled: true,
                    lasso_binding: CanvasLassoBinding::default(),
                },
                style: CanvasStylePolicy {
                    labels_always: true,
                    frame_affinity_enabled: false,
                },
                performance: CanvasPerformancePolicy {
                    viewport_culling_enabled: true,
                    label_culling_enabled: false,
                    edge_lod: EdgeLodPolicy::Full,
                },
                subsystems: SurfaceSubsystemCapabilities::full(),
            },
        );
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_registry_resolves_default_profile() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve(CANVAS_PROFILE_DEFAULT);
        assert!(resolution.matched);
        assert!(!resolution.fallback_used);
        assert_eq!(resolution.resolved_id, CANVAS_PROFILE_DEFAULT);
        assert_eq!(resolution.profile.topology.policy_id, "topology:free");
        assert_eq!(
            resolution.profile.layout_algorithm.algorithm_id,
            "graph_layout:force_directed"
        );
        assert_eq!(
            resolution.profile.interaction.lasso_binding,
            CanvasLassoBinding::ShiftLeftDrag
        );
        assert!(resolution.profile.navigation.zoom_and_pan_enabled);
        assert!(resolution.profile.performance.viewport_culling_enabled);
        // Frame-affinity is off by default — must be explicitly enabled.
        assert!(!resolution.profile.zones_enabled());
    }

    #[test]
    fn canvas_surface_profile_zones_enabled_reflects_style_flag() {
        let mut profile = CanvasRegistry::default()
            .resolve(CANVAS_PROFILE_DEFAULT)
            .profile;
        assert!(!profile.zones_enabled());
        profile.style.frame_affinity_enabled = true;
        assert!(profile.zones_enabled());
    }

    #[test]
    fn canvas_registry_falls_back_for_unknown_profile() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve("canvas:unknown");
        assert!(!resolution.matched);
        assert!(resolution.fallback_used);
        assert_eq!(resolution.resolved_id, CANVAS_PROFILE_DEFAULT);
    }

    #[test]
    fn canvas_registry_default_profile_has_performance_policy() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve(CANVAS_PROFILE_DEFAULT);
        let perf = &resolution.profile.performance;
        assert!(perf.viewport_culling_enabled);
        assert!(!perf.label_culling_enabled);
        assert_eq!(perf.edge_lod, EdgeLodPolicy::Full);
    }

    #[test]
    fn canvas_registry_custom_profile_with_culling_enabled() {
        let mut registry = CanvasRegistry::default();
        registry.register(
            "canvas:perf",
            CanvasSurfaceProfile {
                profile_id: "canvas:perf".to_string(),
                topology: CanvasTopologyPolicy {
                    policy_id: "topology:free".to_string(),
                    directed: false,
                    cycles_allowed: true,
                },
                layout_algorithm: CanvasLayoutAlgorithmPolicy {
                    algorithm_id: "graph_layout:force_directed".to_string(),
                },
                navigation: CanvasNavigationPolicy {
                    zoom_and_pan_enabled: false,
                    wheel_zoom_requires_ctrl: false,
                    keyboard_zoom_step: default_keyboard_zoom_step(),
                    keyboard_pan_step: default_keyboard_pan_step(),
                    camera_fit_padding: default_camera_fit_padding(),
                    wheel_zoom_impulse_scale: default_wheel_zoom_impulse_scale(),
                    wheel_zoom_inertia_damping: default_wheel_zoom_inertia_damping(),
                    wheel_zoom_inertia_min_abs: default_wheel_zoom_inertia_min_abs(),
                    camera_fit_relax: default_camera_fit_relax(),
                    camera_focus_selection_padding: default_camera_focus_selection_padding(),
                },
                interaction: CanvasInteractionPolicy {
                    dragging_enabled: true,
                    node_selection_enabled: true,
                    node_clicking_enabled: true,
                    lasso_binding: CanvasLassoBinding::default(),
                },
                style: CanvasStylePolicy {
                    labels_always: false,
                    frame_affinity_enabled: false,
                },
                performance: CanvasPerformancePolicy {
                    viewport_culling_enabled: true,
                    label_culling_enabled: true,
                    edge_lod: EdgeLodPolicy::SkipLabels,
                },
                subsystems: SurfaceSubsystemCapabilities::full(),
            },
        );
        let resolution = registry.resolve("canvas:perf");
        assert!(resolution.matched);
        let perf = &resolution.profile.performance;
        assert!(perf.viewport_culling_enabled);
        assert!(perf.label_culling_enabled);
        assert_eq!(perf.edge_lod, EdgeLodPolicy::SkipLabels);
    }

    #[test]
    fn canvas_surface_profile_allows_background_pan_when_conditions_met() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve(CANVAS_PROFILE_DEFAULT);
        assert!(
            resolution
                .profile
                .allows_background_pan(true, true, true, false, false, false,)
        );
    }

    #[test]
    fn canvas_surface_profile_blocks_background_pan_for_active_lasso_drag() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve(CANVAS_PROFILE_DEFAULT);
        assert!(
            !resolution
                .profile
                .allows_background_pan(true, true, true, true, false, false,)
        );
    }

    #[test]
    fn canvas_surface_profile_allows_background_pan_when_radial_menu_open() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve(CANVAS_PROFILE_DEFAULT);
        assert!(
            resolution
                .profile
                .allows_background_pan(true, true, true, false, true, false,)
        );
    }

    #[test]
    fn canvas_surface_profile_wheel_zoom_capture_without_ctrl_requirement() {
        let registry = CanvasRegistry::default();
        let resolution = registry.resolve(CANVAS_PROFILE_DEFAULT);
        assert!(resolution.profile.should_capture_wheel_zoom(false));
        assert!(resolution.profile.should_capture_wheel_zoom(true));
        assert!(resolution.profile.navigation.keyboard_zoom_step > 1.0);
        assert!(resolution.profile.navigation.camera_fit_padding > 1.0);
        assert!(resolution.profile.navigation.camera_fit_relax > 0.0);
        assert!(resolution.profile.navigation.camera_focus_selection_padding > 1.0);
        assert!(resolution.profile.navigation.wheel_zoom_impulse_scale > 0.0);
        assert!(resolution.profile.navigation.wheel_zoom_inertia_damping > 0.0);
        assert!(resolution.profile.navigation.wheel_zoom_inertia_min_abs > 0.0);
    }

    #[test]
    fn canvas_surface_profile_wheel_zoom_capture_with_ctrl_requirement() {
        let mut registry = CanvasRegistry::default();
        registry.register(
            "canvas:ctrl-wheel",
            CanvasSurfaceProfile {
                profile_id: "canvas:ctrl-wheel".to_string(),
                topology: CanvasTopologyPolicy {
                    policy_id: "topology:free".to_string(),
                    directed: false,
                    cycles_allowed: true,
                },
                layout_algorithm: CanvasLayoutAlgorithmPolicy {
                    algorithm_id: "graph_layout:force_directed".to_string(),
                },
                navigation: CanvasNavigationPolicy {
                    zoom_and_pan_enabled: false,
                    wheel_zoom_requires_ctrl: true,
                    keyboard_zoom_step: default_keyboard_zoom_step(),
                    keyboard_pan_step: default_keyboard_pan_step(),
                    camera_fit_padding: default_camera_fit_padding(),
                    wheel_zoom_impulse_scale: default_wheel_zoom_impulse_scale(),
                    wheel_zoom_inertia_damping: default_wheel_zoom_inertia_damping(),
                    wheel_zoom_inertia_min_abs: default_wheel_zoom_inertia_min_abs(),
                    camera_fit_relax: default_camera_fit_relax(),
                    camera_focus_selection_padding: default_camera_focus_selection_padding(),
                },
                interaction: CanvasInteractionPolicy {
                    dragging_enabled: true,
                    node_selection_enabled: true,
                    node_clicking_enabled: true,
                    lasso_binding: CanvasLassoBinding::default(),
                },
                style: CanvasStylePolicy {
                    labels_always: true,
                    frame_affinity_enabled: false,
                },
                performance: CanvasPerformancePolicy {
                    viewport_culling_enabled: true,
                    label_culling_enabled: false,
                    edge_lod: EdgeLodPolicy::Full,
                },
                subsystems: SurfaceSubsystemCapabilities::full(),
            },
        );
        let resolution = registry.resolve("canvas:ctrl-wheel");
        assert!(!resolution.profile.should_capture_wheel_zoom(false));
        assert!(resolution.profile.should_capture_wheel_zoom(true));
    }
}
