/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Add-tag: the host text-entry prompt for tagging the selected node(s).
//!
//! A context-menu "Add tag…" opens the prompt (`view.tagging`); keystrokes edit
//! the buffer (the `on_key_pressed` tagging branch); Enter inserts the typed tag
//! on every selected node via `mere::orrery::tag_selected` (which calls the kernel's
//! `Graph::insert_node_tag`) and persists; Escape cancels. Mirrors the switcher
//! rename's host-text path, kept here so the host god-files stay off the 600-LOC
//! ceiling.

use netrender::{Scene, ScenePath};
use register_theme::chrome::{ChromeTheme, Color32};

use super::WindowCtx;
use super::text::HostText;

/// Prompt font size (px).
const PROMPT_FONT: f32 = 14.0;

impl WindowCtx<'_> {
    /// Begin tagging: open the prompt with an empty buffer, but only when a node is
    /// selected (the tag lands on the orrery selection). A no-op otherwise.
    pub(super) fn start_tag(&mut self) {
        if self.orrery().selected_members().is_empty() {
            return;
        }
        self.view.tagging = Some(String::new());
        self.view.request_redraw();
    }

    /// Commit the in-progress tag: insert the (trimmed, non-empty) buffer on every
    /// selected node and persist when anything changed. (Add-tag.)
    pub(super) fn commit_tag(&mut self) {
        let Some(buf) = self.view.tagging.take() else {
            return;
        };
        if self.orrery_mut().tag_selected(&buf) > 0 {
            self.save_session();
        }
        self.view.request_redraw();
    }

    /// Drop the in-progress tag without applying it (Escape). (Add-tag.)
    pub(super) fn cancel_tag(&mut self) {
        if self.view.tagging.take().is_some() {
            self.view.request_redraw();
        }
    }

    /// Append typed `ch` to the tag buffer. No-op when not tagging. (Add-tag.)
    pub(super) fn tag_push(&mut self, ch: &str) {
        if let Some(buf) = self.view.tagging.as_mut() {
            buf.push_str(ch);
            self.view.request_redraw();
        }
    }

    /// Delete the last char of the tag buffer (Backspace). (Add-tag.)
    pub(super) fn tag_backspace(&mut self) {
        if let Some(buf) = self.view.tagging.as_mut() {
            buf.pop();
            self.view.request_redraw();
        }
    }
}

/// A chrome token at `alpha` as premultiplied `[r, g, b, a]` (matches switcher).
fn rgba(c: Color32, alpha: f32) -> [f32; 4] {
    let [r, g, b, _] = c.to_array();
    [
        r as f32 / 255.0 * alpha,
        g as f32 / 255.0 * alpha,
        b as f32 / 255.0 * alpha,
        alpha,
    ]
}

fn stroke(scene: &mut Scene, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4], width: f32) {
    let mut path = ScenePath::new();
    path.move_to(x0, y0).line_to(x1, y1);
    scene.push_shape_stroked(path, color, width);
}

/// Build the add-tag prompt scene: a bordered panel with an "Add tag:" label, the
/// live buffer, and a caret. `text` shapes the glyphs into the scene. The host
/// composites this into a small region over the content band. (Add-tag.)
pub(super) fn tag_prompt_scene(
    buf: &str,
    w: u32,
    h: u32,
    theme: &ChromeTheme,
    text: &mut HostText,
) -> Scene {
    let mut scene = Scene::new(w.max(1), h.max(1));
    let wf = w as f32;
    let hf = h as f32;
    // Panel + a thin border (four edges).
    scene.push_rect(0.0, 0.0, wf, hf, rgba(theme.active_bg, 0.98));
    let edge = rgba(theme.muted_text, 0.6);
    stroke(&mut scene, 0.5, 0.5, wf - 0.5, 0.5, edge, 1.0);
    stroke(&mut scene, wf - 0.5, 0.5, wf - 0.5, hf - 0.5, edge, 1.0);
    stroke(&mut scene, wf - 0.5, hf - 0.5, 0.5, hf - 0.5, edge, 1.0);
    stroke(&mut scene, 0.5, hf - 0.5, 0.5, 0.5, edge, 1.0);

    let pad = 10.0;
    let label = "Add tag:";
    let (label_w, label_h) = text.measure(label, PROMPT_FONT);
    let label_y = (hf - label_h) * 0.5;
    text.push_line(
        &mut scene,
        label,
        PROMPT_FONT,
        rgba(theme.muted_text, 1.0),
        [pad, label_y],
    );

    let bx = pad + label_w + 8.0;
    let max_w = (wf - bx - pad).max(1.0);
    let shown = fit_tail(text, buf, max_w);
    let mut caret_x = bx;
    if !shown.is_empty() {
        let (bw, bh) = text.measure(&shown, PROMPT_FONT);
        let by = (hf - bh) * 0.5;
        text.push_line(
            &mut scene,
            &shown,
            PROMPT_FONT,
            rgba(theme.strong_text, 1.0),
            [bx, by],
        );
        caret_x = bx + bw + 1.0;
    }
    stroke(
        &mut scene,
        caret_x,
        hf * 0.28,
        caret_x,
        hf * 0.72,
        rgba(theme.strong_text, 1.0),
        1.2,
    );
    scene
}

/// Trim `buf` from the front until it fits `max_w` at the prompt font, so the
/// caret end stays visible as the user types.
fn fit_tail(text: &mut HostText, buf: &str, max_w: f32) -> String {
    if buf.is_empty() || max_w <= 0.0 {
        return String::new();
    }
    if text.measure(buf, PROMPT_FONT).0 <= max_w {
        return buf.to_string();
    }
    let mut chars: Vec<char> = buf.chars().collect();
    while chars.len() > 1 {
        chars.remove(0);
        let candidate: String = chars.iter().collect();
        if text.measure(&candidate, PROMPT_FONT).0 <= max_w {
            return candidate;
        }
    }
    chars.into_iter().collect()
}
