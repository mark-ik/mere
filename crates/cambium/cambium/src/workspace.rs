/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Workspace: a whole [`workbench::Workspace`] — the tiled frame plus the
//! float layer — as one composition, for a host that owns the model and
//! composites foreign content itself.
//!
//! [`workspace_view`] is [`frisket_with_current`](crate::frisket_with_current)
//! for the tree, and one absolutely-positioned element per visible float over
//! it. Geometry stays proportional: a float's [`RelativeRect`] becomes CSS
//! percentages and its constraints become `min-*`/`max-*` pixels, so the
//! proportion is resolved by the host's style engine and this crate still
//! carries no layout dependency.
//!
//! # The three membranes
//!
//! A pane's state sits at exactly one of three depths, and which one it is
//! decides what survives a move.
//!
//! 1. **The host's app state, keyed by [`TileId`].** Where an in-house pane's
//!    *durable* state belongs — what the pane is showing, its scroll seat, its
//!    draft. The host lenses it into the child view it hands back from `fill`.
//!    It is the only depth that survives a tile moving to another projection
//!    (another window), because it is not view state at all.
//! 2. **[`Component`](crate::Component).** Widget-local ephemera the parent has
//!    no opinion about — a menu's open flag, a field's caret. It lives in the
//!    view tree, so it survives a rebuild, and a move within one projection
//!    (see 3), and nothing beyond that.
//! 3. **[`PortableKeyed`](crate::PortableKeyed), keyed by [`TileId`].** How the
//!    frame nests a [`Slot::View`] child: dragging a tile between stacks *moves*
//!    its element rather than rebuilding it, so the child keeps its DOM node and
//!    its view state — including any `Component` local state under it — across
//!    that move. This holds **within one projection only**.
//!
//! So: a tile moved to a second projection arrives with a fresh element and no
//! view state. That is not a loss to repair here — the host re-renders it from
//! its own keyed state in the new window, which is why durable pane state lives
//! at depth 1. The float layer is inline rather than portable (a float's child
//! is built directly, not through `PortableKeyed`), so moving a tile between
//! the tree and the float layer also rebuilds its child.

use layout_dom_api::{LayoutDom, LocalName, Namespace};
use workbench::{
    FloatSizeConstraints, FloatingTile, RelativeRect, Tile, TileEvent, TileId, Workspace,
    WorkspaceEvent,
};

use crate::pod::GenetElement;
use crate::{
    FRISKET_TILE_ATTR, GenetCtx, PaneView, Slot, SlotKind, TabAccentColors, TabBar, TabBarNames,
    TabItem, View, el, frisket_with_current, slot_kind, tab_bar_view,
};

/// `data-slot`: which of [`SlotKind`]'s three a content element carries. The
/// same mark the frame writes, read back by [`slot_kind`].
const ATTR_SLOT: &str = "data-slot";

/// The names a float's bar wears, matching the frame's, so one product
/// stylesheet dresses both. (Frisket's own copy is private to that module; the
/// tests here pin the two together through the shared readers.)
const FLOAT_TAB_NAMES: TabBarNames = TabBarNames {
    bar: Some("frisket-tabbar"),
    tab: Some("frisket-tab"),
    selected: Some("active"),
    close: Some("frisket-close"),
    label: Some("frisket-label"),
    key_alias: Some("data-tabid"),
};

/// The minimal structural stylesheet for the workspace's own classes: the
/// float layer's overlay box, and a float's clip. Everything else a float
/// needs is inline, because it is per-float geometry. Layers over
/// [`FRISKET_CSS`](crate::FRISKET_CSS); adds no colour of its own.
pub const WORKSPACE_CSS: &str = "\
    .workspace-floats { position: absolute; left: 0; top: 0; width: 100%; height: 100%; pointer-events: none; } \
    .workspace-float { overflow: hidden; pointer-events: auto; }";

