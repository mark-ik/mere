/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The runtime half of the scenario subsystem: the headed self-drive pumped on the
//! event loop, plus the `Shell`-side executors it shares with the headless path.
//!
//! ## Self-drive, not OS input
//!
//! The headed harness's fragility was OS synthetic input (`SendKeys` focus races,
//! timing). The self-drive removes it: the app injects its own registry commands and
//! captures its own frames. `MEERKAT_SCENARIO=<path>` at boot loads a scenario; the
//! shell then runs it on the event loop, one step per tick, invoking the **same**
//! registry seam the palette / menu / agent harness use and capturing through the
//! existing `MEERKAT_CAPTURE_DIR` self-capture hook. Deterministic, headed, expressed
//! in the one vocabulary.
//!
//! ## The executors are shared
//!
//! [`Shell::scenario_invoke`] / [`Shell::scenario_theme`] / [`Shell::scenario_window_count`]
//! are always compiled and drive the real host seams, so the headless consumer
//! ([`Shell::run_scenario_headless`], test-only) and this headed pump run the *same*
//! step through the *same* code. Only the genuinely-differing layers diverge: spawn
//! needs the event loop, capture needs the GPU.

use std::path::{Path, PathBuf};

use meerkat::command::{Command, context_action_from_id};
use meerkat::submit_omnibar;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::WindowId;
use xilem_serval::TextInput;

use super::{Assertion, KeyChord, KeySym, NamedKey, Step, parse};
use crate::observability::Severity;
use crate::{Shell, WindowCtx};

/// The name of the request file the self-capture hook polls, in `MEERKAT_CAPTURE_DIR`.
const CAPTURE_REQUEST: &str = "request.txt";
/// The sentinel a driver waits on: written once the scenario finishes, its first line
/// `RESULT ok` / `RESULT fail`, then one line per logged step.
const DONE_SENTINEL: &str = "scenario.done";
/// A capture that never clears (bad window index, a window that never renders) must not
/// hang the run. Bound the wait; on timeout the step is logged and skipped.
const CAPTURE_TIMEOUT_TICKS: u32 = 900;

/// Where the current step is in its own little lifecycle. Most steps run and advance in
/// one tick (`Enter`); the two that must wait for frames park in `Settle` / `AwaitCapture`.
#[derive(Clone, Copy)]
enum Phase {
    /// Ready to run the step at `cursor`.
    Enter,
    /// Burn `left` more frames (a spawned window applying, an async settle) then resume.
    Settle { left: u32 },
    /// A capture is in flight on window `win`; poll the request file until the hook
    /// consumes it (or `ticks` exceeds the timeout).
    AwaitCapture { win: usize, ticks: u32 },
    /// A load-time error; report it on the first pump and exit.
    Fatal,
}

/// A loaded scenario plus its run cursor. Built at boot from `MEERKAT_SCENARIO`, pumped
/// once per `about_to_wait` tick while active.
pub(crate) struct ScenarioRunner {
    steps: Vec<Step>,
    cursor: usize,
    phase: Phase,
    /// `MEERKAT_CAPTURE_DIR`, where capture requests + PNGs go. `None` disables captures
    /// (they log a skip), since the self-capture hook is itself inert without it.
    capture_dir: Option<PathBuf>,
    /// Where the done sentinel lands: the capture dir if set, else the scenario file's dir.
    sentinel_dir: PathBuf,
    /// One line per executed step, mirrored into the sentinel for the driver to read.
    log: Vec<String>,
    /// Set by a failed assertion or a load error; decides the sentinel's `RESULT` line.
    failed: bool,
    fatal: Option<String>,
}

