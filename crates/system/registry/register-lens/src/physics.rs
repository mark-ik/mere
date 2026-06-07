use std::sync::OnceLock;

// Physics preset configs are owned by this crate (see `physics_config`).
// The donor sourced them from `graph_canvas::physics_config` alongside the
// `ForceDirectedState` runtime they fed; mere's graph-canvas uses a
// different physics model, so lens carries the preset types itself and a
// consumer maps them onto the active runtime.
use crate::physics_config::{
    DegreeRepulsionConfig, DomainClusteringConfig, GraphPhysicsExtensionConfig, GraphPhysicsTuning,
    HubPullConfig, SceneCollisionPolicy, SemanticClusteringConfig,
};

pub const PHYSICS_ID_DRIFT: &str = "physics:drift";
pub const PHYSICS_ID_SCATTER: &str = "physics:scatter";
pub const PHYSICS_ID_SETTLE: &str = "physics:settle";
pub const PHYSICS_ID_ARCHIPELAGO: &str = "physics:archipelago";
pub const PHYSICS_ID_RESONANCE: &str = "physics:resonance";
pub const PHYSICS_ID_CONSTELLATION: &str = "physics:constellation";
pub const PHYSICS_ID_DEFAULT: &str = PHYSICS_ID_DRIFT;

const PHYSICS_ID_LEGACY_DEFAULT: &str = "physics:default";
const PHYSICS_ID_LEGACY_LIQUID: &str = "physics:liquid";
const PHYSICS_ID_LEGACY_GAS: &str = "physics:gas";
const PHYSICS_ID_LEGACY_SOLID: &str = "physics:solid";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PhysicsMotionTuning {
    pub repulsion_strength: f32,
    pub attraction_strength: f32,
    pub gravity_strength: f32,
    pub damping: f32,
    #[serde(default)]
    pub node_separation: bool,
    #[serde(default)]
    pub viewport_containment: bool,
    pub auto_pause: bool,
}

impl Default for PhysicsMotionTuning {
    fn default() -> Self {
        Self::drift()
    }
}

impl PhysicsMotionTuning {
    pub fn drift() -> Self {
        Self {
            repulsion_strength: 0.28,
            attraction_strength: 0.22,
            gravity_strength: 0.18,
            damping: 0.55,
            node_separation: true,
            viewport_containment: true,
            auto_pause: true,
        }
    }

    pub fn scatter() -> Self {
        Self {
            repulsion_strength: 0.80,
            attraction_strength: 0.05,
            gravity_strength: 0.00,
            damping: 0.80,
            node_separation: false,
            viewport_containment: false,
            auto_pause: false,
        }
    }

    pub fn settle() -> Self {
        Self {
            repulsion_strength: 0.12,
            attraction_strength: 0.42,
            gravity_strength: 0.24,
            damping: 0.40,
            node_separation: true,
            viewport_containment: true,
            auto_pause: true,
        }
    }

    pub fn archipelago() -> Self {
        Self {
            repulsion_strength: 0.18,
            attraction_strength: 0.34,
            gravity_strength: 0.12,
            damping: 0.48,
            node_separation: true,
            viewport_containment: true,
            auto_pause: true,
        }
    }

    pub fn resonance() -> Self {
        Self {
            repulsion_strength: 0.20,
            attraction_strength: 0.28,
            gravity_strength: 0.16,
            damping: 0.50,
            node_separation: true,
            viewport_containment: true,
            auto_pause: true,
        }
    }

