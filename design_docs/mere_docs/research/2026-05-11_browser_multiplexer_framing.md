# Browser Multiplexer — framing brief

**Date**: 2026-05-11
**Status**: Framing probe (post-Phase 2 Part 1; rev 2 after external critique)

> **Crate-name note (2026-06-09 audit):** the builder-facing multiplexer framing holds, but the implementation references are a gpui/`mere-host` snapshot: `mere-host`→`meerkat`, `mere-frame`→`shell/frame`, `graph-layout`→`orrery/arrangements`. Several §5 capabilities (sessions, panes, tear-out, frame layout) have since shipped; the typed-action-bus and SessionServiceRunner items did not (their plans are archived/superseded). Dated references below are historical record.
**Scope**: Names what Mere has become as it's been built — a **browser multiplexer**, structurally analogous to a terminal multiplexer (tmux, GNU Screen) but with graph-typed sessions instead of byte-stream sessions. Maps current capabilities to the multiplexer vocabulary, identifies what's missing for the model to be fully realised, and shows where cartography slots in as the display contract.

**Related**:

- [`2026-05-10_cartography_layer_brief.md`](2026-05-10_cartography_layer_brief.md) — the projection/display layer this brief leans on for the "panes are projections" framing.
- Phase 1 / 1B / 2 work landed in `crates/mere-host/src/` *(historical citation)* <!-- doc-audit: historical-path --> — see `bootstrap.rs`, `graph_registry.rs`, `pane_state.rs`, `tearout.rs`, `graph_switcher.rs`, and the `mere-frame` crate's `FrameLayout` + `GraphId`.

---

## Thesis

> **Mere multiplexes durable graph sessions. Engines are replaceable content producers. Hosts are attach clients. Graph truth is never browser profile state.**

Everything in this brief is either a consequence of that statement or a decision needed to make it real.

The framing is **builder-facing**, not user-facing. Users still see "graphs" and "windows"; the word "multiplexer" is for the people writing Mere.

---

## 1. The framing

A terminal multiplexer is structured around four nouns:

| tmux        | What it is                                                                  |
| ----------- | --------------------------------------------------------------------------- |
| **Session** | A long-lived process bundle. Persists across client disconnects.            |
| **Window**  | A top-level container within a session — a named tab.                       |
| **Pane**    | A subdivision of a window's screen, attached to one PTY.                    |
| **Daemon**  | The server process that owns sessions and lets clients attach / detach.     |

Mere has, without setting out to do so, grown the same shape — once you don't confuse the runtime entity with the durable session:

| Mere                          | tmux equivalent                 | What it is                                                                                                                          |
| ----------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `GraphSessionManifest` (§2)   | Session                         | Durable session identity + everything needed to restore one (graph refs, frame attachments, workers, profile bindings, policies).   |
| `Entity<Graph>` + memory      | (Session's process tree)        | Runtime working set the session is currently using. Created/destroyed across reattach.                                              |
| OS window                     | Client                          | A view into one or more sessions. Closing it doesn't end the session.                                                               |
| `FrameLayout` leaf (pane)     | Pane                            | A subdivision of a window, attached to one `(session_id, pane_content_kind, view_intent)`.                                          |
| `GraphRegistry` (in-process)  | Daemon (logical, in-proc)       | Resolves session identity → runtime entities. Today a `HashMap`; will become a manifest store.                                      |
| Tear-out gesture              | `move-window` / `link-window`   | Detach a pane and reattach in a new (or different) window/session.                                                                  |
| Cmd-N                         | `new-session`                   | Spin up a fresh session in its own window.                                                                                          |
| Graph switcher                | `choose-tree`                   | List every session; pick one to attach.                                                                                             |

What makes this **not just tmux-for-tabs** is that the session content is graph-typed (nodes, edges, projection state) instead of an opaque byte stream. Cross-session operations have real semantics — Phase 3's diff-or-fork question (§11.3) is meaningful only because the session content has structure.

## 2. Identity matrix

The single biggest weakness of an earlier rev of this brief was conflating `Entity<Graph>` with "session." A real multiplexer separates runtime identity from durable identity, and Mere needs the same. Naming the identities now prevents a painful retrofit later.

| Identity                | Owner            | Role                                                                                          |
| ----------------------- | ---------------- | --------------------------------------------------------------------------------------------- |
| `PersonaId`             | App scope        | Whose sessions these are. Drives default profile/UDF scoping (§5.4).                          |
| `SessionId`             | App scope        | Durable session anchor. Stable across restarts. v0: 1:1 with a root graph.                    |
| `GraphId`               | Session scope    | Graph truth identity. A session has *at least* a `root_graph_id`; sub-graphs may appear.      |
| `FrameId`               | Window scope     | Identity of a saved frame layout (already exists in `mere-frame`).                            |
| `PaneId`                | Window scope     | Identity of a leaf inside a `FrameLayout` (already exists).                                   |
| `ViewIntentId`          | Pane scope       | Identity of the persisted `(form_factor, scale, focus, filter, strategy)` bundle (§5.3).      |
| Surface ID              | Pane scope       | `verso-tile` / `platen` surface handle. Ephemeral; bound to a pane at attach time.            |

**Rule:** `GraphId` stays as **graph truth** identity. **`SessionId` wraps runtime/session identity** — workers, attached frames, engine pins, policy grants, restore receipts. v0 maps one `SessionId` 1:1 to one root `GraphId`; later phases let sessions reference sub-graphs and graphlets without changing this identity model.

This dissolves what was previously an "open question": the Phase 3 sticky-note dilemma was not "promotion vs. view-with-diff," it was **"does the sticky-note get a new `SessionId` or share the donor's?"** Much sharper. See §11.3.

## 3. `GraphSessionManifest` — the durable session contract

Persisting `Entity<Graph>` is not enough, and treating the in-memory `HashMap<GraphId, Entity<Graph>>` as authoritative is the architectural mistake to avoid. The session's **manifest** is the thing on disk; the runtime entities are reconstructed from it on attach.

Sketch (canonical fields; specifics negotiable):

```text
GraphSessionManifest {
    session_id:        SessionId,            // stable, content-independent
    root_graph_id:     GraphId,              // primary graph truth
    sub_graph_refs:    Vec<GraphId>,         // graphlets / linked sub-sessions
    display_name:      Option<String>,       // user-set; None falls back to derived
    persona_id:        PersonaId,            // ownership / default policy scope
    created_at:        Timestamp,
    updated_at:        Timestamp,
    storage_path:      PathBuf,              // <data_dir>/sessions/<session_id>/
    active_workers:    Vec<WorkerKind>,      // declared, not running — see §5.7
    attached_frames:   Vec<FrameId>,         // last-known window attachments
    engine_profile:    EngineProfileBinding, // §5.4 — persona/session/graph scoped
    policy:            SessionPolicy,        // capabilities granted to this session
    schema_version:    u32,                  // forward-compat handshake
}
```

The host writes manifest changes through a debounced sink; reads happen at startup (scan dir → reconstruct registry) and on attach/restore.

**Storage shape:** `<data_dir>/sessions/<session_id>/manifest.json` for the manifest itself (hand-inspectable, JSON), plus an Eidetic-backed store for the graph data in the same directory. Mixing substrates is fine — the manifest is small + human-editable; the graph is large + typed.

The manifest is also the **restore receipt source**: when something fails to restore (graph missing, worker can't start, engine route absent), the manifest plus session-scoped diagnostics (§8) is what tells the user what happened.

## 4. Why naming the framing matters

It does three useful things:

1. **Predicts the gaps.** A terminal multiplexer that didn't persist sessions across daemon restarts wouldn't be considered finished. By that standard, Mere's registry is incomplete — it's in-memory only. The framing surfaces this without anyone having to argue for it.
2. **Borrows decades of UX vocabulary.** "Named session," "detach," "reattach," "session preview," "kill session," "rename window," "broadcast to all panes" — these all have lived-in conventions we can pattern-match against rather than rediscover.
3. **Sets expectations for what's interesting.** The multiplexer model says session content is opaque to the multiplexer. Mere's model says it isn't — that's where Mere can go further than tmux ever did. The Phase 3 fork-or-diff question is the canonical example.

## 5. Current capabilities and what's missing

Status as of 2026-05-11 (post Phase 2 Part 1).

### 5.1 Sessions

- ✅ Multiple sessions in one process, keyed by `GraphId` (proxying for `SessionId` today).
- ✅ Sessions outlive the window that opened them.
- ✅ Any window can hold pane(s) attached to any session.
- ❌ **No durable manifest** (§3). App exit = session loss.
- ❌ No persona binding; everything implicitly belongs to a single user.
- ❌ No user-set session names.
- ❌ No "kill session" affordance distinct from "close panel."

**Next deliverable:** §3's `GraphSessionManifest` + on-disk session directories. This is the single biggest gap, and several other gaps (view-intent restore, named sessions, session previews, headless workers) depend on it.

### 5.2 Windows + panes

- ✅ Resizable splits via `FrameLayout` (horizontal / vertical, recursive).
- ✅ Per-leaf `graph_id` — each pane attaches to one session.
- ✅ Per-pane state (`PaneState::{Orrery, Workbench, Other}`) — camera, engine, tile manager.
- ✅ Summon-from-shellbar for workbench / gloss / apparatus / new-orrery.
- ✅ Close-pane button per pane, with orrery-cascade-close semantics.
- ✅ Chrome (shellbar + workbench tile strip) is independently rotatable.
- ✅ Saved frame layouts (per-window, on disk).
- ❌ No drag-to-rearrange of panes within the window's frame yet.
- ❌ No per-pane title customisation.

### 5.3 View-intent persistence (`ViewIntent` sidecar)

The multiplexer-model question "what is a session's persistable state?" has two answers that must be kept separate:

1. **Graph truth** — nodes, edges, content. Lives under `GraphId`; serialised through Eidetic.
2. **The view onto the graph** — scale, scroll, focus, filter, strategy. Lives per-pane, keyed `(session_id, frame_id, pane_id) → ViewIntent`.

The view side **does not live inside graph truth**. Two panes can hold the same graph at different intents without mutating each other's view. Cartography's `ViewIntent` is the persistable bundle:

```text
ViewIntent {
    form_factor:  FormFactor,     // OrreryRoot, WorkbenchSwatch, MinimapThumbnail, ...
    scale:        f32,
    focus:        Option<NodeKey>,
    filter:       Option<NodeFilter>,
    strategy:     ProjectionStrategyId,
    overlays:     Vec<OverlayKind>,
}
```

**Storage:** sidecar file under the session directory: `<data_dir>/sessions/<session_id>/views/<frame_id>/<pane_id>.json`. Cheap to load lazily on attach; cheap to rewrite when a pane's intent changes (debounced).

This makes Mere's session-restore *richer than tmux's*. tmux can only restore "the session existed." Mere can restore "the session existed, and you were looking at *this part* of it, with *these signals visible*."

### 5.4 Engine profile / site data boundary

Engines (Wry/WebView2, Servo via netrender, nematic, future others) own substantial state that **must not silently leak into graph truth**: cookies, permissions, cache, IndexedDB, localStorage. The multiplexer framing makes the boundary explicit:

> Graph truth is what the session knows. Profile state is what the *engine* knows about its content. The session references a profile binding; it does not own profile bytes.

**Tiered scoping (default → escalation):**

1. **Persona-scoped UDF.** All of a persona's sessions share one engine profile by default. Cookies, logins, permissions feel coherent across the persona's workflow. Path: `<data_dir>/personas/<persona_id>/engine-profiles/<engine_kind>/`.
2. **Session-scoped UDF.** When a session is marked isolated (compliance work, untrusted content, throwaway research), it gets its own UDF. Manifest flag: `engine_profile: SessionScoped`.
3. **Graph-scoped UDF.** Reserved for cases where a single graph genuinely demands sandboxing distinct from the rest of the session. Not a default.

For WebView2 specifically: explicit custom UDF placement per Microsoft's guidance (one persona UDF, escalate per session/graph only when needed). For Wry / Servo / nematic the same tiered model applies; specifics are engine-by-engine.

**Wry's role:** compatibility / fallback engine, not portable baseline. Per the Glass-HQ/gpui PlatformSurface direction, surfaces compose at OS level; Wry stays available where a different engine isn't viable. The multiplexer treats Wry as one of N pluggable engines, not the canonical one.

### 5.5 Session previews in the switcher (cartography hook)

The switcher today shows `"Graph e3c9a4f1"` rows. Cartography's `MinimapDescriptor` is the contract for "this is what this session looks like at thumbnail scale" — see §7. Plug-in:

- Switcher row gains a 200×120 thumbnail rendered from the session's projection.
- Clicking a row attaches in the current window (today) or in a new window (planned).
- Hovering enlarges to a multi-pane preview (tmux's `choose-tree` analogue).

Blocked on: cartography strategies materialising in `graph-layout`. Not on session persistence (§5.1).

### 5.6 Scriptable layouts

tmux ships predefined layouts (`even-horizontal`, `tiled`, `main-vertical`, ...). Mere has `FrameLayout` but no layout templates. Natural additions: `apply_layout("reading")`, `apply_layout("triptych")`, exposed via the palette and through the command bus (§5.8).

### 5.7 Headless sessions + `SessionServiceRunner`

A real multiplexer can run sessions with **no attached client at all**. For Mere this means background-fetching, background-indexing, background-intelligence work continues on a session even when no window shows it.

The earlier rev of this brief said "the kernel inherits the daemon role." That was wrong — the kernel should not own native networking or GPU/model runtimes; it stays a pure data layer.

**Right shape:** introduce a `SessionServiceRunner` capability. The manifest *declares* which workers a session wants running (fetcher pool, embedder, indexer, intelligence-signal producer). The runner is implemented by the host today and could move into a separate service process later (§5.9). Workers are processes/threads owned by the runner; the session manifest references them by kind, not by handle.

```text
SessionServiceRunner trait {
    start_worker(session_id, kind) -> WorkerHandle
    stop_worker(handle)
    workers_for(session_id) -> Vec<WorkerStatus>
}
```

This keeps the kernel cohesive and lets the runner choose its execution shape (in-process today, OS service later) without rewriting session semantics.

### 5.8 Typed action bus (precedes any IPC)

A multiplexer is a programmable substrate. tmux exposes this via an external command-line interface; Mere can do better by going through the bus *first*, IPC *later*.

**v0:** introduce a typed action bus with target scopes:

```text
Action {
    target: ActionTarget,  // App | Persona(id) | Session(id) | Frame(id) | Pane(id) | Node(graph, key) | Surface(id)
    kind:   ActionKind,    // typed enum — Open, Close, Split, Tearout, Navigate, Broadcast, …
    args:   ActionArgs,
}
```

Every keybinding, palette invocation, drag gesture, and future cross-pane operation dispatches through the bus. The same bus is what security (§7) and diagnostics (§8) attach to.

**v1+:** external IPC (named pipe / Unix socket) becomes a serialized shell over the same bus — `mere send-action --target session:foo --kind navigate --to mere://X` *is* a bus dispatch, just remoted. This keeps every automation pathway inside one permission spine and one diagnostic trace.

The existing gpui `actions!` types are the right starting place; the bus is the wrapper that adds target scoping + permission gating + diagnostics on top.

### 5.9 Process boundary — single-process logical daemon (today)

Not splitting today. v0 ships a **single-process logical daemon** — one OS process owns the registry, the manifest store, the runners, and every window. The manifest, action bus, and `SessionServiceRunner` are *designed* as if they could be remote, so a future daemon split doesn't rewrite session semantics — only the transport.

Decision deferred (not abandoned): a separate daemon process attached to by per-window clients unlocks "one of my windows crashed; the others didn't notice" + headless server deployments. Probably not v1.

### 5.10 Broadcast / cross-pane operations

Niche but worth noting: tmux can broadcast keystrokes to every pane in a window. Mere's analogue: "apply this filter to every orrery in this window," "navigate every workbench to the same URL." Useful for compare-across-graphs scenarios. The action bus (§5.8) is the natural delivery mechanism; security gating (§7) is mandatory because broadcast is the easiest way to leak across sessions accidentally.

## 6. Tear-out today — mapped to the framing

Three modes shipped in Phase 2 Part 1, re-described in identity terms:

| Action                              | SessionId behaviour             | GraphId behaviour                   | Donor change                              |
| ----------------------------------- | ------------------------------- | ----------------------------------- | ----------------------------------------- |
| `TearOutTileToNewGraphMinimized`    | New `SessionId`                 | New `GraphId`, node copied by URL   | None                                      |
| `TearOutTileToNewGraphVisible`      | New `SessionId`                 | New `GraphId`, node copied by URL   | None                                      |
| `TearOutTileAsStickyNote`           | **Same `SessionId` as donor**   | Same `GraphId` as donor             | Tile (open-binding) closes; node stays    |

The sticky-note's `SessionId` sharing is what makes Phase 3 (§11.3) a real architectural question: does fork-on-divergence change the `SessionId`, or does the session grow a sub-graph reference, or does it stay shared with a diff overlay?

## 7. Security principle

Multiplexers increase blast radius. A single action can now target every pane in a window, every session under a persona, every engine surface mounted into the application. Without gates, that's a leak waiting to happen.

**Principle:** Every cross-session and cross-engine operation routes through a capability-gated action on the bus (§5.8). Denials produce diagnostics receipts (§8). No code path mutates a session except through a bus action.

What this brief deliberately *does not* do: enumerate every capability gate. Several of the operations the gate catalogue would cover (broadcast across panes, external IPC, signing requests, clip capture, custom protocol routing) don't exist yet. A separate brief lands the catalogue alongside the action bus implementation.

The capabilities that are obvious today and should appear in v0 of the gate set:

- Cross-session attach (graph switcher; tear-out into existing session).
- Engine route override (force a non-default engine for a graph).
- Profile escalation (move a session from persona-scoped UDF to session-scoped).
- Worker start (any session manifest declaring a new worker kind).

Everything else accumulates as features land.

## 8. Diagnostics — session-scoped events

Diagnostics is not a phase-3 polish item. It's how the multiplexer model becomes *legible* — the user can ask "why did this session fail to restore?" and get an answer.

Required events (canonical names; emitted through the apparatus diagnostics buffer):

```
session.created           { session_id, persona_id }
session.restored          { session_id, restored_workers, restored_views }
session.restore_missing   { session_id, missing: [GraphId | WorkerKind | ProfileBinding] }
session.killed            { session_id, reason }
session.worker_started    { session_id, kind, runner }
session.worker_failed     { session_id, kind, error }
engine.route_chosen       { graph_id, address, engine }
engine.route_degraded     { graph_id, address, attempted, reason }
surface.attach_failed     { pane_id, surface_id, error }
permission.denied         { action_target, action_kind, capability, reason }
profile.udf_error         { persona_id, session_id?, kind, error }
view_intent.restored      { session_id, pane_id, intent_id }
view_intent.lost          { session_id, pane_id, reason }
```

Every restore + attach + permission path must emit one of these before declaring success or failure. The apparatus panel becomes the multiplexer's status line in the tmux sense.

## 9. Accessibility & automation

`uxtree` (built on accesskit) is the spine. The multiplexer model adds explicit requirements that need to be respected even if the implementation lands later:

- **Switcher keyboard navigation.** Arrow / page keys traverse the session list; Enter attaches in current window; Shift-Enter attaches in new window.
- **Stable labels.** Every session, frame, and pane carries a stable, screen-reader-meaningful label (preferring user-set names, falling back to derived ones).
- **Attach/detach as a11y actions.** Tear-out, summon, close, and kill are exposed as accesskit actions on the relevant nodes — assistive tech can drive the multiplexer without needing the pointer gestures.
- **Preview alt-text.** Switcher thumbnails (§5.5) carry a textual description derived from the graph (e.g., "27 nodes, last visited 2 hours ago, cluster: research").
- **Bus-routed actions.** AccessKit action callbacks route to the same action bus (§5.8) — there is one dispatch path, not two.

Detailed implementation plan lives in its own brief; this section is the requirements anchor.

## 10. Where cartography fits

Cartography is the **display contract** of the multiplexer model. The two layers compose without overlap:

```
                  ┌─────────────────────────────────────┐
                  │ Multiplexer concern                 │
                  │  - SessionId, GraphSessionManifest  │
                  │  - registry (logical daemon)        │
                  │  - windows / panes (FrameLayout)    │
                  │  - tear-out, attach, detach         │
                  │  - action bus, capability gates     │
                  └────────────────┬────────────────────┘
                                   │
                       per-pane (session_id, view_intent)
                                   │
                                   ▼
                  ┌─────────────────────────────────────┐
                  │ Cartography concern                 │
                  │  - LayoutStrategy                   │
                  │  - ProjectionRequest / Projection   │
                  │  - FormFactor, ViewIntent           │
                  │  - MinimapDescriptor                │
                  │  - Overlay vocabulary               │
                  └─────────────────────────────────────┘
```

Three places cartography becomes load-bearing for the multiplexer:

- **Session previews (`MinimapDescriptor`)** — §5.5.
- **Attach modes (`FormFactor`)** — same session, different display density (orrery-root / workbench-swatch / palette-preview / minimap-thumbnail). The pane render becomes `cartography::project(graph, signals, ViewIntent { form_factor, ... })`. No bespoke renderers per surface.
- **View-intent persistence (`ViewIntent`)** — §5.3. The persistable bundle that makes graph-session restore richer than tmux's.

## 11. Open questions

Down from earlier rev — several open questions were resolved by the identity split (§2). What remains:

### 11.1 Manifest storage substrate

JSON for the manifest itself is hand-inspectable and easy to evolve; the graph data goes through Eidetic. Open: do `ViewIntent` sidecars live as JSON too (simple, debuggable) or in an Eidetic store (typed, scales with many sessions)? Leaning JSON for v0, Eidetic if/when contention shows up. **Configurable, not hardcoded.**

### 11.2 Persona model boundaries

Personas appear in the identity matrix and the engine profile boundary, but the persona's own scope (one user / multiple users on one machine / multiple identities for one user) isn't pinned down. Multi-identity-per-user is the most likely answer (work persona vs. research persona vs. throwaway-probe persona) — but worth a dedicated brief before persona-aware UDFs ship.

### 11.3 Tear-out operations — resolved by tearout-operations brief

This rev's framing posed "promotion vs. view-with-diff" as three options. The resolution turned out to be **they're three coexisting operations, not three options:** **leaf** (UI handle, donor unchanged), **branch** (new graphlet in donor's graph), **fork** (new session + graph). The user picks at gesture time. See [tearout-operations brief](2026-05-11_tearout_operations_brief.md) for the trichotomy, gesture model (modifier-keyed drags + toast on ambiguous drag), and identity semantics.

The diff/consolidation concern is resolved by the [memory-tiers brief](2026-05-11_memory_tiers_brief.md): branch and fork state live in **short-term memory** by default; consolidation into eidetic engrams is an affirmative user gesture.

### 11.4 What lands before Phase 3

The framing surfaces a clear pre-Phase-3 sequence:

1. **§3 `GraphSessionManifest` + on-disk session directories.** Biggest single deliverable. Everything downstream depends on it.
2. **§5.3 `ViewIntent` sidecar.** Smaller; depends on cartography contracts being in place. Lives in [short-term memory](2026-05-11_memory_tiers_brief.md).
3. **§5.8 typed action bus.** Refactor of existing action handling; modest LOC; unblocks security gates + future IPC.
4. **Node-per-tile + lineage facet** ([tearout-operations brief §6.1](2026-05-11_tearout_operations_brief.md)). Reshapes `host_helpers::ensure_node_for_address_near` and `host_navigation::navigate_to` so within-tile traversal becomes lineage edges in graph-tree, not new mere-kernel nodes. Precedes Phase 3 because **branch** depends on the lineage facet being live.
5. **Phase 3 = leaf/branch/fork + toast.** Implements the trichotomy from the [tearout-operations brief](2026-05-11_tearout_operations_brief.md).

Phase 2 Part 2 (drag detection) maps directly onto the gesture model in the tearout-operations brief and can land in parallel with the manifest work.

### 11.5 Naming the framing externally

Should "browser multiplexer" show up anywhere users see? Probably not. Internally it's our organising metaphor; externally users see "graphs," "windows," and (when persistence lands) "sessions." Tmux's UX is the model: people don't need to know they're in "a multiplexer," they notice their stuff is still there when they come back.

## 12. Crucial decisions made by this brief

Decisions this brief commits to. Override in subsequent docs if reasoning changes; the point is they're decided now, not left ambient.

1. **`SessionId` is distinct from `GraphId`.** v0 maps 1:1; the type distinction is enforced from day one.
2. **`GraphSessionManifest` is the durable session anchor.** `Entity<Graph>` is the runtime working set, not the persisted unit.
3. **`ViewIntent` lives in a per-pane sidecar**, keyed `(session_id, frame_id, pane_id)`. Not inside graph truth. Not inside `PaneNode::Leaf` (which stays a structural skeleton).
4. **Engine profile defaults to persona-scoped UDF.** Session-scoped and graph-scoped UDFs exist; they require explicit opt-in.
5. **Wry is a fallback engine, not the portable baseline.** PlatformSurface composition is the canonical engine integration path.
6. **Diagnostics receipts are required on every attach / restore / engine-route / permission / worker transition.** Not optional.
7. **Typed action bus precedes external IPC.** IPC, when it ships, is a serialized shell over the bus.
8. **Single-process logical daemon today.** Manifest + bus + runner are designed as if they could be remote, so a future split doesn't rewrite session semantics.
9. **User-facing vocabulary stays at "graphs," "windows," "sessions."** "Browser multiplexer" is builder-facing only.

## 13. What this brief does and doesn't decide

**Decides nothing about implementation order beyond §11.4.** Concrete plans live in their own dated docs.

**Implies concrete follow-ups**, each its own brief or plan doc:

- `GraphSessionManifest` + session-persistence implementation plan (§3, §5.1) — [drafted 2026-05-11](../../archive_docs/2026-06-09_completed_plans/2026-05-11_graph_session_manifest_plan.md).
- `ViewIntent` sidecar + cartography wiring (§5.3, §10).
- Action bus + capability gate catalogue (§5.8, §7) — action bus [drafted 2026-05-11](../../archive_docs/2026-06-09_pivot_superseded/2026-05-11_typed_action_bus_plan.md).
- Switcher-thumbnail implementation (§5.5) — depends on cartography strategies in `graph-layout`.
- Persona model brief (§11.2).
- Tear-out operations brief (§11.3) — [drafted 2026-05-11](2026-05-11_tearout_operations_brief.md).
- Memory tiers brief — [drafted 2026-05-11](2026-05-11_memory_tiers_brief.md).
- Node-per-tile + lineage facet implementation plan — referenced by the tearout-operations brief §6.1; pending.
- `SessionServiceRunner` implementation plan (§5.7).
- Daemon-split research brief (§5.9) — research only, no implementation v1.

These get filed as they're picked up.
