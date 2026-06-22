//! P2.0 verification: the proven 8-turn driver runs against the real
//! `document-host` library (Doc-backed), loading the built `document-core` guest
//! component. Mirrors the probe's end-to-end run, now as a crate test.
//!
//! Build the guest first:
//!   cd guest && cargo build --target wasm32-wasip2 --release
//! then `cargo test`.

use std::path::PathBuf;

#[tokio::test(flavor = "current_thread")]
async fn eight_turns_drive_the_document() {
    let wasm = std::env::var("DOC_HOST_GUEST_WASM").unwrap_or_else(|_| {
        "guest/target/wasm32-wasip2/release/document_core_guest.wasm".to_string()
    });
    let path = PathBuf::from(&wasm);
    assert!(
        path.exists(),
        "guest component missing at {wasm}; build it first: \
         cd guest && cargo build --target wasm32-wasip2 --release"
    );

    let turns = [
        ("set", "Edited intro via node-id."),
        ("append", "Appended under root's id."),
        ("insert", "Inserted before a sibling id."),
        ("subtree", ""),
        ("remove", ""),
        ("stale", ""),
        ("bad-id", ""),
        ("frobnicate", ""),
    ];

    let log = document_host::run_turns(&path, &turns).await.expect("run_turns");
    let joined = log.outcomes.join("\n");

    // Outcomes per turn: four applied id-targeted mutations, a no-op scoped
    // inspect, then the conflict / unknown-node / declined paths.
    assert!(log.outcomes[0].starts_with("set: applied"), "{joined}");
    assert!(log.outcomes[1].starts_with("append: applied"), "{joined}");
    assert!(log.outcomes[2].starts_with("insert: applied"), "{joined}");
    assert!(log.outcomes[3].contains("no-op"), "subtree should be a no-op:\n{joined}");
    assert!(log.outcomes[4].starts_with("remove: applied"), "{joined}");
    assert!(log.outcomes[5].contains("revision-conflict"), "stale should conflict:\n{joined}");
    assert!(log.outcomes[6].contains("unknown-node"), "bad-id should be unknown-node:\n{joined}");
    assert!(log.outcomes[7].contains("declined"), "frobnicate should be declined:\n{joined}");

    assert_eq!(log.final_revision, 4, "exactly four mutations applied\n{joined}");

    // Final tree: root + three paragraphs. Node 3 (appended) was removed; node 4
    // (inserted) sits before node 1 (edited); node 2 unchanged.
    let shape: Vec<(String, String)> =
        log.final_rows.iter().map(|(_, k, t)| (k.clone(), t.clone())).collect();
    assert_eq!(
        shape,
        vec![
            ("root".to_string(), String::new()),
            ("paragraph".to_string(), "Inserted before a sibling id.".to_string()),
            ("paragraph".to_string(), "Edited intro via node-id.".to_string()),
            ("paragraph".to_string(), "Second paragraph.".to_string()),
        ],
        "final document tree mismatch\noutcomes:\n{joined}"
    );
}
