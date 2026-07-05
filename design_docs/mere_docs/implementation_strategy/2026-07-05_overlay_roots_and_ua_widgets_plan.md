# Overlay Roots and UA Widgets — browser features as views

**Date**: 2026-07-05.
**Status**: design/direction (with Mark). No code yet. Two directives, one
substrate; this plan fixes the architecture and the build order.
**The claim being bought**: today a browser feature that touches page content
costs an actor protocol change + rect math + a compositor overlay + bespoke
input routing (find-in-page is the four-part receipt). With overlay roots, the
same feature costs a view function and app state. This is the change that most
reduces what building a browser feature costs.

## The two directives

1. **Overlay roots** (Mark, 2026-07-05): beyond chrome-roots and content-roots,
   add **overlay-roots**: xilem_serval subtrees anchored to nodes in a content
   document, positioned by the engine's own layout rather than by a
   coordinate-math overlay layer. Find-in-page highlights, reader mode,
   annotations, web element clipping, autofill chips, link previews, agent
   affordances all become ordinary views with app state.
2. **UA widgets in xilem_serval** (Mark, same session): the W3C knockout deleted
   form controls; rebuild them as UA shadow content rendered by the same
   xilem_serval control views the chrome uses (`text_field` / `select` /
   `slider` / `checkbox` / `radio` / `toggle` / `button` — all exist,
   `xilem-serval/src/controls/`). One implementation serves the omnibar and
   every web page, and the knockout-then-rebuild strategy gets its first
   rebuild in the layer where iteration is cheap and headless-testable.

**Directive 2 is the standards-aligned pattern, not a clever inversion**
(Mark, follow-up): browsers implement form controls as internal UA shadow
trees, and the standards direction doubles down on exactly that — customizable
`<select>` via `appearance: base-select` renders author-stylable base structure
in place of the native widget; form-associated custom elements +
**ElementInternals** define the contract for what any control owes the form
layer (value/state submission, validity, form reset/restore) and the
accessibility tree (default ARIA semantics). So ElementInternals is the
**checklist** for P3-P5's done-conditions, and **HTML-AAM** defines what each
rebuilt control feeds AccessKit. Building the shadow content out of
xilem_serval controls is the modern implementation shape with the contract
documents already written.

**Directive 1 decomposes into current specs too** (Mark, second follow-up):
overlay-roots are not a new root category needing new architecture. Find
highlights are the **CSS Custom Highlight API** plus `::target-text` — painted
ranges, no per-match DOM at all. Floating chrome anchored to content nodes is
**CSS Anchor Positioning** plus the **Popover API** and the **top layer** —
exactly the machinery browsers now use for their own UI. Isolation of an
overlay subtree from page styles is a **UA shadow root**. So the engine work
is: serval implements top layer, anchor positioning, custom highlights, and UA
shadow roots, with xilem_serval as their first and most demanding consumer.
Only the annotation *data* layer goes beyond rendering specs, and the **W3C
Web Annotation Data Model** exists there for interchange. Two dividends: the
knockout-then-rebuild strategy pays out (chrome needs fund spec subsets that
later serve page authors verbatim), and every engine piece has a conformance
target instead of an invented contract.

Both directives reduce to one engine capability: **a view subtree mounted
inside a document the view layer does not own** — style-isolated from the page
cascade, invisible to page scripts, laid out by the engine relative to a host
node, painted in-band, hit-tested first, events routed to host app state.
Directive 1 mounts it *beside* content (a UA-invoked popover in the top layer,
anchor-positioned); directive 2 mounts it *as* an element's rendering (a UA
shadow root; the element's box is the host box). "Satellite root" below is
internal shorthand for the shared mount; each slot kind is a named spec
subset, not bespoke machinery.

## Findings (code-grounded 2026-07-05)

- **Root topology today.** Chrome root: the host-side shell `ScriptedDom` +
  one xilem_serval runner (unified-document-host). Content roots: actor-side —
  `StaticDocument` for the static HTML lane, `ScriptedDom` for the
  DocumentScript lane (`content/mod.rs:32`, `handlers.rs:365`); the document
  lane retains host-side packets; smolweb has pelt's `SmolwebDocument`. Every
  feature that crosses roots today does it by coordinate math: find highlights
  ship match rects and composite host-side; the drop ghost, comms geometry,
  and the retired live-preview all rode the same pattern.
