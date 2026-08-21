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
