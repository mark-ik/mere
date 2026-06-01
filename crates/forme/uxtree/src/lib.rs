/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # UxTree
//!
//! Portable a11y / automation tree for the
//! [`mere`](https://crates.io/crates/mere) browser.
//!
//! Projects mere's portable structural elements ([`inker::EngineDocument`]
//! today; `platen` workbenches and `verso-core` panes once those land) into
//! a tree of [AccessKit](https://accesskit.dev) `Node`s with stable,
//! deterministic [`NodeId`]s. The same projection feeds screen-reader
//! a11y, automation tooling, and inspector overlays.
//!
//! ## Stable identifiers
//!
//! Every projected node gets an [`accesskit::NodeId`] derived from a domain
//! path. The same document always projects to the same IDs — automation
//! and tests can pin to a specific node without relying on bounds or
//! render order. See [`node_id_for_path`].
//!
//! ## What this crate does NOT own
//!
//! - **Bounds**: gpui (or whichever host) fills `Node::set_bounds` after
//!   layout; the projection here is structural only.
//! - **Event synthesis**: the host owns input. Automation tools build
//!   on top of this tree but route events through host-specific paths.
//! - **Host coupling**: this crate depends on `accesskit` and `inker`,
//!   nothing else. No gpui types here.

#![doc(html_root_url = "https://docs.rs/uxtree/0.0.1")]

use std::hash::{Hash, Hasher};

use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use inker::{DocumentBlock, EngineDocument, InlineSpan, inline_text};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// A built tree of AccessKit nodes — the result of projecting a portable
/// mere structural element into AccessKit's data model.
///
/// `nodes` is the flat list of `(id, node)` pairs in insertion order;
/// `root` is the entry point. Order is deterministic: the root is always
/// pushed last, after all its descendants, so consumers can build an
/// `accesskit::TreeUpdate` by zipping `nodes` directly.
pub struct UxTree {
    pub root: NodeId,
    pub nodes: Vec<(NodeId, Node)>,
}

impl UxTree {
    /// Convert this tree into an [`accesskit::TreeUpdate`] suitable for
    /// pushing to AccessKit's platform adapters
    /// (`accesskit_windows::Adapter`, `accesskit_unix::Adapter`,
    /// `accesskit_macos::Adapter`) — the same shape accesskit_winit
    /// would emit if the host were winit-based.
    ///
    /// The host provides `focus` (the currently-focused node) so OS a11y
    /// APIs can announce the active element. `None` means no focused
    /// node — useful for inspector dumps and tests.
    ///
    /// Note on OS integration: gpui doesn't currently expose window
    /// handles publicly, so `accesskit_windows::Adapter::with_hwnd`
    /// and equivalents on other platforms can't be constructed without
    /// a gpui patch. Producing the [`TreeUpdate`] is the foundation;
    /// pushing it to the OS is a separate piece of work.
    pub fn to_tree_update(&self, focus: Option<NodeId>) -> TreeUpdate {
        TreeUpdate {
            nodes: self.nodes.iter().map(|(id, n)| (*id, n.clone())).collect(),
            tree: Some(Tree::new(self.root)),
            tree_id: TreeId::ROOT,
            focus: focus.unwrap_or(self.root),
        }
    }
}

/// Hash a domain path to a stable [`NodeId`]. The same path always
/// produces the same id — automation and snapshot tests can pin against
/// it across runs.
pub fn node_id_for_path(path: &str) -> NodeId {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    NodeId(hasher.finish())
}

/// Stitch a set of subtrees under a new root. The host typically calls
/// this once per frame to merge mere-domain projections (workbench,
/// orrery, gloss, apparatus, …) into a single application-root tree
/// for accesskit consumption.
///
/// `root_path` produces the stable id for the new root via
/// [`node_id_for_path`]. `root` is the AccessKit node to install at the
/// root (caller chooses role + label + any other properties); its
/// `children` are overwritten with the subtree roots.
pub fn stitch(root_path: &str, root: Node, subtrees: Vec<UxTree>) -> UxTree {
    let root_id = node_id_for_path(root_path);
    let mut nodes = Vec::new();
    let mut child_ids = Vec::with_capacity(subtrees.len());
    for sub in subtrees {
        child_ids.push(sub.root);
        nodes.extend(sub.nodes);
    }
    let mut root = root;
    root.set_children(child_ids);
    nodes.push((root_id, root));
    tracing::debug!(
        root_path,
        subtree_count = child_ids_len(&nodes, root_id),
        node_count = nodes.len(),
        "stitched uxtree subtrees under host root"
    );
    UxTree {
        root: root_id,
        nodes,
    }
}

