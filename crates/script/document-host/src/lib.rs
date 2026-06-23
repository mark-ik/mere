//! DocumentScript component host (P2.1b).
//!
//! The native side of the `document-core` WIT world. It owns a Wasmtime engine +
//! a per-instance `Store<ScriptHost>`, backs the `log` + `document-host.inspect`
//! imports over a **live serval `ScriptedDom`** (via [`dom_view`]), and drives the
//! per-turn `handle-event` contract (§10.2) with atomic, revision-checked `apply`
//! (§10.3).
//!
//! - Exports are invoked via `call_async` (`exports: { default: async }`): turns
//!   run on a fiber, so a future sync `fetch` import can suspend a turn without
//!   blocking (plan §11.7-7). No async import yet, so turns never suspend.
//! - The document is the real HTML DOM (P2.0's in-memory `Doc` is retired): each
//!   element is a view-node named by tag, each text node a `#text` view-node. Node
//!   identity is serval's `NodeId` round-tripped through the WIT `node-id` (`u64`);
//!   `is_live` guards every mutation. The revision counter is host-side.

use std::path::Path;

/// Project / mutate a live serval `ScriptedDom` behind the WIT imports.
pub mod dom_view;

/// The `register-mod-loader` `WasmModRuntime` bridge (P2.4).
pub mod runtime;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use serval_scripted_dom::ScriptedDom;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "wit",
    world: "document-core",
    // Turns run on a fiber so a future sync `fetch` import can suspend them
    // (plan §11.7-7). Host imports stay synchronous.
    exports: { default: async },
});

use crate::mere::script::document::DocumentView;

/// FNV-1a over kind + text. A deterministic stand-in for the change-detection
/// token; a real impl uses a subtree Merkle hash.
pub(crate) fn content_hash(kind: &str, text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in kind.bytes().chain(std::iter::once(0)).chain(text.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

/// A small starting document: `<body><p>Intro paragraph.</p><p>Second paragraph.</p></body>`.
/// In the real integration (P2.5) this comes from the fetched page; here the host
/// seeds it so the contract can be exercised headless.
fn seed_dom() -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let body = dom.create_element(qual("body"));
    dom.append_child(root, body);
    for t in ["Intro paragraph.", "Second paragraph."] {
        let p = dom.create_element(qual("p"));
        dom.append_child(body, p);
        let text = dom.create_text(t);
        dom.append_child(p, text);
    }
    dom
}

/// The per-instance store data: the live DOM + its host-side revision, the WASI
/// floor the std guest needs, and a sink capturing the guest's `log` output.
pub struct ScriptHost {
    dom: ScriptedDom,
    revision: u64,
    logs: Vec<String>,
    wasi: WasiCtx,
    table: ResourceTable,
    /// Resource quotas (P2.2). Default = unlimited (run_turns); run_guarded caps it.
    limits: StoreLimits,
}

impl crate::mere::script::log::Host for ScriptHost {
    fn log(&mut self, message: String) {
        self.logs.push(message);
    }
}

/// The `inspect` capability: the guest calls this back during a turn to pull a
/// (scoped) copied snapshot of the live DOM. Only nodes in the view are nameable.
impl crate::mere::script::document_host::Host for ScriptHost {
    fn inspect(&mut self, query: crate::mere::script::document::DocumentQuery) -> DocumentView {
        use crate::mere::script::document::DocumentQuery;
        match query {
            DocumentQuery::WholeDocument => dom_view::snapshot(&self.dom, self.revision),
            DocumentQuery::Subtree(id) => dom_view::snapshot_subtree(&self.dom, self.revision, id),
        }
    }
}

impl WasiView for ScriptHost {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

/// The result of driving a sequence of turns.
pub struct TurnLog {
    /// One human-readable line per turn (applied / rejected / declined).
    pub outcomes: Vec<String>,
    /// The guest's captured `log` output.
    pub guest_logs: Vec<String>,
    /// The final document, document order: `(id, kind, text)`.
    pub final_rows: Vec<(u64, String, String)>,
    /// The final revision.
    pub final_revision: u64,
}

/// Instantiate the `document-core` component at `component_path` over a seeded
/// live `ScriptedDom`, `activate` it, drive `(kind, payload)` turns applying each
/// returned batch against the live DOM + revision, `deactivate`, and report.
/// Async because exports are invoked via `call_async`.
pub async fn run_turns(component_path: &Path, turns: &[(&str, &str)]) -> wasmtime::Result<TurnLog> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, component_path)?;

    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::document_host::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
        &mut linker,
        |s| s,
    )?;

    let mut store = Store::new(
        &engine,
        ScriptHost {
            dom: seed_dom(),
            revision: 0,
            logs: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: StoreLimits::default(),
        },
    );

    let bindings = DocumentCore::instantiate_async(&mut store, &component, &linker).await?;
    bindings.call_activate(&mut store).await?;

    let mut outcomes = Vec::new();
    for (kind, payload) in turns {
        let ev = Event { kind: (*kind).to_string(), payload: (*payload).to_string() };
        match bindings.call_handle_event(&mut store, &ev).await? {
            Ok(batch) => {
                let cited = batch.expected_revision;
                let h = store.data_mut();
                let before = h.revision;
                match dom_view::apply(&mut h.dom, &mut h.revision, batch) {
                    Ok(new_rev) if new_rev == before => {
                        outcomes.push(format!("{kind}: no-op (rev unchanged at {new_rev})"))
                    }
                    Ok(new_rev) => {
                        outcomes.push(format!("{kind}: applied (cited {cited}) -> rev {new_rev}"))
                    }
                    Err(TurnError::RevisionConflict(cur)) => outcomes
                        .push(format!("{kind}: revision-conflict (cited {cited}, current {cur})")),
                    Err(TurnError::UnknownNode(id)) => {
                        outcomes.push(format!("{kind}: unknown-node {id} (nothing applied)"))
                    }
                    Err(TurnError::Refused(why)) => outcomes.push(format!("{kind}: refused ({why})")),
                }
            }
            Err(TurnError::Refused(why)) => outcomes.push(format!("{kind}: declined ({why})")),
            Err(TurnError::RevisionConflict(cur)) => {
                outcomes.push(format!("{kind}: guest revision-conflict ({cur})"))
            }
            Err(TurnError::UnknownNode(id)) => {
                outcomes.push(format!("{kind}: guest unknown-node {id}"))
            }
        }
    }

    bindings.call_deactivate(&mut store).await?;

    let h = store.data();
    let view = dom_view::snapshot(&h.dom, h.revision);
    let final_rows =
        view.nodes.iter().map(|n| (n.id, n.kind.clone(), n.text.clone())).collect();
    Ok(TurnLog {
        outcomes,
        guest_logs: h.logs.clone(),
        final_rows,
        final_revision: h.revision,
    })
}

