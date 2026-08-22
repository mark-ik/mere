#![cfg(windows)]

use knot_editor::{StartupUnlockedPersonalVault, local_device_root, personal_vault_writer};
use pandect::{DeviceSettings, save_device_settings, wallet_store};
use personae::PersonaId;
use tempfile::tempdir;

#[test]
fn second_owner_is_refused_while_pairing_facts_remain_available() {
    let root = tempdir().unwrap();
    let persona = PersonaId::new();
    save_device_settings(
        root.path(),
        &DeviceSettings {
            startup_unlock_mode: personae::StartupUnlockMode::AutoOs,
        },
    )
    .unwrap();
    wallet_store::ensure_wallet_state(root.path(), persona, "Knot resident receipt").unwrap();
    let device = local_device_root(root.path(), "Knot resident receipt").unwrap();

    let owner = StartupUnlockedPersonalVault::open(root.path(), persona, device, []).unwrap();
    let duplicate = match StartupUnlockedPersonalVault::open(root.path(), persona, device, []) {
        Ok(_) => panic!("a second persona owner must be refused promptly"),
        Err(error) => error,
    };
    assert!(
        duplicate.contains("another resident may already own this persona"),
        "{duplicate}"
    );
    assert_eq!(
        personal_vault_writer(root.path(), persona, device).unwrap(),
        owner.writer(),
        "pairing facts must not reopen the resident-owned Knot stores",
    );

    drop(owner);
    drop(StartupUnlockedPersonalVault::open(root.path(), persona, device, []).unwrap());
}
