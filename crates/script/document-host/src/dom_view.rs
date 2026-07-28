//! P2.1: the genet-coupled adapter. Projects a live `ScriptedDom` into
//! document-core view-nodes (`inspect`) and applies id-targeted mutations back via
//! `LayoutDomMut` (`apply`) — the §11.3 wiring.
//!
//! The document-core view **is** the HTML DOM tree: each element becomes a
//! view-node named by its tag, each text node a `#text` view-node carrying the
//! text. Node identity is genet's `NodeId` round-tripped through the WIT
//! `node-id` (`u64`) via `opaque_id`/`from_raw`; `is_live` validates an id before
//! any mutation (the unknown-node guard), so no host id-map is needed. The
//! revision is host-side (genet's DOM tracks none).

use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, NodeKind, QualName};

use crate::content_hash;
use crate::mere::script::document::{
    AppendArgs, Block, DocumentView, InsertArgs, Mutation, SetTextArgs, ViewNode,
};
use crate::{ApplyBatch, TurnError};

/// Project the whole live document into a copied view stamped with `revision`.
pub fn snapshot(dom: &ScriptedDom, revision: u64) -> DocumentView {
    let mut nodes = Vec::new();
    walk(dom, dom.document(), &mut nodes);
    DocumentView { revision, nodes }
}

/// Project only the subtree rooted at `root_id` (the `subtree` query). An empty
/// view if the id is not live.
pub fn snapshot_subtree(dom: &ScriptedDom, revision: u64, root_id: u64) -> DocumentView {
    let mut nodes = Vec::new();
    let root = NodeId::from_raw(root_id as usize);
    if dom.is_live(root) {
        walk(dom, root, &mut nodes);
    }
    DocumentView { revision, nodes }
}

fn walk(dom: &ScriptedDom, id: NodeId, out: &mut Vec<ViewNode>) {
    out.push(view_node(dom, id));
    for child in dom.dom_children(id) {
        walk(dom, child, out);
    }
}

fn view_node(dom: &ScriptedDom, id: NodeId) -> ViewNode {
    let kind = kind_label(dom, id);
    let text = dom.text(id).unwrap_or("").to_string();
    ViewNode {
        id: dom.opaque_id(id),
        parent: dom.parent(id).map(|p| dom.opaque_id(p)),
        content_hash: content_hash(&kind, &text),
        kind,
        text,
    }
}

/// Elements are named by tag; text nodes are `#text`; anything else by its kind.
fn kind_label(dom: &ScriptedDom, id: NodeId) -> String {
    match dom.kind(id) {
        NodeKind::Element => dom
            .element_name(id)
            .map(|q| q.local.to_string())
            .unwrap_or_else(|| "element".to_string()),
        NodeKind::Text => "#text".to_string(),
        other => format!("#{}", format!("{other:?}").to_lowercase()),
    }
}

/// Apply a batch atomically against the host-side `revision`. Every referenced id
/// is checked live before mutating (unknown-node, nothing applied); an empty batch
/// is a no-op that does not advance the revision. Returns the new (or unchanged)
/// revision.
pub fn apply(
    dom: &mut ScriptedDom,
    revision: &mut u64,
    batch: ApplyBatch,
) -> Result<u64, TurnError> {
    if batch.expected_revision != *revision {
        return Err(TurnError::RevisionConflict(*revision));
    }
    for m in &batch.changes {
        let referenced = match m {
            Mutation::SetText(a) => Some(a.node),
            Mutation::Remove(id) => Some(*id),
            Mutation::InsertBefore(a) => Some(a.reference),
            Mutation::AppendChild(a) => Some(a.parent),
        };
        if let Some(raw) = referenced {
            if !dom.is_live(NodeId::from_raw(raw as usize)) {
                return Err(TurnError::UnknownNode(raw));
            }
        }
    }
    if batch.changes.is_empty() {
        return Ok(*revision);
    }
    for m in batch.changes {
        apply_one(dom, m);
    }
    *revision += 1;
    Ok(*revision)
}

fn apply_one(dom: &mut ScriptedDom, m: Mutation) {
    match m {
        Mutation::SetText(SetTextArgs { node, text }) => {
            dom.set_text(NodeId::from_raw(node as usize), &text);
        }
        Mutation::Remove(id) => {
            dom.remove(NodeId::from_raw(id as usize));
        }
        Mutation::InsertBefore(InsertArgs { reference, new }) => {
            let reference = NodeId::from_raw(reference as usize);
            if let Some(parent) = dom.parent(reference) {
                let child = create_block(dom, new);
                dom.insert_before(parent, child, Some(reference));
            }
        }
        Mutation::AppendChild(AppendArgs { parent, new }) => {
            let parent = NodeId::from_raw(parent as usize);
            let child = create_block(dom, new);
            dom.append_child(parent, child);
        }
    }
}

