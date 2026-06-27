/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `ApplicationHandler` impl for [`Shell`](super::Shell). Factored from
//! `main.rs` to keep files under the workspace 600-LOC ceiling.

use std::sync::Arc;

use netrender::NetrenderOptions;
use orrery::WHEEL_PAN_SCALE;
use serval_winit_host::{RenderCore, modifiers_from_winit};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CursorIcon, Window, WindowId};

use super::observability::Severity;
use super::{Shell, comms_host, fetch, scrying_host, sync, titlebar};

/// The favicon URL to fetch for a freshly-loaded HTML page: the resolved
/// `<link rel="icon">` href if the head declares one, else the well-known
/// `{origin}/favicon.ico` for an http(s) page. `None` for a page with neither (a
/// non-http(s) scheme with no icon link, e.g. gemtext). A lightweight parse of the
/// already-fetched body, reusing serval's `<link>` scan. (Favicon-on-tile.)
pub(crate) fn favicon_url_for(page_url: &str, body: &str) -> Option<String> {
    let base = url::Url::parse(page_url).ok()?;
    let doc = serval_static_dom::StaticDocument::parse(body);
    if let Some(href) = serval_layout::linked_icon_href(&doc) {
        if let Ok(resolved) = base.join(&href) {
            return Some(resolved.to_string());
        }
    }
    // No declared icon: fall back to the well-known location for web pages only.
    if matches!(base.scheme(), "http" | "https") {
        if let Ok(fallback) = base.join("/favicon.ico") {
            return Some(fallback.to_string());
        }
    }
    None
}

/// The platform virtual-key code for a winit key event, for forwarding into a
/// scrying tile's WebView. Named keys map to their Win32 VKs; character keys use
/// the uppercased char (matching Win32 VK_A..VK_Z / VK_0..VK_9). (Scrying X2.)
fn scrying_vk(event: &winit::event::KeyEvent) -> u32 {
    use winit::keyboard::{Key, NamedKey};
    match &event.logical_key {
        Key::Named(n) => match n {
            NamedKey::Enter => 0x0D,
            NamedKey::Tab => 0x09,
            NamedKey::Backspace => 0x08,
            NamedKey::Escape => 0x1B,
            NamedKey::Space => 0x20,
            NamedKey::Delete => 0x2E,
            NamedKey::ArrowLeft => 0x25,
            NamedKey::ArrowUp => 0x26,
            NamedKey::ArrowRight => 0x27,
            NamedKey::ArrowDown => 0x28,
            NamedKey::Home => 0x24,
            NamedKey::End => 0x23,
            NamedKey::PageUp => 0x21,
            NamedKey::PageDown => 0x22,
            _ => 0,
        },
        Key::Character(s) => s
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase() as u32)
            .unwrap_or(0),
        _ => 0,
    }
}

/// Apply one comms actor update to a window's chrome — the per-window half of the
/// MW3 step-5 fan-out. Mirrors the inline mutations the primary's actor-drain does, so a
/// chrome-bearing secondary stays in sync with the primary. (MW3 step 5.)
fn apply_comms_to_chrome(
    view: &mut super::window_view::WindowView,
    update: &comms_host::CommsUpdate,
) {
    match update {
        comms_host::CommsUpdate::Inbox(inbox) => {
            view.chrome_update(|c| c.comms.set_inbox(inbox.clone()));
        }
        comms_host::CommsUpdate::Thread(id, messages) => {
            view.chrome_update(|c| {
                if c.comms.selected() == Some(id) {
                    c.comms.set_thread(messages.clone());
                }
            });
        }
        comms_host::CommsUpdate::Sent(_) => {
            view.chrome_update(|c| c.clear_comms_draft());
        }
        comms_host::CommsUpdate::SendOutcome(line) => {
            view.chrome_update(|c| c.comms.set_send_status(line.clone()));
        }
        comms_host::CommsUpdate::Identity {
            misfin_address,
            cabal_ticket,
        } => {
            view.chrome_update(|c| {
                c.comms
                    .set_identity(misfin_address.clone(), cabal_ticket.clone())
            });
        }
    }
}


mod handler;
mod handler_user;
mod handler_window;
mod shell_ops;
mod window_ctx;
