/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::rc::Rc;

use genet_scripted_dom::ScriptedDom;
use cambium::{
    AnyView, El, OnClick, OptionalAction, PointerClick, GenetAppRunner, GenetCtx, GenetElement,
    TextField, TextInput, el, lens, memoize, on_click, overlay_at, text_field_typed,
};

use comms::{Direction, ProtocolKind};

use super::command::Command;
use super::nav;
use super::suggest;
use super::{
    Chrome, ContextAction, ContextItem, ContextMenu, CrawlIndicator, HistoryStep,
    ShellbarPaneStates,
};

/// The erased view type meerkat's logic produces, so the toolbar's concrete
/// `El<…>` tuple need not be spelled (it grows as the chrome does).
pub type ChromeView = Box<dyn AnyView<Chrome, (), GenetCtx, GenetElement>>;

/// Logic alias for the runner: chrome state → chrome view tree.
pub type ChromeLogic = fn(&Chrome) -> ChromeView;

/// Record a back step for the host to apply to the **focused node's** own history
/// (the chrome can't reach the orrery). The host drains it via `member_history_back`
/// and mirrors the revealed page back into the toolbar/omnibar; the button's
/// enabled-state (host-set from the node) gates an at-the-root click to a no-op.
fn go_back(c: &mut Chrome, _: PointerClick) {
    c.history_step = Some(HistoryStep::Back);
}

/// Record a forward step. Mirror of [`go_back`].
fn go_forward(c: &mut Chrome, _: PointerClick) {
    c.history_step = Some(HistoryStep::Forward);
}

/// A chrome `<button>`: the shared [`cambium::button`] pinned to the chrome's
/// `(Chrome, ())` view domain and carrying `class`. The single spot meerkat spells
/// a button element, replacing the hand-rolled `on_click(el("button", ..), h)` form
/// at every chrome button. The `<button>` tag stamps `role="button"` for the a11y
/// tree, which the bare `el` form does not advertise.
fn button<F, OA>(
    label: impl Into<String>,
    class: &'static str,
    handler: F,
) -> OnClick<El<String, Chrome, ()>, Chrome, (), F>
where
    F: Fn(&mut Chrome, PointerClick) -> OA + 'static,
    OA: OptionalAction<()>,
{
    cambium::button(label, handler).attr("class", class)
}

/// Mirror the current history entry into the reused chrome state: the toolbar
/// location text and `can_go_*` flags, and the live omnibar buffer (so the bar
/// shows the resolved URL after a navigation). `submitted` raises the one-shot
/// `location_submitted` chrome signal — set for an omnibar Enter, cleared for a
/// back/forward step (which is not a fresh user submission).
///
/// `load_status` is intentionally left untouched here: until the content-root
/// engine reports real progress, forcing `Started` would strand the toolbar in
/// a permanent false "loading" state.
pub(super) fn sync_chrome_from_history(c: &mut Chrome, submitted: bool) {
    let url = c.history.current().to_string();
    c.toolbar.editable.location = url.clone();
    c.toolbar.editable.location_dirty = false;
    c.toolbar.editable.location_submitted = submitted;
    // `can_go_*` is host-driven from the focused node's history (see
    // `Shell::sync_nav_buttons`), not the chrome's suggestions log.
    c.omnibar = TextInput::new(url);
    c.close_suggestions();
}

