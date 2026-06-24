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
    // The shared, runtime-neutral `mere:script` contract (one WIT, two runtimes:
    // this native Wasmtime host + the browser jco path). Lives at `crates/script/wit`,
    // not inside this native crate, so the browser consumes the same world.
    path: "../wit",
    world: "document-core",
    // Turns run on a fiber (`exports` async, invoked via `call_async`); host imports
    // are async too so the sync-signature `net.fetch` can be implemented as a host
    // `async fn` that suspends the turn's fiber during I/O without blocking the host
    // thread (plan §11.7-7). The other imports don't await — `async fn` is free there.
    exports: { default: async },
    imports: { default: async },
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

/// A response from [`NetFetcher::fetch`] (a clean public mirror of the WIT
/// `net::response`, so an embedder need not name the generated type).
pub struct NetResponse {
    pub status: u32,
    /// The response media type, if the backend reported one (mirrors WIT `content-type`).
    pub content_type: Option<String>,
    pub body: String,
}

/// The host's network backend for `net.fetch` (§11.7-7). document-host stays
/// dependency-light (no netfetcher/errand), so the embedder injects a concrete
/// fetcher at [`DocumentScript::attach`]. `fetch` is **blocking**: it runs inside
/// the guest's `net.fetch` host call on the turn's fiber, and the (sync) content
/// actor drives the fiber via `pollster`, so the impl blocks that actor thread for
/// the I/O (e.g. `block_on` over netfetcher/errand) and returns a ready result.
/// `Err` surfaces to the guest as `result::err`.
pub trait NetFetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<NetResponse, String>;
}

/// The normalized host of a URL for the `net` allowlist check: the authority between
/// `://` and the next `/?#`, with userinfo (`user@`) and `:port` stripped, IPv6
/// brackets removed, lowercased. Mirrors the embedder's `host_of`; kept local so
/// document-host stays embedder-agnostic (a shared crate is the longer-term home).
fn net_host_of(url: &str) -> String {
    let after = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = after.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split_once(']').map(|(inner, _)| inner).unwrap_or(host_port)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    host.to_ascii_lowercase()
}

/// Whether `url`'s host is in the granted `net` origin allowlist (§E1): an exact host
/// or a `*.suffix` glob. An empty allowlist denies everything (fail-closed).
fn host_in_origins(url: &str, origins: &[String]) -> bool {
    let host = net_host_of(url);
    origins.iter().any(|pattern| {
        let pattern = pattern.to_ascii_lowercase();
        match pattern.strip_prefix("*.") {
            Some(suffix) => host == suffix || host.ends_with(&format!(".{suffix}")),
            None => host == pattern,
        }
    })
}

/// The per-instance store data: the live DOM + its host-side revision, the WASI
/// floor the std guest needs, and a sink capturing the guest's `log` output.
pub struct ScriptHost {
    dom: ScriptedDom,
    revision: u64,
    logs: Vec<String>,
    /// The injected network backend for `net.fetch` (§11.7-7). `None` = no backend
    /// (a `net.fetch` call returns an error); set by [`DocumentScript::attach`].
    fetcher: Option<std::sync::Arc<dyn NetFetcher>>,
    /// The origin allowlist this instance's `net.fetch` may reach (§E1 origin-scoped
    /// net): exact hosts or `*.suffix` globs. Empty = reach nothing (a net-granted
    /// script with no declared origins is denied every fetch). The host enforces this
    /// at the `net.fetch` boundary *before* the backend runs, so `net` means "may
    /// reach these origins", not "open internet". Set by [`DocumentScript::attach`].
    net_origins: Vec<String>,
    /// `net.fetch` calls made in the current turn (§A6), reset to 0 at each turn start.
    fetches_this_turn: u32,
    /// The per-turn `net.fetch` cap; a call past it is refused. Set from `Quota` at
    /// [`DocumentScript::attach`]; [`new_host`] seeds the default for the turn drivers.
    max_fetches_per_turn: u32,
    /// The granted application-capability interface names this instance can see via
    /// the always-linked `caps.granted()` import (§11.4). Authoritative copy is the
    /// `Grant`; this is what the script is told it has.
    granted: Vec<String>,
    wasi: WasiCtx,
    table: ResourceTable,
    /// Resource quotas (P2.2). Default = unlimited (run_turns); run_guarded caps it.
    limits: StoreLimits,
}

