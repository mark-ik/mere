//! Receipt: switching personas is live, remembered, and visible.
//!
//! Three claims were made when the switch intent landed and none had been
//! observed, only unit-tested:
//!
//! 1. **Live.** The resident host and its SSH agent share one
//!    `Arc<Mutex<IdentityVault>>`, so a switch takes effect on the next
//!    operation of everything reading the vault, with no process told to die.
//!    The strong form of that is an agent session held *across* the switch: it
//!    was opened against one persona and must serve the other's keys without
//!    being reconnected.
//! 2. **Remembered.** The choice is written beside the vault, so the rest of
//!    the family opens the same persona next time.
//! 3. **Visible.** The projection offers the switch on every persona except
//!    the one in use, and the selected marker moves once it is applied.
//!
//! Run against a scratch vault directory:
//!
//! ```text
//! cargo run -p graphshell --bin persona_switch_receipt -- <output-dir>
//! ```
//!
//! The vault is created under the output directory and unlocked by a fixed
//! passphrase, deliberately: `Unlock::AutoOs` exists on Windows only, and a
//! receipt that silently skipped elsewhere would print "passed" with no
//! evidence behind it. Nothing here touches the machine's own vault.

use std::path::PathBuf;

use graphshell::identity::VaultProtectionView;
use graphshell::identity_projection::{
    PROFILE_SWITCH_INTENT, SwitchProfileIntentV1, project_identity, render_identity_surface,
};
use graphshell::native::personae_host::{IdentityIntentOutcome, PersonaeHost};
use personae::bootstrap::{self, Unlock};
use personae::roster;
use personae::ssh_slot::{protocol_key_for, slot_for};
use personae::vault::IdentityStorage;
use personae::{Ed25519Keypair, IdentityVault, Profile, ProfileId, UnlockTier};
use serde_json::json;
use ssh_agent_lib::agent::Session;
use ssh_key::Algorithm;

const PASSPHRASE: &[u8] = b"graphshell-persona-switch-receipt";

