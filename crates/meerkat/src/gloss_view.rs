/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! DOM builders for the gloss pane's minimap and recent-visited sections — the
//! [Scene-to-DOM migration plan](../../../design_docs/mere_docs/implementation_strategy/2026-07-01_gloss_scene_to_dom_migration_plan.md)'s
//! target. Parallel to `gloss_outline_view.rs`: folded into the same shell document,
//! rows hit-test and dispatch through the one shell runner instead of the bespoke
//! Scene hit-test the old `minimap_scene`/`recent_scene` used.
//!
//! Phase 1 (recent → pure DOM) and Phase 2 (minimap → DOM node squares + an
//! embedded-Scene edges/rings backdrop, the same split `orrery_element` uses for
//! the main graph canvas) both land here.

use forme::GraphMemberId;
use register_theme::chrome::{ChromeTheme, Color32};
use xilem_serval::{AnyView, Keyed, PointerClick, ServalCtx, ServalElement, clickable, el};

use super::WindowCtx;
use crate::gloss::GlossRowIntent;

pub type GlossRecentView = Box<dyn AnyView<GlossRecentState, (), ServalCtx, ServalElement>>;

/// One recently-visited row: the node's identity (for `data-member` + the click
/// route) and its URL, shown as-is — the same raw-URL label `recent_scene` already
/// rendered (a mechanical migration, not a labeling change).
#[derive(Clone, Debug, PartialEq)]
pub struct GlossRecentRow {
    pub member: GraphMemberId,
    pub url: String,
}

/// The host-built recent-visited projection for one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossRecentSnapshot {
    pub rows: Vec<GlossRecentRow>,
}

#[derive(Default)]
pub struct GlossRecentState {
    pub rows: Vec<GlossRecentRow>,
    pub pending: Vec<GlossRowIntent>,
}

impl WindowCtx<'_> {
    /// Project the graph's `SharedNavigationMemory` (`recent_visited`) into the gloss
    /// recent lens's snapshot: the same up-to-8 entries `recent_scene` drew, host-free
    /// beyond the graph query itself (no NODE_SHEET state needed — recent rows carry
    /// no accent, matching pre-migration behavior). (Scene-to-DOM migration P1.)
    pub(super) fn gloss_recent_snapshot(&self) -> GlossRecentSnapshot {
        let rows = self
            .orrery()
            .graph()
            .recent_visited(8)
            .into_iter()
            .map(|rv| GlossRecentRow {
                member: rv.node,
                url: rv.url,
            })
            .collect();
        GlossRecentSnapshot { rows }
    }
}

pub fn recent_view(state: &GlossRecentState) -> GlossRecentView {
    let header: GlossRecentView = Box::new(
        el::<_, GlossRecentState, ()>("div", "Recent".to_string())
            .attr("class", "gloss-recent-header"),
    );
    let scroll: GlossRecentView = if state.rows.is_empty() {
        Box::new(
            el::<_, GlossRecentState, ()>(
                "div",
                vec![Box::new(
                    el::<_, GlossRecentState, ()>("div", "nothing visited yet".to_string())
                        .attr("class", "gloss-recent-empty"),
                ) as GlossRecentView],
            )
            .attr("class", "gloss-recent-scroll"),
        )
    } else {
        let rows: Keyed<GraphMemberId, GlossRecentView> = state
            .rows
            .iter()
            .map(|row| (row.member, recent_row(row)))
            .collect();
        Box::new(el::<_, GlossRecentState, ()>("div", rows).attr("class", "gloss-recent-scroll"))
    };
    Box::new(
        el::<_, GlossRecentState, ()>("div", vec![header, scroll]).attr("class", "gloss-recent"),
    )
}