impl crate::mere::script::log::Host for ScriptHost {
    async fn log(&mut self, message: String) {
        self.logs.push(message);
    }
}

/// `caps.granted()` (§11.4): report the granted application-capability interface
/// names so a script can adapt to a partial grant. Read-only; the `Grant` is
/// authoritative.
impl crate::mere::script::caps::Host for ScriptHost {
    async fn granted(&mut self) -> Vec<String> {
        self.granted.clone()
    }
}

/// `net.fetch(request)` (§11.7-7): the fiber-async network capability. Sync-signature
/// to the guest, `async fn` here — a real turn suspends the fiber during I/O while the
/// host thread stays live. The real backend (netfetcher for http(s), errand for
/// smolweb) is supplied by the content actor when it wires `net`. Linked only under
/// the `net` grant, and gated here to the instance's origin allowlist (§E1): `net`
/// means "may reach these origins", so a cross-origin URL is refused before the
/// backend runs (closes the credentialed-SSRF / cross-origin-exfil channel).
impl crate::mere::script::net::Host for ScriptHost {
    async fn fetch(
        &mut self,
        req: crate::mere::script::net::Request,
    ) -> Result<crate::mere::script::net::Response, String> {
        // Per-turn rate cap first (§A6): bound how many fetches one turn can issue, so
        // a granted script cannot flood egress in a loop. Reset each turn.
        if self.fetches_this_turn >= self.max_fetches_per_turn {
            return Err(format!(
                "net.fetch: per-turn request limit reached ({})",
                self.max_fetches_per_turn
            ));
        }
        self.fetches_this_turn += 1;
        // Origin gate: the script may only reach its granted origins (default
        // same-origin), so a granted `net` cannot exfiltrate to or read an arbitrary
        // third-party host. Enforced before the backend is even consulted.
        if !host_in_origins(&req.url, &self.net_origins) {
            return Err(format!("net.fetch: {} is outside this script's net origins", req.url));
        }
        // Call the injected backend (netfetcher/errand, supplied by the content actor).
        // Blocking on the turn's fiber: the host thread parks for the I/O, then the
        // fiber resumes with the result. No backend configured = an error to the guest.
        let fetcher = self.fetcher.as_ref().ok_or_else(|| "net backend not configured".to_string())?;
        let resp = fetcher.fetch(&req.url)?;
        Ok(crate::mere::script::net::Response {
            status: resp.status,
            content_type: resp.content_type,
            body: resp.body,
        })
    }
}

/// The `inspect` capability: the guest calls this back during a turn to pull a
/// (scoped) copied snapshot of the live DOM. Only nodes in the view are nameable.
impl crate::mere::script::document_host::Host for ScriptHost {
    async fn inspect(
        &mut self,
        query: crate::mere::script::document::DocumentQuery,
    ) -> DocumentView {
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

/// The full linker: WASI floor + every `mere:script` import (`log`, `caps`,
/// `document-host`). Used by the unguarded/guarded turn drivers, which grant
/// everything; the grant-policy path is [`link_with_grant`]. `caps` is always
/// linked (it reports the grant, it is not a capability — §11.4).
fn full_linker(engine: &Engine) -> wasmtime::Result<Linker<ScriptHost>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::caps::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::net::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::document_host::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
        &mut linker,
        |s| s,
    )?;
    Ok(linker)
}

/// Construct a per-instance [`ScriptHost`] over `dom` at revision 0, told it holds
/// `granted` capabilities (the `caps.granted()` answer) and bounded by `limits`.
/// The turn drivers pass [`seed_dom`]; [`DocumentScript`] passes a caller-supplied
/// live page DOM.
fn new_host(dom: ScriptedDom, granted: Vec<String>, limits: StoreLimits) -> ScriptHost {
    ScriptHost {
        dom,
        revision: 0,
        logs: Vec::new(),
        granted,
        fetcher: None,
        net_origins: Vec::new(),
        fetches_this_turn: 0,
        max_fetches_per_turn: DEFAULT_MAX_FETCHES_PER_TURN,
        wasi: WasiCtxBuilder::new().build(),
        table: ResourceTable::new(),
        limits,
    }
}

