// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! # gloss
//!
//! Host-neutral gloss domain utilities for Mere:
//! - graph-outline snapshot vocabulary
//! - gloss minimap geometry and Scene helpers
//! - `EngineDocument` outline projection into `uxtree`

#![doc(html_root_url = "https://docs.rs/gloss/0.0.1")]

use accesskit::{Node, Role};
use forme::GraphMemberId;
use inker::{Block, EngineDocument, inline_text};
use kernel::graph::Graph;
use netrender::Scene;
use canvas::NodeState;
use register_theme::chrome::{ChromeTheme, Color32};
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Inset (px) of the swatch from the pane edges.
const PAD: f32 = 16.0;
/// Edge length (px) of a minimap node square.
const NODE: f32 = 7.0;
/// Fixed height (px) of the gloss "recent" section.
const RECENT_H: f32 = 110.0;
/// Depth ceiling independent of viewport.
const MAX_OUTLINE_DEPTH: usize = 8;

/// Approximate rendered row / header heights (px) for outline row capping.
pub const OUTLINE_ROW_H: f32 = 22.0;
pub const OUTLINE_HEADER_H: f32 = 18.0;

/// Intent a gloss row queues on click.
#[derive(Clone, Debug, PartialEq)]
pub enum GlossRowIntent {
    Select(String),
}

/// A real graph node backing an outline row.
#[derive(Clone, Debug, PartialEq)]
pub struct GlossOutlineNode {
    pub member: GraphMemberId,
    pub url: String,
    pub state: NodeState,
    pub selected: bool,
}

/// One outline row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossOutlineRow {
    pub depth: usize,
    pub label: String,
    pub node: Option<GlossOutlineNode>,
}

/// Host-neutral outline projection for one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossOutlineSnapshot {
    pub rows: Vec<GlossOutlineRow>,
    pub metrics: glossary::GraphMetrics,
}

/// Cap outline rows to what the current pane height can show.
pub fn cap_outline_rows(
    mut rows: Vec<GlossOutlineRow>,
    available_height: f32,
) -> Vec<GlossOutlineRow> {
    let before = rows.len();
    rows.retain(|r| r.depth <= MAX_OUTLINE_DEPTH);
    let mut hidden = before - rows.len();

    let budget = ((available_height - OUTLINE_HEADER_H).max(0.0) / OUTLINE_ROW_H).floor() as usize;
    let needs_summary = hidden > 0 || rows.len() > budget;
    let visible_budget = if needs_summary {
        budget.saturating_sub(1)
    } else {
        budget
    };
    if rows.len() > visible_budget {
        hidden += rows.len() - visible_budget;
        rows.truncate(visible_budget);
    }
    if needs_summary && budget > 0 {
        rows.push(GlossOutlineRow {
            depth: 0,
            label: format!("+{hidden} more"),
            node: None,
        });
    }
    rows
}

/// Build the gloss outline snapshot for one graph/frame, enriching pure
/// `glossary::outline_rows` output with node state/selection supplied by the
/// caller.
pub fn build_outline_snapshot(
    graph: &Graph,
    mut node_presentation: impl FnMut(GraphMemberId) -> (NodeState, bool),
    available_height: f32,
) -> GlossOutlineSnapshot {
    let rows = glossary::outline_rows(graph)
        .into_iter()
        .map(|row| GlossOutlineRow {
            depth: row.depth,
            label: row.label,
            node: row.url.and_then(|url| {
                let (_, found) = graph.get_node_by_url(&url)?;
                let member = found.id;
                let (state, selected) = node_presentation(member);
                Some(GlossOutlineNode {
                    member,
                    url,
                    state,
                    selected,
                })
            }),
        })
        .collect();
    GlossOutlineSnapshot {
        rows: cap_outline_rows(rows, available_height),
        metrics: glossary::graph_metrics(graph),
    }
}

/// Split the gloss pane rect into minimap / outline / recent sections.
pub fn gloss_sections(rect: [f32; 4]) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let [x0, y0, x1, y1] = rect;
    let total_h = (y1 - y0).max(0.0);
    let recent_h = RECENT_H.min(total_h * 0.5);
    let remaining = total_h - recent_h;
    let minimap_h = (remaining * 0.5).round();
    let minimap_rect = [x0, y0, x1, y0 + minimap_h];
    let outline_rect = [x0, y0 + minimap_h, x1, y1 - recent_h];
    let recent_rect = [x0, y1 - recent_h, x1, y1];
    (minimap_rect, outline_rect, recent_rect)
}

