/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host
//!
//! gpui host crate for the Mere browser. See README.md for the full role
//! description and v0 scope.
//!
//! v0 surface area:
//! - [`init_diagnostics`] — install the tracing subscriber.
//! - [`run`] — bootstrap a gpui application with a single window
//!   rendering a hardcoded `EngineDocument`. Returns when the user
//!   closes the window.
//!
//! Subsequent rounds add `accesskit_winit` integration, mere-workbench
//! projection, and `PlatformSurface` impls.

use accesskit::{Node, NodeId, Role};
use euclid::default::Point2D;
use gpui::{
    AnyElement, App, Bounds, Context, Render, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, relative, rgb, size,
};
use gpui_platform::application;
use inker::{DocumentBlock, Engine, EngineDocument, EngineInput, InlineSpan};
use mere_frame::{FrameLayout, PaneContent, PaneNode, SplitAxis};
use mere_kernel::graph::{Graph, NodeKey};
use mere_kernel::pane::PaneId;
use nematic::MarkdownEngine;
use platen::{FrameId, ProjectedPane, WorkbenchProjection};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
use uxtree::UxTree;

const DEMO_MARKDOWN: &str = r#"# mere-host v0

This window is opened by `mere-host::run()`. The host bootstraps gpui,
parses an `EngineDocument` via `nematic::markdown`, and renders blocks
using gpui's element tree. Inline span styling is flattened in v0.

## Next steps for the host

- `accesskit_winit` integration consuming `uxtree` subtrees
- `mere-workbench` projection rendered alongside this document
- `tracing-subscriber` → `register-diagnostics` bridge
- `PlatformSurface` impls for Windows / Linux
"#;

/// In-memory ring buffer of recently-captured tracing events. Shared
/// between the tracing subscriber's capture layer and the host's
/// apparatus pane. Bounded so long-running sessions don't grow it
/// unbounded.
pub type EventBuffer = Arc<Mutex<VecDeque<CapturedEvent>>>;

const EVENT_BUFFER_CAPACITY: usize = 200;

/// One tracing event, captured at emit time.
#[derive(Clone, Debug)]
pub struct CapturedEvent {
    pub level: tracing::Level,
    pub target: String,
    pub fields: String,
}

/// Custom tracing-subscriber layer that pushes formatted event metadata
/// onto the shared [`EventBuffer`]. Pairs with the regular fmt layer:
/// fmt writes to stderr for human reading, this captures for the
/// in-app apparatus pane.
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
            // Strip surrounding quotes from Debug-formatted strings so
            // the buffer entry reads like the original message text.
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

/// Install the host's tracing subscriber. Honors `RUST_LOG`; defaults to
/// `debug` so portable-crate spans (inker, nematic, platen, mere-identity,
/// mere-transport, graph-canvas) surface during development.
///
/// Returns a shared [`EventBuffer`] that the apparatus pane reads from.
/// Idempotent within a process — calling more than once is a no-op
/// (later calls log a warning and return an empty buffer).
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

/// Bootstrap a gpui application and open the v0 demo window.
pub fn run() {
    let event_buffer = init_diagnostics();
    let document = render_demo_document();
    let graph = render_demo_graph_state();
    let frame_layout = render_demo_frame_layout();
    let app_tree = build_demo_application_tree(&document, &graph, &frame_layout);
    open_window(document, graph, frame_layout, app_tree, event_buffer);
}

/// Project the frame layout, attaching each mere-domain subtree
/// (workbench / orrery / gloss / apparatus) at its corresponding leaf,
/// then stitch the result under a single application-root node.
fn build_demo_application_tree(
    document: &EngineDocument,
    graph: &Graph,
    layout: &FrameLayout,
) -> UxTree {
    let frame_tree = mere_frame::project_frame_with(layout, |content, _pane_id| match content {
        PaneContent::Workbench => Some(render_demo_workbench()),
        PaneContent::Orrery => Some(mere_orrery::project_graph(graph)),
        PaneContent::Gloss => Some(mere_gloss::project_outline(document)),
        PaneContent::Apparatus => Some(mere_apparatus::project_skeleton()),
        _ => None,
    });

    let mut app_root = Node::new(Role::Window);
    app_root.set_label("mere-host (demo)");
    uxtree::stitch("application", app_root, vec![frame_tree])
}

