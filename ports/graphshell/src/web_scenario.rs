// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page-side scenario lane: the page drives itself.
//!
//! `?scenario=<path>` on the page URL names a script; `loader.js` fetches it
//! and hands the text to [`run_scenario`] once the host reports ready. From
//! then on the frame pump ticks one verb per rendered frame through
//! `genet_probe`'s loop — the same parser, the same `act` / `settle` / `wait`
//! / `assert snap` / `capture` / `log` verbs woodshed and turnstone run — and
//! writes the result into the DOM: `<body data-scenario="ok|fail">`, the step
//! log under `#scenario-log`, one JSON line under `#scenario-result`, and each
//! capture as an `<img data-capture="name">` under `#scenario-captures`.
//!
//! No synthetic OS input, no external driver: the receipts the 2026-08-06
//! storage receipt's stop rule asked for become reproducible by loading a URL.
//!
//! What the browser adds to the shared grammar, because its targets are DOM
//! elements rather than cambium surfaces:
//!
//! - `dom click <css>` — `HTMLElement.click()` on the first match, so the
//!   ordinary delegated `data-command` handler runs.
//! - `focus <css>` — move focus, the way Tab would.
//! - `key <chord>` — a real `keydown` (`ctrl+shift+enter`, `left`, `escape`,
//!   `plus`, a bare character) on the active element, so it bubbles through
//!   the host's own listener and its form-field exemption.
//! - `type <css> <text>` — set a field's value and fire `input`.
//! - `click-at <x> <y>` — pointer press and release at canvas coordinates.
//! - `assert dom <css> <substring>` — the element's text contains it.
//! - `assert attr <css> <name> <value>` — an attribute equals it.
//! - `assert title <substring>` and `assert focused <css>`.
//!
//! `assert snap <field> <op> <value>` reads every `data-*` token on `<body>`
//! (as `session`, `detail-open`, `action-count`, …) plus `title`, `camera`
//! and `focused-node` from the canvas, so a scenario asserts the same tokens
//! the H3 receipts read by hand. The generic `click <selector>` resolves over
//! cambium surfaces the page does not retain, so it always misses here and
//! says so in the event stream; use `dom click`.

use std::cell::RefCell;
use std::rc::Rc;

use genet_probe::{Automatable, Driveable, ProbeSnapshot, ProbeSurface, Progress, Scenario};
use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, Element, Event, EventInit, HtmlCanvasElement, HtmlElement,
    HtmlInputElement, HtmlTextAreaElement, ImageData, KeyboardEvent, KeyboardEventInit,
    PointerEvent, PointerEventInit,
};

use super::{BrowserHost, document, element, root};

thread_local! {
    static HOST: RefCell<Option<Rc<RefCell<BrowserHost>>>> = const { RefCell::new(None) };
}

/// A scenario in flight on the host, pumped by [`tick`].
pub(crate) struct ScenarioRun {
    scenario: Scenario,
    steps: usize,
    frames: u32,
}

/// Remember the host so [`run_scenario`] can reach it from JavaScript.
pub(super) fn install(state: &Rc<RefCell<BrowserHost>>) {
    HOST.with(|slot| *slot.borrow_mut() = Some(state.clone()));
}

/// The booted host, for the other JavaScript entries.
pub(super) fn host() -> Option<Rc<RefCell<BrowserHost>>> {
    HOST.with(|slot| slot.borrow().clone())
}

/// Parse and arm a scenario. The frame pump runs it from the next frame.
#[wasm_bindgen]
pub fn run_scenario(text: &str) -> Result<(), JsValue> {
    let host = HOST
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| JsValue::from_str("the host has not booted"))?;
    let scenario = Scenario::parse(text).map_err(|error| JsValue::from_str(&error))?;
    let steps = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count();
    let mut host = host.borrow_mut();
    host.scenario = Some(ScenarioRun {
        scenario,
        steps,
        frames: 0,
    });
    mark(&document().map_err(js)?, "running", None).map_err(js)?;
    Ok(())
}

