// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Faceted filter expression engine.
//!
//! Implements the canonical filter query model from:
//! `repos/graphshell/design_docs/graphshell_docs/implementation_strategy/graph/faceted_filter_surface_spec.md §4`
//!
//! ## Authority contract (spec §2)
//!
//! - Filter truth is derived from graph-owned node/edge metadata.
//! - Evaluation runs through reducer-owned intent paths.
//! - Filter output is a **projection** over graph truth — it never mutates node/edge identity.
//!
//! ## Namespaced extension keys (spec §3)
//!
//! Canonical PMEST facet keys (see `FacetKey` constants) are non-namespaced.
//! Extension facets **must** use `"namespace:name"` format. Non-namespaced
//! extension keys are invalid and must emit a `Warn` diagnostic at evaluation
//! time.

use std::collections::HashMap;

use super::Graph;
use super::facet_projection::facet_projection_for_node;

// ---------------------------------------------------------------------------
// Facet value — the runtime type that a facet key resolves to
// ---------------------------------------------------------------------------

/// Runtime value carried by a facet key.
///
/// Used both as the output of [`FacetProjection`] and as the operand type
/// matched by [`FacetOperator`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FacetValue {
    /// Single scalar (string, number, or boolean encoded as string/f64/bool).
    Scalar(FacetScalar),
    /// Multi-valued collection (tags, edge kinds, frame memberships, …).
    Collection(Vec<FacetScalar>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FacetScalar {
    Text(String),
    Number(f64),
    Bool(bool),
}

// ---------------------------------------------------------------------------
// Canonical PMEST facet key constants (spec §3)
// ---------------------------------------------------------------------------

pub mod facet_keys {
    // Personality
    pub const ADDRESS_KIND: &str = "address_kind";
    pub const DOMAIN: &str = "domain";
    pub const TITLE: &str = "title";
    pub const ADDRESS: &str = "address";

    // Matter
    pub const MIME_HINT: &str = "mime_hint";
    pub const VIEWER_BINDING: &str = "viewer_binding";
    pub const CONTENT_LENGTH: &str = "content_length";

    // Energy
    pub const EDGE_KINDS: &str = "edge_kinds";
    pub const TRAVERSAL_COUNT: &str = "traversal_count";
    pub const IN_DEGREE: &str = "in_degree";
    pub const OUT_DEGREE: &str = "out_degree";

    // Space
    pub const FRAME_MEMBERSHIPS: &str = "frame_memberships";
    pub const FRAME_AFFINITY_REGION: &str = "frame_affinity_region";
    pub const UDC_CLASSES: &str = "udc_classes";
    pub const SPATIAL_CLUSTER: &str = "spatial_cluster";

    // Time
    pub const CREATED_AT: &str = "created_at";
    pub const LAST_TRAVERSAL: &str = "last_traversal";
    pub const LIFECYCLE: &str = "lifecycle";

    /// Returns true for canonical (non-namespaced) PMEST keys.
    pub fn is_canonical(key: &str) -> bool {
        matches!(
            key,
            ADDRESS_KIND
                | DOMAIN
                | TITLE
                | ADDRESS
                | MIME_HINT
                | VIEWER_BINDING
                | CONTENT_LENGTH
                | EDGE_KINDS
                | TRAVERSAL_COUNT
                | IN_DEGREE
                | OUT_DEGREE
                | FRAME_MEMBERSHIPS
                | FRAME_AFFINITY_REGION
                | UDC_CLASSES
                | SPATIAL_CLUSTER
                | CREATED_AT
                | LAST_TRAVERSAL
                | LIFECYCLE
        )
    }

    /// Returns true when `key` is a valid namespaced extension key (`"ns:name"`).
    pub fn is_valid_extension(key: &str) -> bool {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
    }

    /// Returns true when a facet key is valid (canonical or correctly namespaced).
    pub fn is_valid(key: &str) -> bool {
        is_canonical(key) || is_valid_extension(key)
    }
}

// ---------------------------------------------------------------------------
// Filter query model (spec §4)
// ---------------------------------------------------------------------------

/// Composable predicate expression over node facets (spec §4.1).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FacetExpr {
    Predicate(FacetPredicate),
    And(Vec<FacetExpr>),
    Or(Vec<FacetExpr>),
    Not(Box<FacetExpr>),
}

