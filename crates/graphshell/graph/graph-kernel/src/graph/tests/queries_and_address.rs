/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Query tests — address-kind detection, MIME hint, connectivity
//! (hop distances, shortest path, reachability, weak / strong
//! components), neighbor accessors, tag pruning, frame-layout hints.

use super::super::*;

fn hyperlink() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    }
}

// --- MIME / address-kind detection tests ---

#[test]
fn node_created_with_http_url_has_http_address_kind() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));
    let node = graph.get_node(key).unwrap();
    assert_eq!(node.primary_address().address_kind(), AddressKind::Http);
}

#[test]
fn node_created_with_file_url_has_file_address_kind() {
    let mut graph = Graph::new();
    let key = graph.add_node(
        "file:///home/user/doc.pdf".to_string(),
        Point2D::new(0.0, 0.0),
    );
    let node = graph.get_node(key).unwrap();
    assert_eq!(node.primary_address().address_kind(), AddressKind::File);
}

#[test]
fn node_created_with_file_pdf_url_gets_pdf_mime_hint() {
    let mut graph = Graph::new();
    let key = graph.add_node(
        "file:///home/user/document.pdf".to_string(),
        Point2D::new(0.0, 0.0),
    );
    let node = graph.get_node(key).unwrap();
    assert_eq!(node.mime_hint.as_deref(), Some("application/pdf"));
    assert_eq!(node.primary_address().address_kind(), AddressKind::File);
}

#[test]
fn node_created_with_http_url_has_no_mime_hint_by_default() {
    let mut graph = Graph::new();
    let key = graph.add_node(
        "https://example.com/page".to_string(),
        Point2D::new(0.0, 0.0),
    );
    let node = graph.get_node(key).unwrap();
    // Plain HTTP URLs without a recognisable extension yield no MIME hint.
    assert!(node.mime_hint.is_none());
}

// Address type and function tests have moved to `mere_kernel::address::tests`.

#[test]
fn node_address_field_is_consistent_with_url_and_address_kind_at_creation() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));
    let node = graph.get_node(key).unwrap();
    assert_eq!(
        node.primary_address(),
        &Address::Http("https://example.com".to_string())
    );
    assert_eq!(node.primary_address().address_kind(), AddressKind::Http);
    assert_eq!(node.primary_address().as_url_str(), node.url());
    // Invariant: exactly one Primary claim per node at creation.
    assert_eq!(node.addresses.len(), 1);
    assert!(node.addresses[0].is_primary());
}

#[test]
fn node_address_field_stays_in_sync_after_url_update() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));
    graph.update_node_url(key, "file:///home/user/doc.txt".to_string());
    let node = graph.get_node(key).unwrap();
    assert_eq!(
        node.primary_address(),
        &Address::File("file:///home/user/doc.txt".to_string())
    );
    assert_eq!(node.primary_address().address_kind(), AddressKind::File);
    assert_eq!(node.primary_address().as_url_str(), node.url());
}

#[test]
fn snapshot_roundtrip_preserves_mime_hint_and_address_kind() {
    let mut graph = Graph::new();
    let key = graph.add_node(
        "file:///home/user/report.pdf".to_string(),
        Point2D::new(0.0, 0.0),
    );
    assert_eq!(
        graph.get_node(key).unwrap().mime_hint.as_deref(),
        Some("application/pdf")
    );
    assert_eq!(
        graph
            .get_node(key)
            .unwrap()
            .primary_address()
            .address_kind(),
        AddressKind::File
    );

    let snapshot = graph.to_snapshot();
    let restored = Graph::from_snapshot(&snapshot);
    let (_, rnode) = restored
        .get_node_by_url("file:///home/user/report.pdf")
        .unwrap();
    assert_eq!(rnode.mime_hint.as_deref(), Some("application/pdf"));
    assert_eq!(rnode.primary_address().address_kind(), AddressKind::File);
}

