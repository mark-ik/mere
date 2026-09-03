// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The solver registry behind [`sceno::Arrangement::Custom`].
//!
//! The eleven named families are enum variants and need no registration; this
//! catalog is exactly the extension point, so everything in it came from
//! outside. That is why there is no provenance field and no built-in
//! registration pass — both existed in the `arrangements` original because
//! built-ins and mods shared one catalog there, and here they do not.
//!
//! It is also why there is no dynamic-dispatch state shim. The original's
//! `DynLayout` erased `Layout::State` into `Box<dyn Any + Send>` so stateful
//! and stateless layouts could share a trait object. Every solver here is
//! closed-form and has no state to erase, so a plain object-safe trait does the
//! whole job.

use std::collections::HashMap;
use std::sync::Arc;

use sceno::{ScoreItem, Vec2};
use serde::{Deserialize, Serialize};

/// A registered solver's identifier, matched against
/// [`sceno::Arrangement::Custom::id`].
///
/// URN-style — `<namespace>:<family>[:<variant>]`, as in
/// `mod:acme:butterfly`. The id is the persistence key: changing one is a
/// breaking migration for every score that named it, while a solver's config
/// schema can evolve independently.
pub type ArrangementId = String;

/// A per-item disclosure a solver reads.
///
/// Declaring these is what makes a missing disclosure diagnosable. A solver
/// that needs ring indices and receives a score with no `axis` anywhere will
/// place everything on one ring and look like it merely produced an ugly
/// layout; the registry can say "this score disclosed nothing this solver
/// reads" before a single position is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Disclosure {
    /// [`ScoreItem::axis`] — a coordinate on the arrangement's axis.
    Axis,
    /// [`ScoreItem::embedding`] — 2-D coordinates a producer computed.
    Embedding,
    /// [`ScoreItem::weight`] — a scalar for proportional placement.
    Weight,
}

/// What a registered solver advertises about itself. Drives pickers,
/// recommendation and fallback logic, and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverCapability {
    pub id: ArrangementId,
    pub display_name: String,
    pub description: Option<String>,
    /// True when identical input yields identical output, floating-point noise
    /// aside. A solver that says `false` here cannot be used where a score is
    /// expected to replay.
    pub is_deterministic: bool,
    /// Disclosures the solver needs on every item to place meaningfully.
    pub requires: Vec<Disclosure>,
    /// Recommended maximum item count for acceptable performance. `None` is
    /// unbounded or unmeasured.
    pub recommended_max_items: Option<usize>,
    /// Free-form tags for filtering: `"spatial-memory"`, `"time-axis"`,
    /// `"hierarchical"`, `"organic"`.
    pub tags: Vec<String>,
}

impl SolverCapability {
    /// A minimal deterministic capability reading no disclosures.
    pub fn new(id: impl Into<ArrangementId>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            description: None,
            is_deterministic: true,
            requires: Vec::new(),
            recommended_max_items: None,
            tags: Vec::new(),
        }
    }
}

/// A placement solver reached through [`sceno::Arrangement::Custom`].
///
/// The contract is the same one the built-in families answer: items in
/// arrangement order in, one position per item in that same order out. A
/// solver never sees source truth, and never renders.
pub trait Solver: Send + Sync {
    fn capability(&self) -> SolverCapability;

    /// Place every item. `config` is the opaque value the score carried.
    ///
    /// Returning the wrong number of positions is a bug the caller refuses
    /// rather than works around, so return [`SolveError::Unplaceable`] when the
    /// config or the disclosures are not something this solver can work with.
    fn place(
        &self,
        config: &serde_json::Value,
        items: &[&ScoreItem],
    ) -> Result<Vec<Vec2>, SolveError>;
}

