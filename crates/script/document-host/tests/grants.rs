// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! P2.3 verification: the capability grant drives which imports get linked, and a
//! denied *required* capability makes instantiation fail — the capability boundary
//! enforced by the runtime (§10.4 / §11.4). The document-core guest imports
//! `mere:script/document-host` (it calls `inspect`), so omitting that import must
//! fail instantiation; granting it must succeed.

use std::path::PathBuf;

use document_host::Grant;

fn doc_wasm() -> PathBuf {
    let p = std::env::var("DOC_HOST_GUEST_WASM").unwrap_or_else(|_| {
        "guest/target/wasm32-wasip2/release/document_core_guest.wasm".to_string()
    });
    let path = PathBuf::from(&p);
    assert!(
        path.exists(),
        "guest component missing at {p}; build it: cd guest && cargo build --target wasm32-wasip2 --release"
    );
    path
}

#[tokio::test(flavor = "current_thread")]
async fn allow_all_instantiates() {
    let r = document_host::instantiate_with_grant(&doc_wasm(), &Grant::allow_all()).await;
    assert!(r.is_ok(), "allow-all should instantiate, got {r:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn denying_document_capability_fails_instantiation() {
    let r = document_host::instantiate_with_grant(&doc_wasm(), &Grant::deny_document()).await;
    assert!(
        r.is_err(),
        "denying the document capability must fail instantiation (the guest requires it), got {r:?}"
    );
}

#[test]
fn granted_names_reflect_the_grant() {
    // allow_all grants log + document-host + net (the three application capabilities).
    let all = Grant::allow_all().granted_names();
    assert_eq!(all.len(), 3);
    assert!(all.contains(&"mere:script/net".to_string()));
    // deny_document keeps only log (document + net denied).
    let denied = Grant::deny_document().granted_names();
    assert_eq!(denied, vec!["mere:script/log".to_string()]);
}

/// The one-grant unification (participant gate B3): the SAME servitor
/// authority a denizen's gate consults derives this world's import grant.
/// A subject whose caps cover doc/ instantiates; a subject granted only its
/// scenario world fails at instantiation — unimported means unreachable,
/// decided by the one grant.
#[tokio::test(flavor = "current_thread")]
async fn authority_derived_grant_gates_instantiation() {
    use servitor::Grant as IssuedGrant;
    use servitor::{Cap, GrantTable, Mode, Subject};

    let scripter = Subject::new([1; 32]);
    let bystander = Subject::new([2; 32]);
    // Capabilities are typed now: these are hierarchical scopes, which cover by
    // segment prefix, so `doc` covers `doc/log` and friends. `PrefixAuthority`
    // became `GrantTable` in the same round — the matching rule moved onto the
    // capability, so the table no longer names it.
    let authority = GrantTable::new()
        .with_grant(IssuedGrant::new(
            scripter,
            Cap::scope("doc").expect("doc is a valid scope"),
            Mode::Write,
        ))
        .with_grant(IssuedGrant::new(
            bystander,
            Cap::scope("scenario").expect("scenario is a valid scope"),
            Mode::Write,
        ));

    let granted = Grant::from_authority(&authority, scripter);
    let r = document_host::instantiate_with_grant(&doc_wasm(), &granted).await;
    assert!(r.is_ok(), "a doc/-covered subject instantiates, got {r:?}");

    let ungranted = Grant::from_authority(&authority, bystander);
    let r = document_host::instantiate_with_grant(&doc_wasm(), &ungranted).await;
    assert!(
        r.is_err(),
        "a subject without doc/ coverage fails at instantiation (the import is unlinked)"
    );
}
