# Graph View Curation and Interaction Plan

**Date:** 2026-08-03  
**Status:** C3's root-Canvas fold is landed; its Swatch proof remains pending.
C4 is complete: the source-time contract has journal-prefix and Git-authority
adapters, plus a real second source in Isometry's pre-log `GameSnapshot` and
authority `GameEvent` Codicil. Its Overmap Swatch has a Cambium slider that
selects disposable historical snapshots, preserves local curation, disables
world actions while historical, and returns to untouched live truth. The native
root Canvas has a painted source rail with pointer scrubbing plus Home/End and
Page Up/Page Down, rather than a fictional Cambium widget. Historical Canvas
and Swatch headed captures exist. The receipt matrix verifies every current
Canvas arrangement against a journal prefix and all seven public repository
arrangements against an available Git checkpoint, then verifies return to the
retained live source. Joined clients and pre-origin checkpoints remain live-only
until the session handshake carries a verified origin plus public log. C5 is
complete: Mer3ly publishes reduced, commit-pinned public history from
Graphshell through the former WebRender-wgpu fork and the current authority.
Its native headed smoke records ready desktop and mobile source playback,
arrangement changes at a past cursor, keyboard stepping, archive appearance and
closure, and Return to live.
C6 is complete: Mer3ly's versioned public fragment reopens the same historical
source, arrangement, and selection on desktop and mobile without private
references; Mere persists a versioned, content-addressed local live-view recipe
and lets its source owner explicitly refuse missing, stale, denied, or
unsupported requests; Graphshell carries an opaque record reference through its
participant gate; and a projection capture serializes validated Scenotime tables plus
each presentation-resource address, so a replay verifies identical tables and
visual bounds before rendering.

## Outcome

Make the same graph feel manipulable in a root Canvas, a bounded Swatch, and a
remote Graphshell projection:

- pull a node or group and let its owning solver respond;
- select, highlight, hide, isolate, and inspect individual relation cells;
- fold a selection or an explicit hierarchy into one summary object;
- scrub any arrangement through a source's available history;
- share the curated live view or capture the exact projected scene.

The first public temporal proof is the repository graph on mer3ly.net. Its
current graph already morphs among arrangements and refreshes GitHub
`pushedAt` metadata during Pages deployment. The new proof adds historical
snapshots, including the archived Graphshell node and the older WebRender fork
lineage, without pretending today's relations existed in old checkpoints.

This is one curation system with several hosts. It is not a universal graph
rewrite. Graph truth, per-view choices, historical source state, projection
geometry, and scene delivery retain separate owners.

## Boundary map (verified against the live checkouts 2026-08-03)

### Already live

- **Root pulling works.** `canvas/src/input.rs` crosses click slop, calls
  `seiche::Simulation::pin`, updates the node position, then `unpin`s on
  release so the solver settles around the gesture. `seiche::View` already
  carries local drag overrides.
- **Root relation cells work.** `canvas::edge_cells` fans, paints, region-picks,
  and point-picks `(source, target, RelationSelector)` cells. Canvas stores
  selected and hidden cells separately, and truth deletion remains a distinct
  command.
- **Durable local curation exists.**
  `session-runtime/src/view_intent_store.rs` persists a per-frame, per-pane
  record containing hidden relation cells, camera, focus, strategy, and mirror
  tiles. This is the live home to extend for local view state.
- **The graph replay primitive exists.**
  `graph-kernel/src/graph/journal.rs` provides an exported, tested append-only
  `GraphJournal` of attributed `CapturedDelta`s, ordered replay, replay from a
  sequence, forks, and save/load. Both live edits and replay use the
  graph-delta application path. Current references outside its own module are
  the graph re-export only; a session host has not yet installed and persisted
  it. C4 owns that wiring and must not report the primitive's unit tests as a
  live-session history proof.
- **Cambium has pointer capture and a continuous slider.** `on_pointer`
  delivers Down/Move/Up outside the original hit target. `GraphCanvasSwatch`
  already aligns native node buttons with its painted graph. It lacks node
  drag and relation-cell data.
- **Isometry is a real second Swatch consumer.**
  `isometry-views/src/overmap.rs` produces a Cambium `GraphCanvasSwatch`.
- **Grouping vocabulary exists.** Forme carries `Group`,
  `CollapsedGraphlet`, `MemberOf`, and `PinnedIn`. Sceno carries nested
  `Space`s and member `Region`s. The missing piece is a view-level fold with a
  stable identity and boundary-relation policy.