- **The dual already shipped.** `host_pool` (gnode-pool P2) is a container the
  view layer owns whose *children* the host owns, with a splice-safety test
  proving sibling diffing survives foreign children. An overlay root is the
  same contract inverted: a subtree the *view* owns inside a document someone
  else owns. The diffing-tolerance questions were already answered once.
- **The transport already shipped.** `DomMutation` batches stream and splice:
  serval's `BoxTree::graft_subtree` keeps a retained layout emittable across
  structural splices (shell paint plan, 2026-07-03), and the capture/replay
  plan defines the portable mutation vocabulary. A host-side runner can diff a
  view tree against a local mirror and ship the resulting mutation batch over
  the existing actor channel; the actor applies it into the content document's
  retained layout like any other batch. Cross-thread view diffing needs no new
  wire concept.
- **Layout-in-the-actor is the win, not the obstacle.** Because the content
  actor owns the page's `IncrementalLayout`, a satellite subtree laid out
  there rides everything for free: band emission (tall pages), scroll-lock
  (an anchored overlay moves with its anchor at zero host cost), zoom/DPI, and
  paint order. The host-side rect math these features use today re-implements
  exactly this, per feature, badly.
- **Style machinery for isolation exists upstream.** Stylo implements shadow
  trees; serval's cascade rides stylo. The satellite root needs a cascade
  boundary (own sheet set, page cascade does not cross) — a scoping problem
  stylo has vocabulary for, not greenfield.
- **The controls are real.** `text_field` (+ styled/typed variants, caret and
  IME through the same serval caret primitives the omnibar uses), `select`,
  `slider`, `checkbox`/`toggle`, `radio`, `button` — the exact set `<input>`,
  `<select>`, `<details>` need.

## Architecture

**Engine (serval): satellite roots.** A document can carry N satellite
subtrees, each attached to a host node with a slot kind:

- **Overlay slot** = top layer + Popover API semantics + CSS Anchor
  Positioning (subset): the satellite is a UA-invoked popover whose containing
  block derives from the anchor element per the anchor-positioning model,
  promoted to the top layer, dismissal and nesting per popover semantics,
  scroll-tracking the anchor. For document-wide surfaces (reader mode) the
  anchor is the root element.
- **UA shadow slot** = a UA shadow root: the host element's box is the
  containing block and the element's rendered content *is* the satellite (a
  `<select>` renders its control view; its popup is an overlay slot on the
  same element — the two spec subsets compose, exactly as `appearance:
  base-select` composes them).
- **Highlight slot** = the CSS Custom Highlight API + `::target-text`: not a
  subtree at all — host-registered ranges painted by the engine. Find
  highlights, selection-adjacent decorations, and annotation underlines use
  this; it is the cheapest tier and needs no runner.

Invariants, both slots: own cascade scope (UA/overlay sheets only); invisible
to page DOM APIs (the DocumentScript mirror and page-script reflectors never
traverse satellites — this is the **security boundary**: autofill chips must
not be page-readable); hit-test resolves satellites before page content;
satellites join the engine a11y tree (a rebuilt `<select>` is accessible by
construction); satellite mutations classify through the same
RepaintOnly/splice tiers as everything else.

**View layer (xilem_serval): remote runners.** A runner variant that targets a
satellite root in a document owned by another thread: it diffs against a local
mirror and emits `DomMutation` batches over the content actor's command
channel; anchored input events come back typed on the existing update pipe and
drive plain app state host-side. For host-owned documents (the document lane's
future, the shell itself) the runner mounts directly with no channel. Per
window, like the gnode pool: satellite state is per-`WindowView`; shared truth
stays in shared state.

**What a feature costs after this.** Find-in-page: a `Vec<MatchRange>` in app
state and a view function mapping matches to highlight views anchored at their
ranges — the worker keeps finding matches; the rect shipping, band math, and
composite layer all delete. Link preview: an overlay slot on the hovered
anchor mounting a card view. Annotation pin: an overlay slot on the annotated
node whose click opens the statement-bucket-backed editor. Autofill chip: an
overlay slot on the focused input, state host-side, invisible to the page.

## Phases (done-conditions, not dates)

- **P0 — engine spec-subset probe.** serval-layout grows the minimal slice of
  each spec: a top-layer entry anchored per anchor-positioning to a host
  element, carrying a UA-shadow-isolated subtree; plus a registered custom
  highlight painted over a range. Done when: headless tests prove (a) the
  page's own layout is byte-identical with and without the satellite (no
  reflow leak), (b) anchor tracking across scroll and an anchor-moving
  mutation, (c) page sheets do not restyle the satellite (shadow boundary),
  (d) emission lands in the correct band and top-layer paint order, (e) a
  registered highlight paints its range with zero DOM. Each test cites the
  spec section it subsets.