/// Hardcoded 4-quadrant frame layout: workbench top-left, gloss
/// bottom-left, orrery top-right, apparatus bottom-right.
fn render_demo_frame_layout() -> mere_frame::FrameLayout {
    use mere_frame::{FrameId, FrameLayout, PaneContent, PaneId, PaneNode, SplitAxis};
    FrameLayout {
        id: FrameId::new("reading"),
        label: "Reading".to_string(),
        root: PaneNode::Split {
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(1),
                    content: PaneContent::Workbench,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(2),
                    content: PaneContent::Gloss,
                }),
            }),
            second: Box::new(PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(3),
                    content: PaneContent::Orrery,
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(4),
                    content: PaneContent::Apparatus,
                }),
            }),
        },
    }
}


fn render_demo_document() -> EngineDocument {
    let engine = MarkdownEngine::new();
    let input = EngineInput::new("mere-host:demo", DEMO_MARKDOWN);
    engine
        .render(&input)
        .expect("MarkdownEngine should not fail on the demo doc")
}

fn render_demo_workbench() -> UxTree {
    // Hardcoded fixture so the host can demonstrate the full projection
    // chain without needing a real reducer. Replaced by live state once
    // the host wires platen's selectors.
    let projection = WorkbenchProjection {
        active_frame: Some(FrameId::new("frame-demo")),
        frame_label: Some("Reading".to_string()),
        root_view: None,
        panes: vec![
            ProjectedPane {
                pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(0xa1)),
                node: NodeKey::default(),
                surface_host: None,
                is_primary: true,
            },
            ProjectedPane {
                pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(0xa2)),
                node: NodeKey::default(),
                surface_host: None,
                is_primary: false,
            },
            ProjectedPane {
                pane_id: PaneId::from_uuid(uuid::Uuid::from_u128(0xa3)),
                node: NodeKey::default(),
                surface_host: None,
                is_primary: false,
            },
        ],
    };
    mere_workbench::project_workbench(&projection)
}

/// Hardcoded demo graph: three nodes with stable UUIDs so the projected
/// uxtree IDs are deterministic across runs.
fn render_demo_graph_state() -> Graph {
    let mut graph = Graph::new();
    graph.add_node_with_id(
        uuid::Uuid::from_u128(0xb1),
        "https://mere.test/intro".to_string(),
        Point2D::new(0.0, 0.0),
    );
    graph.add_node_with_id(
        uuid::Uuid::from_u128(0xb2),
        "https://mere.test/architecture".to_string(),
        Point2D::new(20.0, 0.0),
    );
    graph.add_node_with_id(
        uuid::Uuid::from_u128(0xb3),
        "https://mere.test/probes".to_string(),
        Point2D::new(40.0, 0.0),
    );
    graph
}

fn open_window(
    document: EngineDocument,
    graph: Graph,
    frame_layout: FrameLayout,
    app_tree: UxTree,
    event_buffer: EventBuffer,
) {
    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| HostRoot {
                    document,
                    graph,
                    frame_layout,
                    app_tree,
                    event_buffer,
                })
            },
        )
        .expect("open_window failed");
        cx.activate(true);
    });
}

/// The host's root entity: holds the demo document and the projected
/// workbench tree. v0.5: real platen selectors replace the hardcoded
/// projection.
struct HostRoot {
    document: EngineDocument,
    graph: Graph,
    frame_layout: FrameLayout,
    app_tree: UxTree,
    event_buffer: EventBuffer,
}

impl Render for HostRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Snapshot tracing events at render time. Apparatus refreshes
        // when the entity re-renders (which doesn't happen automatically
        // — a future round can add a periodic timer or push-on-event).
        let events: Vec<CapturedEvent> = self
            .event_buffer
            .lock()
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xe0e0e0))
            .text_sm()
            .child(render_title_bar(&self.frame_layout))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(render_pane_node(
                        &self.frame_layout.root,
                        &self.document,
                        &self.graph,
                        &self.app_tree,
                        &events,
                    )),
            )
    }
}

