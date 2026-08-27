// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The H1 reference fixture: literal demo truth for [`MereHost`].
//!
//! Data only. It sits beside `mere_host` rather than in it because none of it
//! is host behaviour — every line is a node, a relation, a facet, or an access
//! record that the demo and the tests read back, and keeping it here leaves
//! the host module about what the host does.

use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::{
    ArrangementSubKind, ContainmentSubKind, EdgeAssertion, Graph, NodeKey, ProvenanceSubKind,
    SemanticSubKind,
};
use muniment::Backend;
use serde_json::json;

use crate::access::{AccessContext, record_access};
use crate::handlers::HandlerRegistry;
use crate::mere_host::{
    FIXTURE_DEVICE_ONE_ADDRESS, FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_FILE_ADDRESS,
    FIXTURE_GRANT_ADDRESS, FIXTURE_KEY_ADDRESS, FIXTURE_NON_WEB_ADDRESS, FIXTURE_PERSONA_ADDRESS,
    FIXTURE_RECEIPT_ADDRESS, FIXTURE_REMOTE_ADDRESS, FIXTURE_SCENE_ADDRESS, FIXTURE_WEB_ADDRESS,
    MereHost, MereHostError, SelectedPersonaRef, UNKNOWN_FIXTURE_FACET,
};

