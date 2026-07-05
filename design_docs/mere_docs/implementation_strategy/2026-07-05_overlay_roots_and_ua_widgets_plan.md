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

Both reduce to one engine capability: **a view subtree mounted inside a
document the view layer does not own** — style-isolated from the page cascade,
invisible to page scripts, laid out by the engine relative to a host node,
painted in-band, hit-tested first, events routed to host app state. Directive 1
mounts it *beside* content (anchored, out-of-flow); directive 2 mounts it *as*
an element's rendering (in-flow, the element's box is the host box). Shadow-DOM
semantics minus the authorial API.

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

- **Overlay slot** — out-of-flow, anchored: the satellite's containing block
  derives from the anchor node's fragment (the CSS Anchor Positioning model,
  subset). Painted above the anchor's stacking context, clipped and scrolled
  with the anchor's scroll container. For document-wide surfaces (reader mode)
  the anchor is the root element.
- **UA shadow slot** — in-flow, replacing: the host element's box becomes the
  satellite's containing block and the element's rendered content *is* the
  satellite (a `<select>` renders its control view; its popup is an overlay
  slot on the same element — the two slot kinds compose).

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

- **P0 — engine satellite-root probe.** serval-layout: attach a static
  satellite subtree (overlay slot) to an anchor in a laid-out document.
  Done when: headless tests prove (a) the page's own layout is byte-identical
  with and without the satellite (no reflow leak), (b) the satellite's
  position tracks the anchor across scroll and an anchor-moving mutation,
  (c) page sheets do not restyle the satellite, (d) emission includes the
  satellite in the correct band and paint order.
- **P1 — remote runner seam.** xilem_serval: runner-over-mutations against a
  mirror; content actor applies satellite batches via the graft path.
  Done when: a toy overlay view (counter chip anchored to a page node) runs
  from host app state against a live page in the actor, survives page
  mutations around it, and round-trips a click. The splice-safety analog test
  (page churn around a satellite; satellite churn beside page nodes) is the
  deliverable.
- **P2 — first real feature: find-in-page highlights.** Replace the rect
  pipeline with highlight overlay views; the find worker's matches become app
  state. Done when: parity with today's highlights (count, stepping,
  auto-scroll) with the render.rs match-rect compositing deleted, verified
  headed on a tall page.
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

- **OQ-1**: does the overlay slot adopt CSS Anchor Positioning's vocabulary
  (`anchor()`, position-area) outright so page-authored anchors later share
  the machinery, or a minimal internal contract first? Lean minimal-internal,
  named to allow the later alignment.
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
  knockout-then-rebuild rebuild, done in the cheap layer.

## Progress

- **2026-07-05** — plan written from Mark's two directives (overlay-roots;
  UA-widgets-as-views), grounded against the tree: control-view inventory
  confirmed complete, `host_pool`/`graft_subtree`/mutation-stream seams
  confirmed landed, root topology and the four-part cost of today's
  cross-root features (find-in-page as receipt) documented. No code.
