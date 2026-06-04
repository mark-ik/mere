/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # platen-view
//!
//! The serval view layer for platen's tiled workbench (taffy retarget, Phase 2).
//!
//! platen-core owns the [`Workbench`] tiling model and stays GUI-free. This crate
//! is its **serval-coupled view**: it turns the workbench into an
//! [`xilem_serval`](xilem_serval) view tree that diffs into a `ScriptedDom`, which
//! serval lays out with taffy. No morphorm, no host-side rect math — the tile tree
//! is flex DOM.
//!
//! ## Shape
//!
//! [`workbench_view`] emits a flex row ([`.workbench`](WORKBENCH_SHEET)) of slot
//! columns. Each column ([`.wb-slot`]) is a tab strip ([`.wb-strip`]) over a
//! flex-grow content placeholder ([`.wb-content`]). The strip holds one clickable
//! tab per stacked member (the active one carrying `wb-tab-active`) plus a close
//! button. The content placeholder is tagged with its active member's id
//! (`data-member`) so the host can find its laid-out rect and composite that tile's
//! actor texture there (until serval grows an in-DOM external-texture element).
//!
//! ## Interaction
//!
//! The runner state is a [`WorkbenchScene`] (a geometry-free render model the host
//! syncs from its [`Workbench`] each frame). A tab / button click records a
//! [`WorkbenchAction`] in `pending`; the host drains it with [`WorkbenchScene::take_pending`]
//! and applies it to the real model. Same record-then-drain seam meerkat's chrome
//! already uses for its overlays.

use forme::GraphMemberId;
use kernel::graph::Graph;
use platen::Workbench;
use xilem_serval::{el, on_click, AnyView, PointerClick, ServalCtx, ServalElement};

/// Author CSS for the workbench root. The row of slot columns splits the width
/// equally; each column stacks a fixed-height tab strip over a flex-grow content
/// area. serval maps these through `stylo_taffy`.
pub const WORKBENCH_SHEET: &[&str] = &[
    "div, button { display: block; }",
    ".workbench { display: flex; background-color: rgb(17, 20, 26); }",
    ".wb-slot { display: flex; flex-direction: column; flex-grow: 1; }",
    // Draggable gutter between slots — a fixed-width column the host resizes on.
    ".wb-divider { width: 6px; flex-grow: 0; flex-shrink: 0; background-color: rgb(24, 27, 34); }",
    ".wb-strip { display: flex; height: 28px; background-color: rgb(40, 44, 54); }",
    // The drag drop-target slot's strip lights up (the content area is under the
    // tile's composited texture, so the cue rides the strip).
    ".wb-strip-drop { display: flex; height: 28px; background-color: rgb(58, 86, 138); }",
    ".wb-tabs { display: flex; background-color: rgb(40, 44, 54); flex-grow: 1; }",
    ".wb-tab { font-size: 13px; color: rgb(176, 182, 196); \
        background-color: rgb(34, 38, 48); padding: 6px 12px; margin-right: 2px; }",
    ".wb-tab-active { font-size: 13px; color: rgb(234, 238, 246); \
        background-color: rgb(52, 58, 74); padding: 6px 12px; margin-right: 2px; }",
    ".wb-pin { font-size: 15px; color: rgb(214, 218, 228); \
        background-color: rgb(40, 44, 54); padding: 5px 10px; }",
    ".wb-close { font-size: 15px; color: rgb(214, 218, 228); \
        background-color: rgb(40, 44, 54); padding: 5px 10px; }",
    ".wb-content { flex-grow: 1; background-color: rgb(17, 20, 26); }",
];

/// One tab to render in a slot's strip: its graph member (for click routing) and
/// display label.
#[derive(Clone, Debug, PartialEq)]
pub struct TabView {
    pub member: GraphMemberId,
    pub label: String,
}

/// One slot to render: its tabs in order, which is the visible (active) one,
/// whether the active tab is pinned (kept active in the background + exempt from
/// the actor pool's eviction), and the slot's width weight (flex-grow).
#[derive(Clone, Debug, PartialEq)]
pub struct SlotPlan {
    pub tabs: Vec<TabView>,
    pub active: usize,
    pub pinned: bool,
    /// The slot's share of the row width (flex-grow). Equal (1.0) by default.
    pub weight: f32,
}

impl SlotPlan {
    /// The active tab's member (the visible one, or the first as a fallback).
    fn active_member(&self) -> Option<GraphMemberId> {
        self.tabs.get(self.active).or_else(|| self.tabs.first()).map(|t| t.member)
    }
}

/// What a workbench-view interaction asks the host to do. The host drains a
/// captured action and applies it to the real [`Workbench`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkbenchAction {
    /// Make this member the visible tab of its stack (a tab click).
    Activate(GraphMemberId),
    /// Close this tab (a strip close button) — the host reaps its actor.
    Close(GraphMemberId),
    /// Toggle this tab's pin (a strip pin button) — the background-keep flag, which
    /// also exempts it from the actor pool's eviction.
    TogglePin(GraphMemberId),
}

