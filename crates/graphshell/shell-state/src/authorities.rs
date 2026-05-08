/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable host-facing mutation bundles ("authorities").
//!
//! The M4 runtime extraction bundles per-subsystem mutable references
//! into `*AuthorityMut<'a>` structs so phase-pipeline functions can
//! accept one handle per subsystem instead of threading each field
//! individually. Three of the four bundles are now fully portable:
//!
//! - [`GraphSearchAuthorityMut`] — five `&mut T` refs where every `T`
//!   is portable (`bool`, `String`, `Vec<NodeKey>`, `Option<usize>`).
//! - [`CommandAuthorityMut`] — toggle flag + a reference to the
//!   portable [`CommandPaletteSession`].
//! - [`FocusAuthorityMut`] — focused-node hint + focus-ring animation
//!   bookkeeping. `focus_ring_started_at` uses
//!   [`PortableInstant`](crate::time::PortableInstant); the host
//!   supplies "now" values at call sites (see [`latch_ring`]) rather
//!   than the bundle reaching for a platform clock. Alpha derivation
//!   is no longer on this bundle — production reads
//!   `chrome_ui.focus_ring_settings.duration()` and computes alpha via
//!   [`FocusRingSpec::alpha_at_with_curve`] directly.
//!
//! The remaining bundle stays shell-side:
//!
//! - `ToolbarAuthorityMut` — references
//!   `ProviderSuggestionDriver` (which holds a shell-owned
//!   `crossbeam_channel::Receiver<T>`) and
//!   `&CommandSurfaceTelemetry` (a shell-side `Mutex`-wrapped sink);
//!   both are intentionally non-portable host companions.
//!
//! Pre-M4 slice 10 (2026-04-22) `FocusAuthorityMut` lived in
//! `shell/desktop/ui/gui_state.rs` alongside the prior bundle moves.
//! The graphshell crate re-exports it from its original location so
//! existing import paths still resolve.
//!
//! [`latch_ring`]: FocusAuthorityMut::latch_ring

use crate::command_palette::{CommandPaletteSession, SearchPaletteScope};
use graphshell_core::graph::NodeKey;
use graphshell_core::time::PortableInstant;

/// Host-facing mutation handle for graph-search session state.
///
/// Bundles the five `graph_search_*` fields on `GraphshellRuntime`
/// (`open`, `query`, `filter_mode`, `matches`, `active_match_index`)
/// that previously flowed through `ExecuteUpdateFrameArgs` as
/// individual `&mut` parameters. Callers use [`close`] / [`set_open`] /
/// [`set_query`] instead of field pokes.
///
/// [`close`]: Self::close
/// [`set_open`]: Self::set_open
/// [`set_query`]: Self::set_query
pub struct GraphSearchAuthorityMut<'a> {
    pub open: &'a mut bool,
    pub query: &'a mut String,
    pub filter_mode: &'a mut bool,
    pub matches: &'a mut Vec<NodeKey>,
    pub active_match_index: &'a mut Option<usize>,
}

