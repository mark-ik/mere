//! P2.1b verification: the 8-turn driver runs the DOM-shaped guest end to end
//! against the `document-host` library backed by a live genet `ScriptedDom`
//! (seeded `<body><p>Intro</p><p>Second</p></body>`), asserting the DOM mutates
//! and the conflict / unknown-node / declined paths fire.
//!
//! Build the guest first:
//!   cd guest && cargo build --target wasm32-wasip2 --release
//! then `cargo test`.

use std::path::PathBuf;

#[tokio::test(flavor = "current_thread")]
async fn eight_turns_drive_the_live_dom() {
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
        ("append", ""),
        ("insert", ""),
        ("subtree", ""),
        ("remove", ""),
        ("stale", ""),
        ("bad-id", ""),
        ("frobnicate", ""),
    ];

    let log = document_host::run_turns(&path, &turns)
        .await
        .expect("run_turns");
    let joined = log.outcomes.join("\n");

    assert!(log.outcomes[0].starts_with("set: applied"), "{joined}");
    assert!(log.outcomes[1].starts_with("append: applied"), "{joined}");
    assert!(log.outcomes[2].starts_with("insert: applied"), "{joined}");
    assert!(
        log.outcomes[3].contains("no-op"),
        "subtree should be a no-op:\n{joined}"
    );
    assert!(log.outcomes[4].starts_with("remove: applied"), "{joined}");
    assert!(
        log.outcomes[5].contains("revision-conflict"),
        "stale should conflict:\n{joined}"
    );
    assert!(
        log.outcomes[6].contains("unknown-node"),
        "bad-id should be unknown-node:\n{joined}"
    );
    assert!(
        log.outcomes[7].contains("declined"),
        "frobnicate should be declined:\n{joined}"
    );

    assert_eq!(
        log.final_revision, 4,
        "exactly four mutations applied\n{joined}"
    );

    // Final DOM: <body> has three <p> children (two appended/inserted, one of the
    // originals removed), and the first text node now carries the edited text.
    let p_count = log.final_rows.iter().filter(|(_, k, _)| k == "p").count();
    assert_eq!(
        p_count, 3,
        "expected 3 <p> elements\nrows: {:?}",
        log.final_rows
    );
    assert!(
        log.final_rows
            .iter()
            .any(|(_, k, t)| k == "#text" && t == "Edited intro via node-id."),
        "the edited text should be live in a #text node\nrows: {:?}",
        log.final_rows
    );
}