- **Portable scene time exists, but it means delivery time.** Scenotime's
  epoch, revision, stable tables, snapshots, and diffs describe revisions of a
  projected scene. They do not describe the source graph's historical truth.
- **Graphshell has the reverse path.** The protocol owns advertised actions,
  curation/domain/external effect labels, invocation, stale detection, and
  authority handoff. Its current target is item-only (`InstanceId`).

### Names that must stay distinct

There are two live `ViewIntent` types:

- `cartography::ViewIntent` is an immutable, per-projection request containing
  form factor, dimension, focus, filters, target size, axis values, and
  extents.
- `session_runtime::ViewIntent` is durable, per-pane curation.

This plan calls the second one **view state** in prose. Renaming either public
type is outside the first slices; adding a third type with the same name is
forbidden.

Likewise:

- **source time** selects a graph snapshot or checkpoint;
- **scene revision** lets a host apply Scenotime diffs;
- **arrangement** places the selected source snapshot;
- **animation** interpolates between two projected scenes.

A Timeline arrangement is one way to draw a graph. A time scrubber changes
which historical graph is drawn. Every arrangement can be scrubbed.

## Ownership

| Concern | Owner | Contract in this plan |
|---|---|---|
| Durable graph history | `graph-kernel::GraphJournal` or a source adapter | Opaque ordered cursors and snapshot-at-cursor |
| Git repository history | mer3ly authority build | Historical checkpoint graph, provenance, active interval |
| Per-call projection choices | `cartography::ProjectionRequest` / `cartography::ViewIntent` | Snapshot plus arrangement/lens inputs |
| Durable local curation | `session-runtime::view_intent_store` | Hidden cells, folds, pins, time cursor, camera, focus |
| Root physics | `seiche` plus Canvas | Pin, move, release, explicit persistent pin |
| Bounded graph control | Cambium `GraphCanvasSwatch` | Pointer/keyboard gestures and generic element events |
| Semantic grouping | Forme | Group/member/collapsed arrangement meaning |
| Portable projected output | Sceno | Items, routed relations, regions, spaces; remains intent-free |
| Projected-scene revisions | Scenotime | Stable table ids, snapshots, diffs, picking |
| Remote actions and authority | Graphshell protocol and participant gate | Advertise, invoke, validate, accept/reject/stale |
| Live-view sharing | producer/host | Versioned source reference plus proven view-state fields |
| Exact capture sharing | Chirograph projection capture | Content-addressed serialized snapshot plus presentation resources |

Sceno stays an output contract. A share recipe, history provider, gesture state
machine, or product command does not move into it. A Scenograph contract bump
requires a case that two consumers cannot express with the existing source,
instance, relation-table, region, space, and protocol identities.

## The interaction model

### Pull

The gesture has four semantic moments: begin, move, release, cancel. The host
maps screen coordinates into its own world and the product decides what moves:
one node, the current selection, or a group space.

- Root Canvas keeps Seiche as its solver.
- A Swatch emits element-motion events. Its consumer may update fixed
  positions, call a solver, or decline motion.
- Drag release returns to dynamic simulation by default. **Pin here** is an
  explicit command. A user setting may make release pin automatically.
- Keyboard parity uses Move left/right/up/down, larger modified steps, Pin,
  and Release. The native node target stays at least 44 CSS px even when the
  painted glyph is smaller.
- Pulling a Sceno group edits one `Space` transform. It does not rewrite each
  member's source position.

The common code extracted after the two proofs is the small drag state
machine and event shape. Solver policy remains outside Cambium.

### Edges

The UI has two faces:

1. **Relation strip.** A compact strip along the canvas edge lists relation
   families currently present. Each chip exposes Highlight, Hide/Show, and
   Isolate. Reset restores the view. Color and line style are presentation;
   the text label and state remain available to accessibility.
2. **Link card.** Selecting a drawn link opens a card with one row per relation
   cell between the endpoints. Each row shows direction, family/kind,
   provenance or evidence when present, and confidence/weight when present.
   Actions are Highlight, Hide in this view, and Inspect. Assert, Retract, and
   Delete appear in a separate truth section and use the existing domain-truth
   command path.

Temporary highlight, durable per-view visibility, and graph-truth mutation
are three different layers. The default visibility stack remains Forme's
`GraphDefault < GraphViewOverride < SelectionOverride`; this plan supplies
the missing consumers rather than replacing it.

At dense zoom levels, relation cells may bundle visually. Selection resolves
to the bundle first and the Link card exposes its cells. Bundling never
deduplicates source truth.

