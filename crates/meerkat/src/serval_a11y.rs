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
//! chrome advertises its affordances, and [`chrome_a11y_tree`] hands back the
//! actionable nodes so the host can route a screen reader's request to the chrome's
//! activation paths. (Grab-bag G2.4.)
//!
//! The route stores the whole `NodeId` (`A11yHostAction::ChromeNode`, keyed by
//! `chrome_a11y_id(node)`), the way the orrery's `SelectNodeByUrl` routes do —
//! *not* the salted id reversed back to a `NodeId`. The reversal is debug-broken:
//! a chrome node's a11y id is `CHROME_A11Y_SALT | node.raw()`, but on 64-bit debug
//! builds `raw()` packs a process-unique doc-tag into the same high bits the salt
//! uses, so `& !SALT` cannot recover the tag (it works only in release, where the
//! doc-tag fence compiles out). Storing the node whole sidesteps the id entirely.

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

/// The CSS classes of the shell document's folded-pane wrappers — the positioned subtrees
/// (the orrery + roster + the four list panes) the frame tree now projects with rich,
/// actionable a11y (`orrery_a11y_tree` / `roster_a11y_tree` / `list_pane_a11y_tree`). The
/// chrome walk skips these so each pane appears once in the stitched tree, via its frame-tree
/// projection, not doubled as inert divs here (for the orrery, the bare `.node-card` divs the
/// walk would otherwise emit as unlabeled containers). (Phase 1, step 3b; orrery: slice 4.)
const FOLDED_PANE_WRAPPERS: &[&str] = &[
    "orrery",
    "roster-pane",
    "apparatus-pane",
    "steward-pane",
    "inspector-pane",
    "trail-pane",
];

/// Whether `node` is a folded-pane wrapper the chrome walk should skip (the frame tree owns
/// its a11y). (Phase 1, step 3b.)
fn is_folded_pane(dom: &ScriptedDom, node: NodeId) -> bool {
    dom.attributes(node).any(|attr| {
        attr.name.local.as_ref() == "class"
            && attr.value.split_whitespace().any(|c| FOLDED_PANE_WRAPPERS.contains(&c))
    })
}

/// Build the a11y node for `node` (whose parent sits at `parent_origin` in
/// absolute coords), append it + its element descendants to `out`, record it in
/// `actionable` if it advertises a host action, and return its id. Bounds
/// accumulate down the tree (taffy locations are parent-relative).
fn build(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
    node: NodeId,
    parent_origin: (f64, f64),
    out: &mut Vec<(AccessNodeId, Node)>,
    actionable: &mut Vec<NodeId>,
) -> AccessNodeId {
    let id = chrome_a11y_id(node);
    let role = chrome_role(dom, node);
    let mut access = Node::new(role);
    // Declare the host action a screen reader can invoke on this control, so the
    // chrome is *actionable*: a button takes a `Click`, a text field a `Focus`. The
    // node is recorded in `actionable` so the host can route the request back to
    // the chrome's activation paths (`A11yHostAction::ChromeNode`, keyed by this
    // node's `chrome_a11y_id`). (G2.4.)
    let action = match role {
        Role::Button => Some(Action::Click),
        Role::TextInput => Some(Action::Focus),
        _ => None,
    };
    if let Some(action) = action {
        access.add_action(action);
        actionable.push(node);
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
        if dom.kind(child) == NodeKind::Element && !is_folded_pane(dom, child) {
            children.push(build(dom, fragments, child, origin, out, actionable));
        }
    }
    access.set_children(children);

    out.push((id, access));
    id
}

/// Project the chrome `dom` into a [`UxTree`] using `fragments` (the chrome
/// session's retained layout) for node geometry, returning the tree paired with
/// the chrome nodes that advertise a host action (buttons, fields). The document
/// is the subtree root ([`Role::Window`]); every element below becomes a node with
/// a role, accessible name, bounds, and its element children. The a11y bridge
/// stitches the tree under the host window; the host turns each actionable node
/// into an [`A11yHostAction::ChromeNode`](crate::A11yHostAction::ChromeNode) route
/// keyed by its [`chrome_a11y_id`].
pub(crate) fn chrome_a11y_tree(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
) -> (UxTree, Vec<NodeId>) {
    let root = dom.document();
    let mut nodes = Vec::new();
    let mut actionable = Vec::new();
    build(dom, fragments, root, (0.0, 0.0), &mut nodes, &mut actionable);
    (
        UxTree {
            root: chrome_a11y_id(root),
            nodes,
        },
        actionable,
    )
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
        let (tree, actionable) = chrome_a11y_tree(&dom, &frags);

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

        // Controls declare the host action a screen reader invokes. (G2.4.)
        assert!(node(button).supports_action(Action::Click), "the button advertises Click");
        assert!(node(input).supports_action(Action::Focus), "the input advertises Focus");

        // The actionable controls are handed back for host routing — exactly the
        // button + input, never the container or document. The host keys each into
        // an `A11yHostAction::ChromeNode` route by its `chrome_a11y_id`. (G2.4.)
        assert_eq!(actionable.len(), 2, "only the button + input are actionable");
        assert!(actionable.contains(&button), "the button is routed");
        assert!(actionable.contains(&input), "the input is routed");
        assert!(!actionable.contains(&root), "the document is not actionable");
        assert!(!actionable.contains(&bar), "the container is not actionable");

        // Every id sits in the salted chrome range, disjoint from path hashes.
        assert!(
            tree.nodes.iter().all(|(id, _)| id.0 & CHROME_A11Y_SALT == CHROME_A11Y_SALT),
            "chrome a11y ids are salted out of the path-hash space",
        );
    }
}