/// The render model the workbench view diffs into serval DOM: the slots to draw
/// plus the band size to fill and a captured pending action. The host syncs this
/// from its [`Workbench`] each frame and drains `pending`.
///
/// The slots are laid out by serval/taffy, but the flex root needs a *definite*
/// size to lay into (taffy's root gets the viewport as available space, not a
/// definite size, so an `auto` root would shrink to its content). So the host
/// passes the band's `viewport` (content-band width × height in px) and the root
/// is sized to it; everything below is flex.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbenchScene {
    pub slots: Vec<SlotPlan>,
    /// The content band the tile tree fills, in px (width, height).
    pub viewport: (f32, f32),
    pub pending: Option<WorkbenchAction>,
    /// During a tab drag, the slot the pointer is over (its active member) — the
    /// drop target, highlighted for feedback. `None` when nothing is being dragged.
    pub drag_target: Option<GraphMemberId>,
}

impl WorkbenchScene {
    /// Build the render model from a `workbench`, resolving each member's URL label
    /// against `graph`, to fill `viewport` (the content band, width × height in px).
    pub fn from_workbench(
        workbench: &Workbench,
        graph: &Graph,
        viewport: (f32, f32),
        is_pinned: impl Fn(GraphMemberId) -> bool,
    ) -> Self {
        let slots = workbench
            .slot_views()
            .map(|slot| {
                let active_member = slot.members.get(slot.active).or_else(|| slot.members.first());
                SlotPlan {
                    tabs: slot
                        .members
                        .iter()
                        .map(|&member| TabView {
                            member,
                            label: graph
                                .get_node_by_id(member)
                                .map(|(_, node)| tile_label(node.url()))
                                .unwrap_or_else(|| "(gone)".to_string()),
                        })
                        .collect(),
                    active: slot.active,
                    pinned: active_member.copied().is_some_and(&is_pinned),
                    weight: slot.weight,
                }
            })
            .collect();
        Self { slots, viewport, pending: None, drag_target: None }
    }

    /// Capture an "activate this tab" request (a tab click).
    pub fn activate(&mut self, member: GraphMemberId) {
        self.pending = Some(WorkbenchAction::Activate(member));
    }

    /// Capture a "close this tab" request (a strip close button).
    pub fn close(&mut self, member: GraphMemberId) {
        self.pending = Some(WorkbenchAction::Close(member));
    }

    /// Capture a "toggle this tab's pin" request (a strip pin button).
    pub fn toggle_pin(&mut self, member: GraphMemberId) {
        self.pending = Some(WorkbenchAction::TogglePin(member));
    }

    /// Take the captured action, if any. The host drains this after a click and
    /// applies it to the real [`Workbench`].
    pub fn take_pending(&mut self) -> Option<WorkbenchAction> {
        self.pending.take()
    }
}

/// The erased view type the workbench logic produces (so the concrete `El<…>` tuple
/// need not be spelled).
pub type WorkbenchTreeView = Box<dyn AnyView<WorkbenchScene, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: scene → view tree.
pub type WorkbenchLogic = fn(&WorkbenchScene) -> WorkbenchTreeView;

/// Build the tiled-workbench view: a flex row of slot columns (see the module docs
/// and [`WORKBENCH_SHEET`] for the shape). serval/taffy lays the flex out.
pub fn workbench_view(scene: &WorkbenchScene) -> WorkbenchTreeView {
    let (w, h) = scene.viewport;
    // Slot columns with a draggable divider between each pair. The divider carries
    // its left-slot index (`data-divider`) so the host can resize on drag.
    let last = scene.slots.len().saturating_sub(1);
    let mut columns: Vec<WorkbenchTreeView> = Vec::new();
    for (i, slot) in scene.slots.iter().enumerate() {
        let targeted = scene.drag_target.is_some() && slot.active_member() == scene.drag_target;
        columns.push(slot_column(slot, targeted));
        if i < last {
            columns.push(Box::new(
                el::<_, WorkbenchScene, ()>("div", ())
                    .attr("class", "wb-divider")
                    .attr("data-divider", i.to_string()),
            ));
        }
    }
    // The root carries a *definite* size (the host's band) so the flex row fills the
    // width (slots grow equally) and the height (each slot's content area fills below
    // its strip). taffy hands the root the viewport as available space, not a definite
    // size, so an auto root would shrink to its content.
    Box::new(
        el::<_, WorkbenchScene, ()>("div", columns)
            .attr("class", "workbench")
            .attr("style", format!("width: {w}px; height: {h}px;")),
    )
}

