/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable toolbar session state.
//!
//! The toolbar's session vocabulary — the user-editable location bar,
//! the per-pane drafts, the viewer's load status + navigation
//! capabilities — is host-neutral session state that belongs on the
//! runtime side of the M4 boundary. Host widgets (egui toolbar, the
//! future iced toolbar) render from these types through the view-
//! model; the shell owns the mutations.
//!
//! Pre-M4 slice 2 (2026-04-22) these types lived in
//! `shell/desktop/ui/gui_state.rs` inside the graphshell crate, which
//! pulled in the full egui + servo dependency graph for their unit
//! tests. Moving them here keeps their tests fast and proves the
//! runtime-ownership line is real.
//!
//! The graphshell crate re-exports these types at their original
//! locations (`shell/desktop/ui/gui_state.rs`) so existing
//! `ToolbarState` / `ToolbarEditable` / `ToolbarDraft` call sites
//! resolve unchanged.

use crate::content::ContentLoadState;

/// The editable subset of a toolbar input surface: the fields the
/// user manipulates as they type and submit. Shared between the
/// live [`ToolbarState`] (what the widget currently renders) and
/// per-pane [`ToolbarDraft`]s (saved snapshots for panes that don't
/// currently own the toolbar).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolbarEditable {
    /// The location bar's current text (URL, search query,
    /// omnibar input, etc.). Widget writes on each keystroke.
    pub location: String,
    /// `true` when the user has edited `location` since the last
    /// webview-side sync. Used to suppress location-from-webview
    /// overwrites while the user is mid-edit.
    pub location_dirty: bool,
    /// One-shot signal set when the user hits Enter / submits the
    /// location bar. Consumer clears after dispatch.
    pub location_submitted: bool,
}

/// Per-pane snapshot of the editable toolbar fields. A draft is
/// structurally identical to [`ToolbarEditable`]; the alias preserves
/// the `ToolbarDraft` name used by persistence sites and view-model
/// fields.
pub type ToolbarDraft = ToolbarEditable;

/// Live toolbar session state.
///
/// Aggregates the editable portion ([`ToolbarEditable`]) with the
/// non-editable viewer-side status a user sees in the toolbar chrome:
/// load status, status text, navigation-capability flags, and the
/// "clear data" two-step confirmation flag.
///
/// The `load_status` field is a portable [`ContentLoadState`] — each
/// content provider (Servo today; iced_webview / Wry / MiddleNet
/// Direct Lane in the future) converts its native load status at
/// the provider boundary.
///
/// This type has no `Default` impl because the realistic
/// initialization path reads `location` from persistence (restoring
/// the last session's pane URL) rather than starting blank; the
/// graphshell crate wires that construction through
/// `GraphshellRuntime::new` / `EguiHost::new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarState {
    /// User-editable location bar subset.
    pub editable: ToolbarEditable,
    /// `true` when the "clear graph and saved data" two-step
    /// confirmation is primed (first click received, awaiting
    /// confirmation click within the deadline window).
    pub show_clear_data_confirm: bool,
    /// Current viewer load status; drives the toolbar chip and
    /// disables stop/reload affordances when the viewer is idle.
    pub load_status: ContentLoadState,
    /// Hover-text the viewer is reporting (usually "Loading X…" or
    /// the URL of a hovered link in the content). `None` when no
    /// status text is active.
    pub status_text: Option<String>,
    /// Whether the active viewer has prior entries in its back
    /// history. Toolbar renders the Back button disabled when false.
    pub can_go_back: bool,
    /// Whether the active viewer has forward entries. Toolbar
    /// renders the Forward button disabled when false.
    pub can_go_forward: bool,
}

impl ToolbarState {
    /// Construct a `ToolbarState` with `location` populated from
    /// `initial_location` and every other field at its "idle,
    /// nothing loaded yet" default. The cold-startup construction
    /// path used by `GraphshellRuntime::new_minimal` and
    /// `EguiHost::new` routes through here so the defaults are
    /// documented in one place.
    pub fn with_initial_location(initial_location: impl Into<String>) -> Self {
        Self {
            editable: ToolbarEditable {
                location: initial_location.into(),
                location_dirty: false,
                location_submitted: false,
            },
            show_clear_data_confirm: false,
            load_status: ContentLoadState::Complete,
            status_text: None,
            can_go_back: false,
            can_go_forward: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_editable_default_is_blank() {
        let ed = ToolbarEditable::default();
        assert!(ed.location.is_empty());
        assert!(!ed.location_dirty);
        assert!(!ed.location_submitted);
    }

    #[test]
    fn toolbar_state_with_initial_location_seeds_location() {
        let state = ToolbarState::with_initial_location("https://example.test");
        assert_eq!(state.editable.location, "https://example.test");
        assert!(!state.editable.location_dirty);
        assert!(!state.editable.location_submitted);
    }

    #[test]
    fn toolbar_state_with_initial_location_defaults_every_other_field() {
        // Pin the cold-startup defaults so a future contributor adding
        // a field to `ToolbarState` has to think about whether it
        // defaults here too. Matches the toolbar chip logic which
        // treats `Complete` as "no loading indicator" and the
        // navigation buttons which grey out on `can_go_*: false`.
        let state = ToolbarState::with_initial_location("");
        assert!(!state.show_clear_data_confirm);
        assert_eq!(state.load_status, ContentLoadState::Complete);
        assert!(state.status_text.is_none());
        assert!(!state.can_go_back);
        assert!(!state.can_go_forward);
    }

    #[test]
    fn toolbar_draft_is_structurally_identical_to_editable() {
        // The `ToolbarDraft = ToolbarEditable` alias is load-bearing:
        // persistence sites store drafts as the exact same shape. Pin
        // the identity so the type alias doesn't silently fork into
        // a distinct struct in the future.
        let d: ToolbarDraft = ToolbarEditable {
            location: "draft-text".into(),
            location_dirty: true,
            location_submitted: false,
        };
        let e: ToolbarEditable = d.clone();
        assert_eq!(d, e);
    }

    #[test]
    fn toolbar_editable_clone_is_independent() {
        // Drafts are cloned off the live editable on pane switch. If
        // clone ever degenerated to a shallow shared reference, draft
        // mutations would leak into the live toolbar. Pin independence.
        let mut original = ToolbarEditable {
            location: "a".into(),
            location_dirty: true,
            location_submitted: true,
        };
        let draft = original.clone();
        original.location.push('!');
        original.location_dirty = false;
        assert_eq!(draft.location, "a");
        assert!(draft.location_dirty);
    }

    #[test]
    fn toolbar_state_clone_is_independent() {
        let mut original = ToolbarState::with_initial_location("start");
        let cloned = original.clone();
        original.editable.location.push_str(" more");
        original.load_status = ContentLoadState::Started;
        assert_eq!(cloned.editable.location, "start");
        assert_eq!(cloned.load_status, ContentLoadState::Complete);
    }

    #[test]
    fn editable_subset_can_round_trip_through_state() {
        // Draft persistence flow: snapshot editable → store → later,
        // apply stored editable back onto state. Verify the
        // roundtrip is lossless.
        let mut state = ToolbarState::with_initial_location("original");
        state.editable.location_dirty = true;
        state.editable.location_submitted = true;

        let draft: ToolbarDraft = state.editable.clone();

        // Simulate focus leaving + returning: reset editable to
        // defaults, then restore from the draft.
        state.editable = ToolbarEditable::default();
        assert!(state.editable.location.is_empty());

        state.editable = draft.clone();
        assert_eq!(state.editable.location, "original");
        assert!(state.editable.location_dirty);
        assert!(state.editable.location_submitted);
    }
}