/// What [`workspace_view`] draws: the workspace, which tile the host holds
/// current, and whether the float layer is showing.
///
/// Borrowed rather than owned — the host's model is the truth, and this view
/// reads it for the length of one render.
pub struct WorkspaceModel<'a> {
    pub workspace: &'a Workspace,
    /// The host's current tile. Its stack (or its float) draws a current bar.
    pub current: Option<TileId>,
    /// When false only pinned floats are drawn — [`Workspace::visible_floating`]
    /// applies the rule; this view just asks it.
    pub float_layer_visible: bool,
}

/// Render a whole workspace: the tiled frame, then the float layer over it.
///
/// `on_event` receives every gesture either station raises, already in the
/// workspace's vocabulary — the frame's [`TileEvent`]s arrive wrapped as
/// [`WorkspaceEvent::Tile`] — for the caller to apply through
/// [`Workspace::apply`]. Nothing here mutates the model. `fill` answers what a
/// tile's content element holds, the same three [`Slot`]s the frame uses, and
/// is asked the same way at both stations.
///
/// A float's bar activates and closes through the tile vocabulary rather than a
/// float one, because [`Workspace::apply`] already reads those two float-aware:
/// activating a float raises it, closing one removes it.
pub fn workspace_view<State, AppAction, Ev, Fill>(
    model: &WorkspaceModel<'_>,
    on_event: Ev,
    fill: Fill,
) -> impl View<State, AppAction, GenetCtx, Element = GenetElement>
where
    State: 'static,
    AppAction: 'static,
    Ev: Fn(&mut State, WorkspaceEvent) + Clone + 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction> + Clone + 'static,
{
    let tile_event = on_event.clone();
    let frame = frisket_with_current(
        model.workspace.tiled(),
        model.current,
        move |state: &mut State, event: TileEvent| tile_event(state, WorkspaceEvent::Tile(event)),
        fill.clone(),
    );
    let floats: Vec<PaneView<State, AppAction>> = model
        .workspace
        .visible_floating(model.float_layer_visible)
        .into_iter()
        .map(|float| render_float(float, model.current, &on_event, &fill))
        .collect();
    let layer = el::<_, State, AppAction>("div", floats).attr("class", "workspace-floats");
    el::<_, State, AppAction>("div", (frame, layer))
        .attr("class", "workspace")
        .attr("style", "position: relative; width: 100%; height: 100%;")
}

fn render_float<State, AppAction, Ev, Fill>(
    float: &FloatingTile,
    current: Option<TileId>,
    on_event: &Ev,
    fill: &Fill,
) -> PaneView<State, AppAction>
where
    State: 'static,
    AppAction: 'static,
    Ev: Fn(&mut State, WorkspaceEvent) + Clone + 'static,
    Fill: Fn(&Tile) -> Slot<State, AppAction> + Clone + 'static,
{
    let tile = &float.tile;
    let id = tile.id;
    let mut item = TabItem::new(tile.title.clone()).with_key(id.0.to_string());
    if let Some(accent) = tile.accent {
        item = item.with_accent(TabAccentColors::new(accent.background, accent.foreground));
    }
    let items = [item];
    let activate = on_event.clone();
    let close = on_event.clone();
    // One tab, so the index the bar reports is always this tile's.
    let bar = tab_bar_view(
        TabBar::new(&items, 0)
            .with_names(FLOAT_TAB_NAMES)
            .with_current(current == Some(id)),
        move |state: &mut State, _index: usize| {
            activate(state, WorkspaceEvent::Tile(TileEvent::Activated(id)))
        },
        // The × stops propagation, so a close never also raises.
        Some(move |state: &mut State, _index: usize| {
            close(state, WorkspaceEvent::Tile(TileEvent::Closed(id)))
        }),
    );

    // The content element wears the frame's marks, so a host reads a float's
    // rect back with the same `content_target` / `slot_kind` it already uses.
    // A `View` slot is built inline here rather than through `PortableKeyed`:
    // a float is its own subtree, so there is no sibling stack to move between.
    let slot = fill(tile);
    let kind = match slot {
        Slot::View(_) => SlotKind::View,
        Slot::Surface => SlotKind::Surface,
        Slot::Hole => SlotKind::Hole,
    };
    let child: Option<PaneView<State, AppAction>> = match slot {
        Slot::View(view) => Some(view),
        Slot::Surface | Slot::Hole => None,
    };
    let content = el::<_, State, AppAction>("div", child)
        .attr("class", "frisket-content")
        .attr(FRISKET_TILE_ATTR, id.0.to_string())
        .attr(ATTR_SLOT, slot_attr(kind));

    Box::new(
        el::<_, State, AppAction>("div", (bar, content))
            .attr("class", "workspace-float")
            .attr("style", float_style(float))
            .attr(FRISKET_TILE_ATTR, id.0.to_string())
            .attr("role", "group")
            .attr("aria-label", tile.title.clone()),
    )
}

