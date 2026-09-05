# Workbench Component Plan

**Home:** this plan and its subject moved from genet to mere on 2026-09-03
(platform boundary plan, P2), landing in `mere_docs/implementation_strategy/`;
it moved again the same day into the new `cambium_docs/` area root Mark ruled
for the Cambium family. Workbench is at `crates/cambium/workbench`;
`components/workbench` below is genet's path before the move. The ruling below
calls Workbench "Genet's reusable workspace-organization component"; that was
true when it was written, and the boundary plan reclassed it as application
composition, which is Mere's. Nothing about what it owns changed.

**Date**: 2026-08-31
**Status (2026-09-04)**: W5 opened — Turnstone's panes become tiles, ruled by
Mark; S1 and S2 landed the same day; S3 is Turnstone's and waits on a push
of this repository and a pin bump. W1 through W4 are implemented and landed through
coordinated Genet, Mere, and product branches. W4 has captured native Pelt
acceptance and cancellation receipts, a headed Graphshell browser
save/mutate/reload receipt, and a durable Woodshed open-lane consumer with full
view and host receipts.
The temporary `genet-host-api::tile` compatibility module is now removed.
Pinned products that still need their own current-Genet port retain that work
outside the shared component contract.

## Ruling

Workbench is Genet's reusable workspace-organization component. It owns the
presentation-grade split tree, tab stacks, stable tile identities, arrangement
commands, and host effects such as a tearout request. It does not own browser
sessions, graph arrangements, source data, operating-system windows, or
projection definitions.

Pelt and Graphshell are parallel hosts of Workbench. Pelt attaches document
sessions, engine routing, history, and browser chrome. Graphshell attaches the
projection editor, inspectors, previews, and local or remote endpoint actions.
Mere's Forme remains durable graph-arrangement authority; Platen compiles a
Forme arrangement into a Workbench presentation and translates accepted edits
back through Mere policy.

The crate is named `workbench`. The visible Graphshell authoring tool uses the
plain UI name **Projection Editor**. The editor may be arranged by Workbench,
but Workbench itself stays reusable workspace furniture.

## Findings

### 2026-08-31: the reusable core already exists under the wrong owner

`components/genet-host-api/tile.rs` — since moved to
`components/workbench/lib.rs`, the shim removed — was a zero-dependency split/tab tree with a
shared reducer. Its documentation already says that Pelt owns its local tree
while Mere projects Forme onto it. `DropTarget::Outside` already identifies the
tearout boundary and deliberately leaves the tree unchanged for the embedding
host. The missing piece is a typed host effect and a crate home that matches the
contract's actual responsibility.

`pelt-core::PeltWorkspace` adds browser controllers and routing to that tree.
Cambium's `frisket` renders the tree and emits commands. Neither layer should
become the generic workspace owner.

Mere currently also publishes a package named `workbench`, but that package is
only an AccessKit projection over `platen::Workbench`. The reusable package
name should move here. Mere can keep the small projection function inside
Platen and use a compatibility type alias while its graph-specific layout type
is renamed.

## Boundaries

| Layer | Owns | Explicitly outside it |
| --- | --- | --- |
| `workbench` | Tile tree, tab stacks, split fractions, commands, validation, typed host effects | Documents, graphs, windows, persistence policy |
| Cambium `frisket` | Retained realization, hit regions, accessibility targets, command emission | Authoritative workspace state |
| Pelt | Browser content, controller lifetime, engine routes, history, native shell response | Generic workspace vocabulary |
| Mere Forme/Platen | Durable graph arrangement and projection into Workbench | Generic window organization |
| Graphshell Projection Editor | Projection draft, validation, preview, typed endpoint requests | Source authority and portable scene mutation |
| Native/web host | Window creation, tearout acceptance, storage choice, process lifecycle | Reusable reducer semantics |

`genet-host-api::tile` temporarily re-exported `workbench` during the migration
without a second definition. It is now removed: Genet host contracts import the
shared types directly, as do the audited migrated consumers.

## Phases

### W1. Establish the component

