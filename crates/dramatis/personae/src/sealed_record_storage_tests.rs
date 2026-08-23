use super::*;
use tempfile::tempdir;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SampleRecord {
    label: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CounterRecord {
    value: u64,
}

#[test]
fn round_trips_a_typed_record() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x11; 32]);
    let value = SampleRecord {
        label: "Pocket Meerkat".into(),
        bytes: vec![1, 2, 3, 4],
    };

    store
        .save_record("identity/test-record.json", &value)
        .unwrap();
    let restored = store
        .load_record::<SampleRecord>("identity/test-record.json")
        .unwrap()
        .unwrap();
    assert_eq!(restored, value);
}

#[test]
fn ciphertext_is_bound_to_the_record_path() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x22; 32]);
    let value = SampleRecord {
        label: "Studio PC".into(),
        bytes: vec![9, 8, 7],
    };

    store.save_record("identity/source.json", &value).unwrap();
    let source = dir.path().join("identity/source.json");
    let copied = dir.path().join("identity/copied.json");
    std::fs::create_dir_all(copied.parent().unwrap()).unwrap();
    std::fs::copy(&source, &copied).unwrap();

    let err = store
        .load_record::<SampleRecord>("identity/copied.json")
        .unwrap_err();
    assert!(
        err.to_string().contains("decrypt sealed record"),
        "unexpected error: {err}"
    );
}

#[test]
fn stored_file_is_not_plaintext_json_of_the_record() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x33; 32]);
    let value = SampleRecord {
        label: "Tablet".into(),
        bytes: vec![0xaa; 32],
    };

    store
        .save_record("identity/plaintext-check.json", &value)
        .unwrap();
    let bytes = std::fs::read(dir.path().join("identity/plaintext-check.json")).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("Tablet"));
    assert!(!text.contains("\"label\":\"Tablet\""));
}

#[test]
fn cloned_store_updates_are_one_load_modify_replace_transaction() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x44; 32]);
    store
        .save_record("counter/value.json", &CounterRecord { value: 0 })
        .unwrap();

    let increment = |store: SealedRecordStorage| {
        std::thread::spawn(move || {
            for _ in 0..50 {
                store
                    .update_record(
                        "counter/value.json",
                        |current: Option<CounterRecord>| -> Result<_, IdentityError> {
                            let mut current = current.expect("counter exists");
                            current.value += 1;
                            Ok(((), SealedRecordChange::Replace(current)))
                        },
                    )
                    .unwrap();
            }
        })
    };
    let left = increment(store.clone());
    let right = increment(store.clone());
    left.join().unwrap();
    right.join().unwrap();

    let restored = store
        .load_record::<CounterRecord>("counter/value.json")
        .unwrap()
        .unwrap();
    assert_eq!(restored.value, 100);
}

fn authoritative_store(root: &Path, freshness: &Path) -> SealedRecordStorage {
    SealedRecordStorage::claim_with_file_freshness(root, [0x55; 32], freshness, [0x56; 32]).unwrap()
}

#[test]
fn authoritative_opening_is_exclusive_until_every_clone_drops() {
    let dir = tempdir().unwrap();
    let records = dir.path().join("records");
    let freshness = dir.path().join("freshness");
    let first = authoritative_store(&records, &freshness);
    let retained = first.clone();

    let error = SealedRecordStorage::claim_with_file_freshness(
        &records, [0x55; 32], &freshness, [0x56; 32],
    )
    .err()
    .expect("a second authority must be refused");
    assert!(error.to_string().contains("already held"));

    drop(first);
    assert!(
        SealedRecordStorage::claim_with_file_freshness(
            &records, [0x55; 32], &freshness, [0x56; 32],
        )
        .is_err()
    );
    drop(retained);
    authoritative_store(&records, &freshness);
}

#[test]
fn authority_child_probe() {
    if std::env::var_os("PERSONAE_AUTHORITY_PROBE").is_none() {
        return;
    }
    let records = PathBuf::from(std::env::var_os("PERSONAE_AUTHORITY_RECORDS").unwrap());
    let freshness = PathBuf::from(std::env::var_os("PERSONAE_AUTHORITY_FRESHNESS").unwrap());
    let result =
        SealedRecordStorage::claim_with_file_freshness(records, [0x55; 32], freshness, [0x56; 32]);
    match std::env::var("PERSONAE_AUTHORITY_EXPECT").unwrap().as_str() {
        "blocked" => assert!(
            result
                .err()
                .expect("second process must be blocked")
                .to_string()
                .contains("already held")
        ),
        "open" => drop(result.unwrap()),
        expectation => panic!("unsupported authority probe expectation {expectation:?}"),
    }
}

#[test]
fn authoritative_opening_is_exclusive_across_processes() {
    let dir = tempdir().unwrap();
    let records = dir.path().join("records");
    let freshness = dir.path().join("freshness");
    let authority = authoritative_store(&records, &freshness);

    let probe = |expectation: &str| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sealed_record_storage::tests::authority_child_probe",
                "--nocapture",
            ])
            .env("PERSONAE_AUTHORITY_PROBE", "1")
            .env("PERSONAE_AUTHORITY_RECORDS", &records)
            .env("PERSONAE_AUTHORITY_FRESHNESS", &freshness)
            .env("PERSONAE_AUTHORITY_EXPECT", expectation)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "authority child probe failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };

    probe("blocked");
    drop(authority);
    probe("open");
}