/// A chrome token at `alpha` as a premultiplied `[r, g, b, a]`.
fn rgba(c: Color32, alpha: f32) -> [f32; 4] {
    let [r, g, b, _] = c.to_array();
    [
        r as f32 / 255.0 * alpha,
        g as f32 / 255.0 * alpha,
        b as f32 / 255.0 * alpha,
        alpha,
    ]
}

/// A chrome token as a CSS `rgb(...)` string.
pub fn theme_rgb_css(c: Color32) -> String {
    let [r, g, b, _] = c.to_array();
    format!("rgb({r}, {g}, {b})")
}

/// A minimap node's edge length (px).
pub fn minimap_node_size(selected: bool, size_factor: f32) -> f32 {
    (if selected { NODE + 3.0 } else { NODE }) * size_factor
}

/// The minimap's fit transform.
pub struct MinimapFit {
    scale: f32,
    off_x: f32,
    off_y: f32,
    min_x: f32,
    min_y: f32,
}

impl MinimapFit {
    /// `None` for an empty node set.
    pub fn compute(positions: &[(f32, f32)], w: u32, h: u32) -> Option<Self> {
        if positions.is_empty() {
            return None;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in positions {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        let bbox_w = (max_x - min_x).max(1.0);
        let bbox_h = (max_y - min_y).max(1.0);
        let avail_w = (w as f32 - 2.0 * PAD).max(1.0);
        let avail_h = (h as f32 - 2.0 * PAD).max(1.0);
        let scale = (avail_w / bbox_w).min(avail_h / bbox_h);
        let off_x = PAD + (avail_w - bbox_w * scale) * 0.5;
        let off_y = PAD + (avail_h - bbox_h * scale) * 0.5;
        Some(Self {
            scale,
            off_x,
            off_y,
            min_x,
            min_y,
        })
    }

    pub fn apply(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (
            (x - self.min_x) * self.scale + self.off_x,
            (y - self.min_y) * self.scale + self.off_y,
        )
    }
}

/// Build the minimap's backdrop Scene: edges + signal rings only.
pub fn minimap_backdrop_scene(
    edges: &[((f32, f32), (f32, f32), f32)],
    rings: &[((f32, f32), f32, [f32; 4])],
    w: u32,
    h: u32,
    theme: &ChromeTheme,
) -> Scene {
    let mut scene = Scene::new(w.max(1), h.max(1));

    let edge_color = rgba(theme.muted_text, 0.7);
    for (a, b, weight) in edges {
        let mut path = netrender::ScenePath::new();
        path.move_to(a.0, a.1).line_to(b.0, b.1);
        let width = (1.0 + 0.6 * (*weight - 1.0)).clamp(1.0, 3.0);
        scene.push_shape_stroked(path, edge_color, width);
    }

    const RING_SEGMENTS: usize = 20;
    for (center, factor, color) in rings {
        let r = NODE * *factor;
        let [cr, cg, cb, ca] = *color;
        let premul = [cr * ca, cg * ca, cb * ca, ca];
        let mut path = netrender::ScenePath::new();
        for i in 0..=RING_SEGMENTS {
            let t = (i as f32 / RING_SEGMENTS as f32) * std::f32::consts::TAU;
            let (x, y) = (center.0 + r * t.cos(), center.1 + r * t.sin());
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        scene.push_shape_stroked(path, premul, 1.5);
    }
    scene
}

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
    use inker::InlineSpan;

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

    #[test]
    fn gloss_sections_preserve_top_middle_bottom_order() {
        let (minimap, outline, recent) = gloss_sections([0.0, 0.0, 300.0, 600.0]);
        assert!(minimap[1] <= outline[1]);
        assert!(outline[1] <= recent[1]);
        assert_eq!(minimap[3], outline[1]);
        assert_eq!(outline[3], recent[1]);
    }

    #[test]
    fn cap_outline_rows_adds_summary_when_truncated() {
        let rows = (0..10)
            .map(|idx| GlossOutlineRow {
                depth: 0,
                label: format!("row-{idx}"),
                node: None,
            })
            .collect();
        let capped = cap_outline_rows(rows, OUTLINE_HEADER_H + OUTLINE_ROW_H * 3.0);
        assert_eq!(capped.len(), 3);
        assert_eq!(capped[2].label, "+8 more");
    }
}
