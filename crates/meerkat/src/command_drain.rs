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

use crate::fetch::{ContentState, Fetched};

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
    let more = if rows.rows.len() > MAX_ROWS {
        " …"
    } else {
        ""
    };
    format!(
        "{} result(s): {}{}",
        rows.rows.len(),
        rendered.join(" | "),
        more
    )
}

impl WindowCtx<'_> {
    /// Dev-loop ring-dump escape hatch (Ctrl+Shift+D): write the full diagnostics ring to
    /// `<mere_root>/diagnostics-dump.txt` and surface the path as a toast. The Apparatus pane
    /// only shows a small recent window; this is the captured pulse + faults in full, readable
    /// or shareable outside the UI (and visible on stderr for a terminal dev loop).
    pub(super) fn dump_diagnostics(&mut self) {
        let report = self.shared.observability.dump_report();
        let path = self.shared.session.mere_root.join("diagnostics-dump.txt");
        match std::fs::write(&path, report.as_bytes()) {
            Ok(()) => {
                let where_ = path.display().to_string();
                eprintln!("[meerkat] diagnostics ring dumped to {where_}");
                self.shared.observability.record_notification(
                    Severity::Info,
                    "Diagnostics dumped",
                    where_,
                    true,
                );
            }
            Err(err) => self.shared.observability.record_notification(
                Severity::Warn,
                "Diagnostics dump failed",
                err.to_string(),
                true,
            ),
        }
        self.view.request_redraw();
    }

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
        self.shared
            .sync_handle
            .command(sync::SyncCommand::Connect(ticket));
        self.view.request_redraw();
    }

    /// Materialize the focused page's outbound-link neighborhood onto the canvas
    /// (relational-browse V1, single-hop): the focused node's already-fetched body is
    /// parsed render-free and its links become `Semantic:Hyperlink`-edged graph nodes
    /// around the seed, with no target fetch and no new actor. The contribution rides the
    /// same frame drain content harvest uses. Returns a status note for the omnibar echo.
    pub(super) fn materialize_focused(&mut self) -> Option<String> {
        let Some(member) = self.focused_member() else {
            return Some("Select a node to materialize".to_string());
        };
        self.shared.content.constellation.materialize_links(member);
        Some("Materialized the page's link neighborhood".to_string())
    }

    /// Crawl the focused page's outbound-link neighborhood into the focused graph
    /// (relational-browse V2): seed the crawl actor from the focused node's URL under a
    /// conservative default policy (same-host, shallow). The actor fetches off the
    /// render path; each page's links + metadata stream back as graph contributions the
    /// frame drain applies ([`Self::drain_crawl`]). Returns a status note for the
    /// omnibar echo.
    pub(super) fn crawl_focused(&mut self) -> Option<String> {
        let Some(member) = self.focused_member() else {
            return Some("Select a node to crawl".to_string());
        };
        let url = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, n)| n.url().to_string())
            .filter(|u| !u.is_empty());
        let Some(url) = url else {
            return Some("The focused node has no URL to crawl".to_string());
        };
        let graph = self.view.focused_graph;
        let policy = self.shared.content.crawl.policy();
        let scope = policy.scope.label().to_lowercase();
        let depth = policy.max_depth;
        let mode = if policy.seed_sitemap {
            "whole site, "
        } else {
            ""
        };
        self.shared.content.crawl.start(&url, policy, graph);
        Some(format!("Crawling {url} ({mode}{scope}, depth {depth})…"))
    }

    /// Stop the running crawl (relational-browse V2): flip the crawl actor's cancel
    /// flag so it halts at the next page. A no-op when nothing is running.
    pub(super) fn stop_crawl(&self) -> Option<String> {
        if self.shared.content.crawl.progress().running {
            self.shared.content.crawl.stop();
            Some("Stopping crawl…".to_string())
        } else {
            Some("No crawl is running".to_string())
        }
    }

    fn starter_knot_body(url: &str) -> String {
        let name = url.strip_prefix("knot://").unwrap_or("note");
        format!("# {name}\n\nA new knot note. Start writing.\n")
    }

    /// Toggle the source editor for the focused local knot note. The editor stays in
    /// chrome, but the command is host-drained because the target is a graph member.
    pub(super) fn toggle_focused_knot_editor(&mut self) -> Option<String> {
        if self.view.chrome().knot_editor_open {
            self.view.chrome_update(|c| c.close_knot_editor());
            self.view.request_redraw();
            return None;
        }
        let Some(member) = self.nav_target_member() else {
            return Some("Select a knot:// note to edit".to_string());
        };
        let Some((_, node)) = self.orrery().graph().get_node_by_id(member) else {
            return Some("Select a knot:// note to edit".to_string());
        };
        let url = node.url().to_string();
        if !url.starts_with("knot://") {
            return Some("Focused node is not a knot:// note".to_string());
        }
        let source = node
            .body
            .clone()
            .filter(|body| !body.is_empty())
            .or_else(|| match self.shared.content.pages.get(&url) {
                Some(ContentState::Ready(fetched))
                    if fetched.content_type.as_deref() == Some("text/x-knot") =>
                {
                    Some(fetched.body.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| Self::starter_knot_body(&url));
        let note = format!("Editing {url}");
        self.view
            .chrome_update(move |c| c.open_knot_editor_for(member, url, source));
        self.view.request_redraw();
        Some(note)
    }

    /// Apply the editor's one-shot Save request to the focused graph member's
    /// authored body and refresh the live note-rendering cache.
    pub(super) fn drain_knot_editor_save(&mut self) -> Option<String> {
        let mut request = None;
        self.view
            .chrome_update(|c| request = c.take_knot_editor_save());
        let Some((member, body)) = request else {
            return None;
        };
        let Some((_, node)) = self.orrery().graph().get_node_by_id(member) else {
            return Some("The edited note no longer exists".to_string());
        };
        let url = node.url().to_string();
        if !url.starts_with("knot://") {
            return Some("The edited node is not a knot:// note".to_string());
        }
        let body_for_graph = body.clone();
        let changed = self.orrery_mut().ingest_graph(|graph| {
            let Some((key, _)) = graph.get_node_by_id(member) else {
                return false;
            };
            let Some(node) = graph.get_node_mut(key) else {
                return false;
            };
            let mut changed = false;
            if node.body.as_deref() != Some(body_for_graph.as_str()) {
                node.body = Some(body_for_graph.clone());
                changed = true;
            }
            if node.mime_hint.as_deref() != Some("text/x-knot") {
                node.mime_hint = Some("text/x-knot".to_string());
                changed = true;
            }
            changed
        });
        self.shared.content.pages.insert(
            url.clone(),
            ContentState::Ready(Fetched {
                content_type: Some("text/x-knot".to_string()),
                body,
            }),
        );
        self.view.tile_textures.remove(&member);
        self.view.tile_bands.remove(&member);
        self.view.note_content_heights.remove(&member);
        if changed {
            self.save_session();
        }
        self.view.request_redraw();
        Some(format!("Saved {url}"))
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
                    self.save_session();
                    self.view.request_redraw();
                }
            }
            Command::ShowAllEdges => {
                if self.orrery_mut().show_all_edges() > 0 {
                    self.save_session();
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
            Command::ToggleAlembic => self.toggle_pane(PaneContent::Alembic),
            Command::ToggleShellbar => self.toggle_shellbar_visibility(),
            Command::RetryFocusedContent => self.retry_focused_content(),
            Command::StopFocusedOperation => self.stop_focused_operation(),
            Command::PinFocusedOperation => self.pin_focused_operation(),
            Command::ToggleCompatView => self.toggle_focus_compat(),
            Command::AssertEdge => {
                if self
                    .orrery_mut()
                    .assert_selected_relation(SemanticSubKind::UserGrouped)
                {
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
            // Freeze the focused graph into a content-addressed engram in private memory
            // (the Alembic spine). Reports the engram id + node count.
            Command::SaveGraphEngram => {
                note = Some(self.save_graph_engram());
            }
            // Materialize the focused page's link neighborhood (V1, single-hop): parse
            // the already-fetched body and place its links as graph nodes — no fetch.
            Command::MaterializeFocused => note = self.materialize_focused(),
            // Clip the focused surface/document into a graph-native knot note. A live
            // scriptable surface arms the picker; document lanes clip the cached body.
            Command::ClipFocused => note = self.start_clip_picker(),
            // Edit the focused local knot note's source, writing back to `Node.body`.
            Command::ToggleKnotEditor => note = self.toggle_focused_knot_editor(),
            // Crawl the focused page's link neighborhood into the graph (V2). The crawl
            // actor fetches off the render path; this only seeds it + reports a note.
            Command::CrawlFocused => note = self.crawl_focused(),
            Command::StopCrawl => note = self.stop_crawl(),
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
            self.view
                .chrome_update(move |c| c.run_command_and_close(cmd));
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
            if self
                .orrery_mut()
                .assert_selected_relation(relation_kind_from_str(kind))
            {
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
        // A `compose_engrams("<id-a>", "<id-b>")` call unions the two engrams by URL identity
        // into a new one and echoes the result. (Alembic B7-P3.)
        if let Some((id_a, id_b)) = &outcome.compose_engrams {
            note = Some(self.compose_engrams(id_a, id_b));
        }
        // A `sparql("…")` call runs over the focused graph and echoes the result.
        // Read-only: an ephemeral in-memory store, the kernel stays the authority.
        if let Some(query) = &outcome.sparql_query {
            note = Some(self.run_sparql_query(query));
        }
        // A `recall("…")` call rebuilds the lexical trail index from the browsing
        // corpus (titles + URLs + page text) and echoes the top BM25 hits. (C5.)
        if let Some(query) = &outcome.recall_query {
            note = Some(self.run_recall_query(query));
        }
        // A `capture(…)` call sets + persists the browse-capture consent (plan C4);
        // a bare `capture()` reports the current level.
        if let Some(level) = &outcome.capture_consent {
            note = Some(self.run_capture_consent(level));
        }
        // A `forget(…)` call deletes a page's browsing traces (plan C4).
        if let Some(url) = &outcome.forget_url {
            note = Some(self.run_forget(url));
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

    /// Run a `recall("…")` lexical query (capture plan C5): load the browsing-trace
    /// corpus, pull each visited page's main text from the durable content cache,
    /// re-mint the `eidetic-search` trail index with that text, and echo the top
    /// BM25 hits. The index is derived state, rebuilt from the corpus each call (a
    /// fresh, modest index; an incrementally-maintained one is the optimization).
    fn run_recall_query(&mut self, query: &str) -> String {
        let dir = self.shared.session.mere_root.join("search").join("trail");
        let Some(store) = self.shared.content.store.as_mut() else {
            return "recall: no content store".to_string();
        };
        let memory = match pollster::block_on(eidetic::browsing::BrowsingMemory::load(store, 64)) {
            Ok(memory) => memory,
            Err(err) => return format!("recall: load failed: {err}"),
        };
        let traces: Vec<_> = memory.traces().cloned().collect();
        if traces.is_empty() {
            return "recall: no trail captured yet".to_string();
        }
        // Pull each visited URL's main text from the durable content cache.
        let urls: std::collections::HashSet<String> = traces
            .iter()
            .flat_map(|t| t.events.iter().map(|e| e.to.url.clone()))
            .collect();
        let mut texts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for url in &urls {
            if let Ok(Some(content)) =
                pollster::block_on(session_runtime::content_store::load_content(store, url))
            {
                let body = String::from_utf8_lossy(&content.body);
                let doc = serval_static_dom::StaticDocument::parse(&body);
                if let Some(text) = serval_extract::extract_main_text(&doc) {
                    texts.insert(url.clone(), text);
                }
            }
        }
        let index = match eidetic_search::TrailIndex::rebuild_with_text(&dir, &traces, |u| {
            texts.get(u).cloned()
        }) {
            Ok(index) => index,
            Err(err) => return format!("recall: index build failed: {err}"),
        };
        match index.search(query, 5) {
            Ok(hits) if hits.is_empty() => format!("recall: no hits for \"{query}\""),
            Ok(hits) => {
                let shown: Vec<String> = hits
                    .iter()
                    .map(|h| h.title.clone().unwrap_or_else(|| h.url.clone()))
                    .collect();
                format!("recall \"{query}\": {}", shown.join(" | "))
            }
            Err(err) => format!("recall: search failed: {err}"),
        }
    }

    /// Apply + persist a `>capture` consent change (plan C4). An empty `level`
    /// reports the current setting; a recognized key (`off` / `corridor` / `full`)
    /// sets it and writes settings.json; an unrecognized key echoes an error.
    fn run_capture_consent(&mut self, level: &str) -> String {
        use crate::browse_capture::CaptureConsent;
        if level.trim().is_empty() {
            return format!(
                "capture: {} (off | corridor | full)",
                self.shared.content.capture_consent.as_key()
            );
        }
        let Some(consent) = CaptureConsent::from_key(level) else {
            return format!("capture: unknown level \"{level}\" (off | corridor | full)");
        };
        self.shared.content.capture_consent = consent;
        self.persist_settings();
        format!("capture: {}", consent.as_key())
    }

    /// Forget a page's browsing traces (plan C4): delete every trace mentioning the
    /// url; the `eidetic-search` index is rebuilt from the corpus, so it drops too.
    /// An empty `arg` forgets the focused node's url. Provenance-edge cleanup (the
    /// C3 derivations) is the remaining forget sub-step.
    fn run_forget(&mut self, arg: &str) -> String {
        let url = if !arg.trim().is_empty() {
            arg.trim().to_string()
        } else {
            match self.focused_member().and_then(|m| {
                self.orrery()
                    .graph()
                    .get_node_by_id(m)
                    .map(|(_, n)| n.url().to_string())
                    .filter(|u| !u.is_empty())
            }) {
                Some(u) => u,
                None => return "forget: no focused page with a url".to_string(),
            }
        };
        let Some(store) = self.shared.content.store.as_mut() else {
            return "forget: no content store".to_string();
        };
        let mut memory =
            match pollster::block_on(eidetic::browsing::BrowsingMemory::load(store, 64)) {
                Ok(memory) => memory,
                Err(err) => return format!("forget: load failed: {err}"),
            };
        match pollster::block_on(memory.forget_url(store, &url)) {
            Ok(0) => format!("forget: no traces mentioned {url}"),
            Ok(n) => format!("forget: removed {n} trace(s) for {url}"),
            Err(err) => format!("forget: failed: {err}"),
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
        self.shared
            .observability
            .record_actor("comms", "started", Some(format!("{intent:?}")));
        match intent {
            CommsIntent::Refresh => {
                self.shared
                    .comms_handle
                    .command(comms_host::CommsCommand::Refresh);
            }
            CommsIntent::Open(id) => {
                self.shared
                    .comms_handle
                    .command(comms_host::CommsCommand::Open(id));
            }
            CommsIntent::Send(draft) => {
                self.shared
                    .comms_handle
                    .command(comms_host::CommsCommand::Send(draft));
            }
            CommsIntent::ConnectCabal(ticket) => {
                self.shared
                    .comms_handle
                    .command(comms_host::CommsCommand::ConnectCabal(ticket));
            }
        }
    }

    /// Drain a session chip's captured gesture into the matching `ShellCommand` (the
    /// chrome can't reach the session pool). `Activate` opens the session beside the
    /// current one when Shift is held (the old switcher's Shift+click), else switches
    /// to it. (Chrome bar P4 — sessions in the toolbar.)
    pub(super) fn drain_session_intent(&mut self) {
        let Some(intent) = self.view.chrome().session_intent else {
            return;
        };
        self.view.chrome_update(|c| c.session_intent = None);
        match intent {
            meerkat::SessionIntent::Activate(id) => {
                if self.view.modifiers.shift {
                    self.commands.push(super::ShellCommand::OpenGraphBeside(id));
                } else {
                    self.commands.push(super::ShellCommand::SwitchSession(id));
                }
            }
            meerkat::SessionIntent::Close(id) => {
                self.commands.push(super::ShellCommand::CloseSession(id));
            }
            meerkat::SessionIntent::Create => {
                self.commands.push(super::ShellCommand::CreateSession);
            }
        }
    }
}