#[test]
fn external_freshness_evidence_rejects_an_older_valid_record() {
    let dir = tempdir().unwrap();
    let records = dir.path().join("records");
    let freshness = dir.path().join("freshness");
    let store = authoritative_store(&records, &freshness);
    let relative = "counter/value.json";
    let path = records.join(relative);
    store
        .save_record(relative, &CounterRecord { value: 1 })
        .unwrap();
    let older_valid_record = std::fs::read(&path).unwrap();
    store
        .save_record(relative, &CounterRecord { value: 2 })
        .unwrap();

    std::fs::write(&path, older_valid_record).unwrap();

    let error = store.load_record::<CounterRecord>(relative).unwrap_err();
    assert!(error.to_string().contains("rollback detected"));
}

#[test]
fn unauthenticated_legacy_record_does_not_establish_a_freshness_baseline() {
    let dir = tempdir().unwrap();
    let records = dir.path().join("records");
    let freshness = dir.path().join("freshness");
    let store = authoritative_store(&records, &freshness);
    let relative = Path::new("identity/forged-legacy.json");
    let (path, aad) = resolve_record_path(&records.canonicalize().unwrap(), relative).unwrap();
    save_json_atomic(
        &path,
        &SealedRecordEnvelope {
            version: LEGACY_SEALED_RECORD_FORMAT_VERSION,
            generation: 0,
            deleted: false,
            nonce: vec![0; NONCE_LEN],
            ciphertext: vec![0; 16],
        },
    )
    .unwrap();

    let error = store.load_record::<SampleRecord>(relative).unwrap_err();
    assert!(error.to_string().contains("decrypt sealed record"));
    let ledger = freshness
        .join("records")
        .join(format!("{}.json", blake3::hash(aad.as_bytes()).to_hex()));
    assert!(!ledger.exists());
}

#[test]
fn authoritative_tombstone_rejects_resurrection_after_delete() {
    let dir = tempdir().unwrap();
    let records = dir.path().join("records");
    let freshness = dir.path().join("freshness");
    let store = authoritative_store(&records, &freshness);
    let relative = "identity/deleted.json";
    let path = records.join(relative);
    store
        .save_record(
            relative,
            &SampleRecord {
                label: "Retired".into(),
                bytes: vec![7; 16],
            },
        )
        .unwrap();
    let live_record = std::fs::read(&path).unwrap();

    store.delete_record(relative).unwrap();
    assert_eq!(store.load_record::<SampleRecord>(relative).unwrap(), None);
    std::fs::write(&path, live_record).unwrap();

    let error = store.load_record::<SampleRecord>(relative).unwrap_err();
    assert!(error.to_string().contains("rollback detected"));
}

/// A record written before sealing existed is plain JSON on disk. Saving over
/// it has to work, or the migration that seals it can never run: the save reads
/// the current revision first, and that read used to reject anything that was
/// not already an envelope.
#[test]
fn a_pre_sealing_plaintext_record_can_be_sealed_over() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x44; 32]);
    let path = dir.path().join("identity/legacy.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = SampleRecord {
        label: "Tablet".into(),
        bytes: vec![4, 5, 6],
    };
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    store.save_record("identity/legacy.json", &legacy).unwrap();

    let restored = store
        .load_record::<SampleRecord>("identity/legacy.json")
        .unwrap()
        .unwrap();
    assert_eq!(restored, legacy);
    // And what is on disk is now sealed, not the plaintext it started as.
    let sealed = std::fs::read_to_string(&path).unwrap();
    assert!(!sealed.contains("Tablet"), "the label is still in the clear");
}

/// A seed written before sealing is not JSON at all. It must take the same path
/// as plaintext JSON rather than being mistaken for a damaged envelope.
#[test]
fn a_pre_sealing_raw_record_can_be_sealed_over() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x55; 32]);
    let path = dir.path().join("identity/master.seed");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, [0xde, 0xad, 0xbe, 0xef]).unwrap();

    let value = SampleRecord {
        label: "Seed".into(),
        bytes: vec![0xde, 0xad],
    };
    store.save_record("identity/master.seed", &value).unwrap();
    assert!(
        store
            .load_record::<SampleRecord>("identity/master.seed")
            .unwrap()
            .is_some()
    );
}

/// The other half of that leniency, and the reason it is shaped as it is: a
/// sealed record that is damaged rather than absent must still refuse to be
/// written over. Treating it as "nothing is here" would let a rollback replace
/// a real record unnoticed, which is precisely what the freshness ledger
/// exists to catch.
#[test]
fn a_damaged_sealed_record_is_not_mistaken_for_a_pre_sealing_one() {
    let dir = tempdir().unwrap();
    let store = SealedRecordStorage::open_with_key(dir.path(), [0x66; 32]);
    let value = SampleRecord {
        label: "Real".into(),
        bytes: vec![1],
    };
    store.save_record("identity/real.json", &value).unwrap();

    // Envelope-shaped — version, nonce, ciphertext all present — but the
    // version is the wrong type, so it parses as a value and not as an
    // envelope. Exactly the shape a truncated or tampered record takes.
    let path = dir.path().join("identity/real.json");
    std::fs::write(
        &path,
        br#"{"version":"two","nonce":[1,2,3],"ciphertext":[4,5,6]}"#,
    )
    .unwrap();

    let error = store
        .save_record("identity/real.json", &value)
        .expect_err("a damaged envelope must not be silently replaced");
    assert!(
        format!("{error}").contains("parse sealed record"),
        "unexpected error: {error}"
    );
}