- **P1 — remote runner seam.** xilem_serval: runner-over-mutations against a
  mirror; content actor applies satellite batches via the graft path.
  Done when: a toy overlay view (counter chip anchored to a page node) runs
  from host app state against a live page in the actor, survives page
  mutations around it, and round-trips a click. The splice-safety analog test
  (page churn around a satellite; satellite churn beside page nodes) is the
  deliverable.
- **P2 — first real feature: find-in-page highlights via Custom Highlights.**
  The find worker's matches register as engine highlights (the highlight
  slot); no overlay views, no per-match DOM. Done when: parity with today's
  highlights (count, stepping via the active-highlight style, auto-scroll)
  with the render.rs match-rect compositing deleted, verified headed on a
  tall page. (This lands even cheaper than the overlay-view version first
  drafted — the spec decomposition's immediate payoff.)
- **P3 — UA shadow slot + the cheap widgets.** `<details>`/`<summary>` (pure
  toggle, no IME), `<input type=checkbox|radio>` via the existing control
  views. Done when: a fetched page's checkbox toggles, reflects into the
  page-visible checked state, and the widget is invisible to page scripts
  except through the standard reflected attributes.
- **P4 — `<select>`.** The control view in the shadow slot; its popup as an
  overlay slot on the same element (the popup machinery is P0/P1's, not new).
  Done when: a real page's select opens, picks, commits, and the popup
  overlays page content correctly inside a scrolled band.
- **P5 — `<input type=text>` / `<textarea>`.** The hard one last: `text_field`
  in the shadow slot, caret/IME through the serval caret primitives the
  omnibar already exercises. Done when: typing, selection, IME composition,
  and paste work in a fetched form at parity with the omnibar.
- **P6 — the feature wave.** Link previews, annotation pins (statement-bucket
  consumers), autofill chips, reader mode as a root-anchored overlay
  document. Each is now a view + state slice; each lands as its own small
  plan citing this one.

## Risks / gates

- **Page-script invisibility is load-bearing** (autofill security, and
  correctness: `querySelector`/`children` must not see satellites). The
  DocumentScript mirror seam (`handlers.rs:86`) is the enforcement point to
  test explicitly, not assume.
- **Cascade scoping**: satellites need their own stylist scope; verify stylo's
  shadow-tree machinery reaches serval's cascade path before P0 commits to a
  cheaper hack.
- **Anchor lifetime**: anchor removed → satellite unmounts (event to host);
  P1's churn test owns this.
- **Perf discipline**: satellite applies must classify RepaintOnly when
  value-only (the gnode-pool lesson); P0's emission test should assert the
  tier, and the shell paint plan's partition work is the measuring kit.
- **Threading**: one mutation channel per content actor already exists; the
  runner must batch per frame, not per state change, or chatty features (find
  with hundreds of matches) flood the channel. Batch-per-frame is the
  contract.
- **Scope discipline**: P3-P5 rebuild *rendering + interaction* of controls,
  not the full HTML forms model (form submission, validation, autofocus are
  separate, later, and mostly page-semantics work in serval proper). Use the
  ElementInternals surface as the boundary marker: what it names
  (submission value, validity, states, default ARIA) is the eventual
  obligation; P3-P5 explicitly record which of those each widget slice does
  and does not yet meet, and HTML-AAM names the AccessKit roles/states each
  control emits from day one.

## Open questions

- **OQ-1 — resolved (2026-07-05, spec decomposition): adopt the spec
  vocabularies outright.** Anchor Positioning, Popover/top-layer, Custom
  Highlight API, UA shadow roots — subset per phase, but named and shaped as
  the specs define them, so page-authored use later shares the machinery
  verbatim.
- **OQ-2**: satellite content in engrams/clips — when a user clips a region
  under an annotation pin, the satellite must be excluded from capture
  (provenance C-plans); where is that filter?
- **OQ-3**: can a DocumentScript mod *request* an overlay root (capability-
  gated agent affordances), or are satellites host-only in v1? Host-only v1;
  the capability shape rides the mod permissions model later.
- **OQ-4**: the smolweb/document lanes — same slot machinery over
  `DocumentRenderPacket`, or chrome-DOM overlays as today? Defer until the
  serval-lane version proves the model.

## Cross-refs

- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md) — the
  root topology this extends (kept in place as foundational record).
- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md)
  — ownership map; overlay roots slot into the Render/Interact stages.