/// One slot column: a tab strip over a content placeholder. `targeted` highlights
/// it as the current drag drop target.
fn slot_column(slot: &SlotPlan, targeted: bool) -> WorkbenchTreeView {
    // The tab strip: one clickable tab per member, the visible one marked active.
    let tabs: Vec<WorkbenchTreeView> = slot
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let member = tab.member;
            let class = if i == slot.active { "wb-tab-active" } else { "wb-tab" };
            let view = on_click(
                el::<_, WorkbenchScene, ()>("div", tab.label.clone()).attr("class", class),
                move |s: &mut WorkbenchScene, _: PointerClick| s.activate(member),
            );
            Box::new(view) as WorkbenchTreeView
        })
        .collect();
    let tabs_row = el::<_, WorkbenchScene, ()>("div", tabs).attr("class", "wb-tabs");

    // Pin + close act on the slot's active tab (the visible member).
    let active_member = slot.active_member();
    let pin_glyph = if slot.pinned { "\u{2605}" } else { "\u{2606}" };
    let pin = on_click(
        el::<_, WorkbenchScene, ()>("button", pin_glyph).attr("class", "wb-pin"),
        move |s: &mut WorkbenchScene, _: PointerClick| {
            if let Some(m) = active_member {
                s.toggle_pin(m);
            }
        },
    );
    let close = on_click(
        el::<_, WorkbenchScene, ()>("button", "\u{00d7}").attr("class", "wb-close"),
        move |s: &mut WorkbenchScene, _: PointerClick| {
            if let Some(m) = active_member {
                s.close(m);
            }
        },
    );
    // The strip brightens when this slot is the drag drop target (the content area
    // is covered by the tile's composited texture, so the strip carries the cue).
    let strip_class = if targeted { "wb-strip-drop" } else { "wb-strip" };
    let strip =
        el::<_, WorkbenchScene, ()>("div", (tabs_row, pin, close)).attr("class", strip_class);

    // The content placeholder, tagged with the active member so the host can find
    // its laid-out rect and composite that tile's actor texture there.
    let member_attr = active_member.map(|m| m.to_string()).unwrap_or_default();
    let content = el::<_, WorkbenchScene, ()>("div", ())
        .attr("class", "wb-content")
        .attr("data-member", member_attr);

    // flex-grow = the slot's weight, so a divider drag reweights the columns.
    Box::new(
        el::<_, WorkbenchScene, ()>("div", (strip, content))
            .attr("class", "wb-slot")
            .attr("style", format!("flex-grow: {};", slot.weight)),
    )
}