Move the existing tile vocabulary and reducer into `components/workbench`.
Add a `Workbench` state wrapper whose command result distinguishes an applied
tree change, an unchanged command, and a `TearOut` host request. An outside drop
must not remove the tile before a host accepts custody.

Validation:

- Existing reducer tests move with the contract and remain green.
- New tests prove that an existing tile produces a tearout request without a
  tree mutation and an unknown tile produces no request.
- `genet-host-api` imports shared Workbench types directly, without an alias.

Done when there is one implementation of every tile type and reducer.

### W2. Adopt from Genet hosts

Cambium Frisket and Pelt import the `workbench` crate directly. Pelt stores the
`Workbench` state wrapper while preserving its existing `tree()` and boolean
`apply()` compatibility methods. A richer apply method exposes host effects to
desktop shells without making Pelt create a window.

Validation:

- `workbench`, `genet-host-api`, Cambium, `pelt-core`, and `pelt-desktop`
  compile against one Workbench package.
- Existing Pelt workspace tests retain their behavior.
- A focused Pelt test observes a tearout request and unchanged controller
  custody.

Done when Pelt and Cambium no longer import tile types through
`genet-host-api`.

### W3. Adopt from Mere and found the Projection Editor

Move Mere's AccessKit-only `project_workbench` helper into Platen and retire
the extra package that previously held the `workbench` name. Rename the
graph-specific `platen::Workbench` implementation to `TileLayout`, retaining a
short compatibility alias while downstream products migrate. Platen imports
the new Workbench presentation contract directly.

Add a Graphshell `ProjectionEditor` component that owns an editor workspace
and a typed projection draft. Its initial panels are source, reading,
encoding, arrangement, interaction, preview, and provenance. Saving produces a
typed request for a host-provided sink; it does not write graph or endpoint
authority directly.

Validation:

- Platen round-trips Forme arrangement plus tree geometry unchanged.
- Graphshell can construct the editor, apply workspace commands, validate a
  draft, and hand the validated definition to a fixture sink.
- Graphshell's local/remote projection client remains independent of editor
  state.

Done when Graphshell hosts a real authoring component and Mere has only one
package called `workbench` in its dependency graph.

### W4. Host receipts and compatibility removal

Wire tearout acceptance in one native host and embed the Projection Editor in
one headed Graphshell surface. Prove a second non-browser consumer, preferably
Woodshed, can use Workbench with an open content lane. The temporary
`genet-host-api::tile` alias is removed after an audit confirms its real users
have imported `workbench` directly.

Validation:

- A headed native receipt tears a tile into a second window without losing
  content custody or focus identity.
- Pelt and one non-browser product serialize and restore their own workspace
  policy over the same core snapshot.
- A Graphshell-authored projection previews, saves, reloads, and retains its
  source/provenance account.

Done when the compatibility re-export is unused and the same component has two
heterogeneous headed consumers.


### W5. Panes as tiles: the compositing host

**Ruled by Mark 2026-09-04.** Turnstone's window panes become Workbench tiles:
a tab bar on every tiled pane (a one-tab stack *is* the sub-title bar), a
close × on each tab, panes stacked by dragging one onto another, and the
whole layout saved and restored. The ruling reverses Turnstone's A4 decision
of 2026-08-11 (`turnstone/design_docs/2026-08-08_pane_registry_and_graph_panes_plan.md`,
"The `TileTree` decision"), which kept a Turnstone-private topology because
the shared contract "cannot own mixed GPU/document/Cambium surface routing,
persistence, or stable pane identity." Mark's reading of that sentence is the
thesis of this phase: those three are the stack's to own, and the list is a
list of what the stack is missing. It also fires the revisit trigger the
frisket direction note left standing
(`genet/docs/2026-07-24_frisket_pane_component_direction.md`, follow-on §2:
"Revisit only if turnstone ever wants tabs"). Turnstone wants tabs.

