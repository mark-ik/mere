// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The node-representation axes: face (what the tile shows), body / material,
//! and how each round-trips through cartography.

use super::*;

#[test]
fn node_face_defaults_to_favicon_and_takes_a_per_node_override() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let (key, id) = {
        let (key, node) = canvas
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };
    assert_eq!(
        canvas.node_face(key),
        Face::Favicon,
        "the default face is Favicon, so the look is unchanged",
    );

    // A per-node override is the user's face choice; it wins over the default.
    canvas.set_node_face(id, Face::Bare);
    assert_eq!(
        canvas.node_face(key),
        Face::Bare,
        "the per-node override wins over the default"
    );

    // Clearing the override reverts the node to the default face.
    canvas.clear_node_face(id);
    assert_eq!(
        canvas.node_face(key),
        Face::Favicon,
        "clearing the override reverts to Favicon"
    );
}

#[test]
fn face_and_body_are_independent_axes() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let (key, id) = {
        let (key, node) = canvas
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };

    // Dropping an image gives the node a sprite face and a traced body hull (the import path
    // sets the two together).
    canvas.set_node_sprite(id, "data:image/png;base64,AAAA".to_string());
    canvas.set_node_sprite_hull(id, vec![(-0.4, -0.4), (0.4, -0.4), (0.0, 0.4)]);
    assert_eq!(canvas.node_sprite(key), Some("data:image/png;base64,AAAA"));
    assert_eq!(
        canvas.node_face(key),
        Face::Sprite,
        "a sprite node wears the Sprite face"
    );
    assert!(
        canvas.node_sprite_hull(key).is_some(),
        "and carries the traced body hull"
    );

    // DECOUPLE: switching the face back to Favicon keeps the body hull AND the sprite image —
    // face and body are independent axes (a custom-bodied node can wear a favicon).
    canvas.set_node_face(id, Face::Favicon);
    assert_eq!(canvas.node_face(key), Face::Favicon);
    assert!(
        canvas.node_sprite_hull(key).is_some(),
        "a face switch never reshapes the body"
    );
    assert_eq!(
        canvas.node_sprite(key),
        Some("data:image/png;base64,AAAA"),
        "a face switch never discards the imported sprite",
    );

    // Resetting the body drops the hull (back to the silhouette) but leaves the face alone.
    canvas.clear_node_body(id);
    assert!(
        canvas.node_sprite_hull(key).is_none(),
        "reset body drops the hull"
    );
    assert_eq!(
        canvas.node_face(key),
        Face::Favicon,
        "reset body leaves the face untouched"
    );

    // Removing the sprite drops the image and reverts a still-Sprite face to Favicon.
    canvas.set_node_face(id, Face::Sprite);
    canvas.clear_node_sprite(id);
    assert_eq!(
        canvas.node_sprite(key),
        None,
        "remove sprite drops the image"
    );
    assert_eq!(
        canvas.node_face(key),
        Face::Favicon,
        "and reverts a Sprite face to Favicon"
    );
}

#[test]
fn node_material_overrides_default_and_round_trips_through_cartography() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let (key, id) = {
        let (key, node) = canvas
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };

    // A node takes the default material until overridden.
    assert_eq!(canvas.node_material(key), NodeMaterial::default());

    // An override sets the body's physics (restitution / friction / density). Gravity scale
    // stays at the default 0 — the canvas is a layout surface, and it is not part of the
    // sidecar tuple below.
    canvas.set_node_material(
        id,
        NodeMaterial {
            restitution: 0.6,
            friction: 0.3,
            density: 0.002,
            ..NodeMaterial::default()
        },
    );
    assert_eq!(canvas.node_material(key).restitution, 0.6);
    assert_eq!(canvas.node_material(key).density, 0.002);

    // The override travels to the cartography sidecar as a (restitution, friction, density) tuple.
    let geom = canvas.cartography_geometry();
    let exported: std::collections::HashMap<_, _> = geom.material_iter().collect();
    assert_eq!(
        exported.get(&id),
        Some(&(0.6, 0.3, 0.002)),
        "the material is exported"
    );

    // Clearing reverts to default; re-applying from the sidecar restores it.
    canvas.clear_node_material(id);
    assert_eq!(
        canvas.node_material(key),
        NodeMaterial::default(),
        "cleared reverts to default"
    );
    canvas.apply_cartography_materials(geom.material_iter());
    assert_eq!(
        canvas.node_material(key).restitution,
        0.6,
        "the sidecar round-trips the material"
    );
}

#[test]
fn node_face_override_round_trips_through_cartography() {
    let mut graph = Graph::new();
    graph.add_node(
        "https://one.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    let (key, id) = {
        let (key, node) = canvas
            .graph()
            .get_node_by_url("https://one.example")
            .unwrap();
        (key, node.id)
    };

    // A per-node face override travels to the sidecar as a string code.
    canvas.set_node_face(id, Face::Bare);
    let geom = canvas.cartography_geometry();
    let faces: std::collections::HashMap<_, _> = geom.face_iter().collect();
    assert_eq!(
        faces.get(&id),
        Some(&"bare"),
        "the face override is exported"
    );

    // Clearing reverts to the default; re-applying from the sidecar restores it.
    canvas.clear_node_face(id);
    assert_eq!(
        canvas.node_face(key),
        Face::Favicon,
        "cleared reverts to the default face"
    );
    canvas.apply_cartography_faces(geom.face_iter());
    assert_eq!(
        canvas.node_face(key),
        Face::Bare,
        "the sidecar round-trips the face"
    );
}