impl<'a> GraphSearchAuthorityMut<'a> {
    /// Reborrow the bundle with a shorter lifetime. Needed when the
    /// bundle is threaded through nested phase calls without being
    /// moved out of the outer scope.
    pub fn reborrow(&mut self) -> GraphSearchAuthorityMut<'_> {
        GraphSearchAuthorityMut {
            open: &mut *self.open,
            query: &mut *self.query,
            filter_mode: &mut *self.filter_mode,
            matches: &mut *self.matches,
            active_match_index: &mut *self.active_match_index,
        }
    }

    pub fn is_open(&self) -> bool {
        *self.open
    }

    pub fn set_open(&mut self, value: bool) {
        *self.open = value;
    }

    /// Close the graph-search panel and reset its transient state. The
    /// query text is preserved so a subsequent `set_open(true)` restores
    /// it.
    pub fn close(&mut self) {
        *self.open = false;
        self.matches.clear();
        *self.active_match_index = None;
    }

    pub fn query(&self) -> &str {
        self.query.as_str()
    }

    pub fn query_mut(&mut self) -> &mut String {
        &mut *self.query
    }

    pub fn set_query(&mut self, value: impl Into<String>) {
        *self.query = value.into();
    }

    pub fn filter_mode_active(&self) -> bool {
        *self.filter_mode
    }

    pub fn set_filter_mode(&mut self, value: bool) {
        *self.filter_mode = value;
    }

    pub fn toggle_filter_mode(&mut self) {
        *self.filter_mode = !*self.filter_mode;
    }

    pub fn matches(&self) -> &[NodeKey] {
        self.matches.as_slice()
    }

    pub fn set_matches(&mut self, matches: Vec<NodeKey>) {
        *self.matches = matches;
    }

    pub fn active_match_index(&self) -> Option<usize> {
        *self.active_match_index
    }

    pub fn set_active_match_index(&mut self, value: Option<usize>) {
        *self.active_match_index = value;
    }
}

/// Host-facing mutation handle for runtime-owned command-palette state.
///
/// Per the M4 runtime extraction (§3.2 Command routing), the palette's
/// toggle request and its session state (search query, scope filter,
/// selection cursor, focus-on-open flag) live on `GraphshellRuntime`.
/// The widget mutates them through this bundle instead of stashing
/// search state in `egui::Context::data_mut(...)` persistent storage,
/// the pre-M4 pattern that did not survive a host migration.
///
/// The palette's **open flag** (`show_command_palette`) deliberately
/// stays on `graph_app.workspace.chrome_ui`, not on the runtime, and
/// is NOT a member of this bundle. It's one of eight mutually-
/// exclusive modal flags that `app::ux_navigation` manages as a
/// coordinated cluster; lifting any one flag in isolation would split
/// the cluster. Future work: a "modal surface extraction" session
/// lifts the whole cluster to the runtime as a coherent bundle; until
/// then the widget and callers continue to read/write the open flag
/// directly through `graph_app`.
pub struct CommandAuthorityMut<'a> {
    pub toggle_requested: &'a mut bool,
    pub session: &'a mut CommandPaletteSession,
}

impl<'a> CommandAuthorityMut<'a> {
    pub fn reborrow(&mut self) -> CommandAuthorityMut<'_> {
        CommandAuthorityMut {
            toggle_requested: &mut *self.toggle_requested,
            session: &mut *self.session,
        }
    }

    pub fn toggle_requested(&self) -> bool {
        *self.toggle_requested
    }

    pub fn clear_toggle_request(&mut self) {
        *self.toggle_requested = false;
    }

    /// Arm the session for a fresh open: reset query/scope/selection
    /// and request keyboard focus for the search field on the next
    /// frame. The palette's visibility flag (`show_command_palette`)
    /// lives on workspace state and is flipped by the caller; this
    /// method only touches runtime-owned session state.
    pub fn prime_fresh_open(&mut self, default_scope: SearchPaletteScope) {
        self.session.open_fresh(default_scope);
    }

    pub fn session(&self) -> &CommandPaletteSession {
        self.session
    }

    pub fn session_mut(&mut self) -> &mut CommandPaletteSession {
        self.session
    }
}