/// The toolbar chrome as genet DOM: back / forward buttons and an **editable**
/// omnibar — a reused `cambium` [`text_field`](cambium::text_field)
/// over [`Chrome::omnibar`], composed via [`lens`] exactly like pelt-live's
/// field. The host paints its caret and syncs it into the reused `ToolbarState`
/// on submit.
///
/// The chrome-as-DOM seam — meerkat is the next host widget over the graphshell
/// chrome domain, after the egui and iced toolbars.
///
/// `crawl` is passed in rather than read off `Chrome`: the crawl chip is a
/// window-invariant truth that lives once in [`SharedChrome`](crate::SharedChrome), so
/// the shell view threads the shared value in. (One state, N windows — Slice 0.)
pub fn chrome_view(c: &Chrome, crawl: &CrawlIndicator) -> ChromeView {
    // Reflect the reused nav-capability flags onto the buttons: a spent
    // direction carries a `disabled` class (the host sheet greys it; the
    // handler is already a no-op at the history's edge).
    // The nav buttons depend only on their reused capability flag, so memoize
    // each on its bool: typing in the omnibar (which leaves nav state untouched)
    // does not rebuild them. A spent direction carries a `disabled` class (the
    // host sheet greys it; the handler is already a no-op at the history's edge).
    let back = memoize(c.toolbar.can_go_back, |&can_back: &bool| {
        let class = if can_back { "nav" } else { "nav disabled" };
        button("back", class, go_back as fn(&mut Chrome, PointerClick))
    });
    let forward = memoize(c.toolbar.can_go_forward, |&can_forward: &bool| {
        let class = if can_forward { "nav" } else { "nav disabled" };
        button(
            "forward",
            class,
            go_forward as fn(&mut Chrome, PointerClick),
        )
    });
    // The layout-physics pause/play button: ⏸ while running, ▶ while paused, the
    // same toggle as Space. Memoized on the synced `physics_paused` so the glyph
    // flips with the state; the click sets a one-shot intent the host drains into
    // `orrery.toggle_physics_paused`. (Physics pause.)
    let pause = memoize(c.physics_paused, |&paused: &bool| {
        let glyph = if paused { "\u{25b6}" } else { "\u{23f8}" }; // ▶ / ⏸
        button(
            glyph,
            "nav",
            (|c: &mut Chrome, _: PointerClick| c.physics_toggle = true)
                as fn(&mut Chrome, PointerClick),
        )
    });
    // The omnibar stays on the plain single-line field path for now. The styled
    // field renderer emits child-node churn under the `<input>`, which dirties the
    // retained shell session and forces structural rebuilds on ordinary drag/input
    // frames. Keep this lane structurally stable until the highlight path can ride
    // a non-structural presentation seam.
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_omnibar: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.omnibar;
    let omnibar = lens(make, to_omnibar);
    // The tessera/sync status no longer rides the toolbar — it moved into the
    // Steward pane (the live, actionable plane) and the Apparatus pane (the at-rest
    // record), per the static-vs-live split. The host still folds the real
    // `SyncStatus` into `c.sync`; the panes read it from there. (Chrome bar P1.)
    // The crawl-progress chip (relational-browse V2): "crawling/crawled: N pages" while
    // a crawl runs and after; hidden when none has run. We toggle a `crawl-chip-hidden`
    // class rather than rely on `:empty` — an empty-string text view still leaves an
    // empty text node child, so `:empty` never matches and the chip would paint as a
    // bare pill while idle.
    let crawl_summary = crawl.summary();
    let crawl_class = if crawl_summary.is_empty() {
        "crawl-chip crawl-chip-hidden"
    } else {
        "crawl-chip"
    };
    let crawl_chip = el::<_, Chrome, ()>("div", crawl_summary).attr("class", crawl_class);
    // The create affordance (Chrome bar P5): a segmented `+node | +tile | +field`
    // group, each firing its add verb directly (no menu). When the toolbar is crowded
    // by session chips it collapses to a split-button (primary +node + a caret that
    // opens the full add menu), so the two toolbar additions don't fight for width.
    // "Add session" is dropped — the session strip owns session creation.
    let add_group = add_group(c);
    // The branch chip (graphlet wiring Phase 2): a tear-out **branch** window shows the
    // anchor it forked from, so it reads as a distinct grouping, not a plain leaf. Like
    // the crawl chip, it is always present and toggles a `branch-chip-hidden` class (a
    // leaf / the primary leaves `branch_label` `None`), keeping the toolbar tuple arity
    // fixed. (Tear-out gestures G3.)
    let branch_summary = c.branch_label.clone().unwrap_or_default();
    let branch_class = if branch_summary.is_empty() {
        "branch-chip branch-chip-hidden"
    } else {
        "branch-chip"
    };
    let branch_chip = el::<_, Chrome, ()>("div", branch_summary).attr("class", branch_class);
    // The session strip (Chrome bar P4): inline chips for the open graph sessions, an
    // overflow `+N ⌄`, and an add `+`. Sits after the omnibar (which flex-grows, pushing
    // the strip to the toolbar's right); the chips moved here out of the shellbar.
    let session_strip = session_strip(c);
    let toolbar = el::<_, Chrome, ()>(
        "div",
        (
            back,
            forward,
            pause,
            branch_chip,
            omnibar,
            session_strip,
            add_group,
            crawl_chip,
        ),
    )
    .attr("class", "toolbar");

    // The suggestions dropdown: one row per reused `OmnibarMatch`, the highlight
    // carrying a distinct class. Empty ⇒ a zero-height `div` (closed). The outer
    // `.chrome` container has no background; the host composites it over the
    // content root, so the toolbar and this dropdown float above the page.
    let rows: Vec<ChromeView> = c
        .suggest
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let class = if c.suggest_active == Some(i) {
                "suggestion-active"
            } else {
                "suggestion"
            };
            let row = on_click(
                el::<_, Chrome, ()>("div", suggest::match_label(m)).attr("class", class),
                move |c: &mut Chrome, _: PointerClick| c.navigate_suggestion(i),
            );
            Box::new(row) as ChromeView
        })
        .collect();
    let suggestions = el::<_, Chrome, ()>("div", rows).attr("class", "suggestions");

    // The chrome tree is a Vec so the optional palette overlay can be appended.
    let mut children: Vec<ChromeView> = vec![Box::new(toolbar), Box::new(suggestions)];
    if c.sessions_overflow_open && c.sessions.len() > SESSION_INLINE_CAP {
        children.push(session_overflow(c));
    }
    if c.palette_open {
        children.push(palette_overlay(c));
    }
    if c.find_open {
        children.push(find_bar(c));
    }
    if c.comms.is_open() {
        children.push(comms_pane(c));
    }
    if c.knot_editor_open {
        children.push(knot_editor_pane(c));
    }
    if let Some(comp) = &c.knot_completion {
        children.push(knot_completion_menu(comp));
    }
    // Shellbar: a pane-toggle strip docked to one window edge (F2.1). Its geometry is
    // set inline each frame by the host via `set_attribute` on the `.shellbar` class
    // node, following the comms-pane pattern. A slim (leaf) window omits it, and the
    // user's hide toggle omits it on a full-chrome window. (MW3 step 4; hide-shellbar.)
    if !c.slim && !c.shellbar_hidden {
        // The shellbar depends only on the pane-open states, so memoize on them:
        // an omnibar keystroke (panes unchanged) skips rebuilding the whole strip.
        children.push(
            Box::new(memoize(c.shellbar_panes, |panes: &ShellbarPaneStates| {
                shellbar_view(panes)
            })) as ChromeView,
        );
    }
    // The context menu floats over everything (it is a transient cursor pop-up).
    if let Some(menu) = &c.context_menu {
        children.push(context_menu_view(menu));
    }
    // The tear-out drag ghost: a small pill carrying the dragged node's title, floated at
    // the live cursor (the host repositions it each frame in `render`). Pointer-events are
    // off so it never intercepts the drag. (Tear-out gestures, GA-5.)
    if let Some(label) = &c.tear_ghost {
        // Point-anchored via the overlay primitive (placeholder origin); the render pass sets
        // the live cursor position each frame, like the submenu's `anchor_point` reposition.
        // Carrying `position: absolute` from `overlay_at` no longer leans on the `.tear-ghost`
        // CSS rule for the positioning kind. (Tear-out gestures, GA-5.)
        children.push(Box::new(
            overlay_at::<_, Chrome, ()>(0.0, 0.0, label.clone()).attr("class", "tear-ghost"),
        ) as ChromeView);
    }
    Box::new(el::<_, Chrome, ()>("div", children).attr("class", "chrome"))
}

