/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The gloss outline lens: a DOM section listing the graph as a URL-structure
//! outline ([`glossary::outline_rows`]) plus a compact metrics readout — the first
//! DOM gloss section (gloss-outline plan P1). Folded into the shell document like
//! the roster, so its rows hit-test and dispatch through the one shell runner
//! instead of a bespoke Scene hit-test.

use forme::GraphMemberId;
use orrery::NodeState;
use register_theme::chrome::{ChromeTheme, Color32};
use xilem_serval::{AnyView, PointerClick, ServalCtx, ServalElement, clickable, el};

pub type GlossOutlineView = Box<dyn AnyView<GlossOutlineState, (), ServalCtx, ServalElement>>;

#[derive(Clone, Debug, PartialEq)]
pub enum GlossOutlineIntent {
    /// Select + focus the node at this URL — the shared `Orrery::select_by_url`
    /// primitive the minimap and roster's non-additive click both drive.
    Select(String),
}

/// A real graph node backing an outline row: its identity (for `data-member` +
/// keying) and the NODE_SHEET accent (state + selection) every representation of
/// a node takes. `url` drives the click route directly, independent of whether
/// `member` resolved (defensive; in practice both always resolve together).
#[derive(Clone, Debug, PartialEq)]
pub struct GlossOutlineNode {
    pub member: GraphMemberId,
    pub url: String,
    pub state: NodeState,
    pub selected: bool,
}

/// One outline row. `node` is `Some` for a real graph node (a clickable, tinted
/// row) and `None` for a structural path-segment row (plain text, unclickable) —
/// the host-enriched twin of [`glossary::OutlineRow`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossOutlineRow {
    pub depth: usize,
    pub label: String,
    pub node: Option<GlossOutlineNode>,
}

/// The host-built outline projection for one frame: the depth-tagged rows plus the
/// cheap graph metrics for the header readout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossOutlineSnapshot {
    pub rows: Vec<GlossOutlineRow>,
    pub metrics: glossary::GraphMetrics,
}

#[derive(Default)]
pub struct GlossOutlineState {
    pub rows: Vec<GlossOutlineRow>,
    pub metrics: glossary::GraphMetrics,
    pub pending: Vec<GlossOutlineIntent>,
}

/// Indent step (px) per outline depth level, plus a base inset.
const INDENT_BASE: f32 = 10.0;
const INDENT_STEP: f32 = 14.0;

pub fn gloss_outline_view(state: &GlossOutlineState) -> GlossOutlineView {
    let header = metrics_header(&state.metrics);
    let rows: Vec<GlossOutlineView> = if state.rows.is_empty() {
        vec![Box::new(
            el::<_, GlossOutlineState, ()>("div", "nothing to outline yet".to_string())
                .attr("class", "gloss-outline-empty"),
        )]
    } else {
        state.rows.iter().map(outline_row).collect()
    };
    let scroll: GlossOutlineView = Box::new(
        el::<_, GlossOutlineState, ()>("div", rows).attr("class", "gloss-outline-scroll"),
    );
    Box::new(
        el::<_, GlossOutlineState, ()>("div", vec![header, scroll])
            .attr("class", "gloss-outline"),
    )
}

fn metrics_header(m: &glossary::GraphMetrics) -> GlossOutlineView {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let text = format!(
        "{} node{} \u{b7} {} edge{} \u{b7} {} component{} \u{b7} {} orphan{}",
        m.node_count,
        plural(m.node_count),
        m.edge_count,
        plural(m.edge_count),
        m.component_count,
        plural(m.component_count),
        m.orphan_count,
        plural(m.orphan_count),
    );
    Box::new(el::<_, GlossOutlineState, ()>("div", text).attr("class", "gloss-outline-metrics"))
}

