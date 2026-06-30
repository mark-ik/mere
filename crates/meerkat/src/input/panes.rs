/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Wheel routing, list-pane / roster activation draining, pelt activation keys.

use super::*;

impl WindowCtx<'_> {
    /// Route an orrery-area wheel through the document: dispatch it to the orrery pane element
    /// (the runner resolves the wheel target by walking ancestors from the hit node, so a wheel
    /// over a card or the scene resolves to the orrery element), whose `on_wheel` queues the
    /// delta; then drain it into gyre's pan / Ctrl-zoom. Returns whether gyre consumed it. The
    /// host complement of the orrery pane element's `on_wheel`. (cond 5 input bridge.)
    pub(crate) fn orrery_wheel_through_document(
        &mut self,
        gid: frame::GraphId,
        cx: f32,
        cy: f32,
        dx: f32,
        dy: f32,
    ) -> bool {
        let target = {
            let dom = self.view.dom.borrow();
            let offsets = ScrollOffsets::<NodeId>::default();
            self.view
                .chrome_session
                .as_ref()
                .and_then(|s| s.hit_test(&dom, cx, cy, &offsets))
                .and_then(|hit| self.view.runner.wheel_target(hit))
        };
        if let Some(node) = target {
            // The orrery element's handler reads only `delta`; `local` / `size` are unused here.
            let event = xilem_serval::WheelEvent::new((dx, dy), (0.0, 0.0), (0.0, 0.0));
            self.view.runner.dispatch_wheel(node, event);
        }
        match self.view.take_orrery_wheel() {
            Some((wdx, wdy)) => self.pane_orrery_mut(gid).wheel(wdx, wdy),
            None => false,
        }
    }

    /// Apply the activation keys the folded list panes' button handlers queued. The
    /// apparatus + trail panes are in the shell document, so their button clicks arrive
    /// through `chrome_click` -> `chrome_activate`; this drains + routes them (apparatus:
    /// theme / engine / physics; trail: recover a removed node). The display-only inspector
    /// + steward panes queue nothing. Replaces the old per-pane branches in
    /// `on_mouse_input`. (Phase 1, step 2.)
    pub(crate) fn drain_list_pane_activations(&mut self) {
        use crate::window_view::ShellListPane::{Alembic, Apparatus, Steward, Trail};
        for key in self.view.take_list_pane_activations(Apparatus) {
            self.apply_pelt_activation(&key);
        }
        for key in self.view.take_list_pane_activations(Trail) {
            if let Some(id) = key.strip_prefix("recover:") {
                self.recover_deleted_node(id);
            }
        }
        // Alembic: a clicked engram row thaws that engram into an Orrery pane beside. Pooling a
        // fresh graph re-keys the orrery pool, which a WindowCtx can't do (it holds one orrery
        // borrowed out), so queue it as a ShellCommand the Shell drains — like OpenGraphBeside. (B2.)
        for key in self.view.take_list_pane_activations(Alembic) {
            if let Some(id) = key.strip_prefix("engram:open:") {
                self.commands
                    .push(crate::ShellCommand::OpenEngramBeside(id.to_string()));
            } else if key == "alembic:forget" {
                // Runs in place (drops cached content only, no pool re-keying), so the
                // WindowCtx method is called directly. (Slice C/D forgetting.)
                self.run_forgetting_pass();
            } else if key == "alembic:eviction:cycle" {
                // Cycle the Recent header's eviction policy (persisted; the next forgetting pass
                // uses it). (Editable eviction policy, B4.)
                self.cycle_eviction_policy();
            } else if let Some(url) = key.strip_prefix("alembic:keep:") {
                // One-click promote: add the reserved `saved` tag, moving the row to Saved. (B3.)
                self.keep_node(url);
            } else if let Some(url) = key.strip_prefix("alembic:release:") {
                // One-click demote: remove the `saved` tag. (B3.)
                self.release_node(url);
            } else if let Some(id) = key.strip_prefix("engram:compose:") {
                // The two-select compose gesture: first click marks pending, a second
                // (different) click composes, the same id again deselects. Runs in place (no
                // pool re-keying — it writes a new engram to the store). (B7-P3.)
                self.toggle_compose_selection(id);
            }
        }
        // Steward: the focused-operation action buttons (retry / stop / background-pin)
        // queue `steward:*` keys; route each to its node-ops verb so the row is a real
        // action, not a typed-verb hint. (Audit A2 — Steward rows clickable.)
        for key in self.view.take_list_pane_activations(Steward) {
            // Per-row keys carry the member id (`steward:<verb>:<uuid>`) so the verb
            // acts on that specific live operation; the bare keys act on the focused
            // op. (Chrome bar P2 — Steward process list.)
            if let Some(id) = key.strip_prefix("steward:stop:") {
                if let Ok(member) = forme::GraphMemberId::parse_str(id) {
                    self.stop_operation(member);
                }
            } else if let Some(id) = key.strip_prefix("steward:pin:") {
                if let Ok(member) = forme::GraphMemberId::parse_str(id) {
                    self.pin_operation(member);
                }
            } else if let Some(id) = key.strip_prefix("steward:retry:") {
                if let Ok(member) = forme::GraphMemberId::parse_str(id) {
                    if let Some(url) = self
                        .shared
                        .content
                        .constellation
                        .active_operations()
                        .into_iter()
                        .find(|op| op.member == member)
                        .and_then(|op| op.url)
                    {
                        self.retry_content_url(&url);
                    }
                }
            } else {
                match key.as_str() {
                    "steward:retry" => self.retry_focused_content(),
                    "steward:stop" => self.stop_focused_operation(),
                    "steward:pin" => self.pin_focused_operation(),
                    _ => {}
                }
            }
        }
        // The settings tiles' `pelt/*` pages carry the same control keys as the apparatus, so
        // a page control drives the host identically; a spine entry navigates the tile's node
        // to the chosen page (`navigate_member` retargets its url, which the next frame's
        // settings dispatch re-resolves). (Settings lane P1.)
        for (_member, key) in self.view.take_settings_pane_keys() {
            // `node:<id>` facets controls carry the subject node in the key; the rest are the
            // pelt pages' shared keys (theme / engine toggle / physics / orrery). (Registry P3.)
            if let Some(facet) = key.strip_prefix("nodefacet:") {
                self.apply_node_facet_key(facet);
            } else {
                self.apply_pelt_activation(&key);
            }
        }
        for (member, url) in self.view.take_settings_pane_nav() {
            self.orrery_mut().navigate_member(member, &url);
            self.view.request_redraw();
        }
    }

    /// Apply a `pelt` settings activation key (a theme id, `engine:toggle:<id>`, or a
    /// `phys:damping:*` step). Shared by the apparatus pane and the settings lane's `pelt/*`
    /// pages, so a control drives the host the same wherever it is shown. (Settings lane P1.)
    pub(crate) fn apply_pelt_activation(&mut self, key: &str) {
        match key {
            "phys:damping:down" => self.adjust_physics_damping(-0.5),
            "phys:damping:up" => self.adjust_physics_damping(0.5),
            // The active-tab cap (migrated from the settings overlay): edit the chrome cap;
            // the per-frame `sync_settings` applies it to the actor pool + persists. (P2.)
            "tiles:cap:down" => self.view.chrome_update(Chrome::dec_tab_cap),
            "tiles:cap:up" => self.view.chrome_update(Chrome::inc_tab_cap),
            // The `pelt/orrery` scene toggles, driving the same methods the context menu does
            // (so the page and the menu stay one source of truth). (Settings lane P2b.)
            "orrery:sizebydegree" => self.toggle_orrery_size_by_degree(),
            "orrery:sizebyimportance" => self.toggle_orrery_size_by_importance(),
            "orrery:importance:degree" => {
                self.orrery_mut()
                    .set_importance_metric(orrery::ImportanceMetric::Degree);
                self.view.request_redraw();
            }
            "orrery:importance:betweenness" => {
                self.orrery_mut()
                    .set_importance_metric(orrery::ImportanceMetric::Betweenness);
                self.view.request_redraw();
            }
            "orrery:communityrings" => {
                let on = !self.orrery().show_community_rings();
                self.orrery_mut().set_show_community_rings(on);
                self.view.request_redraw();
            }
            "orrery:bridgerings" => {
                let on = !self.orrery().show_bridge_rings();
                self.orrery_mut().set_show_bridge_rings(on);
                self.view.request_redraw();
            }
            "orrery:bridge:betweenness" => {
                self.orrery_mut()
                    .set_bridge_metric(orrery::BridgeMetric::Betweenness);
                self.view.request_redraw();
            }
            "orrery:bridge:articulation" => {
                self.orrery_mut()
                    .set_bridge_metric(orrery::BridgeMetric::Articulation);
                self.view.request_redraw();
            }
            "orrery:glossscope" => {
                // Crop the gloss lens to the current selection (P6c, gloss scope).
                let on = !self.orrery().gloss_scope_selection();
                self.orrery_mut().set_gloss_scope_selection(on);
                self.view.request_redraw();
            }
            "orrery:glosssize" => {
                // Size the gloss lens's nodes by the importance signal (P6c, gloss encoding).
                let on = !self.orrery().gloss_size_by_importance();
                self.orrery_mut().set_gloss_size_by_importance(on);
                self.view.request_redraw();
            }
            k if k.starts_with("orrery:gloss:") => {
                // The gloss lens-picker: an empty id mirrors the main view (a minimap); a strategy id
                // makes the gloss an independent lens. (Graph signals — P6 / P6b.)
                let id = &k["orrery:gloss:".len()..];
                let next = (!id.is_empty()).then(|| id.to_string());
                self.orrery_mut().set_gloss_strategy(next);
                self.view.request_redraw();
            }
            "orrery:affinity" => {
                // Toggle the affinity-clustering force (cluster structurally-similar nodes).
                let on = !self.orrery().cluster_by_affinity();
                self.orrery_mut().set_cluster_by_affinity(on);
                self.view.request_redraw();
            }
            "orrery:mirror" => self.toggle_mirror_tiles(),
            k if k.starts_with("orrery:layout:") => {
                self.set_orrery_layout(&k["orrery:layout:".len()..]);
            }
            k if k.starts_with("engine:toggle:") => {
                self.toggle_engine(&k["engine:toggle:".len()..]);
            }
            // Crawl scope / depth picker (the settings lane): set the policy a `>crawl`
            // starts under, persist it, and redraw so the picker re-checks. (Crawl controls.)
            k if k.starts_with("crawl:scope:") => {
                if let Some(scope) = crate::crawl::HostScope::from_key(&k["crawl:scope:".len()..]) {
                    self.shared.content.crawl.set_scope(scope);
                    self.persist_settings();
                    self.view.request_redraw();
                }
            }
            k if k.starts_with("crawl:depth:") => {
                if let Ok(depth) = k["crawl:depth:".len()..].parse::<u32>() {
                    self.shared.content.crawl.set_max_depth(depth);
                    self.persist_settings();
                    self.view.request_redraw();
                }
            }
            // "Crawl whole site" mode: flip the sitemap-seed flag, persist, redraw.
            "crawl:sitemap" => {
                let on = !self.shared.content.crawl.seed_sitemap();
                self.shared.content.crawl.set_seed_sitemap(on);
                self.persist_settings();
                self.view.request_redraw();
            }
            k if k.starts_with("crawl:pages:") => {
                if let Ok(pages) = k["crawl:pages:".len()..].parse::<usize>() {
                    self.shared.content.crawl.set_max_pages(pages);
                    self.persist_settings();
                    self.view.request_redraw();
                }
            }
            // DocumentScript capability cyclers (the `pelt/scripts` page): cycle
            // log/document/net through default → Allow → Prompt → Deny. (Tail 3.)
            k if k.starts_with("script:cap:") => {
                self.set_script_cap(&k["script:cap:".len()..]);
            }
            // Theme editor (T5): fork / remove / mode-toggle / per-seed HSL nudge.
            // These must precede the theme-id fallback so they aren't read as ids.
            "theme:fork" => self.fork_active_theme(),
            "theme:remove" => self.remove_active_user_theme(),
            "theme:mode" => self.toggle_active_theme_mode(),
            k if k.starts_with("theme:harmony:") => {
                self.set_active_harmony(&k["theme:harmony:".len()..]);
            }
            k if k.starts_with("theme:seed:") => {
                self.adjust_seed_from_key(&k["theme:seed:".len()..]);
            }
            // Document typography (the `pelt/reading` page): text size / line
            // spacing sliders, link-arrows toggle, font choice, reset.
            k if k.starts_with("doc:") => {
                self.apply_doc_style_key(&k["doc:".len()..]);
            }
            // The persona-configurable context menu (`pelt/menu`): add / remove a command, move it
            // up / down in the order, or reset to the registry default. Persists to the persona
            // store. (Command registry P4.)
            "menu:reset" => self.reset_menu_actions(),
            k if k.starts_with("menu:move:") => {
                let rest = &k["menu:move:".len()..];
                if let Some((id, dir)) = rest.rsplit_once(':') {
                    self.move_menu_action(id, dir == "up");
                }
            }
            k if k.starts_with("menu:toggle:") => {
                self.toggle_menu_action(&k["menu:toggle:".len()..]);
            }
            // The `pelt/scene` page: load / clear a physics backdrop scene, a whirlpool / fountain /
            // liquid / ambient effect, or flip the graph's tangibility, by flat name. (Scene settings.)
            k if k.starts_with("scene:") => {
                self.load_named_scene(&k["scene:".len()..]);
            }
            _ => self.set_theme(key),
        }
    }

    /// Route a `doc:*` typography key from the `pelt/reading` page: `size` /
    /// `linespacing` carry a `<i>:<count>` slider cell (fraction `i/count`);
    /// `bodyfont` / `monofont` carry a family name; `arrows` / `reset` are bare.
    pub(crate) fn apply_doc_style_key(&mut self, rest: &str) {
        let (head, tail) = rest.split_once(':').unwrap_or((rest, ""));
        match head {
            "size" => {
                if let Some(f) = slider_cell_fraction(tail) {
                    self.set_doc_text_size(f);
                }
            }
            "linespacing" => {
                if let Some(f) = slider_cell_fraction(tail) {
                    self.set_doc_line_spacing(f);
                }
            }
            "arrows" => self.toggle_doc_link_arrows(),
            "bodyfont" => self.set_doc_body_font(tail),
            "monofont" => self.set_doc_mono_font(tail),
            "reset" => self.reset_doc_style(),
            _ => {}
        }
    }

    /// Parse a `theme:seed:<seed>:<h|s|l>:<i>:<count>` slider-cell key and set
    /// that channel to the fraction `i/count` on the active user theme.
    pub(crate) fn adjust_seed_from_key(&mut self, rest: &str) {
        let parts: Vec<&str> = rest.split(':').collect();
        let [seed, channel, i, count] = parts[..] else {
            return;
        };
        let (Ok(i), Ok(count)) = (i.parse::<f64>(), count.parse::<f64>()) else {
            return;
        };
        if count <= 0.0 {
            return;
        }
        let ch = channel.chars().next().unwrap_or(' ');
        self.set_active_seed_channel(seed, ch, i / count);
    }

    /// Apply the roster-row intents the shell runner's dispatch queued. The roster is
    /// folded into the shell document, so its row clicks arrive through `chrome_click` ->
    /// `chrome_activate`; this drains + applies them (Shift = additive selection).
    /// Replaces the old roster-pane branch in `on_mouse_input`. (Phase 1.)
    pub(crate) fn drain_roster_intents(&mut self) {
        let intents = self.view.take_roster_intents();
        if intents.is_empty() {
            return;
        }
        let additive = self.view.modifiers.shift;
        for intent in intents {
            match intent {
                crate::roster_view::RosterIntent::SetTab(tab) => {
                    self.view.set_roster_tab(tab);
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::OpenDetail(subject) => {
                    self.view.set_roster_tab(subject.natural_tab());
                    self.view.set_roster_subject(Some(subject));
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::Select(member) => {
                    if additive {
                        self.orrery_mut().toggle_select_member(member);
                        self.view.request_redraw();
                    } else if let Some(url) = self
                        .orrery()
                        .graph()
                        .get_node_by_id(member)
                        .map(|(_, n)| n.url().to_string())
                    {
                        self.orrery_mut().select_by_url(&url);
                        self.view.request_redraw();
                    }
                }
                crate::roster_view::RosterIntent::RelateAs { from, to, kind } => {
                    if self
                        .orrery_mut()
                        .assert_relation_between_members(from, to, kind)
                    {
                        self.save_session();
                    }
                    self.view.set_roster_tab(crate::roster::RosterTab::Links);
                    self.view.set_roster_subject(Some(
                        crate::roster::RosterSubject::RelationCell {
                            from,
                            to,
                            selector: kernel::graph::RelationSelector::Semantic(kind),
                        },
                    ));
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::RetractRelation { from, to, selector } => {
                    if self
                        .orrery_mut()
                        .retract_relation_between_members(from, to, selector)
                        > 0
                    {
                        self.save_session();
                        self.retarget_roster_after_relation_retract(from, to, selector);
                    }
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::HideLinkBundle { from, to } => {
                    if self.orrery_mut().hide_edge_between_members(from, to) {
                        self.save_session();
                        self.view.request_redraw();
                    }
                }
                crate::roster_view::RosterIntent::ShowLinkBundle { from, to } => {
                    if self.orrery_mut().show_edge_between_members(from, to) {
                        self.save_session();
                        self.view.request_redraw();
                    }
                }
                crate::roster_view::RosterIntent::ReconcileGraphlet(graphlet) => {
                    self.commands.push(crate::ShellCommand::ReconcileGraphlet {
                        graph: self.view.focused_graph,
                        graphlet,
                    });
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::KeepGraphletAsSession(graphlet) => {
                    self.commands
                        .push(crate::ShellCommand::KeepGraphletAsSession {
                            graph: self.view.focused_graph,
                            graphlet,
                        });
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::BranchGraphlet(graphlet) => {
                    self.commands.push(crate::ShellCommand::BranchGraphlet {
                        graph: self.view.focused_graph,
                        graphlet,
                    });
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::OpenGraphlet(graphlet) => {
                    self.commands
                        .push(crate::ShellCommand::OpenExistingGraphlet {
                            graph: self.view.focused_graph,
                            graphlet,
                        });
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::SelectField(id) => {
                    if self.orrery_mut().center_on_field(id) {
                        self.view.request_redraw();
                    }
                }
                crate::roster_view::RosterIntent::ToggleFieldVisibility(id) => {
                    self.orrery_mut().toggle_field_visible(id);
                    self.view.request_redraw();
                }
                crate::roster_view::RosterIntent::AdjustFieldStrength(id, delta) => {
                    if let Some(current) = self.orrery().field_strength(id) {
                        let next = (current + delta).clamp(1000.0, 20000.0);
                        if (next - current).abs() > f32::EPSILON
                            && self.orrery_mut().set_field_strength(id, next)
                        {
                            self.save_session();
                            self.view.request_redraw();
                        }
                    }
                }
            }
        }
    }

    fn retarget_roster_after_relation_retract(
        &mut self,
        from: forme::GraphMemberId,
        to: forme::GraphMemberId,
        selector: kernel::graph::RelationSelector,
    ) {
        let Some(subject) = self.view.roster_subject() else {
            return;
        };
        let selected_retracted = matches!(
            subject,
            crate::roster::RosterSubject::RelationCell {
                from: selected_from,
                to: selected_to,
                selector: selected_selector,
            } if selected_from == from && selected_to == to && selected_selector == selector
        );
        let selected_bundle = matches!(
            subject,
            crate::roster::RosterSubject::LinkBundle {
                from: selected_from,
                to: selected_to,
            } if selected_from == from && selected_to == to
        );
        if !selected_retracted && !selected_bundle {
            return;
        }
        if self.relation_bundle_exists(from, to) {
            self.view
                .set_roster_subject(Some(crate::roster::RosterSubject::LinkBundle { from, to }));
        } else {
            self.view.set_roster_subject(None);
        }
        self.view.set_roster_tab(crate::roster::RosterTab::Links);
    }

    fn relation_bundle_exists(&self, from: forme::GraphMemberId, to: forme::GraphMemberId) -> bool {
        let graph = self.orrery().graph();
        let Some(from_key) = graph.get_node_key_by_id(from) else {
            return false;
        };
        let Some(to_key) = graph.get_node_key_by_id(to) else {
            return false;
        };
        graph
            .relations()
            .any(|relation| relation.from == from_key && relation.to == to_key)
    }
}
