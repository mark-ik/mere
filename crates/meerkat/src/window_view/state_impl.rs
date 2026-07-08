/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `WindowView` constructor + per-window host operations. The per-window *view-state*
//! accessors (chrome / orrery snapshot / folded panes) moved to
//! [`WindowCtx`](crate::WindowCtx) — they route through the shared `ShellMultiRunner`
//! (`Shell.multi`) by this window's `projection_id`, so a bare `WindowView` no longer
//! reaches its own view-state (it lives in `AppState.windows[projection_id]`). (One state,
//! N windows — Slice 3.)

use super::*;

impl WindowView {
    /// Mint a window's view over the shared session, bound to its `projection_id` in
    /// `Shell.multi`. Everything else starts at its rest value (empty caches, no
    /// in-progress gesture, the default 1024×600 surface); the caller overrides the
    /// view-session bits it restored (`centered`, `content_location`, `frame_layout`,
    /// `next_pane_id`). A second window is a second projection over the same shared
    /// state. (MW2; Slice 3.)
    pub(crate) fn new(
        kind: WindowKind,
        focused_graph: GraphId,
        dom: Rc<RefCell<ScriptedDom>>,
        projection_id: ProjectionId,
        workbench: Workbench,
    ) -> Self {
        Self {
            kind,
            focused_graph,
            viewports: HashMap::new(),
            selections: HashMap::new(),
            dom,
            projection_id,
            chrome_session: None,
            gnode_pool: GnodePool::default(),
            workbench,
            pelt_shell: None,
            pelt_theme: None,
            pelt_ui_scale: None,
            chisel_slots: chisel::LeafRegistry::new(),
            chisel_slot_cache: chisel::RenderedLeaves::new(),
            chisel_slot_textures: Default::default(),
            session_row_rects: Default::default(),
            session_close_rects: Default::default(),
            session_add_rect: Default::default(),
            tile_rects: Default::default(),
            content_rects: Default::default(),
            settings_rects: Default::default(),
            script_caps: None,
            wallet_unlock_mode: None,
            wallet_locked: None,
            wallet_unlock_status: None,
            find_matches: Default::default(),
            find_member: None,
            find_gen: 0,
            page_selection: None,
            tile_textures: Default::default(),
            content_card_unhealthy: Default::default(),
            tile_bands: Default::default(),
            note_content_heights: Default::default(),
            snapshot_data_uris: Default::default(),
            chrome_base_tex: Default::default(),
            chrome_orrery_tex: Default::default(),
            chrome_base_sig: 0,
            shellbar_style: Default::default(),
            comms_style: Default::default(),
            window_controls_tex: Default::default(),
            divider_tex: Default::default(),
            scroll: Default::default(),
            soft_wrap_goal: None,
            last_left_release: Default::default(),
            workbench_gesture: false,
            frame_divider_drag: Default::default(),
            resize_drag: Default::default(),
            titlebar_press: Default::default(),
            swatch_drag: Default::default(),
            row_reorder_drag: Default::default(),
            caret_drag: Default::default(),
            page_text_drag: Default::default(),
            tear_out_drag: Default::default(),
            clip_picker: None,
            branch_graphlet: None,
            cursor_icon: Default::default(),
            pending_exit: Default::default(),
            context_set: Default::default(),
            context_origin: Default::default(),
            context_link: Default::default(),
            object_card: Default::default(),
            context_field: Default::default(),
            renaming: Default::default(),
            tagging: Default::default(),
            centered: Default::default(),
            healed: Default::default(),
            content_location: Default::default(),
            shown_location: Default::default(),
            focused_tile: Default::default(),
            frame_layout: Default::default(),
            next_pane_id: Default::default(),
            mirror_tiles: false,
            maximized_pane: Default::default(),
            active_content: Default::default(),
            window: Default::default(),
            surface: Default::default(),
            toolbar_h: Default::default(),
            dpi_scale: 1.0,
            width: 1024,
            height: 600,
            cursor: Default::default(),
            modifiers: Default::default(),
            scrying: Default::default(),
            scrying_rects: Default::default(),
            scrying_input_focus: Default::default(),
        }
    }

    /// Request a redraw of this window, if it exists. A per-window operation: each
    /// window drives its own surface, so the registry's event handlers call this on
    /// the target view directly. (MW2 (c).)
    pub(crate) fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
