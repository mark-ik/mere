// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! H6 source: prepare one transfer and keep it alive across three admissions.

use std::path::Path;
use std::sync::RwLock;

use eidetic::PrivacyClass;
use graphshell::access::AccessContext;
use graphshell::carrier::{accept_projection_session, projection_alpn, projection_policy};
use graphshell::lifecycle::SessionAuthority;
use graphshell::mere_host::{MereHost, SelectedPersonaRef, fixture_handlers};
use graphshell::product::{EditableRelation, ExportRequest, LocalFileMetadata, TransferScope};
use graphshell::session_loop::{SessionEnd, serve_admitted_session};
use graphshell::transfer::{
    AccessTransferPolicy, TransferBlobInput, TransferEndpointV1, TransferOperation,
    TransferRequest, TransferRouteV1, prepare_transfer,
};
use graphshell::transfer_endpoint::TransferSourceEndpoint;
use graphshell_endpoint::ResumableProjectionSource;
use muniment::{BlobStore, MemoryBackend};
use notochord::{NetworkId, RevocationLedger, TrustedRoot};
use personae::delegation::{DelegationRevocation, SignedDelegationRevocation};
use personae::{IdentityProvider, InMemoryProvider};
use sha2::{Digest, Sha256};
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};
use uuid::Uuid;

use crate::ROOT_AUTHORITY;
use crate::identity::{assert_same_key, device, graph, now_ms, persona, profile, short};

