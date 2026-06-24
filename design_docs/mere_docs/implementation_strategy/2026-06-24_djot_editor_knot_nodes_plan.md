# Djot Editor + Knot Nodes + Web Clips Plan

**Status: planning (with Mark), 2026-06-24.** Scoped via a multi-agent code sweep
of the live workspace. Adds the *write* side to a knot/djot stack that is already
read-complete, plus an element-pick clip path into the graph.

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
writing surface. Build a djot editor that produces knot nodes you can edit and
render, keep knot the default note format while `sniff` keeps markdown, txt, and
other plain formats opening, and reuse the scrape and capture stack to pick an
element off a live page and land it as a web-clip knot node carrying a
`ClippedFrom` edge back to the source.

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
  Files: `crates/inker/engines/nematic/src/knot/djot.rs`, `knot.rs`
  (`split_frontmatter` / `apply_frontmatter`).
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
- **Existing text-edit substrate in the host.** `xilem_serval::TextInput` holds a
  committed buffer plus an inline IME preedit; `PaneSession::caret_rect` gives the
  caret screen rect; meerkat already wires IME, focus, and a painted caret across
  several chrome fields (omnibar, palette, comms).
  Files: `crates/meerkat/src/ime.rs` (focus map, `set_ime_cursor_area`),
  `input.rs`. The host runs the parley / vello / wgpu / winit stack already. So
  the editor extends a working text widget; it does not stand one up from zero.
- **Swatch pattern.** A graph element rendered as portable serval-laid-out DOM;
  its docstring anticipates future embeddings (facet pane, menu, djot script
  block, orrery card). Today it is the node face and shape surface.
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
  `Vec`. There is no caret-driven block-edit API and no incremental reparse keyed
  to the editor. The editor pane is net-new, though it extends an existing widget.
- **A reachable new-note entry.** Both engines are registered, but no user action
  creates a knot or opens one for editing. The in-the-wings audit names this one
  wire as the biggest gap.
- **Editable-note persistence.** Eidetic engrams are immutable (an edit makes a
  new hash) and the graph snapshot has no body slot. The mutable note body needs
  a home. **Decided: an inline body on `Node`** (see Decisions).
- **Element picker.** No hover-to-select UI exists. Today the path is whole-page
  fetch plus tag and role extraction.
- **Selector to fragment clip.** Nothing captures a chosen element's HTML subtree
  as a stored clip body.
- **Element-rect visual clip.** `acquire_frame` / `capture_snapshot_png` are
  whole-surface; nothing crops to a selected element's box.
- **Web-clip to node spawn plus provenance writer.** `build_clip_knot` produces
  knot text, not a node; nothing spawns a node from it or asserts `ClippedFrom`.
- **Richer djot span fidelity.** The current parse flattens emphasis and strong to
  plain text and does not nest links. Owned by the knot-evaluation plan.

---

## Architecture

### A. Djot editor surface (text to knot model to render)

The source of truth is the **djot source text**, never a re-serialized AST.
jotdown 0.10 is a streaming pull parser with no writer and no mutable AST, so any
"save the rendered blocks back to text" shortcut would silently drop djot the
engine does not recognize. The editor holds the text; the engine owns meaning.

Build the editing core on the host's existing parley-backed text widget
(`xilem_serval::TextInput`), generalized to a multi-line note pane, reusing the
caret, IME preedit, and focus seams meerkat already wires. On each edit, reparse
with `jotdown::Parser::into_offset_iter()` and use the `(Event, Range<usize>)`
byte spans to drive syntax decoration and to locate tagged blocks. jotdown
reparses fast enough per keystroke at note sizes. To save or render, the existing
pipe runs unchanged: text to `Engine::render(text/x-knot)` to `DocumentBlock`s to
the rendered output (and `blocks_to_djot` back out). v1 needs no block-edit API.
Wire `accesskit` through the widget for the control-UX a11y rule. The editor lands
in new files so neither `knot.rs` (289 LOC) nor `djot.rs` (571 LOC, near the 600
ceiling) grows.

### B. Knot as default note, polyglot blocks, other formats

Knot stays the native note format and djot stays the default grammar. The inline
alt-format blocks are two spec-blessed shapes the editor surfaces and routes by
jotdown attributes:

- **Attribute-tagged fenced div** (`{.mere-script lang=rhai}` or
  `{protocol=gemini}` then `:::`) when the body stays djot but is tagged for
  routing through `expand_fenced_blocks` or `evaluate_blocks`.
- **`=FORMAT` raw block** (` ```=html `, ` ```=svg `) when the body is an opaque
  alt-format payload handed verbatim to another engine.

Both hand the editor a format or attribute key plus an exact source span for
decoration, with no custom lexing. The editor makes those regions visible and
editable, and holds the trust rule: a SelfAsserted note you own runs its fences
per setting, received content renders inert source. The descriptor stays a
`CodeBlock` with the fence info string (the shipped decision in the polyglot
plans); the editor adds no new `DocumentBlock` variant.

