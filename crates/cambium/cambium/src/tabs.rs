/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A tab strip: one strip of labelled tabs, one active — `radio_group`'s
//! one-of-N shape wearing the ARIA tabs pattern.
//!
//! Consumer-pull (turnstone, 2026-07-15): three surfaces want the same widget and
//! were each about to hand-roll it — the Roster's data tabs (Nodes / Links /
//! Graphlets / Fields over one `data_grid`), the workbench's tile tabs, and a
//! stacked pane's tabs. One strip, one active index, click or arrow keys to
//! switch.
//!
//! Selection is the caller's state (like the grid's sort and scroll), so the
//! strip renders whatever the caller says is active and reports the change; what
//! a tab *shows* is the caller's business — the strip owns the strip.
//!
//! Roving tabindex per the ARIA tabs pattern: only the active tab is in the tab
//! order, and Left/Right move between tabs (wrapping), so a keyboard reaches the
//! strip once and then arrows within it. Home/End jump to the ends.
//!
//! The host styles the `tablist` container and the `tab` / `tab selected` tabs;
//! this sets no geometry. A strip is a row or a column or a scrolling overflow
//! depending on where it sits, and only the host knows which — so the strip
//! names its parts and leaves their shape alone, per the sheet contract.
//!
//! Three doors onto the same strip, widening as a tab carries more:
//! [`tab_strip`] over bare labels, [`tab_strip_items`] over [`TabItem`]s (a
//! stable `data-tabkey`, a host [`TabAccentColors`] painted inline), and
//! [`tab_strip_closable`] which adds a close control per tab. A strip belonging
//! to the focused stack sets [`TabStrip::with_current`], which marks the
//! `tablist` itself rather than any one tab.
//!
//! All three are [`tab_bar_view`], the DOM-producing core, with
//! `State = TabStrip`. That core is generic over the caller's state, so a host
//! whose activation is not "set an index" — [`frisket`](crate::frisket), whose
//! is "report a [`TileEvent`](workbench::TileEvent)" — draws the same bar
//! rather than hand-rolling a second one. What it wears on top of the shared
//! class names is [`TabBarNames`].

use crate::pod::GenetElement;
use crate::{
    GenetCtx, Key, NamedKey, OptionalAction, PointerClick, View, el, focusable_if, on_click, on_key,
};

/// The state of a tab strip: which tab is active, plus the group's accessible
/// name. Composable onto an app field via [`lens`](crate::lens), like
/// [`RadioGroup`](crate::RadioGroup).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStrip {
    /// Index of the active tab in the `tabs` slice passed to [`tab_strip`]. Out
    /// of range renders every tab inactive.
    pub selected: usize,
    /// Accessible name announced for the strip.
    pub label: String,
    /// Whether this strip is the current one — the host's focused stack, where
    /// several strips share a window. Marks the `tablist`, not a tab.
    pub current: bool,
}

impl TabStrip {
    /// A strip with `selected` active.
    pub fn new(selected: usize) -> Self {
        Self {
            selected,
            label: "Tabs".into(),
            current: false,
        }
    }

    /// Set the accessible name announced for the strip.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Mark this strip as the current one.
    pub fn with_current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// Move the active tab by `delta`, wrapping, over `len` tabs. The arrow-key
    /// step, exposed so a caller can drive the same motion from its own
    /// shortcut. A `len` of 0 leaves the selection alone.
    pub fn step(&mut self, delta: isize, len: usize) {
        self.selected = step_index(self.selected, delta, len);
    }
}

/// Step `from` by `delta` over `len` tabs, wrapping, clamping a stale index
/// first. A `len` of 0 gives `from` back. The arrow-key motion, shared by
/// [`TabStrip::step`] and [`tab_bar_view`]'s key handler.
fn step_index(from: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return from;
    }
    let cur = from.min(len - 1) as isize;
    (cur + delta).rem_euclid(len as isize) as usize
}

