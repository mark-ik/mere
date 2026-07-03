//! §11.4 verification: the always-linked `caps.granted()` discovery import. The
//! document-core guest calls `granted()` in `activate` and logs the result; under
//! the full grant (`run_turns`) it must see both application capabilities. This
//! proves the import is linked regardless of grant and that the guest reads the
//! host's real grant through it.

use std::path::PathBuf;

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
async fn guest_discovers_its_grant_via_caps_granted() {
    // No turns — we only need `activate`, where the guest calls `granted()`.
    let log = document_host::run_turns(&doc_wasm(), &[])
        .await
        .expect("run_turns");
    let activate_line = log
        .guest_logs
        .iter()
        .find(|l| l.contains("granted ["))
        .unwrap_or_else(|| panic!("activate should log the grant; logs: {:?}", log.guest_logs));
    assert!(
        activate_line.contains("mere:script/log")
            && activate_line.contains("mere:script/document-host"),
        "full grant should report both capabilities, got: {activate_line}"
    );
}
