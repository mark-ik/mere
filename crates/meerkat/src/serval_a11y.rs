/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ScriptedDom` → [`UxTree`]: the chrome's accessibility subtree, derived from
//! the DOM that renders (cheap-path plan C4c).
//!
//! The chrome a11y used to be a single hand-built placeholder node covering the
//! toolbar band, so a screen reader saw one opaque "Chrome" application node. This
//! walks the live chrome `ScriptedDom` instead, mapping each element to an
//! accessibility [`Role`] by tag, taking its accessible name from its direct text,
//! and reading its bounds from the chrome session's retained [`FragmentPlane`] (the
//! same layout that rendered the frame). The a11y bridge stitches the result under
//! the host window beside the content-pane subtrees.
//!
//! Adapted from the former `pelt-live` a11y builder, retargeted from
//! `accesskit::TreeUpdate` to meerkat's `UxTree` (which the bridge converts to a
//! `TreeUpdate` once, after stitching). Each control also declares the host action
//! a screen reader can invoke (a button a `Click`, a field a `Focus`), so the
//! chrome advertises its affordances. (Grab-bag G2.4, part 1.)
//!
//! Host *routing* of the resulting `ActionRequest` back to the chrome's activation
//! paths is the follow-up: a chrome node's a11y id is `CHROME_A11Y_SALT |
//! node.raw()`, but on 64-bit debug builds `raw()` packs a doc-tag into the same
//! high bits the salt uses, so the id cannot be reversed to a `NodeId`. Routing
//! must instead store the whole `NodeId` in the projection's `action_routes` (as
//! the orrery's `SelectNodeByUrl` routes do), keyed by `chrome_a11y_id(node)`.

use accesskit::{Action, Node, NodeId as AccessNodeId, Rect, Role};
use layout_dom_api::{LayoutDom, NodeKind};
use serval_layout::FragmentPlane;
use serval_scripted_dom::{NodeId, ScriptedDom};
use uxtree::UxTree;

/// Salt that places chrome a11y ids in a high range distinct from the path-hashed
/// ids the rest of the UxTree uses (content panes, host root, via
/// `uxtree::node_id_for_path`). A chrome node's id is its raw arena index (stable
/// for the document's life) salted into this range — O(1) and allocation-free,
/// unlike re-hashing a path string per node every frame.
const CHROME_A11Y_SALT: u64 = 0xC04E_0000_0000_0000;

/// The a11y id for a chrome DOM node: its raw arena index salted into the chrome
/// range. The bridge's focus points at this for the focused chrome field.
pub(crate) fn chrome_a11y_id(node: NodeId) -> AccessNodeId {
    AccessNodeId(CHROME_A11Y_SALT | node.raw() as u64)
}

/// The accessibility [`Role`] for a chrome DOM node, by kind / element tag. Text
/// nodes never reach here (their content folds into the owner's label).
fn chrome_role(dom: &ScriptedDom, node: NodeId) -> Role {
    match dom.kind(node) {
        NodeKind::Document => Role::Window,
        NodeKind::Element => match dom.element_name(node).map(|q| q.local.as_ref()) {
            Some("button") => Role::Button,
            Some("input") => Role::TextInput,
            Some("p") => Role::Paragraph,
            Some("label") => Role::Label,
            Some("html") => Role::Document,
            _ => Role::GenericContainer,
        },
        _ => Role::GenericContainer,
    }
}

/// The concatenated direct text-child content of `node` — its accessible name (so
/// a `<button>Go</button>` is named `"Go"`, an `<input>` by its buffer).
fn chrome_direct_text(dom: &ScriptedDom, node: NodeId) -> String {
    let mut name = String::new();
    for child in dom.dom_children(node) {
        if dom.kind(child) == NodeKind::Text {
            if let Some(text) = dom.text(child) {
                name.push_str(text);
            }
        }
    }
    name
}

