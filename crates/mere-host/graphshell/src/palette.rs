/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Editor-style action palette.
//!
//! Opens as a centered overlay above the workspace. Lists every
//! registered gpui action with its current keybindings, filters as
//! the user types, and dispatches the selected action on Enter.
//! Escape closes.
//!
//! v0 scope:
//! - Search / filter by display name
//! - Show current bindings per row
//! - Up / Down to navigate, Enter to invoke, Escape to close
//! - Mouse: click a row to invoke
//!
//! v1 (future): inline rebind capture and persistence to the user
//! keymap. The data model already records the action name + current
//! bindings so the rebind UI is mostly a re-render + write path.

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyBinding, MouseButton, MouseUpEvent, Window, actions, div, prelude::*, px, rgb, rgba,
};

use crate::omnibar_input::{OmnibarCancelled, OmnibarInput, OmnibarSubmitted};

actions!(
    mere_palette,
    [
        /// Move the highlighted row up by one.
        Up,
        /// Move the highlighted row down by one.
        Down,
        /// Invoke the highlighted action and close the palette.
        Submit,
        /// Close the palette without invoking anything.
        Cancel,
    ]
);

/// Key-context active while the palette is open. Bindings registered
/// with this context shadow same-key bindings from outer contexts.
pub const KEY_CONTEXT: &str = "Palette";

/// Emitted when the user selects an action to invoke. The host is
/// responsible for materializing the action by name (via
/// `cx.build_action`) and dispatching it.
#[derive(Clone, Debug)]
pub struct PaletteInvoke {
    pub action_name: String,
}

/// Emitted when the palette closes without invoking anything (Escape
/// or click-outside).
#[derive(Clone, Debug)]
pub struct PaletteClosed;

/// One row in the palette list.
#[derive(Clone, Debug)]
pub struct PaletteEntry {
    /// Fully-qualified action name (e.g. `mere::FocusOmnibar`). The
    /// host uses this to materialize and dispatch.
    pub name: &'static str,
    /// Pretty display name: namespace stripped, CamelCase split.
    pub display: String,
    /// Each currently-bound keystroke formatted for display
    /// (e.g. `"⌘L"` on mac, `"Ctrl+L"` on win/linux).
    pub keybinds: Vec<String>,
    /// Doc comment from the action's `actions!` declaration, if any.
    pub documentation: Option<&'static str>,
}

pub struct Palette {
    focus_handle: FocusHandle,
    visible: bool,
    query_input: Entity<OmnibarInput>,
    entries: Vec<PaletteEntry>,
    filtered: Vec<usize>,
    selected: usize,
}

impl EventEmitter<PaletteInvoke> for Palette {}
impl EventEmitter<PaletteClosed> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Palette {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let query_input = cx.new(|cx| OmnibarInput::new("Search actions…", cx));

        // Refilter as the user types in the query field. observe()
        // fires whenever the input calls cx.notify().
        cx.observe(&query_input, |this, input, cx| {
            let q = input.read(cx).content().clone();
            this.refilter(q.as_ref());
            cx.notify();
        })
        .detach();

        // Route IME-style submit (e.g. mac confirm-marked-text) to
        // the same path as the Enter keybind, so the palette behaves
        // identically regardless of how Enter arrived.
        cx.subscribe(
            &query_input,
            |this: &mut Self, _input, _ev: &OmnibarSubmitted, cx| {
                this.submit_selection(cx);
            },
        )
        .detach();
        // Escape inside the query field closes the palette.
        cx.subscribe(
            &query_input,
            |this: &mut Self, _input, _ev: &OmnibarCancelled, cx| {
                this.close(cx);
            },
        )
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            visible: false,
            query_input,
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Open the palette, snapshot the current action registry +
    /// keymap, and focus the query field.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_entries(cx);
        self.visible = true;
        self.query_input.update(cx, |input, cx| {
            input.set_content("", cx);
            input.focus_and_select_all(window, cx);
        });
        cx.notify();
    }

    /// Close the palette and emit `PaletteClosed`.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        self.visible = false;
        cx.emit(PaletteClosed);
        cx.notify();
    }

    /// Toggle open/closed — useful for binding to a single shortcut.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible {
            self.close(cx);
        } else {
            self.open(window, cx);
        }
    }

    /// Snapshot all registered actions and their current bindings.
    fn refresh_entries(&mut self, cx: &mut Context<Self>) {
        let names: Vec<&'static str> = cx.all_action_names().to_vec();
        let keymap_rc = cx.key_bindings();
        let mut entries: Vec<PaletteEntry> = Vec::with_capacity(names.len());
        for name in names {
            let Ok(action) = cx.build_action(name, None) else {
                continue;
            };
            let keybinds = {
                let keymap = keymap_rc.borrow();
                keymap
                    .bindings_for_action(action.as_ref())
                    .map(|b| format_keystrokes(b.keystrokes()))
                    .collect::<Vec<_>>()
            };
            let documentation = cx.action_documentation().get(name).copied();
            entries.push(PaletteEntry {
                name,
                display: prettify_action_name(name),
                keybinds,
                documentation,
            });
        }
        entries.sort_by(|a, b| a.display.cmp(&b.display));
        self.entries = entries;
        self.refilter("");
        self.selected = 0;
    }

    fn refilter(&mut self, query: &str) {
        let q = query.trim().to_lowercase();
        self.filtered = if q.is_empty() {
            (0..self.entries.len()).collect()
        } else {
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    if matches_query(&e.display, &q) || matches_query(&e.name, &q) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect()
        };
        if self.filtered.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len() - 1;
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.filtered.len() - 1;
        } else {
            self.selected -= 1;
        }
        cx.notify();
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.filtered.len();
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        self.submit_selection(cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
    }

    fn submit_selection(&mut self, cx: &mut Context<Self>) {
        let Some(idx) = self.filtered.get(self.selected).copied() else {
            return;
        };
        let name = self.entries[idx].name.to_string();
        self.visible = false;
        cx.emit(PaletteInvoke { action_name: name });
        cx.emit(PaletteClosed);
        cx.notify();
    }
}

