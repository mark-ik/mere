// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Capability grants: Guarded/Grant model, grant-scoped linking, engine + component loading.

use super::*;

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
        let ev = Event {
            kind: kind.to_string(),
            payload: String::new(),
        };
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
    /// Derive this world's import grant from a denizen's structural caps —
    /// the wasm grant bridge as **the import-level face of the one grant**
    /// (participant gate B3). Each capability interface maps to a path under
    /// `doc/` (`doc/log`, `doc/document`, `doc/net`); an interface is linked
    /// only when the provider covers its path for the subject, so an
    /// ungranted import fails at instantiation exactly as a hand-written
    /// Deny does. The same derivation shape as the piccolo lane's (B2):
    /// one authority, per-lane faces.
    pub fn from_authority(
        provider: &impl servitor::AuthorityProvider,
        subject: servitor::Subject,
    ) -> Self {
        use servitor::{Cap, Mode};
        // The `doc/` paths are hierarchical scopes, so they cover by segment
        // prefix. A path that fails to parse denies rather than propagating:
        // an unrepresentable capability is one the provider cannot have
        // granted, and this lane is fail-closed by construction.
        let allow = |path: &str, mode: Mode| match Cap::scope(path) {
            Ok(cap) if provider.covers(subject, &cap, mode) => CapPermission::Allow,
            _ => CapPermission::Deny,
        };
        Self {
            log: allow("doc/log", Mode::Write),
            document: allow("doc/document", Mode::Write),
            net: allow("doc/net", Mode::Write),
        }
    }

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
pub(crate) fn link_with_grant(
    linker: &mut Linker<ScriptHost>,
    grant: &Grant,
) -> wasmtime::Result<()> {
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
pub(crate) fn guarded_engine() -> wasmtime::Result<Engine> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    Engine::new(&config)
}

/// Load a `document-core` component: a precompiled `.cwasm` via `deserialize` (the
/// P2.6 AOT path — no Cranelift on the hot path), else a `.wasm` via `from_file`
/// (JIT). `.cwasm` is for trusted first-party bundled components only.
pub(crate) fn load_component(engine: &Engine, path: &Path) -> wasmtime::Result<Component> {
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
    let (mut store, bindings) = build_instance(
        &engine,
        component_path,
        seed_dom(),
        grant,
        StoreLimits::default(),
    )
    .await?;
    bindings.call_activate(&mut store).await?;
    Ok(())
}