    pub fn constellation() -> Self {
        Self {
            repulsion_strength: 0.16,
            attraction_strength: 0.30,
            gravity_strength: 0.16,
            damping: 0.46,
            node_separation: true,
            viewport_containment: true,
            auto_pause: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct PhysicsOrganizerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degree_repulsion: Option<DegreeRepulsionConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_clustering: Option<DomainClusteringConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_clustering: Option<SemanticClusteringConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_pull: Option<HubPullConfig>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PhysicsProfile {
    pub name: String,
    pub motion: PhysicsMotionTuning,
    #[serde(default)]
    pub organizers: PhysicsOrganizerConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsProfileDescriptor {
    pub id: String,
    pub display_name: String,
    pub summary: String,
    pub profile: PhysicsProfile,
}

#[derive(Debug, Clone)]
pub struct PhysicsProfileResolution {
    pub requested_id: String,
    pub resolved_id: String,
    pub matched: bool,
    pub fallback_used: bool,
    pub display_name: String,
    pub summary: String,
    pub profile: PhysicsProfile,
}

#[derive(Debug, serde::Deserialize)]
struct PhysicsProfileSerde {
    #[serde(default)]
    name: String,
    #[serde(default)]
    motion: Option<PhysicsMotionTuning>,
    #[serde(default)]
    organizers: Option<PhysicsOrganizerConfig>,
    #[serde(default)]
    repulsion_strength: Option<f32>,
    #[serde(default)]
    attraction_strength: Option<f32>,
    #[serde(default)]
    gravity_strength: Option<f32>,
    #[serde(default)]
    damping: Option<f32>,
    #[serde(default)]
    degree_repulsion: Option<bool>,
    #[serde(default)]
    domain_clustering: Option<bool>,
    #[serde(default)]
    semantic_clustering: Option<bool>,
    #[serde(default)]
    semantic_strength: Option<f32>,
    #[serde(default)]
    node_separation: Option<bool>,
    #[serde(default)]
    viewport_containment: Option<bool>,
    #[serde(default)]
    auto_pause: Option<bool>,
}

impl<'de> serde::Deserialize<'de> for PhysicsProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = PhysicsProfileSerde::deserialize(deserializer)?;
        let PhysicsProfileSerde {
            name,
            motion,
            organizers,
            repulsion_strength,
            attraction_strength,
            gravity_strength,
            damping,
            degree_repulsion,
            domain_clustering,
            semantic_clustering,
            semantic_strength,
            node_separation,
            viewport_containment,
            auto_pause,
        } = raw;

        let has_legacy_motion = repulsion_strength.is_some()
            || attraction_strength.is_some()
            || gravity_strength.is_some()
            || damping.is_some()
            || node_separation.is_some()
            || viewport_containment.is_some()
            || auto_pause.is_some();
        let motion = if let Some(motion) = motion {
            motion
        } else if has_legacy_motion {
            PhysicsMotionTuning {
                repulsion_strength: repulsion_strength.unwrap_or(0.28),
                attraction_strength: attraction_strength.unwrap_or(0.22),
                gravity_strength: gravity_strength.unwrap_or(0.18),
                damping: damping.unwrap_or(0.55),
                node_separation: node_separation.unwrap_or(false),
                viewport_containment: viewport_containment.unwrap_or(false),
                auto_pause: auto_pause.unwrap_or(true),
            }
        } else {
            PhysicsMotionTuning::default()
        };

        let has_legacy_organizers = degree_repulsion.unwrap_or(false)
            || domain_clustering.unwrap_or(false)
            || semantic_clustering.unwrap_or(false);
        let organizers = if let Some(organizers) = organizers {
            organizers
        } else if has_legacy_organizers {
            PhysicsOrganizerConfig {
                degree_repulsion: degree_repulsion
                    .unwrap_or(false)
                    .then(DegreeRepulsionConfig::medium),
                domain_clustering: domain_clustering
                    .unwrap_or(false)
                    .then_some(DomainClusteringConfig { strength: 0.04 }),
                semantic_clustering: semantic_clustering.unwrap_or(false).then_some(
                    SemanticClusteringConfig {
                        strength: semantic_strength.unwrap_or(0.05),
                        similarity_floor: 0.10,
                    },
                ),
                hub_pull: None,
            }
        } else {
            PhysicsOrganizerConfig::default()
        };

        Ok(Self {
            name,
            motion,
            organizers,
        })
    }
}

impl Default for PhysicsProfile {
    fn default() -> Self {
        Self::drift()
    }
}

impl PhysicsProfile {
    pub fn graph_physics_tuning(&self) -> GraphPhysicsTuning {
        GraphPhysicsTuning {
            repulsion_strength: self.motion.repulsion_strength,
            attraction_strength: self.motion.attraction_strength,
            gravity_strength: self.motion.gravity_strength,
            damping: self.motion.damping,
        }
    }

    pub fn graph_physics_extensions(
        &self,
        frame_affinity_enabled: bool,
    ) -> GraphPhysicsExtensionConfig {
        GraphPhysicsExtensionConfig {
            degree_repulsion: self.organizers.degree_repulsion,
            domain_clustering: self.organizers.domain_clustering,
            semantic_clustering: self.organizers.semantic_clustering,
            hub_pull: self.organizers.hub_pull,
            frame_affinity_enabled,
        }
    }

    pub fn scene_collision_policy(&self) -> SceneCollisionPolicy {
        SceneCollisionPolicy {
            node_separation_enabled: self.motion.node_separation,
            viewport_containment_enabled: self.motion.viewport_containment,
            node_padding: 4.0,
            region_effect_scale: 1.0,
            containment_response_scale: 1.0,
        }
    }

    pub fn drift() -> Self {
        Self {
            name: "Drift".to_string(),
            motion: PhysicsMotionTuning::drift(),
            organizers: PhysicsOrganizerConfig::default(),
        }
    }

    pub fn scatter() -> Self {
        Self {
            name: "Scatter".to_string(),
            motion: PhysicsMotionTuning::scatter(),
            organizers: PhysicsOrganizerConfig::default(),
        }
    }

    pub fn settle() -> Self {
        Self {
            name: "Settle".to_string(),
            motion: PhysicsMotionTuning::settle(),
            organizers: PhysicsOrganizerConfig {
                degree_repulsion: Some(DegreeRepulsionConfig::mild()),
                ..PhysicsOrganizerConfig::default()
            },
        }
    }

    pub fn archipelago() -> Self {
        Self {
            name: "Archipelago".to_string(),
            motion: PhysicsMotionTuning::archipelago(),
            organizers: PhysicsOrganizerConfig {
                degree_repulsion: Some(DegreeRepulsionConfig::mild()),
                domain_clustering: Some(DomainClusteringConfig { strength: 0.08 }),
                ..PhysicsOrganizerConfig::default()
            },
        }
    }

    pub fn resonance() -> Self {
        Self {
            name: "Resonance".to_string(),
            motion: PhysicsMotionTuning::resonance(),
            organizers: PhysicsOrganizerConfig {
                semantic_clustering: Some(SemanticClusteringConfig {
                    strength: 0.12,
                    similarity_floor: 0.10,
                }),
                ..PhysicsOrganizerConfig::default()
            },
        }
    }

    pub fn constellation() -> Self {
        Self {
            name: "Constellation".to_string(),
            motion: PhysicsMotionTuning::constellation(),
            organizers: PhysicsOrganizerConfig {
                degree_repulsion: Some(DegreeRepulsionConfig::medium()),
                hub_pull: Some(HubPullConfig::default()),
                ..PhysicsOrganizerConfig::default()
            },
        }
    }

}

pub fn physics_profile_descriptors() -> &'static [PhysicsProfileDescriptor] {
    static DESCRIPTORS: OnceLock<Vec<PhysicsProfileDescriptor>> = OnceLock::new();
    DESCRIPTORS
        .get_or_init(|| {
            vec![
                PhysicsProfileDescriptor {
                    id: PHYSICS_ID_DRIFT.to_string(),
                    display_name: "Drift".to_string(),
                    summary: "Gentle browsing motion with bounded drift.".to_string(),
                    profile: PhysicsProfile::drift(),
                },
                PhysicsProfileDescriptor {
                    id: PHYSICS_ID_SCATTER.to_string(),
                    display_name: "Scatter".to_string(),
                    summary: "Broad overview spread for imports and zoomed-out scans.".to_string(),
                    profile: PhysicsProfile::scatter(),
                },
                PhysicsProfileDescriptor {
                    id: PHYSICS_ID_SETTLE.to_string(),
                    display_name: "Settle".to_string(),
                    summary: "Quickly stabilizes into a readable working set.".to_string(),
                    profile: PhysicsProfile::settle(),
                },
                PhysicsProfileDescriptor {
                    id: PHYSICS_ID_ARCHIPELAGO.to_string(),
                    display_name: "Archipelago".to_string(),
                    summary: "Forms separated domain islands with open space between them."
                        .to_string(),
                    profile: PhysicsProfile::archipelago(),
                },
                PhysicsProfileDescriptor {
                    id: PHYSICS_ID_RESONANCE.to_string(),
                    display_name: "Resonance".to_string(),
                    summary: "Pulls semantically related nodes into soft neighborhoods."
                        .to_string(),
                    profile: PhysicsProfile::resonance(),
                },
                PhysicsProfileDescriptor {
                    id: PHYSICS_ID_CONSTELLATION.to_string(),
                    display_name: "Constellation".to_string(),
                    summary: "Makes hubs legible anchors without collapsing nearby leaves."
                        .to_string(),
                    profile: PhysicsProfile::constellation(),
                },
            ]
        })
        .as_slice()
}

pub fn resolve_physics_profile(requested_id: &str) -> PhysicsProfileResolution {
    let requested = requested_id.trim().to_ascii_lowercase();
    let fallback = descriptor_by_id(PHYSICS_ID_DEFAULT)
        .cloned()
        .expect("default physics profile must exist");

    if requested.is_empty() {
        return PhysicsProfileResolution {
            requested_id: requested,
            resolved_id: fallback.id.clone(),
            matched: false,
            fallback_used: true,
            display_name: fallback.display_name.clone(),
            summary: fallback.summary.clone(),
            profile: fallback.profile.clone(),
        };
    }

    if let Some(canonical_id) = canonical_physics_profile_alias(&requested)
        && let Some(descriptor) = descriptor_by_id(canonical_id)
    {
        return PhysicsProfileResolution {
            requested_id: requested,
            resolved_id: descriptor.id.clone(),
            matched: true,
            fallback_used: false,
            display_name: descriptor.display_name.clone(),
            summary: descriptor.summary.clone(),
            profile: descriptor.profile.clone(),
        };
    }

    PhysicsProfileResolution {
        requested_id: requested,
        resolved_id: fallback.id.clone(),
        matched: false,
        fallback_used: true,
        display_name: fallback.display_name,
        summary: fallback.summary,
        profile: fallback.profile,
    }
}

pub fn canonical_physics_profile_id_hint(profile: &PhysicsProfile) -> &'static str {
    let normalized_name = profile.name.trim().to_ascii_lowercase();
    match normalized_name.as_str() {
        "drift" | "default" | "liquid" => PHYSICS_ID_DRIFT,
        "scatter" | "gas" => PHYSICS_ID_SCATTER,
        "settle" | "solid" => PHYSICS_ID_SETTLE,
        "archipelago" => PHYSICS_ID_ARCHIPELAGO,
        "resonance" => PHYSICS_ID_RESONANCE,
        "constellation" => PHYSICS_ID_CONSTELLATION,
        _ if *profile == PhysicsProfile::scatter() => PHYSICS_ID_SCATTER,
        _ if *profile == PhysicsProfile::settle() => PHYSICS_ID_SETTLE,
        _ if *profile == PhysicsProfile::archipelago() => PHYSICS_ID_ARCHIPELAGO,
        _ if *profile == PhysicsProfile::resonance() => PHYSICS_ID_RESONANCE,
        _ if *profile == PhysicsProfile::constellation() => PHYSICS_ID_CONSTELLATION,
        _ => PHYSICS_ID_DRIFT,
    }
}

fn descriptor_by_id(profile_id: &str) -> Option<&'static PhysicsProfileDescriptor> {
    physics_profile_descriptors()
        .iter()
        .find(|descriptor| descriptor.id == profile_id)
}

fn canonical_physics_profile_alias(requested: &str) -> Option<&'static str> {
    match requested {
        PHYSICS_ID_DRIFT | PHYSICS_ID_LEGACY_DEFAULT | PHYSICS_ID_LEGACY_LIQUID => {
            Some(PHYSICS_ID_DRIFT)
        }
        PHYSICS_ID_SCATTER | PHYSICS_ID_LEGACY_GAS => Some(PHYSICS_ID_SCATTER),
        PHYSICS_ID_SETTLE | PHYSICS_ID_LEGACY_SOLID => Some(PHYSICS_ID_SETTLE),
        PHYSICS_ID_ARCHIPELAGO => Some(PHYSICS_ID_ARCHIPELAGO),
        PHYSICS_ID_RESONANCE => Some(PHYSICS_ID_RESONANCE),
        PHYSICS_ID_CONSTELLATION => Some(PHYSICS_ID_CONSTELLATION),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