/// The `data-slot` value for a kind. Mirrors the frame's writer; `slot_kind` is
/// the shared reader, and the tests pin the pair.
fn slot_attr(kind: SlotKind) -> &'static str {
    match kind {
        SlotKind::View => "view",
        SlotKind::Surface => "surface",
        SlotKind::Hole => "hole",
    }
}

/// A float's inline style: proportional placement as percentages, the pixel
/// constraints, the stacking order, and the column skeleton its bar and content
/// sit in.
fn float_style(float: &FloatingTile) -> String {
    let RelativeRect {
        x,
        y,
        width,
        height,
    } = float.rect;
    let FloatSizeConstraints {
        min_width,
        min_height,
        max_width,
        max_height,
    } = float.constraints;
    let mut style = format!(
        "position: absolute; left: {}%; top: {}%; width: {}%; height: {}%; \
         min-width: {}px; min-height: {}px; ",
        pct(x),
        pct(y),
        pct(width),
        pct(height),
        px(min_width),
        px(min_height),
    );
    if let Some(max) = max_width {
        style.push_str(&format!("max-width: {}px; ", px(max)));
    }
    if let Some(max) = max_height {
        style.push_str(&format!("max-height: {}px; ", px(max)));
    }
    style.push_str(&format!(
        "z-index: {}; display: flex; flex-direction: column;",
        float.z
    ));
    style
}

/// A fraction as a percentage number, trimmed: `0.25` is `25`, not `25.0000`.
/// Non-finite reads as zero, matching [`FloatingTile::resolve`].
fn pct(fraction: f32) -> String {
    trim(finite(fraction) * 100.0)
}