impl Palette {
    fn render_row(
        &self,
        visual_idx: usize,
        entry: &PaletteEntry,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bg = if is_selected {
            rgba(0x2d4870ff)
        } else {
            rgba(0x00000000)
        };
        // Clicking a row both selects and invokes it — matching the
        // editor-palette convention. `visual_idx` is the row's index
        // in `self.filtered`, which is what `selected` indexes.
        let click = cx.listener(
            move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                if visual_idx < this.filtered.len() {
                    this.selected = visual_idx;
                    this.submit_selection(cx);
                }
            },
        );

        let mut row = div()
            .id(("palette-row", visual_idx))
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_3()
            .py(px(6.0))
            .bg(bg)
            .hover(|s| s.bg(rgb(0x2a2a2a)))
            .on_mouse_up(MouseButton::Left, click)
            .child(
                div()
                    .flex_1()
                    .text_color(rgb(0xe0e0e0))
                    .child(entry.display.clone()),
            );

        if let Some(doc) = entry.documentation {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x808080))
                    .child(doc.to_string()),
            );
        }
        for key in &entry.keybinds {
            row = row.child(
                div()
                    .text_xs()
                    .px_2()
                    .py(px(2.0))
                    .bg(rgb(0x303030))
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded_sm()
                    .text_color(rgb(0xc0c0c0))
                    .child(key.clone()),
            );
        }
        row.into_any_element()
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            // When hidden, render an empty 0-size div so the entity
            // still has somewhere in the tree to receive events but
            // doesn't paint anything.
            return div().into_any_element();
        }

        let total = self.filtered.len();
        let selected = self.selected;
        let rows: Vec<AnyElement> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(visual_idx, &entry_idx)| {
                let entry = self.entries[entry_idx].clone();
                self.render_row(visual_idx, &entry, visual_idx == selected, cx)
            })
            .collect();

        let footer = format!(
            "{}/{} action{}",
            if total == 0 { 0 } else { selected + 1 },
            total,
            if total == 1 { "" } else { "s" }
        );

        // Backdrop dims the rest of the window and absorbs clicks
        // outside the panel (clicking the backdrop closes).
        let backdrop_click = cx.listener(
            |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                this.close(cx);
            },
        );

        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(80.0))
            .bg(rgba(0x00000060))
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_mouse_up(MouseButton::Left, backdrop_click)
            .child(
                // Inner panel — absorbs clicks so they don't reach
                // the backdrop (which would close the palette).
                div()
                    .w(px(680.0))
                    .max_h(px(520.0))
                    .flex()
                    .flex_col()
                    .bg(rgb(0x1c1c1c))
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded_md()
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_up(MouseButton::Left, |_, _, _| {
                        // Swallow mouse-up so backdrop_click doesn't
                        // fire when clicking inside the panel.
                    })
                    .child(
                        div()
                            .p_2()
                            .border_b_1()
                            .border_color(rgb(0x333333))
                            .child(self.query_input.clone()),
                    )
                    .child(
                        div()
                            .id("palette-list")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_y_scroll()
                            .children(rows),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_xs()
                            .text_color(rgb(0x707070))
                            .border_t_1()
                            .border_color(rgb(0x333333))
                            .child(footer),
                    ),
            )
            .into_any_element()
    }
}

fn matches_query(haystack: &str, q_lower: &str) -> bool {
    haystack.to_lowercase().contains(q_lower)
}

/// Turn `"mere::FocusOmnibar"` into `"Focus Omnibar"`. Strips the
/// namespace, splits CamelCase into space-separated words.
fn prettify_action_name(name: &str) -> String {
    let tail = name.rsplit("::").next().unwrap_or(name);
    let mut out = String::with_capacity(tail.len() + 4);
    for (i, ch) in tail.char_indices() {
        if i > 0 && ch.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Concatenate a multi-keystroke chord into a human-readable string.
/// Single-keystroke bindings are just the keystroke; multi-key chords
/// are space-separated (matching the keymap.json format).
fn format_keystrokes(keystrokes: &[gpui::KeybindingKeystroke]) -> String {
    keystrokes
        .iter()
        .map(|ks| ks.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Default key bindings the palette publishes on its `Palette`
/// context. The host registers these alongside its app keymap.
pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("up", Up, Some(KEY_CONTEXT)),
        KeyBinding::new("down", Down, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", Submit, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(KEY_CONTEXT)),
    ]
}