/// Build the a11y node for `node` (whose parent sits at `parent_origin` in
/// absolute coords), append it + its element descendants to `out`, and return its
/// id. Bounds accumulate down the tree (taffy locations are parent-relative).
fn build(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
    node: NodeId,
    parent_origin: (f64, f64),
    out: &mut Vec<(AccessNodeId, Node)>,
) -> AccessNodeId {
    let id = chrome_a11y_id(node);
    let role = chrome_role(dom, node);
    let mut access = Node::new(role);
    // Declare the host action a screen reader can invoke on this control, so the
    // chrome is *actionable*: a button takes a `Click`, a text field a `Focus`.
    // `apply_a11y_request` recovers the node via `chrome_node_from_a11y_id` and
    // routes the request to the chrome's activation paths. (G2.4.)
    match role {
        Role::Button => access.add_action(Action::Click),
        Role::TextInput => access.add_action(Action::Focus),
        _ => {}
    }

    let name = chrome_direct_text(dom, node);
    if !name.is_empty() {
        access.set_label(name);
    }

    // Absolute bounds from the retained layout; a node with no fragment passes its
    // parent's origin through and contributes no bounds.
    let origin = match fragments.rect_of(node) {
        Some(layout) => {
            let x0 = parent_origin.0 + layout.location.x as f64;
            let y0 = parent_origin.1 + layout.location.y as f64;
            access.set_bounds(Rect::new(
                x0,
                y0,
                x0 + layout.size.width as f64,
                y0 + layout.size.height as f64,
            ));
            (x0, y0)
        }
        None => parent_origin,
    };

    let mut children = Vec::new();
    for child in dom.dom_children(node) {
        if dom.kind(child) == NodeKind::Element {
            children.push(build(dom, fragments, child, origin, out));
        }
    }
    access.set_children(children);

    out.push((id, access));
    id
}

/// Project the chrome `dom` into a [`UxTree`] using `fragments` (the chrome
/// session's retained layout) for node geometry. The document is the subtree root
/// ([`Role::Window`]); every element below becomes a node with a role, accessible
/// name, bounds, and its element children. The a11y bridge stitches this under the
/// host window.
pub(crate) fn chrome_a11y_tree(dom: &ScriptedDom, fragments: &FragmentPlane<NodeId>) -> UxTree {
    let root = dom.document();
    let mut nodes = Vec::new();
    build(dom, fragments, root, (0.0, 0.0), &mut nodes);
    UxTree {
        root: chrome_a11y_id(root),
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_dom_api::{LayoutDomMut, LocalName, Namespace, QualName};

    fn html(local: &str) -> QualName {
        QualName::new(
            None,
            Namespace::from("http://www.w3.org/1999/xhtml"),
            LocalName::from(local),
        )
    }

    /// A chrome-shaped DOM (`<input>` + `<button>Go</button>`) projects to roles by
    /// tag, names folded from text, bounds from layout, and ids in the salted chrome
    /// range (so they never collide with the path-hashed content / host ids).
    #[test]
    fn chrome_dom_projects_to_a11y_subtree() {
        let mut dom = ScriptedDom::new();
        let root = dom.document();
        // A toolbar container holds the controls, as the real chrome does — block
        // children laid out under it get fragments (and so a11y bounds).
        let bar = dom.create_element(html("div"));
        dom.append_child(root, bar);
        let input = dom.create_element(html("input"));
        dom.append_child(bar, input);
        let button = dom.create_element(html("button"));
        dom.append_child(bar, button);
        let label = dom.create_text("Go");
        dom.append_child(button, label);

        let frags =
            serval_layout::render(&dom, &["div, input, button { display: block; }"], 400.0, 60.0);
        let tree = chrome_a11y_tree(&dom, &frags);

        let node = |n: NodeId| {
            tree.nodes
                .iter()
                .find(|(id, _)| *id == chrome_a11y_id(n))
                .map(|(_, node)| node)
                .expect("node in chrome a11y tree")
        };

        // Root is the document as a Window; the subtree root id matches.
        assert_eq!(tree.root, chrome_a11y_id(root));
        assert_eq!(node(root).role(), Role::Window);

        // Roles by tag; the button's name folds from its text child.
        assert_eq!(node(input).role(), Role::TextInput);
        assert_eq!(node(button).role(), Role::Button);
        assert_eq!(node(button).label(), Some("Go"));
        assert!(node(button).bounds().is_some(), "a laid-out node has bounds");

        // Controls declare the host action a screen reader invokes. (G2.4 part 1.)
        assert!(node(button).supports_action(Action::Click), "the button advertises Click");
        assert!(node(input).supports_action(Action::Focus), "the input advertises Focus");

        // Every id sits in the salted chrome range, disjoint from path hashes.
        assert!(
            tree.nodes.iter().all(|(id, _)| id.0 & CHROME_A11Y_SALT == CHROME_A11Y_SALT),
            "chrome a11y ids are salted out of the path-hash space",
        );
    }
}