impl ScenarioRunner {
    /// Build a runner from the environment, or `None` when `MEERKAT_SCENARIO` is unset
    /// (the ordinary interactive launch). A read or parse error still returns a runner:
    /// it carries the message and reports it through the sentinel on the first pump, so a
    /// driver waiting on the sentinel gets a real failure rather than a silent hang.
    pub(crate) fn from_env() -> Option<ScenarioRunner> {
        let path = PathBuf::from(std::env::var_os("MEERKAT_SCENARIO")?);
        let capture_dir = std::env::var_os("MEERKAT_CAPTURE_DIR").map(PathBuf::from);
        let sentinel_dir = capture_dir.clone().unwrap_or_else(|| {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let (steps, fatal) = match std::fs::read_to_string(&path) {
            Ok(src) => match parse(&src) {
                Ok(steps) => (steps, None),
                Err(e) => (Vec::new(), Some(format!("parse error: {e}"))),
            },
            Err(e) => (
                Vec::new(),
                Some(format!("cannot read {}: {e}", path.display())),
            ),
        };
        let phase = if fatal.is_some() {
            Phase::Fatal
        } else {
            Phase::Enter
        };
        let failed = fatal.is_some();
        Some(ScenarioRunner {
            steps,
            cursor: 0,
            phase,
            capture_dir,
            sentinel_dir,
            log: Vec::new(),
            failed,
            fatal,
        })
    }

    fn capture_request_path(&self) -> Option<PathBuf> {
        self.capture_dir.as_ref().map(|d| d.join(CAPTURE_REQUEST))
    }
}

impl Shell {
    /// Advance the active scenario by one tick, from `about_to_wait` (after `apply`, so a
    /// spawn queued last tick has already opened its window). A no-op with no scenario or
    /// before the primary window exists.
    pub(crate) fn pump_scenario(&mut self, event_loop: &ActiveEventLoop) {
        if self.scenario.is_none() || self.primary.is_none() {
            return;
        }
        // Keep the loop hot so ticks progress without OS input, and treat the run as
        // activity so the idle forgetting / snapshot passes stay off (determinism).
        event_loop.set_control_flow(ControlFlow::Poll);
        self.last_activity = std::time::Instant::now();

        // Bounded by the step count: every non-waiting branch advances the cursor or
        // parks in a waiting phase and returns, so this cannot spin.
        loop {
            let phase = self.scenario.as_ref().expect("scenario present").phase;
            match phase {
                Phase::Fatal => {
                    self.finish_scenario(event_loop);
                    return;
                }
                Phase::Settle { left } => {
                    if left == 0 {
                        self.scenario_mut().phase = Phase::Enter;
                        continue;
                    }
                    self.request_window_redraw(0);
                    self.scenario_mut().phase = Phase::Settle { left: left - 1 };
                    return;
                }
                Phase::AwaitCapture { win, ticks } => {
                    let done = self
                        .scenario_ref()
                        .capture_request_path()
                        .map(|p| !p.exists())
                        .unwrap_or(true);
                    if done {
                        self.scenario_mut().phase = Phase::Enter;
                        continue;
                    }
                    if ticks >= CAPTURE_TIMEOUT_TICKS {
                        self.scenario_log("capture timed out (window never rendered?)");
                        self.scenario_mut().failed = true;
                        self.scenario_mut().phase = Phase::Enter;
                        continue;
                    }
                    self.request_window_redraw(win);
                    self.scenario_mut().phase = Phase::AwaitCapture {
                        win,
                        ticks: ticks + 1,
                    };
                    return;
                }
                Phase::Enter => {
                    let (cursor, len) = {
                        let r = self.scenario_ref();
                        (r.cursor, r.steps.len())
                    };
                    if cursor >= len {
                        self.finish_scenario(event_loop);
                        return;
                    }
                    let step = self.scenario_ref().steps[cursor].clone();
                    if self.run_step(step) {
                        return; // the step parked in a waiting phase
                    }
                    // otherwise it advanced the cursor; run the next this tick
                }
            }
        }
    }

    /// Execute one step from `Phase::Enter`. Returns `true` if the step parked in a
    /// waiting phase (the caller must yield the tick), `false` if it advanced the cursor
    /// and the pump should continue to the next step.
    fn run_step(&mut self, step: Step) -> bool {
        match step {
            Step::Invoke { id, win } => {
                let ok = self.scenario_invoke(&id, win);
                self.scenario_log(format!(
                    "invoke {id} @{win}: {}",
                    if ok { "applied" } else { "unknown id" }
                ));
                if !ok {
                    self.scenario_mut().failed = true;
                }
                self.request_window_redraw(win);
                self.scenario_mut().cursor += 1;
                false
            }
            Step::Navigate { url, win } => {
                let ok = self.scenario_navigate(&url, win);
                self.scenario_log(format!(
                    "navigate {url} @{win}: {}",
                    if ok { "submitted" } else { "no window" }
                ));
                if !ok {
                    self.scenario_mut().failed = true;
                }
                self.request_window_redraw(win);
                self.scenario_mut().cursor += 1;
                false
            }
            Step::Key { chord, win } => {
                let ok = self.scenario_key(chord, win);
                self.scenario_log(format!(
                    "key {} @{win}: {}",
                    chord_label(&chord),
                    if ok { "dispatched" } else { "no window" }
                ));
                if !ok {
                    self.scenario_mut().failed = true;
                }
                self.request_window_redraw(win);
                self.scenario_mut().cursor += 1;
                false
            }
            Step::Theme { id, win } => {
                let ok = self.scenario_theme(&id, win);
                self.scenario_log(format!("theme {id} @{win}: {}", if ok { "set" } else { "no window" }));
                self.request_window_redraw(win);
                self.scenario_mut().cursor += 1;
                false
            }
            Step::Log { message } => {
                self.scenario_log(format!("log: {message}"));
                self.scenario_mut().cursor += 1;
                false
            }
            Step::Assert(assertion) => {
                self.eval_assertion(assertion);
                self.scenario_mut().cursor += 1;
                false
            }
            Step::Spawn => {
                self.commands.push(crate::ShellCommand::SpawnWindow);
                self.scenario_log("spawn: window queued");
                self.scenario_mut().cursor += 1;
                // Yield one tick so `apply` opens the window before the next step runs.
                self.scenario_mut().phase = Phase::Settle { left: 1 };
                true
            }
            Step::Settle { frames } => {
                self.scenario_mut().cursor += 1;
                self.scenario_mut().phase = Phase::Settle { left: frames };
                true
            }
            Step::Capture { name, win } => {
                self.scenario_mut().cursor += 1;
                match self.scenario_ref().capture_dir.clone() {
                    Some(dir) => {
                        write_capture_request(&dir, &name, win);
                        self.scenario_log(format!("capture {name} @{win}: requested"));
                        self.request_window_redraw(win);
                        self.scenario_mut().phase = Phase::AwaitCapture { win, ticks: 0 };
                        true
                    }
                    None => {
                        self.scenario_log(format!(
                            "capture {name} @{win}: skipped (MEERKAT_CAPTURE_DIR unset)"
                        ));
                        false
                    }
                }
            }
        }
    }

    fn eval_assertion(&mut self, assertion: Assertion) {
        match assertion {
            Assertion::Windows { op, n } => {
                let actual = self.scenario_window_count();
                let ok = op.eval(actual, n);
                if !ok {
                    self.scenario_mut().failed = true;
                }
                self.scenario_log(format!(
                    "assert windows {} {n}: {} (actual {actual})",
                    op.symbol(),
                    if ok { "PASS" } else { "FAIL" }
                ));
            }
        }
    }

    /// Write the done sentinel and exit the event loop. The first sentinel line is
    /// `RESULT ok` / `RESULT fail` for a driver to branch on; the rest is the step log.
    fn finish_scenario(&mut self, event_loop: &ActiveEventLoop) {
        let Some(runner) = self.scenario.take() else {
            return;
        };
        let result = if runner.failed { "fail" } else { "ok" };
        let mut body = format!("RESULT {result}\n");
        if let Some(fatal) = &runner.fatal {
            body.push_str(&format!("FATAL {fatal}\n"));
        }
        for line in &runner.log {
            body.push_str(line);
            body.push('\n');
        }
        let sentinel = runner.sentinel_dir.join(DONE_SENTINEL);
        if let Err(e) = std::fs::write(&sentinel, &body) {
            eprintln!("[meerkat] scenario sentinel write failed: {e}");
        }
        eprintln!("[meerkat] scenario {result}\n{body}");
        event_loop.exit();
    }

    // --- shared executors (also driven by the headless consumer) -------------------

    /// Invoke a registry id (command verb or context-action) on window `win`, through the
    /// same host seam the palette / menu / agent harness use. `false` for an unknown id or
    /// a non-existent target window. Twin of the agent harness's `agent_invoke`.
    pub(crate) fn scenario_invoke(&mut self, id: &str, win: usize) -> bool {
        match self.window_id_for_projection(win) {
            Some(wid) => match self.window_ctx(wid) {
                Some(mut wc) => wc.scenario_invoke(id),
                None => false,
            },
            // No live OS window for this index: the primary (win 0) still resolves to the
            // primary-or-pending ctx (covers the headless path + pre-`resumed` boot).
            None if win == 0 => self.ctx().scenario_invoke(id),
            None => false,
        }
    }

    /// Navigate window `win` to `url` through the omnibar-submit path. `false` if the
    /// target window does not exist.
    pub(crate) fn scenario_navigate(&mut self, url: &str, win: usize) -> bool {
        match self.window_id_for_projection(win) {
            Some(wid) => match self.window_ctx(wid) {
                Some(mut wc) => {
                    wc.scenario_navigate(url);
                    true
                }
                None => false,
            },
            None if win == 0 => {
                self.ctx().scenario_navigate(url);
                true
            }
            None => false,
        }
    }

    /// Dispatch a key chord to window `win` through the real key path. `false` if the
    /// target window does not exist.
    pub(crate) fn scenario_key(&mut self, chord: KeyChord, win: usize) -> bool {
        match self.window_id_for_projection(win) {
            Some(wid) => match self.window_ctx(wid) {
                Some(mut wc) => {
                    wc.scenario_key(chord);
                    true
                }
                None => false,
            },
            None if win == 0 => {
                self.ctx().scenario_key(chord);
                true
            }
            None => false,
        }
    }

    /// Set the active theme on window `win`. `false` if the target window does not exist.
    pub(crate) fn scenario_theme(&mut self, id: &str, win: usize) -> bool {
        match self.window_id_for_projection(win) {
            Some(wid) => match self.window_ctx(wid) {
                Some(mut wc) => {
                    wc.set_theme(id);
                    true
                }
                None => false,
            },
            None if win == 0 => {
                self.ctx().set_theme(id);
                true
            }
            None => false,
        }
    }

    /// The window count a `windows` assertion reads: the live OS windows once the app is
    /// up, else the projection count (the headless path holds the primary as a pending
    /// view outside the registry, but its projection is already pushed).
    pub(crate) fn scenario_window_count(&self) -> usize {
        if self.windows.is_empty() {
            self.multi.state().windows.len()
        } else {
            self.windows.len()
        }
    }

    fn window_id_for_projection(&self, win: usize) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, v)| v.projection_id.0 == win)
            .map(|(id, _)| *id)
    }

    fn request_window_redraw(&self, win: usize) {
        if let Some((_, v)) = self.windows.iter().find(|(_, v)| v.projection_id.0 == win) {
            v.request_redraw();
        }
    }

    fn scenario_ref(&self) -> &ScenarioRunner {
        self.scenario.as_ref().expect("scenario present")
    }

    fn scenario_mut(&mut self) -> &mut ScenarioRunner {
        self.scenario.as_mut().expect("scenario present")
    }

    fn scenario_log(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.shared.observability.record_diagnostic(
            "meerkat.scenario",
            Severity::Info,
            message.clone(),
        );
        if let Some(r) = self.scenario.as_mut() {
            r.log.push(message);
        }
    }
}