impl Default for TabStrip {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A host-owned tab tint: opaque sRGB `background` + `foreground` bytes painted
/// inline on one tab, overriding the theme's tab colors. The host decides the
/// meaning; the strip just carries the two colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabAccentColors {
    /// The tab's fill.
    pub background: [u8; 3],
    /// The tab's label color.
    pub foreground: [u8; 3],
}

impl TabAccentColors {
    /// An accent from a background and a foreground sRGB triple.
    pub fn new(background: [u8; 3], foreground: [u8; 3]) -> Self {
        Self {
            background,
            foreground,
        }
    }

    /// The inline `style` this accent paints on a tab.
    fn style(self) -> String {
        format!(
            "background-color: rgb({}, {}, {}); color: rgb({}, {}, {});",
            self.background[0],
            self.background[1],
            self.background[2],
            self.foreground[0],
            self.foreground[1],
            self.foreground[2],
        )
    }
}

/// One tab: its label, an optional stable key, an optional host accent.
///
/// The key rides as `data-tabkey` so a host (or a driver) can name a tab by
/// something that holds still when the list is reordered, which the positional
/// index does not. `From<&str>` / `From<String>` keep a label-only strip terse.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabItem {
    /// The tab's visible text, and the name announced for its close control.
    pub label: String,
    /// A caller-stable identity, emitted as `data-tabkey` when present.
    pub key: Option<String>,
    /// A host tint painted inline on this tab.
    pub accent: Option<TabAccentColors>,
}

impl TabItem {
    /// A tab showing `label`, with no key and no accent.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            key: None,
            accent: None,
        }
    }

    /// Set the stable key emitted as `data-tabkey`.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Tint this tab.
    pub fn with_accent(mut self, accent: TabAccentColors) -> Self {
        self.accent = Some(accent);
        self
    }
}

impl From<&str> for TabItem {
    fn from(label: &str) -> Self {
        Self::new(label)
    }
}

impl From<String> for TabItem {
    fn from(label: String) -> Self {
        Self::new(label)
    }
}

/// Extra class and attribute names a bar wears *beyond* the shared
/// `tablist` / `tab` / `tab selected` / `tab-close` vocabulary.
///
/// The default names nothing, which is [`tab_strip`]'s DOM. A host whose own
/// stylesheet already addresses its bar — Frisket's `frisket-*` sheet, which
/// products style directly and which this crate may not rename out from under
/// them — names its tokens here, so one bar wears both vocabularies instead of
/// the crate carrying two tab bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabBarNames {
    /// An extra class token on the `tablist`.
    pub bar: Option<&'static str>,
    /// An extra class token on every `tab`.
    pub tab: Option<&'static str>,
    /// An extra class token on the active tab, beside `selected`.
    pub selected: Option<&'static str>,
    /// An extra class token on every `tab-close`.
    pub close: Option<&'static str>,
    /// Wrap the label text in a `span` of this class rather than leaving it
    /// bare; the tab then names itself with `aria-label`, since its text is no
    /// longer its own child.
    pub label: Option<&'static str>,
    /// Emit each item's key under this second attribute name as well as
    /// `data-tabkey`.
    pub key_alias: Option<&'static str>,
}

/// Everything a bar draws: its tabs, which one is active, the group's
/// accessible name, whether it is the current bar, the names it wears, and any
/// extra attributes the host wants on the `tablist` itself (Frisket's
/// `data-stack`).
///
/// A borrowed description, consumed by [`tab_bar_view`]; nothing it borrows outlives
/// the call (labels, keys and attribute values are cloned into the DOM).
pub struct TabBar<'a> {
    /// The tabs, in order.
    pub items: &'a [TabItem],
    /// Index of the active tab. Out of range renders every tab inactive.
    pub selected: usize,
    /// The `aria-label` announced for the bar; empty emits none.
    pub label: &'a str,
    /// Whether this is the host's current bar.
    pub current: bool,
    /// The extra names this bar wears.
    pub names: TabBarNames,
    /// Extra attributes set on the `tablist`, in order, after `aria-label`.
    pub attrs: &'a [(&'a str, String)],
}

