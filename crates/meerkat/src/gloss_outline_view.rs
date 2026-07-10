/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The gloss outline lens: a DOM section listing the graph as a URL-structure
//! outline ([`mere::glossary::outline_rows`]) plus a compact metrics readout — the first
//! DOM gloss section (gloss-outline plan P1). Folded into the shell document like
//! the roster, so its rows hit-test and dispatch through the one shell runner
//! instead of a bespoke Scene hit-test.

use mere::forme::GraphMemberId;
use mere::canvas::NodeState;
use mere::canvas::palette;
use register_theme::chrome::{ChromeTheme, Color32};
use xilem_serval::{AnyView, PointerClick, ServalCtx, ServalElement, clickable, el};

use mere::gloss::{
    GlossOutlineNode, GlossOutlineRow, GlossOutlineSnapshot, GlossRowIntent, OUTLINE_HEADER_H,
    OUTLINE_ROW_H, cap_outline_rows,
};

pub type GlossOutlineView = Box<dyn AnyView<GlossOutlineState, (), ServalCtx, ServalElement>>;

#[derive(Default)]
pub struct GlossOutlineState {
    pub rows: Vec<GlossOutlineRow>,
    pub metrics: mere::glossary::GraphMetrics,
    pub pending: Vec<GlossRowIntent>,
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
    let scroll: GlossOutlineView =
        Box::new(el::<_, GlossOutlineState, ()>("div", rows).attr("class", "gloss-outline-scroll"));
    Box::new(
        el::<_, GlossOutlineState, ()>("div", vec![header, scroll]).attr("class", "gloss-outline"),
    )
}

/// Bare-scale glance readout: node / edge / component counts only. The full breakdown
/// (relation-family histogram, orphan detail, largest-component sizing) lives in
/// apparatus's "Graph" section instead — gloss answers "where am I, how big, is it
/// fragmented" at a glance beside the minimap; apparatus is where you go to diagnose.
/// (gloss-outline plan, metrics surface split settled 2026-07-01.)
fn metrics_header(m: &mere::glossary::GraphMetrics) -> GlossOutlineView {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let text = format!(
        "{} node{} \u{b7} {} link{} \u{b7} {} component{}",
        m.node_count,
        plural(m.node_count),
        m.edge_count,
        plural(m.edge_count),
        m.component_count,
        plural(m.component_count),
    );
    Box::new(el::<_, GlossOutlineState, ()>("div", text).attr("class", "gloss-outline-metrics"))
}

fn outline_row(row: &GlossOutlineRow) -> GlossOutlineView {
    let indent = format!(
        "padding-left:{}px",
        INDENT_BASE + row.depth as f32 * INDENT_STEP
    );
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
                    st.pending.push(GlossRowIntent::Select(url.clone()));
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
                .attr("class", dot_class(node.selected, node.state)),
        ));
    }
    children.push(Box::new(
        el::<_, GlossOutlineState, ()>("span", row.label.clone())
            .attr("class", "gloss-outline-label"),
    ));
    children
}

/// The row's accent dot classes: the base dot plus the state modifier whose rule
/// `var()`s the shared `--node-*` palette. No color is named here, so the dot
/// inherits node identity through the cascade like every other representation
/// (gnode, workbench tab, minimap square). Selection wins over activation state,
/// which `palette::state_slug` enforces once for everyone.
/// (Representations carry node identity.)
fn dot_class(selected: bool, state: NodeState) -> String {
    format!(
        "gloss-outline-dot gloss-outline-dot-{}",
        palette::state_slug(selected, state)
    )
}

