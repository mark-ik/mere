// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use chirograph::{CapabilityProfile, PresentationCapability, ProjectionSession};
use graphshell::client::RetainedEndpointSession;
use graphshell::view::{ProjectionLayoutView, ProjectionReceiptView, render_projection_receipt};
use graphshell_endpoint::ResumableProjectionSource;
use graphshell_local::LocalCarrier;
use knot_editor::{KnotEndpoint, KnotRosetteConfig, RosetteConfig};
use tempfile::tempdir;

const POEM: &str = "Morning gathers light\nBranches answer night\n\nFootsteps cross the hill\nEvening settles still\n";
const LYRIC: &str =
    "Raise your open hand\nWe will take a stand\n\nCarry home the song\nLet the road run long\n";

#[test]
fn two_real_knot_documents_mount_and_render_as_independent_rosettes() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("poem.knot"), POEM).unwrap();
    std::fs::write(root.path().join("lyric.knot"), LYRIC).unwrap();

    let endpoint = KnotEndpoint::open(root.path())
        .unwrap()
        .with_rosette_config(KnotRosetteConfig {
            geometry: RosetteConfig {
                radius: 180.0,
                ..RosetteConfig::default()
            },
            max_source_bytes: 64 * 1024,
        });
    let carrier = LocalCarrier::new(endpoint, |endpoint, request| endpoint.resume(request));
    let mut retained = RetainedEndpointSession::over(
        Box::new(carrier),
        CapabilityProfile::new([
            PresentationCapability::PortableCard,
            PresentationCapability::NativeGlyph,
        ]),
    )
    .unwrap();

    let rosettes = retained
        .descriptor()
        .projections
        .iter()
        .enumerate()
        .filter(|(_, offer)| offer.label.starts_with("Rosette · "))
        .map(|(index, offer)| (index, offer.label.clone()))
        .collect::<Vec<_>>();
    assert_eq!(rosettes.len(), 2);

    let mut mounted = Vec::new();
    for (index, label) in rosettes {
        let session = retained.mount(index).unwrap();
        let expected = if label.contains("poem") {
            "Morning gathers light"
        } else {
            "Raise your open hand"
        };
        assert_rendered_rosette(&mut retained, &session, expected);
        mounted.push(session);
    }

    assert_ne!(mounted[0], mounted[1]);
    assert!(retained.client().mounted(&mounted[0]).is_some());
    assert!(retained.client().mounted(&mounted[1]).is_some());
}

fn assert_rendered_rosette(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
    expected_line: &str,
) {
    let scene = retained.client().mounted(session).unwrap().scene.clone();
    assert_eq!(scene.active_item_count(), 6);
    assert!(scene.tables.relations.iter().flatten().count() >= 2);
    let presentations = retained
        .resolve_all(session)
        .unwrap()
        .into_iter()
        .map(|(_, presentation)| presentation)
        .collect::<Vec<_>>();
    assert_eq!(presentations.len(), scene.active_item_count());

    let html = render_projection_receipt(&ProjectionReceiptView {
        eyebrow: "Knot · Rosette".into(),
        title: "Rosette".into(),
        lede: "A live sound projection.".into(),
        session: session.0.clone(),
        status: "Live".into(),
        presentations,
        layout: Some(ProjectionLayoutView::from_scene(&scene)),
        intents: Vec::new(),
    });
    assert!(html.contains(expected_line));
    assert!(
        html.contains("<line "),
        "rhyme chords reach the headed view"
    );
}
