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
        if len == 0 {
            return;
        }
        let cur = self.selected.min(len - 1) as isize;
        self.selected = (cur + delta).rem_euclid(len as isize) as usize;
    }
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

/// The one strip. `on_close` present means every tab carries a close control;
/// absent leaves the tab's only child its label text, which is what keeps
/// [`tab_strip`]'s DOM what it always was.
fn strip<Action, Out, F>(
    state: &TabStrip,
    items: &[TabItem],
    on_close: Option<F>,
) -> impl View<TabStrip, Action, GenetCtx, Element = GenetElement> + use<Action, Out, F>
where
    Action: 'static,
    Out: OptionalAction<Action> + 'static,
    F: Fn(usize) -> Out + Clone + 'static,
{
    let len = items.len();
    // One clickable tab per item. The per-tab closures share one type (one
    // closure definition capturing a `usize`), so the `Vec` is homogeneous.
    let tabs: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let selected = i == state.selected;
            // The × is its own control, announced as "Close <label>", and it
            // stops propagation so closing does not also activate.
            let close = on_close.clone().map(|close| {
                on_click(
                    el::<_, TabStrip, Action>("span", "\u{00d7}")
                        .attr("class", "tab-close")
                        .attr("role", "button")
                        .attr("aria-label", format!("Close {}", item.label)),
                    move |_: &mut TabStrip, ev: PointerClick| {
                        ev.stop_propagation();
                        close(i)
                    },
                )
            });
            let mut tab = el::<_, TabStrip, Action>("div", (item.label.clone(), close))
                .attr("role", "tab")
                .attr("aria-selected", if selected { "true" } else { "false" })
                .attr("tabindex", if selected { "0" } else { "-1" })
                .attr("class", if selected { "tab selected" } else { "tab" });
            if let Some(key) = &item.key {
                tab = tab.attr("data-tabkey", key.clone());
            }
            if let Some(accent) = item.accent {
                tab = tab.attr("style", accent.style());
            }
            focusable_if(
                on_key(
                    on_click(tab, move |s: &mut TabStrip, _| s.selected = i),
                    move |s: &mut TabStrip, event| match event.key {
                        Key::Named(NamedKey::ArrowRight) => {
                            s.step(1, len);
                            event.prevent_default();
                        },
                        Key::Named(NamedKey::ArrowLeft) => {
                            s.step(-1, len);
                            event.prevent_default();
                        },
                        Key::Named(NamedKey::Home) => {
                            s.selected = 0;
                            event.prevent_default();
                        },
                        Key::Named(NamedKey::End) if len > 0 => {
                            s.selected = len - 1;
                            event.prevent_default();
                        },
                        _ => {},
                    },
                ),
                selected,
            )
        })
        .collect();
    let strip = el::<_, TabStrip, Action>("div", tabs)
        .attr("role", "tablist")
        .attr(
            "class",
            if state.current {
                "tablist current"
            } else {
                "tablist"
            },
        )
        .attr("aria-label", state.label.clone());
    if state.current {
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
    strip::<Action, (), fn(usize)>(state, items, None)
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
    strip(state, items, Some(on_close))
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