/// Single predicate over one facet key (spec §4.2).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FacetPredicate {
    /// PMEST facet key or `"namespace:name"` extension key.
    pub facet_key: String,
    pub operator: FacetOperator,
    pub operand: FacetOperand,
}

/// Filter operators (spec §4.2).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FacetOperator {
    Eq,
    NotEq,
    In,
    ContainsAny,
    ContainsAll,
    Range,
    Exists,
    NotExists,
}

/// Operand carried by a predicate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FacetOperand {
    Scalar(FacetScalar),
    Set(Vec<FacetScalar>),
    Range { lo: FacetScalar, hi: FacetScalar },
    None,
}

// ---------------------------------------------------------------------------
// Filter result model (spec §6)
// ---------------------------------------------------------------------------

pub type FacetProjection = HashMap<String, FacetValue>;

/// Output of filter evaluation against a graph scope.
#[derive(Debug, Clone)]
pub struct FilterResult {
    /// Node keys satisfying the `FacetExpr`.
    pub matched_nodes: Vec<super::NodeKey>,
    /// Node keys excluded by current filters.
    pub filtered_out_nodes: Vec<super::NodeKey>,
    /// Per-facet bucket counts for visible scope.
    pub facet_counts: HashMap<String, usize>,
}

// ---------------------------------------------------------------------------
// Evaluation (spec §5.1 — runs through reducer-owned intent paths)
// ---------------------------------------------------------------------------

/// Error produced when a predicate cannot be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterEvalError {
    /// Operator applied to an incompatible facet value type.
    TypeMismatch {
        facet_key: String,
        operator: FacetOperator,
        actual_value_type: &'static str,
    },
    /// Extension key used without the `"namespace:name"` format.
    InvalidExtensionKey { key: String },
    /// Facet key not found in projection (used with `Exists`/`NotExists`).
    KeyAbsent { key: String },
}

/// Aggregated filter evaluation result plus non-fatal warnings encountered
/// while evaluating individual node projections.
#[derive(Debug, Clone)]
pub struct FilterEvaluationSummary {
    pub result: FilterResult,
    pub warnings: Vec<FilterEvalError>,
}