- Gnode pool `host_pool` seam + splice-safety test (archived plan,
  2026-07-04 checkpoint) — the inverted dual of the remote runner.
- serval `docs/2026-07-02_dom_mutation_capture_replay_plan.md` +
  `BoxTree::graft_subtree` — the mutation transport + splice substrate.
- [xilem_serval_control_adoption_plan](2026-06-25_xilem_serval_control_adoption_plan.md)
  — the chrome-side control adoption this makes bidirectional.
- Archived [find_in_page_host_ui_plan](../../archive_docs/2026-07-03_completed_plans/2026-06-16_find_in_page_host_ui_plan.md)
  — the rect pipeline P2 retires.
- [petgraph_rdf_plan](2026-06-18_petgraph_rdf_plan.md) statement buckets — the
  annotation-pin backend.
- Serval W3C knockout strategy (project memory) — P3-P5 is the first
  knockout-then-rebuild rebuild, done in the cheap layer; P0's spec subsets
  (top layer, anchor positioning, custom highlights, UA shadow) are rebuilds
  funded by chrome needs that later serve page authors verbatim.
- **W3C Web Annotation Data Model** — the interchange format for the
  annotation data layer (the one piece beyond rendering specs), when
  annotations want portability; the statement-bucket kernel model is the
  native store, the annotation model a projection (same posture as the RDF
  profile).

## Progress

- **2026-07-05** — plan written from Mark's two directives (overlay-roots;
  UA-widgets-as-views), grounded against the tree: control-view inventory
  confirmed complete, `host_pool`/`graft_subtree`/mutation-stream seams
  confirmed landed, root topology and the four-part cost of today's
  cross-root features (find-in-page as receipt) documented. No code.
- **2026-07-05 (same session)** — restructured around Mark's spec decomposition:
  overlay-roots dissolve into top layer + Popover semantics + Anchor
  Positioning (overlay slot), UA shadow roots (isolation + UA widget slot),
  and the Custom Highlight API + `::target-text` (a third, DOM-free highlight
  slot — find-in-page got cheaper again). OQ-1 resolved to spec vocabulary
  outright; Web Annotation Data Model recorded as the interchange projection
  for the one non-rendering layer. Every P0 test now cites the spec section
  it subsets.
