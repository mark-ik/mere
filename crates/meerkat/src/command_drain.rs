/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-side command and omnibar dispatch for a window: draining a captured
//! workbench action (tab switch / close / pin, new tile), a pending peer-connect,
//! a palette / omnibar `Command`, evaluating a `>`-prefixed omnibar command
//! expression through the privileged `CommandShell` (and the read-only
//! `ShellContext` snapshot it queries), and routing a comms intent to the comms
//! actor. The chrome records intents; the host executes them. Factored out of
//! `frame_ops.rs` to keep files under the 600-LOC ceiling.

use frame::PaneContent;
use kernel::graph::SemanticSubKind;
use meerkat::CommsIntent;
use meerkat::command::Command;
use meerkat::shell_eval::{CommandShell, ShellContext};
use platen_view::WorkbenchAction;

use super::observability::Severity;
use super::{WindowCtx, comms_host, sync};

/// Map a `relate("…")` kind word to a [`SemanticSubKind`]. A small curated set
/// of the relations a user reaches for by hand; anything unrecognized falls back
/// to `UserGrouped` (the honest default — a human asserted it).
fn relation_kind_from_str(s: &str) -> SemanticSubKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "cites" | "cite" => SemanticSubKind::Cites,
        "quotes" | "quote" => SemanticSubKind::Quotes,
        "summarizes" | "summary" => SemanticSubKind::Summarizes,
        "elaborates" | "elaborate" => SemanticSubKind::Elaborates,
        "example" | "example_of" | "exampleof" => SemanticSubKind::ExampleOf,
        "supports" | "support" => SemanticSubKind::Supports,
        "contradicts" | "contradict" => SemanticSubKind::Contradicts,
        "questions" | "question" => SemanticSubKind::Questions,
        "same" | "same_entity" | "sameentity" => SemanticSubKind::SameEntityAs,
        "duplicate" | "duplicate_of" | "duplicateof" => SemanticSubKind::DuplicateOf,
        "hyperlink" | "link" => SemanticSubKind::Hyperlink,
        _ => SemanticSubKind::UserGrouped,
    }
}