/// One frame's worth of scenario: called by the frame pump after the frame is
/// rendered and the semantics are mirrored, so an assertion sees the DOM as
/// it stands.
pub(super) fn tick(host: &mut BrowserHost) {
    let Some(mut run) = host.scenario.take() else {
        return;
    };
    run.frames += 1;
    let progress = {
        let mut probe = Probe { host };
        run.scenario.tick(&mut probe)
    };
    // Progress is visible while the run is alive, not only at its end: a
    // scenario that hangs on a `wait` must say how far it got, and the log
    // so far is what says it.
    if let Ok(document) = document() {
        if let Some(body) = document.body() {
            let _ = body.set_attribute("data-scenario-frames", &run.frames.to_string());
        }
        if let Ok(log) = page_element(&document, "scenario-log") {
            log.set_text_content(Some(&run.scenario.finish().log.join("\n")));
        }
    }
    match progress {
        Progress::Running => host.scenario = Some(run),
        Progress::Done => {
            let outcome = run.scenario.finish();
            let result = if outcome.ok { "ok" } else { "fail" };
            let report = serde_json::json!({
                "result": result,
                "steps": run.steps,
                "frames": run.frames,
                "captures": host.capture_count,
                "log": outcome.log,
            });
            if let Ok(document) = document() {
                if let Ok(log) = page_element(&document, "scenario-log") {
                    log.set_text_content(Some(&outcome.log.join("\n")));
                }
                let _ = mark(&document, result, Some(&report.to_string()));
                if let Ok(event) = Event::new("graphshell-scenario-complete") {
                    let _ = document.dispatch_event(&event);
                }
            }
        }
    }
}

/// Write `data-scenario` on the body and, when given, the result line.
fn mark(document: &web_sys::Document, state: &str, result: Option<&str>) -> Result<(), String> {
    let body = document.body().ok_or("document has no body")?;
    body.set_attribute("data-scenario", state)
        .map_err(|_| "could not mark the scenario state")?;
    if let Some(result) = result {
        page_element(document, "scenario-result")?.set_text_content(Some(result));
    }
    Ok(())
}

fn js(error: String) -> JsValue {
    JsValue::from_str(&error)
}

/// A page-level element (the lane's own output lives outside the component).
fn page_element(document: &web_sys::Document, id: &str) -> Result<Element, String> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| format!("missing #{id}"))
}

/// Finish a capture whose readback has landed: encode the pixels as a PNG
/// through a 2D canvas (the browser's encoder, so no image dependency) and
/// hang the data URL off the DOM where a receipt can collect it.
pub(super) fn publish_capture(name: &str, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let document = document()?;
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "could not create the capture canvas")?
        .dyn_into()
        .map_err(|_| "capture canvas is not a canvas")?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .ok_or("no 2d context for the capture")?
        .dyn_into()
        .map_err(|_| "capture context is not 2d")?;
    let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(rgba), width, height)
        .map_err(|_| "could not build the capture image")?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|_| "could not paint the capture")?;
    let url = canvas
        .to_data_url()
        .map_err(|_| "could not encode the capture")?;
    let img = document
        .create_element("img")
        .map_err(|_| "could not create the capture image")?;
    img.set_attribute("data-capture", name)
        .and_then(|_| img.set_attribute("width", &width.to_string()))
        .and_then(|_| img.set_attribute("height", &height.to_string()))
        .and_then(|_| img.set_attribute("alt", &format!("capture {name}")))
        .and_then(|_| img.set_attribute("src", &url))
        .map_err(|_| "could not describe the capture image")?;
    page_element(&document, "scenario-captures")?
        .append_child(&img)
        .map_err(|_| "could not publish the capture")?;
    Ok(())
}

/// The host as the shared driver sees it.
struct Probe<'a> {
    host: &'a mut BrowserHost,
}