/// One persona plus an SSH slot only it holds, so "whose keys is the agent
/// serving" has an unambiguous answer.
fn persona(id: &str, name: &str, seed: u8) -> Result<(Profile, String), Box<dyn std::error::Error>> {
    let mut private = ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)?;
    private.set_comment(format!("{name} key"));
    let mut profile = Profile::new(
        ProfileId(id.to_string()),
        name,
        Ed25519Keypair::from_seed([seed; 32]),
    );
    let key = protocol_key_for(&private);
    let fingerprint = key.instance.clone().unwrap_or_default();
    profile
        .slots
        .insert(key, slot_for(&private, UnlockTier::Session)?);
    Ok((profile, fingerprint))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ports/graphshell/docs/receipts"));
    std::fs::create_dir_all(&out)?;
    let vault_dir = out.join("vault");
    let _ = std::fs::remove_dir_all(&vault_dir);

    let unlock = || Unlock::Passphrase(PASSPHRASE.to_vec().into());
    let opened = bootstrap::open_storage(&vault_dir, unlock())?;
    let (work, work_key) = persona("work", "Work", 0x11)?;
    let (alt, alt_key) = persona("alt", "Late Night Alt", 0x12)?;
    opened.storage.save_profile(&work)?;
    opened.storage.save_profile(&alt)?;

    // The host opens on `work`, exactly as an application would.
    let host = PersonaeHost::new(
        IdentityVault::open(opened.storage, &ProfileId("work".into()))?,
        None,
        VaultProtectionView::Passphrase,
    )
    .with_vault_dir(vault_dir.clone());

    // ── Claim 3: the projection offers the switch, and only where it means
    // something. The persona in use has nothing to become.
    let before = host.snapshot()?;
    let cards = project_identity(&before);
    let offered: Vec<&str> = cards
        .iter()
        .filter(|card| {
            card.actions
                .iter()
                .any(|action| action.intent == PROFILE_SWITCH_INTENT)
        })
        .map(|card| card.key.as_str())
        .collect();
    assert_eq!(
        offered,
        ["identity:profile:alt"],
        "the switch is offered on the other persona and not on the one in use"
    );
    let surface_before = render_identity_surface(&before);
    assert!(surface_before.contains("Speak as this persona"));
    std::fs::write(out.join("01_before_switch.html"), &surface_before)?;

    // ── Claim 1, set up: an agent session opened against `work`, held across
    // the switch. This is the same handle a connected SSH client would be
    // sitting on.
    let mut session = host.agent_session();
    let listed_before: Vec<String> = session
        .request_identities()
        .await?
        .into_iter()
        .map(|identity| identity.comment)
        .collect();
    assert!(
        listed_before.iter().any(|c| c.contains("Work")),
        "the agent starts on the persona the host opened: {listed_before:?}"
    );
    assert!(
        !listed_before.iter().any(|c| c.contains("Late Night Alt")),
        "and does not already serve the other persona: {listed_before:?}"
    );

    // ── The switch itself, through the projected intent rather than a private
    // method, so what is exercised is what the UI actually invokes.
    let outcome = host.apply_intent(
        PROFILE_SWITCH_INTENT,
        &serde_json::to_vec(&SwitchProfileIntentV1 {
            profile: "alt".to_string(),
        })?,
    )?;
    let IdentityIntentOutcome::ProfileSwitch(receipt) = outcome else {
        return Err("a switch intent returned some other outcome".into());
    };
    assert_eq!(receipt.profile, "alt");
    assert!(receipt.remembered, "an on-disk vault records the choice");

    // ── Claim 1, proved: the SAME session, never reconnected, now serves the
    // other persona's keys.
    let listed_after: Vec<String> = session
        .request_identities()
        .await?
        .into_iter()
        .map(|identity| identity.comment)
        .collect();
    assert!(
        listed_after.iter().any(|c| c.contains("Late Night Alt")),
        "the held session follows the switch without a restart: {listed_after:?}"
    );
    assert!(
        !listed_after.iter().any(|c| c.contains("Work")),
        "and stops serving the persona it was opened on: {listed_after:?}"
    );

    // ── Claim 2: the choice is beside the vault, where the rest of the family
    // reads it.
    let remembered = roster::chosen_profile(&vault_dir);
    assert_eq!(
        remembered,
        Some(ProfileId("alt".into())),
        "the family's remembered choice names the new persona"
    );

    // ── Claim 3, after: the selected marker moved, and the switch is now
    // offered on the persona that used to hold it.
    let after = host.snapshot()?;
    let selected: Vec<&str> = after
        .profiles
        .iter()
        .filter(|profile| profile.selected)
        .map(|profile| profile.id.as_str())
        .collect();
    assert_eq!(selected, ["alt"], "the marker follows the switch");
    let cards_after = project_identity(&after);
    let offered_after: Vec<&str> = cards_after
        .iter()
        .filter(|card| {
            card.actions
                .iter()
                .any(|action| action.intent == PROFILE_SWITCH_INTENT)
        })
        .map(|card| card.key.as_str())
        .collect();
    assert_eq!(offered_after, ["identity:profile:work"]);
    std::fs::write(
        out.join("02_after_switch.html"),
        render_identity_surface(&after),
    )?;

    // A switch to a persona that is not there must not move anything.
    let missing = host.apply_intent(
        PROFILE_SWITCH_INTENT,
        &serde_json::to_vec(&SwitchProfileIntentV1 {
            profile: "nobody".to_string(),
        })?,
    );
    assert!(missing.is_err(), "an absent persona is refused");
    assert_eq!(
        host.snapshot()?
            .profiles
            .iter()
            .filter(|p| p.selected)
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["alt"],
        "and a refused switch leaves the host where it was"
    );

    let receipt = json!({
        "claim_live": {
            "session_reconnected": false,
            "served_before": listed_before,
            "served_after": listed_after,
            "work_key": work_key,
            "alt_key": alt_key,
        },
        "claim_remembered": {
            "choice_file": roster::choice_path(&vault_dir).display().to_string(),
            "names": remembered.map(|id| id.0),
        },
        "claim_visible": {
            "offered_before": offered,
            "offered_after": offered_after,
            "selected_after": selected,
        },
        "absent_persona_refused": true,
        "vault_protection": "passphrase (portable, so this runs on every platform)",
    });
    let path = out.join("persona_switch.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&receipt)?)?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    println!("receipt: {}", path.display());
    Ok(())
}