- **2026-07-05 — P0 first slice landed: the highlight slot (engine side).**
  serval-layout gained `highlights.rs` (css-highlight-api-1 subset: named
  registry, static byte ranges, engine-derived geometry; the v0 deviations —
  translucent over-ink painting, name-order priority, per-node ranges — are
  documented in the module header as deliberate cuts) and
  `IncrementalLayout::set_highlight` / `clear_highlight`, painted by
  `emit_paint_list` after content emission via the selection primitives, with
  the document scroll applied so fills land in the emitted band. Registration
  touches no style/layout state: repaint-only by construction. Two headless
  tests prove the P0(e) conditions: a registered range paints with **zero DOM
  mutations and zero relayout** (fragment plane + host rect bit-identical,
  content emission unchanged, clear restores parity), and geometry
  **re-derives across relayout** (a wrapped narrow layout moves the
  highlighted word's fill down with no re-registration). 242/242
  serval-layout lib tests green (serval `components/serval-layout`, one
  commit). Next: P2 wires the find worker's matches onto `set_highlight` in
  the content actor and deletes the render.rs match-rect compositing — or the
  P0 top-layer/anchor probe, whichever lane is quiet.
- **2026-07-05 — P2 engine prep landed; the rest paused on a live collision.**
  `find_text_ranges` split out of `find_text_rects` (caret.rs): find matches
  now exist as `HighlightRange`s before any rect conversion, the shape the
  registry registers directly (244/244 green, committed). The remaining P2
  engine piece — a `HighlightRegistry` on **`ContentLayout`** (the actor's
  HTML-lane retained layout is `ContentLayout`, not `IncrementalLayout`; the
  registry must live on both, appended band-shifted in `emit_band`) — plus the
  actor wiring (`Find` registers "find" + re-renders; a new
  `FindActive { index }` command re-registers the active-match highlight) and
  the render.rs match-rect deletion are **deferred**: lib.rs / paint_emit.rs /
  invalidate.rs are being concurrently rewritten (the `DomMutation::Moved`
  lane) and lib.rs changed underneath the edit. Resume when that settles; the
  design above is the resume map. Rects keep shipping as scroll/step metadata
  either way (the host still needs count + auto-scroll targets).
- **2026-07-05 — P2 landed (engine-painted find-in-page), count-path verified live;
  visual paint pending a content-render confirm.** Full chain wired: `ContentLayout`
  gained the `HighlightRegistry` + `set_highlight`/`clear_highlight`/`find_ranges`/
  `range_rects`, appended band-shifted in `emit_band` (serval, one commit). The content
  actor's `Find` arm now registers the matches as the `"find"` engine highlight (first as
  `"find-active"`) on its retained layout and re-emits; a new `FindActive { index }`
  command (threaded through the content-contract wire enums both directions) re-registers
  the active-match highlight on Enter/Shift+Enter. Host routing (`submit_find_query`,
  `step_find_match`, `toggle_find` close, `find_matches_for`) sends live actor-backed
  pages down the engine path and keeps the find worker + `render/compose.rs` rect overlay
  as the **snapshot-only fallback**; the compose block is now gated
  `!is_active(member)`. Rects still ship as `FindMatches` metadata for the count +
  auto-scroll. **Verified**: builds, find + contract unit tests green, and a headed run
  returned a live **1/25** match count through the actor path (proving request→
  find_ranges→range_rects→FindMatches→chrome count end to end). **Not yet visually
  confirmed**: the engine-painted fills on screen — the loaded page body wasn't
  compositing visibly in the capture (a content-card display matter, unrelated to this
  change), so the in-band highlight paint needs one more headed pass once a page renders
  visibly. **Closed headlessly 2026-07-05**: a `ContentLayout::emit_band` test proves both `"needle"` matches paint a fill in-band with zero DOM/no re-search, the fills band-shift with the content (a lower band moves them up by the band delta), and clearing restores the plain band's command count — a more durable proof than a screenshot (245/245 serval-layout lib tests green, committed). The on-screen confirm remains a nice-to-have once the content card renders visibly, but the engine paint itself is now test-covered. The render.rs rect-compositing was **not deleted**, only demoted to the
  fallback — deleting it waits on that visual confirm + the snapshot-lane decision (a
  snapshot card has no live layout to register highlights on, so the fallback stays for
  it). Concurrent-workstream note: 3 unrelated bin tests were red at commit time
  (graph_delta_log, wallet_pairing, a serval keyed.rs panic), none in touched files.
- **2026-07-05 — P0 overlay-slot probe landed (engine side; all 4 done-conditions
  headless-proven).** `serval-layout/overlays.rs` (top-layer + anchor-positioning +
  UA-shadow subset), `ServalPaintList::push_sublist(origin, sub)` (compose a satellite
  paint list under one transform to the anchor — fill-only in the probe; a text/image
  satellite adds the font/image side-table merge here later), and an `OverlayRegistry`
  on `ContentLayout` with `set_overlay(name, anchor, content)` / `clear_overlay`,
  appended in `emit_band` after content + highlights at the anchor's live
  `absolute_origin` (band-shifted). A satellite is produced by its *own*
  `lay_out_content` call with its own sheet, so isolation is structural. Four tests, one
  per P0 done-condition, all green (248/248 lib):
  - **(a) no reflow leak** — registering a satellite leaves the page's fragment count +
    the anchor's rect byte-identical; `clear_overlay` restores the exact pre-overlay
    command count.
  - **(b) anchor tracking** — the overlay's wrap transform sits at the anchor's top on a
    tall page, and a lower band shifts it up by the band delta (tracks content, not the
    viewport).
  - **(c) style isolation** — a page carrying `.card { color: red }` cannot recolor a
    satellite whose own `.card` cascade set a different colour (the page sheet was never
    handed to the satellite).
  - **(d) top-layer order** — the satellite's fill is the *last* rect in the composed
    stream, painting over every page stacking context.
  With the highlight slot (find-in-page, P2) and now the overlay slot, **P0 is complete
  on the engine side** for both slot kinds the plan defines. Remaining before real
  features: two small accessors added to `ContentLayout` (`fragment_count`, `node_rect`)
  for the anchor geometry a host needs; and P1's remote runner (host view diffs a
  satellite subtree over the actor channel) is the next architectural piece — the probe
  proves the engine seam it targets is sound.