/// Outcome of a guarded single-turn run (P2.2).
#[derive(Debug)]
pub enum Guarded {
    /// The turn completed normally.
    Completed,
    /// The turn was contained — an epoch deadline cancelled a runaway loop, or a
    /// `StoreLimits` cap denied an unbounded allocation. Carries the trap text.
    /// The host thread survived either way.
    Trapped(String),
}

/// Run a single `kind` turn under resource guards: an **epoch deadline** (so an
/// infinite loop is cancelled — epoch interruption, no async needed beyond the
/// fiber call) and a **`StoreLimits` memory cap** (so an unbounded allocation is
/// denied). A misbehaving guest is contained: the call returns a trap and the
/// host survives. This is the §11.2 quota/cancellation proof ("Wasm isolation
/// without quotas is incomplete isolation").
pub async fn run_guarded(
    component_path: &Path,
    kind: &str,
    mem_bytes: usize,
    epoch_deadline_ticks: u64,
) -> wasmtime::Result<Guarded> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    let engine = Arc::new(Engine::new(&config)?);
    let component = Component::from_file(&engine, component_path)?;

    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::document_host::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
        &mut linker,
        |s| s,
    )?;

    let mut store = Store::new(
        &engine,
        ScriptHost {
            dom: seed_dom(),
            revision: 0,
            logs: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new().memory_size(mem_bytes).build(),
        },
    );
    store.limiter(|h| &mut h.limits);
    store.set_epoch_deadline(epoch_deadline_ticks);

    // Watchdog: bump the epoch on a fixed tick so a runaway turn reaches its
    // deadline. One shared thread per run; stopped and joined when the run ends.
    let stop = Arc::new(AtomicBool::new(false));
    let watch_engine = engine.clone();
    let watch_stop = stop.clone();
    let watchdog = std::thread::spawn(move || {
        while !watch_stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
            watch_engine.increment_epoch();
        }
    });

    let outcome = async {
        let bindings = DocumentCore::instantiate_async(&mut store, &component, &linker).await?;
        bindings.call_activate(&mut store).await?;
        let ev = Event { kind: kind.to_string(), payload: String::new() };
        // The inner Result (the guest's batch or a turn-error) is irrelevant here;
        // this run only distinguishes "completed" from "trapped/denied".
        let _ = bindings.call_handle_event(&mut store, &ev).await?;
        Ok::<(), wasmtime::Error>(())
    }
    .await;

    stop.store(true, Ordering::Relaxed);
    let _ = watchdog.join();

    Ok(match outcome {
        Ok(()) => Guarded::Completed,
        Err(e) => Guarded::Trapped(format!("{e}")),
    })
}

