// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! ScriptHost store data + wasmtime host-trait impls and linker setup.

use super::*;

/// The per-instance store data: the live DOM + its host-side revision, the WASI
/// floor the std guest needs, and a sink capturing the guest's `log` output.
pub struct ScriptHost {
    pub(crate) dom: ScriptedDom,
    pub(crate) revision: u64,
    pub(crate) logs: Vec<String>,
    /// The injected network backend for `net.fetch` (§11.7-7). `None` = no backend
    /// (a `net.fetch` call returns an error); set by [`DocumentScript::attach`].
    pub(crate) fetcher: Option<std::sync::Arc<dyn NetFetcher>>,
    /// The origin allowlist this instance's `net.fetch` may reach (§E1 origin-scoped
    /// net): exact hosts or `*.suffix` globs. Empty = reach nothing (a net-granted
    /// script with no declared origins is denied every fetch). The host enforces this
    /// at the `net.fetch` boundary *before* the backend runs, so `net` means "may
    /// reach these origins", not "open internet". Set by [`DocumentScript::attach`].
    pub(crate) net_origins: Vec<String>,
    /// `net.fetch` calls made in the current turn (§A6), reset to 0 at each turn start.
    pub(crate) fetches_this_turn: u32,
    /// The per-turn `net.fetch` cap; a call past it is refused. Set from `Quota` at
    /// [`DocumentScript::attach`]; [`new_host`] seeds the default for the turn drivers.
    pub(crate) max_fetches_per_turn: u32,
    /// The granted application-capability interface names this instance can see via
    /// the always-linked `caps.granted()` import (§11.4). Authoritative copy is the
    /// `Grant`; this is what the script is told it has.
    pub(crate) granted: Vec<String>,
    pub(crate) wasi: WasiCtx,
    pub(crate) table: ResourceTable,
    /// Resource quotas (P2.2). Default = unlimited (run_turns); run_guarded caps it.
    pub(crate) limits: StoreLimits,
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
            return Err(format!(
                "net.fetch: {} is outside this script's net origins",
                req.url
            ));
        }
        // Call the injected backend (netfetcher/errand, supplied by the content actor).
        // Blocking on the turn's fiber: the host thread parks for the I/O, then the
        // fiber resumes with the result. No backend configured = an error to the guest.
        let fetcher = self
            .fetcher
            .as_ref()
            .ok_or_else(|| "net backend not configured".to_string())?;
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
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
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
pub(crate) fn full_linker(engine: &Engine) -> wasmtime::Result<Linker<ScriptHost>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::caps::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
        &mut linker,
        |s| s,
    )?;
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
pub(crate) fn new_host(dom: ScriptedDom, granted: Vec<String>, limits: StoreLimits) -> ScriptHost {
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
        new_host(
            seed_dom(),
            Grant::allow_all().granted_names(),
            StoreLimits::default(),
        ),
    );

    let bindings = DocumentCore::instantiate_async(&mut store, &component, &linker).await?;
    bindings.call_activate(&mut store).await?;

    let mut outcomes = Vec::new();
    for (kind, payload) in turns {
        let ev = Event {
            kind: (*kind).to_string(),
            payload: (*payload).to_string(),
        };
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
                    Err(TurnError::RevisionConflict(cur)) => outcomes.push(format!(
                        "{kind}: revision-conflict (cited {cited}, current {cur})"
                    )),
                    Err(TurnError::UnknownNode(id)) => {
                        outcomes.push(format!("{kind}: unknown-node {id} (nothing applied)"))
                    }
                    Err(TurnError::Refused(why)) => {
                        outcomes.push(format!("{kind}: refused ({why})"))
                    }
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
    let final_rows = view
        .nodes
        .iter()
        .map(|n| (n.id, n.kind.clone(), n.text.clone()))
        .collect();
    Ok(TurnLog {
        outcomes,
        guest_logs: h.logs.clone(),
        final_rows,
        final_revision: h.revision,
    })
}
