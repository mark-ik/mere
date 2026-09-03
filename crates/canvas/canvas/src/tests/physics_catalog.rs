// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The physics catalog: every id builds, every label is plain, a switch does
//! not move a body, and the choice survives a graph reconcile.

use super::*;
use crate::physics_catalog::{
    CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS, CANVAS_PHYSICS_OVERLAYS,
    CANVAS_PHYSICS_PROFILES, PhysicsKindSource, PhysicsLaw, PhysicsOverlay,
};

/// A label in the plain register: a short capitalised phrase, no ids or
/// technical punctuation.
fn is_plain(label: &str) -> bool {
    !label.is_empty()
        && label.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && label.len() <= 16
        && !label.contains(['.', '_', '-', '(', ')'])
}

fn positions(canvas: &Canvas) -> Vec<(NodeKey, Point2D<f32>)> {
    let mut all: Vec<_> = canvas
        .graph()
        .nodes()
        .map(|(key, _)| (key, canvas.view.position_of(key).unwrap_or_default()))
        .collect();
    all.sort_by_key(|(key, _)| key.index());
    all
}

#[test]
fn every_law_id_round_trips_and_its_label_is_plain() {
    assert_eq!(CANVAS_PHYSICS_LAWS.len(), PhysicsLaw::ALL.len());
    for (id, label) in CANVAS_PHYSICS_LAWS {
        let law = PhysicsLaw::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(law.id(), *id);
        assert_eq!(law.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert_eq!(CANVAS_PHYSICS_OVERLAYS.len(), PhysicsOverlay::ALL.len());
    for (id, label) in CANVAS_PHYSICS_OVERLAYS {
        let overlay = PhysicsOverlay::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(overlay.id(), *id);
        assert_eq!(overlay.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert_eq!(
        CANVAS_PHYSICS_KIND_SOURCES.len(),
        PhysicsKindSource::ALL.len()
    );
    for (id, label) in CANVAS_PHYSICS_KIND_SOURCES {
        let source = PhysicsKindSource::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(source.id(), *id);
        assert_eq!(source.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert!(PhysicsLaw::parse("spring").is_none());
    assert!(PhysicsOverlay::parse("").is_none());
}

#[test]
fn every_law_and_overlay_builds_on_the_sample_graph_without_moving_a_body() {
    let mut canvas = Canvas::with_sample_graph();
    canvas.set_physics_paused(true);
    let before = positions(&canvas);
    for law in PhysicsLaw::ALL {
        canvas.set_physics_law(law);
        assert_eq!(canvas.physics_law(), law);
        let expected_min = if law == PhysicsLaw::Still { 0 } else { 1 };
        assert!(
            canvas.law_force_count() >= expected_min,
            "{} builds its forces",
            law.id()
        );
        assert_eq!(
            positions(&canvas),
            before,
            "{} switch moved a body",
            law.id()
        );
    }
    for overlay in PhysicsOverlay::ALL {
        assert!(
            canvas.toggle_physics_overlay(overlay),
            "{} toggles on",
            overlay.id()
        );
        assert_eq!(
            positions(&canvas),
            before,
            "{} switch moved a body",
            overlay.id()
        );
    }
    assert_eq!(canvas.physics_overlays().len(), PhysicsOverlay::ALL.len());
    // Still + every overlay: exactly one force per overlay.
    canvas.set_physics_law(PhysicsLaw::Still);
    assert_eq!(canvas.law_force_count(), PhysicsOverlay::ALL.len());
    for overlay in PhysicsOverlay::ALL {
        assert!(
            !canvas.toggle_physics_overlay(overlay),
            "{} toggles off",
            overlay.id()
        );
    }
    assert_eq!(canvas.law_force_count(), 0);
    for source in PhysicsKindSource::ALL {
        canvas.set_physics_law(PhysicsLaw::Kinds);
        canvas.set_physics_kind_source(source);
        assert_eq!(canvas.physics_kind_source(), source);
        assert!(
            canvas.law_force_count() >= 2,
            "kinds by {} builds",
            source.id()
        );
    }
}

#[test]
fn every_profile_applies_and_names_itself_back() {
    let mut canvas = Canvas::with_sample_graph();
    assert!(
        !canvas.apply_physics_profile("plasma"),
        "an unknown profile is refused"
    );
    for profile in CANVAS_PHYSICS_PROFILES {
        assert!(is_plain(profile.label), "{} is plain", profile.label);
        assert!(
            canvas.apply_physics_profile(profile.id),
            "{} applies",
            profile.id
        );
        assert_eq!(canvas.physics_law(), profile.law);
        assert_eq!(canvas.physics_overlays(), profile.overlays);
        assert_eq!(
            canvas.physics_profile_id(),
            Some(profile.id),
            "{} names itself back",
            profile.id
        );
    }
    // The donor's ten come first, under their own names.
    let donor: Vec<&str> = CANVAS_PHYSICS_PROFILES
        .iter()
        .take(10)
        .map(|p| p.id)
        .collect();
    assert_eq!(
        donor,
        [
            "liquid",
            "gas",
            "solid",
            "archipelago",
            "constellation",
            "crystal",
            "tide",
            "sediment",
            "magnet",
            "void"
        ]
    );
    // And no two profiles share a (law, overlays) pair, or the picker could not name the live one.
    for (i, a) in CANVAS_PHYSICS_PROFILES.iter().enumerate() {
        for b in &CANVAS_PHYSICS_PROFILES[i + 1..] {
            assert!(
                !(a.law == b.law && a.overlays == b.overlays),
                "{} and {} coincide",
                a.id,
                b.id
            );
        }
    }
    // Every law is one pick away: bare, under its own name or a donor's.
    for law in PhysicsLaw::ALL {
        assert!(
            CANVAS_PHYSICS_PROFILES
                .iter()
                .any(|p| p.law == law && p.overlays.is_empty()),
            "{} has a bare profile",
            law.id()
        );
    }
}

#[test]
fn a_living_law_runs_until_paused_and_a_graph_bound_law_survives_a_reconcile() {
    let mut canvas = Canvas::with_sample_graph();
    canvas.set_physics_law(PhysicsLaw::Orbit);
    assert!(canvas.physics_never_rests());
    assert!(canvas.is_settling(), "orbit keeps ticking");
    canvas.set_physics_law(PhysicsLaw::Stress);
    assert!(!canvas.physics_never_rests());
    let count = canvas.law_force_count();
    canvas.visit("https://a-new-node.example");
    assert_eq!(
        canvas.physics_law(),
        PhysicsLaw::Stress,
        "the law survives a topology change"
    );
    assert_eq!(
        canvas.law_force_count(),
        count,
        "stress rebuilt against the new topology"
    );
    // A graph swap keeps the choice too: the scene restore re-applies it afterwards anyway.
    canvas.set_graph(Graph::new());
    assert_eq!(canvas.physics_law(), PhysicsLaw::Stress);
}
