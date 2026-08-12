// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portable node-primitive and host-behavior vocabulary.
//!
//! A graph class does not prescribe application truth or execute a script. It
//! selects a presentation profile: the body a host may draw and collide, plus
//! named host interactions it may bind. Endpoint-authored domain actions remain
//! Graphshell presentation offers; these bindings cover graph-host mechanics
//! such as inspect, follow, recenter, and pin.

use serde::{Deserialize, Serialize};

/// Stable schema for the built-in graph representation registry.
pub const GRAPH_REPRESENTATION_REGISTRY_SCHEMA: &str = "mere.graph-representation-registry/v1";

/// A portable silhouette. The body and hard collider use the same geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveBody {
    Square,
    RoundedSquare,
    Circle,
    Diamond,
    Hexagon,
}

/// One named primitive a renderer can realize in its own paint system.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveProfile {
    pub id: String,
    pub label: String,
    pub body: PrimitiveBody,
}

/// A host-level gesture. This is a binding surface, not executable script.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostGesture {
    Select,
    Activate,
    Drag,
    Follow,
    Compare,
}

/// Graph-host behavior that survives renderer replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostBehavior {
    Inspect,
    OpenPresentation,
    Pin,
    RecenterNeighborhood,
    FollowRelations,
    CompareReplacement,
}

/// Connects one gesture to one host behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorBinding {
    pub gesture: HostGesture,
    pub behavior: HostBehavior,
}

/// Presentation profile selected by one or more graph class labels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentationProfile {
    pub id: String,
    pub classes: Vec<String>,
    pub primitive: PrimitiveProfile,
    pub behaviors: Vec<BehaviorBinding>,
}

/// Ordered portable registry. Unknown classes resolve to `fallback`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphRepresentationRegistry {
    pub schema: String,
    pub profiles: Vec<RepresentationProfile>,
    pub fallback: RepresentationProfile,
}

impl GraphRepresentationRegistry {
    /// Resolve a graph class without promoting the class into presentation truth.
    pub fn resolve(&self, class: &str) -> &RepresentationProfile {
        self.profiles
            .iter()
            .find(|profile| profile.classes.iter().any(|candidate| candidate == class))
            .unwrap_or(&self.fallback)
    }
}

/// Mere's renderer-neutral baseline registry.
pub fn default_graph_representation_registry() -> GraphRepresentationRegistry {
    let inspect = BehaviorBinding {
        gesture: HostGesture::Select,
        behavior: HostBehavior::Inspect,
    };
    let pin = BehaviorBinding {
        gesture: HostGesture::Drag,
        behavior: HostBehavior::Pin,
    };
    let follow = BehaviorBinding {
        gesture: HostGesture::Follow,
        behavior: HostBehavior::FollowRelations,
    };
    let open = BehaviorBinding {
        gesture: HostGesture::Activate,
        behavior: HostBehavior::OpenPresentation,
    };

    GraphRepresentationRegistry {
        schema: GRAPH_REPRESENTATION_REGISTRY_SCHEMA.to_owned(),
        profiles: vec![
            profile(
                "software",
                &["product", "platform", "software", "foundation"],
                "hexagonal-software",
                "hexagonal software body + collider",
                PrimitiveBody::Hexagon,
                &[inspect, open, pin, follow],
            ),
            profile(
                "device",
                &["device", "tool"],
                "rounded-device",
                "rounded device body + collider",
                PrimitiveBody::RoundedSquare,
                &[inspect, pin, follow],
            ),
            profile(
                "document",
                &["document", "page", "note"],
                "square-document",
                "square document face + collider",
                PrimitiveBody::Square,
                &[inspect, open, pin, follow],
            ),
            profile(
                "event",
                &["event"],
                "diamond-event",
                "diamond event hull + collider",
                PrimitiveBody::Diamond,
                &[inspect, pin, follow],
            ),
            profile(
                "actor",
                &["place", "person", "community"],
                "circular-actor",
                "circular actor + collider",
                PrimitiveBody::Circle,
                &[inspect, pin, follow],
            ),
        ],
        fallback: profile(
            "unknown",
            &[],
            "circular-unknown",
            "circular unknown body + collider",
            PrimitiveBody::Circle,
            &[inspect, pin, follow],
        ),
    }
}

fn profile(
    id: &str,
    classes: &[&str],
    primitive_id: &str,
    primitive_label: &str,
    body: PrimitiveBody,
    behaviors: &[BehaviorBinding],
) -> RepresentationProfile {
    RepresentationProfile {
        id: id.to_owned(),
        classes: classes.iter().map(|class| (*class).to_owned()).collect(),
        primitive: PrimitiveProfile {
            id: primitive_id.to_owned(),
            label: primitive_label.to_owned(),
            body,
        },
        behaviors: behaviors.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_class_has_one_profile_and_unknown_classes_fall_back() {
        let registry = default_graph_representation_registry();
        let mut classes = BTreeSet::new();
        for profile in &registry.profiles {
            for class in &profile.classes {
                assert!(classes.insert(class), "class {class} appears twice");
            }
        }
        assert_eq!(
            registry.resolve("document").primitive.body,
            PrimitiveBody::Square
        );
        assert_eq!(
            registry.resolve("future-class").primitive.body,
            PrimitiveBody::Circle
        );
    }

    #[test]
    fn primitive_and_behavior_axes_are_serializable_and_distinct() {
        let registry = default_graph_representation_registry();
        let encoded = serde_json::to_value(&registry).expect("registry serializes");
        assert_eq!(encoded["schema"], GRAPH_REPRESENTATION_REGISTRY_SCHEMA);
        assert_eq!(encoded["profiles"][0]["primitive"]["body"], "hexagon");
        assert_eq!(
            encoded["profiles"][0]["behaviors"][0]["behavior"],
            "inspect"
        );
    }

    #[test]
    fn host_bindings_do_not_claim_endpoint_domain_actions() {
        let registry = default_graph_representation_registry();
        let encoded = serde_json::to_string(&registry).expect("registry serializes");
        for forbidden in ["domain_truth", "external_effect", "payload_schema"] {
            assert!(!encoded.contains(forbidden));
        }
    }
}
