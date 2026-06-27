# Djot Editor + Knot Nodes + Web Clips Plan

**Status: building; editor surface reframed 2026-06-27** (see the Reframe section).
The editor is no longer a chrome panel: a note is a locally-addressed knot document
inker routes to a serval-document tile, created by typing a `knot://` address into
the omnibar. The knot format, clips, polyglot vocabulary, and ergonomics below
stand. Originally scoped 2026-06-24 via multi-agent code sweeps of the live
workspace, adding the *write* side to a knot/djot stack that is already
read-complete, plus an element-pick clip path into the graph, with an editor that
stays pure Rust (jotdown for the outer djot, a `logos` lexer pack for
inner-language injection; tree-sitter an optional breadth hatch).

This plan owns the **editor surface** (a djot writing pane), the **editable knot
node** (a node whose body is a knot you author in place), and the **web-clip
gesture** (pick an element off a live page, land it as a knot node with a
provenance edge). It does not re-scope the engine, the polyglot block vocabulary,
the outline lens, or the extraction lane, all of which already have owners. See
[Cross-references](#cross-references).

---

## The idea

Mere parses `.knot` files (frontmatter for meaning, djot body for content) into
the portable `EngineDocument` block model and serializes back out. That pipe is
read-only today: text in, blocks out, blocks back to text. This plan gives it a
writing surface. Build an ergonomic djot editor that produces knot nodes you can
edit and render, keep knot the default note format while `sniff` keeps markdown,
txt, and other plain formats opening, and reuse the scrape and capture stack to
pick an element off a live page and land it as a web-clip knot node carrying a
`ClippedFrom` edge back to the source.

---

## Reframe (2026-06-27): note as a routed serval-document tile

Converged with Mark. The editor's home and render path change; the knot format,
clips, polyglot vocabulary, and ergonomics below stand. This supersedes the
chrome-panel editor surface (the 2026-06-25 expedient) and the render split that
sent the preview through document-canvas (old Decision 3).

**A note is a locally-addressed document inker routes, not a chrome feature.**
inker already maps a content-type to a producer (`routing.rs`: `text/x-knot` →
`DjotKnotEngine`, `text/html` → the web surface engine). A note is one more producer
to route: a **local-knot producer** resolves a `knot://` address to a node's body,
hands it to `DjotKnotEngine` for an `EngineDocument`, and that renders as a pelt
tile, the same pipeline a page-node takes through the scrying/graft surface engine,
locally addressed. So the note-node correspondence is literal: opening a note-node
is opening a content tile, identical in kind to opening a page-node. The node is the
document; the tile is its view.

**Render through serval + netrender, via a block→view mapper.** The host already
builds a serval `ScriptedDom` by hand: the whole chrome is one, laid out by
serval-layout and painted by netrender through `scene_from_session`. A note is the
same move on a different source tree, `EngineDocument` → xilem_serval views
(headings, paragraphs, lists, code, blockquote, links) → `ScriptedDom` →
serval-layout → netrender. The one new piece is the mapper, and it rides the
view→DOM path the chrome proves every frame. The real web engine renders the note,
so document-canvas leaves the note path (it stays on the node card for now).
Building the view tree directly means no serialize-to-HTML round-trip.

**Edit mode is the source, source-as-truth.** jotdown is read-only, so the buffer
stays the source text. Edit mode is the illume-highlighted serval styled field over
the same node body, re-rendered through the mapper on change. The rendered serval
document is the view; the source field is the edit; both are serval DOM. This is not
WYSIWYG over the rendered tree.

**The omnibar is the new-note entry: address-to-create.** Typing `knot://field-notes`
routes to the local-knot producer; finding nothing there, it creates a new node
claiming that address with an empty body and opens it (wiki-style create-on-miss).
Navigating the same address later opens the existing note. `knot://x` is an
`AddressClaim` on a node: identity stays the Uuid, case folds on resolve, display
keeps the author's case (the carve rule). This closes the "reachable new-note entry"
gap the in-the-wings audit named as dominant, and makes the deferred `knot://`
resolver worth wiring now (it rides `Address::Custom` until an `AddressKind::Knot`
variant lands).

**The welcome page teaches the address vocabulary.** Opening a new node lands on
`mere://welcome`, which shows the sorts of nodes you make by scheme: `knot://` a
note, `http://` a page, `gemini://` a capsule, and so on. The page can itself be a
knot document rendered through the new mapper, so it dogfoods the renderer while it
onboards.

### Re-scoped slices

1. **`EngineDocument` → serval-view mapper**, proven by rendering `mere://welcome`
   as a serval document tile. Static knot, no kernel risk, self-demonstrating.
2. **Omnibar `knot://` routing** to the local-knot producer, create-or-open, opening
   the tile.
3. **Persistence**: the inline `Node` body plus the `knot://` `AddressClaim`, so a
   created note durably reopens (the kernel-schema change, isolated here with its
   snapshot round-trip test).
4. **Edit mode**: source editing (illume-highlighted) over the body, re-rendered
   through the mapper on change.

The clip phases, the query-block and agent-node wave, and the deferred power-editing
below continue unchanged.

---

## What already exists (code-verified)

The substrate is mostly built. The value of this plan is precision about what is
reusable.

- **Knot format plus two registered engines.** `.knot` = frontmatter + djot or
  markdown body, content-type `text/x-knot`. Both `KnotEngine` (CommonMark,
  `nematic.knot`) and `DjotKnotEngine` (jotdown 0.10, `nematic.knot-djot`) are
  registered in `nematic::engines()` (lib.rs:97-98). `routing.rs:424-431` sends
  `text/x-knot` to the djot engine as the default grammar. `DjotKnotEngine`
  parses the body into `Vec<DocumentBlock>`; `blocks_to_djot()` writes it back.
  Files: `crates/inker/engines/nematic/src/knot/djot.rs`, `knot.rs`.
- **Portable block model.** `EngineDocument` / `DocumentBlock` / `InlineSpan`
  (with `Link.predicate` carrying open rel IRIs), serde plus a11y mapped.
  File: `crates/inker/src/document.rs`.
- **Export half.** `to_knot` / `to_markdown` / `to_gemini` / `to_gophermap` /
  `to_text` plus `write_knot_body` / `write_knot_frontmatter`.
  Files: `crates/inker/src/document/render.rs`, `render/export.rs`.
- **Polyglot inline blocks, two seams.** Protocol and format fences
  (`expand_fenced_blocks`: gemtext, gopher, nex, feed, metadata, badge) plus
  inline rewrites (`[[wikilink]]`, `#hashtag`).
  File: `crates/inker/engines/nematic/src/knot/expand.rs`.
- **Script and include passes, host-driven and policy-gated.** `evaluate_blocks`
  (` <lang> eval ` fences, `BlockEvaluator` registry, `EvaluationPolicy`) and
  `resolve_transclusions` (` include <url> `, `TransclusionPolicy`). inker ships
  no evaluator; the registry is empty and host-supplied, so the eval lane lights
  up only when a host evaluator is present.
  Files: `crates/inker/src/document/evaluate.rs`, `transclude.rs`.
- **Statements seam.** Predicate-bearing links become kernel `Semantic` edges via
  `link_statements` / `apply_link_statements` / `resolve_rel`.
  File: `crates/inker/src/statements.rs`.
- **Format sniff.** Detects `text/x-knot` (closing `---` frontmatter) versus
  `text/markdown`. File: `crates/inker/src/sniff.rs`.
- **Clip producer.** `build_clip_knot(blocks, source, trust, note_kind)` and a
  `_with_block_provenance` variant assemble a `.knot` from selected blocks.
  Nothing calls either from a user action yet.
  File: `crates/inker/engines/nematic/src/knot/expand/build.rs`.
- **Page-to-graph pipe.** Whole-page fetch to `GraphContribution` to
  `apply_contribution` to `Orrery::ingest_graph`.
  Files: `crates/meerkat/src/fetch.rs`, `ingest.rs`, `crawl/mod.rs`.
- **Element-selection primitive (in serval).** A CSS selector matcher
  (`Selectors`) wired as `querySelector` / `querySelectorAll` / `matches`, plus
  `extract_*` over `LayoutDom`.
  Files: `repos/serval/components/script-runtime-api/selector.rs`, `dom.rs`;
  `serval/components/serval-extract/lib.rs`.
- **Live-page scripting plus capture.** `execute_script_with_result` against the
  JS-rendered page (scrying_host.rs:260); `acquire_frame() -> SurfaceFrame` (GPU
  texture) and `capture_snapshot_png()` (producer.rs:62,147). Whole-surface only.
- **A real multi-line edit widget.** `xilem_serval::TextInput`
  (`repos/serval/components/xilem-serval/src/controls.rs`) is a String buffer with
  a char-index caret plus anchor selection, IME preedit, ghost text, select-all,
  and a `textarea` handler with column-preserving `move_up` / `move_down`,
  `home_line` / `end_line`, and `set_caret_byte`. It lays out through serval-layout
  on parley; the caret paints via `serval_layout::caret_rect`; meerkat already
  wires IME and focus across chrome fields. So the note editor extends a working
  multi-line widget. Two gaps: it lays the buffer out as plain runs (it styles
  only the preedit and ghost spans, so a per-range highlight channel is net-new),
  and `controls.rs` is already **696 LOC, over the 600 ceiling**, so editor work
  lands in new files.
- **The click-to-place primitive, half-wired.** `caret_byte_at_point` exists as a
  free function in serval-layout (point to byte via parley `Cursor::from_point`)
  but is not threaded through the `IncrementalLayout` session to `set_caret_byte`.
  Wiring that passthrough plus a meerkat call site is the one missing input piece.
- **A styled preview render path.** inker `document-canvas` renders styled
  `EngineDocument` blocks by flattening the inline-span tree into one parley string
  with per-byte-range `StyleProperty` pushes. It disowns caret, selection, and IME
  (the host's job), so it is a preview surface, not an edit surface.
  Files: `crates/inker/document-canvas/src/` (text.rs, style.rs, layout.rs).
- **A pure-Rust XML lexer already in nematic.** `quick-xml` is a nematic
  dependency. It is the inner lexer for `=svg` and other XML payloads, no new
  dependency. File: `crates/inker/engines/nematic/Cargo.toml`.
- **Swatch pattern.** A graph element rendered as portable serval-laid-out DOM,
  with inline-block and replaced-box layout. Today it is the node face and shape
  surface; it is the natural carrier for an inline web-clip block.
  File: `crates/meerkat/src/swatch.rs`.
- **Node model is content-agnostic.** `Node` = `id: Uuid` plus
  `addresses: Vec<AddressClaim>` plus title and metadata. There is no body field.
  A knot node needs no new node *type*.
  Files: `crates/graph/graph-kernel/src/graph/node.rs`, `address.rs`.
- **`ClippedFrom` edge already modeled.** `ProvenanceSubKind::ClippedFrom` exists
  in the edge taxonomy (edge_taxonomy.rs:186) and round-trips through the snapshot
  (snapshot/from.rs, to.rs). It has zero writers on the live path.

---

## What is net-new

- **The edit-in side.** Every mutator today is a host pass that rebuilds the block
  `Vec`. The editor pane is net-new, though it extends `TextInput`. The concrete
  net-new pieces are a per-range style channel on the field, the click-to-place
  wiring, and the highlight/ergonomics/structural layers below.
- **A reachable new-note entry.** Both engines are registered, but no user action
  creates a knot or opens one for editing. The in-the-wings audit names this one
  wire as the biggest gap.
- **Editable-note persistence.** Eidetic engrams are immutable (an edit makes a
  new hash) and the graph snapshot has no body slot. **Decided: an inline body on
  `Node`** (see Decisions).
- **Element picker.** No hover-to-select UI exists. Today the path is whole-page
  fetch plus tag and role extraction.
- **Selector to fragment clip.** Nothing captures a chosen element's HTML subtree
  as a stored clip body.
- **Element-rect visual clip.** `acquire_frame` / `capture_snapshot_png` are
  whole-surface; nothing crops to a selected element's box.
- **Web-clip to node spawn plus provenance writer.** `build_clip_knot` produces
  knot text, not a node; nothing spawns a node from it or asserts `ClippedFrom`.
- **The editor pipe.** Highlight, folds, outline, structural motion, and injection,
  all pure Rust from jotdown plus curated inner lexers. Phase 3 (see Editor
  architecture).
- **Richer djot span fidelity.** The current parse flattens emphasis and strong to
  plain text and does not nest links. Owned by the knot-evaluation plan.

---

## Editor architecture: one djot parser, curated inner lexers

One source text, one djot parser, and a thin per-language lexer dispatch for the
inner content of polyglot blocks. Everything stays pure Rust, so native and the
browser build the same way. The editor decorates and navigates; the engine decides
meaning.

- **jotdown is the parse for both meaning and editor structure.** It is a pure-Rust
  djot parser. `Parser::into_offset_iter()` turns source text into `DocumentBlock`s
  (the meaning pipe: render, export, statements, graph) and into
  `(Event, Range<usize>)` byte spans (the highlight list). Its nested
  `Start(Container)` / `End(Container)` events also build a small container tree,
  which gives folds, the heading outline, and expand-to-enclosing-container
  selection. Source text is the single source of truth; blocks are derived, never
  written back (jotdown is read-only, so a "save the rendered blocks" shortcut would
  silently drop any djot the block model does not cover).
- **Injection is a per-language lexer dispatch.** The one thing jotdown cannot do is
  highlight the inner language of a polyglot block, since it only parses djot. So the
  editor reads the fence or `lang` label, runs that language's own pure-Rust lexer
  over the block's byte range, and merges the styles back. A fence in a language with
  no wired lexer renders plain.

Both feed one `(range, style)` channel into the edit surface. tree-sitter is an
optional later branch, taken only if arbitrary-language injection or tree-sitter's
error-tolerance is wanted (see Crate decision). For a curated note vocabulary it is
not needed, and skipping it keeps the whole editor browser-clean with no C or wasm
toolchain.

### Render split: edit on one surface, preview on the other

Two render paths already exist and both already do per-range styled text. The
editor picks one for each job, and keeps each surface to its strength.

- **Edit surface: extend `xilem_serval::TextInput`.** It is the only thing in the
  stack with the geometry an editor needs (char-index caret, anchor selection, IME
  preedit, ghost text, multi-line motion, `caret_rect`, `set_caret_byte`). What it
  lacks is a per-range style channel: today it lays the buffer out as plain runs,
  styling only the preedit and ghost spans. Highlighting means emitting the body
  as styled inline spans per highlight range, reusing the exact mechanism preedit
  and ghost already prove. Serval is an HTML engine, so this is additive.
- **Preview surface: inker `document-canvas`.** It renders styled `EngineDocument`
  blocks today and disowns caret, selection, and IME, so it stays a preview beside
  the source.

The one missing input primitive gates click-to-place and drag-select:
`caret_byte_at_point` exists in serval-layout but is not wired through the
`IncrementalLayout` session to `set_caret_byte`. Wiring that passthrough plus a
meerkat call site lands in Phase 1.

### Highlight: jotdown spans, then the container tree and injection

The edit surface ships and highlights with jotdown alone, then grows structure.

**Phase 2 highlights from jotdown spans.** `into_offset_iter()` already yields
`(Event, Range<usize>)` for every djot construct, which is the `(range, style)`
list a highlighter needs, and Mere already computes it for the meaning pipe. So
v1 highlighting is: walk the offset iterator, map each event to a style id, hand
the field a `Vec<(Range, StyleId)>`, paint it. Zero new crate, no C, browser-clean
by construction.

**Phase 3 adds the container tree and injection.** Fold the same nested event
stream into a container tree for section folding, the heading outline, and
structural selection. Add the injection dispatch for inner-language highlight. All
pure Rust, all on the same `(range, style)` channel.

### The headline: injection as a pluggable lexer registry

Mere's signature note move is the polyglot block: a knot carrying rhai, html, or
svg inside it. The editor highlights the inner language by reading the block's
fence or `lang` label and running a lexer registered for that language over the
block's byte range, merging the styled spans back. The dispatch is a small registry
of `InjectionLexer` keyed by label, in three tiers:

- **Precise built-in lexers** for the core polyglot vocabulary: `quick-xml` (already
  a nematic dependency) for svg and other xml, `html5ever` for html, rhai's own
  tokenizer for the script languages. Exact and fast. Because the dispatch is ours,
  the `{.mere-script lang=rhai}` div reads its language straight from the attribute,
  with no parser-grammar limitation on how the block is written.
- **A curated logos pack** for broad coverage, so a common ` ```python ` or
  ` ```toml ` block just works. [`logos`](https://github.com/maciejhirsz/logos)
  compiles a language's tokens into one DFA with jump tables and no backtracking,
  faster than a hand-written lexer and well clear of any regex engine. Pure Rust,
  wasm-safe by construction, tiny. Injection only needs token coloring, not a parse
  tree, so a lexer is the right tool and the fastest one. The cost is authoring one
  small lexer per language, since there is no pre-built logos library; bounded for a
  note tool's roughly fifteen common script and format languages, with the mod path
  for the long tail. (`lexgen` is the comparable alternative generator.)
- **Mod lexers.** Anyone registers an `InjectionLexer` for their own language through
  the mod loader, the easy path being another `logos` lexer or the language's own
  tokenizer (as rhai's). The hand-lexers, the pack, and the mods are all the same
  trait at runtime, with no rebuild. A label with no registered lexer renders plain.

Reuse-lexers are optional precision over the always-present logos floor: a language
whose precise tokenizer is feature-gated or swappable (JS under Nova or Boa, rhai,
rune) still highlights from its C-family floor in the pack, and a host overrides with
the precise lexer only when that engine's tokenizer is actually compiled in.
Highlighting never depends on an execution-engine build flag.

This keeps the whole editor pure Rust and wasm-safe with no build apparatus: jotdown
plus logos compile for wasm32-unknown-unknown like the rest of the app, with no
c2rust, no git-fetched grammars, and no fork. The honest cost is curation: you author
and maintain a small lexer set rather than consume a grammar library, and a coarse
lexer cannot match tree-sitter's tree-aware precision (telling a call from a
variable), which rarely matters for a note's code block. logos is fast enough to run
inline at note size, so the off-thread worry that dogged the regex engines does not
arise.

If breadth beyond a curated set is ever wanted, tree-sitter is the optional hatch:
`syntastica` with `runtime-c2rust` reuses the tree-sitter grammar library through the
same `InjectionLexer` trait, wasm-safe via c2rust, at the cost of its build friction
(git-fetched grammars, slow compiles, or a vendored `mere-grammars` fork of
pre-transpiled committed grammars). The regex-grade libraries `synoptic` (lightweight,
incremental, pure Rust) or `syntect` (broad Sublime grammars, pure Rust on
`default-fancy`) are the other ready-made option if you would rather not author
lexers, slower and less precise than logos.

### Build path: pure Rust, no wasm question

The whole editor (jotdown, the container tree, the precise lexers `quick-xml`,
`html5ever`, rhai's tokenizer, and the logos pack and mods) builds for
wasm32-unknown-unknown with no C and no build question, the same target wgpu and
wasm-bindgen already use. Native and the PWA build identically. The only piece that
would carry build cost is the optional tree-sitter breadth hatch (`syntastica`
c2rust), and it is optional, so the default editor never pays it, and never a runtime
JIT.

The only place a wasm or C question arises is the optional tree-sitter branch
(below). It is optional, so the default editor never pays it.

### gpui/Zed cues worth lifting

Four Zed ideas map onto Mere concepts and serve note-taking directly.

- **Anchors (stable offsets).** Positions that survive edits. Mere needs this for
  clip provenance (a `ClippedFrom` range that stays pinned as you edit around it)
  and later for multi-cursor. Implement as a small anchor layer over the buffer,
  not a rope dependency at note size.
- **Block decorations (inline embeds).** Mere's inline web-clip is a swatch and
  block sibling in the layout, not styled text. serval already lays out
  inline-block and replaced boxes, so the clip-in-note rides the existing swatch
  layout. Highlighting is a text-run concern; the clip is a block decoration. Keep
  them separate.
- **Action and keymap tied to the command registry.** Every editor action
  (new-note, fold, outline-jump, expand-selection, pick-and-clip) registers as a
  command id, so both the radial (gamepad) and context (mouse and keyboard) menus
  reach it and both input modes operate it, per the control-UX rule.
- **Undo transactions.** Group auto-generated edits (auto-pair, list continuation)
  with the keystroke that triggered them, so one undo feels like one action. A
  plain snapshot stack, no rope at note size.

Every layer lands in its own small file (the span-to-style mapper, the container
tree, the injection dispatch, the anchor layer, the undo stack, the slash and link
menus), because `controls.rs` (696), `djot.rs` (571), and `knot.rs` (289) cannot
absorb editor work without blowing the ceiling.

---

## Knot, clips, storage

### Knot as default note, polyglot blocks, other formats

Knot stays the native note format and djot stays the default grammar. The inline
alt-format blocks are two spec-blessed shapes the editor surfaces and routes by
jotdown attributes:

- **Attribute-tagged fenced div** (`{.mere-script lang=rhai}` or
  `{protocol=gemini}` then `:::`) when the body stays djot but is tagged for
  routing through `expand_fenced_blocks` or `evaluate_blocks`.
- **`=FORMAT` raw block** (` ```=html `, ` ```=svg `) when the body is an opaque
  alt-format payload handed verbatim to another engine.

Both hand the editor a format or attribute key plus an exact source span, which is
also the key the injection dispatch reads to pick the inner lexer. The editor makes
those regions visible and editable, and holds the trust rule: a SelfAsserted note
you own runs its fences per setting, received content renders inert source. The
descriptor stays a `CodeBlock` with the fence info string (the shipped decision in
the polyglot plans); the editor adds no new `DocumentBlock` variant.

Other formats ride `sniff`. `sniff_content_type` splits knot from markdown today.
Markdown opens through the CommonMark engine; `.txt` opens as a plain body. The
editor edits raw text regardless of format; the format picks which engine renders
the preview and which exporter saves. A markdown or txt note saves back in its own
format (`to_markdown` / `to_text`); converting to knot is an explicit user action.

### Syntax harvest from carve (extensions over djot)

[Carve](https://github.com/markup-carve/carve) is a post-Markdown markup whose
charm is visual mnemonics (`/italic/` leans, `,sub,` sits low, `^super^` points
up). We keep djot's delimiters; its `/` emphasis in particular fights real text
(URLs, paths, dates), and djot's restraint reads better at length. Carve's
*semantics* are the harvest, and they are what a graph notebook wants. They
extend the grammar; knot already carries `[[wikilink]]` and `#hashtag` as inline
rewrites (`crates/inker/engines/nematic/src/knot/expand.rs`).

| Carve idea | Knot mapping | Status |
| --- | --- | --- |
| Cross-ref auto-fills its text from the target (`</#id>`) | A node link with empty display text resolves to the target node's current title and re-resolves when that title changes. djot's `[](#id)` is manual; this is the standout add. | Net-new: a title lookup over the graph, layered on the `[[ ]]` rewrite. |
| `@mention` first-class | A third inline rewrite beside `[[ ]]` and `#`, asserting a mention edge to the named node. | Net-new: `#hashtag` exists, `@` is its missing sibling. |
| Case-preserving IDs, case-insensitive resolution | Reference a node by title in any case; display keeps the author's case, resolution folds case. Fits title / URL identity. | Net-new resolver rule. |
| Wiki auto-resolution (`[Page][]`) | The `[[node]]` model; the `[[` completion row is the authoring half. | Have it. |
| `+` flush-left list continuation | Reference shape for the smart-list-continuation ergonomics row. | Have the goal. |
| Tables with rowspan/colspan (`^` / `<`) | The one batteries gap in djot tables (flat today). | Later rung. |

Discipline: take the semantics, leave the sigils. The syntax-resembles-output
idea is a feel to aim for in the polyglot fences, not a license for a character
zoo over djot's small rule set.

### Web-clip extraction to knot node

Four moves, most primitives present.

1. **Pick.** A hover-to-select picker on the live scrying tile. Run
   `execute_script_with_result` with a `document.elementFromPoint` probe to resolve
   the element under the pointer and read its tag, id, class, and bounding rect.
   The serval selector engine is the matcher for selector-driven picks.
2. **Capture.** For the chosen element, pull its HTML subtree (plus
   `serval-extract` scoped to that subtree) for the **semantic tier**, and read
   its bounding rect to crop a region from `capture_snapshot_png()` for the
   **rendered tier**.
3. **Store.** Feed the captured blocks plus provenance to `build_clip_knot` to
   assemble a `.knot`. The structured side can also ride `ingest.rs` to
   `apply_contribution` to `Orrery::ingest_graph`.
4. **Render and node.** A clip becomes a **node**, not a card. Spawn a node whose
   body is the clip knot, render it as a new swatch kind, and assert a
   `ProvenanceSubKind::ClippedFrom` edge to the source node. Inline in a note, the
   clip renders as a block decoration (swatch sibling, not styled text), pinned by
   a stable anchor so it survives edits around it. The rendered-texture tier
   carries the on-site look; the semantic tier carries editable,
   statements-bearing content. Register the pick-and-clip action as a command id
   with a consent gate. The faithful HTML fragment tier (sanitized html5ever with
   site context) is the later fidelity rung, owned by the knot-evaluation plan; v1
   clips are semantic-tier plus an optional cropped texture. HTML render depth
   stays Serval's job, not nematic's.

### Query blocks and agent nodes

Two knot-node kinds beyond the plain note and the clip, harvested from the
[borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md) (Mark
graduated both into near-term editor work).

- **`=query` block.** A polyglot block (` ```=query `) whose body is a graph query,
  rendered inline as a live result filtered by the active edge config (which edges
  count). The in-note form of a Tinderbox agent. It rides an existing primitive: the
  gloss design elevates the Navigator swatch to a view "usable in a node facet pane,
  a menu, a djot script block, or an orrery card"
  ([gloss_navigator_design](../design/2026-06-07_gloss_navigator_design.md) §2a), so
  the `=query` block is a swatch embedded by a fence, its (scope, lens, filters) the
  query. Edge-config filtering and the result set ride the
  [graph signals layer](2026-06-22_graph_signals_layer_plan.md).
- **Agent node.** Promote a `=query` to a whole knot node: its body is the query or
  policy, its edges are the materialized, continuously-maintained result set (a
  Tinderbox agent made spatial). The editor authors the body, the orrery materializes
  the edges, so it lands just after the `=query` block.

Trust carries over from the note rules: a query runs read-only over graph truth, an
agent node's materialized edges assert provenance, and a received agent node renders
inert until adopted.

### Storage and identity

A knot node needs no new node type. **Decided: an inline mutable body on `Node`.**
Add a `body` field plus a `PersistedNode` variant carrying it, so the live note
path has mutable storage keyed by the node. Edits are plain text buffer saves;
persistence writes source text; re-render is a full reparse. The `knot://`
addressable path and the eidetic publish snapshot are deferred to the federation
phase, and the `knot://` `AddressKind` variant is added only when that resolver is
wired (a knot node rides `Address::Custom` until then, so the scheme is never
dead). This is the highest-blast-radius change in the plan: it touches `Node`,
`PersistedNode`, and the snapshot round-trip, so it is isolated to Phase 2 with
its own round-trip test.

---

## Editor ergonomics (feature set, scoped to note-taking)

| Feature | Why it serves notes | Phase | Cost |
| --- | --- | --- | --- |
| Click-to-place, drag-select | Mouse caret placement and selection. Table stakes the field cannot do yet. | 1 | Cheap, load-bearing. Wire `caret_byte_at_point` through to `set_caret_byte`. |
| Syntax highlight | Emphasis, headings, links, fence boundaries colored as you type. | 2 | Cheap. The `(range, style)` list is already computed by jotdown; net-new is the style channel. |
| Soft-wrap goal column | Up/Down across wrapped lines holds the target column. Lifts the field from Tier 1. | 2 | Cheap. Store goal column on vertical move, clear on horizontal. |
| Undo/redo transactions | Reliable undo, grouping auto-pair and list inserts with their keystroke. | 2 | Cheap at note size. Snapshot stack, no rope. |
| Smart list continuation | Enter continues the list marker and indent; double-Enter ends it. Biggest ergonomics win. | 3 | Cheap. Detect list context from the line plus span. |
| Auto-pairs | One delimiter inserts its close; wrap a selection to bracket it. | 3 | Cheap. Pure buffer logic, grouped into one undo step. |
| `[[` node-link completion | Typing `[[` offers a live menu of graph nodes to link. Core to knot-as-notebook. | 3 | Medium. Graph-query to completion plus the popup. Wikilink rewrite already exists. |
| Slash `/` command menu | `/` at line start inserts blocks (heading, list, fence, `=html`, `mere-script`, clip). | 3 | Medium. Reuses the `[[` popup with a static template list. |
| Injection highlight | Inner rhai/html/svg in a polyglot block highlights in its own language. The headline. | 3 | Medium. `InjectionLexer` registry, one engine: hand-lexers (quick-xml, html5ever, rhai) + a curated `logos` pack (DFA, fastest, pure-Rust, wasm-safe) + logos mods. tree-sitter optional for breadth. |
| Structural selection | Alt-Up grows the selection to the enclosing span, item, section; Alt-Down shrinks. | 3 | Cheap. From the jotdown container tree (Start/End nesting), no extra parser. |
| Fold sections | Collapse a heading's section to skim a long note. | 3 | Medium. Section tree from jotdown; net-new is the fold UI and line-hiding. |
| Outline | Jump-to-heading list for long notes; ties into the gloss outline lens. | 2 (headings) / 3 (nested) | Cheap from jotdown headings; the gloss plan owns the surface. |
| Inline web-clip block | A clipped element rendered inline mid-note with its on-site look. | 4 (semantic), 5 (rendered) | Medium. Rides the swatch layout; producer and edge already modeled. |
| `=query` block | A fenced graph query rendered inline as a live, edge-config-filtered result (a Navigator swatch embedded by a fence). | 4 | Medium. Rides the embeddable swatch + graph-signals; net-new is the fence binding. |
| Agent node | A knot node whose body is a query/policy and whose edges are its live result set (a Tinderbox agent, made spatial). | 5 | Medium-high. Editor authors the body; the orrery materializes + maintains the edges. |
| Multi-cursor | Edit several spots at once. Power-user polish, lower value for prose. | 6 (deferred) | Medium-high. Needs the anchor layer plus a cursor set. |

Cut as IDE gold-plating, not note-taking: language-server completion (the only
completion sources here are the graph and a template list), runnables and
debugger and test gutters (eval fences already cover note-side execution), git
gutter and blame, and a minimap (notes are short and have an outline).

---

## Phasing (done-conditions)

**Phase 1: editable knot, round-trips in memory, mouse-placeable.**
Done: an editor pane opens a `.knot` source string and edits it with caret,
selection, and IME on the extended `TextInput` (multi-line `textarea` handler).
Click-to-place and drag-select work: `caret_byte_at_point` is wired through the
`IncrementalLayout` session to `set_caret_byte` with a meerkat call site. On save,
`Engine::render(text/x-knot)` shows the rendered blocks in the document-canvas
preview pane beside the source. No highlighting, no persistence yet.

**Phase 2: highlight, ergonomics, new-note entry, persistence, other formats.**
Done: jotdown `into_offset_iter` spans drive syntax highlight on the edit surface
via a new per-range style channel (zero new dependency, browser-clean). Sticky
soft-wrap goal column. Undo/redo as grouped transactions. A new-note command id
creates a knot node; the editor saves its body to the inline `Node` body store and
reopen restores it. Opening `.md` or `.txt` routes through `sniff` to the right
engine for preview while editing raw text; each saves back in its own format.
Carries the `Node` body plus `PersistedNode` schema change and its snapshot
round-trip test (highest blast radius, isolated here).

**Phase 3: editor pipe, injection, live authoring affordances.**
Done: a container tree built from jotdown's nested events drives fold sections, the
heading outline (ties into the gloss outline lens plan), and expand/shrink
structural selection. Injection highlights the inner language of a polyglot block:
a per-language lexer dispatch reads the fence or `lang` label and runs that
language's own pure-Rust lexer (`quick-xml` for svg and xml, `html5ever` for html,
rhai's tokenizer for the script languages), merging styles back; a fence with no
wired lexer renders plain. Protocol fences expand and ` <lang> eval ` fences
evaluate under host policy for your own notes (received notes render inert);
wikilinks and hashtags type live. Authoring affordances: smart list continuation,
auto-pairs, `/` slash menu, `[[` node-link completion, all registered as command
ids. All pure Rust, native and PWA identical, no wasm or C toolchain. Eval depends
on a host evaluator being registered.

**Phase 4: semantic web clip to node (inline block decoration).**
Done: a picker selects an element on a live scrying tile, captures its HTML subtree
plus extracted text and links via `execute_script_with_result`, builds a clip knot
with `build_clip_knot`, spawns a knot node, and asserts a `ClippedFrom` edge to the
source. The clip renders as a block decoration pinned by a stable anchor. Consent
UI present, command-registered. First live writer of `ClippedFrom`, so it gets its
own assert plus snapshot test.

**Phase 5: on-site rendered tier.**
Done: the clip carries a cropped texture of the element's rect (from
`capture_snapshot_png`) rendered as a swatch kind, so the clip node shows the
element as it appeared on the page alongside its editable semantic body. Validate
the device-pixel-ratio and scroll-offset mapping between `getBoundingClientRect`
and the captured surface at runtime.

**Query blocks and agent nodes (node-kinds wave, beside the clips).**
Done: a ` ```=query ` fence embeds a Navigator swatch by its (scope, lens, filters),
rendered inline as a live, edge-config-filtered result over graph truth (the gloss
swatch's djot-block consumer). An agent node promotes that query to a whole knot node
whose body is the query/policy and whose edges are the materialized,
continuously-maintained result set, asserted with provenance and rendered inert until
adopted when received. Rides the embeddable swatch and the graph-signals layer: the
editor authors the body, the orrery materializes the edges. Near-term (this
node-kinds wave, the same as the clip phases), not the deferred Phase 6.

**Phase 6 (deferred): fidelity, federation, power-editing.**
Done: clips can render a sanitized HTML fragment with site context (the
knot-evaluation HTML tier); shared notes publish to a `knot://` addressable or
engram path (the `AddressKind` variant added here); emphasis, strong, and nested
links survive the parse round-trip. Multi-cursor lands here over the anchor layer
if note-taking demand proves it. If large imported docs strain the flat-String full
reparse, swap the buffer for a rope and the reparse for an incremental one. If a
note ever needs arbitrary-language injection beyond the curated set, this is where
the optional tree-sitter branch (Crate decision) would land.

---

## Crate decision

**jotdown for the outer djot, a pluggable injection registry for inner languages,
on the host's parley text widget.**

- jotdown 0.10 is the one parse: source text to `DocumentBlock`s (meaning) and to
  highlight spans plus a container tree (editor structure). Source text is the
  single source of truth. [`jotdown`](https://crates.io/crates/jotdown) is
  parse-only, so the editor never re-serializes an AST. Pure Rust, builds for
  wasm32-unknown-unknown like the rest of the app.
- The edit surface is the existing `xilem_serval::TextInput` (multi-line, caret,
  selection, IME), extended with a per-range style channel. Preview renders through
  inker `document-canvas`. Both are already in the stack.
- Injection (inner-language highlight) is an `InjectionLexer` registry dispatched by
  the fence or `lang` label, three tiers and one engine: precise hand-lexers
  ([`quick-xml`](https://crates.io/crates/quick-xml) for svg/xml, already a nematic
  dependency; [`html5ever`](https://crates.io/crates/html5ever) for html; rhai's
  tokenizer for scripts); a curated [`logos`](https://crates.io/crates/logos) pack for
  broad coverage (a DFA lexer, faster than tree-sitter or any regex engine for the
  coloring job, pure Rust, wasm-safe, tiny; you author one small lexer per language,
  since no logos language library exists; `lexgen` is the comparable generator); and
  mod lexers, again `logos` or a language's own tokenizer, registered at runtime with
  no rebuild. A label with no registered lexer renders plain. Injection needs only
  token coloring, not a parse tree, so the lexer is both right and fastest, and
  jotdown plus logos is the whole pure-Rust, wasm-safe, build-apparatus-free stack.
  The optional hatch for breadth beyond the curated set is tree-sitter via
  [`syntastica`](https://crates.io/crates/syntastica) (`runtime-c2rust`, wasm-safe via
  c2rust, at its git-build friction or a vendored `mere-grammars` fork); the
  regex-grade libraries [`synoptic`](https://crates.io/crates/synoptic) or
  [`syntect`](https://crates.io/crates/syntect) (pure Rust on `default-fancy`) are the
  other ready-made-library option, slower and less precise than logos.
- No turnkey djot editor crate exists in Rust. `egui_commonmark`, iced's markdown
  widget, and cosmic-text plus glyphon each couple the editor to a foreign toolkit,
  against the portable mere-domain and vello-host line.
- **Optional: tree-sitter for the outer djot too.** jotdown carries the outer-djot
  highlight and the container tree. If djot's editor-side parse ever wants
  tree-sitter's error-tolerance, `tree-sitter-djot` v2.0.0 (MIT, Jonas Hietala, the
  grammar Helix uses; track Codeberg, the GitHub repo froze 2026-04-27) is the swap,
  on the same c2rust wasm path the injection pack already uses, feeding the same
  `(range, style)` channel. The runtime-grammar `wasm` feature (wasmtime JIT) stays
  banned and will not build for wasm32 anyway. Recorded as a door, not a commitment.

---

## Decisions

Resolved with Mark 2026-06-24:

1. **Body storage: inline body on `Node`**, deferring `knot://` and the eidetic
   publish path to the federation phase.
2. **Editor widget reach: extend `xilem_serval::TextInput`.** It already does
   multi-line, selection, and IME; the editor adds a style channel, not a new
   widget.
3. **Render split** — *superseded by the 2026-06-27 Reframe.* Originally: edit on the
   serval field, preview on document-canvas. The Reframe renders the note through
   serval-views + netrender (the web engine) and keeps document-canvas off the note
   path; edit mode stays the serval source field.
4. **Outer-djot pipe: pure-Rust jotdown.** Highlight spans plus a container tree
   (folds, outline, structural selection) from one jotdown parse. No C, no wasm
   question for the editor floor. Inner-language injection is a separate registry
   (Decision 7), engine `logos`, with tree-sitter only as an optional breadth hatch.
5. **Buffer: stay on flat String plus char-index.** Snapshot undo and full reparse
   are fast at note size. A rope waits for large imported docs (Phase 6).
6. **Multi-cursor: out of v1, deferred to Phase 6.** Cheap to add once anchors
   exist; not core to prose.
7. **Injection is a pluggable `InjectionLexer` registry; the engine is `logos`.**
   Precise hand-lexers (quick-xml svg/xml, html5ever html, rhai scripts) for the core
   vocabulary; a curated `logos` pack (a DFA lexer, the fastest pure-Rust option,
   wasm-safe, no build apparatus) for broad coverage; `logos` again for runtime mods.
   One trait and one engine, and injection needs only coloring (jotdown owns the outer
   structure). The cost is authoring and maintaining a small lexer set rather than
   consuming a grammar library. tree-sitter (`syntastica` c2rust) is the optional
   hatch for breadth beyond the curated set; `synoptic` or `syntect` are the
   regex-grade ready-made alternatives.

Open:

1. **The logos pack's language list.** Which languages get a hand-authored `logos`
   lexer in the v1 pack (lean: rhai, lua, js, html, svg, css, json, toml, yaml,
   markdown, plus a few code languages), and whether to harvest any existing community
   logos lexers. The off-thread question largely dissolves, since logos is fast enough
   to color a block inline.
2. **Editor crate placement.** A portable editing core (style channel, span mapper,
   container tree, injection dispatch, anchor and undo layers) split from host
   render glue, versus a single host module. Lean portable where the parley
   coupling allows.
3. **Picker mechanism and surface.** `elementFromPoint` hit-test versus
   selector-driven pick versus both; live scrying tile only, or also serval-laid-out
   static pages. Lean: `elementFromPoint` on the live tile first.
4. **Clip default tier.** Semantic-only by default with the cropped texture opt-in
   until Phase 5 proves the crop path, versus always capturing both.

---

## Risks

- jotdown has no writer and no mutable AST, so a byte-faithful round-trip of
  arbitrary djot is impossible through the engine. The editor holds the source text
  as truth; `blocks_to_djot` covers only the recognized block vocabulary.
- `controls.rs` is already 696 LOC (over the ceiling), `djot.rs` 571, `knot.rs`
  289. Every editor layer lands in a new small file; none of these three grows.
- The inline `Node` body is net-new kernel schema touching `Node`, `PersistedNode`,
  and the snapshot round-trip. Highest blast radius, isolated to Phase 2.
- The logos pack has no bundled language library, so the work moves from consuming
  grammars to authoring and maintaining one small lexer per language. Bounded for a
  note tool's curated set, and the mod path covers the long tail, but it is owned
  code, and a coarse lexer is less precise than a tree-sitter tree (telling a call
  from a variable), which rarely matters for a note's code block. A language with no
  lexer renders plain. If breadth ever outgrows curation, the optional tree-sitter
  hatch (`syntastica` c2rust) reuses the grammar library at its build-friction cost.
- The container-tree builder, the injection registry, and the curated `logos` lexer
  set are net-new code Mere owns. logos keeps each lexer small, but the set is owned
  and maintained rather than free-ridden from a grammar community. Bounded for a note
  vocabulary; weigh again only if breadth grows, when the tree-sitter hatch amortizes
  better.
- `ClippedFrom` is modeled and persists but has zero live writers. Phase 4 is its
  first writer, so the assert path and its snapshot round-trip need their own test.
- The element-rect crop assumes the JS bounding rect maps cleanly onto the captured
  surface's pixel space. Device-pixel-ratio and scroll-offset mismatches between
  `getBoundingClientRect` and `capture_snapshot_png` are a runtime failure mode to
  validate, not reason about statically.
- The trust rule must hold for clips: a clipped element from a received page is
  received content and renders inert, never evaluates. Easy to regress if the clip
  pipe reuses the own-note render path.
- The host has no prior parley-on-vello multi-line *edit* surface wired (the
  existing fields are chrome single-liners on the same widget). The Phase 1 risk is
  caret, selection, and IME geometry in the note pane, not the parse loop.

---

## Cross-references

This plan extends, and does not re-scope, the following owners:

- [2026-05-08 polyglot knot design](../../nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md)
  and [2026-06-13 polyglot block resolver plan](../../nematic_docs/implementation_strategy/2026-06-13_polyglot_block_resolver_plan.md):
  the block vocabulary and the descriptor-as-`CodeBlock` decision.
- [2026-06-12 knot evaluation and export plan](../../nematic_docs/implementation_strategy/2026-06-12_knot_evaluation_export_plan.md):
  the eval and include passes, the HTML fragment fidelity tier (K4), and richer
  span fidelity. Phase 6 fidelity work belongs there.
- [2026-06-23 gloss outline lens plan](2026-06-23_gloss_outline_lens_plan.md):
  owns the graph-outline-as-editable-knot payoff at its P4 and the outline surface.
  The editor here is the shared writing surface that P4 also uses.
- [2026-06-23 node body face model plan](2026-06-23_node_body_face_model_plan.md):
  owns the node's Body and Face presentation. The clip swatch kind composes with
  it.
- [2026-06-21 command registry configurable menus plan](2026-06-21_command_registry_configurable_menus_plan.md):
  every editor action and the clip gesture register as command ids here.
- [2026-06-10 scrying tile plan](2026-06-10_scrying_tile_plan.md) and
  [2026-06-23 render ladder and extraction plan](2026-06-23_render_ladder_and_extraction_plan.md):
  own the live tile and the parse-and-extract axis the clip path draws on.
- [2026-06-23 browser extension companion plan](2026-06-23_browser_extension_companion_plan.md):
  the consented-capture sink; the web clip is one driver of it.
- [2026-06-15 in-the-wings and browser-bar audit](../research/2026-06-15_in_the_wings_and_browser_bar_audit.md),
  synergy 4: names the new-note wire as the dominant gap.
- [2026-06-18 interaction model spine](../technical_architecture/2026-06-18_interaction_model_spine.md):
  djot is a definitely-support format on the spine; this plan is its write stage.

---

## Findings

Code-verified anchors from the 2026-06-24 sweeps, kept for the next session:

- Both knot engines registered: `nematic/src/lib.rs:97-98`; djot is the routed
  default for `text/x-knot`: `inker/src/routing.rs:424-431`.
- jotdown 0.10 is a streaming, read-only `Parser`; `into_offset_iter()` yields
  `(Event, Range<usize>)`; nested `Start(Container, Attributes)` / `End(Container)`
  events fold into a container tree (folds, outline, structural selection); raw
  blocks carry a `=FORMAT` tag. No AST, no writer. Pure Rust, wasm32 clean.
- `xilem_serval::TextInput` (`repos/serval/components/xilem-serval/src/controls.rs`,
  696 LOC): String buffer, char-index caret, anchor selection, IME preedit, ghost,
  `select_all`, and a `textarea` handler with `move_up` / `move_down` / `home_line`
  / `end_line` / `set_caret_byte`. Plain-run render; styles only preedit and ghost.
- `caret_byte_at_point` is a free function in serval-layout (parley
  `Cursor::from_point`), not wired through `IncrementalLayout` to `set_caret_byte`.
- inker `document-canvas` renders styled `EngineDocument` via per-byte-range
  `StyleProperty` pushes; disowns caret, selection, IME.
- Injection is an `InjectionLexer` registry keyed by language label, one engine.
  Tier 1, precise hand-lexers (all pure Rust, wasm32 clean): `quick-xml` (already a
  nematic dependency) for svg/xml, `html5ever` for html, rhai's tokenizer for scripts.
  Tier 2, a curated `logos` pack for broad coverage: `logos` compiles a language's
  tokens into one DFA (jump tables, no backtracking, built to beat hand-written
  lexers; a hand-tuned lexer can still edge it ~20-30%), pure Rust, wasm-safe, tiny.
  No pre-built logos language library exists, so each language is a small authored
  lexer; `lexgen` is the comparable alternative generator. Tier 3, mod lexers: more
  `logos`, registered at runtime. Injection needs only token coloring, not a tree, so
  the lexer is both right and fastest.
- Breadth hatch and ready-made-library alternatives, used only if curation is
  unwelcome: tree-sitter via `syntastica` (`runtime-c2rust` + `parsers-git` + a
  `some`/`most`/`all` group; wasm-safe via c2rust, no Oniguruma or wasmtime JIT;
  git-fetched grammars + slow compiles, or a vendored `mere-grammars` fork of
  pre-transpiled committed grammars; `Union` + custom-language path to add more;
  initial parse ~2-3x a hand-written parser, incremental edits under 1 ms).
  Regex-grade: `synoptic` (lightweight, ~3 deps, incremental, pure Rust) and `syntect`
  (Sublime grammars; pure Rust only via `default-features = false, features =
  ["default-fancy"]`, since the default `regex-onig` links Oniguruma; `fancy-regex`
  ~half the speed; trim the `SyntaxSet`, ~2 MB full).
- Optional outer-djot branch: `tree-sitter-djot` v2.0.0, MIT, Jonas Hietala, used by
  Helix; ships `injections.scm` / `folds.scm` / `indents.scm` / `textobjects.scm`;
  GitHub froze 2026-04-27, track Codeberg. Plain `parser.c` plus `scanner.c` (C, not
  C++), so `tree-sitter-wasm-build-tool` (which excludes only C++ scanners) plus
  `tree-sitter-c2rust` is the wasm32-unknown-unknown path, the same c2rust mechanism
  the syntastica injection pack uses. The runtime-grammar `wasm` feature is a banned
  wasmtime JIT and will not build for wasm32 anyway.
- Clip producer: `build_clip_knot(blocks, source, trust, note_kind)` plus
  `build_clip_knot_with_block_provenance` at `expand/build.rs:12,43`.
- `Node` carries no body; identity is `id: Uuid` plus `addresses`. `AddressKind`:
  Http, File, Data, Clip, Directory, Custom (`address.rs`).
- `ProvenanceSubKind::ClippedFrom` at `edge_taxonomy.rs:186`, round-trips through
  `snapshot/from.rs` and `to.rs`, zero live writers.
- inker ships no `BlockEvaluator`; the registry is host-supplied and empty.

---

## Progress

- **2026-06-24, scope sweep.** Scoped via a multi-agent code sweep (five mappers
  plus crate research, synthesis, adversarial verify). Corrected an early claim that
  the djot engine was unregistered (it is registered and routed as default).
  Decisions: inline `Node` body, defer `knot://`; write this plan.
- **2026-06-24, editor enrichment.** Second workflow (tree-sitter and Zed/gpui
  research plus a render-path code probe) confirmed `TextInput` is already a
  multi-line edit widget, found the two existing per-range styled render paths and
  the half-wired `caret_byte_at_point` primitive, added the ergonomics feature set,
  and resolved the editor-widget, render-split, buffer, and multi-cursor decisions.
- **2026-06-24, pure-Rust re-aim.** Verified the pure-Rust path (jotdown for
  highlight plus a container tree for folds/outline/structural, and per-language
  pure-Rust lexers for injection: `quick-xml` already a dep, `html5ever`, rhai's
  tokenizer, optional `syntect`+`fancy-regex`). Reframed the editor pipe as pure
  Rust by decision, demoted tree-sitter to an optional branch for
  arbitrary-language injection, and dissolved the PWA/wasm build question (the
  pure-Rust stack builds for wasm32-unknown-unknown like the rest of the app).
  Updated the architecture, feature table, phasing, crate decision, decisions,
  risks, and findings. No code yet.
- **2026-06-24, syntect default pack.** Verified syntect + `fancy-regex` tradeoffs:
  pure Rust only on `default-fancy` (the default links the Oniguruma C lib), about
  half onig's speed and best run off the main thread, trim the syntax set (the full
  default adds ~2 MB to a wasm binary), debug builds slow. No big compromise. Recast
  injection as a pluggable `InjectionLexer` registry (precise hand-lexers plus a
  syntect default pack plus mod lexers, one trait) so the broad default pack and the
  mod-parser story share a seam. No code yet.
- **2026-06-24, tree-sitter structural pack.** Mark wanted pure-Rust, wasm-safe
  tree-sitter with a low-cost language set and mod extensibility. Verified that
  `syntastica` (`runtime-c2rust` + `parsers-git` + `some`) delivers exactly that:
  tree-sitter for wasm32-unknown-unknown with no Oniguruma or wasmtime JIT and a
  feature-selected language subset. Made it the structural default pack (replacing
  syntect, which moves to a regex-grade fallback beside `synoptic`), kept `logos` as
  the easy runtime mod path, and recorded the vendored `mere-grammars` fork
  (pre-transpiled committed grammars) as the build-friction-free end-state, with
  consume-then-fork sequencing. Updated the injection section, build-path section,
  feature table, crate decision, decisions 4 and 7, the open question, risks, and
  findings. No code yet.
- **2026-06-24, logos re-aim.** Beyond tree-sitter on the same restrictions, a DFA
  lexer (`logos`) is the more performant fit, because injection needs token coloring,
  not tree-sitter's tree (which builds a full incremental parse). Made `logos` the
  engine across all three injection tiers (precise hand-lexers, the curated pack, and
  mods), so jotdown-plus-logos is the whole pure-Rust, wasm-safe, build-apparatus-free
  stack, and demoted tree-sitter (`syntastica`) to the optional breadth hatch. The
  trade is authoring and maintaining a small lexer set versus consuming a grammar
  library. Updated the injection and build-path sections, feature table, crate
  decision, decision 7 and the open question, risks, and findings. No code yet.
- **2026-06-24, Phase 1 slice 1a (serval).** Exposed
  `IncrementalLayout::caret_byte_at_point` and `caret_byte_vertical` on the session
  (`repos/serval/components/serval-layout/incremental.rs`), mirroring the existing
  `caret_rect` method and delegating to the `crate::caret::*` free functions over the
  session's retained `built` / `text_ctx` / `fragments`. This is the one missing input
  primitive for click-to-place and soft-wrap vertical motion. `cargo check -p
  serval-layout` green (1m03s; only pre-existing warnings, none from the change). Not
  committed: serval `main` carries Mark's in-flight script-engine work, so the change
  is isolated to serval-layout and left uncommitted. Next: the meerkat call site
  (pointer-down → `caret_byte_at_point` → `TextInput::set_caret_byte`).
- **2026-06-25, Phase 1 slice 1b (meerkat).** Added `PaneSession::caret_byte_at_point`
  and `caret_byte_vertical` wrappers (`crates/meerkat/src/pane_session.rs`, a file
  untouched by the in-flight work), delegating to the new session methods and
  mirroring the existing `caret_rect` wrapper. `cargo check -p meerkat` green (52s,
  run from inside mere on the pinned 1.93.0 toolchain; only the expected
  unused-method warning until a call site lands). The mere tree compiles with the
  concurrent work in place, so it is at a buildable checkpoint. The input-primitive
  bridge is now complete end to end: serval session method → meerkat wrapper →
  (next) call site → `TextInput::set_caret_byte`. The remaining Phase 1 pieces (the
  pointer-down call site and the two-pane editor shell) land in the in-flight pane
  files (`input.rs` / `render.rs` / `pane_data.rs` / `views.rs`), so they wait on the
  concurrent pane-system work or move to an isolated worktree.
- **2026-06-25, Phase 2/3 portable core (`knot-editor` crate).** Created
  `crates/inker/knot-editor` (registered in the workspace; collision-free, since no
  `inker` files or the root `Cargo.toml` are in the in-flight set), the portable
  editor pipe, dep `jotdown` only. `highlight.rs`: `highlight_djot(src) -> Vec<Span>`
  walks jotdown's `into_offset_iter` byte spans into `(range, SyntaxKind)`, one span
  per construct (verified ranges: `# A heading` → 0..11; `` `code` `` → Verbatim
  33..39; fenced block → one CodeBlock region; link/image/blockquote/div whole).
  `injection.rs`: the `InjectionLexer` trait + `InjectionRegistry` (case-insensitive
  label dispatch, `lex_at` offsets inner spans into the document, mods override
  built-ins) — the one seam the three tiers share. `SyntaxKind` carries both the
  djot-structural classes and the generic code-token classes inner lexers emit.
  `cargo test -p knot-editor` green: 11 tests, 0 warnings. Next portable pieces: a
  first `logos` inner lexer (proving the pack tier) and the highlighter→registry
  dispatch for code/raw blocks. The host-side Phase 1 shell still waits on the
  in-flight pane work.
- **2026-06-25, logos pack tier + dispatch.** Added `logos` 0.16 and the curated
  pack (`src/pack.rs`): a `JsonLexer` (logos DFA → string / number / keyword /
  punctuation) as the first pack language, and `default_pack()` returning a registry
  pre-loaded with it. Wired the highlighter→registry dispatch: `highlight(src,
  &registry)` captures each code/raw block's inner range and language from jotdown's
  events and calls `registry.lex_at`, merging the inner-language spans on top of the
  block region; an unregistered language stays a plain region. `cargo test -p
  knot-editor` green: 15 tests, 0 warnings. The full pure-Rust editor pipe is now
  proven end to end in isolation: source text → djot structure spans + injected
  inner-language spans, no C, no wasm question. Growing the pack is a token enum plus
  one `register` line. The host shell still waits on the in-flight pane work.
- **2026-06-25, pack languages batch + format map.** Surveyed the workspace deps for
  reuse tokenizers (free, no new dep, registered host-side): `boa_parser` (JS),
  `cssparser` (CSS), `html5ever` (HTML), `pulldown-cmark` (Markdown), `oxttl`+`oxrdf`
  (Turtle/RDF), `toml_edit`, plus `quick-xml` (XML/SVG/RSS) and rhai. Found engrams
  are JSON: `Engram` (`eidetic-core/src/engram.rs:48`) = `schema` + `payload: Vec<u8>`
  where the payload format is mere-native / json-schema / json-ld, all JSON, so the
  JSON lexer already renders engram schema + data. Added to the logos pack
  (`src/pack.rs`): a keyword-parameterized `ClikeLexer` (one DFA, block-comment
  callback; Rust keyword set, with Rune / JS-fallback sharing it) and a `TomlLexer`;
  `default_pack()` registers json / json-ld / jsonld / toml / rust / rs. `cargo test
  -p knot-editor` green: 18 tests, 0 warnings. **Architecture recorded**: the portable
  `knot-editor` stays lean (jotdown + the logos pack); the free reuse-lexers
  (Boa / cssparser / html5ever / oxttl / pulldown-cmark / rhai / quick-xml) live
  host-side as `InjectionLexer` impls over the same registry, since those deps are
  already in meerkat/serval and wasm-available. This refines Open question 1: the
  pack is logos for languages without a tree tokenizer; everything else reuses the
  host's existing tokenizer.
- **2026-06-25, engine-independent floors (Nova/Boa robustness).** Corrected the
  reuse-lexer framing: highlighting must not depend on which execution engine is
  compiled in (JS under Nova vs Boa, or a rhai/rune-feature-gated build). The rule:
  the portable logos pack is an always-present **floor**; reuse-lexers (Boa, Nova,
  oxc, rhai's tokenizer) are **optional precision overrides** a host registers for
  whatever the build ships. Added C-family floors for `js` / `javascript` / `mjs`,
  `rhai`, `rune` (backtick template-literal strings added to the Clike lexer for JS).
  `cargo test -p knot-editor` green: 19 tests, 0 warnings. mere's current tree
  resolves Boa (not Nova or oxc), so today's override target would be Boa, but the
  floor makes JS highlight regardless.
- **2026-06-25, Lua floor + refined pack scope.** Added a Lua floor lexer
  (`src/pack/lua.rs`, a submodule so pack.rs stays under the 600-LOC ceiling): `--`
  line and `--[[ ]]` block comments plus `[[ ]]` long strings via callbacks, with
  single-char punctuation so the `--` / `[[` tokens win on length. Two bugs caught and
  fixed by the tests (a line-comment regex out-matching the block-comment token; a
  greedy punctuation run swallowing `--[[`). `cargo test -p knot-editor` green: 21
  tests, 0 warnings. Refined the pack-vs-reuse split: the logos pack floors languages
  with no reliably-present tokenizer (Rust, JS, rhai, rune, Lua, JSON / JSON-LD,
  TOML); CSS / HTML / Markdown / Turtle keep a tokenizer whenever serval is loaded
  (cssparser / html5ever / pulldown-cmark / oxttl), so those are host-side reuse, not
  logos floors. Pack labels now: json, json-ld, jsonld, toml, rust, rs, js,
  javascript, mjs, rhai, rune, lua.
- **2026-06-25, container tree (dir 1) + reuse crate (dir 2).** Direction 1: added
  `src/tree.rs` (portable) — `container_tree` folds jotdown's nested events into a
  block-container tree, deriving `folds` (multi-line sections / lists / quotes / code
  / divs), `outline` (headings with levels and text, for the gloss outline lens), and
  `expand_selection` (smallest enclosing container, for Alt-Up grow-selection). All
  jotdown container-variant guesses compiled (Section / Heading / List / ListItem /
  TaskListItem / Blockquote / CodeBlock / Div / Paragraph / Table). `cargo test -p
  knot-editor` green: 26 tests. The portable editor core is now complete: highlight +
  injection + 8 floor languages + structure. Direction 2: created
  `crates/inker/knot-editor-host` (the host-side reuse layer, dep knot-editor +
  pulldown-cmark), with a Markdown reuse-lexer over pulldown-cmark's offset-iter (the
  same shape as the djot highlighter) and `full_pack()` = portable pack + reuse
  overrides. `cargo test -p knot-editor-host` green: 2 tests. CSS (cssparser), HTML
  (html5ever), precise JS (boa_parser), Turtle (oxttl) are the documented next
  reuse-lexers there, each over its existing host tokenizer. Direction 3 (the host
  edit surface) stays parked on the in-flight pane/graphlet work.
- **2026-06-25, CSS + HTML floors.** Added `src/pack/web.rs` (portable): coarse
  `logos` floors for CSS (`/* */` comments, strings, hex colors, numbers with units
  via a callback, `@`-rules) and HTML (`<!-- -->` comments, tag openers, attribute
  strings, `&entities;`). Chose logos over cssparser / html5ever reuse: those are
  parse-oriented and do not yield clean highlight byte-spans, so a DFA is the right
  tool and stays engine-independent. logos caught two ambiguities (a number's unit
  suffix overlapping idents; a `-`-led ident), fixed via a unit-attaching callback
  and a tighter ident start. `cargo test -p knot-editor` green: 28 tests, 0 warnings.
  Pack now: json, json-ld, jsonld, toml, rust, rs, js, javascript, mjs, rhai, rune,
  lua, css, html, htm. Direction 3 (host shell) re-checked: still blocked. mere is
  ahead 2 (some pane work committed: pane_data / pane_geom / pane_session now clean),
  but `input.rs`, `render.rs`, and `views.rs` remain in the in-flight graphlet work,
  and those are the three files the shell's call site, caret paint, and pane render
  land in.
- **2026-06-25, editor model (the host brain).** Built `KnotEditor` in
  `crates/inker/knot-editor-host/src/editor.rs` (added deps inker + nematic): holds the
  knot source and derives the two things the editor surface draws — `highlights` (djot
  structure + injected polyglot colouring via `full_pack`), `outline` / `folds`
  (structure), and `rendered` (the preview `EngineDocument` via `DjotKnotEngine`, the
  same engine path the app renders knots through); `set_source` syncs the host buffer.
  `cargo test -p knot-editor-host` green: 6 tests (my code 0 warnings; the 22 are
  pre-existing kernel/nematic dep warnings). This is the host-side editor brain,
  collision-free. Remaining host-shell work is the **distributed pane render/input
  wiring**: meerkat has no single pane-render switchboard — `render.rs` resolves each
  pane type through scattered `pane_of_content` / `matches!` lookups with per-type draw
  paths, so the editor pane (a `PaneContent::Custom("knot-editor")`, which avoids enum
  churn) is a multi-point addition across the live `render.rs` (draw the editable
  textarea + the document-canvas preview, driving `KnotEditor`) and `input.rs` (the
  click-to-place call site over the landed `PaneSession::caret_byte_at_point`). That
  pass is in the in-flight render/input files.
- **2026-06-25, host shell increments A+B (meerkat, additive).** Mark OK'd working in
  the live files. **Reframed the editor as a chrome panel** (like the comms pane, built
  in views.rs with the `text_field` DSL) rather than a `PaneContent` leaf — no enum
  churn, no distributed render dispatch, much cleaner. A (lib.rs `Chrome`): added
  `knot_source: TextInput` + `knot_editor_open: bool` + `open_knot_editor` /
  `close_knot_editor`. B (views.rs): a `knot_editor_pane(c)` builder — a `text_field`
  lensed onto `knot_source` (class `knot-editor-source`, the focus key) in a
  `knot-editor-pane` panel with a title + close, mirroring `comms_pane`; wired into the
  chrome children behind `c.knot_editor_open`. `cargo check -p meerkat` green (1m21s,
  0 errors; my code 0 warnings — the lone new warning is a pre-existing unused
  `session_runtime::ShellbarEdge` import in his in-flight views.rs). Remaining to make
  it visible + editable: a `Command::ToggleKnotEditor` (mirror `ToggleComms`, with the
  Command-enum match arms); inline panel geometry in render.rs (comms/shellbar set
  their rect via `set_attribute` each frame); and the focus/caret wiring (a
  `FocusedField::KnotEditor` in ime.rs + the input.rs caret_field, so the field paints
  a caret and takes IME — basic click-to-type already rides the existing text_field
  DOM-focus path). The editor model (`KnotEditor`) then drives highlight + preview.
- **2026-06-25, host shell steps 1-3 (visible editable panel) + committed.** Finished the
  chrome-panel editor end to end, all additive in the live files: a
  `Command::ToggleKnotEditor` (verb `knot_editor`, label, `ALL` bumped to 33, `menu_scope`
  Always, command_drain no-op, lib.rs handler + `toggle_knot_editor`) opens it via the
  palette or the `>knot_editor` omnibar shell; `knot_editor_pane` renders an
  absolutely-positioned styled panel; `FocusedField::KnotEditor` (ime.rs) + the input.rs
  caret_field arm route caret / IME to the `knot-editor-source` field, so it edits through
  the existing text_field DOM-focus path. Two compile fixes the checker caught (the `ALL`
  array size 32→33; the `menu_scope` exhaustive arm). `cargo check -p meerkat` green (28s),
  no new warnings from my code. **`>knot_editor` now opens a docked, editable knot panel.**
  Committed my editor work to mere `main` in three batches (explicit pathspecs, no
  attribution trailers, on top of Mark's `fa5f32a`): the `knot-editor-host` editor model;
  the meerkat panel; this plan. Mark's graphlet work (app_handler / graphlets / main /
  nav_sync / session_ops + the graphlet/tearout plans) left untouched; the serval-layout
  caret primitive was already committed. Follow-ons: a multi-line textarea (the field is
  single-line today), the highlight overlay + the rendered preview pane (both driving
  `KnotEditor`), and responsive geometry.
- **2026-06-25, multi-line textarea + command automation.** Swapped the editor field
  from a single-line `text_field` to `textarea_typed` (the `edit_multiline` handler +
  a `<textarea>` tag), so Enter inserts a newline and Up/Down move between lines, and
  taught the focus model (`input_under_class`, `is_text_input`) to accept `<textarea>`
  as well as `<input>` (a general fix, not editor-specific). Then, from Mark's question,
  removed the two add-a-command footguns the earlier wiring hit: `Command::ALL` is now
  `[Command; <Command as strum::EnumCount>::COUNT]` (auto-sized via strum, already in the
  tree; a forgotten variant is a compile error, so the explicit ordered-list obligation
  stays without a manual count), and `menu_scope` got a `_ => MenuScope::Always` default
  (new commands need no arm; only narrower selection/node/edge scopes are explicit).
  `cargo check -p meerkat` green, no new warnings. Committed my paths in two batches
  (textarea; command automation), leaving Mark's graphlet work untouched.
- **2026-06-25, carve syntax harvest.** Mark flagged
  [carve](https://github.com/markup-carve/carve), a post-Markdown markup with visual
  mnemonics. Added a *Syntax harvest from carve* subsection under Knot/clips/storage:
  keep djot's delimiters (carve's flagship `/italic/` fights URLs / paths / dates) and
  lift the graph-native semantics instead. Net-new are cross-refs that auto-fill display
  text from the target node's title, `@mention` as the missing sibling to the existing
  `[[ ]]` / `#` rewrites, and case-insensitive title resolution; `[[ ]]` wiki links and
  the `+` list continuation are already planned. Recorded the discipline: take the
  semantics, leave the sigils. Plan-only, no code.
- **2026-06-26, query blocks + agent nodes graduated.** Per Mark, folded two
  borrowed-ideas items into the editor plan's near-term node-kinds wave (beside the
  clip phases, not the deferred Phase 6): a `=query` polyglot block (a graph query
  rendered inline as a live, edge-config-filtered result) and an agent node (a knot
  node whose body is the query/policy and whose edges are the materialized result
  set). Grounded the `=query` block on an existing primitive: the gloss swatch is
  already designed to embed in a djot script block, so the block is a swatch bound by
  a fence. Added the *Query blocks and agent nodes* subsection, two ergonomics rows
  (Phase 4 / 5), and a phasing block; cross-refs the borrowed-ideas brief, the gloss
  design, and the graph-signals plan. Plan-only.
- **2026-06-26, highlight render + illume promotion (spun out).** Built serval's
  `styled_textarea` (per-range styled `<span>` runs, the Phase-2 highlight render
  surface; serval `6a3ceace`) and tinct's `syntax` palette (themed contrast-gated
  colours; tincture `03661ce`). The highlight core's promotion to a standalone sibling
  lexer crate (**illume**) and the full text-legibility architecture are spun out to
  their own plan, [illume text lexer plan](2026-06-26_illume_text_lexer_plan.md): the
  editor consumes illume (lexer: text → spans) + tinct (palette: role → colour) + the
  serval styled field (renderer), with the host owning the `SyntaxKind` → `SyntaxRole`
  seam. Three editor-architecture fixes resolved there: #1 colours derive from tinct
  (never hardcoded), #2 one style-aware field body (not a styled fork), #3 `KnotEditor`
  becomes a stateless deriver so the host's `TextInput` is the single buffer. The
  remaining editor wiring (the bridge, the deriver) is tracked in the illume plan.
- **2026-06-27, editor reframe: note as a routed serval-document tile.** Converged
  with Mark across the session. The chrome-panel editor surface (the 2026-06-25
  expedient) and the document-canvas preview split (Decision 3) are superseded: a note
  is a locally-addressed knot document inker routes to a serval-document tile, rendered
  through serval-views + netrender (the same `ScriptedDom` → serval-layout → netrender
  path the chrome already uses), with the omnibar's `knot://` address-to-create as the
  new-note entry and `mere://welcome` teaching the scheme vocabulary. Added the Reframe
  section + the re-scoped slices (mapper → welcome tile; `knot://` routing; persistence;
  edit mode). Context: tinct 0.1.0 + illume 0.0.1 were published earlier this session
  (see the illume plan). Starting slice 1, the `EngineDocument` → serval-view mapper.
- **2026-06-27, slice 1 done + native-smolweb-plan alignment.** Built the
  document-family block→view mapper (`meerkat/note_view.rs`: `DocumentBlock` /
  `InlineSpan` → xilem_serval `el` / `text`, every block + inline-span variant, 3 tests;
  mere `0ab66a7`) and the render surface (`meerkat/note_surface.rs`: `note_scene` builds
  the views into a `ScriptedDom` via a `ServalAppRunner`, lays out, lowers to a
  `netrender::Scene` through the chrome's `scene_from_session` path; a test renders
  `mere://welcome` end to end; mere `3d7c7ea`). The
  [native smolweb rendering plan](../../nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md)
  (2026-06-27) frames this mapper as its **Phase D**, the document family: djot/knot,
  markdown, and reader-mode HTML all ride this one mapper (so it is not note-specific —
  its doc + eventual name should read "document-family"). The **smolweb family**
  (gemtext / gopher / feed / scroll / misfin) instead gets per-format native views in a
  new `serval/smolweb-views`, shareable with pelt because they avoid `DocumentBlock`; so
  the slice-1b content integration routes only document-family content through
  `note_scene`, never smolweb. Pending prerequisite from that plan: **`DocumentBlock::Table`**
  (the enum lacks it; both djot and markdown need it) lands as its own change — touching
  the mapper, the round-trip exporters, and every exhaustive `DocumentBlock` match —
  before the live djot/markdown tile.