fn recent_row(row: &GlossRecentRow) -> GlossRecentView {
    let url = row.url.clone();
    let label: GlossRecentView = Box::new(
        el::<_, GlossRecentState, ()>("span", row.url.clone()).attr("class", "gloss-recent-label"),
    );
    Box::new(clickable(
        el::<_, GlossRecentState, ()>("div", vec![label])
            .attr("class", "gloss-recent-row")
            .attr("data-member", row.member.to_string()),
        move |st: &mut GlossRecentState, _: PointerClick| {
            st.pending.push(GlossRowIntent::Select(url.clone()));
        },
    ))
}

/// The minimap's embedded-Scene key for its edges/rings backdrop. The DOM node
/// squares (`minimap_view`'s children) ride the shell document directly; only the
/// vector backdrop needs a Scene raster, embedded via `<external-texture>` exactly
/// like the orrery pane's own edges/backdrop (`window_view::ORRERY_SCENE_KEY`). A
/// fresh reserved key, disjoint from it — the compositor keys the whole document,
/// not per-pane, so reusing the orrery's key would collide. (Scene-to-DOM migration P2.)
pub const GLOSS_MINIMAP_SCENE_KEY: u64 = 0xF0F0_0000_0000_0002;

pub type GlossMinimapView = Box<dyn AnyView<GlossMinimapState, (), ServalCtx, ServalElement>>;

/// One minimap node: identity, pane-local position + size (already mapped through
/// `crate::gloss::MinimapFit` — this struct carries no world coordinates), and its
/// resolved CSS color. Selection-tinted only, matching pre-migration behavior — the
/// minimap has never carried the outline's full NODE_SHEET state palette, just
/// selected-vs-not.
#[derive(Clone, Debug, PartialEq)]
pub struct GlossMinimapNode {
    pub member: GraphMemberId,
    pub url: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: String,
}

/// The host-built minimap projection for one frame: pane-local node squares plus
/// the pane's pixel size (the DOM node coordinates and the embedded backdrop
/// texture's dimensions must agree, so they travel together).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossMinimapSnapshot {
    pub nodes: Vec<GlossMinimapNode>,
    pub w: u32,
    pub h: u32,
}

#[derive(Default)]
pub struct GlossMinimapState {
    pub nodes: Vec<GlossMinimapNode>,
    pub w: u32,
    pub h: u32,
    pub pending: Vec<GlossRowIntent>,
}

/// The minimap's DOM node squares only — no `<external-texture>` inside this lensed
/// subtree. **Debugging note (Scene-to-DOM migration P2):** the original design put
/// the backdrop `<external-texture>` here, as `orrery_element` does for the main
/// canvas. That broke the whole chrome document (toolbar/shellbar/roster/outline all
/// went blank or partial) the moment the minimap opened — reproducible, not a
/// screenshot-timing artifact (confirmed via full render-pipeline tracing: rasterize,
/// compose, and `frame.present()` all completed with no panic/error, so the bug is a
/// DOM/layout issue, not a crash). The one thing genuinely novel about this element
/// vs. roster/outline/recent (whose lensed content is all normal-flow, no
/// `external-texture`) was an `<external-texture>` living inside a *lensed* subtree
/// at all — orrery's own backdrop is a *direct, non-lensed* child of the shell tuple.
/// This experiment removes that variable: the backdrop moves to a top-level
/// non-lensed element in `shell_view` (`window_view/views.rs`), matching orrery's
/// pattern exactly; only the interactive node squares stay lensed here.
pub fn minimap_view(state: &GlossMinimapState) -> GlossMinimapView {
    let children: Keyed<GraphMemberId, GlossMinimapView> = state
        .nodes
        .iter()
        .map(|node| (node.member, minimap_node_view(node)))
        .collect();
    Box::new(el::<_, GlossMinimapState, ()>("div", children).attr("class", "gloss-minimap"))
}

fn minimap_node_view(node: &GlossMinimapNode) -> GlossMinimapView {
    let half = node.size * 0.5;
    let (cx, cy) = (node.x - half, node.y - half);
    let style = format!(
        "position:absolute;left:0;top:0;transform:translate({cx}px,{cy}px);width:{}px;height:{}px;background-color:{}",
        node.size, node.size, node.color
    );
    let url = node.url.clone();
    Box::new(clickable(
        el::<_, GlossMinimapState, ()>("div", ())
            .attr("class", "gloss-minimap-node")
            .attr("style", style)
            .attr("data-member", node.member.to_string()),
        move |st: &mut GlossMinimapState, _: PointerClick| {
            st.pending.push(GlossRowIntent::Select(url.clone()));
        },
    ))
}

