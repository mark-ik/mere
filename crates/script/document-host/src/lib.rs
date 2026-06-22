//! DocumentScript component host (P2.0).
//!
//! The native side of the `document-core` WIT world. P2.0 transplants the proven
//! probe host into a real library: it owns a Wasmtime engine + a per-instance
//! `Store<ScriptHost>`, backs the `log` + `document-host.inspect` imports, and
//! drives the per-turn `handle-event` contract (§10.2) with atomic,
//! revision-checked `apply` (§10.3).
//!
//! Two deliberate choices set the P2 foundation:
//!
//! - **Exports are invoked via `call_async`** (`exports: { default: async }`), so
//!   turns run on a fiber. P2.0 has no async host import, so turns never suspend;
//!   but when the sync-signature `fetch` import lands (plan §11.7-7, fiber async),
//!   a script's `fetch()` will suspend its turn without blocking — no refactor.
//! - The document is backed by an **in-memory `Doc`** here (P2.0). P2.1 swaps this
//!   backing for serval's live `ScriptedDom` behind the same imports.
//!
//! Node identity is the host's opaque `node-id` (the script can only name nodes the
//! host put in a snapshot); the revision counter is host-side, bumped on each
//! applied batch.

use std::path::Path;

/// P2.1: project / mutate a live serval `ScriptedDom` behind the same WIT imports.
pub mod dom_view;

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "wit",
    world: "document-core",
    // Turns run on a fiber so a future sync `fetch` import can suspend them
    // (plan §11.7-7). Host imports stay synchronous.
    exports: { default: async },
});

use crate::mere::script::document::{
    AppendArgs, DocumentQuery, DocumentView, InsertArgs, Mutation, SetTextArgs, ViewNode,
};

/// A node in the host's authoritative document. `id` is the opaque identity the
/// script addresses; assigned once, never reused.
struct DocNode {
    id: u64,
    parent: Option<u64>,
    kind: String,
    text: String,
}

/// The host-owned document (P2.0 in-memory backing; P2.1 → serval `ScriptedDom`).
pub struct Doc {
    nodes: Vec<DocNode>, // document order
    revision: u64,
    next_id: u64,
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

impl Doc {
    /// A root with two paragraphs.
    pub fn new() -> Self {
        Self {
            nodes: vec![
                DocNode { id: 0, parent: None, kind: "root".into(), text: String::new() },
                DocNode { id: 1, parent: Some(0), kind: "paragraph".into(), text: "Intro paragraph.".into() },
                DocNode { id: 2, parent: Some(0), kind: "paragraph".into(), text: "Second paragraph.".into() },
            ],
            revision: 0,
            next_id: 3,
        }
    }

    /// `(id, kind, text)` per node, document order — for assertions / inspection.
    pub fn rows(&self) -> Vec<(u64, String, String)> {
        self.nodes.iter().map(|n| (n.id, n.kind.clone(), n.text.clone())).collect()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn view_node(&self, n: &DocNode) -> ViewNode {
        ViewNode {
            id: n.id,
            parent: n.parent,
            kind: n.kind.clone(),
            text: n.text.clone(),
            content_hash: content_hash(&n.kind, &n.text),
        }
    }

    fn snapshot(&self) -> DocumentView {
        DocumentView {
            revision: self.revision,
            nodes: self.nodes.iter().map(|n| self.view_node(n)).collect(),
        }
    }

    fn snapshot_subtree(&self, root: u64) -> DocumentView {
        let ids = self.subtree(root);
        DocumentView {
            revision: self.revision,
            nodes: self
                .nodes
                .iter()
                .filter(|n| ids.contains(&n.id))
                .map(|n| self.view_node(n))
                .collect(),
        }
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// `id` plus every node reachable downward from it.
    fn subtree(&self, id: u64) -> Vec<u64> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let p = out[i];
            for n in &self.nodes {
                if n.parent == Some(p) && !out.contains(&n.id) {
                    out.push(n.id);
                }
            }
            i += 1;
        }
        out
    }

