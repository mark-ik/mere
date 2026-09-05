/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Frisket: a pane frame — splits and tab-stacks — as one composition.
//!
//! On a hand press the *frisket* is the hinged frame whose cut-out apertures
//! decide what prints where; this is the same frame, over a window. The name
//! comes from turnstone's crate, which this module retires: the family had spelled
//! "a tree of resizable panes" four times (this view trapped in `ports/pelt`, a
//! second tree in turnstone, the reusable `workbench` contract, the furniture here),
//! and they resolve to one implementation. Direction:
//! `genet:docs/2026-07-24_frisket_pane_component_direction.md`.
//!
//! **The tree is the caller's state**, like the split's ratio and the strip's
//! selection: [`frisket`] renders whatever tree the caller hands it and reports
//! gestures as [`TileEvent`]s for the caller to apply through
//! [`TileTree::apply`]. The component never mutates the tree, so one host can
//! hold a plain tree while another projects one from richer truth (mere's platen
//! projects a forme arrangement onto it) and both drive the same view.
//!
//! **No layout dependency, by design.** The parts here are the view and the DOM
//! *semantics*: which element is a divider, which is a tab, which tab bar
//! belongs to which stack. A host lays the DOM out and hit-tests it, then asks
//! [`divider_target`] / [`tab_target`] / [`stack_target`] what it hit. That
//! keeps the walking logic in one place — it was duplicated per host before —
//! while layout stays where the host's engine is. The one gesture that needs
//! real geometry, resolving a tab drop to an insertion index, takes the rects
//! through a closure ([`tab_drop_index`]).
//!
//! Content is a [`Slot`]. [`frisket`] draws a placeholder marked with the
//! active tile's id and nothing else, and the host composites a document scene,
//! an external texture, or its own surface into that rect. [`frisket_with`]
//! widens that to three: the same hole, a surface the host names as one, or a
//! child view the frame nests. So a browser pane, a practice-set pane, a
//! tactical map pane and a pane made of ordinary views are the same frame.
//!
//! A [`Slot::View`] child is rendered through a
//! [`PortableKeyed`](crate::PortableKeyed) keyed by [`TileId`], so dragging a
//! tile into another stack *moves* its element rather than rebuilding it: the
//! child keeps its DOM node, its view state and its handlers.
//!
//! The tab bar is [`tab_bar_view`](crate::tab_bar_view), the crate's one tab
//! bar, wearing Frisket's `frisket-*` names on top of the shared `tablist` /
//! `tab` / `tab-close` vocabulary so a product stylesheet written against
//! either keeps matching.

use std::marker::PhantomData;

use layout_dom_api::{LayoutDom, LocalName, Namespace};
use meristem::{MessageCtx, MessageResult, Mut, ViewMarker};
use workbench::{SplitAxis, TabStack, Tile, TileEvent, TileId, TilePath, TileTree};

use crate::pod::GenetElement;
use crate::{
    AnyView, GenetCtx, PortableKeyed, TabAccentColors, TabBar, TabBarNames, TabItem, View, el,
    tab_bar_view,
};

/// The erased child-view type a pane frame nests: a split holds splits or
/// stacks, so the recursion needs one boxed type.
pub type PaneView<State, AppAction> = Box<dyn AnyView<State, AppAction, GenetCtx, GenetElement>>;

/// `data-divider`: the split path a divider resizes.
const ATTR_DIVIDER: &str = "data-divider";
/// `data-dindex`: which boundary within that split.
const ATTR_DINDEX: &str = "data-dindex";
/// `data-tabkey`: the tile a tab represents, in the shared tab-bar vocabulary.
const ATTR_TABKEY: &str = "data-tabkey";
/// `data-tabid`: the same value under Frisket's older name, kept because hosts
/// address tabs by it from their own code.
const ATTR_TABID: &str = "data-tabid";
/// `data-stack`: the path of the stack a tab bar belongs to.
const ATTR_STACK: &str = "data-stack";
/// `data-slot`: which of [`SlotKind`]'s three a content element carries.
const ATTR_SLOT: &str = "data-slot";

/// The names Frisket's bar wears on top of the shared `tablist` / `tab` /
/// `tab selected` / `tab-close` vocabulary. Products style `frisket-*`
/// directly, so the bar answers to both.
const FRISKET_TAB_NAMES: TabBarNames = TabBarNames {
    bar: Some("frisket-tabbar"),
    tab: Some("frisket-tab"),
    selected: Some("active"),
    close: Some("frisket-close"),
    label: Some("frisket-label"),
    key_alias: Some(ATTR_TABID),
};
/// `data-tile`: the active tile whose content area this is. A host finds this
/// element's rect to composite the tile's content into it.
pub const FRISKET_TILE_ATTR: &str = "data-tile";

