/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tests for the facet filter engine (filter.rs).

use super::super::filter::facet_keys;
use super::super::filter::*;

fn text(s: &str) -> FacetScalar {
    FacetScalar::Text(s.to_string())
}

fn num(n: f64) -> FacetScalar {
    FacetScalar::Number(n)
}

fn proj(pairs: &[(&str, FacetValue)]) -> FacetProjection {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

// G03 — canonical facet keys are valid, non-namespaced extensions are not
#[test]
fn canonical_keys_are_valid_non_namespaced_extensions_are_not() {
    assert!(facet_keys::is_valid(facet_keys::TITLE));
    assert!(facet_keys::is_valid(facet_keys::LIFECYCLE));
    assert!(facet_keys::is_valid("myns:custom_facet")); // valid extension
    assert!(!facet_keys::is_valid("custom_facet")); // missing namespace
    assert!(!facet_keys::is_valid("ns:")); // empty name
    assert!(!facet_keys::is_valid(":name")); // empty namespace
}

// Spec §9: namespaced extension keys enforced
#[test]
fn invalid_extension_key_returns_error() {
    let pred = FacetPredicate {
        facet_key: "notnamespaced".to_string(),
        operator: FacetOperator::Eq,
        operand: FacetOperand::Scalar(text("foo")),
    };
    let p = proj(&[]);
    let result = pred.evaluate(&p);
    assert!(
        matches!(result, Err(FilterEvalError::InvalidExtensionKey { .. })),
        "non-namespaced extension key must return InvalidExtensionKey error"
    );
}

// Spec §9: operator semantics are type-safe
#[test]
fn type_mismatch_returns_error_not_panic() {
    // Range on a text scalar is a type mismatch
    let pred = FacetPredicate {
        facet_key: facet_keys::TITLE.to_string(),
        operator: FacetOperator::Range,
        operand: FacetOperand::Range {
            lo: num(0.0),
            hi: num(10.0),
        },
    };
    let p = proj(&[(facet_keys::TITLE, FacetValue::Scalar(text("hello")))]);
    let result = pred.evaluate(&p);
    assert!(
        matches!(result, Err(FilterEvalError::TypeMismatch { .. })),
        "Range on text facet must return TypeMismatch, not panic"
    );
}

// Spec §9: PMEST canonical facets are queryable
#[test]
fn eq_predicate_matches_scalar_facet() {
    let pred = FacetPredicate {
        facet_key: facet_keys::LIFECYCLE.to_string(),
        operator: FacetOperator::Eq,
        operand: FacetOperand::Scalar(text("Active")),
    };
    let p_match = proj(&[(facet_keys::LIFECYCLE, FacetValue::Scalar(text("Active")))]);
    let p_miss = proj(&[(facet_keys::LIFECYCLE, FacetValue::Scalar(text("Cold")))]);

    assert_eq!(pred.evaluate(&p_match).unwrap(), true);
    assert_eq!(pred.evaluate(&p_miss).unwrap(), false);
}

#[test]
fn contains_any_predicate_matches_collection_facet() {
    let pred = FacetPredicate {
        facet_key: facet_keys::EDGE_KINDS.to_string(),
        operator: FacetOperator::ContainsAny,
        operand: FacetOperand::Set(vec![text("Hyperlink"), text("UserGrouped")]),
    };
    let p = proj(&[(
        facet_keys::EDGE_KINDS,
        FacetValue::Collection(vec![text("Hyperlink"), text("TraversalDerived")]),
    )]);
    assert_eq!(pred.evaluate(&p).unwrap(), true);
}

#[test]
fn range_predicate_matches_numeric_facet() {
    let pred = FacetPredicate {
        facet_key: facet_keys::IN_DEGREE.to_string(),
        operator: FacetOperator::Range,
        operand: FacetOperand::Range {
            lo: num(1.0),
            hi: num(5.0),
        },
    };
    let p_in = proj(&[(facet_keys::IN_DEGREE, FacetValue::Scalar(num(3.0)))]);
    let p_out = proj(&[(facet_keys::IN_DEGREE, FacetValue::Scalar(num(0.0)))]);
    assert_eq!(pred.evaluate(&p_in).unwrap(), true);
    assert_eq!(pred.evaluate(&p_out).unwrap(), false);
}

#[test]
fn exists_predicate_checks_key_presence() {
    let pred = FacetPredicate {
        facet_key: facet_keys::MIME_HINT.to_string(),
        operator: FacetOperator::Exists,
        operand: FacetOperand::None,
    };
    let p_present = proj(&[(facet_keys::MIME_HINT, FacetValue::Scalar(text("text/html")))]);
    let p_absent: FacetProjection = std::collections::HashMap::new();

    assert_eq!(pred.evaluate(&p_present).unwrap(), true);
    assert_eq!(pred.evaluate(&p_absent).unwrap(), false);
}

#[test]
fn and_expr_requires_all_predicates() {
    let expr = FacetExpr::And(vec![
        FacetExpr::Predicate(FacetPredicate {
            facet_key: facet_keys::LIFECYCLE.to_string(),
            operator: FacetOperator::Eq,
            operand: FacetOperand::Scalar(text("Active")),
        }),
        FacetExpr::Predicate(FacetPredicate {
            facet_key: facet_keys::DOMAIN.to_string(),
            operator: FacetOperator::Eq,
            operand: FacetOperand::Scalar(text("example.com")),
        }),
    ]);
    let p_both = proj(&[
        (facet_keys::LIFECYCLE, FacetValue::Scalar(text("Active"))),
        (facet_keys::DOMAIN, FacetValue::Scalar(text("example.com"))),
    ]);
    let p_one = proj(&[
        (facet_keys::LIFECYCLE, FacetValue::Scalar(text("Active"))),
        (facet_keys::DOMAIN, FacetValue::Scalar(text("other.com"))),
    ]);
    assert_eq!(expr.evaluate(&p_both).unwrap(), true);
    assert_eq!(expr.evaluate(&p_one).unwrap(), false);
}

#[test]
fn not_expr_inverts_predicate() {
    let expr = FacetExpr::Not(Box::new(FacetExpr::Predicate(FacetPredicate {
        facet_key: facet_keys::LIFECYCLE.to_string(),
        operator: FacetOperator::Eq,
        operand: FacetOperand::Scalar(text("Cold")),
    })));
    let p_cold = proj(&[(facet_keys::LIFECYCLE, FacetValue::Scalar(text("Cold")))]);
    let p_active = proj(&[(facet_keys::LIFECYCLE, FacetValue::Scalar(text("Active")))]);
    assert_eq!(expr.evaluate(&p_cold).unwrap(), false);
    assert_eq!(expr.evaluate(&p_active).unwrap(), true);
}

// Omnibar token parser tests
#[test]
fn omnibar_facet_token_parses_eq() {
    let expr = parse_omnibar_facet_token("facet:lifecycle=Active").unwrap();
    let FacetExpr::Predicate(pred) = expr else {
        panic!("expected predicate expr");
    };
    assert_eq!(pred.facet_key, "lifecycle");
    assert_eq!(pred.operator, FacetOperator::Eq);
    assert_eq!(pred.operand, FacetOperand::Scalar(text("Active")));
}

#[test]
fn omnibar_facet_token_parses_not_eq() {
    let expr = parse_omnibar_facet_token("facet:!lifecycle=Cold").unwrap();
    let FacetExpr::Predicate(pred) = expr else {
        panic!("expected predicate expr");
    };
    assert_eq!(pred.operator, FacetOperator::NotEq);
    assert_eq!(pred.operand, FacetOperand::Scalar(text("Cold")));
}

#[test]
fn omnibar_facet_token_parses_exists() {
    let expr = parse_omnibar_facet_token("facet:mime_hint").unwrap();
    let FacetExpr::Predicate(pred) = expr else {
        panic!("expected predicate expr");
    };
    assert_eq!(pred.facet_key, "mime_hint");
    assert_eq!(pred.operator, FacetOperator::Exists);
}

#[test]
fn omnibar_facet_token_rejects_invalid_key() {
    // Non-namespaced extension key must be rejected
    assert!(parse_omnibar_facet_token("facet:custom_key=foo").is_none());
}

#[test]
fn omnibar_facet_token_accepts_namespaced_extension() {
    let expr = parse_omnibar_facet_token("facet:myns:custom=foo").unwrap();
    let FacetExpr::Predicate(pred) = expr else {
        panic!("expected predicate expr");
    };
    assert_eq!(pred.facet_key, "myns:custom");
    assert_eq!(pred.operator, FacetOperator::Eq);
}

#[test]
fn omnibar_token_without_prefix_returns_none() {
    assert!(parse_omnibar_facet_token("lifecycle=Active").is_none());
    assert!(parse_omnibar_facet_token("facet:").is_none());
}

#[test]
fn omnibar_udc_token_uses_collection_operator() {
    let expr = parse_omnibar_facet_token("facet:udc_classes=udc:51").unwrap();
    let FacetExpr::Predicate(pred) = expr else {
        panic!("expected predicate expr");
    };
    assert_eq!(pred.facet_key, facet_keys::UDC_CLASSES);
    assert_eq!(pred.operator, FacetOperator::ContainsAny);
    assert_eq!(pred.operand, FacetOperand::Set(vec![text("udc:51")]));
}

#[test]
fn udc_contains_any_supports_parent_prefix_match() {
    let expr = FacetExpr::Predicate(FacetPredicate {
        facet_key: facet_keys::UDC_CLASSES.to_string(),
        operator: FacetOperator::ContainsAny,
        operand: FacetOperand::Set(vec![text("udc:51")]),
    });
    let projection = proj(&[(
        facet_keys::UDC_CLASSES,
        FacetValue::Collection(vec![text("udc:519.6")]),
    )]);

    assert!(expr.evaluate(&projection).unwrap());
}
