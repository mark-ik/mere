use std::ffi::OsStr;
use std::fs;

use graphshell::client::ResolvedContent;
use graphshell::sessions::RetainedEndpointSession;
use graphshell_protocol::{
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
    let mut retained = RetainedEndpointSession::spawn(
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