pub(crate) async fn serve(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    destination_key: [u8; 32],
    network: NetworkId,
    file: &Path,
    revoked: bool,
) -> Result<(), String> {
    let file_bytes =
        std::fs::read(file).map_err(|error| format!("read {}: {error}", file.display()))?;
    if file_bytes.is_empty() {
        return Err("the physical transfer fixture must not be empty".to_string());
    }
    let file_byte_len = file_bytes.len();
    let source_key = me.master_public_key().to_bytes();
    let persona = persona(&owner);
    let source_device = device(&source_key);
    let destination_device = device(&destination_key);

    let backend = MemoryBackend::new();
    let mut host = MereHost::empty(
        backend.clone(),
        SelectedPersonaRef {
            persona: persona.clone(),
            profile: "mere.base".to_string(),
        },
        fixture_handlers(),
        AccessContext {
            persona: persona.clone(),
            device: source_device.clone(),
            at_ms: now_ms(),
        },
    );
    let url = host
        .create_address(
            "https://example.test/graphshell/h6-physical",
            "Graphshell H6 physical carrier",
        )
        .map_err(|error| error.to_string())?;
    host.edit_node(
        url,
        "Graphshell H6 physical carrier",
        ["h6".to_string(), "physical".to_string(), "url".to_string()],
    )
    .map_err(|error| error.to_string())?;

    // Spelled out rather than `{:x}`: on the digest 0.11 row a digest is a
    // `hybrid_array::Array`, which has no `LowerHex` impl. Same shape as
    // `transfer::hex_digest`, which this bin cannot reach (a bin links the
    // library as an external crate, and that helper is private to it).
    let sha256: String = Sha256::digest(&file_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let file_name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("h6-physical.bin")
        .to_string();
    let file_node = host
        .create_file_metadata(LocalFileMetadata {
            content_hash: sha256,
            name: file_name,
            media_type: "application/octet-stream".to_string(),
            byte_len: file_bytes.len() as u64,
            last_modified_ms: now_ms(),
        })
        .map_err(|error| error.to_string())?;
    host.edit_node(
        file_node,
        "H6 physical file",
        ["file".to_string(), "h6".to_string(), "physical".to_string()],
    )
    .map_err(|error| error.to_string())?;
    host.assert_product_relation(file_node, url, EditableRelation::Cites)
        .map_err(|error| error.to_string())?;

    let source_blobs = BlobStore::new(backend.clone());
    let mut authority = backend.clone();
    let transfer_id = Uuid::new_v4();
    let manifest = prepare_transfer(
        &host,
        &mut authority,
        &source_blobs,
        TransferRequest {
            transfer_id,
            operation: TransferOperation::Replicate,
            source: TransferEndpointV1 {
                graph: graph(&source_key),
                persona: persona.clone(),
                device: source_device,
            },
            destination: TransferEndpointV1 {
                graph: graph(&destination_key),
                persona,
                device: destination_device,
            },
            route: TransferRouteV1 {
                carrier: "graphshell.admitted.p2panda/v1".to_string(),
                peer: short(&destination_key),
            },
            selection: ExportRequest {
                focused: file_node,
                selected: vec![file_node, url],
                scope: TransferScope::SelectedSubgraph,
                exported_at_ms: now_ms(),
                include_local_file_locations: false,
                scene: None,
            },
            access_policy: AccessTransferPolicy::ExcludeSourceHistory,
            privacy: PrivacyClass::TrustedPeersOnly,
        },
        vec![TransferBlobInput {
            node_id: file_node,
            role: "primary".to_string(),
            media_type: "application/octet-stream".to_string(),
            bytes: file_bytes.clone(),
        }],
    )
    .await
    .map_err(|error| error.to_string())?;
    let blob_hash = manifest.blobs[0].content_hash;
    let mut endpoint = TransferSourceEndpoint::new(manifest, [(blob_hash, file_bytes)])
        .map_err(|error| error.to_string())?;

    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![projection_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|error| format!("bind: {error}"))?;
    assert_same_key(&carrier, &me)?;
    let ticket = carrier
        .ticket()
        .await
        .map_err(|error| format!("ticket: {error}"))?;

    println!("h6_transfer_peer serve");
    println!("  transfer: {transfer_id}");
    println!("  objects: 2; blobs: 1; bytes: {file_byte_len}");
    println!("  ticket: {ticket}");
    println!("  run on the destination:");
    println!("    h6_transfer_peer connect --peer {ticket}");
    if revoked {
        println!("  the third session's transfer intent will be revoked");
    }

    let policy = projection_policy(
        network,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: owner.master_public_key().to_bytes(),
        }],
        vec![profile()],
        None,
    );
    let revocations = RwLock::new(RevocationLedger::new());

    for attempt in 1..=3 {
        println!("  waiting for session {attempt}...");
        let admission_ledger = revocations.read().expect("ledger lock").clone();
        let outcome = accept_projection_session(&carrier, &policy, &admission_ledger, now_ms(), 0)
            .await
            .map_err(|error| format!("accept: {error}"))?;
        let mut session = match outcome {
            Ok(session) => session,
            Err(refusal) => return Err(format!("admission refused: {refusal:?}")),
        };
        println!(
            "  session {attempt}: admitted {}",
            short(&session.principal.subject)
        );
        let authority = SessionAuthority::retain_admitted(&session);

        if revoked
            && attempt == 3
            && let Some(certificate) = session.claims.delegations.first()
        {
            let statement = SignedDelegationRevocation::issue(
                &owner,
                DelegationRevocation::new(
                    certificate.certificate.id(),
                    owner.master_public_key().to_bytes(),
                    certificate.certificate.scope.clone(),
                    now_ms(),
                    [12; 32],
                ),
            )
            .expect("issue revocation");
            if !revocations.write().expect("ledger lock").fold(&statement) {
                return Err("the owner's transfer revocation did not verify".to_string());
            }
            println!("  grant revoked before the transfer intent");
        }

        let mut resume = |endpoint: &mut TransferSourceEndpoint, request| {
            ResumableProjectionSource::resume(endpoint, request).map_err(|error| error.to_string())
        };
        let summary = serve_admitted_session(
            &mut session,
            &authority,
            &revocations,
            &mut endpoint,
            &mut resume,
            now_ms,
        )
        .await
        .map_err(|error| format!("serve: {error}"))?;
        println!(
            "  session {attempt}: served {}; ended {:?}",
            summary.answered, summary.end
        );
        if attempt < 3 && matches!(summary.end, SessionEnd::Lapsed(_)) {
            return Err("transfer authority lapsed before the revocation proof".to_string());
        }
    }
    Ok(())
}
