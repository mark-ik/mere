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
use session_runtime::settings_store;

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

/// Compact one-line rendering of SPARQL rows for the omnibar echo: the row count,
/// then up to five rows as `var=val, …` joined by ` | `, with a trailing `…` when
/// truncated. (A first cut; a results pane is the follow-on.)
fn format_sparql_rows(rows: &linked_data::query::QueryRows) -> String {
    const MAX_ROWS: usize = 5;
    let rendered: Vec<String> = rows
        .rows
        .iter()
        .take(MAX_ROWS)
        .map(|row| {
            rows.variables
                .iter()
                .zip(row)
                .map(|(var, cell)| format!("{var}={}", cell.as_deref().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect();
    let more = if rows.rows.len() > MAX_ROWS { " …" } else { "" };
    format!(
        "{} result(s): {}{}",
        rows.rows.len(),
        rendered.join(" | "),
        more
    )
}

impl WindowCtx<'_> {
    /// Execute a pending "connect to peer" request the chrome queued (S5.1): take
    /// the ticket the verb captured from the address bar and drive the sync actor.
    /// The chrome records the intent; this is the host executing it.
    pub(super) fn drain_pending_connect(&mut self) {
        let Some(ticket) = self.view.chrome().pending_connect.clone() else {
            return;
        };
        self.view.chrome_update(|c| {
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
        let Some(cmd) = self.view.chrome().pending_command else {
            return None;
        };
        self.view.chrome_update(|c| c.pending_command = None);
        // Audit every host command through the one observability spine, by its registry id
        // (the same id the palette / agent / script address) — the "everything observable"
        // half of the one seam. (Command registry P3.)
        self.shared.observability.record_diagnostic(
            "meerkat.command.invoked",
            Severity::Info,
            format!("{} ({})", cmd.verb(), cmd.label()),
        );
        // Tally the invocation for the context menu's frequency auto-suggest. (Command registry S3.)
        self.record_command_usage(cmd.verb());
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
            Command::ToggleProjection => {
                let iso = !self.orrery_mut().is_isometric();
                self.orrery_mut().set_isometric(iso);
                self.view.request_redraw();
            }
            Command::ToggleRoster => self.toggle_pane(PaneContent::Roster),
            Command::ToggleGloss => self.toggle_pane(PaneContent::Gloss),
            Command::ToggleApparatus => self.toggle_pane(PaneContent::Apparatus),
            Command::ToggleInspector => self.toggle_pane(PaneContent::Inspector),
            Command::ToggleTrail => self.toggle_pane(PaneContent::Trail),
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
            Command::CloseGraphPane => {
                if self.has_multiple_graph_panes() {
                    self.close_focused_graph_pane();
                } else {
                    note = Some("Only one graph view is open".to_string());
                }
            }
            Command::ExportGraph => {
                note = Some(self.export_graph_jsonld());
            }
            // Settings opens the pelt settings lane as a workbench tile (the consolidated
            // config surface), defaulting to the appearance page. (Settings lane P1.)
            Command::OpenSettings => self.open_settings_tile("pelt/appearance"),
            // Node settings opens the focused node's facets tile (the `node:<id>` provider),
            // at the info page. (Settings lane P3.)
            Command::OpenNodeSettings => match self.focused_member() {
                Some(member) => self.open_settings_tile(&format!("node:{member}/info")),
                None => note = Some("Select a node to open its settings".to_string()),
            },
            // History / connect / comms verbs run in the chrome; never queued here as host
            // intents.
            Command::Back
            | Command::Forward
            | Command::Home
            | Command::ConnectPeer
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
            self.view.chrome_update(move |c| c.run_command_and_close(cmd));
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
        // A `scene("pyramid")` (or the `>scene pyramid` sugar) loads that backdrop scene / effect /
        // ambient sim on the focused orrery, through the same `load_named_scene` vocabulary the Scene
        // settings page drains — so the page and the verb never drift. (Scene verb.)
        if let Some(name) = &outcome.scene_request {
            note = Some(if self.load_named_scene(name) {
                format!("scene: {name}")
            } else {
                format!("unknown scene: {name}")
            });
        }
        // A `sparql("…")` call runs over the focused graph and echoes the result.
        // Read-only: an ephemeral in-memory store, the kernel stays the authority.
        if let Some(query) = &outcome.sparql_query {
            note = Some(self.run_sparql_query(query));
        }
        // DocumentScript triggers (P2.5): attach / deliver-event / detach on the
        // focused tile. `attach` resolves the script's capability permissions (App
        // default for now; the Session-scope override store is the follow-on); the
        // actor maps them to the link grant and a denied capability fails the attach.
        // The re-render rides the actor's `Scene`; `ScriptOutcome` carries the result.
        if let Some(path) = &outcome.attach_script {
            match self.focused_member() {
                Some(member) => {
                    // Session-scope script permissions, read on demand from settings.json
                    // (attach is a rare explicit action, not a hot path). The App-scope
                    // default Allow stands where unset; `resolve_attach_permissions` applies
                    // the narrowing rule, then the actor maps the result to the link grant —
                    // so a session `document: Deny` fails the attach at instantiation.
                    // (Follow-on #1.)
                    let prefs = settings_store::load_settings(&self.shared.session.mere_root)
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                        .script_permissions;
                    let policy = crate::content::script::ScriptCapPolicy {
                        log: prefs.log,
                        document: prefs.document,
                        net: prefs.net,
                    };
                    let (log, document, net) =
                        crate::content::script::resolve_attach_permissions(policy);
                    self.shared.content.constellation.attach_script(
                        member,
                        std::path::PathBuf::from(path),
                        log,
                        document,
                        net,
                    );
                }
                None => note = Some("Focus a tile to attach a script to".to_string()),
            }
        }
        if let Some((kind, payload)) = &outcome.script_event {
            if let Some(member) = self.focused_member() {
                self.shared.content.constellation.deliver_script_event(
                    member,
                    kind.clone(),
                    payload.clone(),
                );
            }
        }
        if outcome.detach_script {
            if let Some(member) = self.focused_member() {
                self.shared.content.constellation.detach_script(member);
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
        self.view.chrome_update(move |c| c.show_location(&shown));
        self.view.request_redraw();
    }

    /// Run a `>sparql("…")` query over the focused graph and format a one-line
    /// result for the omnibar echo. Read-only (builds an ephemeral in-memory
    /// store via `linked_data::query`; the kernel stays the authority).
    fn run_sparql_query(&self, query: &str) -> String {
        match linked_data::query::sparql(self.orrery().graph(), query) {
            Err(err) => format!("SPARQL error: {err}"),
            Ok(rows) if rows.rows.is_empty() => "0 results".to_string(),
            Ok(rows) => format_sparql_rows(&rows),
        }
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

        let chrome = self.view.chrome();
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
        let Some(intent) = self.view.chrome().comms_intent.clone() else {
            return;
        };
        self.view.chrome_update(|c| c.comms_intent = None);
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
