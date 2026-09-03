// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable ways to read one graph authority.
//!
//! A reading selects actors, a presentation surface, and an emphasis. It does
//! not own graph truth, node placement, or renderer code. Arrangements remain
//! a separate choice except where the reading has one necessary projection.

use serde::{Deserialize, Serialize};

/// Stable schema for Mere's built-in graph-reading registry.
pub const GRAPH_READING_REGISTRY_SCHEMA: &str = "mere.graph-reading-registry/v1";

/// Which actors a reading projects from the supplied authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorScope {
    /// Every actor in the selected authority revision.
    All,
    /// The selected revision compared with its immediate predecessor.
    AdjacentRevision,
    /// The focus actor and actors joined to it by one relation.
    FocusAndNeighbors,
}

/// The renderer-neutral surface required by a reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingSurface {
    /// Positioned graph actors and routed relations.
    Spatial,
    /// Exact source-by-target relation lookup.
    RelationMatrix,
}

/// The fact a renderer should make most legible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingEmphasis {
    Identity,
    Change,
    Activity,
    FocusDistance,
    Relation,
}

/// One portable reading of graph authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphReadingProfile {
    pub id: String,
    pub label: String,
    pub description: String,
    pub actor_scope: ActorScope,
    pub surface: ReadingSurface,
    pub emphasis: ReadingEmphasis,
    /// Initial arrangement for spatial surfaces. Matrix-like surfaces omit it.
    pub default_arrangement: Option<String>,
    /// True when the reading's meaning depends on that arrangement.
    pub arrangement_locked: bool,
}

/// Ordered reading registry. The first profile is the default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphReadingRegistry {
    pub schema: String,
    pub profiles: Vec<GraphReadingProfile>,
}

impl GraphReadingRegistry {
    pub fn resolve(&self, id: &str) -> Option<&GraphReadingProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn default_profile(&self) -> &GraphReadingProfile {
        self.profiles
            .first()
            .expect("Mere's graph-reading registry is never empty")
    }
}

/// Mere's renderer-neutral baseline readings.
pub fn default_graph_reading_registry() -> GraphReadingRegistry {
    GraphReadingRegistry {
        schema: GRAPH_READING_REGISTRY_SCHEMA.to_owned(),
        profiles: vec![
            profile(
                "graph",
                "Graph",
                "Every actor and typed relation in the selected authority.",
                ActorScope::All,
                ReadingSurface::Spatial,
                ReadingEmphasis::Identity,
                ReadingArrangement::Selectable("graph_layout:stack"),
            ),
            profile(
                "changes",
                "Changes",
                "The selected authority compared with its immediate predecessor.",
                ActorScope::AdjacentRevision,
                ReadingSurface::Spatial,
                ReadingEmphasis::Change,
                ReadingArrangement::Selectable("graph_layout:stack"),
            ),
            profile(
                "activity",
                "Activity",
                "Every actor positioned by its source time.",
                ActorScope::All,
                ReadingSurface::Spatial,
                ReadingEmphasis::Activity,
                ReadingArrangement::Locked("graph_layout:timeline"),
            ),
            profile(
                "neighbors",
                "Neighbors",
                "The selected actor and every actor one relation away.",
                ActorScope::FocusAndNeighbors,
                ReadingSurface::Spatial,
                ReadingEmphasis::FocusDistance,
                ReadingArrangement::Selectable("graph_layout:radial"),
            ),
            profile(
                "matrix",
                "Matrix",
                "An exact source-by-target lookup of direct relations.",
                ActorScope::All,
                ReadingSurface::RelationMatrix,
                ReadingEmphasis::Relation,
                ReadingArrangement::None,
            ),
        ],
    }
}

enum ReadingArrangement {
    Selectable(&'static str),
    Locked(&'static str),
    None,
}

fn profile(
    id: &str,
    label: &str,
    description: &str,
    actor_scope: ActorScope,
    surface: ReadingSurface,
    emphasis: ReadingEmphasis,
    arrangement: ReadingArrangement,
) -> GraphReadingProfile {
    let (default_arrangement, arrangement_locked) = match arrangement {
        ReadingArrangement::Selectable(id) => (Some(id.to_owned()), false),
        ReadingArrangement::Locked(id) => (Some(id.to_owned()), true),
        ReadingArrangement::None => (None, true),
    };
    GraphReadingProfile {
        id: id.to_owned(),
        label: label.to_owned(),
        description: description.to_owned(),
        actor_scope,
        surface,
        emphasis,
        default_arrangement,
        arrangement_locked,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn registry_separates_reading_scope_from_arrangement() {
        let registry = default_graph_reading_registry();
        let neighbors = registry.resolve("neighbors").expect("neighbors reading");
        assert_eq!(neighbors.actor_scope, ActorScope::FocusAndNeighbors);
        assert_eq!(neighbors.surface, ReadingSurface::Spatial);
        assert_eq!(
            neighbors.default_arrangement.as_deref(),
            Some("graph_layout:radial")
        );
        assert!(!neighbors.arrangement_locked);

        let matrix = registry.resolve("matrix").expect("matrix reading");
        assert_eq!(matrix.surface, ReadingSurface::RelationMatrix);
        assert!(matrix.default_arrangement.is_none());
    }

    #[test]
    fn reading_ids_are_unique_and_registry_is_serializable() {
        let registry = default_graph_reading_registry();
        let ids = registry
            .profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), registry.profiles.len());

        let encoded = serde_json::to_value(&registry).expect("reading registry serializes");
        assert_eq!(encoded["schema"], GRAPH_READING_REGISTRY_SCHEMA);
        assert_eq!(encoded["profiles"][1]["actor_scope"], "adjacent_revision");
        assert_eq!(encoded["profiles"][3]["emphasis"], "focus_distance");
    }
}