#[test]
fn hop_distances_shortest_path_and_reachability_use_undirected_connectivity() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 0.0));
    let d = graph.add_node("https://d.com".to_string(), Point2D::new(3.0, 0.0));

    let _ = graph.assert_relation(a, b, hyperlink());
    let _ = graph.assert_relation(b, c, hyperlink());

    let hops = graph.hop_distances_from(a);
    assert_eq!(hops.get(&a).copied(), Some(0));
    assert_eq!(hops.get(&b).copied(), Some(1));
    assert_eq!(hops.get(&c).copied(), Some(2));
    assert!(hops.get(&d).is_none());

    let path = graph.shortest_path(a, c).expect("path should exist");
    assert_eq!(path.first().copied(), Some(a));
    assert_eq!(path.last().copied(), Some(c));

    assert!(graph.is_reachable(a, c));
    assert!(!graph.is_reachable(a, d));
}

#[test]
fn orphan_and_weak_component_accessors_report_expected_partitions() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 0.0));
    let d = graph.add_node("https://d.com".to_string(), Point2D::new(3.0, 0.0));
    let e = graph.add_node("https://e.com".to_string(), Point2D::new(4.0, 0.0));

    let _ = graph.assert_relation(a, b, hyperlink());
    let _ = graph.assert_relation(d, e, hyperlink());

    let mut orphans = graph.orphan_node_keys();
    orphans.sort_by_key(|k| k.index());
    assert_eq!(orphans, vec![c]);

    let mut sizes: Vec<usize> = graph
        .weakly_connected_components()
        .into_iter()
        .map(|component| component.len())
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![1, 2, 2]);
}

#[test]
fn component_accessors_handle_empty_graph() {
    let graph = Graph::new();

    assert!(graph.orphan_node_keys().is_empty());
    assert!(graph.weakly_connected_components().is_empty());
    assert!(graph.strongly_connected_components().is_empty());
}

#[test]
fn sorted_neighbor_and_connected_import_accessors_are_stable() {
    let mut graph = Graph::new();
    let seed = graph.add_node("https://seed.example".to_string(), Point2D::new(0.0, 0.0));
    let left = graph.add_node("https://left.example".to_string(), Point2D::new(1.0, 0.0));
    let right = graph.add_node("https://right.example".to_string(), Point2D::new(2.0, 0.0));
    let shared = graph.add_node("https://shared.example".to_string(), Point2D::new(2.5, 0.0));
    let isolated = graph.add_node(
        "https://isolated.example".to_string(),
        Point2D::new(3.0, 0.0),
    );

    let _ = graph.assert_relation(seed, right, hyperlink());
    let _ = graph.assert_relation(left, seed, hyperlink());
    let _ = graph.assert_relation(left, shared, hyperlink());
    let _ = graph.assert_relation(right, shared, hyperlink());

    let sorted_neighbors = graph.neighbors_undirected_sorted(seed);
    assert_eq!(sorted_neighbors, vec![left, right]);

    let import_nodes = graph.connected_frame_import_nodes(&[isolated, seed]);
    assert_eq!(import_nodes, vec![seed, left, right, isolated]);

    let depth_one = graph.connected_candidates_with_depth(seed, 1);
    assert_eq!(depth_one, vec![(left, 1), (right, 1)]);

    let depth_two = graph.connected_candidates_with_depth(seed, 2);
    assert!(depth_two.contains(&(left, 1)));
    assert!(depth_two.contains(&(right, 1)));
    assert!(depth_two.contains(&(shared, 2)));
}

#[test]
fn strongly_connected_components_reports_cycle_partition() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.com".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b.com".to_string(), Point2D::new(1.0, 0.0));
    let c = graph.add_node("https://c.com".to_string(), Point2D::new(2.0, 0.0));
    let d = graph.add_node("https://d.com".to_string(), Point2D::new(3.0, 0.0));

    let _ = graph.assert_relation(a, b, hyperlink());
    let _ = graph.assert_relation(b, c, hyperlink());
    let _ = graph.assert_relation(c, a, hyperlink());
    let _ = graph.assert_relation(c, d, hyperlink());

    let mut sizes: Vec<usize> = graph
        .strongly_connected_components()
        .into_iter()
        .map(|component| component.len())
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![1, 3]);
}

