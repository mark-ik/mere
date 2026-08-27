// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The camera itself: zoom anchoring, the screen/world inverse, and a resize
//! that must hold the viewport's center world point fixed.

use super::*;

#[test]
fn zoom_at_keeps_the_anchor_world_point_fixed() {
    let mut canvas = Canvas::new();
    canvas.camera.offset = (100.0, 50.0);
    canvas.camera.zoom = 1.0;
    let anchor = (200.0, 80.0);
    let world = |o: &Canvas| {
        (
            (anchor.0 - o.camera.offset.0) / o.camera.zoom,
            (anchor.1 - o.camera.offset.1) / o.camera.zoom,
        )
    };
    let before = world(&canvas);
    canvas.zoom_at(anchor, 2.0);
    let after = world(&canvas);
    assert!((after.0 - before.0).abs() < 0.01, "anchor world x fixed");
    assert!((after.1 - before.1).abs() < 0.01, "anchor world y fixed");
    assert_eq!(canvas.camera.zoom, 2.0, "zoom applied");
}

#[test]
fn screen_to_world_inverts_the_camera() {
    let mut canvas = Canvas::new();
    canvas.camera.offset = (100.0, 50.0);
    canvas.camera.zoom = 2.0;
    let w = canvas.screen_to_world((300.0, 150.0));
    assert!((w.x - 100.0).abs() < 0.01, "world x = (300-100)/2");
    assert!((w.y - 50.0).abs() < 0.01, "world y = (150-50)/2");
}

#[test]
fn camera_round_trips_and_guards_bad_zoom() {
    let mut canvas = Canvas::new();
    canvas.set_camera(CameraView {
        offset: (123.0, -45.0),
        zoom: 2.5,
    });
    let cv = canvas.camera();
    assert_eq!(cv.offset, (123.0, -45.0));
    assert_eq!(cv.zoom, 2.5);
    // A zero / non-finite zoom falls back to 1.0 rather than collapsing.
    canvas.set_camera(CameraView {
        offset: (0.0, 0.0),
        zoom: 0.0,
    });
    assert_eq!(canvas.camera().zoom, 1.0);
}

#[test]
fn resize_keeps_the_viewport_center_world_point_fixed() {
    let mut canvas = Canvas::new();
    // A non-trivial pan + zoom at the starting viewport.
    canvas.camera.offset = (512.0, 300.0);
    canvas.camera.zoom = 1.5;
    let center_world =
        |o: &Canvas| o.screen_to_world((o.view_w as f32 / 2.0, o.view_h as f32 / 2.0));
    let before = center_world(&canvas);
    // Grow the surface the way startup does (1024x600 -> 2560x1504).
    canvas.resize(2560, 1504);
    let after = center_world(&canvas);
    assert!(
        (after.x - before.x).abs() < 0.01,
        "center world x fixed across resize"
    );
    assert!(
        (after.y - before.y).abs() < 0.01,
        "center world y fixed across resize"
    );
    assert_eq!(canvas.camera.zoom, 1.5, "resize leaves zoom untouched");
}