impl WindowCtx<'_> {
    /// Invoke a registry id against this window: a command verb runs + drains like the
    /// palette path; a context-action id seeds the working set from the live selection and
    /// runs the context drain. `false` for an id in neither registry. Mirrors the agent
    /// harness's `agent_invoke_command` / `agent_invoke_context` so both stay one seam.
    pub(crate) fn scenario_invoke(&mut self, id: &str) -> bool {
        if let Some(cmd) = Command::from_id(id) {
            self.chrome_update(move |c| c.run_command_and_close(cmd));
            let _ = self.drain_pending_command();
            self.drain_pending_connect();
            self.sync_comms_pane();
            self.sync_settings();
            true
        } else if let Some(action) = context_action_from_id(id) {
            let set = self.selection_working_set();
            self.view.context_set = set;
            self.view.context_origin = None;
            self.chrome_update(move |c| c.pick_context(action));
            self.drain_pending_context();
            true
        } else {
            false
        }
    }

    /// Navigate this window to `url` through the omnibar-submit path: seed the address
    /// bar, submit (classify + resolve + history), and run the host's `sync_orrery` apply
    /// (mint / navigate the node, load). The same route Enter in the omnibar takes.
    pub(crate) fn scenario_navigate(&mut self, url: &str) {
        let url = url.to_string();
        self.chrome_update(move |c| {
            c.omnibar = TextInput::new(&url);
            submit_omnibar(c);
        });
        self.sync_orrery();
        self.view.request_redraw();
    }

    /// Dispatch a key chord to this window through the real `on_key_pressed` path: set the
    /// modifier state the handler reads (as an OS `ModifiersChanged` would), press the key,
    /// then restore the modifiers so nothing leaks to the next step.
    pub(crate) fn scenario_key(&mut self, chord: KeyChord) {
        let (c, s, a, m) = (
            self.view.modifiers.ctrl,
            self.view.modifiers.shift,
            self.view.modifiers.alt,
            self.view.modifiers.meta,
        );
        self.view.modifiers.ctrl = chord.ctrl;
        self.view.modifiers.shift = chord.shift;
        self.view.modifiers.alt = chord.alt;
        self.view.modifiers.meta = chord.meta;
        self.on_key_pressed(&winit_key(chord.key));
        self.view.modifiers.ctrl = c;
        self.view.modifiers.shift = s;
        self.view.modifiers.alt = a;
        self.view.modifiers.meta = m;
        self.view.request_redraw();
    }
}