/// The right-click context menu: a small panel of action rows floated at the
/// cursor (abs-positioned in window coords). Each row captures its
mod panels;
use panels::*;

/// otherwise classify the typed text into a [`NavTarget`](nav::NavTarget),
/// resolve it to a URL, push history, and mirror into the reused `ToolbarState`
/// + omnibar. Empty input with no highlight is a no-op. The host calls this on
/// Enter in the focused omnibar; the reused domain stays `String`-based.
pub fn submit_omnibar(c: &mut Chrome) {
    if let Some(i) = c.suggest_active {
        c.navigate_suggestion(i);
        return;
    }
    if c.omnibar.text().trim().is_empty() {
        return;
    }
    let url = nav::classify(c.omnibar.text()).resolve();
    c.content_location = url.clone();
    c.history.visit(url);
    sync_chrome_from_history(c, true);
}

/// How many session chips show inline in the toolbar before the rest fold into the
/// `+N ⌄` overflow dropdown. (Chrome bar P4.)
pub(crate) const SESSION_INLINE_CAP: usize = 4;

/// The toolbar create affordance (Chrome bar P5): a segmented `+node | +tile | +field`
/// group whose buttons fire their add verb directly. When the toolbar is crowded by
/// session chips (overflow active) it collapses to a split-button — a primary `+`
/// (add node) plus a caret that opens the full add menu — so it stops fighting the
/// session strip for width. "Add session" is gone (the strip owns it).
fn add_group(c: &Chrome) -> ChromeView {
    if c.sessions.len() > SESSION_INLINE_CAP {
        let primary = button(
            "\u{ff0b}",
            "add-split-primary",
            (|c: &mut Chrome, _: PointerClick| c.pick_context(ContextAction::AddNode))
                as fn(&mut Chrome, PointerClick),
        );
        let caret = button(
            "\u{2304}",
            "add-split-caret",
            (|c: &mut Chrome, ev: PointerClick| c.open_add_menu(ev.local.0, ev.local.1))
                as fn(&mut Chrome, PointerClick),
        );
        return Box::new(
            el::<_, Chrome, ()>(
                "div",
                vec![
                    Box::new(primary) as ChromeView,
                    Box::new(caret) as ChromeView,
                ],
            )
            .attr("class", "add-split"),
        ) as ChromeView;
    }
    let seg = |label: &'static str, action: ContextAction, class: &'static str| -> ChromeView {
        Box::new(button(
            label,
            class,
            move |c: &mut Chrome, _: PointerClick| c.pick_context(action),
        )) as ChromeView
    };
    Box::new(
        el::<_, Chrome, ()>(
            "div",
            vec![
                seg(
                    "\u{ff0b}node",
                    ContextAction::AddNode,
                    "add-seg add-seg-first",
                ),
                seg("\u{ff0b}tile", ContextAction::AddTile, "add-seg"),
                seg(
                    "\u{ff0b}field",
                    ContextAction::AddField,
                    "add-seg add-seg-last",
                ),
            ],
        )
        .attr("class", "add-group"),
    ) as ChromeView
}

