/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host-apparatus
//!
//! gpui renderer for the apparatus pane (the system inspector strip).
//! Owns the tracing capture infrastructure and the uxtree dump
//! rendering — anything else the apparatus pane will eventually
//! display (register-diagnostics taps, accesskit inspector,
//! profiler) lands here too.
//!
//! Companion to portable [`mere-apparatus`] (which projects an
//! a11y-shaped skeleton subtree).

#![doc(html_root_url = "https://docs.rs/mere-host-apparatus/0.0.1")]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use accesskit::{NodeId, Role};
use gpui::{AnyElement, div, prelude::*, px, rgb};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
use uxtree::UxTree;

const EVENT_BUFFER_CAPACITY: usize = 200;

/// Shared in-memory ring buffer of recently-captured tracing events.
pub type EventBuffer = Arc<Mutex<VecDeque<CapturedEvent>>>;

#[derive(Clone, Debug)]
pub struct CapturedEvent {
    pub level: tracing::Level,
    pub target: String,
    pub fields: String,
}

/// Install a tracing subscriber that:
///  1) writes to stderr via `tracing-subscriber::fmt`, honoring `RUST_LOG`
///     (default `debug`),
///  2) captures every event into the returned [`EventBuffer`] for the
///     apparatus pane to render.
///
/// Idempotent within a process — calling more than once is a no-op
/// (later calls return an empty buffer).
pub fn init_diagnostics() -> EventBuffer {
    let buffer: EventBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(EVENT_BUFFER_CAPACITY)));
    let capture = CaptureLayer {
        buffer: buffer.clone(),
    };
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(capture)
        .try_init();
    buffer
}

struct CaptureLayer {
    buffer: EventBuffer,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let captured = CapturedEvent {
            level: *metadata.level(),
            target: metadata.target().to_string(),
            fields: visitor.into_string(),
        };
        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= EVENT_BUFFER_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(captured);
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    other: Vec<(String, String)>,
}

impl FieldVisitor {
    fn into_string(self) -> String {
        let mut out = String::new();
        if let Some(msg) = self.message {
            out.push_str(&msg);
        }
        for (k, v) in self.other {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&k);
            out.push('=');
            out.push_str(&v);
        }
        out
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let formatted = format!("{value:?}");
        if field.name() == "message" {
            let stripped = formatted
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .map(|s| s.to_string())
                .unwrap_or(formatted);
            self.message = Some(stripped);
        } else {
            self.other.push((field.name().to_string(), formatted));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.other.push((field.name().to_string(), value.to_string()));
        }
    }
}

/// Render the apparatus pane's body — tracing events on top, application
/// uxtree dump below.
pub fn render(app_tree: &UxTree, events: &[CapturedEvent]) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(render_tracing_section(events))
        .child(render_uxtree_section(app_tree))
        .into_any_element()
}

fn render_tracing_section(events: &[CapturedEvent]) -> AnyElement {
    let rows: Vec<AnyElement> = events
        .iter()
        .rev()
        .take(40)
        .map(|ev| {
            let level_color = match ev.level {
                tracing::Level::ERROR => rgb(0xff8080),
                tracing::Level::WARN => rgb(0xffd080),
                tracing::Level::INFO => rgb(0xa0d0ff),
                tracing::Level::DEBUG => rgb(0xa0a0a0),
                tracing::Level::TRACE => rgb(0x707070),
            };
            div()
                .flex()
                .flex_row()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .w(px(48.0))
                        .text_color(level_color)
                        .child(ev.level.to_string()),
                )
                .child(
                    div()
                        .w(px(220.0))
                        .text_color(rgb(0x808080))
                        .child(ev.target.clone()),
                )
                .child(
                    div()
                        .text_color(rgb(0xd0d0d0))
                        .child(ev.fields.clone()),
                )
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x707070))
                .pb_1()
                .child(format!(
                    "tracing events (most recent first, {} captured)",
                    events.len()
                )),
        )
        .children(rows)
        .into_any_element()
}

fn render_uxtree_section(app_tree: &UxTree) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x707070))
                .pb_1()
                .child("application uxtree (live)"),
        )
        .child(render_uxtree_node(app_tree, app_tree.root, 0))
        .into_any_element()
}

/// Walk a uxtree from a node id, rendering each node as an indented
/// row. Pub-exposed because apparatus is the canonical home for
/// uxtree-dump rendering — other panes (or future inspector tools)
/// can reuse it.
pub fn render_uxtree_node(tree: &UxTree, node_id: NodeId, depth: usize) -> AnyElement {
    let Some((_, node)) = tree.nodes.iter().find(|(id, _)| *id == node_id) else {
        return div().into_any_element();
    };
    let indent = px((depth as f32) * 12.0);
    let role_label = format!("{:?}", node.role());
    let label = node.label().unwrap_or("").to_string();
    let descr = node.description();

    let header_color = match node.role() {
        Role::Window => rgb(0xffffff),
        Role::Group => rgb(0xb0d0ff),
        Role::Link => rgb(0xa0ffa0),
        Role::ListItem => rgb(0xffd0a0),
        _ => rgb(0xe0e0e0),
    };

    let mut row = div().flex().flex_col().child(
        div()
            .flex()
            .flex_row()
            .gap_2()
            .pl(indent)
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x707070))
                    .child(role_label),
            )
            .child(div().text_color(header_color).child(label)),
    );
    if let Some(d) = descr {
        row = row.child(
            div()
                .pl(px((depth as f32) * 12.0 + 16.0))
                .text_xs()
                .text_color(rgb(0x808080))
                .child(format!("({d})")),
        );
    }

    let children: Vec<NodeId> = node.children().to_vec();
    let row = row.children(
        children
            .into_iter()
            .map(|child_id| render_uxtree_node(tree, child_id, depth + 1)),
    );
    row.into_any_element()
}
