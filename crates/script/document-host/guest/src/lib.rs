//! The `document-core` guest, P2.1b shape: it operates on the real HTML DOM the
//! host now backs `inspect`/`apply` with — elements (named by tag) and `#text`
//! nodes — rather than the P2.0 abstract `Doc` model. Per-turn `handle-event`
//! (§10.2): pull the snapshot via `inspect` (§5), return an atomic id-targeted
//! batch (§10.3). Node identity is the host's opaque `node-id`; `content-hash` is a
//! token it reads, never an address. Uses only its granted `log` capability.

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

fn first<'a>(view: &'a DocumentView, kind: &str) -> Option<&'a ViewNode> {
    view.nodes.iter().find(|n| n.kind == kind)
}

/// A new empty element block named `tag`.
fn element(tag: &str) -> Block {
    Block { kind: tag.to_string(), text: String::new() }
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
            // Edit the first text node's content.
            "set" => {
                let t = first(&view, "#text")
                    .ok_or_else(|| TurnError::Refused("no text node".to_string()))?;
                vec![Mutation::SetText(SetTextArgs { node: t.id, text: payload })]
            }
            // Append a new <p> under <body> (id-relative anchor).
            "append" => {
                let body =
                    first(&view, "body").ok_or_else(|| TurnError::Refused("no body".to_string()))?;
                vec![Mutation::AppendChild(AppendArgs { parent: body.id, new: element("p") })]
            }
            // Insert a new <p> before the first <p> (id-relative anchor).
            "insert" => {
                let p = first(&view, "p")
                    .ok_or_else(|| TurnError::Refused("no paragraph".to_string()))?;
                vec![Mutation::InsertBefore(InsertArgs { reference: p.id, new: element("p") })]
            }
            // Remove <body>'s last child.
            "remove" => {
                let body =
                    first(&view, "body").ok_or_else(|| TurnError::Refused("no body".to_string()))?;
                let last = view
                    .nodes
                    .iter()
                    .filter(|n| n.parent == Some(body.id))
                    .next_back()
                    .ok_or_else(|| TurnError::Refused("body has no children".to_string()))?;
                vec![Mutation::Remove(last.id)]
            }
            // Cite a stale revision → host rejects with the current one (§10.3).
            "stale" => {
                let t = first(&view, "#text")
                    .ok_or_else(|| TurnError::Refused("no text node".to_string()))?;
                return Ok(ApplyBatch {
                    expected_revision: view.revision.saturating_sub(1),
                    changes: vec![Mutation::SetText(SetTextArgs {
                        node: t.id,
                        text: "stale edit".to_string(),
                    })],
                });
            }
            // Unknown id → atomic validation rejects it.
            "bad-id" => vec![Mutation::SetText(SetTextArgs {
                node: u64::MAX,
                text: "ghost".to_string(),
            })],
            // Scoped pull: inspect only the <body> subtree (smaller copy). No-op turn.
            "subtree" => {
                if let Some(body) = first(&view, "body") {
                    let sub = inspect(DocumentQuery::Subtree(body.id));
                    log(&format!(
                        "  scoped inspect subtree({}) -> {} of {} nodes",
                        body.id,
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
