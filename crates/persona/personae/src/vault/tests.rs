use std::path::PathBuf;

use super::*;

fn make_profile(seed: u8, name: &str) -> Profile {
    Profile::new(
        ProfileId(format!("p-{seed}")),
        name,
        Ed25519Keypair::from_seed([seed; 32]),
    )
}

fn nostr_slot() -> IdentitySlot {
    IdentitySlot::Direct {
        kind: "nostr".to_string(),
        payload: SecretBytes::new(vec![0x42; 32]),
        lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
        unlock_tier: UnlockTier::Session,
    }
}

fn matrix_slot(state_dir: PathBuf) -> IdentitySlot {
    IdentitySlot::Bootstrap {
        kind: "matrix".to_string(),
        bootstrap: SecretBytes::new(b"login-flow-seed".to_vec()),
        state_dir,
        lineage: CredentialLineage::LocallyGeneratedExternallyRegistered,
        unlock_tier: UnlockTier::Session,
    }
}

#[test]
fn vault_provides_master_public_key_and_derives() {
    let storage = InMemoryStorage::new();
    let profile = make_profile(7, "personal");
    let expected_pk = profile.master.public_key();
    let vault = IdentityVault::with_profile(storage, profile);

    assert_eq!(vault.master_public_key().to_bytes(), expected_pk.to_bytes());

    let derived = vault.derive_keypair(b"some-cabal-salt").unwrap();
    // Same vault + same salt → same derived public key (determinism).
    let again = vault.derive_keypair(b"some-cabal-salt").unwrap();
    assert_eq!(
        derived.public_key().to_bytes(),
        again.public_key().to_bytes()
    );
}

#[test]
fn add_and_retrieve_slot_round_trip() {
    let storage = InMemoryStorage::new();
    let profile = make_profile(11, "work");
    let mut vault = IdentityVault::with_profile(storage, profile);

    let key = ProtocolKey::new("nostr", None);
    vault.add_slot(key.clone(), nostr_slot()).unwrap();

    let slot = vault.slot(&key).expect("slot present");
    assert_eq!(slot.kind(), "nostr");
    assert_eq!(
        slot.lineage(),
        CredentialLineage::LocallyGeneratedExternallyRegistered
    );
    assert_eq!(slot.unlock_tier(), UnlockTier::Session);
    match slot {
        IdentitySlot::Direct { payload, .. } => {
            assert_eq!(payload.as_slice(), &[0x42; 32]);
        }
        _ => panic!("expected Direct"),
    }
}

#[test]
fn bootstrap_slot_carries_state_dir() {
    let storage = InMemoryStorage::new();
    let profile = make_profile(13, "alt");
    let mut vault = IdentityVault::with_profile(storage, profile);

    let dir = PathBuf::from("/tmp/matrix-state");
    let key = ProtocolKey::new("matrix", Some("homeserver-1".into()));
    vault
        .add_slot(key.clone(), matrix_slot(dir.clone()))
        .unwrap();

    match vault.slot(&key).expect("present") {
        IdentitySlot::Bootstrap {
            kind,
            bootstrap,
            state_dir,
            ..
        } => {
            assert_eq!(kind, "matrix");
            assert_eq!(bootstrap.as_slice(), b"login-flow-seed");
            assert_eq!(state_dir, &dir);
        }
        _ => panic!("expected Bootstrap"),
    }
}

#[test]
fn save_then_load_round_trips_profile_and_slots() {
    let storage = InMemoryStorage::new();
    let id = ProfileId("p-42".into());
    let mut profile = Profile::new(id.clone(), "loaded", Ed25519Keypair::from_seed([42; 32]));
    profile
        .slots
        .insert(ProtocolKey::new("nostr", None), nostr_slot());
    profile.slots.insert(
        ProtocolKey::new("matrix", None),
        matrix_slot(PathBuf::from("/tmp/m")),
    );

    storage.save_profile(&profile).unwrap();

    let loaded = storage.load_profile(&id).unwrap();
    assert_eq!(loaded.display_name, "loaded");
    assert_eq!(
        loaded.master.public_key().to_bytes(),
        profile.master.public_key().to_bytes()
    );
    assert_eq!(loaded.slots.len(), 2);
    assert!(loaded.has_slot(&ProtocolKey::new("nostr", None)));
    assert!(loaded.has_slot(&ProtocolKey::new("matrix", None)));
}

#[test]
fn remove_slot_returns_true_when_present_and_persists() {
    let storage = InMemoryStorage::new();
    let profile = make_profile(5, "x");
    let mut vault = IdentityVault::with_profile(storage.clone(), profile);

    let key = ProtocolKey::new("nostr", None);
    vault.add_slot(key.clone(), nostr_slot()).unwrap();
    assert!(vault.remove_slot(&key).unwrap());
    assert!(!vault.remove_slot(&key).unwrap());

    // Storage reflects the removal too.
    let summary = storage.list_profiles().unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].slot_count, 0);
}

#[test]
fn list_profiles_summarizes_each() {
    let storage = InMemoryStorage::new();
    let mut a = make_profile(1, "alpha");
    a.slots
        .insert(ProtocolKey::new("nostr", None), nostr_slot());
    let b = make_profile(2, "beta");

    storage.save_profile(&a).unwrap();
    storage.save_profile(&b).unwrap();

    let mut summaries = storage.list_profiles().unwrap();
    summaries.sort_by(|x, y| x.id.cmp(&y.id));

    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].display_name, "alpha");
    assert_eq!(summaries[0].slot_count, 1);
    assert_eq!(summaries[1].display_name, "beta");
    assert_eq!(summaries[1].slot_count, 0);
}

#[test]
fn switching_profiles_is_live_and_a_failed_switch_changes_nothing() {
    let storage = InMemoryStorage::new();
    let work = make_profile(1, "work");
    let personal = make_profile(2, "personal");
    storage.save_profile(&work).unwrap();
    storage.save_profile(&personal).unwrap();

    let mut vault = IdentityVault::open(&storage, &work.id).unwrap();
    let before = vault.master_public_key().to_bytes();

    vault.switch_profile(&personal.id).unwrap();
    assert_eq!(vault.current_profile().id, personal.id);
    assert_ne!(
        vault.master_public_key().to_bytes(),
        before,
        "everything reading this vault now speaks as the new persona"
    );

    // A profile that will not load leaves the current one untouched rather
    // than half-switched.
    assert!(vault.switch_profile(&ProfileId("absent".into())).is_err());
    assert_eq!(vault.current_profile().id, personal.id);
}

#[test]
fn missing_profile_returns_backend_error() {
    // Profile doesn't impl Debug (master keypair is secret material),
    // so we match on the Result rather than expect_err().
    let storage = InMemoryStorage::new();
    match storage.load_profile(&ProfileId("nope".into())) {
        Err(IdentityError::Backend(_)) => {}
        Err(other) => panic!("unexpected error variant: {other}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}