fn render_title_bar(layout: &FrameLayout) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_2()
        .bg(rgb(0x141414))
        .border_b_1()
        .border_color(rgb(0x2a2a2a))
        .child(
            div()
                .text_color(rgb(0xe0e0e0))
                .child(format!("mere-host (demo) — {} frame", layout.label)),
        )
        .into_any_element()
}

/// Recursively render a [`PaneNode`]: splits become flex containers
/// honoring the ratio + axis; leaves dispatch on `PaneContent` to the
/// matching domain renderer.
fn render_pane_node(
    node: &PaneNode,
    document: &EngineDocument,
    graph: &Graph,
    app_tree: &UxTree,
    events: &[CapturedEvent],
) -> AnyElement {
    match node {
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let first_child = render_pane_node(first, document, graph, app_tree, events);
            let second_child = render_pane_node(second, document, graph, app_tree, events);
            let first_basis = relative(*ratio);
            let second_basis = relative(1.0 - *ratio);
            match axis {
                SplitAxis::Horizontal => div()
                    .flex()
                    .flex_row()
                    .size_full()
                    .child(div().h_full().flex_basis(first_basis).child(first_child))
                    .child(div().h_full().flex_basis(second_basis).child(second_child))
                    .into_any_element(),
                SplitAxis::Vertical => div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(div().w_full().flex_basis(first_basis).child(first_child))
                    .child(div().w_full().flex_basis(second_basis).child(second_child))
                    .into_any_element(),
            }
        }
        PaneNode::Leaf { content, .. } => render_leaf(content, document, graph, app_tree, events),
    }
}

/// Wrap a leaf's content in a chrome (border + tag header + scrollable body).
fn render_leaf(
    content: &PaneContent,
    document: &EngineDocument,
    graph: &Graph,
    app_tree: &UxTree,
    events: &[CapturedEvent],
) -> AnyElement {
    let body = match content {
        PaneContent::Workbench => render_document_inner(document),
        PaneContent::Orrery => render_graph_inner(graph),
        PaneContent::Gloss => render_gloss_inner(document),
        PaneContent::Apparatus => render_apparatus_inner(app_tree, events),
        _ => div()
            .text_color(rgb(0x707070))
            .child("(empty pane)")
            .into_any_element(),
    };
    let scroll_id: gpui::ElementId = gpui::SharedString::from(format!("pane-scroll-{}", content.tag())).into();
    div()
        .flex()
        .flex_col()
        .size_full()
        .border_1()
        .border_color(rgb(0x2a2a2a))
        .child(
            div()
                .px_3()
                .py_1()
                .text_xs()
                .text_color(rgb(0x808080))
                .bg(rgb(0x141414))
                .border_b_1()
                .border_color(rgb(0x2a2a2a))
                .child(content.tag().to_string()),
        )
        .child(
            div()
                .id(scroll_id)
                .flex_1()
                .overflow_y_scroll()
                .p_3()
                .child(body),
        )
        .into_any_element()
}

fn render_document_inner(document: &EngineDocument) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .children(document.blocks.iter().map(render_block))
        .into_any_element()
}

/// v0 graph view: cards listing each node with title + address. Real
/// graph-canvas viz (camera / scene / hit-testing) lands later.
fn render_graph_inner(graph: &Graph) -> AnyElement {
    let cards: Vec<AnyElement> = graph
        .nodes()
        .map(|(_, node)| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_3()
                .rounded_md()
                .bg(rgb(0x252525))
                .border_1()
                .border_color(rgb(0x3a3a3a))
                .child(
                    div()
                        .text_color(rgb(0xffffff))
                        .child(node.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xa0ffa0))
                        .child(node.address.as_url_str().to_string()),
                )
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .children(cards)
        .into_any_element()
}