impl Probe<'_> {
    /// Queue a pointer event for the canvas. Queued, not dispatched: see
    /// [`DomAction`].
    fn pointer(&mut self, kind: &'static str, x: f32, y: f32) {
        self.host.deferred_dom.push(DomAction::Pointer { kind, x, y });
    }

    fn app_verb(&mut self, line: &str) -> Result<(), String> {
        let (verb, rest) = split_first(line);
        match verb {
            "dom" => {
                let (what, css) = split_first(rest);
                if what != "click" {
                    return Err(format!("dom wants 'click <css>', got '{line}'"));
                }
                // Resolved now, so a wrong selector fails this step; clicked
                // after the tick, so the host's listener can take the host.
                html_element(css)?;
                self.host.deferred_dom.push(DomAction::Click(css.to_string()));
                Ok(())
            }
            "focus" => html_element(rest)?
                .focus()
                .map_err(|_| format!("focus '{rest}': refused")),
            "key" => {
                let spec = parse_chord(rest)?;
                self.host.deferred_dom.push(DomAction::Key(spec));
                Ok(())
            }
            "type" => {
                let (css, text) = split_first(rest);
                let target = find(css)?;
                if target.dyn_ref::<HtmlInputElement>().is_none()
                    && target.dyn_ref::<HtmlTextAreaElement>().is_none()
                {
                    return Err(format!("type '{css}': not an input or textarea"));
                }
                self.host.deferred_dom.push(DomAction::Type {
                    css: css.to_string(),
                    text: text.to_string(),
                });
                Ok(())
            }
            "click-at" => {
                let mut parts = rest.split_whitespace();
                let x: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("click-at wants x and y")?;
                let y: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("click-at wants x and y")?;
                self.press(x, y);
                self.release(x, y);
                Ok(())
            }
            "assert" => self.app_assert(rest, line),
            _ => Err(format!("unknown verb: {line}")),
        }
    }

    fn app_assert(&self, rest: &str, line: &str) -> Result<(), String> {
        let (kind, arg) = split_first(rest);
        match kind {
            "dom" => {
                let (css, expected) = split_first(arg);
                let text = find(css)?.text_content().unwrap_or_default();
                if text.contains(expected) {
                    Ok(())
                } else {
                    Err(format!("assert dom {css} '{expected}': got '{}'", text.trim()))
                }
            }
            "attr" => {
                let mut parts = arg.splitn(3, char::is_whitespace);
                let css = parts.next().ok_or("assert attr wants <css> <name> <value>")?;
                let name = parts.next().ok_or("assert attr wants <css> <name> <value>")?;
                let expected = parts.next().unwrap_or("").trim();
                let got = find(css)?.get_attribute(name);
                if got.as_deref() == Some(expected) {
                    Ok(())
                } else {
                    Err(format!("assert attr {css} {name} '{expected}': got {got:?}"))
                }
            }
            "title" => {
                let title = document()?.title();
                if title.contains(arg) {
                    Ok(())
                } else {
                    Err(format!("assert title '{arg}': got '{title}'"))
                }
            }
            "focused" => {
                let wanted = find(arg)?;
                let active = document()?.active_element();
                if active.as_ref().is_some_and(|active| active.is_same_node(Some(&wanted))) {
                    Ok(())
                } else {
                    Err(format!(
                        "assert focused {arg}: focus is on {:?}",
                        active.map(|active| active.tag_name())
                    ))
                }
            }
            _ => Err(format!("unknown assertion: {line}")),
        }
    }
}

