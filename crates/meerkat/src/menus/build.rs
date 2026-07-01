/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Context-menu item building (curated/search/suggested, resolve).

use super::*;

impl WindowCtx<'_> {
    /// Open the right-click context menu over the current selection's working set,
    /// at window `(x, y)`. A no-op when nothing is selected (no set to act on). A
    /// single-member set offers one "open tile"; a larger set offers splits vs a
    /// stack. The host remembers the set; the chrome renders the rows.
    pub(crate) fn open_context_menu_at(&mut self, x: f32, y: f32) {
        let set = self.selection_working_set();
        // Empty-canvas menus mint at the cursor: record the anchor (so "Add node" lands under
        // it) and any field box under the point (so "Delete field" can target it). A selection
        // menu never mints at the cursor, so it just clears the stale anchor.
        if set.is_empty() {
            let (ofx, ofy) = self.orrery_point(x, y);
            self.view.context_origin = Some((ofx, ofy));
            self.view.context_field = self.orrery().field_at_screen(ofx, ofy);
        } else {
            self.view.context_origin = None;
        }
        // The working member set drives applicability; set it before building so the curated rows
        // resolve against it. The menu opens with an empty query (curated rows; typing searches).
        self.view.context_set = set;
        let items = self.build_curated_menu_items();
        self.view
            .chrome_update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
    }

    /// The curated context-menu rows (the cursor palette's zero-query view): the persona's pinned
    /// commands resolved against the working set, plus the dynamic rows (delete field + layout
    /// submenu on the empty canvas, the single-focus radial toggle, the close-pane foot). Reads the
    /// working set from `context_set`, so callers set it first. (Command registry P4 / S1.)
    pub(crate) fn build_curated_menu_items(&self) -> Vec<ContextItem> {
        let len = self.view.context_set.len();
        // 1. The persona-curated command list: each configured registry id, resolved + applicability-
        //    filtered for this selection. One ordered list drives the canvas / single / multi menus.
        let mut items = Vec::new();
        for id in self.menu_actions() {
            if let Some(item) = self.resolve_menu_item(&id, len) {
                items.push(item);
            }
        }
        // 1b. No applicable pins for this context -> offer the most-used applicable commands instead
        //     (the frequency auto-suggest, S3). Each is a pinnable row, so a suggestion can be
        //     promoted to a pin. "Nothing pinned -> the mini bar suggests for you."
        if items.is_empty() {
            items.extend(self.suggested_menu_items(len));
        }
        // 2. Dynamic / parameterized rows that aren't flat catalog gestures.
        if len == 0 {
            if self.view.context_field.is_some() {
                items.push(ContextItem::new("Delete field", ContextAction::DeleteField));
            }
            // A bare click on a relation cell (no node) picks the edge but leaves the
            // node working-set empty, so this row can't ride the registry's len>=1
            // `MenuScope::Selection` gate — it's a dynamic row like "Delete field"
            // above. (Roster detail cards R6 — canvas/context visibility action.)
            if self.orrery().has_selected_edges() {
                items.push(ContextItem::new(
                    meerkat::command::Command::HideSelectedEdge.label(),
                    ContextAction::RunCommand(meerkat::command::Command::HideSelectedEdge.verb()),
                ));
            }
            // The 10 layout strategies fold under one "Layout" submenu instead of a flat tail on
            // the empty-canvas menu (the active one is ✓-marked inside). (Submenus.)
            items.push(ContextItem::with_children(
                "Layout",
                self.layout_picker_items(),
            ));
        } else if self.orrery().focused_key().is_some() {
            // Radial centers the orrery on the focused node (BFS rings) and re-centers live; a
            // focus-driven toggle (✓ when active), so it rides the selection menu. (Layout — radial.)
            let radial_on = self.orrery().layout_strategy() == Some("radial.default");
            items.push(ContextItem::new(
                if radial_on {
                    "Radial layout  \u{2713}"
                } else {
                    "Radial layout"
                },
                if radial_on {
                    ContextAction::SetLayoutStrategy("")
                } else {
                    ContextAction::SetLayoutStrategy("radial.default")
                },
            ));
        }
        // A two-node selection: offer the relation-kind picker as a "Relate as…" submenu so a
        // drawn edge can carry a semantic kind (cites / supports / …), not just the default
        // UserGrouped — one parent row expanding the 11 kinds instead of 11 flat rows. (A3 / submenus.)
        if len == 2 {
            items.push(ContextItem::with_children(
                "Relate as\u{2026}",
                self.relate_picker_items(),
            ));
        }
        if self.has_multiple_graph_panes() {
            items.push(ContextItem::new(
                "Close graph view",
                ContextAction::CloseGraphPane,
            ));
        }
        items
    }

    /// The search results for a non-empty menu query (the cursor palette): every registry command +
    /// context action matching `query`, mapped to runnable rows. Searching is intentful, so results
    /// are not applicability-filtered; a searched command runs in the current context (a graceful
    /// no-op if it does not apply). (Searchable context menu S1.)
    pub(crate) fn search_menu_items(&self, query: &str) -> Vec<ContextItem> {
        use meerkat::command::{PaletteItem, context_action_id, context_action_palette_label};
        let pinned = |id: &str| {
            self.shared
                .presentation
                .menu_actions
                .iter()
                .any(|a| a == id)
        };
        meerkat::command::palette_items(query)
            .into_iter()
            .filter_map(|item| match item {
                PaletteItem::Command(cmd) => Some(ContextItem::searchable(
                    cmd.label(),
                    ContextAction::RunCommand(cmd.verb()),
                    cmd.verb(),
                    pinned(cmd.verb()),
                )),
                // Catalog context actions always have an id (they came from the catalog).
                PaletteItem::Context(action) => context_action_id(action).map(|id| {
                    ContextItem::searchable(
                        context_action_palette_label(action).unwrap_or_default(),
                        action,
                        id,
                        pinned(id),
                    )
                }),
            })
            .collect()
    }

    /// The auto-suggested rows for an empty-pin context (command registry S3): the most-used
    /// commands that apply to this selection, not already pinned, top 6. Pinnable rows (a suggestion
    /// can be promoted to a pin). Empty until commands have been run a few times.
    pub(crate) fn suggested_menu_items(&self, len: usize) -> Vec<ContextItem> {
        let usage = &self.shared.presentation.command_usage;
        let pinned = |id: &str| {
            self.shared
                .presentation
                .menu_actions
                .iter()
                .any(|a| a == id)
        };
        let mut ranked: Vec<(&str, u32)> = usage.iter().map(|(id, n)| (id.as_str(), *n)).collect();
        // Most-used first; ties broken by id so the order is stable frame to frame.
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        ranked
            .into_iter()
            .filter(|(id, _)| {
                !pinned(id) && meerkat::command::registry_scope(id).is_some_and(|s| s.applies(len))
            })
            .take(6)
            .filter_map(|(id, _)| self.searchable_row_from_id(id))
            .collect()
    }

    /// Resolve a registry id (a `Command` verb or a context-action id) to a pinnable search-result
    /// row: the label runs it, the pin toggle pins / unpins it. `None` for an unknown id. Shared by
    /// the suggestions (S3); search results build the same shape from `palette_items`. (Cursor palette.)
    pub(crate) fn searchable_row_from_id(&self, id: &str) -> Option<ContextItem> {
        use meerkat::command::{
            Command, context_action_from_id, context_action_id, context_action_palette_label,
        };
        let pinned = self
            .shared
            .presentation
            .menu_actions
            .iter()
            .any(|a| a == id);
        if let Some(cmd) = Command::from_id(id) {
            Some(ContextItem::searchable(
                cmd.label(),
                ContextAction::RunCommand(cmd.verb()),
                cmd.verb(),
                pinned,
            ))
        } else {
            let action = context_action_from_id(id)?;
            let cid = context_action_id(action)?;
            Some(ContextItem::searchable(
                context_action_palette_label(action).unwrap_or_default(),
                action,
                cid,
                pinned,
            ))
        }
    }

    /// Rebuild the open menu's rows from its current query: the curated rows when empty, the search
    /// results otherwise. Resets the highlight (the list changed). Called on each query edit.
    /// (Searchable context menu S1.)
    pub(crate) fn rebuild_context_menu(&mut self) {
        let Some(query) = self
            .view
            .chrome()
            .context_menu
            .as_ref()
            .map(|m| m.query.clone())
        else {
            return;
        };
        let items = if query.trim().is_empty() {
            self.build_curated_menu_items()
        } else {
            self.search_menu_items(&query)
        };
        self.view.chrome_update(move |c| {
            if let Some(menu) = &mut c.context_menu {
                menu.items = items;
                menu.selected = None;
                // The rows changed, so any open submenu's parent index is stale — collapse it.
                // (Typing into the menu exits a submenu; nested submenus.)
                menu.submenu = None;
            }
        });
        self.view.request_redraw();
    }

    /// The persona-curated context-menu command list (command registry P4): the user's configured
    /// order, or the registry default ([`DEFAULT_MENU_ACTIONS`](meerkat::command::DEFAULT_MENU_ACTIONS))
    /// when unset. Owned (cloned) so the build loop can borrow `self` to resolve each id.
    pub(crate) fn menu_actions(&self) -> Vec<String> {
        self.shared.presentation.menu_actions.clone()
    }

    /// Resolve a configured registry `id` to a context-menu row for a selection of `len` members,
    /// or `None` if it doesn't apply here (wrong selection shape, an unmet dynamic condition, or an
    /// unknown id). Handles both the menu's native context actions and global commands (carried via
    /// [`ContextAction::RunCommand`]). (Command registry P4.)
    pub(crate) fn resolve_menu_item(&self, id: &str, len: usize) -> Option<ContextItem> {
        if !meerkat::command::registry_scope(id)?.applies(len) {
            return None;
        }
        if let Some(action) = meerkat::command::context_action_from_id(id) {
            return self.resolve_context_action_item(action, len);
        }
        // A global command added to the menu: run it by verb.
        let cmd = meerkat::command::Command::from_id(id)?;
        Some(ContextItem::new(
            cmd.label(),
            ContextAction::RunCommand(cmd.verb()),
        ))
    }

    /// Build the row for a native context action at this selection: its count-adapted, ✓-marked
    /// label, or `None` when a dynamic condition isn't met (Relate needs exactly two, Show all a
    /// scope lens, Mirror tiles open). (Command registry P4.)
    pub(crate) fn resolve_context_action_item(
        &self,
        action: ContextAction,
        len: usize,
    ) -> Option<ContextItem> {
        use ContextAction::*;
        let item = match action {
            OpenSplits => ContextItem::new(
                if len == 1 {
                    "Open tile"
                } else {
                    "Open in splits"
                },
                OpenSplits,
            ),
            Stack => ContextItem::new("Open in a stack", Stack),
            AddTag => ContextItem::new("Add tag\u{2026}", AddTag),
            ResizeNode => ContextItem::new("Resize", ResizeNode),
            OpenNodeFacets => ContextItem::new("Node settings\u{2026}", OpenNodeFacets),
            IsolateSelection => ContextItem::new("Isolate", IsolateSelection),
            CrystallizeSelection => ContextItem::new("Crystallize selection", CrystallizeSelection),
            OpenComponentGraphlet => ContextItem::new("Open component", OpenComponentGraphlet),
            OpenNeighborhoodGraphlet => {
                ContextItem::new("Open neighborhood", OpenNeighborhoodGraphlet)
            }
            OpenLinkWebGraphlet => ContextItem::new("Open link web", OpenLinkWebGraphlet),
            ToggleSizeByDegree => {
                let on = self.orrery().size_by_degree();
                ContextItem::new(
                    if on {
                        "Size by degree  \u{2713}"
                    } else {
                        "Size by degree"
                    },
                    ToggleSizeByDegree,
                )
            }
            ToggleSizeByImportance => {
                let on = self.orrery().size_by_importance();
                ContextItem::new(
                    if on {
                        "Size by importance  \u{2713}"
                    } else {
                        "Size by importance"
                    },
                    ToggleSizeByImportance,
                )
            }
            AddNode => ContextItem::new("Add node", AddNode),
            AddField => ContextItem::new("Add field", AddField),
            AddTile => ContextItem::new("Add tile", AddTile),
            AddSession => ContextItem::new("Add graph session", AddSession),
            ShowAllNodes => {
                if !self.orrery().is_scoped() {
                    return None; // only when a scope lens is active
                }
                ContextItem::new("Show all", ShowAllNodes)
            }
            MirrorTiles => {
                if !self.view.mirror_tiles && self.view.workbench.open_members().is_empty() {
                    return None; // only with tiles open (or already mirroring, to turn off)
                }
                ContextItem::new(
                    if self.view.mirror_tiles {
                        "Mirror tiles  \u{2713}"
                    } else {
                        "Mirror tiles"
                    },
                    MirrorTiles,
                )
            }
            _ => return None,
        };
        Some(item)
    }
}