fn outline_row(row: &GlossOutlineRow) -> GlossOutlineView {
    let indent = format!("padding-left:{}px", INDENT_BASE + row.depth as f32 * INDENT_STEP);
    let mut class = "gloss-outline-row".to_string();
    if row.node.is_none() {
        class.push_str(" gloss-outline-row-structural");
    }
    if row.node.as_ref().is_some_and(|n| n.selected) {
        class.push_str(" gloss-outline-row-selected");
    }
    let children = outline_row_children(row);
    match &row.node {
        Some(node) => {
            let url = node.url.clone();
            Box::new(clickable(
                el::<_, GlossOutlineState, ()>("div", children)
                    .attr("class", class)
                    .attr("style", indent)
                    .attr("data-member", node.member.to_string()),
                move |st: &mut GlossOutlineState, _: PointerClick| {
                    st.pending.push(GlossOutlineIntent::Select(url.clone()));
                },
            ))
        }
        None => Box::new(
            el::<_, GlossOutlineState, ()>("div", children)
                .attr("class", class)
                .attr("style", indent),
        ),
    }
}

fn outline_row_children(row: &GlossOutlineRow) -> Vec<GlossOutlineView> {
    let mut children: Vec<GlossOutlineView> = Vec::with_capacity(2);
    if let Some(node) = &row.node {
        children.push(Box::new(
            el::<_, GlossOutlineState, ()>("span", ())
                .attr("class", "gloss-outline-dot")
                .attr(
                    "style",
                    format!(
                        "background-color:{}",
                        accent_rgb(node.selected, node.state)
                    ),
                ),
        ));
    }
    children.push(Box::new(
        el::<_, GlossOutlineState, ()>("span", row.label.clone())
            .attr("class", "gloss-outline-label"),
    ));
    children
}

/// The row's accent dot color: the same NODE_SHEET palette every node
/// representation (gnode, workbench tab, minimap square) tints from — selection
/// wins (amber), else the activation state (open green / closed red / idle blue).
/// (Representations carry node identity.)
fn accent_rgb(selected: bool, state: NodeState) -> &'static str {
    if selected {
        return "rgb(232, 150, 40)";
    }
    match state {
        NodeState::Open => "rgb(58, 140, 94)",
        NodeState::Closed => "rgb(166, 72, 72)",
        NodeState::Idle => "rgb(54, 92, 156)",
    }
}