/// The toolbar session strip: a chip per open session (up to [`SESSION_INLINE_CAP`]),
/// an overflow `+N ⌄` button when there are more, and an add `+`. A chip's label
/// activates its session (Shift = open beside, resolved host-side); its × closes it.
/// Replaces the host-drawn shellbar switcher. (Chrome bar P4.)
fn session_strip(c: &Chrome) -> ChromeView {
    let mut children: Vec<ChromeView> = Vec::new();
    for chip in c.sessions.iter().take(SESSION_INLINE_CAP) {
        children.push(session_chip(chip));
    }
    let extra = c.sessions.len().saturating_sub(SESSION_INLINE_CAP);
    if extra > 0 {
        let class = if c.sessions_overflow_open {
            "session-overflow-btn session-overflow-btn-open"
        } else {
            "session-overflow-btn"
        };
        children.push(Box::new(on_click(
            el::<_, Chrome, ()>("div", format!("+{extra}\u{2002}\u{2304}")).attr("class", class),
            |c: &mut Chrome, _: PointerClick| c.toggle_sessions_overflow(),
        )) as ChromeView);
    }
    children.push(Box::new(on_click(
        el::<_, Chrome, ()>("div", "\u{ff0b}").attr("class", "session-add"),
        |c: &mut Chrome, _: PointerClick| c.request_create_session(),
    )) as ChromeView);
    Box::new(el::<_, Chrome, ()>("div", children).attr("class", "session-strip")) as ChromeView
}

