// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn physics_profile_lookup_maps_legacy_profiles_to_helper_era_ids() {
    let liquid = resolve_physics_profile(PHYSICS_ID_LEGACY_LIQUID);
    assert!(liquid.matched);
    assert_eq!(liquid.resolved_id, PHYSICS_ID_DRIFT);

    let gas = resolve_physics_profile(PHYSICS_ID_LEGACY_GAS);
    assert!(gas.matched);
    assert_eq!(gas.resolved_id, PHYSICS_ID_SCATTER);

    let solid = resolve_physics_profile(PHYSICS_ID_LEGACY_SOLID);
    assert!(solid.matched);
    assert_eq!(solid.resolved_id, PHYSICS_ID_SETTLE);
}

#[test]
fn physics_profile_exports_tuning_for_consumer_application() {
    let profile = PhysicsProfile {
        name: "Custom".to_string(),
        motion: PhysicsMotionTuning {
            repulsion_strength: 0.61,
            attraction_strength: 0.19,
            gravity_strength: 0.27,
            damping: 0.48,
            node_separation: true,
            viewport_containment: false,
            auto_pause: true,
        },
        organizers: PhysicsOrganizerConfig {
            degree_repulsion: Some(DegreeRepulsionConfig::mild()),
            domain_clustering: None,
            semantic_clustering: None,
            hub_pull: None,
        },
    };

    // The runtime bridge (`apply_to_state`) was dropped with the donor's
    // `ForceDirectedState`; lens now exports a `GraphPhysicsTuning` the
    // consumer maps onto the active physics runtime.
    let tuning = profile.graph_physics_tuning();
    assert_eq!(tuning.repulsion_strength, 0.61);
    assert_eq!(tuning.attraction_strength, 0.19);
    assert_eq!(tuning.damping, 0.48);
    assert_eq!(tuning.gravity_strength, 0.27);
}

#[test]
fn physics_profile_exposes_graph_physics_extensions() {
    let profile = PhysicsProfile {
        name: "Custom".to_string(),
        motion: PhysicsMotionTuning::default(),
        organizers: PhysicsOrganizerConfig {
            degree_repulsion: None,
            domain_clustering: Some(DomainClusteringConfig { strength: 0.08 }),
            semantic_clustering: Some(SemanticClusteringConfig {
                strength: 0.23,
                similarity_floor: 0.10,
            }),
            hub_pull: Some(HubPullConfig::default()),
        },
    };

    let extensions = profile.graph_physics_extensions(false);

    assert!(extensions.degree_repulsion.is_none());
    assert_eq!(
        extensions.domain_clustering,
        Some(DomainClusteringConfig { strength: 0.08 })
    );
    assert_eq!(
        extensions.semantic_clustering,
        Some(SemanticClusteringConfig {
            strength: 0.23,
            similarity_floor: 0.10,
        })
    );
    assert_eq!(extensions.hub_pull, Some(HubPullConfig::default()));
    assert!(!extensions.frame_affinity_enabled);

    let collision_policy = profile.scene_collision_policy();
    assert!(collision_policy.node_separation_enabled);
    assert!(collision_policy.viewport_containment_enabled);

    let extensions_zones = profile.graph_physics_extensions(true);
    assert!(extensions_zones.frame_affinity_enabled);
}

#[test]
fn physics_profile_deserializes_legacy_flat_shape() {
    let raw = r#"{
        "name":"Gas",
        "repulsion_strength":0.8,
        "attraction_strength":0.05,
        "gravity_strength":0.0,
        "damping":0.8,
        "degree_repulsion":true,
        "domain_clustering":false,
        "semantic_clustering":false,
        "semantic_strength":0.0,
        "auto_pause":false
    }"#;

    let decoded: PhysicsProfile =
        serde_json::from_str(raw).expect("legacy physics profile should deserialize");

    assert_eq!(decoded.name, "Gas");
    assert_eq!(decoded.motion.repulsion_strength, 0.8);
    assert_eq!(
        decoded.organizers.degree_repulsion,
        Some(DegreeRepulsionConfig::medium())
    );
    assert!(!decoded.motion.auto_pause);
}

#[test]
fn canonical_physics_profile_id_hint_maps_new_and_legacy_names() {
    assert_eq!(
        canonical_physics_profile_id_hint(&PhysicsProfile::drift()),
        PHYSICS_ID_DRIFT
    );
    assert_eq!(
        canonical_physics_profile_id_hint(&PhysicsProfile {
            name: "Gas".to_string(),
            motion: PhysicsMotionTuning::scatter(),
            organizers: PhysicsOrganizerConfig::default(),
        }),
        PHYSICS_ID_SCATTER
    );
}