fn child_ids_len(nodes: &[(NodeId, Node)], root_id: NodeId) -> usize {
    nodes
        .iter()
        .find(|(id, _)| *id == root_id)
        .map(|(_, n)| n.children().len())
        .unwrap_or(0)
}

/// Project an [`EngineDocument`] into an [`UxTree`] rooted at a
/// `Role::Document` node. Children come from the document's blocks; inline
/// spans are flattened into the parent block's accessible name today
/// (with `Role::Link` children as the one exception, since links are
/// interactive and need addressable identity).
pub fn project_document(doc: &EngineDocument) -> UxTree {
    let mut nodes = Vec::new();
    let root_path = engine_root_path(&doc.address);
    let root_id = node_id_for_path(&root_path);

    let mut child_ids = Vec::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        let path = format!("{root_path}#blocks/{i}");
        let id = project_block(block, &path, &mut nodes);
        child_ids.push(id);
    }

    let mut root = Node::new(Role::Document);
    if let Some(title) = &doc.title {
        root.set_label(title.clone());
    }
    if let Some(lang) = &doc.lang {
        root.set_language(lang.clone());
    }
    root.set_children(child_ids);

    nodes.push((root_id, root));
    tracing::debug!(
        address = %doc.address,
        node_count = nodes.len(),
        "projected EngineDocument into UxTree"
    );
    UxTree {
        root: root_id,
        nodes,
    }
}

fn engine_root_path(address: &str) -> String {
    format!("engine:{address}")
}

fn project_block(block: &DocumentBlock, path: &str, nodes: &mut Vec<(NodeId, Node)>) -> NodeId {
    let id = node_id_for_path(path);
    let node = match block {
        DocumentBlock::Heading { level, spans } => {
            let mut n = Node::new(Role::Heading);
            n.set_label(inline_text(spans));
            n.set_level(*level as usize);
            attach_link_children(spans, path, "heading-link", nodes, &mut n);
            n
        }
        DocumentBlock::Paragraph { spans } => {
            let mut n = Node::new(Role::Paragraph);
            n.set_label(inline_text(spans));
            attach_link_children(spans, path, "paragraph-link", nodes, &mut n);
            n
        }
        DocumentBlock::CodeBlock { language, text } => {
            // AccessKit 0.24 uses one Role::Code for both inline and block
            // code; block context is implicit from the parent.
            let mut n = Node::new(Role::Code);
            n.set_value(text.clone());
            if let Some(lang) = language {
                // Programming-language hint, not natural-language. Stored
                // as an annotation; AccessKit has no dedicated role
                // property for this, so we use the description.
                n.set_description(format!("language: {lang}"));
            }
            n
        }
        DocumentBlock::Quote { blocks } => {
            let mut n = Node::new(Role::Blockquote);
            let mut child_ids = Vec::with_capacity(blocks.len());
            for (i, inner) in blocks.iter().enumerate() {
                let inner_path = format!("{path}/quote/{i}");
                child_ids.push(project_block(inner, &inner_path, nodes));
            }
            n.set_children(child_ids);
            n
        }
        DocumentBlock::List { ordered, items } => {
            let mut n = Node::new(Role::List);
            if *ordered {
                n.set_description("ordered");
            }
            let mut item_ids = Vec::with_capacity(items.len());
            for (i, item_blocks) in items.iter().enumerate() {
                let item_path = format!("{path}/list/{i}");
                let item_id = node_id_for_path(&item_path);
                let mut item_node = Node::new(Role::ListItem);
                let mut item_children = Vec::with_capacity(item_blocks.len());
                for (j, inner) in item_blocks.iter().enumerate() {
                    let inner_path = format!("{item_path}/item/{j}");
                    item_children.push(project_block(inner, &inner_path, nodes));
                }
                item_node.set_children(item_children);
                nodes.push((item_id, item_node));
                item_ids.push(item_id);
            }
            n.set_children(item_ids);
            n
        }
        DocumentBlock::Image { url, alt } => {
            let mut n = Node::new(Role::Image);
            n.set_label(alt.clone());
            n.set_value(url.clone());
            n
        }
        DocumentBlock::Preformatted { text } => {
            // No dedicated Role::Pre in AccessKit 0.24; reuse Role::Code
            // since the semantic ("verbatim text, preserve whitespace") is
            // the same for screen readers.
            let mut n = Node::new(Role::Code);
            n.set_value(text.clone());
            n
        }
        DocumentBlock::Rule => Node::new(Role::Splitter),
        DocumentBlock::FeedHeader {
            title,
            subtitle,
            summary,
            source_url,
        } => {
            let mut n = Node::new(Role::Section);
            n.set_label(title.clone());
            // Concatenate the optional fields into the description so the
            // a11y projection has a reasonable accessible description even
            // without consuming sub-nodes.
            let mut bits: Vec<String> = Vec::new();
            if let Some(s) = subtitle {
                bits.push(s.clone());
            }
            if let Some(s) = summary {
                bits.push(s.clone());
            }
            if let Some(url) = source_url {
                bits.push(url.clone());
            }
            if !bits.is_empty() {
                n.set_description(bits.join(" — "));
            }
            n
        }
        DocumentBlock::FeedEntry {
            title,
            date,
            summary,
            article_url,
            source_url,
        } => {
            let mut n = Node::new(Role::Article);
            n.set_label(title.clone());
            let mut bits: Vec<String> = Vec::new();
            if let Some(d) = date {
                bits.push(d.clone());
            }
            if let Some(s) = summary {
                bits.push(s.clone());
            }
            if let Some(url) = article_url {
                bits.push(url.clone());
            }
            if let Some(url) = source_url {
                bits.push(url.clone());
            }
            if !bits.is_empty() {
                n.set_description(bits.join(" — "));
            }
            n
        }
        DocumentBlock::MetadataRow { label, value } => {
            // No definition-list role pair in AccessKit 0.24's stable
            // surface; project as a Group whose label is the term and
            // value is the value, so screen readers can announce both.
            let mut n = Node::new(Role::Group);
            n.set_label(label.clone());
            n.set_value(value.clone());
            n
        }
        DocumentBlock::Badge { text } => {
            let mut n = Node::new(Role::Note);
            n.set_label(text.clone());
            n
        }
    };
    nodes.push((id, node));
    id
}