/// Map a scenario [`KeySym`] onto the `winit` key `on_key_pressed` matches against.
fn winit_key(sym: KeySym) -> WinitKey {
    match sym {
        KeySym::Char(c) => WinitKey::Character(c.to_string().into()),
        KeySym::Named(n) => WinitKey::Named(match n {
            NamedKey::Enter => WinitNamedKey::Enter,
            NamedKey::Escape => WinitNamedKey::Escape,
            NamedKey::Tab => WinitNamedKey::Tab,
            NamedKey::Backspace => WinitNamedKey::Backspace,
            NamedKey::Delete => WinitNamedKey::Delete,
            NamedKey::Space => WinitNamedKey::Space,
            NamedKey::Up => WinitNamedKey::ArrowUp,
            NamedKey::Down => WinitNamedKey::ArrowDown,
            NamedKey::Left => WinitNamedKey::ArrowLeft,
            NamedKey::Right => WinitNamedKey::ArrowRight,
            NamedKey::Home => WinitNamedKey::Home,
            NamedKey::End => WinitNamedKey::End,
            NamedKey::PageUp => WinitNamedKey::PageUp,
            NamedKey::PageDown => WinitNamedKey::PageDown,
            NamedKey::F(n) => f_key(n),
        }),
    }
}

/// `1..=12` to the corresponding `winit` function key. The parser bounds `n` to that
/// range, so the fallback is unreachable.
fn f_key(n: u8) -> WinitNamedKey {
    match n {
        1 => WinitNamedKey::F1,
        2 => WinitNamedKey::F2,
        3 => WinitNamedKey::F3,
        4 => WinitNamedKey::F4,
        5 => WinitNamedKey::F5,
        6 => WinitNamedKey::F6,
        7 => WinitNamedKey::F7,
        8 => WinitNamedKey::F8,
        9 => WinitNamedKey::F9,
        10 => WinitNamedKey::F10,
        11 => WinitNamedKey::F11,
        _ => WinitNamedKey::F12,
    }
}

