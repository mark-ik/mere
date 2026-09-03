// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::resident::CastellanResident;
use tempfile::tempdir;

fn store(root: &std::path::Path, persona: PersonaId) -> SecretServiceStore {
    let resident = CastellanResident::claim(
        root.join("records"),
        [0x91; 32],
        root.join("freshness"),
        [0x92; 32],
    )
    .unwrap();
    resident.secret_service(persona, SecretServiceLimits::default())
}

fn attributes() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("application".into(), "turnstone".into()),
        ("account".into(), "mark".into()),
    ])
}

#[test]
fn default_collection_stores_searches_replaces_and_deletes_a_secret() {
    let dir = tempdir().unwrap();
    let store = store(dir.path(), PersonaId::new());
    let collection = store.ensure_default_collection("Castellan", 10).unwrap();
    let item = store
        .create_item(NewSecretItem {
            collection: collection.id,
            label: "Turnstone".into(),
            attributes: attributes(),
            secret: b"first".to_vec(),
            content_type: "text/plain; charset=utf8".into(),
            replace: false,
            unix_secs: 11,
        })
        .unwrap();

    assert_eq!(store.search(&attributes()).unwrap(), vec![item.clone()]);
    assert_eq!(store.limits(), SecretServiceLimits::default());
    assert_eq!(
        store.aliases().unwrap().get("default"),
        Some(&collection.id)
    );
    let first = store.secret(item.id).unwrap();
    assert_eq!(first.bytes.as_slice(), b"first");
    assert_eq!(first.content_type, "text/plain; charset=utf8");
    store
        .set_secret(item.id, b"intermediate".to_vec(), "text/plain", 12)
        .unwrap();
    assert_eq!(
        store.secret(item.id).unwrap().bytes.as_slice(),
        b"intermediate"
    );

    let replaced = store
        .create_item(NewSecretItem {
            collection: collection.id,
            label: "Turnstone login".into(),
            attributes: attributes(),
            secret: b"second".to_vec(),
            content_type: "text/plain; charset=utf8".into(),
            replace: true,
            unix_secs: 13,
        })
        .unwrap();
    assert_eq!(replaced.id, item.id);
    assert_eq!(store.items(collection.id).unwrap().len(), 1);
    assert_eq!(store.secret(item.id).unwrap().bytes.as_slice(), b"second");

    store.delete_item(item.id).unwrap();
    assert!(matches!(
        store.item(item.id),
        Err(SecretServiceError::ItemNotFound(id)) if id == item.id
    ));
}

#[test]
fn persona_namespaces_and_plaintext_storage_stay_separate() {
    let dir = tempdir().unwrap();
    let owner = store(dir.path(), PersonaId::new());
    let collection = owner.ensure_default_collection("Castellan", 10).unwrap();
    owner
        .create_item(NewSecretItem {
            collection: collection.id,
            label: "Private".into(),
            attributes: attributes(),
            secret: b"not-on-disk-in-clear".to_vec(),
            content_type: "text/plain".into(),
            replace: false,
            unix_secs: 11,
        })
        .unwrap();
    drop(owner);

    let bytes = std::fs::read_dir(dir.path().join("records"))
        .unwrap()
        .flat_map(|entry| walk(entry.unwrap().path()))
        .flat_map(|path| std::fs::read(path).unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(!String::from_utf8_lossy(&bytes).contains("not-on-disk-in-clear"));

    let other = store(dir.path(), PersonaId::new());
    assert!(other.collections().unwrap().is_empty());
}

#[test]
fn aliases_must_be_valid_dbus_object_path_elements() {
    let dir = tempdir().unwrap();
    let store = store(dir.path(), PersonaId::new());

    assert!(matches!(
        store.create_collection("Invalid alias", Some("not/a/path"), 10),
        Err(SecretServiceError::InvalidText("collection alias"))
    ));
}

fn walk(path: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    if path.is_file() {
        return vec![path];
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|entry| walk(entry.path()))
        .collect()
}