/// A pixel length, trimmed the same way.
fn px(length: f32) -> String {
    trim(finite(length))
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Four decimals, then strip the trailing zeros — so an f32 that is a hair off
/// a round number does not print its error into the style.
fn trim(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn attr<D: LayoutDom>(dom: &D, node: D::NodeId, name: &str) -> Option<String> {
    dom.attribute(node, &Namespace::default(), &LocalName::from(name))
        .map(|value| value.to_string())
}

/// Every content element under `root` — frame and floats alike — whose slot the
/// host must fill itself: the [`SlotKind::Surface`] and [`SlotKind::Hole`] ones,
/// with the tile each belongs to and the node whose rect to composite into.
/// [`SlotKind::View`] elements are the frame's own and are left out.
///
/// In document order, so the frame's slots come before the float layer's and
/// the floats follow their z order. A host calls this once a frame, after
/// layout, and has exactly the list of rects it owns.
pub fn composited_slots<D: LayoutDom>(
    dom: &D,
    root: D::NodeId,
) -> Vec<(TileId, SlotKind, D::NodeId)> {
    let mut out = Vec::new();
    collect_slots(dom, root, &mut out);
    out
}

fn collect_slots<D: LayoutDom>(
    dom: &D,
    node: D::NodeId,
    out: &mut Vec<(TileId, SlotKind, D::NodeId)>,
) {
    if attr(dom, node, ATTR_SLOT).is_some() {
        // `slot_kind` answers for the node itself when the node carries the
        // mark, so the frame's reader stays the only decoder.
        if let Some(kind) = slot_kind(dom, node)
            && matches!(kind, SlotKind::Surface | SlotKind::Hole)
            && let Some(id) = attr(dom, node, FRISKET_TILE_ATTR).and_then(|v| v.parse::<u64>().ok())
        {
            out.push((TileId(id), kind, node));
        }
    }
    for child in dom.dom_children(node) {
        collect_slots(dom, child, out);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use genet_scripted_dom::{NodeId, ScriptedDom};
    use workbench::{
        ContentSource, DocumentRef, DropTarget, FloatEvent, SplitAxis, TabStack, TileBranch,
        TilePath, TileTree,
    };

    use super::*;
    use crate::{
        AnyView, COMPONENT_PROBE_ATTR, ComponentView, DomHandle, GenetAppRunner, GenetMultiRunner,
        PointerClick, component, content_target, el, on_click,
    };

    type TestView = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
    /// A runner over one workspace view, plus the DOM it drew into.
    type Harness = (
        DomHandle,
        GenetAppRunner<State, fn(&State) -> TestView, TestView, ()>,
    );

    struct State {
        workspace: Workspace,
        current: Option<TileId>,
        layer_visible: bool,
        events: Vec<WorkspaceEvent>,
    }

    fn tile(id: u64, title: &str) -> Tile {
        Tile {
            id: TileId(id),
            title: title.to_string(),
            content: ContentSource::Document(DocumentRef(format!("doc:{id}"))),
            accent: None,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> RelativeRect {
        RelativeRect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    /// A row split of two stacks (tiles 1+2 left, 3 right), with tiles 4 and 5
    /// floated over it; 5 is pinned and on top.
    fn sample() -> Workspace {
        let mut ws = Workspace::new(TileTree::Split {
            axis: SplitAxis::Row,
            children: vec![
                TileBranch {
                    fraction: 0.5,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(1, "First"), tile(2, "Second")],
                        active: 0,
                    }),
                },
                TileBranch {
                    fraction: 0.5,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(3, "Third"), tile(4, "Fourth"), tile(5, "Fifth")],
                        active: 0,
                    }),
                },
            ],
        });
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(4),
                rect: rect(0.25, 0.5, 0.5, 0.25),
            }
            .into(),
        );
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(5),
                rect: rect(0.1, 0.1, 0.3, 0.3),
            }
            .into(),
        );
        ws.apply(
            &FloatEvent::SetConstraints {
                tile: TileId(4),
                constraints: FloatSizeConstraints {
                    min_width: 300.0,
                    min_height: 120.0,
                    max_width: None,
                    max_height: Some(400.0),
                },
            }
            .into(),
        );
        ws.apply(
            &FloatEvent::SetPinned {
                tile: TileId(5),
                pinned: true,
            }
            .into(),
        );
        ws
    }

    /// Tile 2 gets a surface, tile 4 (a float) a hole, tile 5 (a float) a view.
    fn fill_by_id(tile: &Tile) -> Slot<State, ()> {
        match tile.id.0 {
            2 | 3 => Slot::Surface,
            5 => Slot::View(Box::new(
                el::<_, State, ()>("p", "float child").attr("class", "float-child"),
            )),
            _ => Slot::Hole,
        }
    }

    fn view(state: &State) -> TestView {
        Box::new(workspace_view(
            &WorkspaceModel {
                workspace: &state.workspace,
                current: state.current,
                float_layer_visible: state.layer_visible,
            },
            |state: &mut State, event| state.events.push(event),
            fill_by_id,
        ))
    }

    fn harness(state: State) -> Harness {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = GenetAppRunner::new(dom.clone(), view as fn(&State) -> TestView, state);
        (dom, runner)
    }

    fn sample_state() -> State {
        State {
            workspace: sample(),
            current: None,
            layer_visible: true,
            events: Vec::new(),
        }
    }

    fn attr_of<'a>(dom: &'a ScriptedDom, node: NodeId, name: &str) -> Option<&'a str> {
        dom.attribute(node, &Namespace::default(), &LocalName::from(name))
    }

    fn has_class(dom: &ScriptedDom, node: NodeId, class: &str) -> bool {
        attr_of(dom, node, "class")
            .is_some_and(|value| value.split_whitespace().any(|token| token == class))
    }

    fn all_with_class(dom: &ScriptedDom, root: NodeId, class: &str) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut walk = vec![root];
        while let Some(node) = walk.pop() {
            if has_class(dom, node, class) {
                out.push(node);
            }
            let children: Vec<_> = dom.dom_children(node).collect();
            walk.extend(children.into_iter().rev());
        }
        out
    }

    /// The floats in the order they were drawn (z order).
    fn floats(dom: &ScriptedDom, root: NodeId) -> Vec<NodeId> {
        let layer = all_with_class(dom, root, "workspace-floats");
        assert_eq!(layer.len(), 1, "one float layer");
        dom.dom_children(layer[0]).collect()
    }

    fn float_for(dom: &ScriptedDom, root: NodeId, id: u64) -> NodeId {
        floats(dom, root)
            .into_iter()
            .find(|node| attr_of(dom, *node, FRISKET_TILE_ATTR) == Some(&id.to_string()))
            .unwrap_or_else(|| panic!("float for tile {id}"))
    }

    #[test]
    fn the_frame_and_the_float_layer_both_render_with_the_floats_geometry() {
        let (dom, runner) = harness(sample_state());
        let root = runner.root();
        let dom = dom.borrow();

        assert!(has_class(&dom, root, "workspace"));
        assert_eq!(
            attr_of(&dom, root, "style"),
            Some("position: relative; width: 100%; height: 100%;")
        );
        // The frame is there, with its two stacks and their content elements.
        assert_eq!(all_with_class(&dom, root, "frisket-stack").len(), 2);
        // Two floats over it, in z order: 4 was floated first, 5 raised over it.
        let order: Vec<_> = floats(&dom, root)
            .iter()
            .map(|node| attr_of(&dom, *node, FRISKET_TILE_ATTR).unwrap().to_string())
            .collect();
        assert_eq!(order, ["4", "5"]);

        let four = float_for(&dom, root, 4);
        assert!(has_class(&dom, four, "workspace-float"));
        assert_eq!(attr_of(&dom, four, "role"), Some("group"));
        assert_eq!(attr_of(&dom, four, "aria-label"), Some("Fourth"));
        let style = attr_of(&dom, four, "style").expect("float style");
        // Proportion as percentages; constraints in px; z from the model.
        for fragment in [
            "position: absolute;",
            "left: 25%;",
            "top: 50%;",
            "width: 50%;",
            "height: 25%;",
            "min-width: 300px;",
            "min-height: 120px;",
            "max-height: 400px;",
            "z-index: 1;",
            "display: flex;",
            "flex-direction: column;",
        ] {
            assert!(style.contains(fragment), "{fragment:?} missing from {style:?}");
        }
        assert!(
            !style.contains("max-width"),
            "an absent max emits nothing: {style:?}"
        );
        assert!(
            attr_of(&dom, float_for(&dom, root, 5), "style")
                .is_some_and(|s| s.contains("z-index: 2;")),
            "the raised float stacks over the other"
        );

        // A float's own bar and content, wearing the frame's marks.
        assert_eq!(all_with_class(&dom, four, "frisket-tabbar").len(), 1);
        let content = all_with_class(&dom, four, "frisket-content");
        assert_eq!(content.len(), 1);
        assert_eq!(content_target(&*dom, content[0]), Some(TileId(4)));
        assert_eq!(slot_kind(&*dom, content[0]), Some(SlotKind::Hole));
        assert_eq!(dom.dom_children(content[0]).count(), 0, "a hole is empty");

        // Tile 5's float asked for a view, and got its child inline.
        let five_content = all_with_class(&dom, float_for(&dom, root, 5), "frisket-content");
        assert_eq!(slot_kind(&*dom, five_content[0]), Some(SlotKind::View));
        let child = dom.dom_children(five_content[0]).next().expect("child");
        assert!(has_class(&dom, child, "float-child"));
    }

    #[test]
    fn a_hidden_float_layer_shows_only_the_pinned_floats() {
        let mut state = sample_state();
        state.layer_visible = false;
        let (dom, runner) = harness(state);
        let root = runner.root();
        let dom = dom.borrow();
        let ids: Vec<_> = floats(&dom, root)
            .iter()
            .map(|node| attr_of(&dom, *node, FRISKET_TILE_ATTR).unwrap().to_string())
            .collect();
        assert_eq!(ids, ["5"], "only the pinned float outlives the hidden layer");
    }

    #[test]
    fn composited_slots_lists_the_host_owned_content_of_both_stations() {
        let (dom, runner) = harness(sample_state());
        let root = runner.root();
        let dom = dom.borrow();

        let slots = composited_slots(&*dom, root);
        // Tile 1 (the left stack's active tab) is a hole, tile 3 a surface,
        // float 4 a hole. Float 5 is a view slot and is the frame's business.
        assert_eq!(
            slots
                .iter()
                .map(|(id, kind, _)| (id.0, *kind))
                .collect::<Vec<_>>(),
            [
                (1, SlotKind::Hole),
                (3, SlotKind::Surface),
                (4, SlotKind::Hole),
            ],
            "document order: the frame's slots, then the float layer's"
        );
        for (_, _, node) in &slots {
            assert!(has_class(&dom, *node, "frisket-content"));
        }
        assert!(
            !slots.iter().any(|(id, ..)| id.0 == 5),
            "a view slot is not the host's to composite"
        );
    }

    #[test]
    fn the_current_tile_marks_its_stack_and_its_float() {
        // A tile in the tree: its stack's bar is current, the other is not.
        let mut state = sample_state();
        state.current = Some(TileId(2));
        let (dom, runner) = harness(state);
        let root = runner.root();
        {
            let dom = dom.borrow();
            let bars = all_with_class(&dom, root, "frisket-tabbar");
            let current: Vec<_> = bars
                .iter()
                .filter(|bar| attr_of(&dom, **bar, "aria-current") == Some("true"))
                .collect();
            assert_eq!(current.len(), 1, "exactly one current bar");
            assert_eq!(
                attr_of(&dom, *current[0], "data-stack"),
                Some("0"),
                "the stack that holds tile 2"
            );
        }

        // A floating tile: its float's bar is the current one instead.
        let mut state = sample_state();
        state.current = Some(TileId(4));
        let (dom, runner) = harness(state);
        let root = runner.root();
        let dom = dom.borrow();
        let four = float_for(&dom, root, 4);
        let bar = all_with_class(&dom, four, "frisket-tabbar")[0];
        assert_eq!(attr_of(&dom, bar, "aria-current"), Some("true"));
        assert!(has_class(&dom, bar, "current"));
        assert!(
            all_with_class(&dom, root, "frisket-tabbar")
                .iter()
                .filter(|b| attr_of(&dom, **b, "aria-current") == Some("true"))
                .count()
                == 1,
            "and no stack claims it"
        );
    }

    #[test]
    fn a_floats_bar_reports_activation_and_close_in_the_tile_vocabulary() {
        let (dom, mut runner) = harness(sample_state());
        let root = runner.root();
        let before = runner.state().workspace.clone();

        let (tab, close) = {
            let dom = dom.borrow();
            let four = float_for(&dom, root, 4);
            let tab = all_with_class(&dom, four, "frisket-tab")[0];
            let close = all_with_class(&dom, four, "frisket-close")[0];
            (tab, close)
        };

        runner.dispatch_click(tab, PointerClick::at((2.0, 2.0)));
        assert_eq!(
            runner.state().events,
            [WorkspaceEvent::Tile(TileEvent::Activated(TileId(4)))]
        );

        runner.dispatch_click(close, PointerClick::at((2.0, 2.0)));
        assert_eq!(
            runner.state().events,
            [
                WorkspaceEvent::Tile(TileEvent::Activated(TileId(4))),
                WorkspaceEvent::Tile(TileEvent::Closed(TileId(4))),
            ],
            "the close reports one close and does not also activate"
        );
        assert_eq!(
            runner.state().workspace,
            before,
            "the view reports gestures; the caller owns the model"
        );
    }

    // ---- acceptance: what survives a move, and where ------------------------

    /// The acceptance state: two workspaces (one per projection) over one
    /// shared, `TileId`-keyed note store — the host's depth-1 pane state.
    struct AcceptState {
        spaces: Vec<Workspace>,
        notes: HashMap<u64, String>,
    }

    type AcceptView = Box<dyn AnyView<AcceptState, (), GenetCtx, GenetElement>>;

    /// A counter pane as a `Component`: its bump count is local view state, its
    /// note comes in as a prop from the host's shared store.
    #[derive(Clone, PartialEq)]
    struct CounterProps {
        tile: u64,
        note: String,
    }

    struct CounterLocal {
        bumps: u32,
    }

    fn counter_body(
        props: &CounterProps,
        local: &CounterLocal,
    ) -> ComponentView<CounterLocal, ()> {
        Box::new(
            on_click(
                el::<_, CounterLocal, ()>("div", ())
                    .attr("class", "counter")
                    .attr("data-bumps", local.bumps.to_string())
                    .attr("data-note", props.note.clone()),
                |local: &mut CounterLocal, _: PointerClick| local.bumps += 1,
            ),
        )
    }

    fn counter(tile: u64, note: String) -> AcceptView {
        Box::new(
            component(
                CounterProps { tile, note },
                |_: &CounterProps| CounterLocal { bumps: 0 },
                |_: &CounterProps, _: &CounterProps, _: &mut CounterLocal| {},
                counter_body,
                |_: &mut AcceptState, _: ()| {},
            )
            .probe_id(format!("counter-{tile}")),
        )
    }

    fn accept_state() -> AcceptState {
        // Projection 0: two stacks so the drag has somewhere to land and
        // neither stack empties. Projection 1: a lone tile, the other window.
        let primary = Workspace::new(TileTree::Split {
            axis: SplitAxis::Row,
            children: vec![
                TileBranch {
                    fraction: 0.5,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(1, "Counter"), tile(2, "Spare")],
                        active: 0,
                    }),
                },
                TileBranch {
                    fraction: 0.5,
                    tree: TileTree::Stack(TabStack {
                        tabs: vec![tile(3, "Hole")],
                        active: 0,
                    }),
                },
            ],
        });
        let secondary = Workspace::new(TileTree::single(tile(9, "Elsewhere")));
        AcceptState {
            spaces: vec![primary, secondary],
            notes: HashMap::from([(1u64, "kept by the host".to_string())]),
        }
    }

    /// One projection's view: its own workspace, filled from the shared state.
    fn projection(index: usize) -> impl FnMut(&AcceptState) -> AcceptView {
        move |state: &AcceptState| {
            let notes = state.notes.clone();
            Box::new(workspace_view(
                &WorkspaceModel {
                    workspace: &state.spaces[index],
                    current: None,
                    float_layer_visible: true,
                },
                |_: &mut AcceptState, _| {},
                move |tile: &Tile| match tile.id.0 {
                    1 => Slot::View(counter(
                        1,
                        notes.get(&1).cloned().unwrap_or_default(),
                    )),
                    _ => Slot::Hole,
                },
            ))
        }
    }

    fn probe(dom: &ScriptedDom, root: NodeId, id: &str) -> Option<NodeId> {
        if attr_of(dom, root, COMPONENT_PROBE_ATTR) == Some(id) {
            return Some(root);
        }
        dom.dom_children(root)
            .find_map(|child| probe(dom, child, id))
    }

    /// The acceptance: a `Component` pane keeps its element and its local
    /// counter across a drag within one projection, and keeps neither across a
    /// move to a second projection — where the host's `TileId`-keyed state is
    /// what carries the pane instead.
    #[test]
    fn a_pane_keeps_its_element_and_local_state_within_a_projection_and_neither_across_one() {
        let dom_a: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let dom_b: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let mut runner: GenetMultiRunner<AcceptState, _, AcceptView, ()> =
            GenetMultiRunner::new(accept_state());
        let a = runner.push_projection(dom_a.clone(), projection(0));
        let b = runner.push_projection(dom_b.clone(), projection(1));

        let root_a = runner.root(a).expect("projection a root");
        let counter_node = probe(&dom_a.borrow(), root_a, "counter-1").expect("the counter pane");
        let held = dom_a.borrow().parent(counter_node).expect("its content");
        assert_eq!(
            attr_of(&dom_a.borrow(), counter_node, "data-bumps"),
            Some("0")
        );

        // Bump the component's own local state through a click.
        runner.dispatch_click(a, counter_node, PointerClick::at((2.0, 2.0)));
        runner.dispatch_click(a, counter_node, PointerClick::at((2.0, 2.0)));
        assert_eq!(
            attr_of(&dom_a.borrow(), counter_node, "data-bumps"),
            Some("2"),
            "the component owns its counter across the dispatch rebuild"
        );

        // Drag the tile into the other stack of the SAME projection.
        runner.update(|state| {
            state.spaces[0].apply(&WorkspaceEvent::Tile(TileEvent::Dragged {
                tile: TileId(1),
                to: DropTarget::Stack {
                    stack: TilePath(vec![1]),
                    index: 0,
                },
            }));
        });
        let moved = probe(&dom_a.borrow(), root_a, "counter-1").expect("the counter, moved");
        assert_eq!(
            moved, counter_node,
            "the same element moved; PortableKeyed carried it"
        );
        let landed = dom_a.borrow().parent(moved).expect("its content element");
        assert_ne!(landed, held, "it really did change stacks");
        assert_eq!(content_target(&*dom_a.borrow(), landed), Some(TileId(1)));
        assert_eq!(
            content_target(&*dom_a.borrow(), held),
            Some(TileId(2)),
            "and the stack it left fell back to its spare tab"
        );
        assert_eq!(
            attr_of(&dom_a.borrow(), moved, "data-bumps"),
            Some("2"),
            "and its component-local counter came with it"
        );

        // Now move the tile to the SECOND projection, the way a tear-out to a
        // window does: take it from one workspace and insert it in the other.
        runner.update(|state| {
            let tile = state.spaces[0].take_tile(TileId(1)).expect("tile 1");
            assert!(state.spaces[1].tiled_mut().insert_tab_after(TileId(9), tile));
        });

        assert!(
            probe(&dom_a.borrow(), root_a, "counter-1").is_none(),
            "the tile left the first window"
        );
        let root_b = runner.root(b).expect("projection b root");
        let arrived = probe(&dom_b.borrow(), root_b, "counter-1").expect("the counter, arrived");
        // Node ids are per-document, so comparing one across the two doms
        // proves nothing; what proves the element is new is that the first
        // window no longer holds one (asserted above) and that the arrival
        // carries a freshly-initialised local state.
        assert_eq!(
            attr_of(&dom_b.borrow(), arrived, "data-bumps"),
            Some("0"),
            "and nothing view-local survived the crossing"
        );
        assert_eq!(
            attr_of(&dom_b.borrow(), arrived, "data-note"),
            Some("kept by the host"),
            "what the host keeps keyed by TileId is readable in the new window"
        );
    }
}