/// The default structural stylesheet: flex skeleton, tab bar, divider, and the
/// content hole. A theme layers over it; nothing here is decorative beyond the
/// minimum needed to be usable unstyled.
pub const FRISKET_CSS: &str = "\
    .frisket-split { width: 100%; height: 100%; } \
    .frisket-branch { display: flex; } \
    .frisket-stack { width: 100%; height: 100%; } \
    .frisket-tabbar { display: flex; flex-grow: 0; flex-shrink: 0; flex-basis: 44px; align-items: stretch; height: 44px; padding: 4px 2px 0 2px; background: #33333a; } \
    .frisket-tab { display: flex; align-items: center; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0; max-width: 360px; overflow: hidden; padding: 8px 10px; font-size: 15px; line-height: 1.2; color: #cccccc; background: #2a2a30; margin-right: 3px; } \
    .frisket-tab.active { color: #ffffff; background: #4a4a55; } \
    .frisket-label { flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0; overflow: hidden; white-space: nowrap; } \
    .frisket-close { flex-grow: 0; flex-shrink: 0; flex-basis: 28px; width: 28px; height: 28px; margin-left: 4px; padding: 4px 0; text-align: center; font-size: 15px; color: #999999; } \
    .frisket-content { flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-height: 0; background: #ffffff; } \
    .frisket-divider { flex-grow: 0; flex-shrink: 0; flex-basis: 10px; background: #1a1a1f; }";

/// Encode a split path (`[0, 1]`) as an attribute string (`"0.1"`); the root
/// split is the empty string.
pub fn encode_pane_path(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

/// Decode a `data-divider` / `data-stack` value back to a [`TilePath`].
pub fn decode_pane_path(s: &str) -> TilePath {
    if s.is_empty() {
        TilePath(Vec::new())
    } else {
        TilePath(s.split('.').filter_map(|p| p.parse().ok()).collect())
    }
}

/// What fills one stack's content element.
///
/// The frame draws the element either way; the difference is who owns what is
/// inside it. A [`Slot::View`] is a child view the frame nests and keeps alive
/// across a tile's move between stacks; [`Slot::Surface`] and [`Slot::Hole`]
/// leave it empty for a host to composite into, and differ only in what the
/// host reads back off the DOM ([`slot_kind`]).
pub enum Slot<State, AppAction> {
    /// A child view, rendered inside the content element.
    View(PaneView<State, AppAction>),
    /// Empty; the host composites its own surface into the element's rect.
    Surface,
    /// Empty and unclaimed — the original content hole.
    Hole,
}

/// Which of [`Slot`]'s three a content element carries, read back off the DOM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    View,
    Surface,
    Hole,
}

impl SlotKind {
    /// The `data-slot` value for this kind.
    fn attr_value(self) -> &'static str {
        match self {
            SlotKind::View => "view",
            SlotKind::Surface => "surface",
            SlotKind::Hole => "hole",
        }
    }

    fn from_attr(value: &str) -> Option<Self> {
        match value {
            "view" => Some(SlotKind::View),
            "surface" => Some(SlotKind::Surface),
            "hole" => Some(SlotKind::Hole),
            _ => None,
        }
    }
}

/// One stack's content child, rebuilt from its tile and the host's `fill` on
/// demand rather than held as a value.
///
/// [`PortableKeyed`] needs a `Clone` child view — the parked previous view is
/// what an adopting rebuild diffs against — and an erased `PaneView` is not
/// `Clone`. Cloning the *recipe* (the tile plus the fill closure) instead is
/// the `TaggedText` pattern, and it is what buys a tile its element and view
/// state across a drag into another stack.
struct SlotChild<State, AppAction, Fill> {
    tile: Tile,
    fill: Fill,
    marker: PhantomData<fn() -> (State, AppAction)>,
}

impl<State, AppAction, Fill> SlotChild<State, AppAction, Fill>
where
    State: 'static,
    AppAction: 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction>,
{
    fn new(tile: Tile, fill: Fill) -> Self {
        Self {
            tile,
            fill,
            marker: PhantomData,
        }
    }

    /// The view this child stands for. Only ever built for a tile whose slot is
    /// a [`Slot::View`]; the other arms cannot be reached from `render_stack`,
    /// and degrade to an empty element rather than a panic if `fill` is not a
    /// function of its tile alone.
    fn inner(&self) -> PaneView<State, AppAction> {
        match (self.fill)(&self.tile) {
            Slot::View(view) => view,
            Slot::Surface | Slot::Hole => Box::new(el::<_, State, AppAction>("div", ())),
        }
    }
}

impl<State, AppAction, Fill: Clone> Clone for SlotChild<State, AppAction, Fill> {
    fn clone(&self) -> Self {
        Self {
            tile: self.tile.clone(),
            fill: self.fill.clone(),
            marker: PhantomData,
        }
    }
}

impl<State, AppAction, Fill> ViewMarker for SlotChild<State, AppAction, Fill> {}

