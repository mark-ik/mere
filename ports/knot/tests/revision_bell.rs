use std::fs;

use graphshell_protocol::{CarrierRequestBody, CarrierResponseBody, ResumeReply, ResumeRequest};
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