/// The outline's author CSS, themed from the chrome tokens — mirrors `roster_sheet`.
pub fn gloss_outline_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        // The panel root carries the `--node-*` palette, so every row's dot below
        // resolves its fill from the cascade rather than an inline color.
        format!(
            ".gloss-outline {{ position: relative; overflow: hidden; height: 100%; box-sizing: border-box; background-color: {}; {} }}",
            rgb(c.panel_bg),
            palette::custom_property_declarations()
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
        ".gloss-outline-dot-idle { background-color: var(--node-idle-bg); }".to_string(),
        ".gloss-outline-dot-open { background-color: var(--node-open-bg); }".to_string(),
        ".gloss-outline-dot-closed { background-color: var(--node-closed-bg); }".to_string(),
        ".gloss-outline-dot-selected { background-color: var(--node-selected-bg); }".to_string(),
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
    use mere::kernel::graph::fixtures::GraphFixtures;

    type GlossOutlineLogic = fn(&GlossOutlineState) -> GlossOutlineView;

    struct OutlinePane {
        pane: ViewPane<GlossOutlineState, GlossOutlineLogic, GlossOutlineView>,
    }

    impl OutlinePane {
        fn new() -> Self {
            Self {
                pane: ViewPane::new(
                    gloss_outline_view as GlossOutlineLogic,
                    GlossOutlineState::default(),
                ),
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

        fn take_intents(&mut self) -> Vec<GlossRowIntent> {
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

    fn plain_row(depth: usize, label: &str) -> GlossOutlineRow {
        GlossOutlineRow {
            depth,
            label: label.to_string(),
            node: Some(node(1, "https://x.test/", NodeState::Idle, false)),
        }
    }

    #[test]
    fn cap_leaves_a_short_list_untouched() {
        let rows = vec![plain_row(0, "a"), plain_row(0, "b")];
        let capped = cap_outline_rows(rows.clone(), 1000.0);
        assert_eq!(capped, rows);
    }

    #[test]
    fn cap_truncates_to_the_row_budget_with_a_summary_row() {
        // ~2 rows' worth of height: budget = floor((44 - 18) / 22) = 1.
        let rows: Vec<_> = (0..5).map(|i| plain_row(0, &i.to_string())).collect();
        let capped = cap_outline_rows(rows, 44.0);
        assert_eq!(
            capped.len(),
            1,
            "budget(1) reserves its one slot for the summary"
        );
        assert_eq!(capped[0].label, "+5 more");
        assert!(capped[0].node.is_none(), "the summary row is not clickable");
    }

    #[test]
    fn cap_collapses_rows_past_the_depth_ceiling() {
        let mut rows = vec![plain_row(0, "shallow")];
        rows.push(plain_row(usize::MAX, "too deep"));
        let capped = cap_outline_rows(rows, 1000.0);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0].label, "shallow");
        assert_eq!(capped[1].label, "+1 more");
    }

    #[test]
    fn cap_never_touches_glossary_rows_uncapped_export_stays_whole() {
        use mere::kernel::graph::Graph;
        let mut g = Graph::new();
        for i in 0..50 {
            g.add_node(
                format!("https://site.test/{i}"),
                euclid::default::Point2D::new(0.0, 0.0),
            );
        }
        // `outline_rows`/`outline_djot` never see a pane height; they always return the
        // complete tree regardless of how small the gloss pane's cap would be. 51 = the
        // one shared "site.test" host row (structural) + the 50 leaves under it.
        assert_eq!(mere::glossary::outline_rows(&g).len(), 51);
        assert_eq!(mere::glossary::outline_djot(&g).lines().count(), 51);
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
                    GlossOutlineRow {
                        depth: 0,
                        label: "site.test".to_string(),
                        node: None,
                    },
                    GlossOutlineRow {
                        depth: 1,
                        label: "Guide".to_string(),
                        node: Some(node(1, "https://site.test/guide", NodeState::Idle, false)),
                    },
                ],
                metrics: mere::glossary::GraphMetrics {
                    node_count: 1,
                    ..Default::default()
                },
            },
        );
        let _ = pane.pane.frame(280, 240, &Default::default());
        let dom = pane.dom();
        let dom = dom.borrow();
        assert_eq!(count_by_class(&dom, dom.document(), "gloss-outline-row"), 2);
        assert_eq!(
            count_by_class(&dom, dom.document(), "gloss-outline-row-structural"),
            1
        );
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
                metrics: mere::glossary::GraphMetrics::default(),
            },
        );
        let _ = pane.pane.frame(280, 240, &Default::default());
        let row_node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "gloss-outline-row").expect("a row")
        };
        pane.pane
            .dispatch_click(row_node, PointerClick::at((20.0, 10.0)));
        assert_eq!(
            pane.take_intents(),
            vec![GlossRowIntent::Select(
                "https://site.test/guide".to_string()
            )]
        );
    }
}