impl WindowCtx<'_> {
    /// Apply a pending workbench action the workbench root captured: switch the
    /// visible tab, close a tab (reaping its actor), or toggle its pin (the
    /// background-keep flag, which also exempts it from cap eviction).
    pub(super) fn drain_workbench_action(&mut self) {
        let Some(action) = self.view.workbench_runner.state().pending else {
            return;
        };
        self.view.workbench_runner.update(|s| s.pending = None);
        match action {
            WorkbenchAction::Activate(member) => {
                self.view.workbench.activate(member);
                self.view.focused_tile = Some(member);
            }
            WorkbenchAction::Close(member) => {
                self.view.workbench.close_tile(member);
                self.shared.content.constellation.reap(member);
                if self.view.workbench.open_members().is_empty() {
                    // Closing the last tile closes the workbench pane entirely (back
                    // to just the orrery), rather than leaving an empty pane.
                    // (Workbench-as-pane.)
                    self.close_workbench();
                } else if self.view.focused_tile == Some(member) {
                    self.view.focused_tile = self.view.workbench.open_members().first().copied();
                }
            }
            WorkbenchAction::TogglePin(member) => {
                let pinned = self.shared.content.constellation.is_background(member);
                self.shared.content.constellation.set_background(member, !pinned);
            }
            WorkbenchAction::NewTile => {
                // The "+" affordance: mint a fresh unlinked node and open it as a
                // tile, focused — a non-omnibar "new tile". Summon/tile the workbench
                // pane first (idempotent), matching every other tile-opening path, so
                // the new tile is actually shown regardless of the prior projection.
                self.open_workbench();
                let url = "mere://welcome";
                let member = self.orrery_mut().open_member_as_new_node(None, url);
                self.view.workbench.open_tile(member);
                self.view.focused_tile = Some(member);
                self.ensure_content(url);
                self.save_session();
            }
        }
        self.view.request_redraw();
    }

    /// Execute a pending "connect to peer" request the chrome queued (S5.1): take
    /// the ticket the verb captured from the address bar and drive the sync actor.
    /// The chrome records the intent; this is the host executing it.
    pub(super) fn drain_pending_connect(&mut self) {
        let Some(ticket) = self.view.runner.state().pending_connect.clone() else {
            return;
        };
        self.view.runner.update(|c| {
            c.pending_connect = None;
        });
        if ticket.is_empty() {
            tracing::warn!("connect to peer: paste the peer's ticket in the address bar first");
            return;
        }
        // Route the verb to the sync actor; it runs the dial on its runtime and logs
        // the outcome (the actor boundary, so no synchronous result here).
        self.shared.sync_handle.command(sync::SyncCommand::Connect(ticket));
        self.view.request_redraw();
    }

    /// Execute a pending host action the palette queued: take it from the chrome
    /// and dispatch to the matching shell method. Returns a one-shot user-facing
    /// note for commands that want to report why they no-opped (the omnibar echoes
    /// it; other callers ignore the return). `None` means "nothing to say".
    pub(super) fn drain_pending_command(&mut self) -> Option<String> {
        let Some(cmd) = self.view.runner.state().pending_command else {
            return None;
        };
        self.view.runner.update(|c| c.pending_command = None);
        let mut note = None;
        match cmd {
            Command::ToggleWorkbench => self.toggle_workbench(),
            Command::DeleteNode => self.delete_focused_node(),
            Command::BackgroundNode => self.toggle_focus_background(),
            Command::HideSelectedEdge => {
                if self.orrery_mut().hide_selected_edges() > 0 {
                    self.view.request_redraw();
                }
            }
            Command::ShowAllEdges => {
                if self.orrery_mut().show_all_edges() > 0 {
                    self.view.request_redraw();
                }
            }
            Command::ToggleRoster => self.toggle_pane(PaneContent::Roster),
            Command::ToggleGloss => self.toggle_pane(PaneContent::Gloss),
            Command::ToggleApparatus => self.toggle_pane(PaneContent::Apparatus),
            Command::ToggleInspector => self.toggle_pane(PaneContent::Inspector),
            Command::ToggleSteward => self.toggle_pane(PaneContent::Steward),
            Command::RetryFocusedContent => self.retry_focused_content(),
            Command::StopFocusedOperation => self.stop_focused_operation(),
            Command::PinFocusedOperation => self.pin_focused_operation(),
            Command::ToggleCompatView => self.toggle_focus_compat(),
            Command::AssertEdge => {
                if self.orrery_mut().assert_selected_relation(SemanticSubKind::UserGrouped) {
                    self.save_session();
                    self.view.request_redraw();
                } else {
                    note = Some("Select exactly two nodes to relate".to_string());
                }
            }
            Command::RetractEdge => {
                if self.orrery_mut().retract_selected_relation() > 0 {
                    self.save_session();
                    self.view.request_redraw();
                } else {
                    note = Some("Select two nodes (or an edge) to unrelate".to_string());
                }
            }
            // History / connect / settings / comms verbs run in the chrome; never
            // queued here as host intents.
            Command::Back
            | Command::Forward
            | Command::Home
            | Command::ConnectPeer
            | Command::OpenSettings
            | Command::ToggleComms => {}
        }
        note
    }

    /// Evaluate a `>`-prefixed omnibar command expression through the privileged
    /// [`CommandShell`] and apply what it emits. The expression reads a read-only
    /// [`ShellContext`] snapshot and returns the [`Command`]s it called; each is
    /// run through the same chrome path a palette pick takes (so back / forward /
    /// pane toggles behave identically), then the result text (or the error) is
    /// echoed in the omnibar. The fourth driver of the one `Command` spine,
    /// alongside the palette, the agent harness, and accesskit actions. (Omnibar
    /// command shell, S3.)
    pub(super) fn submit_omnibar_command(&mut self, expr: &str) {
        let context = self.shell_context();
        let outcome = CommandShell::new().eval(expr, &context);
        // `pending_command` is a single slot, so each emitted command is applied
        // and drained before the next — the same per-interaction routine
        // `chrome_activate` runs.
        let mut note: Option<String> = None;
        for &cmd in &outcome.commands {
            self.view.runner.update(move |c| c.run_command_and_close(cmd));
            self.drain_pending_connect();
            if let Some(n) = self.drain_pending_command() {
                note = Some(n);
            }
            self.drain_comms_intent();
            self.drain_history_step();
            self.drain_physics_toggle();
            self.sync_settings();
            self.sync_orrery();
        }
        // A kind-qualified `relate("cites")` applies the chosen relation to the
        // selected pair directly (the 0-arg `relate()` rode the AssertEdge path
        // above with the default kind).
        if let Some(kind) = &outcome.relation_kind {
            if self.orrery_mut().assert_selected_relation(relation_kind_from_str(kind)) {
                self.save_session();
                self.view.request_redraw();
            } else {
                note = Some("Select exactly two nodes to relate".to_string());
            }
        }
        let (severity, echo) = match &outcome.error {
            Some(err) => (Severity::Warn, format!("error: {err}")),
            None => (Severity::Info, outcome.text.clone()),
        };
        // An honest, inspectable record of every command-line eval (no placebo):
        // the expression, how many commands it ran, and the result / error.
        self.shared.observability.record_diagnostic(
            "meerkat.omnibar.command",
            severity,
            format!("{expr:?} -> {} command(s); {echo}", outcome.commands.len()),
        );
        // Reset the bar. Priority: a command's no-op note (e.g. "select two nodes
        // to relate") so a silent no-op explains itself; else a query result /
        // error; else the focused location. Either way the typed `>expr` is cleared
        // and the (now command-empty) suggestion dropdown closes.
        let shown = note
            .or(if echo.is_empty() { None } else { Some(echo) })
            .unwrap_or_else(|| self.current_focus_url().unwrap_or_default());
        self.view.runner.update(move |c| c.show_location(&shown));
        self.view.request_redraw();
    }

    /// A read-only snapshot of host state the command shell may query: the
    /// location, history + nav capability, the focused node, and every graph node
    /// URL (the cross-to-orrery reach). Built fresh per eval; nothing writes it.
    fn shell_context(&self) -> ShellContext {
        // The focused node's inspection rows (the same the Inspector pane shows),
        // surfaced to scripts as `inspect()`.
        let node = self
            .focused_member()
            .and_then(|member| self.orrery().graph().get_node_by_id(member))
            .map(|(_, node)| node);
        let state = node.and_then(|node| self.shared.content.pages.get(node.url()));
        let inspect = super::inspector::inspector_rows(node, state);

        let chrome = self.view.runner.state();
        ShellContext {
            current_url: self.current_focus_url().unwrap_or_default(),
            history: chrome.history.entries().to_vec(),
            can_back: chrome.toolbar.can_go_back,
            can_forward: chrome.toolbar.can_go_forward,
            focused_node: self.orrery().focused_url().map(str::to_string),
            nodes: self
                .orrery()
                .graph()
                .nodes()
                .map(|(_, node)| node.url().to_string())
                .collect(),
            inspect,
        }
    }

    /// Run the chrome's pending comms request (P6c): take the recorded
    /// [`CommsIntent`] and route it to the comms actor as a `CommsCommand`. The
    /// chrome can't reach the actor, so it records the intent and the host drains
    /// it here (mirrors [`drain_pending_command`](Self::drain_pending_command)).
    pub(super) fn drain_comms_intent(&mut self) {
        let Some(intent) = self.view.runner.state().comms_intent.clone() else {
            return;
        };
        self.view.runner.update(|c| c.comms_intent = None);
        self.shared.observability
            .record_actor("comms", "started", Some(format!("{intent:?}")));
        match intent {
            CommsIntent::Refresh => {
                self.shared.comms_handle.command(comms_host::CommsCommand::Refresh);
            }
            CommsIntent::Open(id) => {
                self.shared.comms_handle
                    .command(comms_host::CommsCommand::Open(id));
            }
            CommsIntent::Send(draft) => {
                self.shared.comms_handle
                    .command(comms_host::CommsCommand::Send(draft));
            }
            CommsIntent::ConnectCabal(ticket) => {
                self.shared.comms_handle
                    .command(comms_host::CommsCommand::ConnectCabal(ticket));
            }
        }
    }
}