/// A tab label: the URL with its scheme trimmed and truncated to fit. Mirrors
/// meerkat's `tile_label`; the trimming is cosmetic.
fn tile_label(url: &str) -> String {
    let trimmed =
        url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
    if trimmed.chars().count() > 36 {
        format!("{}\u{2026}", trimmed.chars().take(35).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use kernel::geometry::PortablePoint;
    use layout_dom_api::LayoutDom;
    use serval_scripted_dom::{NodeId, ScriptedDom};
    use uuid::Uuid;
    use xilem_serval::ServalAppRunner;

    use super::*;

    fn tab(n: u128, label: &str) -> TabView {
        TabView { member: Uuid::from_u128(n), label: label.to_string() }
    }

    fn scene(slots: Vec<SlotPlan>) -> WorkbenchScene {
        WorkbenchScene { slots, viewport: (800.0, 600.0), pending: None, drag_target: None }
    }

    fn runner(
        scene: WorkbenchScene,
    ) -> ServalAppRunner<WorkbenchScene, WorkbenchLogic, WorkbenchTreeView> {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        ServalAppRunner::new(dom, workbench_view as WorkbenchLogic, scene)
    }

    /// Count elements carrying exactly class `class` in the subtree at `id`.
    fn count_class(dom: &ScriptedDom, id: NodeId, class: &str) -> usize {
        let here = usize::from(
            dom.attributes(id).any(|a| a.name.local.as_ref() == "class" && a.value == class),
        );
        here + dom.dom_children(id).map(|c| count_class(dom, c, class)).sum::<usize>()
    }

    #[test]
    fn two_split_slots_render_a_column_strip_and_content_each() {
        let r = runner(scene(vec![
            SlotPlan { tabs: vec![tab(1, "https://a.example")], active: 0, pinned: false, weight: 1.0 },
            SlotPlan { tabs: vec![tab(2, "https://b.example")], active: 0, pinned: false, weight: 1.0 },
        ]));
        let dom = r.dom();
        let dom = dom.borrow();
        let root = r.root();
        assert_eq!(count_class(&dom, root, "workbench"), 1, "one workbench root");
        assert_eq!(count_class(&dom, root, "wb-slot"), 2, "a column per slot");
        assert_eq!(count_class(&dom, root, "wb-strip"), 2, "a strip per slot");
        assert_eq!(count_class(&dom, root, "wb-content"), 2, "a content placeholder per slot");
        assert_eq!(count_class(&dom, root, "wb-tab-active"), 2, "each lone tab is active");
    }

    #[test]
    fn a_stack_renders_its_tabs_with_one_active() {
        let r = runner(scene(vec![SlotPlan {
            tabs: vec![tab(1, "a"), tab(2, "b"), tab(3, "c")],
            active: 1,
            pinned: false,
            weight: 1.0,
        }]));
        let dom = r.dom();
        let dom = dom.borrow();
        let root = r.root();
        assert_eq!(count_class(&dom, root, "wb-slot"), 1, "one stacked slot");
        assert_eq!(count_class(&dom, root, "wb-tab-active"), 1, "the visible tab");
        assert_eq!(count_class(&dom, root, "wb-tab"), 2, "the two inactive tabs");
    }

    #[test]
    fn empty_scene_renders_a_bare_workbench_root() {
        let r = runner(scene(Vec::new()));
        let dom = r.dom();
        let dom = dom.borrow();
        let root = r.root();
        assert_eq!(count_class(&dom, root, "workbench"), 1);
        assert_eq!(count_class(&dom, root, "wb-slot"), 0);
    }

    #[test]
    fn from_workbench_resolves_labels_and_grouping() {
        let mut wb = Workbench::new();
        wb.open_tile(Uuid::from_u128(1));
        wb.open_tile(Uuid::from_u128(2));
        wb.stack_all(); // one stack of two
        let mut graph = Graph::new();
        graph.add_node_with_id(Uuid::from_u128(1), "https://x.example".to_string(), PortablePoint::new(0.0, 0.0));
        graph.add_node_with_id(Uuid::from_u128(2), "https://y.example".to_string(), PortablePoint::new(0.0, 0.0));
        let scene = WorkbenchScene::from_workbench(&wb, &graph, (800.0, 600.0), |_| false);
        assert_eq!(scene.slots.len(), 1, "grouped into one slot");
        assert_eq!(scene.slots[0].tabs.len(), 2);
        assert_eq!(scene.slots[0].tabs[0].label, "x.example", "scheme trimmed");
        assert!(!scene.slots[0].pinned);
        // A member missing from the graph resolves to a placeholder label.
        let mut wb2 = Workbench::new();
        wb2.open_tile(Uuid::from_u128(9));
        let scene2 = WorkbenchScene::from_workbench(&wb2, &Graph::new(), (800.0, 600.0), |_| false);
        assert_eq!(scene2.slots[0].tabs[0].label, "(gone)");
    }

    /// The drag drop target highlights its slot's strip.
    #[test]
    fn drag_target_highlights_its_slot_strip() {
        let mut s = scene(vec![
            SlotPlan { tabs: vec![tab(1, "a")], active: 0, pinned: false, weight: 1.0 },
            SlotPlan { tabs: vec![tab(2, "b")], active: 0, pinned: false, weight: 1.0 },
        ]);
        s.drag_target = Some(Uuid::from_u128(2));
        let r = runner(s);
        let dom = r.dom();
        let dom = dom.borrow();
        let root = r.root();
        assert_eq!(count_class(&dom, root, "wb-strip-drop"), 1, "the target slot's strip lit");
        assert_eq!(count_class(&dom, root, "wb-strip"), 1, "the other slot's plain strip");
    }

    /// The pin predicate marks a slot pinned when its active member is pinned, and
    /// the pin button captures a TogglePin.
    #[test]
    fn pin_predicate_and_toggle() {
        let mut wb = Workbench::new();
        wb.open_tile(Uuid::from_u128(1));
        let pinned = Uuid::from_u128(1);
        let scene = WorkbenchScene::from_workbench(&wb, &Graph::new(), (800.0, 600.0), |m| m == pinned);
        assert!(scene.slots[0].pinned, "the active member is pinned");
        let mut s = scene;
        s.toggle_pin(Uuid::from_u128(1));
        assert_eq!(s.pending, Some(WorkbenchAction::TogglePin(Uuid::from_u128(1))));
    }

    #[test]
    fn action_methods_capture_pending() {
        let mut s = scene(vec![SlotPlan { tabs: vec![tab(1, "a")], active: 0, pinned: false, weight: 1.0 }]);
        s.activate(Uuid::from_u128(1));
        assert_eq!(s.pending, Some(WorkbenchAction::Activate(Uuid::from_u128(1))));
        assert_eq!(s.take_pending(), Some(WorkbenchAction::Activate(Uuid::from_u128(1))));
        assert_eq!(s.take_pending(), None, "drained");
        s.close(Uuid::from_u128(2));
        assert_eq!(s.pending, Some(WorkbenchAction::Close(Uuid::from_u128(2))));
    }
}