/// Render a chord back to its `ctrl+shift+f` form for the step log.
fn chord_label(chord: &KeyChord) -> String {
    let mut s = String::new();
    if chord.ctrl {
        s.push_str("ctrl+");
    }
    if chord.shift {
        s.push_str("shift+");
    }
    if chord.alt {
        s.push_str("alt+");
    }
    if chord.meta {
        s.push_str("meta+");
    }
    match chord.key {
        KeySym::Char(c) => s.push(c),
        KeySym::Named(NamedKey::F(n)) => s.push_str(&format!("f{n}")),
        KeySym::Named(named) => s.push_str(match named {
            NamedKey::Enter => "enter",
            NamedKey::Escape => "escape",
            NamedKey::Tab => "tab",
            NamedKey::Backspace => "backspace",
            NamedKey::Delete => "delete",
            NamedKey::Space => "space",
            NamedKey::Up => "up",
            NamedKey::Down => "down",
            NamedKey::Left => "left",
            NamedKey::Right => "right",
            NamedKey::Home => "home",
            NamedKey::End => "end",
            NamedKey::PageUp => "pageup",
            NamedKey::PageDown => "pagedown",
            NamedKey::F(_) => "f",
        }),
    }
    s
}

/// Write the self-capture request the render hook polls: the target PNG path, then the
/// window's projection index so only that window's frame captures (a leaf and the primary
/// can both be in flight). The single-line legacy form (path only) still captures on any
/// window.
fn write_capture_request(dir: &Path, name: &str, win: usize) {
    let png = dir.join(format!("{name}.png"));
    let request = dir.join(CAPTURE_REQUEST);
    let body = format!("{}\n{win}\n", png.display());
    if let Err(e) = std::fs::write(&request, body) {
        eprintln!("[meerkat] scenario capture request write failed: {e}");
    }
}

