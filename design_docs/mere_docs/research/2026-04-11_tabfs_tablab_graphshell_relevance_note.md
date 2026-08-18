> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-18 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/graphshell_docs/research/2026-04-11_tabfs_tablab_graphshell_relevance_note.md`
> - Source commit: `401e2fcc`
>
> None of it was ever built as written. It is filed here as research, not as a
> plan and not as a statement of where Mere is going. It speaks the donor
> vocabulary (*Graphshell* the old browser product, *Verse* the retired network
> layer) and reasons about donor crate names, `graph-tree` and `graph-canvas`,
> that no longer exist under those names. Its relative links point at donor
> docs that are gone locally.
>
> Its sibling note of the same date,
> `2026-04-11_linked_data_over_middlenet_relevance_note.md`, was recovered into
> this folder in the same pass and cites this one; both were written as
> external-project relevance assessments in the same sitting.

---

# TabFS / TabLab Relevance Note For Graphshell

**Date**: 2026-04-11
**Status**: Research note
**Purpose**: Assess whether TabFS and TabLab materially inform Graphshell's
current plans, especially around `graph-tree`, `graph-canvas`, viewer/capture
surfaces, and future automation/agent workflows.

**Related**:

- `2026-04-10_ui_framework_alternatives_and_graph_tree_discovery.md`
- `2026-02-27_egui_wgpu_custom_canvas_migration_requirements.md`
- `../technical_architecture/node_object_query_model.md`
- `../implementation_strategy/graph/2026-04-10_vello_scene_canvas_rapier_scene_mode_architecture_plan.md`
- `../technical_architecture/graph_canvas_spec.md`
- `../implementation_strategy/subsystem_history/2026-04-02_browser_history_import_plan.md`
- `../implementation_strategy/2026-03-01_complete_feature_inventory.md`
- `../../verse_docs/technical_architecture/2026-02-23_verse_tier2_architecture.md`

**External references**:

- `https://omar.website/tabfs/`
- `https://github.com/jerlendds/tablab`

---

## 1. Short Answer

These projects are interesting and relevant to Graphshell, but mostly as a
**browser-surface / capture / automation bridge** idea.

They are **not** a strong reason to reshape either:

- `graph-tree`, or
- the core `graph-canvas` architecture.

They are much more relevant to:

- viewer/Verso capture surfaces,
- browser-state inspection,
- agent-accessible browser artifacts,
- archival and import workflows,
- and a future Graphshell-native "browser as queryable object space" bridge.

---

## 2. What They Seem To Offer

### 2.1 TabFS

TabFS's main idea is that browser tabs and related browser state can be exposed
through a filesystem-shaped interface.

That is useful as a **product metaphor** even if Graphshell never adopts a
filesystem mount literally.

The important idea is:

- tabs, pages, and browser actions become inspectable and scriptable through
  ordinary tools,
- browser state stops being trapped behind a UI-only chrome model.

### 2.2 TabLab

TabLab appears more technically relevant than TabFS for Graphshell.

Its current README describes:

- a Rust rewrite of TabFS,
- browser state mounted as a filesystem,
- open tabs, requests, cookies, console logs, and history entries exposed as
  files,
- and added layers such as DuckDB persistence, SQL queries, JS evaluation,
  WebSocket transport, REST API, AI summarization, and WARC archival.

This overlaps with several Graphshell-adjacent interests:

- local-first browser-state capture,
- structured queryable browser artifacts,
- archive/export potential,
- and agent-readable browser surfaces.

---

## 3. Relevance To Current Graphshell Directions

### 3.1 Relevance to `graph-tree`

Low.

`graph-tree` is about:

- graphlet-native workbench/navigator structure,
- pane and tree projection,
- portable tree semantics.

TabFS/TabLab do not meaningfully inform that tree/workbench problem.

At most, they reinforce the general architectural preference for a portable,
framework-agnostic core with host adapters. But `graph-tree` already has that
direction without needing browser-filesystem inspiration.

### 3.2 Relevance to `graph-canvas`

Low to medium.

They do **not** change the case for `graph-canvas` as a custom canvas
subsystem. The canvas still needs to own:

- scene derivation,
- camera/projection,
- hit testing,
- interaction grammar,
- render packets,
- and backend selection.

What TabFS/TabLab do suggest is that `graph-canvas` may eventually visualize
more kinds of browser-derived state:

- request activity,
- console trails,
- cookie/session state,
- page summaries,
- imported archive artifacts.

That is an **input opportunity**, not a reason to alter the crate boundary.

### 3.3 Relevance to viewer/capture/automation

High.

This is where the fit is strongest.

Graphshell already has or plans:

- browsing history import,
- WARC/archive lanes,
- agent/automation surfaces,
- filesystem/source mapping ideas,
- viewer and capture workflows.

TabFS/TabLab are much closer to a future:

- browser-state bridge,
- viewer-side artifact export surface,
- or agent-readable live browser introspection API

than to a renderer or tile-tree abstraction.

---

## 4. What Graphshell Should Borrow

The best borrow is **the interface idea**, not necessarily the literal
implementation.

### 4.1 Strong ideas worth reusing

- browser state should be queryable, not trapped in UI chrome
- tabs/resources/logs/history can be modeled as inspectable objects
- ordinary tools and agents should be able to query browser state
- persistence/query/archive layers can sit beside live browser state
- a filesystem-like shape is a useful interoperability layer even if it is not
  the canonical internal model

### 4.2 Potentially useful capability families

If Graphshell pursues this lane, useful capability families would include:

- live tabs/pages
- network requests and responses
- cookies/session state
- console output
- DOM/script evaluation
- history entries
- archived captures / WARCs

Those capabilities line up well with future:

- import/capture flows,
- diagnostic and analysis surfaces,
- agent-facing browsing instrumentation,
- and durable node/archive enrichment.

---

## 5. What Graphshell Should Not Borrow

### 5.1 Not a core app model

Graphshell should not treat "browser as filesystem" as the canonical internal
truth model.

Filesystem shape is a good interop surface. It is not a good replacement for:

- graph truth,
- node/view state,
- scene composition,
- or reducer-owned behavior.

### 5.2 Not a `graph-canvas` dependency

TabFS/TabLab should not be treated as a dependency or architectural driver for:

- `graph-canvas`,
- Vello integration,
- Parry/Rapier scene architecture,
- or projected 2.5D/isometric rendering.

### 5.3 Not a security shortcut

Any Graphshell-native version of this idea would need stronger capability
boundaries than "just mount everything."

In particular:

- cookies/session state are highly sensitive,
- JS evaluation is an authority boundary,
- network and archive export are privacy boundaries,
- agent access must be explicit and auditable.

This makes the idea more naturally aligned with Graphshell's capability and
viewer/automation planning than with a casual plugin or mount abstraction.

---

## 6. Recommendation

Treat TabFS and TabLab as inspiration for a future **browser surface bridge**
lane.

That lane would sit conceptually near:

- Viewer / Verso,
- browser-history import,
- archive/WARC workflows,
- and agent-readable browser instrumentation.

It should **not** be folded into the immediate `graph-tree` or `graph-canvas`
programs.

### Recommended position in the current roadmap

- `graph-tree`: no direct action needed
- `graph-canvas`: no boundary change needed
- viewer/capture/archive/agent surfaces: genuine future follow-on candidate

If Graphshell pursues this, the first useful framing is likely:

- a Graphshell-native browser bridge with a capability API,
- optional filesystem/REST/query adapters for interoperability,
- explicit privacy and consent boundaries,
- and a clear split between live browser state and imported/durable graph
  artifacts.

---

## 7. Concrete Next-Step Option

If this idea becomes active work later, the right first doc is probably not a
canvas or tree doc.

It should be something like:

- `browser_surface_bridge_spec.md`, or
- `viewer_browser_state_bridge_plan.md`

with explicit sections for:

- capability surface,
- privacy/security model,
- live vs archived state,
- import-to-graph mapping,
- agent access patterns,
- and whether filesystem shape is just an adapter or a first-class host
  surface.

That would let Graphshell evaluate the idea seriously without muddying the
current `graph-canvas` and `graph-tree` architecture work.