/// A capability's resolved permission. Mirrors `kernel::permissions::Permission`
/// (`Allow < Prompt < Deny`); the kernel five-scope → `Grant` mapping is a thin
/// P2.5 adapter in the caller (the content actor), so `document-host` needs no
/// graph-kernel dependency (§11.4: the policy lives here, the resolution is input).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapPermission {
    Allow,
    Prompt,
    Deny,
}

/// Which `mere:script` application capabilities a script is granted. The WASI
/// runtime floor is always linked (a std guest needs it); these gate the
/// application imports. A `Deny` (or, for P2, a `Prompt`) omits the import, so a
/// component that *requires* it fails to instantiate — the secure default the probe
/// proved (§10.4): unimported means unreachable.
#[derive(Clone, Debug)]
pub struct Grant {
    pub log: CapPermission,
    pub document: CapPermission,
}

impl Grant {
    /// Everything the document-core world offers.
    pub fn allow_all() -> Self {
        Self { log: CapPermission::Allow, document: CapPermission::Allow }
    }

    /// `log` only — the inspect/apply document capability denied.
    pub fn deny_document() -> Self {
        Self { log: CapPermission::Allow, document: CapPermission::Deny }
    }

    /// The granted application-capability interface names (the seam a future
    /// `caps.granted()` discovery import returns).
    pub fn granted_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if self.log == CapPermission::Allow {
            names.push("mere:script/log".to_string());
        }
        if self.document == CapPermission::Allow {
            names.push("mere:script/document-host".to_string());
        }
        names
    }
}

/// Link the WASI floor (always) plus exactly the granted `mere:script` imports.
/// Only `Allow` links; `Deny`/`Prompt` omit. (A `Prompt` must be resolved to
/// Allow/Deny before instantiation by the caller; P2 omits it conservatively.)
fn link_with_grant(linker: &mut Linker<ScriptHost>, grant: &Grant) -> wasmtime::Result<()> {
    wasmtime_wasi::p2::add_to_linker_async(linker)?;
    if grant.log == CapPermission::Allow {
        crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(linker, |s| s)?;
    }
    if grant.document == CapPermission::Allow {
        crate::mere::script::document_host::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
            linker,
            |s| s,
        )?;
    }
    Ok(())
}

/// Build a `document-core` instance over a fresh seeded DOM on `engine`, linking
/// exactly the granted imports (plus the WASI floor). Returns the live store +
/// bindings **before** `activate` — the caller drives the lifecycle. Shared by
/// [`instantiate_with_grant`] and the [`runtime`] mod-loader bridge. Fails if the
/// component requires an import the grant omitted (the runtime-enforced boundary).
pub(crate) async fn build_instance(
    engine: &Engine,
    component_path: &Path,
    grant: &Grant,
) -> wasmtime::Result<(Store<ScriptHost>, DocumentCore)> {
    let component = Component::from_file(engine, component_path)?;
    let mut linker = Linker::new(engine);
    link_with_grant(&mut linker, grant)?;
    let mut store = Store::new(
        engine,
        ScriptHost {
            dom: seed_dom(),
            revision: 0,
            logs: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: StoreLimits::default(),
        },
    );
    let bindings = DocumentCore::instantiate_async(&mut store, &component, &linker).await?;
    Ok((store, bindings))
}

/// Try to instantiate (and `activate`) the component under `grant`. Succeeds only
/// if every import the component *requires* is granted; a denied required
/// capability makes instantiation fail — the capability boundary, enforced by the
/// runtime, not by host convention. The seed DOM + granted names are wired so the
/// guest (and a future `caps.granted()`) see exactly what was allowed.
pub async fn instantiate_with_grant(component_path: &Path, grant: &Grant) -> wasmtime::Result<()> {
    let engine = Engine::default();
    let (mut store, bindings) = build_instance(&engine, component_path, grant).await?;
    bindings.call_activate(&mut store).await?;
    Ok(())
}