- **2026-07-05 — P1 compose layer landed (text-capable overlays; the runner seam
  already exists).** The blocker between the probe and a *real* overlay feature was
  that link previews / autofill chips / the counter chip all carry **text**, and
  `push_sublist` was fill-only. Fixed: `push_sublist` now merges the satellite's
  font/image side-tables into the parent, and the merge is **index-free** — `DrawText`/
  `DrawImage` reference resources by key (`FontInstanceKey`/`ImageKey`), not by Vec
  index, so the satellite's commands stay valid verbatim and only unseen resources are
  appended (dedup by key). A P1 test composes a text satellite (`count: 7` chip) into a
  page that has its own text and asserts both faces survive and every composed glyph
  run's font resolves in the merged table (249/249 lib green, serval `d20c06d`). The
  runner half is **already in hand**: `xilem-serval::ServalAppRunner` builds a view into
  a `ScriptedDom` and `update(f)` re-diffs it against new state emitting `DomMutation`s —
  a live host runner over a satellite DOM. So the remaining P1 work is integration, not
  new architecture: a host `WindowView` satellite runner whose emitted `ScriptedDom`
  feeds `set_overlay(anchor, ...)` each state change, anchored to a page node. That
  integration lands meerkat-side (where the concurrent workstream is active), so it is
  the natural next session; the engine + compose seams it targets are now proven end to
  end. **Both slot kinds are now feature-ready on the engine side**: highlights consume
  in find-in-page today; overlays can carry a real laid-out, text-bearing, isolated,
  anchor-tracked, top-layer subtree.
- **2026-07-05 — P1 landed meerkat-side: the overlay-slot host seam, proven end to
  end.** The full pipeline now runs host app state → engine-composited overlay on a
  live page. Pieces:
  - **Actor seam** (`meerkat/src/content`): `ContentCommand::SetOverlay { name, anchor,
    content: ServalPaintList, .. }` / `ClearOverlay`, native-desktop-only
    (`#[cfg(not(target_arch = "wasm32"))]`). The actor arms `ensure_html_layout`, resolve
    the `OverlayAnchor` against the live document (`Root` → `document_element()`),
    register the satellite via `ContentLayout::set_overlay`, bust the scene fingerprint,
    and re-emit. Repaint-only by construction (the satellite is a pre-emitted list
    composited after content), so the page never reflows. The wasm content-worker
    command-transfer half gates to `wasm32` (overlays don't cross the serialized worker
    wire in v1 — a `ServalPaintList` would need the Scene transfer's font/image dedup),
    so the overlay commands need no `ContentCommandMessage` variant.
  - **Constellation seam**: `request_set_overlay` / `request_clear_overlay`, the
    overlay-slot counterpart of `request_find` (the host command API; live caller is P6).
  - **Host satellite runner**: `ViewPane::paint_list` (+ `PaneSession::paint_list`) —
    the reusable runner already used by every list pane now also emits the pre-lowering
    `ServalPaintList` an overlay wants (it rode `IncrementalLayout::emit_paint_list`,
    which also hit-tests, so the same primitive serves render + input). No new runner
    architecture: `ServalAppRunner` → `ScriptedDom` → `IncrementalLayout` → paint list,
    exactly the chrome path.
  - **Proof** (`meerkat/src/overlay_probe.rs`, the P1 done-condition end to end): a
    `CounterChip` satellite `ViewPane` over app state mounts as an overlay on a live page
    in a real content actor; a click round-trips through the full pipeline (host runner →
    `SetOverlay` → the actor's `set_overlay` → the band) — count 0→1 changes the composed
    glyph sequence — and clearing restores the exact baseline band. A companion actor
    test (`content/tests.rs`) isolates the pure no-reflow property (a text-free chip
    composites with the page's glyph runs byte-stable; clear restores the exact op count).
    All green (`cargo test -p meerkat --bin meerkat overlay`, 4/4).
  - **Deferred (by design):** the *live* placement of an overlay satellite into a
    `WindowView` frame — which page node it anchors to on screen, on-screen click
    hit-testing against the composited rect — lands with the **first real overlay feature
    (P6)**: a link preview or annotation pin supplies a real view + anchor, rather than
    wiring throwaway demo scaffolding for the toy chip now. The geometric survival half
    (anchor tracking across scroll bands + an anchor-moving mutation) is already the
    serval-layout P0 test's territory; the meerkat tests prove the actor integration and
    the host round-trip. `OverlayAnchor` is deliberately a role-named enum (v1: `Root`)
    so P6 adds `FindMatch` / `LinkAt` / a node handle without changing the command shape.