Other formats ride `sniff`. `sniff_content_type` splits knot from markdown today.
Markdown opens through the CommonMark engine; `.txt` opens as a plain body. The
editor edits raw text regardless of format; the format picks which engine renders
the preview and which exporter saves. A markdown or txt note saves back in its own
format (`to_markdown` / `to_text`); converting to knot is an explicit user action.

### C. Web-clip extraction to knot node

Four moves, most primitives present.

1. **Pick.** A hover-to-select picker on the live scrying tile. Run
   `execute_script_with_result` with a `document.elementFromPoint` probe to
   resolve the element under the pointer and read its tag, id, class, and bounding
   rect. The serval selector engine is the matcher for selector-driven picks.
2. **Capture.** For the chosen element, pull its HTML subtree (plus
   `serval-extract` scoped to that subtree) for the **semantic tier**, and read
   its bounding rect to crop a region from `capture_snapshot_png()` for the
   **rendered tier**.
3. **Store.** Feed the captured blocks plus provenance to `build_clip_knot` to
   assemble a `.knot`. The structured side can also ride
   `ingest.rs` to `apply_contribution` to `Orrery::ingest_graph`.
4. **Render and node.** A clip becomes a **node**, not a card. Spawn a node whose
   body is the clip knot, render it as a new swatch kind, and assert a
   `ProvenanceSubKind::ClippedFrom` edge to the source node (the first live writer
   for that edge family). The rendered-texture tier carries the on-site look; the
   semantic tier carries editable, statements-bearing content. Register the
   pick-and-clip action as a command id per the command-registry plan, with a
   consent gate. The faithful HTML fragment tier (sanitized html5ever with site
   context) is the later fidelity rung, owned by the knot-evaluation plan; v1
   clips are semantic-tier plus an optional cropped texture. HTML render depth
   stays Serval's job, not nematic's.

### D. Storage and identity

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

## Phasing (done-conditions)

**Phase 1: editable knot, round-trips in memory.**
Done: an editor pane opens a `.knot` source string, edits it with caret,
selection, and IME (reusing the existing text widget seams), and on save calls
`Engine::render(text/x-knot)` to show the rendered blocks beside the source. No
highlighting, no persistence yet. The buffer round-trips text to blocks to
rendered output.

**Phase 2: highlighting, new-note entry, persistence, other formats.**
Done: jotdown `into_offset_iter` spans drive basic syntax highlighting; a
new-note command id creates a knot node; the editor saves its body to the inline
`Node` body store and reopen restores it; opening a `.md` or `.txt` file routes
through `sniff` to the right engine for preview while editing raw text; markdown
and txt save back in their own format. Carries the `Node` body plus
`PersistedNode` schema change and its snapshot round-trip test.

**Phase 3: polyglot inline blocks visible and routed.**
Done: the editor decorates attribute-tagged divs and `=FORMAT` raw blocks from
jotdown spans; protocol fences expand and ` <lang> eval ` fences evaluate under
host policy for your own notes (and render inert for received notes); wikilinks
and hashtags type live. Eval depends on a host evaluator being registered.

**Phase 4: semantic web clip to node.**
Done: a picker selects an element on a live scrying tile, captures its HTML
subtree plus extracted text and links via `execute_script_with_result`, builds a
clip knot with `build_clip_knot`, spawns a knot node, and asserts a `ClippedFrom`
edge to the source. Consent UI present, command-registered. First live writer of
`ClippedFrom`, so it gets its own assert plus snapshot test.

**Phase 5: on-site rendered tier.**
Done: the clip carries a cropped texture of the element's rect (from
`capture_snapshot_png`) rendered as a swatch kind, so the clip node shows the
element as it appeared on the page alongside its editable semantic body. Validate
the device-pixel-ratio and scroll-offset mapping between `getBoundingClientRect`
and the captured surface at runtime.

**Phase 6 (deferred): fidelity and federation.**
Done: clips can render a sanitized HTML fragment with site context (the
knot-evaluation HTML tier); shared notes publish to a `knot://` addressable or
engram path (the `AddressKind` variant added here); emphasis, strong, and nested
links survive the parse round-trip.

---

## Crate decision

**jotdown source-text editing on the host's existing parley text widget.**

- jotdown 0.10 is a read-only streaming pull parser. No mutable AST, no writer.
  Treat the djot source text as the single source of truth and use
  `Parser::into_offset_iter()` spans to decorate it. Reparse on edit.
- The editing core is the existing parley-backed `xilem_serval::TextInput`
  (buffer, caret, IME preedit, focus), generalized to a multi-line note pane.
  Render through the host's vello / wgpu path. `ropey` is optional if large notes
  need it; `accesskit` for a11y. Confirm whether `TextInput` generalizes to
  multi-line or whether a multi-line sibling on the same parley layer is the
  cleaner net-new piece (Open question 1).