/// Walk inline spans for `Link`s and project each as a `Role::Link` child
/// of the enclosing block. Other inline styling (Code/Emphasis/Strong)
/// stays as text in the parent's accessible name; links are interactive
/// so they need their own addressable identity.
fn attach_link_children(
    spans: &[InlineSpan],
    parent_path: &str,
    tag: &str,
    nodes: &mut Vec<(NodeId, Node)>,
    parent: &mut Node,
) {
    let mut existing = parent.children().to_vec();
    let mut counter = 0usize;
    walk_inline_links(spans, &mut |link_url, link_title, link_spans| {
        let link_path = format!("{parent_path}/{tag}/{counter}");
        let link_id = node_id_for_path(&link_path);
        let mut link_node = Node::new(Role::Link);
        let label = inline_text(link_spans);
        if !label.is_empty() {
            link_node.set_label(label);
        }
        if let Some(t) = link_title {
            link_node.set_description(t.clone());
        }
        link_node.set_value(link_url.to_string());
        nodes.push((link_id, link_node));
        existing.push(link_id);
        counter += 1;
    });
    if !existing.is_empty() {
        parent.set_children(existing);
    }
}

fn walk_inline_links<'a, F>(spans: &'a [InlineSpan], f: &mut F)
where
    F: FnMut(&'a str, Option<&'a String>, &'a [InlineSpan]),
{
    for span in spans {
        match span {
            InlineSpan::Link { url, title, spans, .. } => {
                f(url.as_str(), title.as_ref(), spans.as_slice());
                walk_inline_links(spans, f);
            }
            InlineSpan::Emphasis(inner) | InlineSpan::Strong(inner) => {
                walk_inline_links(inner, f);
            }
            InlineSpan::Text(_)
            | InlineSpan::Code(_)
            | InlineSpan::LineBreak
            | InlineSpan::SoftBreak => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{
        DocumentBlock, DocumentProvenance, DocumentTrustState, EngineDocument, InlineSpan,
    };

    fn doc_with(blocks: Vec<DocumentBlock>) -> EngineDocument {
        EngineDocument {
            address: "doc:test".to_string(),
            title: Some("Test Doc".to_string()),
            content_type: "text/plain".to_string(),
            lang: Some("en".to_string()),
            provenance: DocumentProvenance::default(),
            trust: DocumentTrustState::Unknown,
            diagnostics: Vec::new(),
            blocks,
        }
    }

    #[test]
    fn document_root_is_role_document_with_title_and_lang() {
        let doc = doc_with(vec![]);
        let tree = project_document(&doc);
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Document);
        assert_eq!(root.label(), Some("Test Doc"));
        assert_eq!(root.language(), Some("en"));
    }

    #[test]
    fn ids_are_deterministic() {
        let doc = doc_with(vec![DocumentBlock::Paragraph {
            spans: vec![InlineSpan::Text("hi".to_string())],
        }]);
        let a = project_document(&doc);
        let b = project_document(&doc);
        assert_eq!(a.root, b.root);
        assert_eq!(
            a.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            b.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn heading_carries_level_and_label() {
        let doc = doc_with(vec![DocumentBlock::Heading {
            level: 2,
            spans: vec![InlineSpan::Text("Section".to_string())],
        }]);
        let tree = project_document(&doc);
        let heading = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.role() == Role::Heading)
            .expect("heading node");
        assert_eq!(heading.label(), Some("Section"));
        assert_eq!(heading.level(), Some(2));
    }

    #[test]
    fn list_projects_with_listitem_children() {
        let doc = doc_with(vec![DocumentBlock::List {
            ordered: false,
            items: vec![
                vec![DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("first".to_string())],
                }],
                vec![DocumentBlock::Paragraph {
                    spans: vec![InlineSpan::Text("second".to_string())],
                }],
            ],
        }]);
        let tree = project_document(&doc);
        let list_items = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::ListItem)
            .count();
        assert_eq!(list_items, 2);
    }

    #[test]
    fn to_tree_update_preserves_root_and_node_count() {
        let mut root_node = Node::new(Role::Window);
        root_node.set_label("app");
        let root_id = node_id_for_path("app-root");
        let mut child_node = Node::new(Role::Group);
        child_node.set_label("child");
        let child_id = node_id_for_path("app-root/child");
        root_node.set_children(vec![child_id]);

        let tree = UxTree {
            root: root_id,
            nodes: vec![(child_id, child_node), (root_id, root_node)],
        };
        let update = tree.to_tree_update(None);

        assert_eq!(update.nodes.len(), 2);
        let tree_root = update.tree.as_ref().expect("tree present").root;
        assert_eq!(tree_root, root_id);
        // No focus override → focus defaults to root.
        assert_eq!(update.focus, root_id);

        let with_focus = tree.to_tree_update(Some(child_id));
        assert_eq!(with_focus.focus, child_id);
    }

    #[test]
    fn stitch_combines_subtrees_under_a_new_root() {
        let mut subtree_a_root = Node::new(Role::Group);
        subtree_a_root.set_label("A");
        let subtree_a = UxTree {
            root: node_id_for_path("a"),
            nodes: vec![(node_id_for_path("a"), subtree_a_root)],
        };
        let mut subtree_b_root = Node::new(Role::Group);
        subtree_b_root.set_label("B");
        let subtree_b = UxTree {
            root: node_id_for_path("b"),
            nodes: vec![(node_id_for_path("b"), subtree_b_root)],
        };

        let mut host_root = Node::new(Role::Window);
        host_root.set_label("host");
        let stitched = stitch("host", host_root, vec![subtree_a, subtree_b]);

        let (_, root) = stitched
            .nodes
            .iter()
            .find(|(id, _)| *id == stitched.root)
            .unwrap();
        assert_eq!(root.role(), Role::Window);
        assert_eq!(root.children().len(), 2);
        assert_eq!(stitched.nodes.len(), 3);
    }

    #[test]
    fn link_in_paragraph_projects_as_addressable_child() {
        let doc = doc_with(vec![DocumentBlock::Paragraph {
            spans: vec![
                InlineSpan::Text("see ".to_string()),
                InlineSpan::Link {
                    url: "https://example.test/".to_string(),
                    title: None,
                    spans: vec![InlineSpan::Text("the docs".to_string())],
                    predicate: None,
                },
            ],
        }]);
        let tree = project_document(&doc);
        let link = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.role() == Role::Link)
            .expect("link node");
        assert_eq!(link.label(), Some("the docs"));
        assert_eq!(link.value(), Some("https://example.test/"));
    }
}
