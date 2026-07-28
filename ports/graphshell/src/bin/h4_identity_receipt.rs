//! Generate H4a's headed and machine-readable Personae approval receipts.

use std::path::PathBuf;
use std::time::Duration;

use graphshell::identity::VaultProtectionView;
use graphshell::identity_projection::{
    SIGNING_APPROVE_ONCE_INTENT, SigningDecisionIntentV1, render_identity_surface,
};
use graphshell::native::personae_host::PersonaeHost;
use personae::ssh_slot::{protocol_key_for, slot_for};
use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId, UnlockTier};
use serde_json::json;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::proto::SignRequest;
use ssh_key::Algorithm;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ports/graphshell/docs/receipts"));
    std::fs::create_dir_all(&output_root)?;

    let mut private = ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)?;
    private.set_comment("Graphshell receipt key");
    let public = ssh_key::PublicKey::from(&private);
    let mut profile = Profile::new(
        ProfileId("research".to_string()),
        "Research",
        Ed25519Keypair::from_seed([0x44; 32]),
    );
    profile.slots.insert(
        protocol_key_for(&private),
        slot_for(&private, UnlockTier::PerUse)?,
    );
    let host = PersonaeHost::with_decision_timeout(
        IdentityVault::with_profile(InMemoryStorage::new(), profile),
        None,
        VaultProtectionView::Ephemeral,
        Duration::from_secs(5),
    );
    let mut agent = host.agent_session();
    let signing = tokio::spawn(async move {
        agent
            .sign(SignRequest {
                credential: public.key_data().clone().into(),
                data: b"h4-receipt-cleartext-must-not-project".to_vec(),
                flags: 0,
            })
            .await
    });

    let pending = loop {
        let snapshot = host.snapshot()?;
        if let Some(request_id) = snapshot
            .pending_signing
            .first()
            .map(|pending| pending.request.request_id)
        {
            break (snapshot, request_id);
        }
        tokio::task::yield_now().await;
    };
    let pending_json = pending.0.to_public_json()?;
    let html = render_identity_surface(&pending.0);
    assert!(!pending_json.contains("h4-receipt-cleartext-must-not-project"));
    assert!(!html.contains("h4-receipt-cleartext-must-not-project"));
    assert!(html.contains("Approve once"));
    assert!(html.contains("Deny"));
    assert!(html.contains("standalone agent retained"));

    let payload = serde_json::to_vec(&SigningDecisionIntentV1 {
        request_id: pending.1,
    })?;
    host.apply_intent(SIGNING_APPROVE_ONCE_INTENT, &payload)?;
    let signature = signing.await??;
    let completed = host.snapshot()?;
    assert!(completed.pending_signing.is_empty());
    assert_eq!(completed.signing_history.len(), 1);

    let html_path = output_root.join("h4_identity_surface.html");
    let json_path = output_root.join("h4_identity_receipt.json");
    std::fs::write(&html_path, html)?;
    std::fs::write(
        &json_path,
        serde_json::to_vec_pretty(&json!({
            "schema": "graphshell.h4a.identity-receipt/v1",
            "pending": {
                "request_id": pending.1,
                "operation": pending.0.pending_signing[0].request.operation,
                "payload_digest": pending.0.pending_signing[0].request.payload_digest,
                "cleartext_payload_absent": true,
                "private_material_absent": true
            },
            "decision": {
                "intent": SIGNING_APPROVE_ONCE_INTENT,
                "signature_bytes": signature.as_bytes().len(),
                "history_records": completed.signing_history.len()
            },
            "cutover": {
                "standard_endpoint_changed": false,
                "standalone_agent_retained": true
            }
        }))?,
    )?;
    println!("{}", html_path.display());
    println!("{}", json_path.display());
    Ok(())
}