impl FacetExpr {
    /// Evaluate this expression against a node's facet projection.
    ///
    /// Per spec §4.2 operator invariants:
    /// - Operator/type mismatch resolves to **no match** (returns `false`) and
    ///   the caller should emit a `Warn` diagnostic via the diagnostics channel
    ///   `ux:facet_filter_type_mismatch`.
    /// - `Range` on non-ordered types is invalid (resolves to no match).
    /// - `ContainsAny`/`ContainsAll` require collection-valued facets.
    ///
    /// Returns `Ok(true/false)` for normal match/no-match, or
    /// `Err(FilterEvalError)` for structural problems (invalid key, type mismatch)
    /// that the caller must log as a diagnostic.
    pub fn evaluate(&self, projection: &FacetProjection) -> Result<bool, FilterEvalError> {
        match self {
            FacetExpr::Predicate(pred) => pred.evaluate(projection),
            FacetExpr::And(exprs) => {
                for e in exprs {
                    if !e.evaluate(projection)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            FacetExpr::Or(exprs) => {
                for e in exprs {
                    if e.evaluate(projection)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            FacetExpr::Not(inner) => Ok(!inner.evaluate(projection)?),
        }
    }
}

impl FacetPredicate {
    pub fn evaluate(&self, projection: &FacetProjection) -> Result<bool, FilterEvalError> {
        // Validate key before evaluation
        if !facet_keys::is_valid(&self.facet_key) {
            return Err(FilterEvalError::InvalidExtensionKey {
                key: self.facet_key.clone(),
            });
        }

        match self.operator {
            FacetOperator::Exists => return Ok(projection.contains_key(&self.facet_key)),
            FacetOperator::NotExists => return Ok(!projection.contains_key(&self.facet_key)),
            _ => {}
        }

        let Some(value) = projection.get(&self.facet_key) else {
            // Key absent = no match for all other operators
            return Ok(false);
        };

        match (&self.operator, value, &self.operand) {
            (FacetOperator::Eq, FacetValue::Scalar(scalar), FacetOperand::Scalar(operand)) => {
                Ok(scalar == operand)
            }
            (FacetOperator::NotEq, FacetValue::Scalar(scalar), FacetOperand::Scalar(operand)) => {
                Ok(scalar != operand)
            }
            (FacetOperator::In, FacetValue::Scalar(scalar), FacetOperand::Set(set)) => {
                Ok(set.contains(scalar))
            }
            (FacetOperator::ContainsAny, FacetValue::Collection(coll), FacetOperand::Set(set)) => {
                Ok(set.iter().any(|operand| {
                    coll.iter()
                        .any(|value| collection_value_matches(&self.facet_key, value, operand))
                }))
            }
            (FacetOperator::ContainsAll, FacetValue::Collection(coll), FacetOperand::Set(set)) => {
                Ok(set.iter().all(|operand| {
                    coll.iter()
                        .any(|value| collection_value_matches(&self.facet_key, value, operand))
                }))
            }
            (
                FacetOperator::Range,
                FacetValue::Scalar(FacetScalar::Number(n)),
                FacetOperand::Range {
                    lo: FacetScalar::Number(lo),
                    hi: FacetScalar::Number(hi),
                },
            ) => Ok(n >= lo && n <= hi),
            // Type mismatches: return Err so caller can emit Warn diagnostic
            (op, _, _) => Err(FilterEvalError::TypeMismatch {
                facet_key: self.facet_key.clone(),
                operator: op.clone(),
                actual_value_type: value_type_name(value),
            }),
        }
    }
}

impl FacetExpr {
    pub fn display_label(&self) -> String {
        match self {
            FacetExpr::Predicate(predicate) => predicate.display_label(),
            FacetExpr::And(exprs) => exprs
                .iter()
                .map(FacetExpr::display_label)
                .collect::<Vec<_>>()
                .join(" AND "),
            FacetExpr::Or(exprs) => exprs
                .iter()
                .map(FacetExpr::display_label)
                .collect::<Vec<_>>()
                .join(" OR "),
            FacetExpr::Not(inner) => format!("NOT ({})", inner.display_label()),
        }
    }
}

impl FacetPredicate {
    pub fn display_label(&self) -> String {
        match (&self.operator, &self.operand) {
            (FacetOperator::Exists, _) => self.facet_key.clone(),
            (FacetOperator::NotExists, _) => format!("!{}", self.facet_key),
            (FacetOperator::Eq, FacetOperand::Scalar(value)) => {
                format!("{}={}", self.facet_key, facet_scalar_label(value))
            }
            (FacetOperator::NotEq, FacetOperand::Scalar(value)) => {
                format!("{}!={}", self.facet_key, facet_scalar_label(value))
            }
            (FacetOperator::In, FacetOperand::Set(values)) => {
                format!("{} in {}", self.facet_key, facet_set_label(values))
            }
            (FacetOperator::ContainsAny, FacetOperand::Set(values)) => {
                format!("{} has any {}", self.facet_key, facet_set_label(values))
            }
            (FacetOperator::ContainsAll, FacetOperand::Set(values)) => {
                format!("{} has all {}", self.facet_key, facet_set_label(values))
            }
            (FacetOperator::Range, FacetOperand::Range { lo, hi }) => format!(
                "{} in [{}..{}]",
                self.facet_key,
                facet_scalar_label(lo),
                facet_scalar_label(hi)
            ),
            _ => self.facet_key.clone(),
        }
    }
}

fn value_type_name(value: &FacetValue) -> &'static str {
    match value {
        FacetValue::Scalar(FacetScalar::Text(_)) => "text",
        FacetValue::Scalar(FacetScalar::Number(_)) => "number",
        FacetValue::Scalar(FacetScalar::Bool(_)) => "bool",
        FacetValue::Collection(_) => "collection",
    }
}

fn facet_scalar_label(value: &FacetScalar) -> String {
    match value {
        FacetScalar::Text(text) => text.clone(),
        FacetScalar::Number(number) => number.to_string(),
        FacetScalar::Bool(flag) => flag.to_string(),
    }
}

fn facet_set_label(values: &[FacetScalar]) -> String {
    values
        .iter()
        .map(facet_scalar_label)
        .collect::<Vec<_>>()
        .join("|")
}

fn is_collection_facet_key(key: &str) -> bool {
    matches!(
        key,
        facet_keys::EDGE_KINDS | facet_keys::FRAME_MEMBERSHIPS | facet_keys::UDC_CLASSES
    )
}

fn collection_value_matches(facet_key: &str, value: &FacetScalar, operand: &FacetScalar) -> bool {
    match (facet_key, value, operand) {
        (facet_keys::UDC_CLASSES, FacetScalar::Text(actual), FacetScalar::Text(expected)) => {
            udc_operand_matches(actual, expected)
        }
        _ => value == operand,
    }
}

fn udc_operand_matches(actual: &str, expected: &str) -> bool {
    let actual = actual.trim().to_ascii_lowercase();
    let expected = expected.trim().to_ascii_lowercase();
    actual == expected || actual.starts_with(&expected)
}

pub fn evaluate_filter_result(graph: &Graph, expr: &FacetExpr) -> FilterEvaluationSummary {
    let mut matched_nodes = Vec::new();
    let mut filtered_out_nodes = Vec::new();
    let mut facet_counts = HashMap::new();
    let mut warnings = Vec::new();

    for (key, _) in graph.nodes() {
        let Some(projection) = facet_projection_for_node(graph, key) else {
            filtered_out_nodes.push(key);
            continue;
        };

        match expr.evaluate(&projection) {
            Ok(true) => {
                matched_nodes.push(key);
                for facet_key in projection.keys() {
                    *facet_counts.entry(facet_key.clone()).or_insert(0) += 1;
                }
            }
            Ok(false) => filtered_out_nodes.push(key),
            Err(error) => {
                warnings.push(error);
                filtered_out_nodes.push(key);
            }
        }
    }

    FilterEvaluationSummary {
        result: FilterResult {
            matched_nodes,
            filtered_out_nodes,
            facet_counts,
        },
        warnings,
    }
}

// ---------------------------------------------------------------------------
// Omnibar `facet:` token parser (spec §7.1)
// ---------------------------------------------------------------------------

/// Parse a minimal `facet:key=value` or `facet:key` (Exists check) token
/// entered via the omnibar.
///
/// Returns `None` with no side effects on invalid input — the caller is
/// responsible for emitting `ux:facet_filter_invalid_query` (Warn).
///
/// Supported syntax:
/// - `facet:key=value`  → `Eq` or `ContainsAny` depending on facet kind
/// - `facet:!key=value` → `NotEq` or negated `ContainsAny` depending on facet kind
/// - `facet:key`        → `Exists` predicate
pub fn parse_omnibar_facet_token(token: &str) -> Option<FacetExpr> {
    let body = token.strip_prefix("facet:")?;
    if body.is_empty() {
        return None;
    }

    // `facet:!key=value` → NotEq / Not(ContainsAny)
    if let Some(rest) = body.strip_prefix('!') {
        if let Some((key, value)) = rest.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if !facet_keys::is_valid(&key) || key.is_empty() || value.is_empty() {
                return None;
            }
            let collection_facet = is_collection_facet_key(&key);
            let predicate = FacetPredicate {
                facet_key: key,
                operator: if collection_facet {
                    FacetOperator::ContainsAny
                } else {
                    FacetOperator::NotEq
                },
                operand: if collection_facet {
                    FacetOperand::Set(vec![FacetScalar::Text(value)])
                } else {
                    FacetOperand::Scalar(FacetScalar::Text(value))
                },
            };
            return if collection_facet {
                Some(FacetExpr::Not(Box::new(FacetExpr::Predicate(predicate))))
            } else {
                Some(FacetExpr::Predicate(predicate))
            };
        }
        return None;
    }

    // `facet:key=value` → Eq / ContainsAny
    if let Some((key, value)) = body.split_once('=') {
        let key = key.trim().to_string();
        let value = value.trim().to_string();
        if !facet_keys::is_valid(&key) || key.is_empty() || value.is_empty() {
            return None;
        }
        let collection_facet = is_collection_facet_key(&key);
        return Some(FacetExpr::Predicate(FacetPredicate {
            facet_key: key,
            operator: if collection_facet {
                FacetOperator::ContainsAny
            } else {
                FacetOperator::Eq
            },
            operand: if collection_facet {
                FacetOperand::Set(vec![FacetScalar::Text(value)])
            } else {
                FacetOperand::Scalar(FacetScalar::Text(value))
            },
        }));
    }

    // `facet:key` → Exists
    let key = body.trim().to_string();
    if !facet_keys::is_valid(&key) || key.is_empty() {
        return None;
    }
    Some(FacetExpr::Predicate(FacetPredicate {
        facet_key: key,
        operator: FacetOperator::Exists,
        operand: FacetOperand::None,
    }))
}