/// Instantiate the `document-core` component at `component_path` over a seeded
/// live `ScriptedDom`, `activate` it, drive `(kind, payload)` turns applying each
/// returned batch against the live DOM + revision, `deactivate`, and report.
/// Async because exports are invoked via `call_async`.
pub async fn run_turns(component_path: &Path, turns: &[(&str, &str)]) -> wasmtime::Result<TurnLog> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, component_path)?;

    let linker = full_linker(&engine)?;
    let mut store = Store::new(
        &engine,
        new_host(seed_dom(), Grant::allow_all().granted_names(), StoreLimits::default()),
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

    let linker = full_linker(&engine)?;
    let mut store = Store::new(
        &engine,
        new_host(
            seed_dom(),
            Grant::allow_all().granted_names(),
            StoreLimitsBuilder::new().memory_size(mem_bytes).build(),
        ),
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
    /// Network egress (`net.fetch`, §11.7-7). Powerful — defaults to denied
    /// everywhere except an explicit grant; a script importing `net` fails to
    /// instantiate unless this is `Allow`.
    pub net: CapPermission,
}

impl Grant {
    /// Everything the document-core world offers (including `net`).
    pub fn allow_all() -> Self {
        Self {
            log: CapPermission::Allow,
            document: CapPermission::Allow,
            net: CapPermission::Allow,
        }
    }

    /// `log` only — the document (inspect/apply) and network capabilities denied.
    pub fn deny_document() -> Self {
        Self {
            log: CapPermission::Allow,
            document: CapPermission::Deny,
            net: CapPermission::Deny,
        }
    }

    /// The granted application-capability interface names (the `caps.granted()`
    /// discovery answer, §11.4).
    pub fn granted_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if self.log == CapPermission::Allow {
            names.push("mere:script/log".to_string());
        }
        if self.document == CapPermission::Allow {
            names.push("mere:script/document-host".to_string());
        }
        if self.net == CapPermission::Allow {
            names.push("mere:script/net".to_string());
        }
        names
    }
}

/// Link the WASI floor (always) plus exactly the granted `mere:script` imports.
/// Only `Allow` links; `Deny`/`Prompt` omit. (A `Prompt` must be resolved to
/// Allow/Deny before instantiation by the caller; P2 omits it conservatively.)
fn link_with_grant(linker: &mut Linker<ScriptHost>, grant: &Grant) -> wasmtime::Result<()> {
    wasmtime_wasi::p2::add_to_linker_async(linker)?;
    // `caps` is always linked — it reports the grant, it is not itself a capability
    // (§11.4), so even a maximally-denied instance can discover it has nothing.
    crate::mere::script::caps::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(linker, |s| s)?;
    if grant.log == CapPermission::Allow {
        crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(linker, |s| s)?;
    }
    if grant.document == CapPermission::Allow {
        crate::mere::script::document_host::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
            linker,
            |s| s,
        )?;
    }
    if grant.net == CapPermission::Allow {
        crate::mere::script::net::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(linker, |s| s)?;
    }
    Ok(())
}

/// Build a `document-core` instance over `dom` on `engine`, linking exactly the
/// granted imports (plus the WASI floor) and bounding the store by `limits`.
/// Returns the live store + bindings **before** `activate` — the caller drives the
/// lifecycle. Shared by [`instantiate_with_grant`], the [`runtime`] mod-loader
/// bridge, and [`DocumentScript`]. Fails if the component requires an import the
/// grant omitted (the runtime-enforced boundary).
/// The engine config the guarded [`DocumentScript`] paths use: epoch interruption
/// on, so a runaway turn can be trapped. Shared by [`DocumentScript::attach`] and
/// [`precompile_to_cwasm`] so an AOT `.cwasm` is config-compatible with the engine
/// that loads it (`deserialize` checks the compile config matches).
fn guarded_engine() -> wasmtime::Result<Engine> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    Engine::new(&config)
}

/// Load a `document-core` component: a precompiled `.cwasm` via `deserialize` (the
/// P2.6 AOT path — no Cranelift on the hot path), else a `.wasm` via `from_file`
/// (JIT). `.cwasm` is for trusted first-party bundled components only.
fn load_component(engine: &Engine, path: &Path) -> wasmtime::Result<Component> {
    if path.extension().and_then(|e| e.to_str()) == Some("cwasm") {
        // SAFETY: a `.cwasm` is a trusted first-party build artifact produced by
        // `precompile_to_cwasm` with a config-matching engine; `deserialize` loads
        // precompiled machine code (no Cranelift). Never deserialize untrusted bytes.
        unsafe { Component::deserialize_file(engine, path) }
    } else {
        Component::from_file(engine, path)
    }
}