/// Why a custom arrangement could not be realized.
#[derive(Debug, Clone, PartialEq)]
pub enum SolveError {
    /// No solver is registered under this id.
    Unregistered(ArrangementId),
    /// A solver is registered, but the score discloses nothing it reads.
    MissingDisclosure {
        id: ArrangementId,
        required: Disclosure,
    },
    /// The solver declined: a config it cannot parse, or items it cannot place.
    Unplaceable { id: ArrangementId, reason: String },
    /// The solver returned a different number of positions than there were
    /// items. Refused rather than truncated or padded.
    CountMismatch {
        id: ArrangementId,
        items: usize,
        positions: usize,
    },
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unregistered(id) => write!(f, "no solver registered for arrangement {id:?}"),
            Self::MissingDisclosure { id, required } => write!(
                f,
                "solver {id:?} reads {required:?}, which no item in the score disclosed"
            ),
            Self::Unplaceable { id, reason } => write!(f, "solver {id:?} declined: {reason}"),
            Self::CountMismatch {
                id,
                items,
                positions,
            } => write!(
                f,
                "solver {id:?} returned {positions} positions for {items} items"
            ),
        }
    }
}

impl std::error::Error for SolveError {}

/// Why a solver could not be registered.
#[derive(Debug, Clone, PartialEq)]
pub enum RegisterError {
    /// The id was empty or all whitespace.
    InvalidId(ArrangementId),
    /// Something is already registered under this id. Unregister first if
    /// replacement is intended — silently replacing would change what every
    /// persisted score naming that id means.
    DuplicateId(ArrangementId),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "invalid arrangement id: {id:?}"),
            Self::DuplicateId(id) => write!(f, "arrangement id already registered: {id:?}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Catalog of solvers keyed by [`ArrangementId`].
#[derive(Default, Clone)]
pub struct SolverRegistry {
    solvers: HashMap<ArrangementId, Arc<dyn Solver>>,
}

impl std::fmt::Debug for SolverRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SolverRegistry")
            .field("ids", &self.solvers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl SolverRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a solver.
    pub fn register(&mut self, solver: Arc<dyn Solver>) -> Result<(), RegisterError> {
        let id = solver.capability().id;
        if id.trim().is_empty() {
            return Err(RegisterError::InvalidId(id));
        }
        if self.solvers.contains_key(&id) {
            return Err(RegisterError::DuplicateId(id));
        }
        self.solvers.insert(id, solver);
        Ok(())
    }

    /// Remove a solver by id. True if one was present.
    pub fn unregister(&mut self, id: &str) -> bool {
        self.solvers.remove(id).is_some()
    }

    pub fn resolve(&self, id: &str) -> Option<Arc<dyn Solver>> {
        self.solvers.get(id).cloned()
    }

    pub fn capabilities(&self) -> Vec<SolverCapability> {
        self.solvers.values().map(|s| s.capability()).collect()
    }

    /// Capabilities carrying an exact tag.
    pub fn filter_by_tag(&self, tag: &str) -> Vec<SolverCapability> {
        self.solvers
            .values()
            .map(|s| s.capability())
            .filter(|cap| cap.tags.iter().any(|known| known == tag))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.solvers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.solvers.is_empty()
    }
}

/// Realize a score, resolving [`sceno::Arrangement::Custom`] through `registry`.
///
/// The eleven named families never touch the registry; they are solved by
/// `scenomise` exactly as [`crate::solve`] would. Only a custom arrangement
/// consults it, and a failure there is returned rather than absorbed: a score
/// naming a solver nobody registered has not been laid out, and saying so is the
/// difference between a diagnosable error and a canvas of items at the origin.
pub fn solve(score: &sceno::Score, registry: &SolverRegistry) -> Result<sceno::Scene, SolveError> {
    let sceno::Arrangement::Custom { id, config } = &score.arrangement else {
        return Ok(crate::solve(score));
    };

    let solver = registry
        .resolve(id)
        .ok_or_else(|| SolveError::Unregistered(id.clone()))?;
    let capability = solver.capability();

    // Checked before solving, so a score that disclosed nothing the solver reads
    // fails by name instead of producing a plausible-looking wrong layout.
    for required in &capability.requires {
        let disclosed = score.items.iter().any(|item| match required {
            Disclosure::Axis => item.axis.is_some(),
            Disclosure::Embedding => item.embedding.is_some(),
            Disclosure::Weight => item.weight.is_some(),
        });
        if !disclosed && !score.items.is_empty() {
            return Err(SolveError::MissingDisclosure {
                id: id.clone(),
                required: *required,
            });
        }
    }

    let mut failure = None;
    let scene = crate::solve_with(score, |items| match solver.place(config, items) {
        Ok(positions) if positions.len() == items.len() => Some(positions),
        Ok(positions) => {
            failure = Some(SolveError::CountMismatch {
                id: id.clone(),
                items: items.len(),
                positions: positions.len(),
            });
            None
        }
        Err(error) => {
            failure = Some(error);
            None
        }
    });

    match failure {
        Some(error) => Err(error),
        None => Ok(scene),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{
        Arrangement, AxisValue, Footprint, Placement, Representation, Score, Size2, SourceRef,
    };

    struct Line {
        capability: SolverCapability,
    }

    impl Line {
        fn new(requires: Vec<Disclosure>) -> Arc<dyn Solver> {
            Arc::new(Self {
                capability: SolverCapability {
                    requires,
                    ..SolverCapability::new("mod:test:line", "Test line")
                },
            })
        }
    }

    impl Solver for Line {
        fn capability(&self) -> SolverCapability {
            self.capability.clone()
        }

        fn place(
            &self,
            config: &serde_json::Value,
            items: &[&ScoreItem],
        ) -> Result<Vec<Vec2>, SolveError> {
            let pitch = config.get("pitch").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32;
            Ok((0..items.len())
                .map(|index| Vec2::new(index as f32 * pitch, 0.0))
                .collect())
        }
    }

    /// Returns one position too few, whatever it is asked for.
    struct Miscounts;

    impl Solver for Miscounts {
        fn capability(&self) -> SolverCapability {
            SolverCapability::new("mod:test:miscounts", "Miscounting solver")
        }

        fn place(
            &self,
            _config: &serde_json::Value,
            items: &[&ScoreItem],
        ) -> Result<Vec<Vec2>, SolveError> {
            Ok(vec![Vec2::ZERO; items.len().saturating_sub(1)])
        }
    }

    fn card(id: u32) -> ScoreItem {
        ScoreItem {
            source: SourceRef::new("fixture", id.to_string()),
            ordinal: id,
            footprint: Footprint::Rect {
                size: Size2::new(10.0, 10.0),
            },
            representation: Representation::Card,
            placement: Placement::Ordinal,
            layer: 0,
            visible: true,
            axis: None,
            embedding: None,
            weight: None,
        }
    }

    fn custom_score(config: serde_json::Value) -> Score {
        let mut score = Score::new(Arrangement::Custom {
            id: "mod:test:line".to_string(),
            config,
        });
        for id in 0..3 {
            score.items.push(card(id));
        }
        score
    }

    #[test]
    fn a_registered_solver_places_the_score() {
        let mut registry = SolverRegistry::new();
        registry.register(Line::new(Vec::new())).unwrap();

        let scene = solve(
            &custom_score(serde_json::json!({ "pitch": 25.0 })),
            &registry,
        )
        .expect("registered solver resolves");
        assert_eq!(scene.items.len(), 3);
        assert_eq!(scene.items[2].transform.translate, Vec2::new(50.0, 0.0));
    }

    #[test]
    fn the_config_reaches_the_solver() {
        let mut registry = SolverRegistry::new();
        registry.register(Line::new(Vec::new())).unwrap();
        let scene = solve(
            &custom_score(serde_json::json!({ "pitch": 1.0 })),
            &registry,
        )
        .unwrap();
        assert_eq!(scene.items[2].transform.translate, Vec2::new(2.0, 0.0));
    }

    #[test]
    fn an_unregistered_id_is_an_error_not_an_empty_canvas() {
        let registry = SolverRegistry::new();
        assert_eq!(
            solve(&custom_score(serde_json::json!({})), &registry),
            Err(SolveError::Unregistered("mod:test:line".to_string()))
        );
    }

    #[test]
    fn a_score_disclosing_nothing_the_solver_reads_fails_by_name() {
        // The failure this check exists for: without it the solver runs, places
        // everything as though every item disclosed nothing, and the result
        // looks like a bad layout rather than a missing input.
        let mut registry = SolverRegistry::new();
        registry
            .register(Line::new(vec![Disclosure::Axis]))
            .unwrap();
        assert_eq!(
            solve(&custom_score(serde_json::json!({})), &registry),
            Err(SolveError::MissingDisclosure {
                id: "mod:test:line".to_string(),
                required: Disclosure::Axis,
            })
        );
    }

    #[test]
    fn a_disclosed_score_satisfies_the_requirement() {
        let mut registry = SolverRegistry::new();
        registry
            .register(Line::new(vec![Disclosure::Axis]))
            .unwrap();
        let mut score = custom_score(serde_json::json!({}));
        score.items[1].axis = Some(AxisValue::Numeric(1.0));
        assert!(solve(&score, &registry).is_ok());
    }

    #[test]
    fn a_miscounting_solver_is_refused_rather_than_indexed_into() {
        let mut registry = SolverRegistry::new();
        registry.register(Arc::new(Miscounts)).unwrap();
        let mut score = custom_score(serde_json::json!({}));
        score.arrangement = Arrangement::Custom {
            id: "mod:test:miscounts".to_string(),
            config: serde_json::json!({}),
        };
        assert_eq!(
            solve(&score, &registry),
            Err(SolveError::CountMismatch {
                id: "mod:test:miscounts".to_string(),
                items: 3,
                positions: 2,
            })
        );
    }

    #[test]
    fn a_named_family_never_consults_the_registry() {
        // An empty registry must not stop a Grid score from solving.
        let mut score = Score::new(Arrangement::Grid(sceno::Grid::default()));
        score.items.push(card(0));
        let scene = solve(&score, &SolverRegistry::new()).expect("built-ins need no registry");
        assert_eq!(scene.items.len(), 1);
    }

    #[test]
    fn ids_are_unique_and_replacement_is_explicit() {
        let mut registry = SolverRegistry::new();
        registry.register(Line::new(Vec::new())).unwrap();
        assert_eq!(
            registry.register(Line::new(Vec::new())),
            Err(RegisterError::DuplicateId("mod:test:line".to_string()))
        );
        assert!(registry.unregister("mod:test:line"));
        assert!(registry.register(Line::new(Vec::new())).is_ok());
    }

    #[test]
    fn an_empty_id_is_rejected() {
        struct Nameless;
        impl Solver for Nameless {
            fn capability(&self) -> SolverCapability {
                SolverCapability::new("   ", "Nameless")
            }
            fn place(
                &self,
                _config: &serde_json::Value,
                items: &[&ScoreItem],
            ) -> Result<Vec<Vec2>, SolveError> {
                Ok(vec![Vec2::ZERO; items.len()])
            }
        }
        let mut registry = SolverRegistry::new();
        assert!(matches!(
            registry.register(Arc::new(Nameless)),
            Err(RegisterError::InvalidId(_))
        ));
    }

    #[test]
    fn capabilities_filter_by_tag() {
        struct Tagged;
        impl Solver for Tagged {
            fn capability(&self) -> SolverCapability {
                SolverCapability {
                    tags: vec!["time-axis".to_string()],
                    ..SolverCapability::new("mod:test:tagged", "Tagged")
                }
            }
            fn place(
                &self,
                _config: &serde_json::Value,
                items: &[&ScoreItem],
            ) -> Result<Vec<Vec2>, SolveError> {
                Ok(vec![Vec2::ZERO; items.len()])
            }
        }
        let mut registry = SolverRegistry::new();
        registry.register(Line::new(Vec::new())).unwrap();
        registry.register(Arc::new(Tagged)).unwrap();
        assert_eq!(registry.capabilities().len(), 2);
        assert_eq!(registry.filter_by_tag("time-axis").len(), 1);
    }
}