/// The outline's author CSS, themed from the chrome tokens — mirrors `roster_sheet`.
pub fn gloss_outline_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        format!(
            ".gloss-outline {{ position: relative; overflow: hidden; height: 100%; box-sizing: border-box; background-color: {}; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".gloss-outline-metrics {{ display: block; font-size: 10px; color: {}; padding: 4px 10px; }}",
            rgb(c.muted_text)
        ),
        ".gloss-outline-scroll { display: block; overflow: scroll; height: 100%; box-sizing: border-box; padding-bottom: 8px; }".to_string(),
        ".gloss-outline-row { display: flex; align-items: center; gap: 6px; padding: 3px 10px 3px 0; }".to_string(),
        format!(
            ".gloss-outline-row-structural {{ padding-top: 6px; }} .gloss-outline-row-structural .gloss-outline-label {{ font-size: 10px; color: {}; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".gloss-outline-row-selected {{ background-color: {}; }}",
            rgb(c.active_bg)
        ),
        format!(
            ".gloss-outline-label {{ display: inline-block; font-size: 12px; color: {}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}",
            rgb(c.body_text)
        ),
        ".gloss-outline-dot { display: inline-block; width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }".to_string(),
        format!(
            ".gloss-outline-empty {{ display: block; font-size: 12px; color: {}; padding: 8px 10px; }}",
            rgb(c.muted_text)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use layout_dom_api::LayoutDom;
    use register_theme::chrome::ChromeTheme;
    use serval_scripted_dom::{NodeId, ScriptedDom};
    use xilem_serval::PointerClick;

    use super::*;
    use crate::view_pane::ViewPane;

    type GlossOutlineLogic = fn(&GlossOutlineState) -> GlossOutlineView;

    struct OutlinePane {
        pane: ViewPane<GlossOutlineState, GlossOutlineLogic, GlossOutlineView>,
    }

    impl OutlinePane {
        fn new() -> Self {
            Self {
                pane: ViewPane::new(gloss_outline_view as GlossOutlineLogic, GlossOutlineState::default()),
            }
        }

        fn set_snapshot(&mut self, theme: &ChromeTheme, snapshot: GlossOutlineSnapshot) {
            self.pane.set_sheets(gloss_outline_sheet(theme));
            self.pane.update(|s| {
                s.rows = snapshot.rows;
                s.metrics = snapshot.metrics;
            });
        }

        fn dom(&self) -> std::rc::Rc<std::cell::RefCell<ScriptedDom>> {
            self.pane.dom()
        }

        fn take_intents(&mut self) -> Vec<GlossOutlineIntent> {
            let mut out = Vec::new();
            self.pane.update(|s| out = std::mem::take(&mut s.pending));
            out
        }
    }

    fn node(member: u128, url: &str, state: NodeState, selected: bool) -> GlossOutlineNode {
        GlossOutlineNode {
            member: GraphMemberId::from_u128(member),
            url: url.to_string(),
            state,
            selected,
        }
    }

    fn first_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Option<NodeId> {
        if dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        }) {
            return Some(id);
        }
        dom.dom_children(id)
            .find_map(|child| first_by_class(dom, child, class))
    }

    fn count_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> usize {
        let here = usize::from(dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        }));
        here + dom
            .dom_children(id)
            .map(|c| count_by_class(dom, c, class))
            .sum::<usize>()
    }

    #[test]
    fn empty_outline_shows_empty_state() {
        let mut pane = OutlinePane::new();
        pane.set_snapshot(&ChromeTheme::default(), GlossOutlineSnapshot::default());
        let _ = pane.pane.frame(280, 240, &Default::default());
        let dom = pane.dom();
        let dom = dom.borrow();
        assert!(first_by_class(&dom, dom.document(), "gloss-outline-empty").is_some());
    }

    #[test]
    fn renders_structural_and_node_rows() {
        let mut pane = OutlinePane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            GlossOutlineSnapshot {
                rows: vec![
                    GlossOutlineRow { depth: 0, label: "site.test".to_string(), node: None },
                    GlossOutlineRow {
                        depth: 1,
                        label: "Guide".to_string(),
                        node: Some(node(1, "https://site.test/guide", NodeState::Idle, false)),
                    },
                ],
                metrics: glossary::GraphMetrics { node_count: 1, ..Default::default() },
            },
        );
        let _ = pane.pane.frame(280, 240, &Default::default());
        let dom = pane.dom();
        let dom = dom.borrow();
        assert_eq!(count_by_class(&dom, dom.document(), "gloss-outline-row"), 2);
        assert_eq!(count_by_class(&dom, dom.document(), "gloss-outline-row-structural"), 1);
        assert!(first_by_class(&dom, dom.document(), "gloss-outline-metrics").is_some());
    }

    #[test]
    fn clicking_a_node_row_queues_select_by_url() {
        let mut pane = OutlinePane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            GlossOutlineSnapshot {
                rows: vec![GlossOutlineRow {
                    depth: 0,
                    label: "Guide".to_string(),
                    node: Some(node(1, "https://site.test/guide", NodeState::Open, false)),
                }],
                metrics: glossary::GraphMetrics::default(),
            },
        );
        let _ = pane.pane.frame(280, 240, &Default::default());
        let row_node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "gloss-outline-row").expect("a row")
        };
        pane.pane.dispatch_click(row_node, PointerClick::at((20.0, 10.0)));
        assert_eq!(
            pane.take_intents(),
            vec![GlossOutlineIntent::Select("https://site.test/guide".to_string())]
        );
    }
}