    /// Apply a batch atomically against its cited revision. Validates every
    /// referenced id before mutating; an empty batch is a no-op that does not
    /// advance the revision. Returns the new (or unchanged) revision.
    fn apply(&mut self, batch: ApplyBatch) -> Result<u64, TurnError> {
        if batch.expected_revision != self.revision {
            return Err(TurnError::RevisionConflict(self.revision));
        }
        for m in &batch.changes {
            let referenced = match m {
                Mutation::SetText(a) => Some(a.node),
                Mutation::Remove(id) => Some(*id),
                Mutation::InsertBefore(a) => Some(a.reference),
                Mutation::AppendChild(a) => Some(a.parent),
            };
            if let Some(id) = referenced {
                if self.index_of(id).is_none() {
                    return Err(TurnError::UnknownNode(id));
                }
            }
        }
        if batch.changes.is_empty() {
            return Ok(self.revision);
        }
        for m in batch.changes {
            self.apply_one(m);
        }
        self.revision += 1;
        Ok(self.revision)
    }

    fn apply_one(&mut self, m: Mutation) {
        match m {
            Mutation::SetText(SetTextArgs { node, text }) => {
                if let Some(i) = self.index_of(node) {
                    self.nodes[i].text = text;
                }
            }
            Mutation::Remove(id) => {
                let gone = self.subtree(id);
                self.nodes.retain(|n| !gone.contains(&n.id));
            }
            Mutation::InsertBefore(InsertArgs { reference, new }) => {
                if let Some(i) = self.index_of(reference) {
                    let parent = self.nodes[i].parent;
                    let id = self.mint();
                    self.nodes.insert(i, DocNode { id, parent, kind: new.kind, text: new.text });
                }
            }
            Mutation::AppendChild(AppendArgs { parent, new }) => {
                let id = self.mint();
                self.nodes.push(DocNode { id, parent: Some(parent), kind: new.kind, text: new.text });
            }
        }
    }

    fn mint(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

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

/// The per-instance store data: the document, the WASI floor the std guest needs,
/// and a sink capturing the guest's `log` output.
pub struct ScriptHost {
    doc: Doc,
    logs: Vec<String>,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl crate::mere::script::log::Host for ScriptHost {
    fn log(&mut self, message: String) {
        self.logs.push(message);
    }
}

/// The `inspect` capability: the guest calls this back during a turn to pull a
/// (scoped) copied snapshot. Only nodes the host put in the view are nameable.
impl crate::mere::script::document_host::Host for ScriptHost {
    fn inspect(&mut self, query: DocumentQuery) -> DocumentView {
        match query {
            DocumentQuery::WholeDocument => self.doc.snapshot(),
            DocumentQuery::Subtree(id) => self.doc.snapshot_subtree(id),
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

/// Instantiate the `document-core` component at `component_path`, `activate` it,
/// drive `(kind, payload)` turns applying each returned batch against the live
/// revision, `deactivate`, and report. Async because exports are invoked via
/// `call_async` (the fiber foundation for a future suspending `fetch`).
pub async fn run_turns(
    component_path: &Path,
    turns: &[(&str, &str)],
) -> wasmtime::Result<TurnLog> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, component_path)?;

    let mut linker = Linker::new(&engine);
    // The store is async (exports run on fibers), so WASI is linked async.
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    // Host imports are synchronous host fns (they run on the async store without
    // suspending); only a future `fetch` would be linked async.
    crate::mere::script::log::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(&mut linker, |s| s)?;
    crate::mere::script::document_host::add_to_linker::<ScriptHost, HasSelf<ScriptHost>>(
        &mut linker,
        |s| s,
    )?;

    let mut store = Store::new(
        &engine,
        ScriptHost {
            doc: Doc::new(),
            logs: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
        },
    );

    let bindings = DocumentCore::instantiate_async(&mut store, &component, &linker).await?;
    bindings.call_activate(&mut store).await?;

    let mut outcomes = Vec::new();
    for (kind, payload) in turns {
        let rev = store.data().doc.revision;
        let ev = Event { kind: (*kind).to_string(), payload: (*payload).to_string() };
        match bindings.call_handle_event(&mut store, &ev).await? {
            Ok(batch) => {
                let cited = batch.expected_revision;
                match store.data_mut().doc.apply(batch) {
                    Ok(new_rev) if new_rev == rev => {
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

    let data = store.data();
    Ok(TurnLog {
        outcomes,
        guest_logs: data.logs.clone(),
        final_rows: data.doc.rows(),
        final_revision: data.doc.revision(),
    })
}