### Fold

Two commands are deliberately distinct:

- **Fold selection** makes an arbitrary selected set into one summary object.
- **Collapse descendants** follows a chosen hierarchical relation family and
  direction from one parent.

The second command never infers hierarchy from arbitrary connectivity.

A fold records a stable fold id, ordered members, source scope, and boundary
policy in view state. Internal relations disappear from the current
projection. Relations crossing the boundary attach to the summary object and
bundle by relation family with a count. Expand restores the members and their
stable placement anchors. Saving a group into graph truth is a later explicit
command, not a side effect of folding.

### Time

Every historical source implements the same behavioral contract without
sharing a storage format:

```text
extent() -> earliest/current cursor
ticks(window, density) -> labeled stable cursors
snapshot(cursor) -> graph truth plus source revision/provenance
follow_live() -> optional current-cursor updates
```

A cursor is opaque and stable within its source. For `GraphJournal` it is a
sequence. For Git it is a commit hash plus authored/committed time metadata.
Wall-clock labels are optional, because causal order is the stronger truth for
some sources.

The control is a continuous Cambium scrubber composed from the existing
pointer-capture and slider substrate. It adds tick labels, step back/forward,
play/pause, Return to live, and a visible historical-state badge. Keyboard
arrows step ticks; Page Up/Down step larger intervals; Home/End reach the
extent. Tick density, playback rate, checkpoint interval, and follow-live are
settings.

Scrubbing builds a temporary historical graph, runs the currently selected
arrangement, and lets Scenotime diff or animate the resulting scene. It never
mutates the live graph and never writes the viewed cursor back as current
truth. Restore from here is a separate, confirmable action using the existing
delta/engram boundary.

### Share

There are two user verbs:

- **Share live view** references a source and a historical/live cursor, then
  reprojects against graph content the recipient is allowed to read.
- **Capture this projection** publishes the exact Scenotime snapshot and the
  separately addressed presentation resources needed to reproduce it.

The live record is producer-owned and versioned. It grows only from fields
already proven in earlier slices:

```text
version
source reference and cursor/revision
scope
arrangement id and parameters
edge visibility and highlights
folds
explicit pins
lens and overlays
selection and focus
camera, when the sharer includes it
```

Small public records may live in a URL fragment. Larger records and every
record containing private source references use a content-addressed object
behind the existing disclosure and capability boundary. Secret material,
private graph content, local filesystem paths, and capability tokens are not
serialized into a URL.

## Build order

Each slice is independently landable. Work stops at its receipt wall before
the next public contract is designed.

### C0. Freeze the boundary and fixtures

**Files:**

- Mere: `crates/canvas/canvas/src/input.rs`, `edge_cells.rs`, `selection.rs`
- Mere: `crates/graph/graph-kernel/src/graph/journal.rs`
- Mere: `crates/system/session-runtime/src/view_intent_store.rs`
- Genet: `components/cambium/cambium/src/pointer.rs`, `slider.rs`,
  `graph_canvas.rs`
- Isometry: `crates/isometry-views/src/overmap.rs`
- mer3ly: `assets/repo-graph.js`, `crates/repo-graph`, authority/site builders

Add focused characterization tests where the baseline lacks one. Record two
small fixture graphs:

- a multi-relation graph with two cells on one endpoint pair and one explicit
  parent relation;
- a temporal graph with add, edit, relation change, foldable group, and
  removal checkpoints.

**Done when:** the existing Canvas drag/release and edge-cell selection tests,
GraphJournal replay equality, Cambium captured pointer drag, and Isometry
Swatch headed smoke are named as stable starting receipts.

### C1. Pull on Canvas and Swatch

**Mere:** adapt the existing Canvas drag path to a four-moment element-motion
state machine while retaining the exact Seiche behavior. Add explicit Pin and
Release commands and keyboard movement.

**Cambium:** add node pointer capture to `GraphCanvasSwatch`, large aligned hit
targets, and a generic callback/event carrying node id, phase, and normalized
position. Keep solver and graph mutation out of the component.

**Isometry:** consume that callback in Overmap. The proof may use a local
position override first; if Isometry wants physical settling, its owning
projection/solver supplies it.

After both consumers work, extract only the shared drag state machine. Remove
the replaced duplicate from Canvas or document why host coordinate conversion
must remain separate.

**Done when:** a node can be pulled with pointer and keyboard in the Mere root
Canvas and the Isometry Swatch; release and explicit pin visibly differ; a
drag never changes graph-truth fingerprints.