impl<B: Backend> MereHost<B> {
    /// Deterministic H1 source truth with synthetic public identity material.
    pub fn fixture(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
    ) -> Result<Self, MereHostError> {
        let mut host = Self::empty(
            backend,
            selected_persona.clone(),
            handlers,
            AccessContext {
                persona: selected_persona.persona.clone(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 3_000,
            },
        );

        let web = add_fixture_node(
            &mut host.graph,
            FIXTURE_WEB_ADDRESS,
            "I2P port notes",
            &["research", "transport"],
            10,
        );
        let non_web = add_fixture_node(
            &mut host.graph,
            FIXTURE_NON_WEB_ADDRESS,
            "I2P service",
            &["address", "transport"],
            20,
        );
        let file = add_fixture_node(
            &mut host.graph,
            FIXTURE_FILE_ADDRESS,
            "Reference notes",
            &["local", "reference"],
            30,
        );
        let scene = add_fixture_node(
            &mut host.graph,
            FIXTURE_SCENE_ADDRESS,
            "Reference-host scene",
            &["scene"],
            40,
        );
        let remote = add_fixture_node(
            &mut host.graph,
            FIXTURE_REMOTE_ADDRESS,
            "Mounted remote projection",
            &["remote", "projection"],
            50,
        );
        let persona = add_fixture_node(
            &mut host.graph,
            FIXTURE_PERSONA_ADDRESS,
            "Alice",
            &["identity", "public"],
            60,
        );
        let device_one = add_fixture_node(
            &mut host.graph,
            FIXTURE_DEVICE_ONE_ADDRESS,
            "Alice's laptop",
            &["device", "public"],
            70,
        );
        let device_two = add_fixture_node(
            &mut host.graph,
            FIXTURE_DEVICE_TWO_ADDRESS,
            "Alice's phone",
            &["device", "public"],
            80,
        );
        let key = add_fixture_node(
            &mut host.graph,
            FIXTURE_KEY_ADDRESS,
            "SSH signing key reference",
            &["key-reference", "public"],
            90,
        );
        let grant = add_fixture_node(
            &mut host.graph,
            FIXTURE_GRANT_ADDRESS,
            "Open-address grant",
            &["grant", "public"],
            100,
        );
        let receipt = add_fixture_node(
            &mut host.graph,
            FIXTURE_RECEIPT_ADDRESS,
            "Synthetic signing receipt",
            &["receipt", "public", "synthetic"],
            110,
        );

        assert_relation(
            &mut host.graph,
            web,
            non_web,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: Some("I2P endpoint".to_string()),
                decay_progress: None,
            },
        );
        assert_relation(
            &mut host.graph,
            web,
            file,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: Some("working notes".to_string()),
                decay_progress: None,
            },
        );
        assert_relation(
            &mut host.graph,
            scene,
            web,
            EdgeAssertion::Arrangement {
                sub_kind: ArrangementSubKind::FrameMember,
            },
        );
        assert_relation(
            &mut host.graph,
            scene,
            file,
            EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::CollectionMember,
            },
        );
        assert_relation(
            &mut host.graph,
            remote,
            scene,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Elaborates,
                label: Some("mounted beside local graph".to_string()),
                decay_progress: None,
            },
        );
        assert_relation(
            &mut host.graph,
            persona,
            device_one,
            EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::CollectionMember,
            },
        );
        assert_relation(
            &mut host.graph,
            persona,
            device_two,
            EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::CollectionMember,
            },
        );
        assert_relation(
            &mut host.graph,
            device_one,
            key,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::DependsOn,
                label: Some("public key reference".to_string()),
                decay_progress: None,
            },
        );
        assert_relation(
            &mut host.graph,
            grant,
            persona,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Supports,
                label: Some("subject".to_string()),
                decay_progress: None,
            },
        );
        assert_relation(
            &mut host.graph,
            receipt,
            grant,
            EdgeAssertion::Provenance {
                sub_kind: ProvenanceSubKind::GeneratedFrom,
            },
        );

        host.set_facet(
            remote,
            "graphshell.remote-mount/v1",
            json!({
                "endpoint": "loopback:g1",
                "session": "loopback:g1-presentation",
                "status": "mounted"
            }),
        )?;
        host.set_facet(
            persona,
            "personae.public-persona/v1",
            json!({
                "display_name": "Alice",
                "profile_ref": selected_persona.profile,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            device_one,
            "personae.public-device/v1",
            json!({
                "label": "laptop",
                "subject": FIXTURE_PERSONA_ADDRESS,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            device_two,
            "personae.public-device/v1",
            json!({
                "label": "phone",
                "subject": FIXTURE_PERSONA_ADDRESS,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            key,
            "personae.public-key-reference/v1",
            json!({
                "algorithm": "ssh-ed25519",
                "fingerprint": "SHA256:synthetic-graphshell-h1",
                "synthetic": true
            }),
        )?;
        host.set_facet(
            grant,
            "personae.public-grant/v1",
            json!({
                "subject": FIXTURE_PERSONA_ADDRESS,
                "action": "graphshell.address.open",
                "expires_at_ms": 9_999_999,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            receipt,
            "personae.signing-receipt/v1",
            json!({
                "key_ref": FIXTURE_KEY_ADDRESS,
                "payload_digest": "blake3:synthetic",
                "signature_ref": "personae-signature:synthetic",
                "synthetic": true
            }),
        )?;
        host.set_facet(
            file,
            UNKNOWN_FIXTURE_FACET,
            json!({
                "carrier": "future",
                "facets": ["opaque", "preserve-me"],
                "version": 7
            }),
        )?;

        record_access(
            &mut host.graph,
            web,
            &AccessContext {
                persona: selected_persona.persona.clone(),
                device: FIXTURE_DEVICE_ONE_ADDRESS.to_string(),
                at_ms: 1_000,
            },
            "graphshell.inspect",
        )?;
        record_access(
            &mut host.graph,
            web,
            &AccessContext {
                persona: selected_persona.persona,
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 2_000,
            },
            "system.default",
        )?;

        let score = host.score();
        host.set_facet(
            scene,
            "graphshell.saved-scene/v1",
            serde_json::to_value(score).expect("Scenograph scores serialize"),
        )?;
        host.projection_revision = 1;
        Ok(host)
    }
}

fn add_fixture_node(
    graph: &mut Graph,
    address: &str,
    title: &str,
    tags: &[&str],
    at_ms: u64,
) -> NodeKey {
    let id = Graph::node_namespace_id(address);
    let key = add_node(
        graph,
        Some(id),
        address.to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    apply_graph_delta(
        graph,
        GraphDelta::SetNodeTitle {
            key,
            title: title.to_string(),
        },
    );
    for tag in tags {
        apply_graph_delta(
            graph,
            GraphDelta::InsertNodeTag {
                key,
                tag: (*tag).to_string(),
            },
        );
    }
    apply_graph_delta(
        graph,
        GraphDelta::ReplayTouchNodeLastVisitedById {
            node_id: id,
            timestamp_ms: at_ms,
        },
    );
    key
}