/// Host-facing mutation handle bundling the focus fields the render
/// / compositor path touches each frame. Replaces the individual
/// `&mut`-field parameters (`focused_node_hint`,
/// `focus_ring_node_key`, `focus_ring_started_at`) that
/// `TileRenderPassArgs` and `PostRenderPhaseArgs` carried pre-M4.1.
///
/// Per the M3.5 runtime boundary design (§3.1 Focus authority), focus
/// policy truth belongs on the runtime. Callers destructure the
/// runtime at the host boundary, assemble a `FocusAuthorityMut`, and
/// pass it down. The render path calls named methods
/// ([`clear_hint`], [`set_hint`], [`latch_ring`], …) instead of
/// dereferencing raw refs.
///
/// `focus_ring_started_at` is a [`PortableInstant`]; the host
/// provides "now" values at each call site rather than the bundle
/// reaching for a platform clock. This keeps the bundle WASM-safe
/// (no `std::time::Instant::now()` dependency).
///
/// [`clear_hint`]: Self::clear_hint
/// [`set_hint`]: Self::set_hint
/// [`latch_ring`]: Self::latch_ring
pub struct FocusAuthorityMut<'a> {
    pub focused_node_hint: &'a mut Option<NodeKey>,
    /// Whether the graph canvas currently owns focus. Read-only this
    /// frame — the value is produced upstream by the focus-authority
    /// projection.
    pub graph_surface_focused: bool,
    pub focus_ring_node_key: &'a mut Option<NodeKey>,
    pub focus_ring_started_at: &'a mut Option<PortableInstant>,
}

