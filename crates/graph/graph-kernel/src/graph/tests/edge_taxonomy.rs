// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Tests for edge taxonomy (RelationKind tag round-trips, predicate IRI mapping).

use super::super::edge_data::{predicate_iri, sub_kind_from_iri};
use super::super::edge_taxonomy::*;
use strum::IntoEnumIterator;

#[test]
fn predicate_iri_round_trips_for_every_semantic_sub_kind() {
    for sk in SemanticSubKind::iter() {
        let iri = predicate_iri(sk);
        assert_eq!(sub_kind_from_iri(iri), Some(sk), "iri {iri} for {sk:?}");
    }
    assert!(sub_kind_from_iri("https://example.com/unknown").is_none());
}

fn all_kinds() -> Vec<RelationKind> {
    // Driven by `EnumIter`, so a new sub-kind is covered automatically with
    // no parallel list to maintain. `Traversal` is the one family with no
    // sub-kind.
    let mut out = Vec::new();
    out.extend(SemanticSubKind::iter().map(RelationKind::Semantic));
    out.push(RelationKind::Traversal);
    out.extend(ContainmentSubKind::iter().map(RelationKind::Containment));
    out.extend(ArrangementSubKind::iter().map(RelationKind::Arrangement));
    out.extend(ImportedSubKind::iter().map(RelationKind::Imported));
    out.extend(ProvenanceSubKind::iter().map(RelationKind::Provenance));
    out
}

#[test]
fn tag_round_trips_for_every_relation_kind() {
    for kind in all_kinds() {
        let tag = kind.tag();
        let decoded = RelationKind::from_tag(tag);
        assert_eq!(decoded, Some(kind), "tag {tag:#x} for {kind:?}");
    }
}

#[test]
fn tag_is_unique_per_kind() {
    let kinds = all_kinds();
    let tags: std::collections::HashSet<u32> = kinds.iter().map(|k| k.tag()).collect();
    assert_eq!(tags.len(), kinds.len(), "every kind must have a unique tag");
}

#[test]
fn from_tag_rejects_unknown_family() {
    // Family byte 0xff (255) — no family at that ordinal.
    assert!(RelationKind::from_tag(0xff_00_00_00).is_none());
}

#[test]
fn from_tag_rejects_unknown_sub_kind() {
    // Semantic family (0), sub ordinal 99 — out of range.
    assert!(RelationKind::from_tag(99).is_none());
}