- No turnkey djot editor crate exists in Rust. [`jotdown`](https://crates.io/crates/jotdown)
  is parse-only. `egui_commonmark`, iced's markdown widget, and cosmic-text plus
  glyphon each couple the editor to a foreign toolkit, against the portable
  mere-domain and vello-host line. The host's own parley widget is the
  toolkit-agnostic fit.

---

## Decisions

Resolved with Mark 2026-06-24:

1. **Body storage: inline body on `Node`**, deferring `knot://` and the eidetic
   publish path to the federation phase. The `knot://` `AddressKind` variant is
   added only when its resolver is wired.

Open:

1. **Editor widget reach.** Does `xilem_serval::TextInput` generalize to a
   multi-line decorated note pane, or is a multi-line sibling on the same parley
   layer the cleaner build? Resolve by reading the widget before Phase 1.
2. **Editor crate placement.** A portable editing core (text buffer plus jotdown
   span decoration) split from host-side render glue, versus a single host module.
   Lean: keep the toolkit-agnostic core off the host dependency where the parley
   coupling allows.
3. **Picker mechanism and surface.** `elementFromPoint` hit-test versus
   selector-driven pick versus both; live scrying tile only, or also
   serval-laid-out static pages. Lean: `elementFromPoint` on the live tile first,
   selector and static-page picks as a follow-on.
4. **Clip default tier.** Semantic-only by default (Phase 4) with the cropped
   texture opt-in until Phase 5 proves the crop path, versus always capturing
   both.

---

## Risks

- jotdown has no writer and no mutable AST, so a byte-faithful round-trip of
  arbitrary djot is impossible through the engine. The editor must hold the source
  text as truth; `blocks_to_djot` covers only the recognized block vocabulary.
- `djot.rs` is at 571 LOC against the 600 ceiling and `knot.rs` at 289. Editor and
  fidelity work land in new files, not appended here.
- The inline `Node` body is net-new kernel schema touching `Node`,
  `PersistedNode`, and the snapshot round-trip. It is the highest-blast-radius
  change and stays isolated to Phase 2.
- `ClippedFrom` is modeled and persists but has zero live writers. Phase 4 is its
  first writer, so the assert path and its snapshot round-trip need their own test.
- The element-rect crop assumes the JS bounding rect maps cleanly onto the
  captured surface's pixel space. Device-pixel-ratio and scroll-offset mismatches
  between `getBoundingClientRect` and `capture_snapshot_png` are a runtime failure
  mode to validate, not reason about statically.
- The trust rule must hold for clips: a clipped element from a received page is
  received content and must render inert, never evaluate. Easy to regress if the
  clip pipe reuses the own-note render path.
- The host has no prior parley-on-vello *multi-line edit* surface wired (the
  existing fields are chrome single-liners). The Phase 1 risk is the caret,
  selection, and IME geometry in the note pane, not the parse loop.

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
  owns the graph-outline-as-editable-knot payoff at its P4. The editor here is the
  shared writing surface that P4 also uses.
- [2026-06-23 node body face model plan](2026-06-23_node_body_face_model_plan.md):
  owns the node's Body and Face presentation. The clip swatch kind composes with
  it.
- [2026-06-21 command registry configurable menus plan](2026-06-21_command_registry_configurable_menus_plan.md):
  the new-note and clip actions register as command ids here.
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

Code-verified anchors from the 2026-06-24 scope sweep, kept for the next session:

- Both knot engines registered: `nematic/src/lib.rs:97-98`; djot is the routed
  default for `text/x-knot`: `inker/src/routing.rs:424-431`.
- jotdown 0.10 is a streaming `Parser` yielding `Event` with
  `into_offset_iter()` byte spans; attributes ride `Start(Container, Attributes)`;
  raw blocks carry a `=FORMAT` tag. No AST, no writer.
- Clip producer signature: `build_clip_knot(blocks, source, trust, note_kind)`
  plus `build_clip_knot_with_block_provenance` at `expand/build.rs:12,43`.
- `Node` carries no body; identity is `id: Uuid` plus `addresses`. `AddressKind`
  variants today: Http, File, Data, Clip, Directory, Custom (`address.rs`).
- `ProvenanceSubKind::ClippedFrom` at `edge_taxonomy.rs:186`, round-trips through
  `snapshot/from.rs` and `to.rs`, zero live writers.
- Host text-edit substrate: `xilem_serval::TextInput` (buffer + IME preedit),
  `PaneSession::caret_rect`, meerkat focus map in `meerkat/src/ime.rs`. Stack is
  parley / vello / wgpu / winit. No multi-line edit surface wired yet.
- inker ships no `BlockEvaluator`; the registry is host-supplied and empty.

---

## Progress

- **2026-06-24.** Scoped via a multi-agent code sweep (five mappers plus crate
  research, synthesis, adversarial verify). Verify pass corrected an early mapper
  claim that the djot engine was unregistered or experimental (it is registered
  and routed as default). Editor-infra mapper failed its structured-output cap;
  the `xilem_serval::TextInput` finding was filled in by hand and flips the Phase 1
  framing from cold-start to extend-existing-widget. Decisions taken with Mark:
  inline `Node` body for the live note path, defer `knot://`; write this plan.
  No code yet.
