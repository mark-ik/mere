//! P2.5a verification: the `DocumentScript` API drives a script over a
//! **caller-provided** live `ScriptedDom` (the seam the meerkat content actor
//! consumes), the script mutates that DOM, and the caller reads the mutations back
//! for rendering. Also checks the conflict / unknown-node / declined paths and that
//! `detach` returns the mutated DOM. Quotas are exercised by `tests/guarded.rs`;
//! here the focus is the external-DOM round trip.

use std::path::PathBuf;
use std::sync::Arc;

use document_host::{DocumentScript, Grant, NetFetcher, NetResponse, Quota, TurnOutcome};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use serval_scripted_dom::{NodeId, ScriptedDom};

/// A stub network backend for the fetch test: echoes the requested URL (the real
/// backend, netfetcher/errand, lives in the meerkat content actor).
struct EchoFetcher;
impl NetFetcher for EchoFetcher {
    fn fetch(&self, url: &str) -> Result<NetResponse, String> {
        Ok(NetResponse { status: 200, content_type: Some("text/plain".into()), body: format!("fetched:{url}") })
    }
}

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

fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

/// Build `<body><p>Hello</p></body>`; return (dom, body id, first text node id).
fn page() -> (ScriptedDom, NodeId, NodeId) {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let body = dom.create_element(qual("body"));
    dom.append_child(root, body);
    let p = dom.create_element(qual("p"));
    dom.append_child(body, p);
    let t = dom.create_text("Hello");
    dom.append_child(p, t);
    (dom, body, t)
}

fn text_of(dom: &ScriptedDom, id: NodeId) -> Option<String> {
    dom.text(id).map(|s| s.to_string())
}

#[test]
fn script_mutates_a_caller_provided_dom_and_reads_back() {
    let (dom, body, t1) = page();
    let body_count_before = dom.dom_children(body).count();
    assert_eq!(body_count_before, 1, "body starts with one <p>");

    let mut script =
        DocumentScript::attach(&doc_wasm(), dom, &Grant::allow_all(), Quota::default(), None)
            .expect("attach should succeed under the full grant");
    assert_eq!(script.revision(), 0);

    // The guest's "set" edits the first #text node.
    let out = script.deliver_event("set", "Edited via node-id").expect("set turn");
    assert_eq!(out, TurnOutcome::Applied(1), "set should apply and bump the revision");
    assert_eq!(
        text_of(script.dom(), t1).as_deref(),
        Some("Edited via node-id"),
        "the live DOM the caller renders reflects the script's edit"
    );

    // "append" adds a <p> under <body>.
    let out = script.deliver_event("append", "").expect("append turn");
    assert_eq!(out, TurnOutcome::Applied(2));
    assert_eq!(script.dom().dom_children(body).count(), 2, "a <p> was appended");

    // The error paths surface as semantic outcomes (no WIT types leaked).
    assert_eq!(
        script.deliver_event("bad-id", "").expect("bad-id turn"),
        TurnOutcome::UnknownNode(u64::MAX),
    );
    assert!(
        matches!(script.deliver_event("frobnicate", "").expect("decline turn"), TurnOutcome::Refused(_)),
        "an unknown event kind is declined by the guest",
    );
    // A rejected turn left the revision where the last applied turn put it.
    assert_eq!(script.revision(), 2);

    // detach returns the mutated DOM to the caller.
    let dom = script.detach().expect("detach runs deactivate + returns the DOM");
    assert_eq!(dom.dom_children(body).count(), 2, "detached DOM keeps the appended <p>");
    assert_eq!(text_of(&dom, t1).as_deref(), Some("Edited via node-id"));
}

#[test]
fn precompiled_cwasm_attaches_and_drives_without_codegen() {
    // P2.6 AOT: compile the guest to a `.cwasm`, then attach from it (a `deserialize`
    // load — no Cranelift on the hot path) and drive a turn. The precompiled path
    // mutates the live DOM exactly like the JIT `.wasm` path.
    let cwasm = document_host::precompile_to_cwasm(&doc_wasm()).expect("precompile to cwasm");
    let path = std::env::temp_dir().join(format!("doc-core-aot-{}.cwasm", std::process::id()));
    std::fs::write(&path, &cwasm).expect("write cwasm");

    let (dom, _body, t1) = page();
    let mut script = DocumentScript::attach(&path, dom, &Grant::allow_all(), Quota::default(), None)
        .expect("attach from a precompiled .cwasm");
    assert_eq!(
        script.deliver_event("set", "Edited via AOT").expect("turn"),
        TurnOutcome::Applied(1),
    );
    assert_eq!(text_of(script.dom(), t1).as_deref(), Some("Edited via AOT"));
    let _ = script.detach();
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_granted_script_fetches_through_the_async_net_import() {
    // §11.7-7 fiber-async: the guest calls `fetch` *synchronously*; the host's async
    // `net.fetch` import (invoked on the turn's fiber) returns a response the guest
    // writes into the DOM. `net` is granted via `allow_all`; without it the guest —
    // which imports `net` — would fail to instantiate.
    let (dom, _body, t1) = page();
    let fetcher: Arc<dyn NetFetcher> = Arc::new(EchoFetcher);
    let mut script =
        DocumentScript::attach(&doc_wasm(), dom, &Grant::allow_all(), Quota::default(), Some(fetcher))
            .expect("attach with net granted");
    let out = script.deliver_event("fetch", "https://example.com/x").expect("fetch turn");
    assert_eq!(out, TurnOutcome::Applied(1));
    assert_eq!(
        text_of(script.dom(), t1).as_deref(),
        Some("fetched:https://example.com/x"),
        "the injected fetcher's response landed in the live DOM",
    );
}
