use std::ffi::OsStr;
use std::fs;

use graphshell::client::ResolvedContent;
use graphshell::sessions::spawn_endpoint_session;
use chirograph::{
    CapabilityProfile, CarrierRequestBody, CarrierResponseBody, IntentResult,
    PresentationCapability, ResumeReply, ResumeRequest, SaveTextV1,
};
use graphshell_stdio::StdioCarrier;
use tempfile::tempdir;

#[test]
fn a_real_knot_process_rings_and_resumes_after_a_disk_edit() {
    let root = tempdir().unwrap();
    let path = root.path().join("field.knot");
    fs::write(&path, "one").unwrap();
    let mut carrier = StdioCarrier::spawn(
        env!("CARGO_BIN_EXE_knot_endpoint"),
        [root.path().as_os_str()],
    )
    .unwrap();

    let descriptor = match carrier.request(CarrierRequestBody::Discover).unwrap() {
        CarrierResponseBody::Descriptor(descriptor) => descriptor,
        other => panic!("expected descriptor, got {other:?}"),
    };
    let request = descriptor.projections[0].request.clone();
    let snapshot = match carrier
        .request(CarrierRequestBody::Snapshot(request.clone()))
        .unwrap()
    {
        CarrierResponseBody::Snapshot(snapshot) => snapshot,
        other => panic!("expected snapshot, got {other:?}"),
    };

    fs::write(&path, "one two three").unwrap();
    let notice = carrier.wait_for_notice().unwrap();
    assert_eq!(notice.session, snapshot.session);
    assert_eq!(notice.epoch, snapshot.scene.epoch);
    assert!(notice.revision > snapshot.scene.revision);

    let reply = match carrier
        .request(CarrierRequestBody::Resume(ResumeRequest {
            session: snapshot.session.clone(),
            epoch: snapshot.scene.epoch,
            revision: snapshot.scene.revision,
        }))
        .unwrap()
    {
        CarrierResponseBody::Resume(reply) => reply,
        other => panic!("expected resume reply, got {other:?}"),
    };
    let ResumeReply::Snapshot(next) = reply else {
        panic!("Knot should replace the stale snapshot");
    };
    assert_eq!(next.scene.revision, notice.revision);

    assert!(matches!(
        carrier.request(CarrierRequestBody::Close).unwrap(),
        CarrierResponseBody::Closed
    ));
    carrier.shutdown().unwrap();
}

#[test]
fn a_retained_graphshell_session_saves_a_real_knot_file() {
    let root = tempdir().unwrap();
    let path = root.path().join("field.knot");
    fs::write(&path, "# Field\n").unwrap();
    let args: [&OsStr; 3] = [
        OsStr::new("directory-write"),
        root.path().as_os_str(),
        OsStr::new("4096"),
    ];
    let mut retained = spawn_endpoint_session(
        env!("CARGO_BIN_EXE_knot_endpoint"),
        args,
        CapabilityProfile::new([
            PresentationCapability::EditableText,
            PresentationCapability::PortableCard,
        ]),
    )
    .unwrap();
    let session = retained.mount(0).unwrap();
    let (target, editable, action) = retained
        .resolve_all(&session)
        .unwrap()
        .into_iter()
        .find_map(|(target, presentation)| match presentation.content {
            ResolvedContent::EditableText(editable) if editable.address.ends_with("field.knot") => {
                Some((target, editable, presentation.semantics.actions[0].clone()))
            }
            _ => None,
        })
        .expect("writable process disclosed editable source");

    let result = retained
        .invoke(
            &session,
            target,
            &action,
            &SaveTextV1 {
                base_token: editable.base_token,
                source: "# Saved over Graphshell\n".into(),
            },
        )
        .unwrap();
    assert_eq!(result, IntentResult::Accepted);
    assert!(retained.wait_for_change().unwrap());
    retained.close().unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "# Saved over Graphshell\n"
    );
}

#[cfg(windows)]
#[test]
fn a_real_startup_unlocked_vault_process_saves_restarts_and_stays_sealed() {
    let root = tempdir().unwrap();
    let persona = personae::PersonaId::new();
    let settings = session_runtime::DeviceSettings {
        startup_unlock_mode: personae::StartupUnlockMode::AutoOs,
    };
    session_runtime::save_device_settings(root.path(), &settings).unwrap();
    session_runtime::wallet_store::ensure_wallet_state(
        root.path(),
        persona,
        "Knot process receipt",
    )
    .unwrap();
    let authority = knot::StartupUnlockedPersonalVault::open(
        root.path(),
        persona,
        knot::local_device_root(root.path(), "knot receipt").unwrap(),
        [],
    )
    .unwrap();
    authority
        .author_document(knot::VaultDocument {
            id: "field-note".into(),
            title: "Field note".into(),
            body: b"# Private\n".to_vec(),
            media_type: "text/vnd.knot".into(),
        })
        .unwrap();
    drop(authority);

    let args = vec![
        OsStr::new("persona-vault").to_os_string(),
        root.path().as_os_str().to_os_string(),
        persona.as_uuid().to_string().into(),
        "4096".into(),
    ];
    let profile = CapabilityProfile::new([
        PresentationCapability::EditableText,
        PresentationCapability::PortableCard,
    ]);
    let mut retained =
        spawn_endpoint_session(env!("CARGO_BIN_EXE_knot_endpoint"), &args, profile.clone())
            .unwrap();
    let session = retained.mount(0).unwrap();
    let (target, editable, action) = retained
        .resolve_all(&session)
        .unwrap()
        .into_iter()
        .find_map(|(target, presentation)| match presentation.content {
            ResolvedContent::EditableText(editable)
                if editable.address == "knot://vault/field-note" =>
            {
                Some((target, editable, presentation.semantics.actions[0].clone()))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(editable.source, "# Private\n");
    assert_eq!(
        retained
            .invoke(
                &session,
                target,
                &action,
                &SaveTextV1 {
                    base_token: editable.base_token,
                    source: "# Private revised\n".into(),
                },
            )
            .unwrap(),
        IntentResult::Accepted
    );
    assert!(retained.wait_for_change().unwrap());
    retained.close().unwrap();

    let mut reopened =
        spawn_endpoint_session(env!("CARGO_BIN_EXE_knot_endpoint"), &args, profile).unwrap();
    let session = reopened.mount(0).unwrap();
    let source = reopened
        .resolve_all(&session)
        .unwrap()
        .into_iter()
        .find_map(|(_, presentation)| match presentation.content {
            ResolvedContent::EditableText(editable)
                if editable.address == "knot://vault/field-note" =>
            {
                Some(editable.source)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(source, "# Private revised\n");
    reopened.close().unwrap();

    let clear = b"# Private revised\n";
    for path in walk_files(root.path()) {
        let bytes = fs::read(&path).unwrap();
        assert!(
            !bytes.windows(clear.len()).any(|window| window == clear),
            "cleartext leaked to {}",
            path.display()
        );
    }
}

#[cfg(windows)]
fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}