impl<'a> TabBar<'a> {
    /// A bar over `items` with `selected` active, unnamed and not current.
    pub fn new(items: &'a [TabItem], selected: usize) -> Self {
        Self {
            items,
            selected,
            label: "",
            current: false,
            names: TabBarNames::default(),
            attrs: &[],
        }
    }

    /// Set the accessible name announced for the bar.
    pub fn with_label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Mark this bar as the current one.
    pub fn with_current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// Wear these extra class and attribute names.
    pub fn with_names(mut self, names: TabBarNames) -> Self {
        self.names = names;
        self
    }

    /// Set extra attributes on the `tablist` element.
    pub fn with_attrs(mut self, attrs: &'a [(&'a str, String)]) -> Self {
        self.attrs = attrs;
        self
    }
}

/// Join a base class token with an optional extra one.
fn class_with(base: &str, extra: Option<&str>) -> String {
    match extra {
        Some(extra) => format!("{base} {extra}"),
        None => base.to_string(),
    }
}

/// **The one tab bar.** A `tablist` of `tab`s over any `State`: activation is
/// whatever `on_activate` does to that state, and `on_close`, when present,
/// gives every tab a `tab-close` control whose click stops propagation so a
/// close never also activates.
///
/// Activation is a state change by construction — a bar switching tabs emits no
/// action — so `on_activate` returns nothing. Closing is the caller's business
/// and bubbles, so `on_close` returns an [`OptionalAction`].
///
/// Roving tabindex per the ARIA tabs pattern: only the active tab is in the tab
/// order, and Left/Right (wrapping) plus Home/End move the active tab by
/// calling `on_activate` with the target index.
///
/// [`tab_strip_items`] and [`tab_strip_closable`] are this with
/// `State = TabStrip`; [`frisket`](crate::frisket) is this with the host's own
/// state and a [`TileEvent`](workbench::TileEvent) per gesture.
pub fn tab_bar_view<State, Action, Out, A, C>(
    bar: TabBar<'_>,
    on_activate: A,
    on_close: Option<C>,
) -> impl View<State, Action, GenetCtx, Element = GenetElement> + use<State, Action, Out, A, C>
where
    State: 'static,
    Action: 'static,
    Out: OptionalAction<Action> + 'static,
    A: Fn(&mut State, usize) + Clone + 'static,
    C: Fn(&mut State, usize) -> Out + Clone + 'static,
{
    let len = bar.items.len();
    let names = bar.names;
    // One clickable tab per item. The per-tab closures share one type (one
    // closure definition capturing a `usize`), so the `Vec` is homogeneous.
    let tabs: Vec<_> = bar
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let selected = i == bar.selected;
            // The × is its own control, announced as "Close <label>", and it
            // stops propagation so closing does not also activate.
            let close = on_close.clone().map(|close| {
                on_click(
                    el::<_, State, Action>("span", "\u{00d7}")
                        .attr("class", class_with("tab-close", names.close))
                        .attr("role", "button")
                        .attr("aria-label", format!("Close {}", item.label)),
                    move |state: &mut State, ev: PointerClick| {
                        ev.stop_propagation();
                        close(state, i)
                    },
                )
            });
            // The label is bare text unless the host named a wrapper span for
            // it; exactly one of the two is ever present.
            let bare = names.label.is_none().then(|| item.label.clone());
            let wrapped = names.label.map(|class| {
                el::<_, State, Action>("span", item.label.clone()).attr("class", class)
            });
            let mut tab = el::<_, State, Action>("div", (bare, wrapped, close))
                .attr("role", "tab")
                .attr("aria-selected", if selected { "true" } else { "false" })
                .attr("tabindex", if selected { "0" } else { "-1" })
                .attr(
                    "class",
                    if selected {
                        class_with(&class_with("tab selected", names.tab), names.selected)
                    } else {
                        class_with("tab", names.tab)
                    },
                );
            if names.label.is_some() {
                // The text lives in a child span, so the tab names itself.
                tab = tab.attr("aria-label", item.label.clone());
            }
            if let Some(key) = &item.key {
                tab = tab.attr("data-tabkey", key.clone());
                if let Some(alias) = names.key_alias {
                    tab = tab.attr(alias, key.clone());
                }
            }
            if let Some(accent) = item.accent {
                tab = tab.attr("style", accent.style());
            }
            let activate = on_activate.clone();
            let arrows = on_activate.clone();
            focusable_if(
                on_key(
                    on_click(tab, move |state: &mut State, _| activate(state, i)),
                    move |state: &mut State, event| {
                        // The key reaches the focused tab, which roving
                        // tabindex keeps as the active one, so `i` is the seat
                        // the motion starts from.
                        let target = match event.key {
                            Key::Named(NamedKey::ArrowRight) => Some(step_index(i, 1, len)),
                            Key::Named(NamedKey::ArrowLeft) => Some(step_index(i, -1, len)),
                            Key::Named(NamedKey::Home) => Some(0),
                            Key::Named(NamedKey::End) if len > 0 => Some(len - 1),
                            _ => None,
                        };
                        if let Some(target) = target {
                            arrows(state, target);
                            event.prevent_default();
                        }
                    },
                ),
                selected,
            )
        })
        .collect();
    let mut strip = el::<_, State, Action>("div", tabs)
        .attr("role", "tablist")
        .attr(
            "class",
            class_with(
                if bar.current { "tablist current" } else { "tablist" },
                names.bar,
            ),
        );
    if !bar.label.is_empty() {
        strip = strip.attr("aria-label", bar.label.to_string());
    }
    for (name, value) in bar.attrs {
        strip = strip.attr(*name, value.clone());
    }
    if bar.current {
        strip.attr("aria-current", "true")
    } else {
        strip
    }
}

