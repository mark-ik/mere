//! The shipped `document-core` guest (transplanted from the P0/P1 probe).
//!
//! A direct-Rust component implementing the per-turn `handle-event` (§10.2): it
//! pulls the snapshot it needs via the `inspect` host import (§5) and returns an
//! atomic, revision-checked batch of id-targeted mutations (§10.3). Node identity
//! is the host's opaque `node-id`; `content-hash` is a change-detection token it
//! reads, never an address. Uses only its granted `log` capability.

wit_bindgen::generate!({
    path: "../wit",
    world: "document-core",
});

use crate::mere::script::document::{
    AppendArgs, Block, DocumentQuery, DocumentView, InsertArgs, Mutation, SetTextArgs, ViewNode,
};
use crate::mere::script::document_host::inspect;
use crate::mere::script::log::log;

struct Component;

fn first_paragraph(view: &DocumentView) -> Option<&ViewNode> {
    view.nodes.iter().find(|n| n.kind == "paragraph")
}

fn root(view: &DocumentView) -> Option<&ViewNode> {
    view.nodes.iter().find(|n| n.parent.is_none())
}

fn para(text: String) -> Block {
    Block { kind: "paragraph".to_string(), text }
}

impl Guest for Component {
    fn activate() {
        log("guest: activated");
    }

    fn deactivate() {
        log("guest: deactivated");
    }

    fn handle_event(ev: Event) -> Result<ApplyBatch, TurnError> {
        let Event { kind, payload } = ev;
        let view = inspect(DocumentQuery::WholeDocument);
        log(&format!(
            "guest: turn '{kind}' at rev {} ({} nodes pulled)",
            view.revision,
            view.nodes.len()
        ));

        let changes = match kind.as_str() {
            "set" => {
                let p = first_paragraph(&view)
                    .ok_or_else(|| TurnError::Refused("no paragraph to edit".to_string()))?;
                vec![Mutation::SetText(SetTextArgs { node: p.id, text: payload })]
            }
            "append" => {
                let r = root(&view).ok_or_else(|| TurnError::Refused("no root".to_string()))?;
                vec![Mutation::AppendChild(AppendArgs { parent: r.id, new: para(payload) })]
            }
            "insert" => {
                let p = first_paragraph(&view)
                    .ok_or_else(|| TurnError::Refused("no paragraph".to_string()))?;
                vec![Mutation::InsertBefore(InsertArgs { reference: p.id, new: para(payload) })]
            }
            "remove" => {
                let last = view
                    .nodes
                    .iter()
                    .rev()
                    .find(|n| n.parent.is_some())
                    .ok_or_else(|| TurnError::Refused("nothing to remove".to_string()))?;
                vec![Mutation::Remove(last.id)]
            }
            "stale" => {
                let p = first_paragraph(&view)
                    .ok_or_else(|| TurnError::Refused("no paragraph".to_string()))?;
                return Ok(ApplyBatch {
                    expected_revision: view.revision.saturating_sub(1),
                    changes: vec![Mutation::SetText(SetTextArgs {
                        node: p.id,
                        text: "stale edit".to_string(),
                    })],
                });
            }
            "bad-id" => vec![Mutation::SetText(SetTextArgs {
                node: u64::MAX,
                text: "ghost".to_string(),
            })],
            "subtree" => {
                if let Some(r) = root(&view) {
                    let sub = inspect(DocumentQuery::Subtree(r.id));
                    log(&format!(
                        "  scoped inspect subtree({}) -> {} of {} nodes",
                        r.id,
                        sub.nodes.len(),
                        view.nodes.len()
                    ));
                }
                vec![]
            }
            other => return Err(TurnError::Refused(format!("unknown event kind: {other}"))),
        };

        Ok(ApplyBatch { expected_revision: view.revision, changes })
    }
}

export!(Component);
