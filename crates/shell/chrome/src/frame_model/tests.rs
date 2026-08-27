// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::time::Duration;

use kernel::graph::NodeKey;
use kernel::time::PortableInstant;

use super::{FocusRingCurve, FocusRingSpec, FrameHostInput};

fn spec(node: NodeKey, started: PortableInstant, duration_ms: u64) -> FocusRingSpec {
    FocusRingSpec {
        node_key: node,
        started_at: started,
        duration: Duration::from_millis(duration_ms),
    }
}

#[test]
fn alpha_is_zero_when_focused_node_differs() {
    let now = PortableInstant(0);
    let s = spec(NodeKey::new(1), now, 500);
    assert_eq!(s.alpha_at(Some(NodeKey::new(2)), now), 0.0);
    assert_eq!(s.alpha_at(None, now), 0.0);
}

#[test]
fn alpha_is_full_at_start_and_zero_past_duration() {
    let start = PortableInstant(1_000);
    let s = spec(NodeKey::new(7), start, 500);
    // Exactly at start -> full intensity.
    assert!((s.alpha_at(Some(NodeKey::new(7)), start) - 1.0).abs() < 1e-6);
    // After duration -> clamped to zero.
    let past = start.saturating_add_ms(600);
    assert_eq!(s.alpha_at(Some(NodeKey::new(7)), past), 0.0);
}

#[test]
fn alpha_fades_linearly_through_duration() {
    let start = PortableInstant(1_000);
    let s = spec(NodeKey::new(3), start, 1_000);
    let half = start.saturating_add_ms(500);
    let alpha = s.alpha_at(Some(NodeKey::new(3)), half);
    // At t = duration/2: linear alpha = 0.5.
    assert!((alpha - 0.5).abs() < 1e-6);
}

#[test]
fn alpha_at_with_curve_applies_ease_out() {
    let start = PortableInstant(0);
    let s = spec(NodeKey::new(5), start, 1_000);
    let half = PortableInstant(500);
    let linear = s.alpha_at_with_curve(Some(NodeKey::new(5)), half, FocusRingCurve::Linear);
    let ease_out = s.alpha_at_with_curve(Some(NodeKey::new(5)), half, FocusRingCurve::EaseOut);
    // EaseOut at half-progress: (1 - 0.5)² = 0.25, vs linear 0.5.
    assert!((linear - 0.5).abs() < 1e-6);
    assert!((ease_out - 0.25).abs() < 1e-6);
}

#[test]
fn alpha_at_with_curve_applies_step() {
    let start = PortableInstant(0);
    let s = spec(NodeKey::new(5), start, 1_000);
    // Step: 1.0 while progress < 1.0, else 0.0.
    let half = PortableInstant(500);
    let almost = PortableInstant(999);
    let past = PortableInstant(1_000);
    assert_eq!(
        s.alpha_at_with_curve(Some(NodeKey::new(5)), half, FocusRingCurve::Step),
        1.0
    );
    assert_eq!(
        s.alpha_at_with_curve(Some(NodeKey::new(5)), almost, FocusRingCurve::Step),
        1.0
    );
    // At duration: clamped to 0.0 (elapsed >= duration gate).
    assert_eq!(
        s.alpha_at_with_curve(Some(NodeKey::new(5)), past, FocusRingCurve::Step),
        0.0
    );
}

#[test]
fn alpha_is_zero_when_duration_is_zero() {
    // Defensive: `duration_ms = 0` is a valid config (user picks
    // "instant-off ring" via reduced-motion preferences). Pin
    // that division-by-zero doesn't happen.
    let start = PortableInstant(0);
    let s = spec(NodeKey::new(2), start, 0);
    assert_eq!(s.alpha_at(Some(NodeKey::new(2)), start), 0.0);
}

#[test]
fn focus_ring_curve_from_str_and_display_round_trip() {
    for curve in [
        FocusRingCurve::Linear,
        FocusRingCurve::EaseOut,
        FocusRingCurve::Step,
    ] {
        let s = curve.to_string();
        let back: FocusRingCurve = s.parse().expect("round trip");
        assert_eq!(back, curve);
    }
}

#[test]
fn focus_ring_curve_default_is_linear() {
    assert_eq!(FocusRingCurve::default(), FocusRingCurve::Linear);
}

#[test]
fn focus_ring_curve_alpha_from_progress_clamps() {
    // Progress outside [0,1] clamps — pin it so malformed inputs
    // don't produce out-of-range alphas.
    assert_eq!(FocusRingCurve::Linear.alpha_from_progress(-0.5), 1.0 - 0.0);
    assert_eq!(FocusRingCurve::Linear.alpha_from_progress(1.5), 0.0);
}

#[test]
fn frame_host_input_default_is_empty() {
    let input = FrameHostInput::default();
    assert!(input.events.is_empty());
    assert!(input.pointer_hover.is_none());
    assert!(!input.wants_keyboard);
    assert!(!input.wants_pointer);
    assert!(!input.had_input_events);
}
