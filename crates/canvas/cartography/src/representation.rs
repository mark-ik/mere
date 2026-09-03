// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable node-primitive and host-behavior vocabulary.
//!
//! A graph class does not prescribe application truth or execute a script. It
//! selects a presentation profile: the body a host may draw and collide, plus
//! named host interactions it may bind. Endpoint-authored domain actions remain
//! Graphshell presentation offers; these bindings cover graph-host mechanics
//! such as inspect, follow, recenter, and pin.

use sceno::Representation;
use serde::{Deserialize, Serialize};

/// Stable schema for the built-in graph representation registry.
pub const GRAPH_REPRESENTATION_REGISTRY_SCHEMA: &str = "mere.graph-representation-registry/v2";

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

/// A host fact that can select one rung of a representation ladder.
///
/// Screen measures describe the realized item, in pixels. `ZoomLevel` and
/// `Recency` are host-side view facts; recency is normalized to `0..=1`, newest
/// first. `Focused` reads as `1.0` or `0.0`, which keeps every condition in one
/// small comparison grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepresentationMeasure {
    ScreenWidth,
    ScreenHeight,
    ZoomLevel,
    Recency,
    Focused,
}

/// Numeric comparisons understood by representation conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperation {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

/// One declarative test for entering or retaining a representation rung.
///
/// `hysteresis` relaxes the threshold only while this condition's rung is
/// already selected. A card selected by `zoom_level >= 1.0` with `0.1`
/// hysteresis therefore remains a card down to zoom `0.9`, but a glyph still
/// has to reach `1.0` before it becomes a card.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepresentationCondition {
    pub measure: RepresentationMeasure,
    pub operation: ConditionOperation,
    pub threshold: f32,
    pub hysteresis: f32,
}

/// The host facts evaluated by a representation ladder for one item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepresentationState {
    pub screen_width: f32,
    pub screen_height: f32,
    pub zoom_level: f32,
    pub recency: f32,
    pub focused: bool,
}

impl RepresentationState {
    fn measure(self, measure: RepresentationMeasure) -> f32 {
        match measure {
            RepresentationMeasure::ScreenWidth => self.screen_width,
            RepresentationMeasure::ScreenHeight => self.screen_height,
            RepresentationMeasure::ZoomLevel => self.zoom_level,
            RepresentationMeasure::Recency => self.recency,
            RepresentationMeasure::Focused => {
                if self.focused {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

impl RepresentationCondition {
    /// Evaluate this condition. Invalid numeric input never selects a rung.
    pub fn matches(self, state: RepresentationState, retaining: bool) -> bool {
        let value = state.measure(self.measure);
        if !value.is_finite() || !self.threshold.is_finite() || !self.hysteresis.is_finite() {
            return false;
        }
        let padding = if retaining {
            self.hysteresis.max(0.0)
        } else {
            0.0
        };
        match self.operation {
            ConditionOperation::LessThan => value < self.threshold + padding,
            ConditionOperation::LessThanOrEqual => value <= self.threshold + padding,
            ConditionOperation::GreaterThan => value > self.threshold - padding,
            ConditionOperation::GreaterThanOrEqual => value >= self.threshold - padding,
        }
    }
}

/// One target representation and the conditions that all must hold to use it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepresentationRung {
    pub representation: Representation,
    pub conditions: Vec<RepresentationCondition>,
}

/// Ordered representation policy. The first matching rung wins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepresentationLadder {
    pub rungs: Vec<RepresentationRung>,
    pub fallback: Representation,
}

impl RepresentationLadder {
    /// Select a rung from host facts, retaining `previous` through its declared
    /// hysteresis band when possible.
    pub fn select(
        &self,
        state: RepresentationState,
        previous: Option<&Representation>,
    ) -> Representation {
        self.rungs
            .iter()
            .find(|rung| {
                let retaining = previous == Some(&rung.representation);
                !rung.conditions.is_empty()
                    && rung
                        .conditions
                        .iter()
                        .all(|condition| condition.matches(state, retaining))
            })
            .map(|rung| rung.representation.clone())
            .unwrap_or_else(|| self.fallback.clone())
    }
}

/// Presentation profile selected by one or more graph class labels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepresentationProfile {
    pub id: String,
    pub classes: Vec<String>,
    pub primitive: PrimitiveProfile,
    pub behaviors: Vec<BehaviorBinding>,
    /// Host-side policy for choosing the `sceno` representation slot.
    pub ladder: RepresentationLadder,
}

/// Ordered serialized registry. Unknown classes resolve to `fallback`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

    /// Resolve a set of class labels in registry order. This makes selection
    /// deterministic even when the source stores its labels in a hash set.
    pub fn resolve_classes<'a>(
        &self,
        classes: impl IntoIterator<Item = &'a str>,
    ) -> &RepresentationProfile {
        let classes: Vec<&str> = classes.into_iter().collect();
        self.profiles
            .iter()
            .find(|profile| {
                profile
                    .classes
                    .iter()
                    .any(|candidate| classes.contains(&candidate.as_str()))
            })
            .unwrap_or(&self.fallback)
    }
}

