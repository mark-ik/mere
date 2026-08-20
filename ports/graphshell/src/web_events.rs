//! Browser input and animation-frame wiring for the Graphshell presenter.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{
    Element, Event, HtmlSelectElement, KeyboardEvent, MouseEvent, PointerEvent, WheelEvent,
};

use super::{ActiveSession, BrowserHost, document, update_semantics, window};

pub(super) fn install_events(state: &Rc<RefCell<BrowserHost>>) -> Result<(), String> {
    let canvas = state.borrow().canvas_element.clone();

    let down_state = state.clone();
    let pointer_down = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        if let Some(button) = BrowserHost::pointer_button(event.button()) {
            let mut host = down_state.borrow_mut();
            if host.active == ActiveSession::Local {
                let (x, y) = host.pointer_position(event.client_x(), event.client_y());
                host.canvas.pointer_down(button, x, y);
            }
        }
    });
    canvas
        .add_event_listener_with_callback("pointerdown", pointer_down.as_ref().unchecked_ref())
        .map_err(|_| "could not attach pointerdown")?;
    pointer_down.forget();

    let move_state = state.clone();
    let pointer_move = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        let mut host = move_state.borrow_mut();
        if host.active == ActiveSession::Local {
            let (x, y) = host.pointer_position(event.client_x(), event.client_y());
            host.canvas.cursor_moved(x, y);
        }
    });
    canvas
        .add_event_listener_with_callback("pointermove", pointer_move.as_ref().unchecked_ref())
        .map_err(|_| "could not attach pointermove")?;
    pointer_move.forget();

    // Keep a mousemove compatibility path for browser automation and legacy
    // primary-button input. Replaying the same coordinate through Canvas is
    // idempotent when both pointer and mouse events are present.
    let mouse_move_state = state.clone();
    let mouse_move = Closure::<dyn FnMut(MouseEvent)>::new(move |event: MouseEvent| {
        let mut host = mouse_move_state.borrow_mut();
        if host.active == ActiveSession::Local {
            let (x, y) = host.pointer_position(event.client_x(), event.client_y());
            host.canvas.cursor_moved(x, y);
        }
    });
    canvas
        .add_event_listener_with_callback("mousemove", mouse_move.as_ref().unchecked_ref())
        .map_err(|_| "could not attach mousemove fallback")?;
    mouse_move.forget();

    let up_state = state.clone();
    let pointer_up = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        if let Some(button) = BrowserHost::pointer_button(event.button()) {
            let mut host = up_state.borrow_mut();
            if host.active == ActiveSession::Local {
                let (x, y) = host.pointer_position(event.client_x(), event.client_y());
                host.canvas.pointer_up(button, x, y);
                if let Some(member) = host.canvas.focused_member() {
                    host.primary_member = Some(member);
                }
                host.refresh_representation_score();
                host.chrome_dirty = true;
                let _ = update_semantics(&mut host);
            }
        }
    });
    canvas
        .add_event_listener_with_callback("pointerup", pointer_up.as_ref().unchecked_ref())
        .map_err(|_| "could not attach pointerup")?;
    pointer_up.forget();

    let wheel_state = state.clone();
    let wheel = Closure::<dyn FnMut(WheelEvent)>::new(move |event: WheelEvent| {
        event.prevent_default();
        let mut host = wheel_state.borrow_mut();
        if host.active == ActiveSession::Local {
            let (x, y) = host.pointer_position(event.client_x(), event.client_y());
            host.canvas.cursor_moved(x, y);
            host.canvas.set_ctrl(event.ctrl_key());
            host.canvas
                .wheel(-(event.delta_x() as f32), -(event.delta_y() as f32));
            host.canvas.set_ctrl(false);
            host.refresh_representation_score();
            let _ = update_semantics(&mut host);
        }
    });
    canvas
        .add_event_listener_with_callback("wheel", wheel.as_ref().unchecked_ref())
        .map_err(|_| "could not attach wheel")?;
    wheel.forget();

    let click_state = state.clone();
    let click = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Some(target) = event
            .target()
            .and_then(|target| target.dyn_into::<Element>().ok())
        else {
            return;
        };
        if target.has_attribute("data-action-draft-submit") {
            let mut host = click_state.borrow_mut();
            host.run_command("submit-action-draft");
            let _ = update_semantics(&mut host);
            return;
        }
        let Some(command) = target.get_attribute("data-command") else {
            return;
        };
        let mut host = click_state.borrow_mut();
        host.run_command(&command);
        let _ = update_semantics(&mut host);
    });
    document()?
        .add_event_listener_with_callback("click", click.as_ref().unchecked_ref())
        .map_err(|_| "could not attach command listener")?;
    click.forget();

    let change_state = state.clone();
    let change = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<HtmlSelectElement>().ok())
        else {
            return;
        };
        let Some(field) = select.get_attribute("data-action-draft-field") else {
            return;
        };
        let mut host = change_state.borrow_mut();
        host.choose_action_draft(&field, &select.value());
        let _ = update_semantics(&mut host);
    });
    document()?
        .add_event_listener_with_callback("change", change.as_ref().unchecked_ref())
        .map_err(|_| "could not attach action-draft change listener")?;
    change.forget();

    let key_state = state.clone();
    let keydown = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        if event
            .target()
            .and_then(|target| target.dyn_into::<Element>().ok())
            .is_some_and(|target| {
                target.has_attribute("data-action-draft-field")
                    || target.has_attribute("data-action-draft-submit")
            })
        {
            return;
        }
        let command = match event.key().as_str() {
            "ArrowLeft" => "pan-left",
            "ArrowRight" => "pan-right",
            "ArrowUp" => "pan-up",
            "ArrowDown" => "pan-down",
            "+" | "=" => "zoom-in",
            "-" | "_" => "zoom-out",
            "Enter" => "open-detail",
            "Escape" => "close-detail",
            _ => return,
        };
        event.prevent_default();
        let mut host = key_state.borrow_mut();
        host.run_command(command);
        let _ = update_semantics(&mut host);
    });
    document()?
        .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
        .map_err(|_| "could not attach keyboard listener")?;
    keydown.forget();
    Ok(())
}

pub(super) fn schedule_frames(state: Rc<RefCell<BrowserHost>>) -> Result<(), String> {
    type FrameClosure = Closure<dyn FnMut(f64)>;
    let frame = Rc::new(RefCell::new(None::<FrameClosure>));
    let next = frame.clone();
    let callback_state = state.clone();
    *next.borrow_mut() = Some(Closure::new(move |host_ms: f64| {
        {
            let mut host = callback_state.borrow_mut();
            if let Err(error) = host.render(host_ms) {
                web_sys::console::error_1(&error.clone().into());
                if let Ok(document) = document() {
                    document.set_title(&format!("GRAPHSHELL H3 FAIL: {error}"));
                }
            } else {
                let _ = update_semantics(&mut host);
            }
        }
        if let Ok(window) = window()
            && let Some(callback) = frame.borrow().as_ref()
        {
            let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
        }
    }));
    let callback = next.borrow();
    window()?
        .request_animation_frame(
            callback
                .as_ref()
                .expect("frame callback installed")
                .as_ref()
                .unchecked_ref(),
        )
        .map_err(|_| "could not schedule first frame")?;
    Ok(())
}