**What the stack has, read 2026-09-04.** `workbench::TileTree` is N-ary
splits over leaf tab-stacks with a reducer (`Activated`, `Closed`,
`Dragged` to a stack, an edge, or outside, `DividerMoved`) and one host
effect (`TearOut`). A `Tile` is a stable `TileId(u64)`, a title, and a
`ContentSource`, whose `Open { kind, id }` tail already carries a
non-document lane verbatim. Collapse prunes empty stacks and single-child
splits, and leaves a one-tab stack a stack. `cambium::frisket` renders that
tree as a DOM frame — splits, dividers, a tab bar per stack with a close ×
per tab, one content hole per stack — and lends its hit-test walk
(`divider_target`, `tab_target`, `close_target`, `stack_target`,
`tab_drop_index`) to hosts that lay out and hit-test themselves. Pelt
composites documents into the holes. `cambium::tab_strip` is the strip the
Workbench pane already wears per cell; it has no close affordance.

**What Turnstone has that the stack lacks.** `SpaceBlueprint`: a float layer
(relative rect, pixel constraints, z-order, pinned, visible, dock targets,
transactional tear-out from a float — A5, receipt-proven), a normalization
policy, a `Grid` node (never constructed outside the model and its tests),
and serde on all of it. And the thing it lacks itself: at rest the compositor
still walks the older binary `PaneNode` tree from `frame.json`; the blueprint
is promoted from it on the first float and never saved.

**The renderer, ruled by Mark 2026-09-04: one.** The first draft of this
phase kept Turnstone compositing one surface per pane and added a strip
surface per stack. Mark rejected that for the host's own content: a Roster
is not mail arriving at the house, and treating it as a hole beside a
WebView keeps two renderers in the family. So Turnstone renders **one frame
per window**, and content crosses into it by one of three membranes,
chosen per tile from its `ContentSource`:

- **In-house Cambium content** (Roster, Inspector, Arrange, a nested
  workspace) is a *child view* in the frame's tree — same layout pass,
  scene, focus system and accessibility tree. The primitive is
  `cambium::Component` (a state-owning boundary: parent props in, local
  state retained, a typed event out, the child tree erased), with
  `PortableKeyed` carrying a pane's element and view state across a move
  between stacks, splits and windows, and `GenetMultiRunner` giving one
  state N windows. This retires Turnstone's per-pane retained-runner maps
  (A1) in favour of pane-keyed state under one runner.
- **Contributed Cambium content** (a port's pane, e.g. Distillery's
  read-only surface) keeps its own state and comes through the erased
  retained-surface trait `cambium::surface` froze on 2026-08-26,
  composited by rect.
- **A foreign renderer** (a document engine's page, the graph's GPU canvas,
  a WebView) is frisket's *hole*: the frame leaves a rect and the host
  composites. This is what the hole was always for, and nothing else.

Nested tiling falls out: a nested workspace is an in-house pane whose view
is another frame, one recursion rather than a special case.

Phases, each with its done-condition. S1 and S2 are stack work in this
repository; S3 is Turnstone's, and consumes them by git pin.

**S1. `workbench` carries persistence and floats.**
- A `serde` feature deriving `Serialize`/`Deserialize` on `TileTree`,
  `TileBranch`, `TabStack`, `Tile`, `TabAccent`, `ContentSource`, `TilePath`
  and the float types below. The crate stays free of a storage choice: it
  serializes, the host decides where.