fn render_gloss_inner(document: &EngineDocument) -> AnyElement {
    let outline_tree = mere_gloss::project_outline(document);
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(render_uxtree_node(&outline_tree, outline_tree.root, 0))
        .into_any_element()
}

fn render_apparatus_inner(app_tree: &UxTree, events: &[CapturedEvent]) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(render_apparatus_section_tracing(events))
        .child(render_apparatus_section_uxtree(app_tree))
        .into_any_element()
}

fn render_apparatus_section_tracing(events: &[CapturedEvent]) -> AnyElement {
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

fn render_apparatus_section_uxtree(app_tree: &UxTree) -> AnyElement {
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


/// Walk a uxtree from a node id, rendering each node as an indented row.
/// Each row shows the role + label so the projection structure is visible.
fn render_uxtree_node(tree: &UxTree, node_id: NodeId, depth: usize) -> AnyElement {
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

    let mut row = div()
        .flex()
        .flex_col()
        .child(
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
                .child(
                    div()
                        .text_color(header_color)
                        .child(label),
                ),
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
    let child_count = children.len();
    let row = row.children(
        children
            .into_iter()
            .map(|child_id| render_uxtree_node(tree, child_id, depth + 1)),
    );

    if child_count == 0 && depth > 0 {
        row.into_any_element()
    } else {
        row.into_any_element()
    }
}

fn render_block(block: &DocumentBlock) -> AnyElement {
    match block {
        DocumentBlock::Heading { level, spans } => div()
            .when(*level == 1, |d| d.text_2xl())
            .when(*level == 2, |d| d.text_xl())
            .when(*level >= 3, |d| d.text_lg())
            .text_color(rgb(0xffffff))
            .child(flatten_inline(spans))
            .into_any_element(),

        DocumentBlock::Paragraph { spans } => {
            div().child(flatten_inline(spans)).into_any_element()
        }

        DocumentBlock::CodeBlock { text, .. } => div()
            .bg(rgb(0x2a2a2a))
            .text_color(rgb(0xb0d0ff))
            .p_2()
            .rounded_sm()
            .child(text.clone())
            .into_any_element(),

        DocumentBlock::Quote { blocks } => div()
            .pl_4()
            .border_l_2()
            .border_color(rgb(0x555555))
            .text_color(rgb(0xb0b0b0))
            .flex()
            .flex_col()
            .gap_2()
            .children(blocks.iter().map(render_block))
            .into_any_element(),

        DocumentBlock::List { ordered, items } => div()
            .flex()
            .flex_col()
            .gap_1()
            .children(items.iter().enumerate().map(|(i, item_blocks)| {
                let prefix = if *ordered {
                    format!("{}.", i + 1)
                } else {
                    "•".to_string()
                };
                div()
                    .flex()
                    .gap_2()
                    .child(div().w(px(20.0)).child(prefix))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap_1()
                            .children(item_blocks.iter().map(render_block)),
                    )
                    .into_any_element()
            }))
            .into_any_element(),

        DocumentBlock::Image { alt, url } => div()
            .text_color(rgb(0x888888))
            .child(format!("[image: {alt} ({url})]"))
            .into_any_element(),

        DocumentBlock::Preformatted { text } => div()
            .bg(rgb(0x2a2a2a))
            .p_2()
            .rounded_sm()
            .child(text.clone())
            .into_any_element(),

        DocumentBlock::Rule => div().h(px(1.0)).bg(rgb(0x444444)).into_any_element(),

        // Variants outside markdown's range (FeedHeader/FeedEntry/
        // MetadataRow/Badge from feed engines) get a placeholder. v0
        // doesn't render feed content; subsequent rounds add real
        // styling per variant when a feed engine probe lands.
        other => div()
            .bg(rgb(0x331111))
            .text_color(rgb(0xffaaaa))
            .p_2()
            .rounded_sm()
            .child(format!("[unrendered block variant: {other:?}]"))
            .into_any_element(),
    }
}

fn flatten_inline(spans: &[InlineSpan]) -> String {
    inker::inline_text(spans)
}