/// Mere's first host-side LOD ladder: focused content stays live; recent
/// content earns a card at close zoom; everything else is a glyph.
pub fn default_representation_ladder() -> RepresentationLadder {
    RepresentationLadder {
        rungs: vec![
            RepresentationRung {
                representation: Representation::LivePane,
                conditions: vec![RepresentationCondition {
                    measure: RepresentationMeasure::Focused,
                    operation: ConditionOperation::GreaterThanOrEqual,
                    threshold: 1.0,
                    hysteresis: 0.0,
                }],
            },
            RepresentationRung {
                representation: Representation::Card,
                conditions: vec![
                    RepresentationCondition {
                        measure: RepresentationMeasure::ScreenWidth,
                        operation: ConditionOperation::GreaterThan,
                        threshold: 0.0,
                        hysteresis: 0.0,
                    },
                    RepresentationCondition {
                        measure: RepresentationMeasure::Recency,
                        operation: ConditionOperation::GreaterThanOrEqual,
                        threshold: 0.5,
                        hysteresis: 0.0,
                    },
                    RepresentationCondition {
                        measure: RepresentationMeasure::ZoomLevel,
                        operation: ConditionOperation::GreaterThanOrEqual,
                        threshold: 1.0,
                        hysteresis: 0.1,
                    },
                ],
            },
        ],
        fallback: Representation::Glyph,
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
        ladder: default_representation_ladder(),
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
        assert_eq!(
            encoded["profiles"][0]["ladder"]["rungs"][1]["conditions"][2]["measure"],
            "zoom_level"
        );
        assert_eq!(
            serde_json::from_value::<GraphRepresentationRegistry>(encoded)
                .expect("registry deserializes"),
            registry
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

    #[test]
    fn profile_ladder_selects_from_data_and_retains_through_hysteresis() {
        let ladder = default_representation_ladder();
        let state = |zoom_level, focused| RepresentationState {
            screen_width: 64.0 * zoom_level,
            screen_height: 64.0 * zoom_level,
            zoom_level,
            recency: 1.0,
            focused,
        };

        assert_eq!(ladder.select(state(1.0, false), None), Representation::Card);
        assert_eq!(
            ladder.select(state(0.95, false), Some(&Representation::Card)),
            Representation::Card
        );
        assert_eq!(
            ladder.select(state(0.89, false), Some(&Representation::Card)),
            Representation::Glyph
        );
        assert_eq!(
            ladder.select(state(0.95, false), Some(&Representation::Glyph)),
            Representation::Glyph
        );
        assert_eq!(
            ladder.select(state(0.4, true), Some(&Representation::Glyph)),
            Representation::LivePane
        );
    }

    #[test]
    fn class_set_resolution_follows_registry_order() {
        let registry = default_graph_representation_registry();
        let profile = registry.resolve_classes(["device", "document"]);
        assert_eq!(profile.id, "device");
    }
}