/// Ahead-of-time compile a `document-core` `.wasm` component to a `.cwasm` byte blob
/// (P2.6): Cranelift runs *here* (build time), not at `attach`. Write the bytes to a
/// `.cwasm` file and load it with [`DocumentScript::attach`], which `deserialize`s
/// with codegen off. Trusted / first-party components only (`deserialize` trusts the
/// bytes). The blob is a per-target build artifact — never commit it.
pub fn precompile_to_cwasm(component_path: &Path) -> wasmtime::Result<Vec<u8>> {
    let engine = guarded_engine()?;
    let component = Component::from_file(&engine, component_path)?;
    component.serialize()
}

pub(crate) async fn build_instance(
    engine: &Engine,
    component_path: &Path,
    dom: ScriptedDom,
    grant: &Grant,
    limits: StoreLimits,
) -> wasmtime::Result<(Store<ScriptHost>, DocumentCore)> {
    let component = load_component(engine, component_path)?;
    let mut linker = Linker::new(engine);
    link_with_grant(&mut linker, grant)?;
    let mut store = Store::new(engine, new_host(dom, grant.granted_names(), limits));
    // Harmless when `limits` is the unlimited default; enforces the cap otherwise.
    store.limiter(|h| &mut h.limits);
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
    let (mut store, bindings) =
        build_instance(&engine, component_path, seed_dom(), grant, StoreLimits::default()).await?;
    bindings.call_activate(&mut store).await?;
    Ok(())
}

/// Default per-turn `net.fetch` budget (§A6). Generous for a real turn (a handful of
/// API calls), a hard ceiling on a fetch flood. Per *turn*, reset each `handle-event`,
/// so total egress is bounded by this times the host-paced turn rate.
const DEFAULT_MAX_FETCHES_PER_TURN: u32 = 32;

/// Per-turn resource quota for a [`DocumentScript`] (§11.7-6: fixed constants for
/// P2). `mem_bytes` caps the instance's linear memory; `epoch_deadline_ticks` is
/// how many 5ms watchdog ticks a single guest call may run before it is trapped;
/// `max_fetches_per_turn` caps `net.fetch` calls within one turn (§A6).
#[derive(Clone, Copy, Debug)]
pub struct Quota {
    pub mem_bytes: usize,
    pub epoch_deadline_ticks: u64,
    pub max_fetches_per_turn: u32,
}

impl Default for Quota {
    fn default() -> Self {
        // ~64 MiB and ~1s wall (200 * 5ms): generous for a turn, fatal to a runaway.
        Self {
            mem_bytes: 64 * 1024 * 1024,
            epoch_deadline_ticks: 200,
            max_fetches_per_turn: DEFAULT_MAX_FETCHES_PER_TURN,
        }
    }
}

/// The outcome of delivering one event to a [`DocumentScript`]. The script's batch
/// (if any) has already been applied to the live DOM; this reports what happened
/// without leaking the WIT types to the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    /// The batch applied (or was an empty no-op); carries the resulting revision.
    Applied(u64),
    /// Optimistic-concurrency conflict: the script cited a stale revision. Carries
    /// the current revision to rebase against.
    Conflict(u64),
    /// A mutation referenced an unknown / out-of-scope node id; nothing applied.
    UnknownNode(u64),
    /// The script declined the event, or the host refused the batch. Carries why.
    Refused(String),
}

/// Run `fut` to completion under an epoch watchdog: a thread bumps `engine`'s epoch
/// every 5ms so a guest call that exceeds its `set_epoch_deadline` is trapped. The
/// thread is stopped and joined before returning, so nothing spins between turns.
fn guarded_block_on<T>(
    engine: Engine,
    fut: impl std::future::Future<Output = wasmtime::Result<T>>,
) -> wasmtime::Result<T> {
    let stop = Arc::new(AtomicBool::new(false));
    let watch_stop = stop.clone();
    let watchdog = std::thread::spawn(move || {
        while !watch_stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
            engine.increment_epoch();
        }
    });
    let result = pollster::block_on(fut);
    stop.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    result
}

/// A live DocumentScript attached to a caller-provided page [`ScriptedDom`] — the
/// P2.5 seam the content actor consumes. It owns the wasm instance plus the page
/// DOM (which the script mutates) for the attachment's lifetime; the caller renders
/// from [`dom`](Self::dom) and reads [`revision`](Self::revision) to gate
/// re-renders. Every guest call runs under the epoch + `StoreLimits` guards (§11.2),
/// so a runaway or memory-bomb script is contained and the host survives. A sync
/// surface (drives the async exports via `pollster`) for the sync content-actor
/// thread.
pub struct DocumentScript {
    store: Store<ScriptHost>,
    bindings: DocumentCore,
    epoch_deadline_ticks: u64,
}

