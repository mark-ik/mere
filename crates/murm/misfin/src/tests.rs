/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::transport::ensure_identity_with_root;
use super::*;

use tempfile::TempDir;

#[test]
fn gemmail_extracts_metadata_and_subject() {
    let gemmail = parse_gemmail(
        "< friend@example.com Friendly Person\n: one@example.com two@example.com\n@ 2023-05-09T19:39:15Z\n# A note on flowers\n\nThe green ones bite.\n",
    );

    assert_eq!(
        gemmail
            .sender
            .as_ref()
            .map(|sender| sender.address.as_addr_spec()),
        Some("friend@example.com".to_string())
    );
    assert_eq!(gemmail.recipients.len(), 2);
    assert_eq!(gemmail.timestamp.as_deref(), Some("2023-05-09T19:39:15Z"));
    assert_eq!(gemmail.subject.as_deref(), Some("A note on flowers"));
    assert_eq!(gemmail.body, "# A note on flowers\n\nThe green ones bite.");
}

#[test]
fn identity_status_reports_persisted_identity() {
    let tempdir = TempDir::new().expect("temp dir should be created");
    let spec = MisfinIdentitySpec {
        address: MisfinAddress::parse("worker@hive.local").expect("sender should parse"),
        blurb: Some("Worker Bee".to_string()),
    };

    let status =
        ensure_identity_with_root(&spec, Some(tempdir.path())).expect("identity should be created");

    assert!(status.exists);
    assert_eq!(status.address, "worker@hive.local");
    assert!(status.path.expect("identity path should exist").exists());
    assert!(status.certificate_fingerprint.is_some());
}