impl<'a> FocusAuthorityMut<'a> {
    /// Reborrow the bundle with a shorter lifetime. Needed when the
    /// bundle flows through multiple call sites the borrow checker
    /// can't statically prove are mutually exclusive.
    pub fn reborrow(&mut self) -> FocusAuthorityMut<'_> {
        FocusAuthorityMut {
            focused_node_hint: &mut *self.focused_node_hint,
            graph_surface_focused: self.graph_surface_focused,
            focus_ring_node_key: &mut *self.focus_ring_node_key,
            focus_ring_started_at: &mut *self.focus_ring_started_at,
        }
    }

    /// Current focus hint (the node a pane wants to own focus on this
    /// frame, or `None` when the hint has been cleared).
    pub fn hint(&self) -> Option<NodeKey> {
        *self.focused_node_hint
    }

    /// Whether the graph canvas surface currently owns focus. When
    /// true, pane-level focus mutations should reset the node hint so
    /// the canvas retains input routing.
    pub fn graph_surface_focused(&self) -> bool {
        self.graph_surface_focused
    }

    /// Replace the focus hint with `value`.
    pub fn set_hint(&mut self, value: Option<NodeKey>) {
        *self.focused_node_hint = value;
    }

    /// Clear the focus hint unconditionally.
    pub fn clear_hint(&mut self) {
        *self.focused_node_hint = None;
    }

    /// Clear the focus hint if it currently points at `node_key`.
    /// Returns `true` when the clear fired.
    pub fn clear_hint_if_matches(&mut self, node_key: NodeKey) -> bool {
        if *self.focused_node_hint == Some(node_key) {
            *self.focused_node_hint = None;
            true
        } else {
            false
        }
    }

    /// Latch a new focus-ring animation from a focus transition delta.
    /// Called once per frame after the active-pane focused-node is
    /// resolved; a no-op when `changed_this_frame` is false so the
    /// ring keeps fading toward its current target.
    ///
    /// `now` is the host-supplied monotonic timestamp for this frame;
    /// callers pass their platform's `PortableInstant` shim
    /// (`portable_time::portable_now()` on desktop).
    pub fn latch_ring(
        &mut self,
        changed_this_frame: bool,
        new_focused_node: Option<NodeKey>,
        now: PortableInstant,
    ) {
        if !changed_this_frame {
            return;
        }
        *self.focus_ring_node_key = new_focused_node;
        *self.focus_ring_started_at = new_focused_node.map(|_| now);
    }

    // Note: alpha-derivation methods previously lived here as
    // `ring_alpha`/`ring_alpha_with_curve`. They were removed alongside
    // the vestigial `focus_ring_duration` field — the production render
    // path now sources the duration from `chrome_ui.focus_ring_settings`
    // and computes alpha via `FocusRingSpec::alpha_at_with_curve`
    // directly. See iced-host migration plan §10 (2026-04-23
    // progress log) for the cleanup rationale.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_search_close_clears_matches_but_preserves_query() {
        // The session's query-preservation contract is load-bearing for
        // the toggle UX: users can dismiss the overlay, navigate, and
        // reopen to see the same text. Pin it.
        let mut open = true;
        let mut query = "rust".to_string();
        let mut filter_mode = true;
        let mut matches = vec![NodeKey::new(1), NodeKey::new(2)];
        let mut active_match_index = Some(0);

        let mut authority = GraphSearchAuthorityMut {
            open: &mut open,
            query: &mut query,
            filter_mode: &mut filter_mode,
            matches: &mut matches,
            active_match_index: &mut active_match_index,
        };

        authority.close();

        assert!(!*authority.open);
        assert_eq!(authority.query(), "rust");
        // filter_mode is intentionally NOT reset by close — it's a
        // user preference that persists across open/close cycles.
        assert!(*authority.filter_mode);
        assert!(authority.matches().is_empty());
        assert_eq!(authority.active_match_index(), None);
    }

    #[test]
    fn graph_search_reborrow_preserves_access_after_nested_mutation() {
        let mut open = false;
        let mut query = String::new();
        let mut filter_mode = false;
        let mut matches = Vec::new();
        let mut active_match_index = None;

        let mut outer = GraphSearchAuthorityMut {
            open: &mut open,
            query: &mut query,
            filter_mode: &mut filter_mode,
            matches: &mut matches,
            active_match_index: &mut active_match_index,
        };

        {
            let mut inner = outer.reborrow();
            inner.set_query("async");
            inner.set_open(true);
        }

        assert_eq!(outer.query(), "async");
        assert!(outer.is_open());
    }

    #[test]
    fn graph_search_toggle_filter_mode_flips_independently_of_open() {
        let mut open = false;
        let mut query = String::new();
        let mut filter_mode = false;
        let mut matches = Vec::new();
        let mut active_match_index = None;

        let mut authority = GraphSearchAuthorityMut {
            open: &mut open,
            query: &mut query,
            filter_mode: &mut filter_mode,
            matches: &mut matches,
            active_match_index: &mut active_match_index,
        };

        authority.toggle_filter_mode();
        assert!(authority.filter_mode_active());
        authority.toggle_filter_mode();
        assert!(!authority.filter_mode_active());
    }

    #[test]
    fn command_authority_prime_fresh_open_resets_session_state() {
        let mut toggle = false;
        let mut session = CommandPaletteSession::default();
        session.query = "stale".into();
        session.selected_index = Some(3);

        let mut authority = CommandAuthorityMut {
            toggle_requested: &mut toggle,
            session: &mut session,
        };

        authority.prime_fresh_open(SearchPaletteScope::ActivePane);

        assert_eq!(authority.session().scope, SearchPaletteScope::ActivePane);
        assert!(authority.session().query.is_empty());
        assert_eq!(authority.session().selected_index, None);
        assert!(authority.session().focus_search_on_next_frame);
    }

    #[test]
    fn command_authority_clear_toggle_request_clears_one_shot_flag() {
        // The toggle-requested flag is a one-shot signal: set by the
        // keyboard handler, observed by the palette widget, then
        // cleared. Pin that clear is idempotent.
        let mut toggle = true;
        let mut session = CommandPaletteSession::default();

        let mut authority = CommandAuthorityMut {
            toggle_requested: &mut toggle,
            session: &mut session,
        };

        assert!(authority.toggle_requested());
        authority.clear_toggle_request();
        assert!(!authority.toggle_requested());
        // Idempotent — clearing an already-clear flag is a no-op.
        authority.clear_toggle_request();
        assert!(!authority.toggle_requested());
    }

    #[test]
    fn focus_authority_latch_ring_is_noop_when_nothing_changed() {
        let mut focused_node_hint = Some(NodeKey::new(5));
        let mut focus_ring_node_key = Some(NodeKey::new(1));
        let mut focus_ring_started_at = Some(PortableInstant(500));
        let mut authority = FocusAuthorityMut {
            focused_node_hint: &mut focused_node_hint,
            graph_surface_focused: false,
            focus_ring_node_key: &mut focus_ring_node_key,
            focus_ring_started_at: &mut focus_ring_started_at,
        };

        // changed_this_frame = false — no mutation.
        authority.latch_ring(false, Some(NodeKey::new(99)), PortableInstant(1_000));
        assert_eq!(*authority.focus_ring_node_key, Some(NodeKey::new(1)));
        assert_eq!(*authority.focus_ring_started_at, Some(PortableInstant(500)));
    }

    #[test]
    fn focus_authority_latch_ring_records_new_target_and_timestamp() {
        let mut focused_node_hint = None;
        let mut focus_ring_node_key = None;
        let mut focus_ring_started_at = None;
        let mut authority = FocusAuthorityMut {
            focused_node_hint: &mut focused_node_hint,
            graph_surface_focused: false,
            focus_ring_node_key: &mut focus_ring_node_key,
            focus_ring_started_at: &mut focus_ring_started_at,
        };

        let now = PortableInstant(2_000);
        authority.latch_ring(true, Some(NodeKey::new(42)), now);
        assert_eq!(*authority.focus_ring_node_key, Some(NodeKey::new(42)));
        assert_eq!(*authority.focus_ring_started_at, Some(now));
    }

    #[test]
    fn focus_authority_latch_ring_clears_timestamp_on_none() {
        // When the new focus target is None (focus surrendered),
        // latch_ring clears the started_at timestamp so ring_alpha
        // returns 0.0 even if the node_key survives. Pin this.
        let mut focused_node_hint = None;
        let mut focus_ring_node_key = Some(NodeKey::new(7));
        let mut focus_ring_started_at = Some(PortableInstant(500));
        let mut authority = FocusAuthorityMut {
            focused_node_hint: &mut focused_node_hint,
            graph_surface_focused: false,
            focus_ring_node_key: &mut focus_ring_node_key,
            focus_ring_started_at: &mut focus_ring_started_at,
        };

        authority.latch_ring(true, None, PortableInstant(1_000));
        assert_eq!(*authority.focus_ring_node_key, None);
        assert_eq!(*authority.focus_ring_started_at, None);
    }

    // Note: `focus_authority_ring_alpha_*` tests removed alongside the
    // `ring_alpha`/`ring_alpha_with_curve` methods (2026-04-23).
    // Production alpha derivation lives on `FocusRingSpec::alpha_at_with_curve`
    // which has its own dedicated test coverage in `frame_model.rs`.

    #[test]
    fn focus_authority_clear_hint_if_matches_fires_only_on_match() {
        let mut focused_node_hint = Some(NodeKey::new(7));
        let mut focus_ring_node_key = None;
        let mut focus_ring_started_at = None;
        let mut authority = FocusAuthorityMut {
            focused_node_hint: &mut focused_node_hint,
            graph_surface_focused: false,
            focus_ring_node_key: &mut focus_ring_node_key,
            focus_ring_started_at: &mut focus_ring_started_at,
        };

        // Non-match: hint preserved, returns false.
        assert!(!authority.clear_hint_if_matches(NodeKey::new(99)));
        assert_eq!(*authority.focused_node_hint, Some(NodeKey::new(7)));

        // Match: hint cleared, returns true.
        assert!(authority.clear_hint_if_matches(NodeKey::new(7)));
        assert_eq!(*authority.focused_node_hint, None);
    }

    #[test]
    fn command_authority_reborrow_yields_distinct_handle_on_same_backing() {
        let mut toggle = false;
        let mut session = CommandPaletteSession::default();

        let mut outer = CommandAuthorityMut {
            toggle_requested: &mut toggle,
            session: &mut session,
        };

        {
            let inner = outer.reborrow();
            *inner.toggle_requested = true;
        }

        // Outer still observes the mutation because the reborrow
        // pointed at the same backing storage, not a separate copy.
        assert!(outer.toggle_requested());
    }
}