/// A tab strip over a [`TabStrip`] and tab labels: one tab per label, clicking
/// one activates it.
///
/// Each tab is a `tab` (or `tab selected`) element with `role="tab"` and
/// `aria-selected`, inside a `tablist` container (`role="tablist"`). Left/Right
/// (and Home/End) move the active tab from the keyboard. `+ use<Action>` keeps
/// the opaque type from borrowing `state` / `tabs` (the labels are cloned in).
///
/// Generic over `Action` (like [`data_grid`](crate::data_grid), unlike the
/// `()`-actioned controls): the strip switching a tab is a state change, not an
/// action, so it emits none — and staying generic lets it sit in a view tree
/// whose siblings DO bubble actions without the caller reaching for
/// [`map_action`](crate::map_action). Compose onto an app field with
/// [`lens`](crate::lens).
///
/// The label-only door onto [`tab_strip_items`]; the DOM is identical.
pub fn tab_strip<Action>(
    state: &TabStrip,
    tabs: &[&str],
) -> impl View<TabStrip, Action, GenetCtx, Element = GenetElement> + use<Action>
where
    Action: 'static,
{
    let items: Vec<TabItem> = tabs.iter().map(|label| TabItem::from(*label)).collect();
    tab_strip_items(state, &items)
}

/// A tab strip over [`TabItem`]s: [`tab_strip`] plus each tab's optional
/// `data-tabkey` and inline accent. Activation is still a state change, so the
/// strip emits no action.
pub fn tab_strip_items<Action>(
    state: &TabStrip,
    items: &[TabItem],
) -> impl View<TabStrip, Action, GenetCtx, Element = GenetElement> + use<Action>
where
    Action: 'static,
{
    tab_bar_view::<TabStrip, Action, (), _, fn(&mut TabStrip, usize)>(
        TabBar::new(items, state.selected)
            .with_label(&state.label)
            .with_current(state.current),
        |s: &mut TabStrip, i| s.selected = i,
        None,
    )
}

