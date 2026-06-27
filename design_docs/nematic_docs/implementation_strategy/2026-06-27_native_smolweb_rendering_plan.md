# Native Smolweb Rendering Plan — each format idiomatic, on serval views

**Date**: 2026-06-27
**Status**: planning (with Mark). Net-new architecture; greenfield on the serval
side (no existing smolweb render in serval or pelt, code-verified).

**Thesis**: render every smolweb format natively and idiomatically, the way
[Lagrange](https://gmi.skyjake.fi/lagrange/) (gemtext typesetter, SDL) and
[Geopard](https://github.com/ranfdev/Geopard) (gemtext into GTK/adwaita widgets)
do, rather than flattening everything into one document model first. Lower a
format into the gemtext view only where the line model genuinely fits; give the
rest their own view. `DocumentBlock` stops being the universal renderer and keeps
the two jobs it is actually good at: knot **capture** and uniform **cards**.

**Relation to the djot plan**: this is the smolweb half of the same conclusion the
[djot editor plan](../../mere_docs/implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md)
reached for notes (its 2026-06-27 reframe): build the view tree directly as
`xilem_serval` views, no serialize-to-HTML round-trip, source-as-truth for edit.
The djot reframe renders `EngineDocument` → `xilem_serval` views → `ScriptedDom`
→ serval-layout → netrender. This plan renders the smolweb AST → `xilem_serval`
views through the same tail. Same render target, different native intermediate.

---

## 1. The two-family model

Both families end at `xilem_serval` views, painted by serval-layout + netrender,
the same path the host chrome and the djot note tile already use every frame. They
differ only in their native intermediate model, and that difference is what decides
where the code lives and whether pelt can share it.

| Family | Native model | Renderer | Lives in | Shared with pelt? |
| --- | --- | --- | --- | --- |
| **Document** (djot/knot, markdown, reader-HTML) | `DocumentBlock` | one block→view mapper | mere | no (imports `DocumentBlock`) |
| **Smolweb** (gemtext, gopher, feed, scroll, misfin) | per-format AST | per-format native views | serval | yes (avoids `DocumentBlock`) |

The smolweb views are serval-resident and shared with pelt *because* they avoid
`DocumentBlock`. The djot/markdown mapper is mere-resident *because* it embraces
`DocumentBlock`. That single line is what keeps pelt able to render the smolweb
half without dragging in the mere graph kernel (the original spin-out problem:
`nematic → inker → kernel → forme`).

### Where each format lands

**Lower into `gemtext_view`** (the line model fits cleanly):

- **nex** — directory links + text.
- **finger** — plain profile text (or a trivial text view).
- **spartan** — body is `text/gemini`; add the `=:` prompt-upload as an input-link affordance.
- **guppy** — gemtext body; only the transport differs (errand's UDP path).

**Own native view** (gemtext lowering loses the point):

- **gopher** — `gopher_view`: item-type-aware menu (icon per type: text/dir/search/image/binary/telnet), inline search prompt for type 7. Type-0 text files fall back to a plain/pre view.
- **feed (RSS/Atom)** — `feed_view`: a subscription / article-list idiom (entry cards, dates, source), not a wall of links. Detail in §3.
- **scroll** — `scroll_view`: gemtext body plus Scroll's ring-nav chrome (prev / next / up, author/metadata).
- **misfin** — `message_view`: gemini-based mail, so a message idiom, not a document.

**Out of scope here**: titan (a write/upload action, no render), markdown and djot
(document family, §2), HTML (serval proper), live web (scry surface frames).

---

## 2. Markdown rides the djot mapper (no new view)

Markdown is the format that proves the document family: it does not get a bespoke
view, because `DocumentBlock` is already markdown-shaped. It collapses into the
djot mapper.

```rust
// in mere (the mapper imports DocumentBlock, so it is host-coupled by design)
fn markdown_tile(bytes: &[u8]) -> impl SerView {
    let doc = nematic::markdown::parse(bytes);   // pulldown-cmark today; jotdown after migration
    block_to_view(&doc)                          // THE SAME mapper djot/knot uses
}
```

`markdown::parse` and `djot::parse` both produce `DocumentBlock`, so they share one
mapper. Markdown's inline richness (emphasis, inline code, links inside prose,
nested lists) is handled by the mapper's `InlineSpan` rendering, which gemtext's
flat line model never needed. That is the real reason gemtext earns its own view
and markdown does not: gemtext has no inline cascade, markdown does, and the mapper
already speaks inline.

**Prerequisite — `DocumentBlock::Table`**: the block enum
([crates/inker/src/document.rs](../../../crates/inker/src/document.rs)) has no
table variant (Heading/Paragraph/CodeBlock/Quote/List/Image/Preformatted/Rule/
Feed\*/MetadataRow/Badge/Link/breaks). Both djot and markdown have tables, and both
ride the same mapper, so `DocumentBlock::Table` is a shared prerequisite of the
whole document family, not a sub-step of either consumer. Adding the variant
touches the mapper, the round-trip exporters
([render.rs](../../../crates/inker/src/document/render.rs) /
[render/export.rs](../../../crates/inker/src/document/render/export.rs)), and every
exhaustive match on `DocumentBlock`, so it lands as its own change **before** either
the djot note tile or the markdown tile renders. Owned by the djot plan;
sequenced ahead of its mapper here as the Table prerequisite.

---

## 3. The feed view (genuinely bespoke)

Feed is unique the other way from markdown: it refuses the document model. A feed
is a subscription of entries, not a body of blocks.

```rust
// in serval/smolweb-views (imports errand/parse + xilem_serval; NO DocumentBlock)
pub fn feed_view(feed: &Feed, theme: &Palette) -> impl SerView {
    flex_col((
        feed_header(feed, theme),     // title, site link, description, unread count, Subscribe action
        flex_col(feed.entries.iter().map(|e| feed_entry_card(e, theme))),
    ))
}

fn feed_entry_card(e: &Entry, theme: &Palette) -> impl SerView {
    focusable(flex_col((
        row((title_link(&e.title, &e.url), relative_date(e.published))),
        e.summary.as_deref().map(|s| summary_snippet(s, theme)),
        source_badge(&e.source),
    )))
    .on_action(|cx| cx.navigate(&e.url))   // opening an entry fetches + renders its own format
}
```

What makes feed its own thing:

- **It is a list, not a document.** Entry cards with title-as-link, relative date, summary snippet, source badge. Arrowable rows, the reader feel, not a flat stack of link lines.
- **Subscribe is an action, not a render.** A feed is the one smolweb surface you do not just read, you subscribe to. The header's Subscribe button does a real thing: add the feed to a subscription store and poll it, with genuine state (last-fetched, new-since). Backed by a real refresh, never a placebo spinner.
- **It straddles web and smolweb.** RSS/Atom arrive over http(s) too. `feed_view` is scoped to structured RSS/Atom (smolweb-parse's `quick-xml` feed parser). The gemtext "gemfeed" convention (date-prefixed link lines) is just gemtext, so it renders through `gemtext_view`; feed-detection there is a separate concern.
- **Opening an entry recurses into the family.** The entry URL may be gemini, http, or gopher, so `navigate` hands off to whichever view that scheme resolves to. Feed is a hub into the other renderers.

---

## 4. The gemtext view (the family base)

```rust
// in serval/smolweb-views (depends on xilem-serval + errand/parse + tinct)
pub fn gemtext_view(doc: &Gemtext, theme: &Palette) -> impl SerView {
    flex_col(doc.lines.iter().map(|line| match line {
        GemLine::Heading { level, text } => heading(*level, text, theme),
        GemLine::Text(t)                 => paragraph(t, theme),
        GemLine::Link { url, label }     => focusable(link_line(url, label, theme))
                                              .on_action(|cx| cx.navigate(url)),
        GemLine::Item(t)                 => list_item(t, theme),
        GemLine::Quote(t)                => blockquote(t, theme),
        GemLine::Pre { alt, text }       => preformatted(alt, text, theme),
    }))
}
```

The link line is the whole reason to go native: a `focusable` serval widget with
its own hover/focus/visited styling and a navigate action, the way Geopard makes
every link a row you arrow through. illume can run its entity pass inside paragraph
text (URLs, mentions lighting up), so prose gets the same treatment as the rest of
the host.

This mirrors the illume pattern Mark already ships
([illume plan](../../mere_docs/implementation_strategy/2026-06-26_illume_text_lexer_plan.md)):
portable core + tinct palette + serval renderer + host bridge. Smolweb is the same
four tiers — errand transport (source), errand/parse (parse), tinct (palette),
serval views (render) — with the host picking the view.

---

## 5. Crate homes and dependency direction (the one rule)

```text
errand  (sibling crate, crates.io-shaped: the smolweb crate)
   ├── transport (default)       bytes in/out  ──► pelt fetch, meerkat, murm/misfin
   └── parse (feature)           bytes -> per-format AST
         ├── serval/smolweb-views  (gemtext / gopher / feed / scroll / misfin views)
         │       └── consumed by BOTH pelt and mere (no DocumentBlock, so portable)
         └── mere/nematic          (AST -> DocumentBlock: capture + cards only)

inker DocumentBlock
   └── mere block->view mapper  (djot / markdown / reader-HTML)
           └── consumed by mere only (imports DocumentBlock, host-coupled by design)
```

Nothing in serval depends on mere. nematic shrinks from "owns parse-and-render" to
a lowering (AST → DocumentBlock) on the capture path.

**Why parse folds into errand (not a separate sibling).** errand already encodes
each protocol's transport structure; the parse is the document structure of the
same protocols, so they evolve in lockstep (add a protocol, add both) — exactly the
two-repos-in-lockstep friction the bundle-when-lockstep rule avoids. Transport and
parse are almost always paired (fetch a capsule, then render it), so one dep gives a
forker "everything smolweb, host-agnostic." The `parse` feature keeps transport pure
(murm/misfin/meerkat stay on a parser-free default; `quick-xml` rides the gate for
feeds only; gemtext/gopher/nex need no new deps). serval/smolweb-views depending on
`errand` with the parse feature is the same public sibling pelt already pulls for
transport, so the one-way direction holds.

**Markdown stays home.** CommonMark is not smolweb, so it does not join errand. Its
portable parse is essentially `pulldown-cmark` already; only the `DocumentBlock`
lowering is mere-specific, so it stays in nematic. There is no markdown crate to
spin out.

### DocumentBlock's settled role

It stops being the universal renderer and keeps two jobs:

- **Capture**: clip from any native view → lower into `DocumentBlock` → store in a knot. The smolweb AST feeds both the native view and this lowering, so they never diverge.
- **Cards / previews / orrery nodes**: uniform thumbnails across formats, where a capsule, an article, and a note should look alike.

Focused viewing goes native. Cards and clips stay `DocumentBlock`. Per-surface, not
all-or-nothing.

---

## Findings (2026-06-27, verified against code)

- **nematic cannot spin out as-is.** `nematic/Cargo.toml` has `inker.workspace = true`; every engine file imports `inker::{Engine, DocumentBlock, EngineRegistry, routing}`. Cone: `nematic → inker → kernel (graph-kernel) → forme, node-lineage, petgraph, rkyv, accesskit`. Spinning it out would invert the one-way rule (mere consumes serval, not the reverse).
- **`DocumentBlock` is not a smolweb model.** inker has two dispatch paths ([surface_engine.rs](../../../crates/inker/src/surface_engine.rs) comment): document registry for `nematic.*` *and* `serval.web`; surface registry for `scrying.web`. Static HTML maps into `DocumentBlock` too. It is mere's universal semantic-document model, the knot/reader/card substrate.
- **`DocumentBlock` is the live render model today.** `platen/document_scene.rs` paints blocks; `meerkat/card`, `gloss`, `forme/uxtree` consume them. `inker/src/document/render.rs` + `render/export.rs` round-trip blocks back to markdown / gemtext / gophermap / knot (capture path).
- **errand is already the shared transport** (sibling repo, crates.io-shaped, deps `url`/`tokio`/`rustls`/`ring`). Consumed by `meerkat`, `murm/misfin`. Serval has zero references to it.
- **pelt's fetch seam exists and is documented for this.** `pelt-core::ResourceFetcher` (`fn fetch(&url) -> Option<Vec<u8>>`); `LocalFetcher` dispatches by scheme (data / http(s) via netfetcher / file). errand slots in as a smolweb-scheme branch.
- **serval is the shared render layer.** xilem-serval is the Xilem view layer over serval (`html_qual`, `focusable`, `styled_field`, `text`); the illume plan already renders through `xilem_serval::styled_textarea`. So native smolweb views on serval primitives serve pelt and mere alike.
- **Greenfield.** No `gemtext`/`gopher`/`smolweb` render exists in serval/components or serval/ports.

---

## Phases

Built so each phase stands alone and the early ones are serval-local and small.

- **A — pelt → errand (transport).** Add `errand` to `pelt-desktop` behind a `smolweb` feature; add `smolweb_get_bytes` beside `http_get_bytes` (reuse the existing `runtime().block_on` bridge); branch `LocalFetcher::fetch` on `errand::Scheme::parse` for gemini/gopher/nex/finger/spartan/guppy. Done condition: `pelt --features smolweb gemini://…` fetches bytes. ~60 lines + tests.
- **B — errand gains `parse` (feature-gated).** Rather than a separate sibling, fold the smolweb parsers into errand behind a `parse` feature (transport stays the default; murm/misfin/meerkat keep a parser-free errand). Lift the tokenizer halves of nematic's `gemtext.rs` / `gopher.rs` / `nex.rs` / `feed.rs` / `finger.rs` / `spartan.rs` / `guppy.rs` / `scroll.rs` into `errand::parse::*` returning per-format ASTs (no inker types; feeds add `quick-xml` under the gate). nematic keeps the `Engine` impls as `ast → DocumentBlock` lowerings and depends on `errand` with the `parse` feature; `knot/expand.rs` and serval/smolweb-views consume the same module. Done condition: nematic builds against `errand/parse`, all existing nematic tests green, parse and lowering separated.
- **C — serval/smolweb-views.** New serval component (depends on xilem-serval + `errand/parse` + tinct). Ship `gemtext_view` first (covers gemtext + nex/finger/spartan/guppy), then `gopher_view`, then `feed_view`. Wire into pelt as the render side of A. Done condition: `pelt gemini://…` renders a native, arrowable gemtext page; gopher menus show item-type affordances; a feed renders as an article list.
- **Table (prerequisite, djot-plan-owned).** Add `DocumentBlock::Table` + its mapper case + round-trip export **before** either the djot note tile or the markdown tile renders, since both consume it. Sequenced ahead of D.
- **D — block→view mapper (document family).** The djot plan owns this (its slice 1). markdown rides it (the `markdown_tile` of §2) rather than growing a bespoke view; reader-mode HTML rides it too.

---

## Progress

- **2026-06-27**: Plan created from the nematic-spin-out conversation with Mark. Resolved the product question (native + idiomatic per format, lower into gemtext only where clean). Architecture verified against the live tree (Findings above). No code yet.

---

## Cross-references

- [djot editor + knot nodes plan](../../mere_docs/implementation_strategy/2026-06-24_djot_editor_knot_nodes_plan.md) — owns the document-family block→view mapper (Phase D); its 2026-06-27 reframe set the direct-to-serval-views render path this plan shares.
- [illume text lexer plan](../../mere_docs/implementation_strategy/2026-06-26_illume_text_lexer_plan.md) — the portable-core + tinct + serval-renderer + host-bridge pattern this mirrors; the entity pass reused inside gemtext prose.
- [polyglot knot design](2026-05-08_polyglot_knot_design.md) — the knot format the capture path lowers into; `knot/expand.rs` shares the smolweb parse functions.
- [knot evaluation + export plan](2026-06-12_knot_evaluation_export_plan.md) — the `to_gemtext` / gophermap exporters on the capture/round-trip side of `DocumentBlock`.
- errand (sibling repo `mark-ik/errand`) — the shared smolweb transport Phase A wires into pelt.