/// One session chip: an optional mini-graph thumbnail plus a label (both activate
/// the session) and a × that closes it. The thumbnail is a pre-painted data URI
/// (`session_thumbs`, refreshed on session/graph change, never per frame).
fn session_chip(chip: &crate::SessionChip) -> ChromeView {
    let class = if chip.active {
        "session-chip session-chip-active"
    } else {
        "session-chip"
    };
    let id = chip.id;
    let mut children: Vec<ChromeView> = Vec::new();
    if let Some(uri) = &chip.thumb {
        children.push(Box::new(on_click(
            el::<_, Chrome, ()>("img", ())
                .attr("src", uri.clone())
                .attr("class", "session-chip-thumb"),
            move |c: &mut Chrome, _: PointerClick| c.pick_session(id),
        )) as ChromeView);
    }
    children.push(Box::new(on_click(
        el::<_, Chrome, ()>("div", chip.label.clone()).attr("class", "session-chip-label"),
        move |c: &mut Chrome, _: PointerClick| c.pick_session(id),
    )) as ChromeView);
    children.push(Box::new(on_click(
        el::<_, Chrome, ()>("div", "\u{00d7}").attr("class", "session-chip-close"),
        move |c: &mut Chrome, _: PointerClick| c.request_close_session(id),
    )) as ChromeView);
    Box::new(el::<_, Chrome, ()>("div", children).attr("class", class)) as ChromeView
}

/// The session overflow dropdown: the sessions past the inline cap, one clickable row
/// each (activate). Anchored under the toolbar's right like the suggestions list; the
/// host positions it inline. (Chrome bar P4.)
fn session_overflow(c: &Chrome) -> ChromeView {
    let rows: Vec<ChromeView> = c
        .sessions
        .iter()
        .skip(SESSION_INLINE_CAP)
        .map(|chip| {
            let id = chip.id;
            let class = if chip.active {
                "session-overflow-row session-chip-active"
            } else {
                "session-overflow-row"
            };
            Box::new(on_click(
                el::<_, Chrome, ()>("div", chip.label.clone()).attr("class", class),
                move |c: &mut Chrome, _: PointerClick| c.pick_session(id),
            )) as ChromeView
        })
        .collect();
    Box::new(el::<_, Chrome, ()>("div", rows).attr("class", "session-overflow")) as ChromeView
}

/// The shellbar strip: one pane-toggle button per toggle-able pane, each active
/// state mirroring the current frame layout. The host positions the div inline
/// each frame via the `.shellbar` class node, following the comms-pane geometry
/// pattern (F2.1).
fn shellbar_view(panes: &ShellbarPaneStates) -> ChromeView {
    fn btn(label: &'static str, active: bool, cmd: Command) -> ChromeView {
        let class = if active {
            "shellbar-btn-active"
        } else {
            "shellbar-btn"
        };
        Box::new(button(
            label,
            class,
            move |c: &mut Chrome, _: PointerClick| c.run_command(cmd),
        )) as ChromeView
    }
    let buttons: Vec<ChromeView> = vec![
        btn("\u{229e}", panes.workbench, Command::ToggleWorkbench), // ⊞
        btn("\u{2261}", panes.roster, Command::ToggleRoster),       // ≡
        btn("\u{25ce}", panes.gloss, Command::ToggleGloss),         // ◎
        btn("\u{25c8}", panes.trail, Command::ToggleTrail),         // ◈ (was ⇝, no glyph)
        btn("\u{25bd}", panes.alembic, Command::ToggleAlembic), // ▽ distillation funnel (⚗/⚛ are emoji-only)
        btn("\u{2699}", panes.apparatus, Command::ToggleApparatus), // ⚙
        btn("\u{25c9}", panes.inspector, Command::ToggleInspector), // ◉
        btn("\u{2692}", panes.steward, Command::ToggleSteward), // ⚒
        btn("@", panes.comms, Command::ToggleComms),            // @ (was ✉, no glyph)
    ];
    Box::new(el::<_, Chrome, ()>("div", buttons).attr("class", "shellbar"))
}

/// Chrome logic for the standalone chrome runner (tests + the bare chrome-widget path):
/// the crawl chip reads a default idle indicator, since there is no shared chrome cell
/// here. The windowed app uses `shell_view`, which threads the live shared crawl in.
/// (One state, N windows — Slice 0.)
pub(crate) fn chrome_view_standalone(c: &Chrome) -> ChromeView {
    chrome_view(c, &CrawlIndicator::default())
}

/// Build the chrome via a [`GenetAppRunner`] over a fresh [`ScriptedDom`] — the
/// same diff path the windowed host will drive, minus layout / paint. Returns the
/// runner so callers (and tests) can inspect the DOM, dispatch input, and rebuild.
pub fn runner(initial_location: &str) -> GenetAppRunner<Chrome, ChromeLogic, ChromeView> {
    let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
    GenetAppRunner::new(
        dom,
        chrome_view_standalone as ChromeLogic,
        Chrome::new(initial_location),
    )
}