impl<State, AppAction, Fill> View<State, AppAction, GenetCtx> for SlotChild<State, AppAction, Fill>
where
    State: 'static,
    AppAction: 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction> + 'static,
{
    type Element = GenetElement;
    type ViewState = <PaneView<State, AppAction> as View<State, AppAction, GenetCtx>>::ViewState;

    fn build(&self, ctx: &mut GenetCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        self.inner().build(ctx, app_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut GenetCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        self.inner()
            .rebuild(&prev.inner(), view_state, ctx, element, app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut GenetCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.inner().teardown(view_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<AppAction> {
        self.inner()
            .message(view_state, message, element, app_state)
    }
}

/// Render `tree` as a pane frame with every content element left a hole.
/// `on_event` receives every gesture the frame raises (a tab activated, a tab
/// closed); the caller applies it to its own authoritative tree.
///
/// The returned view fills its parent, so a host wraps it in whatever frame it
/// wants — a status bar under it, chrome above it.
///
/// [`frisket_with`] with a `fill` of [`Slot::Hole`], exactly.
pub fn frisket<State, AppAction, Ev>(
    tree: &TileTree,
    on_event: Ev,
) -> impl View<State, AppAction, GenetCtx, Element = GenetElement>
where
    State: 'static,
    AppAction: 'static,
    Ev: Fn(&mut State, TileEvent) + Clone + 'static,
{
    frisket_with(tree, on_event, |_: &Tile| Slot::Hole)
}

/// Render `tree` as a pane frame, asking `fill` what each stack's active tile
/// puts in its content element.
///
/// A [`Slot::View`] child is rendered through a [`PortableKeyed`] keyed by
/// [`TileId`], so dragging a tile to another stack moves its element rather
/// than rebuilding it: the child keeps its DOM node, its view state and its
/// handlers across the one rebuild that relocates it.
pub fn frisket_with<State, AppAction, Ev, Fill>(
    tree: &TileTree,
    on_event: Ev,
    fill: Fill,
) -> impl View<State, AppAction, GenetCtx, Element = GenetElement>
where
    State: 'static,
    AppAction: 'static,
    Ev: Fn(&mut State, TileEvent) + Clone + 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction> + Clone + 'static,
{
    el::<_, State, AppAction>("div", render_node(tree, &[], &on_event, &fill))
        .attr("class", "frisket-body")
        .attr(
            "style",
            "display: flex; width: 100%; height: 100%; min-height: 0;",
        )
}

fn render_node<State, AppAction, Ev, Fill>(
    node: &TileTree,
    path: &[usize],
    on_event: &Ev,
    fill: &Fill,
) -> PaneView<State, AppAction>
where
    State: 'static,
    AppAction: 'static,
    Ev: Fn(&mut State, TileEvent) + Clone + 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction> + Clone + 'static,
{
    match node {
        TileTree::Split { axis, children } => {
            let dir = match axis {
                SplitAxis::Row => "row",
                SplitAxis::Column => "column",
            };
            let path_attr = encode_pane_path(path);
            // A draggable divider between adjacent children, each carrying its
            // split's path and boundary index so a host can resolve a drag to a
            // `DividerMoved` without keeping its own map.
            let mut items: Vec<PaneView<State, AppAction>> = Vec::new();
            for (index, branch) in children.iter().enumerate() {
                let mut child_path = path.to_vec();
                child_path.push(index);
                let inner = render_node(&branch.tree, &child_path, on_event, fill);
                items.push(Box::new(
                    el::<_, State, AppAction>("div", inner)
                        .attr("class", "frisket-branch")
                        .attr(
                            "style",
                            format!(
                                "flex-grow: {frac}; flex-shrink: {frac}; flex-basis: 0px; min-width: 0; min-height: 0;",
                                frac = branch.fraction
                            ),
                        ),
                ));
                if index + 1 < children.len() {
                    items.push(Box::new(
                        el::<_, State, AppAction>("div", ())
                            .attr("class", "frisket-divider")
                            // An ARIA separator so the divider is reachable and
                            // announced, not just draggable.
                            .attr("role", "separator")
                            .attr(
                                "aria-orientation",
                                match axis {
                                    SplitAxis::Row => "vertical",
                                    SplitAxis::Column => "horizontal",
                                },
                            )
                            .attr(ATTR_DIVIDER, path_attr.clone())
                            .attr(ATTR_DINDEX, index.to_string()),
                    ));
                }
            }
            Box::new(
                el::<_, State, AppAction>("div", items)
                    .attr("class", "frisket-split")
                    .attr("style", format!("display: flex; flex-direction: {dir};")),
            )
        },
        TileTree::Stack(stack) => render_stack(stack, path, on_event, fill),
    }
}

fn render_stack<State, AppAction, Ev, Fill>(
    stack: &TabStack,
    path: &[usize],
    on_event: &Ev,
    fill: &Fill,
) -> PaneView<State, AppAction>
where
    State: 'static,
    AppAction: 'static,
    Ev: Fn(&mut State, TileEvent) + Clone + 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction> + Clone + 'static,
{
    // The stack's tabs as the crate's tab items: the tile's id is the tab's
    // stable key, and a host accent is the item's accent, so a product can tint
    // a tab to match its content without forking the component's CSS.
    let items: Vec<TabItem> = stack
        .tabs
        .iter()
        .map(|tile| {
            let mut item = TabItem::new(tile.title.clone()).with_key(tile.id.0.to_string());
            if let Some(accent) = tile.accent {
                item =
                    item.with_accent(TabAccentColors::new(accent.background, accent.foreground));
            }
            item
        })
        .collect();
    let ids: Vec<TileId> = stack.tabs.iter().map(|tile| tile.id).collect();
    let close_ids = ids.clone();
    let activate_event = on_event.clone();
    let close_event = on_event.clone();
    // The bar carries its stack's path, so a tab dropped on it resolves to
    // `DropTarget::Stack` (merge into this stack) rather than an edge split.
    let bar_attrs = [(ATTR_STACK, encode_pane_path(path))];
    let bar = tab_bar_view(
        TabBar::new(&items, stack.active)
            .with_names(FRISKET_TAB_NAMES)
            .with_attrs(&bar_attrs),
        move |state: &mut State, index: usize| {
            if let Some(id) = ids.get(index) {
                activate_event(state, TileEvent::Activated(*id));
            }
        },
        // The × stops propagation, so a close never also activates.
        Some(move |state: &mut State, index: usize| {
            if let Some(id) = close_ids.get(index) {
                close_event(state, TileEvent::Closed(*id));
            }
        }),
    );

    // The content element: marked with the active tile's id and with which of
    // the three slots it carries. A `View` slot nests the host's child through
    // a `PortableKeyed` keyed by the tile, so the child survives the tile being
    // dragged to another stack; `Surface` and `Hole` stay empty, and what fills
    // their rect is the host's business — which is what lets one frame serve a
    // document, an external texture, or an app's own surface.
    let active_tile = stack.tabs.get(stack.active);
    let active = active_tile.map(|tile| tile.id.0).unwrap_or(0);
    let kind = match active_tile.map(fill) {
        Some(Slot::View(_)) => SlotKind::View,
        Some(Slot::Surface) => SlotKind::Surface,
        Some(Slot::Hole) | None => SlotKind::Hole,
    };
    let children: PortableKeyed<TileId, SlotChild<State, AppAction, Fill>> = match active_tile {
        Some(tile) if kind == SlotKind::View => {
            PortableKeyed::new(vec![(tile.id, SlotChild::new(tile.clone(), fill.clone()))])
        },
        _ => PortableKeyed::new(Vec::new()),
    };
    let content = el::<_, State, AppAction>("div", children)
        .attr("class", "frisket-content")
        .attr(FRISKET_TILE_ATTR, active.to_string())
        .attr(ATTR_SLOT, kind.attr_value());

    Box::new(
        el::<_, State, AppAction>("div", (bar, content))
            .attr("class", "frisket-stack")
            .attr("style", "display: flex; flex-direction: column;"),
    )
}

/// What a divider press resolves to: the split it resizes and which boundary.
/// The host supplies the split's pixel extent from its own layout to turn a drag
/// delta into a fraction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DividerTarget {
    pub path: TilePath,
    pub index: usize,
}

fn attr<D: LayoutDom>(dom: &D, node: D::NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(|value| value.to_string())
}

fn has_class<D: LayoutDom>(dom: &D, node: D::NodeId, class: &str) -> bool {
    attr(dom, node, "class")
        .is_some_and(|value| value.split_whitespace().any(|token| token == class))
}

/// Walk up from a hit node to the divider it belongs to, if any.
///
/// The host hit-tests its laid-out DOM and hands the node here rather than
/// re-implementing the walk; this is the duplication the module exists to end.
pub fn divider_target<D: LayoutDom>(dom: &D, hit: D::NodeId) -> Option<DividerTarget> {
    let mut node = hit;
    loop {
        if let Some(path) = attr(dom, node, ATTR_DIVIDER) {
            return Some(DividerTarget {
                path: decode_pane_path(&path),
                index: attr(dom, node, ATTR_DINDEX)?.parse().ok()?,
            });
        }
        node = dom.parent(node)?;
    }
}

/// Walk up from a hit node to the tab it belongs to. A press on the close ×
/// resolves to `None`, because that press is a close and not the start of a tab
/// drag.
pub fn tab_target<D: LayoutDom>(dom: &D, hit: D::NodeId) -> Option<TileId> {
    if has_class(dom, hit, "tab-close") {
        return None;
    }
    let mut node = hit;
    loop {
        if let Some(id) = attr(dom, node, ATTR_TABKEY).and_then(|s| s.parse::<u64>().ok()) {
            return Some(TileId(id));
        }
        node = dom.parent(node)?;
    }
}

/// Resolve a press on a tab's close affordance to the tile it closes.
/// Returns `None` for every other descendant of the tab.
pub fn close_target<D: LayoutDom>(dom: &D, hit: D::NodeId) -> Option<TileId> {
    if !has_class(dom, hit, "tab-close") {
        return None;
    }
    let mut node = dom.parent(hit)?;
    loop {
        if let Some(id) = attr(dom, node, ATTR_TABKEY).and_then(|s| s.parse::<u64>().ok()) {
            return Some(TileId(id));
        }
        node = dom.parent(node)?;
    }
}

/// Walk up from a content descendant to the active tile whose Frisket hole
/// contains it.
pub fn content_target<D: LayoutDom>(dom: &D, hit: D::NodeId) -> Option<TileId> {
    let mut node = hit;
    loop {
        if let Some(id) = attr(dom, node, FRISKET_TILE_ATTR).and_then(|s| s.parse::<u64>().ok()) {
            return Some(TileId(id));
        }
        node = dom.parent(node)?;
    }
}

/// Walk up from a content descendant to which slot its Frisket content element
/// carries: a host composites into [`SlotKind::Surface`] and
/// [`SlotKind::Hole`] elements and leaves [`SlotKind::View`] ones to the frame.
pub fn slot_kind<D: LayoutDom>(dom: &D, hit: D::NodeId) -> Option<SlotKind> {
    let mut node = hit;
    loop {
        if let Some(value) = attr(dom, node, ATTR_SLOT) {
            return SlotKind::from_attr(&value);
        }
        node = dom.parent(node)?;
    }
}

/// Walk up from a hit node to the stack whose tab bar it is in.
pub fn stack_target<D: LayoutDom>(dom: &D, hit: D::NodeId) -> Option<TilePath> {
    let mut node = hit;
    loop {
        if let Some(path) = attr(dom, node, ATTR_STACK) {
            return Some(decode_pane_path(&path));
        }
        node = dom.parent(node)?;
    }
}

/// Where a tab dropped at `x` would land in the tab bar under `hit`: the stack's
/// path and the insertion index, counting the tabs whose horizontal centre sits
/// left of the cursor.
///
/// The one gesture that needs real geometry, so the rects arrive through
/// `rect_of` (`(x, y, w, h)`, host space) instead of this module growing a
/// layout dependency.
pub fn tab_drop_index<D: LayoutDom>(
    dom: &D,
    hit: D::NodeId,
    x: f32,
    rect_of: impl Fn(D::NodeId) -> Option<(f32, f32, f32, f32)>,
) -> Option<(TilePath, usize)> {
    let mut node = hit;
    let bar = loop {
        if attr(dom, node, ATTR_STACK).is_some() {
            break node;
        }
        node = dom.parent(node)?;
    };
    let path = decode_pane_path(&attr(dom, bar, ATTR_STACK)?);
    let mut before = 0usize;
    let mut stack = vec![bar];
    while let Some(current) = stack.pop() {
        if attr(dom, current, ATTR_TABKEY).is_some() {
            if let Some((rx, _, rw, _)) = rect_of(current) {
                if rx + rw / 2.0 < x {
                    before += 1;
                }
            }
        }
        for child in dom.dom_children(current) {
            stack.push(child);
        }
    }
    Some((path, before))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use genet_scripted_dom::{NodeId, ScriptedDom};
    use workbench::{ContentSource, DocumentRef, DropTarget, Tile, TileBranch};

    use super::*;
    use crate::{DomHandle, GenetAppRunner, PointerClick};

    type TestView = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
    /// A runner over one of the module's frames, plus the DOM it drew into.
    type Harness = (
        DomHandle,
        GenetAppRunner<State, fn(&State) -> TestView, TestView, ()>,
    );

    struct State {
        tree: TileTree,
        events: Vec<TileEvent>,
    }

    fn tile(id: u64, title: &str) -> Tile {
        Tile {
            id: TileId(id),
            title: title.to_string(),
            content: ContentSource::Document(DocumentRef(format!("doc:{id}"))),
            accent: None,
        }
    }

    /// A row split of two stacks; the left one holds two tabs, second active.
    fn sample() -> TileTree {
        TileTree::Split {
            axis: SplitAxis::Row,
            children: vec![
                TileBranch {
                    fraction: 0.7,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(1, "First"), tile(2, "Second")],
                        active: 1,
                    }),
                },
                TileBranch {
                    fraction: 0.3,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(3, "Third")],
                        active: 0,
                    }),
                },
            ],
        }
    }

    fn view(state: &State) -> TestView {
        Box::new(frisket(&state.tree, |state: &mut State, event| {
            state.events.push(event)
        }))
    }

    fn harness() -> Harness {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = State {
            tree: sample(),
            events: Vec::new(),
        };
        let runner = GenetAppRunner::new(dom.clone(), view as fn(&State) -> TestView, state);
        (dom, runner)
    }

    fn attr_of<'a>(dom: &'a ScriptedDom, node: NodeId, name: &str) -> Option<&'a str> {
        dom.attribute(node, &Namespace::default(), &LocalName::from(name))
    }

    fn find_attr(dom: &ScriptedDom, root: NodeId, name: &str, value: &str) -> Option<NodeId> {
        if attr_of(dom, root, name) == Some(value) {
            return Some(root);
        }
        dom.dom_children(root)
            .find_map(|child| find_attr(dom, child, name, value))
    }

    fn find_class(dom: &ScriptedDom, root: NodeId, class: &str) -> Option<NodeId> {
        if has_class(dom, root, class) {
            return Some(root);
        }
        dom.dom_children(root)
            .find_map(|child| find_class(dom, child, class))
    }

    fn count_class(dom: &ScriptedDom, root: NodeId, class: &str) -> usize {
        usize::from(has_class(dom, root, class))
            + dom
                .dom_children(root)
                .map(|child| count_class(dom, child, class))
                .sum::<usize>()
    }

    /// A slot per tile id: 2 gets a child view, 3 a surface, everything else a
    /// hole. A plain `fn` so the fill is `Clone + 'static` without ceremony.
    fn fill_by_id(tile: &Tile) -> Slot<State, ()> {
        match tile.id.0 {
            2 => Slot::View(Box::new(
                el::<_, State, ()>("p", "child").attr("class", "slot-child"),
            )),
            3 => Slot::Surface,
            _ => Slot::Hole,
        }
    }

    fn slot_view(state: &State) -> TestView {
        Box::new(frisket_with(
            &state.tree,
            |state: &mut State, event| state.events.push(event),
            fill_by_id,
        ))
    }

    fn slot_harness() -> Harness {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = State {
            tree: sample(),
            events: Vec::new(),
        };
        let runner = GenetAppRunner::new(dom.clone(), slot_view as fn(&State) -> TestView, state);
        (dom, runner)
    }

    #[test]
    fn a_view_slot_inlines_its_child_and_the_empty_slots_stay_empty() {
        let (dom, runner) = slot_harness();
        let root = runner.root();
        let dom = dom.borrow();

        // Tile 2 is the left stack's active tab and fills with a view: the
        // child is inside the content element, which says so.
        let view_slot = find_attr(&dom, root, FRISKET_TILE_ATTR, "2").expect("view content");
        assert_eq!(attr_of(&dom, view_slot, "data-slot"), Some("view"));
        assert_eq!(attr_of(&dom, view_slot, "class"), Some("frisket-content"));
        let child = dom.dom_children(view_slot).next().expect("the slot child");
        assert!(has_class(&*dom, child, "slot-child"));
        assert_eq!(dom.dom_children(view_slot).count(), 1);

        // Tile 3's stack asked for a surface: marked, and left empty for the
        // host to composite into.
        let surface = find_attr(&dom, root, FRISKET_TILE_ATTR, "3").expect("surface content");
        assert_eq!(attr_of(&dom, surface, "data-slot"), Some("surface"));
        assert_eq!(dom.dom_children(surface).count(), 0);

        // And the default frame is all holes, also empty.
        let (hole_dom, hole_runner) = harness();
        let hole_dom = hole_dom.borrow();
        let hole = find_attr(&hole_dom, hole_runner.root(), FRISKET_TILE_ATTR, "2")
            .expect("hole content");
        assert_eq!(attr_of(&hole_dom, hole, "data-slot"), Some("hole"));
        assert_eq!(hole_dom.dom_children(hole).count(), 0);
    }

    #[test]
    fn a_content_element_reports_its_slot_and_still_names_its_tile() {
        let (dom, runner) = slot_harness();
        let root = runner.root();
        let dom = dom.borrow();

        let view_slot = find_attr(&dom, root, FRISKET_TILE_ATTR, "2").expect("view content");
        let surface = find_attr(&dom, root, FRISKET_TILE_ATTR, "3").expect("surface content");
        assert_eq!(slot_kind(&*dom, view_slot), Some(SlotKind::View));
        assert_eq!(slot_kind(&*dom, surface), Some(SlotKind::Surface));
        // A descendant of a view slot answers for the element it is in.
        let child = dom.dom_children(view_slot).next().expect("the slot child");
        assert_eq!(slot_kind(&*dom, child), Some(SlotKind::View));
        // The tab bar is not in any content element, and neither is the root.
        let bar = find_class(&dom, root, "frisket-tabbar").expect("tab bar");
        assert_eq!(slot_kind(&*dom, bar), None);
        assert_eq!(slot_kind(&*dom, root), None);

        // The slot mark rides beside the tile mark; neither displaces the other.
        assert_eq!(content_target(&*dom, child), Some(TileId(2)));
        assert_eq!(content_target(&*dom, surface), Some(TileId(3)));

        let (hole_dom, hole_runner) = harness();
        let hole_dom = hole_dom.borrow();
        let hole = find_class(&hole_dom, hole_runner.root(), "frisket-content").expect("hole");
        assert_eq!(slot_kind(&*hole_dom, hole), Some(SlotKind::Hole));
    }

    /// The tear-out contract, through the frame: a tile dragged into another
    /// stack takes its content child's DOM node with it, rather than the child
    /// being torn down here and rebuilt there. (`PortableKeyed`, moveBefore S5.)
    #[test]
    fn a_view_slot_keeps_its_element_when_its_tile_moves_to_another_stack() {
        fn every_tile(tile: &Tile) -> Slot<State, ()> {
            Slot::View(Box::new(
                el::<_, State, ()>("p", format!("tile {}", tile.id.0)).attr("class", "slot-child"),
            ))
        }
        fn view(state: &State) -> TestView {
            Box::new(frisket_with(
                &state.tree,
                |state: &mut State, event| state.events.push(event),
                every_tile,
            ))
        }

        // Two stacks, both keeping a spare tab so neither empties and collapses
        // the split out from under the move.
        let tree = TileTree::Split {
            axis: SplitAxis::Row,
            children: vec![
                TileBranch {
                    fraction: 0.5,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(1, "First"), tile(4, "Fourth")],
                        active: 0,
                    }),
                },
                TileBranch {
                    fraction: 0.5,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(3, "Third")],
                        active: 0,
                    }),
                },
            ],
        };
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let mut runner = GenetAppRunner::new(
            dom.clone(),
            view as fn(&State) -> TestView,
            State {
                tree,
                events: Vec::new(),
            },
        );
        let root = runner.root();
        let (left_content, right_content, moved) = {
            let dom = dom.borrow();
            let left = find_attr(&dom, root, FRISKET_TILE_ATTR, "1").expect("left content");
            let right = find_attr(&dom, root, FRISKET_TILE_ATTR, "3").expect("right content");
            let moved = dom.dom_children(left).next().expect("tile 1's slot child");
            (left, right, moved)
        };

        // Drag tile 1 into the right-hand stack. The left stack rebuilds first
        // in tree order, so it parks the child and the right stack adopts it.
        runner.update(|state| {
            state.tree.apply(&TileEvent::Dragged {
                tile: TileId(1),
                to: DropTarget::Stack {
                    stack: TilePath(vec![1]),
                    index: 0,
                },
            });
        });

        let dom = dom.borrow();
        assert_eq!(
            attr_of(&dom, right_content, FRISKET_TILE_ATTR),
            Some("1"),
            "the dropped tile is active in the stack it landed in"
        );
        assert_eq!(
            attr_of(&dom, left_content, FRISKET_TILE_ATTR),
            Some("4"),
            "the stack it left falls back to its other tab"
        );
        assert_eq!(
            dom.dom_children(right_content).next(),
            Some(moved),
            "the same node moved; the child was not rebuilt in its new stack"
        );
        assert_eq!(dom.parent(moved), Some(right_content));
        assert_eq!(
            dom.dom_children(left_content).count(),
            1,
            "the vacated stack shows its new active tile's own child"
        );
        assert_ne!(
            dom.dom_children(left_content).next(),
            Some(moved),
            "and that child is a different element"
        );
    }

    #[test]
    fn a_pane_path_round_trips() {
        for path in [vec![], vec![0], vec![1, 0, 2]] {
            let encoded = encode_pane_path(&path);
            assert_eq!(decode_pane_path(&encoded), TilePath(path));
        }
    }

    #[test]
    fn the_frame_draws_splits_dividers_stacks_and_one_content_hole_per_stack() {
        let (dom, runner) = harness();
        let root = runner.root();
        let dom = dom.borrow();

        assert_eq!(count_class(&dom, root, "frisket-stack"), 2);
        // N children means N-1 dividers, never one per child.
        assert_eq!(count_class(&dom, root, "frisket-divider"), 1);
        assert_eq!(count_class(&dom, root, "frisket-tabbar"), 2);
        assert_eq!(count_class(&dom, root, "frisket-content"), 2);
        assert_eq!(count_class(&dom, root, "frisket-tab"), 3);

        // The content hole names the ACTIVE tile, the second tab here.
        let hole = find_attr(&dom, root, FRISKET_TILE_ATTR, "2").expect("active content hole");
        assert!(has_class(&*dom, hole, "frisket-content"));

        // Fractions ride on the branch as flex-grow, so layout sizes the panes.
        let split = find_class(&dom, root, "frisket-split").expect("split");
        let branch = dom
            .dom_children(split)
            .find(|child| has_class(&*dom, *child, "frisket-branch"))
            .expect("first branch");
        assert!(
            attr_of(&dom, branch, "style").is_some_and(|s| {
                s.contains("flex-grow: 0.7")
                    && s.contains("flex-shrink: 0.7")
                    && s.contains("flex-basis: 0px")
            }),
            "style {:?}",
            attr_of(&dom, branch, "style")
        );
    }

    #[test]
    fn a_tab_is_an_aria_tab_and_only_the_active_one_is_selected() {
        let (dom, runner) = harness();
        let root = runner.root();
        let dom = dom.borrow();
        let first = find_attr(&dom, root, ATTR_TABID, "1").expect("first tab");
        let second = find_attr(&dom, root, ATTR_TABID, "2").expect("second tab");
        assert_eq!(attr_of(&dom, first, "role"), Some("tab"));
        assert_eq!(attr_of(&dom, first, "aria-selected"), Some("false"));
        assert_eq!(attr_of(&dom, second, "aria-selected"), Some("true"));
        assert_eq!(attr_of(&dom, first, "aria-label"), Some("First"));
        // The divider announces itself as a separator with an orientation.
        let divider = find_class(&dom, root, "frisket-divider").expect("divider");
        assert_eq!(attr_of(&dom, divider, "role"), Some("separator"));
        assert_eq!(attr_of(&dom, divider, "aria-orientation"), Some("vertical"));
    }

    #[test]
    fn gestures_report_events_and_never_mutate_the_tree() {
        let (dom, mut runner) = harness();
        let root = runner.root();
        let before = runner.state().tree.clone();

        let first = find_attr(&dom.borrow(), root, ATTR_TABID, "1").expect("first tab");
        runner.dispatch_click(first, PointerClick::at((2.0, 2.0)));
        assert_eq!(runner.state().events, [TileEvent::Activated(TileId(1))]);

        // The close x reports a close and stops propagation, so the tab's own
        // activate handler must not also fire.
        let close = find_attr(&dom.borrow(), root, "aria-label", "Close Second").expect("close");
        runner.dispatch_click(close, PointerClick::at((2.0, 2.0)));
        assert_eq!(
            runner.state().events,
            [
                TileEvent::Activated(TileId(1)),
                TileEvent::Closed(TileId(2)),
            ],
            "the close does not also activate its tab"
        );

        assert_eq!(
            runner.state().tree,
            before,
            "the component reports gestures; the caller owns the tree"
        );
    }

    #[test]
    fn a_hit_resolves_to_the_divider_it_belongs_to() {
        let (dom, runner) = harness();
        let root = runner.root();
        let dom = dom.borrow();
        let divider = find_class(&dom, root, "frisket-divider").expect("divider");
        assert_eq!(
            divider_target(&*dom, divider),
            Some(DividerTarget {
                path: TilePath(vec![]),
                index: 0,
            }),
            "the root split's first boundary"
        );
        let tab = find_attr(&dom, root, ATTR_TABID, "1").expect("tab");
        assert_eq!(divider_target(&*dom, tab), None);
    }

    #[test]
    fn a_hit_inside_a_tab_resolves_to_its_tile_but_the_close_does_not() {
        let (dom, runner) = harness();
        let root = runner.root();
        let dom = dom.borrow();

        // The label is a child of the tab, so the walk has to climb.
        let tab = find_attr(&dom, root, ATTR_TABID, "3").expect("third tab");
        let label = dom
            .dom_children(tab)
            .find(|child| has_class(&*dom, *child, "frisket-label"))
            .expect("label");
        assert_eq!(tab_target(&*dom, label), Some(TileId(3)));

        let close = find_attr(&dom, root, "aria-label", "Close Third").expect("close");
        assert_eq!(tab_target(&*dom, close), None);
        assert_eq!(close_target(&*dom, close), Some(TileId(3)));
        assert_eq!(close_target(&*dom, tab), None);
    }

    #[test]
    fn a_content_hole_resolves_to_its_active_tile() {
        let (dom, runner) = harness();
        let root = runner.root();
        let dom = dom.borrow();
        let content = find_class(&dom, root, "frisket-content").expect("content hole");
        let active = attr(&*dom, content, FRISKET_TILE_ATTR)
            .and_then(|id| id.parse::<u64>().ok())
            .map(TileId)
            .expect("active tile id");
        assert_eq!(content_target(&*dom, content), Some(active));
        assert_eq!(content_target(&*dom, root), None);
    }

    #[test]
    fn a_tab_bar_hit_resolves_to_its_stack_and_a_drop_index() {
        let (dom, runner) = harness();
        let root = runner.root();
        let dom = dom.borrow();
        let bar = find_class(&dom, root, "frisket-tabbar").expect("first tab bar");
        assert_eq!(stack_target(&*dom, bar), Some(TilePath(vec![0])));

        // Two tabs 100 wide at x=0 and x=100, so centres at 50 and 150. The
        // rects arrive through the closure: no layout engine in this crate.
        let rect_of = |node: NodeId| -> Option<(f32, f32, f32, f32)> {
            match attr_of(&dom, node, ATTR_TABID) {
                Some("1") => Some((0.0, 0.0, 100.0, 40.0)),
                Some("2") => Some((100.0, 0.0, 100.0, 40.0)),
                _ => None,
            }
        };
        assert_eq!(
            tab_drop_index(&*dom, bar, 10.0, rect_of),
            Some((TilePath(vec![0]), 0)),
            "left of both centres inserts first"
        );
        assert_eq!(
            tab_drop_index(&*dom, bar, 120.0, rect_of),
            Some((TilePath(vec![0]), 1))
        );
        assert_eq!(
            tab_drop_index(&*dom, bar, 400.0, rect_of),
            Some((TilePath(vec![0]), 2)),
            "past both centres appends"
        );
    }
}