impl Automatable for Probe<'_> {
    /// The page retains no cambium surface: the chrome scene is rebuilt from a
    /// model each frame and the DOM mirror is where the semantics live. So the
    /// shared `click` / `assert text` verbs see nothing here, by design; the
    /// DOM verbs above are the page's equivalents.
    fn with_surfaces<R>(&self, f: impl FnOnce(&[ProbeSurface<'_>]) -> R) -> R {
        f(&[])
    }

    fn snapshot(&self) -> ProbeSnapshot {
        let mut snap = ProbeSnapshot::default();
        if let Ok(document) = document() {
            snap = snap.with_field("title", document.title());
            // The component's tokens on its root, then the page's (the
            // scenario's own) on the body.
            let carriers: Vec<Element> = root()
                .ok()
                .into_iter()
                .chain(document.body().map(Element::from))
                .collect();
            for body in carriers {
                for name in body.get_attribute_names().iter() {
                    let Some(name) = name.as_string() else {
                        continue;
                    };
                    if let Some(key) = name.strip_prefix("data-") {
                        let value = body.get_attribute(&name).unwrap_or_default();
                        snap = snap.with_field(key, value);
                    }
                }
            }
        }
        let canvas = &self.host.canvas_element;
        let camera = canvas.get_attribute("data-camera").unwrap_or_default();
        // The camera's parts as numbers too, for `<=` / `>=`: a pan's distance
        // is velocity-shaped and differs run to run, so a scenario asserts
        // that it moved past a bound, not that it landed on a pixel.
        let mut parts = camera.split(',');
        snap = snap.with_field("camera-x", parts.next().unwrap_or_default());
        snap = snap.with_field("camera-y", parts.next().unwrap_or_default());
        snap = snap.with_field("camera", camera);
        snap = snap.with_field(
            "focused-node",
            canvas.get_attribute("data-focused-node").unwrap_or_default(),
        );
        snap = snap.with_field("action-status", self.host.action_status.clone());
        snap = snap.with_field("remote-link", self.host.remote_link_name());
        snap = snap.with_field("remote-state", self.host.remote_status.clone());
        snap = snap.with_field("remote-resume", self.host.remote_last_resume.clone());
        snap = snap.with_field(
            "remote-revision",
            self.host
                .remote_revision()
                .map(|revision| revision.to_string())
                .unwrap_or_default(),
        );
        snap = snap.with_field("product-status", self.host.product_status.clone());
        snap.with_field("scenario-frames", self.host.scenario_frames.to_string())
    }

    fn drain_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.host.probe_events)
    }

    fn act(&mut self, label: &str) -> bool {
        self.host.run_command(label)
    }

    fn press(&mut self, x: f32, y: f32) {
        self.pointer("pointerdown", x, y);
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.pointer("pointermove", x, y);
    }

    fn release(&mut self, x: f32, y: f32) {
        self.pointer("pointerup", x, y);
    }

    /// A capture in flight, or a remote answer still to come.
    fn busy(&mut self) -> Option<bool> {
        Some(self.host.capture_pending.is_some() || self.host.remote_in_flight())
    }
}

impl Driveable for Probe<'_> {
    /// Arm a capture: the next rendered frame composes and reads it back, and
    /// `wait` holds until it lands (see `busy`).
    fn capture(&mut self, name: &str) -> bool {
        if self.host.capture_pending.is_some() || self.host.capture_request.is_some() {
            return false;
        }
        self.host.capture_request = Some(name.to_string());
        true
    }

    fn app_step(&mut self, line: &str) -> Result<(), String> {
        self.app_verb(line)
    }
}

fn find(css: &str) -> Result<Element, String> {
    document()?
        .query_selector(css)
        .map_err(|_| format!("'{css}' is not a selector"))?
        .ok_or_else(|| format!("'{css}' matches nothing"))
}

fn html_element(css: &str) -> Result<HtmlElement, String> {
    find(css)?
        .dyn_into::<HtmlElement>()
        .map_err(|_| format!("'{css}' is not an HTML element"))
}

