/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Thin-slice probe: parse a markdown string with `nematic::markdown`, then
//! render the resulting `inker::EngineDocument` in a Glass-HQ/gpui window.
//!
//! No parley, no PlatformSurface, no platen. Inline styling (emphasis,
//! strong, code, link) is flattened to plain text in v0; preserving it is
//! v0.5.

use gpui::{
    AnyElement, App, Bounds, Context, Render, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use gpui_platform::application;
use inker::{DocumentBlock, Engine, EngineDocument, EngineInput, InlineSpan};
use nematic::MarkdownEngine;

const SAMPLE: &str = r#"# Markdown probe

This is a portable `EngineDocument`, derived from a markdown source by
`nematic::markdown` and rendered through Glass-HQ/gpui. No platen, no
verso-tile, no PlatformSurface — just inker → gpui.

## What this probe is for

- Validate that an `EngineDocument` composes into gpui's element model
- Surface the parley question
- Stay decoupled from the rest of the mere stack

> "No layout coordinates here yet — gpui's text system does the work."

```rust
let engine = MarkdownEngine::new();
let document = engine.render(&input)?;
```

1. Parse markdown
2. Walk blocks
3. Render

---

End of sample.
"#;

struct DocumentView {
    document: EngineDocument,
}

impl Render for DocumentView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_8()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xe0e0e0))
            .text_sm()
            .children(self.document.blocks.iter().map(render_block))
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

        // Newer DocumentBlock variants (FeedHeader, FeedEntry, MetadataRow,
        // Badge) aren't produced by nematic.markdown for the sample doc;
        // a placeholder renderer keeps the probe compiling if the schema
        // grows further.
        other => div()
            .bg(rgb(0x331111))
            .text_color(rgb(0xffaaaa))
            .p_2()
            .rounded_sm()
            .child(format!("[unrendered: {other:?}]"))
            .into_any_element(),
    }
}

// v0: flatten inline spans to plain text. v0.5 should preserve emphasis /
// strong / code / link styling using gpui's text-style children instead of
// concatenating into a String.
fn flatten_inline(spans: &[InlineSpan]) -> String {
    let mut out = String::new();
    for span in spans {
        write_inline(span, &mut out);
    }
    out
}

fn write_inline(span: &InlineSpan, out: &mut String) {
    match span {
        InlineSpan::Text(s) => out.push_str(s),
        InlineSpan::Code(s) => {
            out.push('`');
            out.push_str(s);
            out.push('`');
        }
        InlineSpan::Emphasis(spans) | InlineSpan::Strong(spans) => {
            for s in spans {
                write_inline(s, out);
            }
        }
        InlineSpan::Link { spans, .. } => {
            for s in spans {
                write_inline(s, out);
            }
        }
        InlineSpan::LineBreak => out.push('\n'),
        InlineSpan::SoftBreak => out.push(' '),
    }
}

fn main() {
    // Default to `debug` so the inker/nematic spans surface; override with
    // `RUST_LOG` (e.g. `RUST_LOG=trace cargo run`).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .init();

    let engine = MarkdownEngine::new();
    let input = EngineInput::new("probe:markdown-v0", SAMPLE);
    let document = engine
        .render(&input)
        .expect("MarkdownEngine should not fail on the sample");

    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| DocumentView { document }),
        )
        .unwrap();
        cx.activate(true);
    });
}