### C2. Curate relation cells everywhere

**Mere:** keep `EdgeCell` as the source-level identity. Wire the current
hidden-cell set through the full
`GraphDefault < GraphViewOverride < SelectionOverride` evaluation and persist
the per-pane override.

**Cambium:** replace endpoint-only `GraphCanvasEdge` with an edge record that
has consumer-owned stable id, endpoints, family/kind, visibility/emphasis, and
optional accessible label. Add aligned route hit targets and callbacks for
select/highlight; the consumer owns the Link card data.

**Scenotime/Graphshell:** use epoch-local `RelationId` for remote picking. Once
the local and Swatch proofs need remote edge actions, extend Graphshell's
item-only invocation target to a protocol-owned target enum covering item,
relation, and region. The endpoint maps the table id back to its source cell.
This changes the Graphshell protocol, not Sceno.

**UI:** land the Relation strip and Link card in the root Canvas first, then a
compact strip/card variant around `GraphCanvasSwatch`.

**Done when:** two relations on the same endpoints can be separately picked,
highlighted, hidden, shown, and inspected in Canvas and Swatch; remote table-id
round trips are proven before the protocol version changes; truth deletion is
still a separately labeled command.

### C3. Fold selection and explicit hierarchies

Extend durable view state with versioned fold records. Project a folded set
through Forme's existing grouping vocabulary and Sceno `Space`/`Region`
output. Do not add source-truth group nodes.

Order:

1. Fold selection in root Canvas.
2. Expand with stable member placement and undo/redo of the curation action.
3. Bundle/count boundary relation cells.
4. Prove the same projection in a bounded Swatch.
5. Add Collapse descendants, requiring an explicit relation family and
   direction selector.

**Done when:** fold/expand round-trips member identity and placement; internal
cells return; boundary counts are exact; changing the selected hierarchy
family changes the descendant set predictably; graph truth remains byte-for-
byte equivalent before and after view-state removal.

### C4. Add source time and the shared scrubber

Introduce the smallest source-time trait after implementing two adapters:

- `GraphJournal` sequence cursor to replayed `Graph`;
- Git checkpoint cursor to repository authority graph.

The exact trait name is decided from those implementations. It must be usable
without Scenotime and must not require wall-clock timestamps.

Build the Swatch scrubber from Cambium's existing continuous slider and pointer
capture. Bind it to `GraphCanvasSwatch`; the native root Canvas owns no Cambium
DOM, so it renders the same source-prefix selection as a painted rail with
pointer and keyboard controls. Keep arrangement selection unchanged while the
source snapshot changes.

If human-readable timestamps are needed for journal entries, add a versioned
metadata envelope or side index. Do not replace `CapturedDelta`, silently
change the existing journal wire format, or introduce the old proposed
`GraphMutation` enum.

**Done when:** the same fixture can scrub through journal sequence and Git
commit cursors; Radial, Timeline, and one non-grid arrangement all show the
selected historical snapshot; switching arrangements preserves the cursor;
Return to live resumes source updates; scrubbing leaves current graph truth
unchanged.

### C5. Publish mer3ly repository history

**Repository:** `C:\Users\mark_\Code\repos\mer3ly`

Extend the authority build to derive checkpoint graphs from actual historical
repository trees. The site artifact contains reduced public snapshots or
deltas, never a live Git checkout or GitHub token.

For each checkpoint:

- identify repositories active at that time;
- read that checkpoint's manifests and authority docs;
- derive node metadata and relations from those historical inputs;
- record commit/provenance and active intervals;
- run the same arrangement engine used for the current graph.

Include archived Graphshell as a historical node and the old WebRender fork
lineage where the public Git evidence supports them. The current public graph
continues to refresh reduced GitHub metadata at deployment. A historical
checkpoint remains pinned to its source commits rather than acquiring today's
status.

Ship the scrubber on desktop and mobile with large controls and a compact
date/status label. The current arrangement morph control and selected node
survive scrubbing.

**Done when:** the site can play several months of repository evolution;
archived nodes appear and disappear at evidenced checkpoints; relation changes
match historical files; current mode matches freshly generated authority; the
headed smoke suite records desktop and mobile playback, arrangement change at
a past cursor, keyboard stepping, and Return to live.

### C6. Share proven views

Serialize only fields landed in C1-C5 into a versioned producer-owned live-view
record. Implement local URL-fragment round trip for public mer3ly data first.
Then prove a Mere local-session content-addressed record. Finally carry the
same source-owned record or opaque reference through Graphshell, subject to
its participant gate.

