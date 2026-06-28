/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # gloss
//!
//! Gloss domain layer — peripheral content-commentary strip. v0
//! projects an `EngineDocument` outline (heading list).
//!
//! See README for scope.

#![doc(html_root_url = "https://docs.rs/gloss/0.0.1")]

use accesskit::{Node, Role};
use inker::{Block, EngineDocument, inline_text};
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Project the outline (heading list) of an `EngineDocument` into a
/// uxtree subtree rooted at a gloss group.
pub fn project_outline(doc: &EngineDocument) -> UxTree {
    let mut nodes = Vec::new();
    let root_path = format!("gloss/outline/{}", doc.address);
    let root_id = node_id_for_path(&root_path);

    let mut child_ids = Vec::new();
    for (idx, block) in doc.blocks.iter().enumerate() {
        if let Block::Heading { level, spans } = block {
            let path = format!("{root_path}/heading/{idx}");
            let acc_id = node_id_for_path(&path);
            let mut acc_node = Node::new(Role::Heading);
            acc_node.set_label(inline_text(spans));
            acc_node.set_level(*level as usize);
            nodes.push((acc_id, acc_node));
            child_ids.push(acc_id);
        }
    }

    let mut root = Node::new(Role::Group);
    root.set_label("Outline");
    root.set_children(child_ids);
    nodes.push((root_id, root));

    tracing::debug!(
        address = %doc.address,
        heading_count = nodes.len() - 1,
        "projected EngineDocument outline into gloss subtree"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{Block, EngineDocument, InlineSpan};

    fn doc_with_headings() -> EngineDocument {
        EngineDocument {
            address: "doc:test".to_string(),
            title: Some("Test".to_string()),
            content_type: "text/markdown".to_string(),
            lang: None,
            diagnostics: Vec::new(),
            provenance: Default::default(),
            trust: Default::default(),
            blocks: vec![
                Block::Heading {
                    level: 1,
                    spans: vec![InlineSpan::Text("Introduction".to_string())],
                },
                Block::Paragraph {
                    spans: vec![InlineSpan::Text("body".to_string())],
                },
                Block::Heading {
                    level: 2,
                    spans: vec![InlineSpan::Text("Background".to_string())],
                },
                Block::Heading {
                    level: 1,
                    spans: vec![InlineSpan::Text("Architecture".to_string())],
                },
            ],
        }
    }

    #[test]
    fn root_is_outline_group() {
        let tree = project_outline(&doc_with_headings());
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Outline"));
    }

    #[test]
    fn each_heading_projects_with_level() {
        let tree = project_outline(&doc_with_headings());
        let headings: Vec<_> = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Heading)
            .map(|(_, n)| (n.label().unwrap_or("").to_string(), n.level()))
            .collect();
        assert_eq!(headings.len(), 3);
        assert!(headings.contains(&("Introduction".to_string(), Some(1))));
        assert!(headings.contains(&("Background".to_string(), Some(2))));
        assert!(headings.contains(&("Architecture".to_string(), Some(1))));
    }

    #[test]
    fn empty_document_emits_root_only() {
        let doc = EngineDocument {
            address: "doc:empty".to_string(),
            title: None,
            content_type: "text/plain".to_string(),
            lang: None,
            diagnostics: Vec::new(),
            provenance: Default::default(),
            trust: Default::default(),
            blocks: vec![],
        };
        let tree = project_outline(&doc);
        assert_eq!(tree.nodes.len(), 1);
    }
}