impl DocumentScript {
    /// Attach `component_path` to a live page `dom` under `grant` + `quota`, running
    /// `activate`. Fails if a required capability is denied (instantiation fails) or
    /// `activate` traps. The page DOM moves into the instance; recover the
    /// (script-mutated) DOM with [`detach`](Self::detach).
    pub fn attach(
        component_path: &Path,
        dom: ScriptedDom,
        grant: &Grant,
        quota: Quota,
        fetcher: Option<std::sync::Arc<dyn NetFetcher>>,
        net_origins: Vec<String>,
    ) -> wasmtime::Result<Self> {
        let engine = guarded_engine()?;
        let limits = StoreLimitsBuilder::new().memory_size(quota.mem_bytes).build();
        let (mut store, bindings) =
            pollster::block_on(build_instance(&engine, component_path, dom, grant, limits))?;
        // The network backend for `net.fetch` (None = unconfigured -> fetch errors)
        // plus the origin allowlist gating which URLs it may reach (§E1) and the
        // per-turn fetch cap (§A6).
        store.data_mut().fetcher = fetcher;
        store.data_mut().net_origins = net_origins;
        store.data_mut().max_fetches_per_turn = quota.max_fetches_per_turn;
        store.set_epoch_deadline(quota.epoch_deadline_ticks);
        let engine = store.engine().clone();
        let activate = bindings.call_activate(&mut store);
        guarded_block_on(engine, activate)?;
        Ok(Self { store, bindings, epoch_deadline_ticks: quota.epoch_deadline_ticks })
    }

    /// Deliver one event under the guards; apply the returned batch to the live DOM;
    /// report the outcome. `Err` means the guest call **trapped** (epoch-cancelled
    /// runaway or memory-bomb) — the host survives and the DOM is unchanged; the
    /// caller should detach the script.
    pub fn deliver_event(&mut self, kind: &str, payload: &str) -> wasmtime::Result<TurnOutcome> {
        let ev = Event { kind: kind.to_string(), payload: payload.to_string() };
        self.store.set_epoch_deadline(self.epoch_deadline_ticks);
        self.store.data_mut().fetches_this_turn = 0; // fresh per-turn fetch budget (§A6)
        let engine = self.store.engine().clone();
        let turn = self.bindings.call_handle_event(&mut self.store, &ev);
        let result = guarded_block_on(engine, turn)?;
        Ok(match result {
            Ok(batch) => {
                let h = self.store.data_mut();
                match dom_view::apply(&mut h.dom, &mut h.revision, batch) {
                    Ok(rev) => TurnOutcome::Applied(rev),
                    Err(TurnError::RevisionConflict(c)) => TurnOutcome::Conflict(c),
                    Err(TurnError::UnknownNode(id)) => TurnOutcome::UnknownNode(id),
                    Err(TurnError::Refused(w)) => TurnOutcome::Refused(w),
                }
            }
            Err(TurnError::RevisionConflict(c)) => TurnOutcome::Conflict(c),
            Err(TurnError::UnknownNode(id)) => TurnOutcome::UnknownNode(id),
            Err(TurnError::Refused(w)) => TurnOutcome::Refused(w),
        })
    }

    /// The live (possibly script-mutated) page DOM, for the caller to render.
    pub fn dom(&self) -> &ScriptedDom {
        &self.store.data().dom
    }

    /// The current host-side revision (bumped on every applied batch). The caller
    /// gates a re-render on this changing.
    pub fn revision(&self) -> u64 {
        self.store.data().revision
    }

    /// Run `deactivate` (guarded) and return the page DOM to the caller. A trap in
    /// `deactivate` is reported as `Err`, but the DOM is still recovered on success.
    pub fn detach(mut self) -> wasmtime::Result<ScriptedDom> {
        self.store.set_epoch_deadline(self.epoch_deadline_ticks);
        let engine = self.store.engine().clone();
        let deactivate = self.bindings.call_deactivate(&mut self.store);
        guarded_block_on(engine, deactivate)?;
        Ok(self.store.into_data().dom)
    }
}