/// The recent section's author CSS, themed from the chrome tokens — mirrors
/// `gloss_outline_sheet`.
pub fn gloss_recent_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        format!(
            ".gloss-recent {{ position: relative; overflow: hidden; height: 100%; box-sizing: border-box; background-color: {}; border-top: 1px solid {}; }}",
            rgb(c.panel_bg),
            rgb(c.muted_text)
        ),
        format!(
            ".gloss-recent-header {{ display: block; font-size: 10px; color: {}; padding: 5px 16px 4px; }}",
            rgb(c.muted_text)
        ),
        ".gloss-recent-scroll { display: block; overflow: scroll; height: 100%; box-sizing: border-box; }".to_string(),
        ".gloss-recent-row { display: block; padding: 2px 16px; }".to_string(),
        format!(
            ".gloss-recent-label {{ display: block; font-size: 11px; color: {}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}",
            rgb(c.body_text)
        ),
        format!(
            ".gloss-recent-empty {{ display: block; font-size: 11px; color: {}; padding: 2px 16px; }}",
            rgb(c.muted_text)
        ),
    ]
}

/// The minimap's author CSS — the DOM half only (node squares); the backdrop
/// (edges/rings) rides the embedded Scene, unstyled by this sheet.
///
/// Deliberately no `position: relative` and no `background-color` on
/// `.gloss-minimap`. A `relative` wrapper here would add a third positioning level
/// (gloss-minimap-pane[absolute] > gloss-minimap[relative] > gloss-minimap-node
/// [absolute]) — one deeper than the orrery's own gnodes (orrery[absolute] >
/// gnode[absolute], no relative wrapper between) — which corrupted the whole
/// chrome document; static positioning lets `.gloss-minimap-node`'s `position:
/// absolute` resolve its containing block to the pane wrapper directly, matching
/// the orrery's (working) depth. A `background-color` here would paint over the
/// backdrop `<external-texture>` (the edges/rings Scene): this div is part of
/// `chrome_scene`, which composites *after* the backdrop at the same rect, so an
/// opaque fill here hides every edge, leaving only the node squares (drawn within
/// this same opaque layer) visible. The backdrop's own `ColorLoad::Clear` already
/// fills the identical panel color from underneath, so the pane stays fully opaque
/// on screen; only the *source* of that fill changes.
pub fn gloss_minimap_sheet(_c: &ChromeTheme) -> Vec<String> {
    vec![
        ".gloss-minimap { overflow: hidden; height: 100%; box-sizing: border-box; }".to_string(),
        ".gloss-minimap-node { box-sizing: border-box; }".to_string(),
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

    type GlossRecentLogic = fn(&GlossRecentState) -> GlossRecentView;

    struct RecentPane {
        pane: ViewPane<GlossRecentState, GlossRecentLogic, GlossRecentView>,
    }

    impl RecentPane {
        fn new() -> Self {
            Self {
                pane: ViewPane::new(recent_view as GlossRecentLogic, GlossRecentState::default()),
            }
        }

        fn set_snapshot(&mut self, theme: &ChromeTheme, snapshot: GlossRecentSnapshot) {
            self.pane.set_sheets(gloss_recent_sheet(theme));
            self.pane.update(|s| s.rows = snapshot.rows);
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
    fn empty_recent_shows_empty_state() {
        let mut pane = RecentPane::new();
        pane.set_snapshot(&ChromeTheme::default(), GlossRecentSnapshot::default());
        let _ = pane.pane.frame(280, 240, &Default::default());
        let dom = pane.dom();
        let dom = dom.borrow();
        assert!(first_by_class(&dom, dom.document(), "gloss-recent-empty").is_some());
    }

    #[test]
    fn renders_one_row_per_recent_entry() {
        let mut pane = RecentPane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            GlossRecentSnapshot {
                rows: vec![
                    GlossRecentRow {
                        member: GraphMemberId::from_u128(1),
                        url: "https://a.test/".to_string(),
                    },
                    GlossRecentRow {
                        member: GraphMemberId::from_u128(2),
                        url: "https://b.test/".to_string(),
                    },
                ],
            },
        );
        let _ = pane.pane.frame(280, 240, &Default::default());
        let dom = pane.dom();
        let dom = dom.borrow();
        assert_eq!(count_by_class(&dom, dom.document(), "gloss-recent-row"), 2);
        assert!(first_by_class(&dom, dom.document(), "gloss-recent-header").is_some());
    }

    #[test]
    fn clicking_a_recent_row_queues_select_by_url() {
        let mut pane = RecentPane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            GlossRecentSnapshot {
                rows: vec![GlossRecentRow {
                    member: GraphMemberId::from_u128(1),
                    url: "https://a.test/".to_string(),
                }],
            },
        );
        let _ = pane.pane.frame(280, 240, &Default::default());
        let row_node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "gloss-recent-row").expect("a row")
        };
        pane.pane
            .dispatch_click(row_node, PointerClick::at((20.0, 10.0)));
        assert_eq!(
            pane.take_intents(),
            vec![GlossRowIntent::Select("https://a.test/".to_string())]
        );
    }

    type GlossMinimapLogic = fn(&GlossMinimapState) -> GlossMinimapView;

    struct MinimapPane {
        pane: ViewPane<GlossMinimapState, GlossMinimapLogic, GlossMinimapView>,
    }

    impl MinimapPane {
        fn new() -> Self {
            Self {
                pane: ViewPane::new(
                    minimap_view as GlossMinimapLogic,
                    GlossMinimapState::default(),
                ),
            }
        }

        fn set_snapshot(&mut self, theme: &ChromeTheme, snapshot: GlossMinimapSnapshot) {
            self.pane.set_sheets(gloss_minimap_sheet(theme));
            self.pane.update(|s| {
                s.nodes = snapshot.nodes;
                s.w = snapshot.w;
                s.h = snapshot.h;
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

    fn minimap_node(member: u128, url: &str, x: f32, y: f32) -> GlossMinimapNode {
        GlossMinimapNode {
            member: GraphMemberId::from_u128(member),
            url: url.to_string(),
            x,
            y,
            size: 7.0,
            color: "rgb(216, 222, 234)".to_string(),
        }
    }

    #[test]
    fn minimap_renders_the_backdrop_and_a_square_per_node() {
        let mut pane = MinimapPane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            GlossMinimapSnapshot {
                nodes: vec![
                    minimap_node(1, "https://a.test/", 10.0, 10.0),
                    minimap_node(2, "https://b.test/", 40.0, 40.0),
                ],
                w: 200,
                h: 150,
            },
        );
        let _ = pane.pane.frame(200, 150, &Default::default());
        let dom = pane.dom();
        let dom = dom.borrow();
        assert_eq!(
            count_by_class(&dom, dom.document(), "gloss-minimap-node"),
            2
        );
        assert!(first_by_class(&dom, dom.document(), "gloss-minimap").is_some());
    }

    #[test]
    fn clicking_a_minimap_node_queues_select_by_url() {
        let mut pane = MinimapPane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            GlossMinimapSnapshot {
                nodes: vec![minimap_node(1, "https://a.test/", 10.0, 10.0)],
                w: 200,
                h: 150,
            },
        );
        let _ = pane.pane.frame(200, 150, &Default::default());
        let node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "gloss-minimap-node").expect("a node square")
        };
        pane.pane.dispatch_click(node, PointerClick::at((3.0, 3.0)));
        assert_eq!(
            pane.take_intents(),
            vec![GlossRowIntent::Select("https://a.test/".to_string())]
        );
    }
}