#[test]
fn removing_tag_prunes_stale_icon_override() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));
    assert!(graph.insert_node_tag(key, "research".to_string()));
    assert!(graph.set_node_tag_icon_override(
        key,
        "research",
        Some(crate::types::BadgeIcon::Emoji("🔬".to_string()))
    ));

    assert!(graph.remove_node_tag(key, "research"));
    assert!(
        graph
            .node_tag_presentation(key)
            .is_some_and(|presentation| presentation.icon_overrides.is_empty())
    );
}

#[test]
fn system_tag_icon_cannot_be_overridden() {
    let mut graph = Graph::new();
    let key = graph.add_node("https://example.com".to_string(), Point2D::new(0.0, 0.0));
    assert!(graph.insert_node_tag(key, "#pin".to_string()));
    assert!(!graph.set_node_tag_icon_override(
        key,
        "#pin",
        Some(crate::types::BadgeIcon::Emoji("🔬".to_string()))
    ));
}

#[test]
fn frame_layout_metadata_survives_snapshot_roundtrip() {
    let mut graph = Graph::new();
    let frame = graph.add_node("verso://frame/demo".to_string(), Point2D::new(0.0, 0.0));
    let first = graph.add_node("https://first.example".to_string(), Point2D::new(1.0, 0.0));
    let second = graph.add_node("https://second.example".to_string(), Point2D::new(2.0, 0.0));
    let first_id = graph.get_node(first).unwrap().id.to_string();
    let second_id = graph.get_node(second).unwrap().id.to_string();

    assert!(graph.append_frame_layout_hint(
        frame,
        FrameLayoutHint::SplitHalf {
            first: first_id.clone(),
            second: second_id.clone(),
            orientation: SplitOrientation::Horizontal,
        },
    ));
    assert!(graph.set_frame_split_offer_suppressed(frame, true));

    let restored = Graph::from_snapshot(&graph.to_snapshot());
    let (restored_frame, _) = restored.get_node_by_url("verso://frame/demo").unwrap();
    let hints = restored.frame_layout_hints(restored_frame).unwrap();

    assert_eq!(hints.len(), 1);
    assert_eq!(
        hints[0],
        FrameLayoutHint::SplitHalf {
            first: first_id,
            second: second_id,
            orientation: SplitOrientation::Horizontal,
        }
    );
    assert_eq!(
        restored.frame_split_offer_suppressed(restored_frame),
        Some(true)
    );
}

#[test]
fn frame_layout_hint_move_reorders_hints() {
    let mut graph = Graph::new();
    let frame = graph.add_node("verso://frame/demo".to_string(), Point2D::new(0.0, 0.0));
    let first = graph.add_node("https://first.example".to_string(), Point2D::new(1.0, 0.0));
    let second = graph.add_node("https://second.example".to_string(), Point2D::new(2.0, 0.0));
    let third = graph.add_node("https://third.example".to_string(), Point2D::new(3.0, 0.0));
    let first_id = graph.get_node(first).unwrap().id.to_string();
    let second_id = graph.get_node(second).unwrap().id.to_string();
    let third_id = graph.get_node(third).unwrap().id.to_string();

    assert!(graph.append_frame_layout_hint(
        frame,
        FrameLayoutHint::SplitHalf {
            first: first_id.clone(),
            second: second_id.clone(),
            orientation: SplitOrientation::Vertical,
        },
    ));
    assert!(graph.append_frame_layout_hint(
        frame,
        FrameLayoutHint::SplitHalf {
            first: second_id.clone(),
            second: third_id.clone(),
            orientation: SplitOrientation::Horizontal,
        },
    ));

    assert!(graph.move_frame_layout_hint(frame, 1, 0));
    let hints = graph.frame_layout_hints(frame).unwrap();
    assert_eq!(
        hints[0],
        FrameLayoutHint::SplitHalf {
            first: second_id.clone(),
            second: third_id,
            orientation: SplitOrientation::Horizontal,
        }
    );
    assert_eq!(
        hints[1],
        FrameLayoutHint::SplitHalf {
            first: first_id,
            second: second_id.clone(),
            orientation: SplitOrientation::Vertical,
        }
    );
    assert!(!graph.move_frame_layout_hint(frame, 1, 1));
}