- A float layer beside the tiled tree — `Workspace { tiled: TileTree,
  floating: Vec<FloatingTile> }` or the equivalent — ported from
  Turnstone's A5 semantics: `FloatingTile { tile, rect: RelativeRect,
  constraints, z, pinned, visible }`, events to float a tile, dock it (to a
  stack, beside a tile, or the tiled root), raise it, and tear it out from a
  float; each tile at exactly one station. `TearOut` stays the one host
  effect.
- `Grid` is not ported: nothing in Turnstone constructs one. Recorded here
  so its absence is a decision.
*Done when* a `Workspace` round-trips through serde with fractions, active
tabs, floats and z-order intact; the A5 station walk (tile → float → nested
split → tear-out → return → dock) passes as a `workbench` unit test with one
`TileId` throughout; and Graphshell, Pelt and Woodshed build unchanged.

*Landed 2026-09-04.* `workbench::float` — `RelativeRect`,
`FloatSizeConstraints`, `FloatingTile` (owning its `Tile` while it floats,
with `resolve` for the host's pixel rect), `FloatDockTarget`, `FloatEvent`,
`WorkspaceEvent`, and `Workspace { tiled, floating }` with `apply`, the
station rule (`contains`, refused duplicates), `take_floating` /
`take_tile` / `insert_floating` for a move between windows, and z
normalization. `TileTree` gained `empty`, `is_empty`, `take_tile`,
`split_beside` and `insert_tab_after`. The `serde` feature derives on every
state type, `Edge` included, and the crate still picks no storage. Deviations
from Turnstone's blueprint, deliberate: float events are their own enum
wrapped in `WorkspaceEvent` rather than new `TileEvent` variants, so no
consumer's exhaustive match breaks; the floating tile *owns* its `Tile`
(Turnstone kept a spec list beside the tree); and a tile arriving from
another workspace is inserted whole with its rect and pin rather than
re-created. 20 tests with the feature, 19 without, clippy clean; cambium,
graphshell, pelt-core and pelt-desktop build unchanged. Woodshed consumes
by pin and is unaffected until it moves.

**S2. Cambium: the strip closes, the frame takes a slot, and a
`workspace` composition wires them.**
- `tab_strip` gains an optional close affordance per tab, emitting a typed
  close the caller maps, with the ARIA name "Close <title>", and stopping
  propagation so a close never also activates — frisket's existing contract,
  moved to the strip so frisket's tab bar, the Workbench pane's cells, and
  every pane bar are one widget. The strip can mark its stack active,
  closing the "nothing marks which pane is active" gap Turnstone's plan
  names. frisket's `render_stack` is re-expressed on it.
- frisket gains a **slot resolver**: `frisket_with(tree, on_event, fill)`
  where `fill(&Tile) -> Slot` answers `View(child)`, `Surface(retained)` or
  `Hole`. A `View` is inlined as the stack's content child; a `Surface` and
  a `Hole` keep today's placeholder marked with the tile so the host reads
  the rect back. `frisket(tree, on_event)` stays as the all-holes form Pelt
  uses today.
- A new **`cambium::workspace`** composition, beside `command_surface` and
  `graph_canvas`: a `Workspace` from `workbench` plus a registry from
  content kind to slot, pane-keyed state behind `Component` boundaries,
  `PortableKeyed` identity, focus, the float layer's furniture, and the list
  of holes and surfaces the host composites this frame. A module, not a
  crate, per the frisket direction note's ruling that modularity, packaging
  and publishing are three decisions; Cambium is the home every host
  already depends on.
*Done when* frisket's tests and the Workbench pane's strip tests pass on the
shared strip; a close-× press reports a close and not an activation; Pelt's
tile surface is unchanged through the all-holes form; and a `workspace`
acceptance test stacks an in-house component beside a hole, moves the
component to another stack and then to a second projection, and finds its
local state intact each time.

*Landed 2026-09-04*, three commits by opus subagents from briefs, each
verified and committed by path because another lane was live in
`cambium-rootstock` and this directory's zoom plan at the time.
`dc5ff2b0`: `TabStrip::current`, `TabItem` (label, `data-tabkey`, inline
accent), `tab_strip_items`, `tab_strip_closable` (a `tab-close` control
whose click stops propagation and returns the caller's action).
`f0c7966b`: the strip's DOM core is `tab_bar_view`, generic over the host's
state, and frisket renders its bar with it; `Slot`, `SlotKind`,
`frisket_with`, `slot_kind`; stack content is `PortableKeyed` by `TileId`
and a test pins a view slot's `NodeId` across a drag between stacks.
`f0c7966b`'s vocabulary is **additive**: the elements wear both the shared
tokens (`tablist`, `tab`, `tab-close`, `data-tabkey`) and the `frisket-*`
tokens Pelt's sheet and lookups still read, so Pelt is unchanged; frisket
tabs gained the strip's roving tabindex and arrow keys. The core is named
`tab_bar_view` because `selection_bar` already exports `tab_bar`. Third
commit: `workspace_view`, `WorkspaceModel`, `composited_slots`,
`WORKSPACE_CSS`, `frisket_with_current`; the acceptance test runs two
`GenetMultiRunner` projections over one state and proves the plan's claim
in its honest form — element and component-local state survive a move
between stacks within a projection; a tile moved to a second projection
arrives fresh and the host's `TileId`-keyed state is what survives, which
the module header now states as the rule. cambium 198/198, pelt-desktop
64/64, pelt-core 6/6, graphshell builds.

Follow-ups owed, none blocking S3: a Pelt slice onto the shared bar names,
after which the `frisket-*` aliases and `TabBarNames` can go; frisket's
bar names and slot literals made `pub(crate)` so `workspace.rs` stops
duplicating them; a float's view child through `PortableKeyed` so a move
between the tree and the float layer keeps its element. And a workspace
finding: mere has no `rustfmt.toml`, so `cargo fmt` strips the `},`
match-arm commas cambium inherited from genet — a formatter run in this
crate now churns twenty-five files, and which style wins is Mark's call.

**S3. Turnstone's panes ride the tree.** (Owned by
`turnstone/design_docs/2026-08-08_pane_registry_and_graph_panes_plan.md`,
A4 as revised 2026-09-04; summarized here so the two halves read as one.)
- `PaneId` ↔ `TileId` one-to-one; `PaneContent`, graph binding and `PaneSpec`
  stay Turnstone's, keyed by `PaneId` beside the tree, never in it — the
  content-payload rule the frisket note set.
- Each window renders **one `cambium::workspace` frame**. Turnstone's
  registry maps each pane kind to a membrane: its own Cambium panes become
  `Component`s in the frame; contributed panes go through the retained-
  surface trait; the graph canvas and WebViews are holes the shell
  composites at the rects the frame reports. `pane.rs`'s binary walk, the
  per-pane runner maps in `shell/renderers.rs`, and `cambium_pane.rs`'s
  one-view-per-surface seam retire into that frame. Press on a tab
  activates, press on × closes *that* pane, drag a tab to stack, split
  beside, or tear out.
- The saved layout is the serialized `Workspace` (primary and lens spaces),
  replacing `frame.json`; the old binary tree is read once and migrated,
  because it is real user data, then not written again. `SpaceBlueprint`,
  `legacy_bridge.rs`, the binary `PaneNode` and its store retire.
*Done when* `a5_floating_pane.scn` and `physics_native.scn` pass unchanged
in intent, a new stack receipt opens two panes, stacks one onto the other,
switches tabs, closes one from its ×, restarts, and finds the stack where it
was; the accessibility tree is the frame's own rather than a stitched one;
and the graph pane's frame time is not worse than today's.

**Sequencing and the pin.** Turnstone consumes mere by git revision and the
family is git-first, so S3 cannot start against unpushed S1/S2: pushing mere
and bumping Turnstone's pin are Mark's steps between S2 and S3. The
`physics catalog` P4 native drag half waits on the same bump.

## Progress

- 2026-08-31: ownership ruling recorded after checking `TileTree`,
  `PeltWorkspace`, Cambium Frisket, Mere Platen/Forme, and Graphshell's current
  local/remote host. W1 and W2 started in an isolated Genet worktree because the
  shared checkout acquired concurrent edits during the review.
- 2026-08-31: W1 moved the sole tile vocabulary and raw `TileTree` reducer into
  published MPL-2.0 package `workbench`. During migration,
  `genet-host-api::tile` was a dependency-backed compatibility re-export. The
  `Workbench` wrapper returns a
  typed `TearOut` effect for a valid outside drop without changing the tree, and
  treats an unknown tile as unchanged.
- 2026-08-31: W2 migrated Cambium Frisket and Pelt core/desktop imports to
  `workbench`. `PeltWorkspace` now retains the wrapper, preserves `tree()` and
  boolean `apply()` behavior, and exposes a richer Pelt outcome so desktop hosts
  can observe a tearout request before deciding custody. Focused unit tests cover
  core tearout and Pelt controller custody. `cargo test -p workbench -p
  genet-host-api -p cambium --offline -j 1` passed (15, 9, and 179 tests), as
  did the focused Pelt custody test and Pelt desktop checks with and without the
  `livery` feature.
- 2026-08-31: W3 landed on Mere branch
  `codex/workbench-integration-20260831` at `565285020ca26dcc63dbc6c256f4b9372d503de2`.
  Mere pins this package at Genet revision
  `d25ef444d216cc71f6897d122c55a92530d5a6ca`, Platen projects directly into
  the shared types, and the former Mere `workbench` package is retired.
  Graphshell's Projection Editor hosts its seven tools as open-lane Workbench
  tiles while draft validation and persistence remain editor and host concerns.
  A standalone cross-repo harness passed eight focused tests. The full Mere
  workspace gate still encounters its older `genet-taffy =0.13.1` patch mismatch
  and workspace-wide resolver fan-out, which are outside this component slice.
- 2026-08-31: W4's native Pelt route now keeps one shared `RenderCore` and
  creates an OS-decorated secondary `WindowSurface` rather than booting a
  second device. A document-controller outside drop creates a hidden
  destination at that window's own size/DPI, composes and presents a
  source-owned frame, then transfers the stable `TileId`, controller, route,
  and model focus through `accept_tearout`; only then does it show/focus the
  destination. The accepted window redraws, resizes, routes pointer, key, IME,
  and focus input, and closes independently. Pre-accept cancellation,
  configure/present failure, and the rare acceptance race preserve source tree
  membership, controller custody, and model focus, then restore its primary
  geometry/DPI. Native surface-producer import is a Pelt-owned follow-up: it
  now accepts custody only after a typed shared-device import or cache-transfer
  receipt has composed the hidden destination. Secondary-window AccessKit is recorded below.

  The headed acceptance command was captured at exit 0:
  `pelt.exe --workspace-tearout-receipt --size 960x640
  ports/pelt/examples/workspace/p5-fallback/index.html`. Its stdout recorded
  `window=true redraws=3 size=960x640 tiles=1 tearout_receipt=true
  tearout_cancellation_receipt=false routes=2=genet.livery:document`.
  The remaining primary tile is correctly tile 2 after transferred source
  custody. The separate cancellation command with the same fixture exited 0
  and recorded `window=true redraws=2 tiles=2 tearout_receipt=false
  tearout_cancellation_receipt=true routes=1=genet.livery:document,2=genet.livery:document`.
  It proves that the real hidden native preflight was declined before
  `accept_tearout` and retained source custody. Computer Use separately
  captured the live interactive `Pelt — Pelt fallback receipt` window with two
  fallback tiles and splitter; its accessibility tree exposed both tab items,
  content regions, and the separator.
- 2026-08-31: Graphshell's authoring surface advanced after the initial
  `ProjectionEditor` boundary. Mere commits
  `7323c703bf3c989ce0fe8c240cd047a4bfbd2fcc` (headed editor surface),
  `f01dd1914937e4cf5bbc8cc1a052e26922ba54fc` (authoring loop), and
  `86eb4331f179862a4a5e8c02faea7f7b3af2e972` (keyboard guard) attach the
  editor to Graphshell's web entry point while retaining draft, save-sink, and
  endpoint authority in Graphshell. The earlier standalone cross-repo harness
  remains the eight-test component receipt. The headed browser receipt is now
  green at Mere `49f9b99ed52e543c992a1a45e599dc6760d2ab41`: the stripped
  `wasm32-unknown-unknown` bundle is 34,679,440 bytes with SHA-256
  `A6A43EA0D1FB510E9EEF897B6C9232BB66F5D1F5927DF54DADE3463F55D3DB85`,
  and the browser completed the full source/provenance save, mutate, and reload
  loop.
- 2026-08-31: Woodshed's final second-consumer branch
  `codex/workbench-consumer-20260831` is at `1611201`.
  `woodshed-views` owns a four-panel stable-ID `ContentSource::Open` workspace
  over existing Practice, Set, Related, and Settings surfaces. Its active
  panel is host-global across split-local stacks; divider fractions are
  validated; and its JSON workspace snapshot is embedded in the existing
  `PersistedSession` with a safe fallback. Woodshed translates the shared
  typed tearout request into a host effect and keeps product/persistence
  authority local. The root Parley patch is present, with lock pins for
  Workbench `d25ef444d216cc71f6897d122c55a92530d5a6ca`, Parley Genet
  `583266`, and `wasm-bindgen` `0.2.127`.

  The final locked view receipt,
  `cargo test -p woodshed-views --locked --offline -j1 --config …`, passed
  11/11. The exact host restore receipt,
  `cargo test -p woodshed-genet host_session_restores_the_workspace_policy
  --locked --offline -j1 --config …`, passed 1/1 after 15m16s. Formatting and
  diff checks passed. This closes the non-browser open-lane and Woodshed
  persistence evidence, but is not a Woodshed headed workspace receipt.
- 2026-09-01: Turnstone's direct Workbench consumer migration landed at
  `75af890`; its full offline `cargo check` is green. Hocket's direct import
  migration landed at `bc98cc8`, but its focused compile remains blocked before
  compilation by Hocket's pre-existing dependency on removed `genet-layout`.
  That is a separate product port, not a Workbench alias use.
- 2026-09-01: the remaining `genet-host-api::tile` compatibility module and its
  re-export test were deleted after the Genet source audit found no real alias
  consumers. `genet-host-api::settings` imports `workbench::SettingsRef`
  directly and retains its real Workbench dependency. W4's native Pelt
  acceptance/cancellation, Woodshed open-lane/persistence, and Graphshell
  wasm/browser-headed evidence are captured. The native surface-producer
  follow-up remains Pelt-owned and does not change the Workbench contract.
- 2026-09-01: the coordinated consumer-first remote landing completed.
  Woodshed `main` is `1611201c903`, Turnstone `main` is `75af89070bb`, and
  Hocket `main` is `bc98cc8ee83`. Mere's `main` advanced concurrently, so the
  Workbench branch merged it without conflicts before landing as
  `2f85051245f`; the incoming Djinn, Distillery, Pandect, and Castellan paths
  had no changed-path intersection with the Platen or Projection Editor lane.
  The earlier headed Graphshell browser receipt remains attached to feature
  parent `49f9b99ed52`. A repeated post-merge focused test was stopped while
  Cargo remained in workspace resolution before starting rustc, so it is not
  an additional green receipt. Audits of all four remote consumer `main`
  sources found no remaining `genet-host-api::tile` uses. Hocket's broader
  retired Genet patch/layout port remains separate from this migration.
- 2026-09-02: Pelt's native surface-producer tearout follow-up is implemented
  on Genet branch `codex/pelt-surface-producer-20260902`. A destination-owned
  `SurfaceTearoutImportReceipt` either imports the first D3D12 shared frame on
  the existing `RenderCore` device or provisionally transfers the source's
  already-imported cache for a reused resource epoch. The receipt waits,
  composes, and returns the resource to `COMMON` before `accept_tearout`; every
  cancelled, import-failed, configure-failed, or pre-accept close path restores
  the source cache, viewport, tree membership, controller/surface custody, and
  model focus. Later destination frames refresh the same receipt. The isolated
  Windows gate `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=C:\\t\\genet-pelt-surface-producer-target cargo test -p pelt-desktop --lib -j 1 --message-format=short`
  passed 53/53, including the D3D12 importer tests and retained-source tearout
  test; `git diff --check` passed. This is a native unit/compile receipt, not a
  new headed Scrying surface-tearout receipt. Secondary-window AccessKit was
  implemented in the parallel lane recorded next.

- 2026-09-02: Pelt's deferred secondary-window accessibility slice is
  implemented on branch `codex/pelt-secondary-accesskit-20260902`. Each
  accepted tearout owns a fresh `WorkspaceAccessibility` bridge, high-range
  child namespace, virtual focus state, and action map. The hidden preflight
  installs the adapter before reveal; every secondary redraw reprojects its
  stable tile/content aperture and document subtree, AccessKit wakes only that
  window, and closing the tearout drops its adapter with the window entry.
  Controller actions continue through Pelt's ordinary input and document
  session seams; Workbench remains unaware of OS accessibility. The focused
  Pelt suite passed 27/27. The headed workspace-tearout command exited 0 and
  logged independent `accessibility Installed` events with 20 primary nodes
  and 18 secondary nodes, followed by `window=true redraws=3 size=960x640
  tiles=1 tearout_receipt=true routes=2=genet.livery:document`. Physical
  screen-reader verification remains open.
- 2026-09-02: the deferred Graphshell post-merge test is closed on current Mere
  `main` `4d68c465e58`. The first native test link exposed a Windows archive
  limit: debug-heavy `libcanvas.rlib` was 4,266,610,132 bytes, MSVC rejected it
  with `LNK4003`, and the resulting link reported 94 unresolved Canvas
  symbols. Repeating the identical seven Projection Editor tests with
  `CARGO_INCREMENTAL=0` and `--config profile.test.debug=0` reduced that
  archive to 347,000,108 bytes; the executable linked and all 7/7 tests passed.
- 2026-09-02: the surface-producer and secondary-accessibility branches were
  merged as Genet `7092898fc02`. A cold isolated `cargo test -p pelt-desktop
  --lib --offline -j 1` passed 54/54. The combined headed tearout command also
  exited 0, installing independent AccessKit trees with 20 primary nodes and
  18 secondary nodes before reporting `window=true redraws=3 size=960x640
  tiles=1 tearout_receipt=true routes=2=genet.livery:document`.
- 2026-09-02: Hocket's separate current-Genet product port landed on Hocket
  `main` as `e8b4b137583`. It now uses `cambium-genet-winit-host`; Hocket keeps
  state, Firewheel ticking, workers, custom leaves, scenario capture, and
  update policy while the shared host owns lifecycle, layout, paint, input,
  and accessibility. The focused check passed, as did 55/55 tests after
  excluding `identity::tests::every_home_says_which_situation_the_user_is_in`.
  That assertion still fails because it requires the word `DPAPI`; its
  `identity.rs` is unchanged from Hocket's preceding `origin/main`.
- 2026-09-02: W4's surface-producer follow-up is complete on the final Genet
  integration at code parent `532f09c6f0d`, with Scrying pinned to completed
  upstream host-migration revision `5a8688476091`. The transfer rehosts the
  same producer only after destination preflight, preserves or restores the
  source cache on every ordinary failure, retains destination custody with a
  terminal diagnostic when the native migration result is indeterminate, and
  completes receipt state from secondary-window events. Accepted windows keep
  independent AccessKit projections and action routing.

  Windows focus evidence now distinguishes Winit top-level focus from the
  actual native focus identity used by a composition surface. W4 accepts a
  real Winit `Focused(true)` event, or one stable native sample in which the
  destination is the foreground HWND and the foreground GUI thread's keyboard
  focus is that HWND or its descendant. It still separately requires the
  transferred tile's destination tree, model focus, removed source custody,
  and a composed visible destination frame. Independent observations cannot be
  combined into a false pass, retries are bounded, and receipt-only completion
  exits directly from the secondary event path.

  `cargo test -p pelt-desktop --lib --offline -j 1` passed 64/64; the focused
  formatting and diff gates passed. The exact live Scrying acceptance command
  passed three consecutive headed runs at exit 0, each recording
  `window=true redraws=4 size=960x640 tiles=1 tearout_receipt=true
  routes=2=genet.livery:document`, with 20 primary and 10 destination AccessKit
  nodes. The matching Scrying cancellation command exited 0 with
  `redraws=4 tiles=2 tearout_cancellation_receipt=true` and retained route 1 as
  `scrying.web:surface:CompositedTexture`. The ordinary Livery acceptance also
  exited 0 with 20 primary and 18 destination nodes. Physical inspection has
  already shown two independent HWND accessibility trees and keyboard focus;
  a captured Narrator spoken-announcement receipt remains manual and open.
- 2026-09-02: This closes the Workbench plan's W4 requirement on Genet `main`
  and therefore satisfies the explicit pre-P2 prerequisite in the platform
  boundary and repository topology plan. These follow-ups do not touch that
  plan's four P1 mixed seams.