#[cfg(test)]
impl Shell {
    /// The headless consumer of the same vocabulary: run each step against state, skipping
    /// the layers a headless process cannot exercise (spawn needs the event loop, capture
    /// needs the GPU). Returns the step log (each `assert` line ending PASS / FAIL) so a
    /// test can prove the same scenario the headed app self-drives also drives the
    /// in-process harness. (The unification the headed-automation plan names.)
    pub(crate) fn run_scenario_headless(&mut self, steps: &[Step]) -> Vec<String> {
        let mut log = Vec::new();
        for step in steps {
            match step {
                Step::Invoke { id, win } => {
                    let ok = self.scenario_invoke(id, *win);
                    log.push(format!(
                        "invoke {id} @{win}: {}",
                        if ok { "applied" } else { "unknown id" }
                    ));
                }
                Step::Navigate { url, win } => {
                    let ok = self.scenario_navigate(url, *win);
                    log.push(format!(
                        "navigate {url} @{win}: {}",
                        if ok { "submitted" } else { "no window" }
                    ));
                }
                Step::Key { chord, win } => {
                    let ok = self.scenario_key(*chord, *win);
                    log.push(format!(
                        "key {} @{win}: {}",
                        chord_label(chord),
                        if ok { "dispatched" } else { "no window" }
                    ));
                }
                Step::Theme { id, win } => {
                    let ok = self.scenario_theme(id, *win);
                    log.push(format!("theme {id} @{win}: {}", if ok { "set" } else { "no window" }));
                }
                Step::Assert(Assertion::Windows { op, n }) => {
                    let actual = self.scenario_window_count();
                    let ok = op.eval(actual, *n);
                    log.push(format!(
                        "assert windows {} {n}: {} (actual {actual})",
                        op.symbol(),
                        if ok { "PASS" } else { "FAIL" }
                    ));
                }
                Step::Log { message } => log.push(format!("log: {message}")),
                Step::Settle { .. } => {}
                Step::Spawn => log.push("spawn: skipped (headless)".to_string()),
                Step::Capture { name, win } => {
                    log.push(format!("capture {name} @{win}: skipped (headless)"))
                }
            }
        }
        log
    }
}