/// A tab strip whose tabs can be closed: [`tab_strip_items`] plus, per tab, a
/// `tab-close` control (`role="button"`, announced as "Close <label>").
///
/// Closing is the caller's business, so it bubbles: the control's click calls
/// `on_close(index)` and stops propagation, so a close never also activates the
/// tab. `Out` is an [`OptionalAction`], so a host may return an action, an
/// `Option` of one, or `()` when the close is handled some other way.
pub fn tab_strip_closable<Action, Out, F>(
    state: &TabStrip,
    items: &[TabItem],
    on_close: F,
) -> impl View<TabStrip, Action, GenetCtx, Element = GenetElement> + use<Action, Out, F>
where
    Action: 'static,
    Out: OptionalAction<Action> + 'static,
    F: Fn(usize) -> Out + Clone + 'static,
{
    tab_bar_view(
        TabBar::new(items, state.selected)
            .with_label(&state.label)
            .with_current(state.current),
        |s: &mut TabStrip, i| s.selected = i,
        Some(move |_: &mut TabStrip, i| on_close(i)),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use genet_scripted_dom::{NodeId, ScriptedDom};
    use layout_dom_api::{LayoutDom, LocalName, Namespace};

    use super::*;
    use crate::{DomHandle, GenetAppRunner};

    /// The action a closable strip bubbles in these tests.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Closed(usize);
    impl crate::Action for Closed {}

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

    fn dom_handle() -> DomHandle {
        Rc::new(RefCell::new(ScriptedDom::new()))
    }

    /// Two items, the second keyed and tinted.
    fn items() -> Vec<TabItem> {
        vec![
            TabItem::from("One"),
            TabItem::new("Two")
                .with_key("two")
                .with_accent(TabAccentColors::new([1, 2, 3], [250, 251, 252])),
        ]
    }

    #[test]
    fn new_holds_selection() {
        assert_eq!(TabStrip::new(2).selected, 2);
        assert_eq!(TabStrip::default().selected, 0);
        assert_eq!(TabStrip::default().label, "Tabs");
        assert_eq!(TabStrip::new(0).with_label("Roster").label, "Roster");
        assert!(!TabStrip::default().current);
        assert!(TabStrip::new(0).with_current(true).current);
    }

    #[test]
    fn step_wraps_both_ways() {
        let mut s = TabStrip::new(0);
        s.step(-1, 4);
        assert_eq!(s.selected, 3, "left from the first wraps to the last");
        s.step(1, 4);
        assert_eq!(s.selected, 0, "right from the last wraps to the first");
        s.step(2, 4);
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn step_is_inert_without_tabs_and_clamps_a_stale_selection() {
        let mut s = TabStrip::new(3);
        s.step(1, 0);
        assert_eq!(s.selected, 3, "no tabs: the selection is left alone");
        // A selection past the end (the caller shrank the tab set) clamps before
        // stepping rather than wrapping from a bogus index.
        let mut s = TabStrip::new(9);
        s.step(1, 3);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn a_label_strip_keeps_the_dom_it_always_had() {
        let dom = dom_handle();
        let runner = GenetAppRunner::new(
            dom.clone(),
            |s: &TabStrip| tab_strip::<()>(s, &["One", "Two"]),
            TabStrip::new(1).with_label("Roster"),
        );
        assert_eq!(
            dom.borrow().outer_html(runner.root()),
            "<div role=\"tablist\" class=\"tablist\" aria-label=\"Roster\">\
             <div role=\"tab\" aria-selected=\"false\" tabindex=\"-1\" class=\"tab\">One</div>\
             <div role=\"tab\" aria-selected=\"true\" tabindex=\"0\" class=\"tab selected\">Two</div>\
             </div>",
        );

        // The label door is the item door: same DOM, byte for byte.
        let items = dom_handle();
        let over_items = GenetAppRunner::new(
            items.clone(),
            |s: &TabStrip| tab_strip_items::<()>(s, &[TabItem::from("One"), TabItem::from("Two")]),
            TabStrip::new(1).with_label("Roster"),
        );
        assert_eq!(
            items.borrow().outer_html(over_items.root()),
            dom.borrow().outer_html(runner.root()),
        );
    }

    #[test]
    fn the_current_strip_is_marked_on_the_tablist() {
        let dom = dom_handle();
        let runner = GenetAppRunner::new(
            dom.clone(),
            |s: &TabStrip| tab_strip::<()>(s, &["One"]),
            TabStrip::new(0).with_current(true),
        );
        let dom = dom.borrow();
        let root = runner.root();
        assert_eq!(attr_of(&dom, root, "aria-current"), Some("true"));
        assert_eq!(attr_of(&dom, root, "class"), Some("tablist current"));

        // And a strip that is not current carries neither.
        let plain = dom_handle();
        let plain_runner = GenetAppRunner::new(
            plain.clone(),
            |s: &TabStrip| tab_strip::<()>(s, &["One"]),
            TabStrip::new(0),
        );
        let plain = plain.borrow();
        let root = plain_runner.root();
        assert_eq!(attr_of(&plain, root, "aria-current"), None);
        assert_eq!(attr_of(&plain, root, "class"), Some("tablist"));
    }

    #[test]
    fn an_item_carries_its_key_and_paints_its_accent() {
        let dom = dom_handle();
        let runner = GenetAppRunner::new(
            dom.clone(),
            |s: &TabStrip| tab_strip_items::<()>(s, &items()),
            TabStrip::new(0),
        );
        let dom = dom.borrow();
        let root = runner.root();
        let keyed = find_attr(&dom, root, "data-tabkey", "two").expect("the keyed tab");
        assert_eq!(attr_of(&dom, keyed, "role"), Some("tab"));
        assert_eq!(
            attr_of(&dom, keyed, "style"),
            Some("background-color: rgb(1, 2, 3); color: rgb(250, 251, 252);"),
        );
        // An item with neither carries neither attribute at all.
        let plain = find_attr(&dom, root, "class", "tab selected").expect("the first tab");
        assert_eq!(attr_of(&dom, plain, "data-tabkey"), None);
        assert_eq!(attr_of(&dom, plain, "style"), None);
    }

    #[test]
    fn a_closable_tab_carries_a_close_control() {
        let dom = dom_handle();
        let runner = GenetAppRunner::new(
            dom.clone(),
            |s: &TabStrip| tab_strip_closable(s, &items(), Closed),
            TabStrip::new(0),
        );
        let dom = dom.borrow();
        let close = find_attr(&dom, runner.root(), "aria-label", "Close Two").expect("close");
        assert_eq!(attr_of(&dom, close, "role"), Some("button"));
        assert_eq!(attr_of(&dom, close, "class"), Some("tab-close"));
        // The whole shape: label text, then the close control, inside the tab.
        let tab = find_attr(&dom, runner.root(), "data-tabkey", "two").expect("the second tab");
        assert_eq!(
            dom.outer_html(tab),
            "<div role=\"tab\" aria-selected=\"false\" tabindex=\"-1\" class=\"tab\" \
             data-tabkey=\"two\" \
             style=\"background-color: rgb(1, 2, 3); color: rgb(250, 251, 252);\">Two\
             <span class=\"tab-close\" role=\"button\" aria-label=\"Close Two\">\u{00d7}</span>\
             </div>",
        );
    }

    #[test]
    fn a_close_reports_its_index_and_leaves_the_selection_alone() {
        let dom = dom_handle();
        let mut runner = GenetAppRunner::new(
            dom.clone(),
            |s: &TabStrip| tab_strip_closable(s, &items(), Closed),
            TabStrip::new(0),
        );
        let close = find_attr(&dom.borrow(), runner.root(), "aria-label", "Close Two")
            .expect("the second tab's close");
        let actions = runner.dispatch_click(close, PointerClick::at((1.0, 1.0)));
        assert_eq!(actions, [Closed(1)]);
        assert_eq!(
            runner.state().selected,
            0,
            "the close stops propagation, so it does not also activate its tab"
        );
    }

    #[test]
    fn clicking_a_tab_selects_it_and_reports_nothing() {
        let dom = dom_handle();
        let mut runner = GenetAppRunner::new(
            dom.clone(),
            |s: &TabStrip| tab_strip_closable(s, &items(), Closed),
            TabStrip::new(0),
        );
        let second =
            find_attr(&dom.borrow(), runner.root(), "data-tabkey", "two").expect("the second tab");
        let actions = runner.dispatch_click(second, PointerClick::at((1.0, 1.0)));
        assert!(
            actions.is_empty(),
            "activation is a state change, not an action"
        );
        assert_eq!(runner.state().selected, 1);
    }
}
