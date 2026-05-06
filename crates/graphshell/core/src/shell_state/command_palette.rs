/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable command-palette session state.
//!
//! The palette's live session — the user's in-progress search query, the
//! scope filter, the keyboard-navigation cursor, and the Tier-1 category
//! selection in contextual mode — is host-neutral runtime state. Host
//! widgets (the egui palette today; the future iced palette) read and
//! mutate this state through the `CommandAuthorityMut` bundle.
//!
//! Pre-M4 slice 3 (2026-04-22) these types lived in
//! `shell/desktop/ui/command_palette_state.rs` inside the graphshell
//! crate, where their unit tests pulled in the full egui + servo
//! dependency graph. Moving them here keeps their tests fast and proves
//! the runtime-ownership line is real.
//!
//! The graphshell crate re-exports these types at their original module
//! path (`shell/desktop/ui/command_palette_state.rs`) so existing
//! `CommandPaletteSession` / `SearchPaletteScope` call sites resolve
//! unchanged.

use crate::actions::ActionCategory;

/// Scope filter for command-palette search results.
///
/// Was previously a private enum inside `render::command_palette`; lifted
/// to the runtime in M4 session 3 so it can live on
/// [`CommandPaletteSession`] and flow through `CommandPaletteViewModel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchPaletteScope {
    CurrentTarget,
    ActivePane,
    ActiveGraph,
    Workbench,
}

impl SearchPaletteScope {
    pub const ALL: [Self; 4] = [
        Self::CurrentTarget,
        Self::ActivePane,
        Self::ActiveGraph,
        Self::Workbench,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CurrentTarget => "Current Target",
            Self::ActivePane => "Active Pane",
            Self::ActiveGraph => "Active Graph",
            Self::Workbench => "Workbench",
        }
    }
}

impl Default for SearchPaletteScope {
    fn default() -> Self {
        Self::Workbench
    }
}

impl core::fmt::Display for SearchPaletteScope {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::CurrentTarget => "current-target",
            Self::ActivePane => "active-pane",
            Self::ActiveGraph => "active-graph",
            Self::Workbench => "workbench",
        };
        f.write_str(s)
    }
}

impl core::str::FromStr for SearchPaletteScope {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim() {
            "current-target" => Ok(Self::CurrentTarget),
            "active-pane" => Ok(Self::ActivePane),
            "active-graph" => Ok(Self::ActiveGraph),
            "workbench" => Ok(Self::Workbench),
            _ => Err(()),
        }
    }
}

/// Live session state for the command palette.
///
/// Holds the user's in-progress search query, the chosen scope filter,
/// the keyboard-navigation cursor, and a one-shot "focus the search
/// field next frame" flag. Consumed by the palette widget through the
/// `CommandAuthorityMut` bundle.
#[derive(Clone, Debug, Default)]
pub struct CommandPaletteSession {
    /// Current search text the user has typed into the palette.
    pub query: String,
    /// Active scope filter for result filtering.
    pub scope: SearchPaletteScope,
    /// Keyboard-navigation cursor into the filtered result list.
    /// `None` means no row is highlighted; when the result list is
    /// non-empty this is clamped to `0..filtered.len()` by the widget
    /// before use.
    pub selected_index: Option<usize>,
    /// Set to `true` when the palette is opened (or re-opened) so the
    /// widget grabs focus for the search field on the next frame. The
    /// widget clears the flag after applying focus.
    pub focus_search_on_next_frame: bool,
    /// Open-state from the previous frame, so the widget can detect
    /// the closed→open transition and call `open_fresh(...)` exactly
    /// once on open. Always set by the widget before it returns.
    pub was_open_last_frame: bool,
    /// Last-selected Tier 1 category in contextual palette mode.
    /// Previously stashed in `egui::Context::data_mut(...)` persistent
    /// storage under the `"command_palette_tier1_category"` key; moved
    /// onto the session in M4 session 3's contextual-mode follow-on.
    /// `None` means "use the first available category" (falls back to
    /// `ActionCategory::Graph` when no ordered categories are
    /// available).
    pub tier1_category: Option<ActionCategory>,
}

impl CommandPaletteSession {
    /// Reset the session to a newly-opened state: empty query, default
    /// scope, no selection, and the focus flag armed so the search field
    /// grabs keyboard focus on the next frame.
    ///
    /// `default_scope` lets the caller pick the initial scope (a
    /// workspace setting in M4 §h).
    pub fn open_fresh(&mut self, default_scope: SearchPaletteScope) {
        self.query.clear();
        self.scope = default_scope;
        self.selected_index = None;
        self.focus_search_on_next_frame = true;
    }

