/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ScriptedDom` → [`UxTree`]: the chrome's accessibility subtree.
//!
//! The projection itself is the engine's — [`serval_layout::build_subtree`] walks
//! the live chrome `ScriptedDom`, mapping each element to a [`Role`] by ARIA
//! `role=` then tag, folding its direct text into the accessible name, reading
//! bounds from the chrome session's retained [`FragmentPlane`], and declaring the
//! host action each control accepts (a button a `Click`, a field a `Focus`). This
//! module supplies only the chrome *policy*: the salted id scheme, the folded-pane
//! skip list, and the [`UxTree`] wrap the bridge stitches under the host window
//! beside the content-pane subtrees. Sharing the engine walk keeps the chrome from
//! drifting behind on standards support (it used to miss ARIA roles and toggled
//! state a forked copy never grew). (Grab-bag G2.4.)
//!
//! The route stores the whole `NodeId` (`A11yHostAction::ChromeNode`, keyed by
//! `chrome_a11y_id(node)`), the way the orrery's `SelectNodeByUrl` routes do —
//! *not* the salted id reversed back to a `NodeId`. The reversal is debug-broken:
//! a chrome node's a11y id is `CHROME_A11Y_SALT | node.raw()`, but on 64-bit debug
//! builds `raw()` packs a process-unique doc-tag into the same high bits the salt
//! uses, so `& !SALT` cannot recover the tag (it works only in release, where the
//! doc-tag fence compiles out). Storing the node whole sidesteps the id entirely.

use accesskit::NodeId as AccessNodeId;
use layout_dom_api::LayoutDom;
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

/// The CSS classes of the shell document's folded-pane wrappers — the positioned subtrees
/// (the orrery + roster + the four list panes) the frame tree now projects with rich,
/// actionable a11y (`orrery_a11y_tree` / `roster_a11y_tree` / `list_pane_a11y_tree`). The
/// chrome walk skips these so each pane appears once in the stitched tree, via its frame-tree
/// projection, not doubled as inert divs here (for the orrery, the bare `.gnode` divs the
/// walk would otherwise emit as unlabeled containers). (Phase 1, step 3b; orrery: slice 4.)
const FOLDED_PANE_WRAPPERS: &[&str] = &[
    "orrery",
    "roster-pane",
    "apparatus-pane",
    "steward-pane",
    "inspector-pane",
    "trail-pane",
    "gloss-outline-pane",
    "gloss-recent-pane",
    "gloss-minimap-pane",
];

/// Whether `node` is a folded-pane wrapper the chrome walk should skip (the frame tree owns
/// its a11y). (Phase 1, step 3b.)
fn is_folded_pane(dom: &ScriptedDom, node: NodeId) -> bool {
    dom.attributes(node).any(|attr| {
        attr.name.local.as_ref() == "class"
            && attr
                .value
                .split_whitespace()
                .any(|c| FOLDED_PANE_WRAPPERS.contains(&c))
    })
}

/// Project the chrome `dom` into a [`UxTree`] using `fragments` (the chrome
/// session's retained layout) for node geometry, returning the tree paired with
/// the chrome nodes that advertise a host action (buttons, fields). The engine
/// walk ([`serval_layout::build_subtree`]) does the projection; the chrome supplies
/// the salted id scheme ([`chrome_a11y_id`], so ids stay disjoint from the
/// path-hashed content/host ids) and the folded-pane skip ([`is_folded_pane`], so
/// each pane appears once via its own richer frame-tree projection). The document
/// is the subtree root ([`accesskit::Role::Window`]). The a11y bridge stitches the
/// tree under the host window; the host turns each actionable node into an
/// [`A11yHostAction::ChromeNode`](crate::A11yHostAction::ChromeNode) route keyed by
/// its [`chrome_a11y_id`].
pub(crate) fn chrome_a11y_tree(
    dom: &ScriptedDom,
    fragments: &FragmentPlane<NodeId>,
) -> (UxTree, Vec<NodeId>) {
    let root = dom.document();
    let (nodes, root_id, actionable) = serval_layout::build_subtree(
        dom,
        fragments,
        root,
        &|_dom: &ScriptedDom, node: NodeId| chrome_a11y_id(node),
        &|dom: &ScriptedDom, node: NodeId| is_folded_pane(dom, node),
    );
    (
        UxTree {
            root: root_id,
            nodes,
        },
        actionable,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Role};
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

        let frags = serval_layout::render(
            &dom,
            &["div, input, button { display: block; }"],
            400.0,
            60.0,
        );
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
        assert!(
            node(button).bounds().is_some(),
            "a laid-out node has bounds"
        );

        // Controls declare the host action a screen reader invokes. (G2.4.)
        assert!(
            node(button).supports_action(Action::Click),
            "the button advertises Click"
        );
        assert!(
            node(input).supports_action(Action::Focus),
            "the input advertises Focus"
        );

        // The actionable controls are handed back for host routing — exactly the
        // button + input, never the container or document. The host keys each into
        // an `A11yHostAction::ChromeNode` route by its `chrome_a11y_id`. (G2.4.)
        assert_eq!(
            actionable.len(),
            2,
            "only the button + input are actionable"
        );
        assert!(actionable.contains(&button), "the button is routed");
        assert!(actionable.contains(&input), "the input is routed");
        assert!(
            !actionable.contains(&root),
            "the document is not actionable"
        );
        assert!(
            !actionable.contains(&bar),
            "the container is not actionable"
        );

        // Every id sits in the salted chrome range, disjoint from path hashes.
        assert!(
            tree.nodes
                .iter()
                .all(|(id, _)| id.0 & CHROME_A11Y_SALT == CHROME_A11Y_SALT),
            "chrome a11y ids are salted out of the path-hash space",
        );
    }
}
