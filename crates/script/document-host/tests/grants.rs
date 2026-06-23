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
    assert_eq!(Grant::allow_all().granted_names().len(), 2);
    let denied = Grant::deny_document().granted_names();
    assert_eq!(denied, vec!["mere:script/log".to_string()]);
}