Capture this projection serializes a validated Scenotime snapshot and addresses its
presentation resources. Loading either form reports missing source access,
stale cursor, unsupported arrangement, or unavailable resource explicitly.

**Done when:** a live link reprojects at the same historical cursor and
curation state; a projection capture reproduces the same stable tables and visual
bounds; a recipient without source authority receives a refusal rather than
redacted data disguised as an empty graph; private references never appear in
the URL fixture.

## Dependency gates

```text
C0 boundary fixtures
  -> C1 pull proof
  -> C2 relation identity and curation
  -> C3 fold uses stable item/relation identities
  -> C4 source time and scrubber
  -> C5 mer3ly historical proof
  -> C6 share only the fields that survived those proofs
```

C2 may begin after C0 alongside the host-local part of C1, but its public
Graphshell target change waits for Canvas and Swatch receipts. C4's two source
adapters can be developed together; the shared trait is written after both
shapes are visible. C6 remains last.

## Verification wall

Every slice records the evidence level honestly:

- **contract:** serde/version round trips, malformed/stale input rejection;
- **truth:** graph snapshot fingerprint unchanged by drag, hide, highlight,
  fold, scrub, arrangement morph, and sharing;
- **projection:** stable source ids rebind to stable item/relation/region ids
  within the declared epoch; replaying the same cursor and view state yields
  equivalent scene tables;
- **interaction:** pointer capture, cancel, keyboard parity, focus retention,
  44 px targets, and screen-reader labels;
- **headed:** Mere root Canvas and Isometry Swatch for C1-C4; mer3ly desktop
  and mobile for C5-C6;
- **remote:** Graphshell stale epoch/revision and unauthorized intent receipts
  for edge/fold/share actions when those slices reach the protocol.

Minimum command wall is selected per touched repo, plus `git diff --check`:

```text
# mere
cargo test -p mere-kernel -p mere-canvas -p session-runtime
cargo test -p sceno -p scenotime -p chirograph -p graphshell-client

# genet / Cambium
cargo test -p cambium

# isometry
cargo test -p isometry-views

# mer3ly
cargo test --locked
cargo test --manifest-path crates/repo-graph/Cargo.toml --locked
npm run smoke
```

The exact package names/commands are rechecked in each live checkout before
execution. A green unit suite is not reported as headed proof.

## Stop rules

- Stop a shared extraction until two real consumers use the behavior.
- Stop a Sceno or Graphshell public-contract change until local Canvas and
  Swatch proofs cannot express the requirement through existing identities.
- Stop historical relation playback at node activity if the checkpoint
  extractor cannot prove the old relation from old inputs.
- Stop fold work if a parent relation and direction have not been named.
- Stop a view action that changes graph-truth fingerprints.
- Stop sharing before the record has versioning, authority checks, and a test
  excluding private references from URLs.
- Stop after each slice's receipt wall. Do not begin the next abstraction to
  make an incomplete proof appear general.

## Non-goals

- Reviving the archived Graphshell checkout or its dead Scenograph pin
- Moving history or product intent into Sceno
- Treating Scenotime revisions as graph-history cursors
- Inferring parenthood from arbitrary edges
- Saving a fold into graph truth as an automatic consequence of collapsing it
- Applying today's repository relations backward through time
- Replacing Seiche with a Cambium-owned physics solver
- Designing one maximal share schema before its fields have working consumers

## Supersession and adjacent authority

This plan supersedes the operative substrate and scrubber sections of
[event_log_timeline_plan](2026-07-01_event_log_timeline_plan.md). That plan's
undo/restore distinctions remain useful, but its proposed `GraphMutation` log
and discrete `SliderSpec` premise are obsolete now that `GraphJournal` and
Cambium pointer capture are implemented.

It extends rather than rewrites:

- [swatch_primitive_plan](2026-06-27_swatch_primitive_plan.md), whose P6
  generic component was deliberately not extracted from two bespoke consumers;
  Cambium plus Isometry now provide a new, real second-consumer seam;
- [scenograph_0_0_3_release_plan](2026-07-24_scenograph_0_0_3_release_plan.md), especially
  the settled ownership of output identity, scene revision, protocol intent,
  and authority;
- [graph_write_path_migration_plan](2026-07-01_graph_write_path_migration_plan.md),
  which keeps truth mutation behind graph deltas;
- [swatch_primitive_design](../design/2026-06-27_swatch_primitive_design.md),
  especially truth versus per-instance curation and cells-as-edges.
