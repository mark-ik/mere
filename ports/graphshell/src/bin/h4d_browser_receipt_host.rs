//! Headed-browser approval receipt host.
//!
//! This binary is not an installer target. It uses the product native-message
//! runner with an in-memory Personae vault and one real pending `PerUse` SSH
//! signature, so browser automation can prove the visible approval gesture.

use std::sync::Arc;
use std::time::Duration;

use graphshell::browser_carrier::{AllowedExtensions, BrowserLauncher};
use graphshell::identity::VaultProtectionView;
use graphshell::native::browser_host::serve_identity_native_messages;
use graphshell::native::identity_ui::UnavailableNativeIdentityUi;
use graphshell::native::personae_host::PersonaeHost;
use personae::ssh_slot::{protocol_key_for, slot_for};
use personae::{
    Ed25519Keypair, IdentityVault, InMemoryProvider, InMemoryStorage, Profile, ProfileId,
    UnlockTier,
};
use signature::Verifier;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::proto::SignRequest;
use ssh_key::Algorithm;

const SIGNED_PAYLOAD: &[u8] = b"graphshell-h4d-headed-browser-approval";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("graphshell H4d receipt host: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let launcher = BrowserLauncher::parse(&args)?;
    AllowedExtensions::default().admit(&launcher)?;

    let identity = InMemoryProvider::from_seed([0x51; 32]);
    let mut private = ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)?;
    private.set_comment("H4d headed approval receipt");
    let public = ssh_key::PublicKey::from(&private);
    let mut profile = Profile::new(
        ProfileId("receipt".to_string()),
        "H4d receipt",
        Ed25519Keypair::from_seed([0x51; 32]),
    );
    profile.slots.insert(
        protocol_key_for(&private),
        slot_for(&private, UnlockTier::PerUse)?,
    );
    let host = Arc::new(PersonaeHost::with_decision_timeout(
        IdentityVault::with_profile(InMemoryStorage::new(), profile),
        None,
        VaultProtectionView::Ephemeral,
        Duration::from_secs(60),
    ));
    let mut agent = host.agent_session();
    let credential = public.key_data().clone().into();
    let signing = tokio::spawn(async move {
        agent
            .sign(SignRequest {
                credential,
                data: SIGNED_PAYLOAD.to_vec(),
                flags: 0,
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if !host
                .snapshot()
                .expect("snapshot")
                .pending_signing
                .is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;

    let mut reader = tokio::io::stdin();
    let mut writer = tokio::io::stdout();
    let summary = serve_identity_native_messages(
        &identity,
        Arc::clone(&host),
        &UnavailableNativeIdentityUi,
        launcher,
        &mut reader,
        &mut writer,
        10 * 60 * 1_000,
    )
    .await?
    .ok_or("browser disconnected before admission")?;
    let signature = signing.await??;
    public.key_data().verify(SIGNED_PAYLOAD, &signature)?;
    let snapshot = host.snapshot()?;
    if !snapshot.pending_signing.is_empty() || snapshot.signing_history.len() != 1 {
        return Err("headed approval did not leave one completed signing record".into());
    }
    eprintln!(
        "graphshell H4d receipt host: verified browser approval; served {} request(s); ended {:?}",
        summary.answered, summary.end
    );
    Ok(())
}
