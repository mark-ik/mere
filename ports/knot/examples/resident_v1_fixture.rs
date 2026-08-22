//! Seed the isolated on-disk state used by the headed resident V1 receipt.
//!
//! This is a receipt fixture, not an alternate authoring path. The document is
//! authored through `StartupUnlockedPersonalVault`, the same signed operation
//! store the resident opens, and Graphshell's ordinary owner settings select
//! that persona for the first-party `knot` route.

use std::path::PathBuf;

use graphshell::native::owner_settings::{KnotResidentSettings, OwnerSettings, settings_path};
use knot::{StartupUnlockedPersonalVault, VaultDocument, local_device_root};
use personae::{PersonaId, ProfileId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let app_dir = PathBuf::from(args.next().ok_or(
        "usage: resident_v1_fixture <graphshell-app-dir> <data-root> <profile> <persona-uuid>",
    )?);
    let data_root = PathBuf::from(args.next().ok_or("missing data root")?);
    let profile = ProfileId(
        args.next()
            .ok_or("missing profile")?
            .into_string()
            .map_err(|_| "profile is not Unicode")?,
    );
    let persona = args
        .next()
        .ok_or("missing persona UUID")?
        .into_string()
        .map_err(|_| "persona UUID is not Unicode")?
        .parse::<uuid::Uuid>()
        .map(PersonaId::from_uuid)?;
    if args.next().is_some() {
        return Err("resident_v1_fixture received unexpected arguments".into());
    }

    pandect::wallet_store::ensure_wallet_state(&data_root, persona, "resident-v1")?;
    let device_root = local_device_root(&data_root, "resident-v1")?;
    let authority = StartupUnlockedPersonalVault::open(&data_root, persona, device_root, [])?;
    authority.author_document(VaultDocument {
        id: "field-note".into(),
        title: "Resident V1".into(),
        body: b"# Resident V1\n".to_vec(),
        media_type: "text/djot".into(),
    })?;
    drop(authority);

    OwnerSettings {
        knot: Some(KnotResidentSettings {
            persona: persona.as_uuid().to_string(),
            device_label: "resident-v1".into(),
            ..KnotResidentSettings::default()
        }),
        ..OwnerSettings::default()
    }
    .save(&settings_path(&app_dir, &profile))?;

    println!(
        "seeded knot://vault/field-note for persona {}",
        persona.as_uuid()
    );
    Ok(())
}