    /// Advance the selection cursor by `delta` (positive cycles down,
    /// negative cycles up), wrapping within `result_count`. A no-op
    /// when `result_count` is zero. Initializes to `0` (down) or
    /// `result_count - 1` (up) if no row is currently selected.
    pub fn step_selection(&mut self, delta: isize, result_count: usize) {
        if result_count == 0 {
            self.selected_index = None;
            return;
        }
        let count = result_count as isize;
        let next = match self.selected_index {
            Some(current) => {
                let current = current as isize;
                ((current + delta).rem_euclid(count)) as usize
            }
            None => {
                if delta >= 0 {
                    0
                } else {
                    (count - 1) as usize
                }
            }
        };
        self.selected_index = Some(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn scope_default_is_workbench() {
        assert_eq!(SearchPaletteScope::default(), SearchPaletteScope::Workbench);
    }

    #[test]
    fn scope_all_enumerates_every_variant_once() {
        // Pin the invariant that adding a new variant requires touching
        // `ALL`. The settings dropdown iterates `ALL` to render scope
        // options; a variant omitted from `ALL` becomes invisible to the
        // user.
        let all = SearchPaletteScope::ALL;
        assert_eq!(all.len(), 4);
        assert!(all.contains(&SearchPaletteScope::CurrentTarget));
        assert!(all.contains(&SearchPaletteScope::ActivePane));
        assert!(all.contains(&SearchPaletteScope::ActiveGraph));
        assert!(all.contains(&SearchPaletteScope::Workbench));
    }

    #[test]
    fn scope_display_and_fromstr_roundtrip() {
        for scope in SearchPaletteScope::ALL {
            let rendered = scope.to_string();
            let parsed = SearchPaletteScope::from_str(&rendered)
                .expect("Display output should parse back via FromStr");
            assert_eq!(parsed, scope, "roundtrip mismatch for {scope:?}");
        }
    }

    #[test]
    fn scope_fromstr_trims_whitespace() {
        assert_eq!(
            SearchPaletteScope::from_str("  workbench  ").unwrap(),
            SearchPaletteScope::Workbench
        );
    }

    #[test]
    fn scope_fromstr_rejects_unknown() {
        assert!(SearchPaletteScope::from_str("neighborhood").is_err());
        assert!(SearchPaletteScope::from_str("").is_err());
    }

    #[test]
    fn scope_serde_uses_variant_names() {
        // The scope is persisted in the session settings document via
        // serde; its discriminants must remain stable or existing user
        // settings will fail to deserialize after an update. Pin the
        // wire shape.
        let json = serde_json::to_string(&SearchPaletteScope::ActiveGraph).unwrap();
        assert_eq!(json, "\"ActiveGraph\"");
        let back: SearchPaletteScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SearchPaletteScope::ActiveGraph);
    }

    #[test]
    fn scope_label_is_stable() {
        assert_eq!(SearchPaletteScope::CurrentTarget.label(), "Current Target");
        assert_eq!(SearchPaletteScope::ActivePane.label(), "Active Pane");
        assert_eq!(SearchPaletteScope::ActiveGraph.label(), "Active Graph");
        assert_eq!(SearchPaletteScope::Workbench.label(), "Workbench");
    }

    #[test]
    fn session_default_is_blank() {
        let s = CommandPaletteSession::default();
        assert!(s.query.is_empty());
        assert_eq!(s.scope, SearchPaletteScope::Workbench);
        assert_eq!(s.selected_index, None);
        assert!(!s.focus_search_on_next_frame);
        assert!(!s.was_open_last_frame);
        assert_eq!(s.tier1_category, None);
    }

    #[test]
    fn open_fresh_clears_query_and_seeds_scope_and_arms_focus() {
        let mut s = CommandPaletteSession {
            query: "leftover".into(),
            scope: SearchPaletteScope::ActivePane,
            selected_index: Some(3),
            focus_search_on_next_frame: false,
            was_open_last_frame: true,
            tier1_category: Some(ActionCategory::Graph),
        };

        s.open_fresh(SearchPaletteScope::CurrentTarget);

        assert!(s.query.is_empty());
        assert_eq!(s.scope, SearchPaletteScope::CurrentTarget);
        assert_eq!(s.selected_index, None);
        assert!(s.focus_search_on_next_frame);
        // open_fresh is called on the closed→open transition, so the
        // widget's bookkeeping fields (`was_open_last_frame`,
        // `tier1_category`) are intentionally preserved — clearing them
        // would destroy the contextual category the user last chose.
        assert!(s.was_open_last_frame);
        assert_eq!(s.tier1_category, Some(ActionCategory::Graph));
    }

    #[test]
    fn step_selection_is_noop_when_no_results() {
        let mut s = CommandPaletteSession {
            selected_index: Some(7),
            ..Default::default()
        };
        s.step_selection(1, 0);
        assert_eq!(s.selected_index, None);
    }

    #[test]
    fn step_selection_initializes_from_none_downward_to_zero() {
        let mut s = CommandPaletteSession::default();
        s.step_selection(1, 5);
        assert_eq!(s.selected_index, Some(0));
    }

    #[test]
    fn step_selection_initializes_from_none_upward_to_last() {
        let mut s = CommandPaletteSession::default();
        s.step_selection(-1, 5);
        assert_eq!(s.selected_index, Some(4));
    }

    #[test]
    fn step_selection_wraps_forward_at_end() {
        let mut s = CommandPaletteSession {
            selected_index: Some(4),
            ..Default::default()
        };
        s.step_selection(1, 5);
        assert_eq!(s.selected_index, Some(0));
    }

    #[test]
    fn step_selection_wraps_backward_at_zero() {
        let mut s = CommandPaletteSession {
            selected_index: Some(0),
            ..Default::default()
        };
        s.step_selection(-1, 5);
        assert_eq!(s.selected_index, Some(4));
    }

    #[test]
    fn step_selection_accepts_large_deltas_via_rem_euclid() {
        // Programmatic advances (e.g. PageDown) can pass multi-step
        // deltas; rem_euclid handles negative deltas correctly whereas
        // plain `%` would yield negative indices. Pin the contract.
        let mut s = CommandPaletteSession {
            selected_index: Some(2),
            ..Default::default()
        };
        s.step_selection(12, 5);
        assert_eq!(s.selected_index, Some((2 + 12) % 5));

        s.selected_index = Some(2);
        s.step_selection(-7, 5);
        // (2 + -7).rem_euclid(5) == 0
        assert_eq!(s.selected_index, Some(0));
    }
}