#[cfg(test)]
mod tests {
    use crate::scenario::parse;

    fn test_app() -> crate::test_support::TestShell {
        let (_tx, rx) = std::sync::mpsc::channel();
        let temp = crate::test_support::temp_session_dir("mere-scenario-tests");
        let shell = crate::Shell::new_with_session_dir(
            crate::test_support::event_loop_proxy(),
            rx,
            temp.path().to_path_buf(),
        );
        crate::test_support::TestShell::new(shell, temp)
    }

    #[test]
    fn headless_runs_the_same_vocabulary_the_headed_app_self_drives() {
        let src = "\
# the multi-window smoke, headless: spawn/capture degrade, invoke + assert are real
invoke roster
assert windows == 1
spawn
capture shot
log fin
";
        let steps = parse(src).expect("parses");
        let mut app = test_app();
        let log = app.run_scenario_headless(&steps);

        assert_eq!(log[0], "invoke roster @0: applied");
        assert!(log[1].ends_with("PASS (actual 1)"), "{}", log[1]);
        assert_eq!(log[2], "spawn: skipped (headless)");
        assert!(log[3].contains("skipped (headless)"), "{}", log[3]);
        assert_eq!(log[4], "log: fin");
    }

    #[test]
    fn an_unknown_registry_id_is_reported_not_applied() {
        let steps = parse("invoke not_a_real_verb").expect("parses");
        let mut app = test_app();
        let log = app.run_scenario_headless(&steps);
        assert_eq!(log[0], "invoke not_a_real_verb @0: unknown id");
    }

    #[test]
    fn invoke_routes_through_the_real_command_seam() {
        // `roster` toggles the roster pane; a second invoke toggles it back. Proves the
        // scenario invoke reaches the same host effect the palette does, not a stub.
        let steps = parse("invoke roster").expect("parses");
        let mut app = test_app();
        let opened_before = app.ctx().pane_of_content(&frame::PaneContent::Roster).is_some();
        app.run_scenario_headless(&steps);
        let opened_after = app.ctx().pane_of_content(&frame::PaneContent::Roster).is_some();
        assert_ne!(opened_before, opened_after, "roster pane toggled");
    }

    #[test]
    fn key_ctrl_f_toggles_the_find_bar() {
        // Ctrl+F is a chord, not a registry command; the `key` verb reaches the same
        // `on_key_pressed` -> `toggle_find` path the real keyboard does.
        let steps = parse("key ctrl+f").expect("parses");
        let mut app = test_app();
        let before = app.ctx().chrome().find_open;
        app.run_scenario_headless(&steps);
        let after = app.ctx().chrome().find_open;
        assert_ne!(before, after, "find bar toggled by the chord");
    }

    #[test]
    fn navigate_drives_the_omnibar_submit_path() {
        let steps = parse("navigate https://example.com").expect("parses");
        let mut app = test_app();
        app.run_scenario_headless(&steps);
        assert_eq!(
            app.ctx().chrome().content_location(),
            "https://example.com",
            "navigate resolved + submitted the URL through the omnibar path"
        );
    }
}