/// A block becomes one DOM node: `#text` → a text node; otherwise an element named
/// by `kind` (element text content is a separate `#text` block).
fn create_block(dom: &mut ScriptedDom, block: Block) -> NodeId {
    if block.kind == "#text" {
        dom.create_text(&block.text)
    } else {
        dom.create_element(qual(&block.kind))
    }
}

fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build `<body><p>Intro</p><p>Second</p></body>`; return (dom, body, first text node).
    fn seed() -> (ScriptedDom, NodeId, NodeId) {
        let mut dom = ScriptedDom::new();
        let root = dom.document();
        let body = dom.create_element(qual("body"));
        dom.append_child(root, body);
        let p1 = dom.create_element(qual("p"));
        dom.append_child(body, p1);
        let t1 = dom.create_text("Intro");
        dom.append_child(p1, t1);
        let p2 = dom.create_element(qual("p"));
        dom.append_child(body, p2);
        let t2 = dom.create_text("Second");
        dom.append_child(p2, t2);
        (dom, body, t1)
    }

    #[test]
    fn snapshot_projects_the_dom() {
        let (dom, _body, t1) = seed();
        let view = snapshot(&dom, 7);
        assert_eq!(view.revision, 7);
        // document root + body + p + #text + p + #text = 6 nodes.
        assert_eq!(view.nodes.len(), 6);
        let kinds: Vec<&str> = view.nodes.iter().map(|n| n.kind.as_str()).collect();
        assert!(kinds.contains(&"body"));
        assert_eq!(kinds.iter().filter(|k| **k == "p").count(), 2);
        assert_eq!(kinds.iter().filter(|k| **k == "#text").count(), 2);

        let t1_view = view
            .nodes
            .iter()
            .find(|n| n.id == dom.opaque_id(t1))
            .unwrap();
        assert_eq!(t1_view.kind, "#text");
        assert_eq!(t1_view.text, "Intro");
    }

    #[test]
    fn subtree_query_scopes_the_view() {
        let (dom, body, _t1) = seed();
        let body_id = dom.opaque_id(body);
        let view = snapshot_subtree(&dom, 0, body_id);
        // body + p + #text + p + #text = 5 (excludes the document root).
        assert_eq!(view.nodes.len(), 5);
        assert!(view.nodes.iter().all(|n| n.kind != "#document"));
    }

    #[test]
    fn apply_mutates_and_enforces_revision_and_liveness() {
        let (mut dom, body, t1) = seed();
        let mut rev = 0u64;
        let t1_id = dom.opaque_id(t1);
        let body_id = dom.opaque_id(body);

        // set-text on the first text node.
        let r = apply(
            &mut dom,
            &mut rev,
            ApplyBatch {
                expected_revision: 0,
                changes: vec![Mutation::SetText(SetTextArgs {
                    node: t1_id,
                    text: "Edited".into(),
                })],
            },
        );
        assert!(matches!(r, Ok(1)), "set-text should apply: {r:?}");
        assert_eq!(dom.text(NodeId::from_raw(t1_id as usize)), Some("Edited"));

        // append a <p> under body.
        let r = apply(
            &mut dom,
            &mut rev,
            ApplyBatch {
                expected_revision: 1,
                changes: vec![Mutation::AppendChild(AppendArgs {
                    parent: body_id,
                    new: Block {
                        kind: "p".into(),
                        text: String::new(),
                    },
                })],
            },
        );
        assert!(matches!(r, Ok(2)), "append should apply: {r:?}");
        assert_eq!(dom.dom_children(body).count(), 3);

        // stale revision → conflict carrying the current revision.
        let r = apply(
            &mut dom,
            &mut rev,
            ApplyBatch {
                expected_revision: 0,
                changes: vec![Mutation::Remove(t1_id)],
            },
        );
        assert!(
            matches!(r, Err(TurnError::RevisionConflict(2))),
            "stale should conflict: {r:?}"
        );

        // unknown id → unknown-node, nothing applied, revision unchanged.
        let r = apply(
            &mut dom,
            &mut rev,
            ApplyBatch {
                expected_revision: 2,
                changes: vec![Mutation::SetText(SetTextArgs {
                    node: u64::MAX,
                    text: "x".into(),
                })],
            },
        );
        assert!(
            matches!(r, Err(TurnError::UnknownNode(x)) if x == u64::MAX),
            "bad id should be unknown-node: {r:?}"
        );
        assert_eq!(rev, 2, "revision unchanged after rejected batches");
    }
}