/// A DOM event a verb asked for, dispatched by the frame pump *after* the
/// tick has released its borrow of the host.
///
/// The reason is re-entrancy, and the first headed run found it: the host's
/// own listeners (`web_events`) take `borrow_mut` on the host, and a
/// synthetic event dispatched from inside the tick — where the host is
/// already borrowed — re-enters that borrow and traps. The browser swallows
/// a listener's exception, `dispatchEvent` returns normally, and the step
/// looks like it ran. So verbs only queue; the pump dispatches once the host
/// is free, and the effect is in the DOM by the next frame, which is when
/// the next step looks.
pub(crate) enum DomAction {
    Pointer { kind: &'static str, x: f32, y: f32 },
    Click(String),
    Key(KeySpec),
    Type { css: String, text: String },
}

/// A parsed key chord: modifiers and the `KeyboardEvent.key` value.
pub(crate) struct KeySpec {
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
    meta: bool,
}

/// Dispatch queued actions. Returns event-stream lines for anything that
/// could not be dispatched, so a scenario can assert on the failure rather
/// than lose it.
pub(super) fn run_deferred(actions: Vec<DomAction>) -> Vec<String> {
    let mut events = Vec::new();
    for action in actions {
        if let Err(error) = dispatch(action) {
            events.push(format!("dom-error {error}"));
        }
    }
    events
}

fn dispatch(action: DomAction) -> Result<(), String> {
    match action {
        DomAction::Pointer { kind, x, y } => {
            let canvas = element("graphshell-canvas")?;
            let rect = canvas.get_bounding_client_rect();
            let init = PointerEventInit::new();
            init.set_bubbles(true);
            init.set_cancelable(true);
            init.set_client_x((rect.left() + f64::from(x)) as i32);
            init.set_client_y((rect.top() + f64::from(y)) as i32);
            init.set_button(0);
            init.set_buttons(if kind == "pointerup" { 0 } else { 1 });
            init.set_pointer_id(1);
            init.set_is_primary(true);
            let event = PointerEvent::new_with_event_init_dict(kind, &init)
                .map_err(|_| format!("could not build {kind}"))?;
            canvas
                .dispatch_event(&event)
                .map_err(|_| format!("could not dispatch {kind}"))?;
            Ok(())
        }
        DomAction::Click(css) => {
            html_element(&css)?.click();
            Ok(())
        }
        DomAction::Key(spec) => {
            let init = KeyboardEventInit::new();
            init.set_bubbles(true);
            init.set_cancelable(true);
            init.set_key(&spec.key);
            init.set_ctrl_key(spec.ctrl);
            init.set_shift_key(spec.shift);
            init.set_alt_key(spec.alt);
            init.set_meta_key(spec.meta);
            let document = document()?;
            // The active element, the way a person's key would land; the
            // canvas when nothing else holds focus.
            let target: Element = match document.active_element() {
                Some(active) if active.tag_name() != "BODY" => active,
                _ => element("graphshell-canvas")?,
            };
            let event = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
                .map_err(|_| "could not build the key event".to_string())?;
            target
                .dispatch_event(&event)
                .map_err(|_| "could not dispatch the key event".to_string())?;
            Ok(())
        }
        DomAction::Type { css, text } => {
            let target = find(&css)?;
            if let Some(input) = target.dyn_ref::<HtmlInputElement>() {
                input.set_value(&text);
            } else if let Some(area) = target.dyn_ref::<HtmlTextAreaElement>() {
                area.set_value(&text);
            }
            let init = EventInit::new();
            init.set_bubbles(true);
            let event = Event::new_with_event_init_dict("input", &init)
                .map_err(|_| "could not build the input event".to_string())?;
            target
                .dispatch_event(&event)
                .map_err(|_| "could not dispatch the input event".to_string())?;
            Ok(())
        }
    }
}

/// `ctrl+shift+enter`: modifiers then one key. A chord of `plus` is spelled
/// by name because `+` is the separator.
fn parse_chord(chord: &str) -> Result<KeySpec, String> {
    let mut spec = KeySpec {
        key: String::new(),
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    };
    let mut named = false;
    for part in chord.split('+').map(str::trim).filter(|part| !part.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => spec.ctrl = true,
            "shift" => spec.shift = true,
            "alt" => spec.alt = true,
            "meta" | "cmd" => spec.meta = true,
            name => {
                spec.key = key_name(name, part);
                named = true;
            }
        }
    }
    if !named {
        return Err(format!("key '{chord}': no key named"));
    }
    Ok(spec)
}

fn key_name(lower: &str, verbatim: &str) -> String {
    match lower {
        "enter" | "return" => "Enter",
        "escape" | "esc" => "Escape",
        "tab" => "Tab",
        "space" => " ",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "plus" => "+",
        "minus" => "-",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        _ => verbatim,
    }
    .to_string()
}

fn split_first(line: &str) -> (&str, &str) {
    match line.trim().split_once(char::is_whitespace) {
        Some((head, tail)) => (head, tail.trim()),
        None => (line.trim(), ""),
    }
}
