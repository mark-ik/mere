# `GraphSessionManifest` + session persistence — implementation plan

**Date**: 2026-05-11
**Status**: Implementation plan — pre-build
**Scope**: Replace `mere-host`'s in-memory `HashMap<GraphId, Entity<Graph>>` registry with a durable session model anchored by `GraphSessionManifest`. Introduces `SessionId` distinct from `GraphId`, on-disk session directories, and registry-rebuild-from-disk on startup. Single biggest gap identified by the multiplexer framing brief and the deliverable everything else (named sessions, switcher previews, view-intent persistence, headless workers, fork-on-divergence) depends on.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) — §2 (identity matrix), §3 (manifest sketch), §5.1 (sessions today vs. needed). This plan operationalises those sections.
- [`../research/2026-05-11_tearout_operations_brief.md`](../research/2026-05-11_tearout_operations_brief.md) — **fork** operation references manifest fields (`parent_session`); **branch** uses graph-tree graphlets and doesn't touch the manifest beyond session-level consolidation.
- [`../research/2026-05-11_memory_tiers_brief.md`](../research/2026-05-11_memory_tiers_brief.md) — manifest is long-term; session graph data is medium-term (disk-persisted, not engram-shaped); consolidation produces engrams referenced by `consolidated_engrams` on the manifest.
- Current registry: [`crates/mere-host/src/graph_registry.rs`](../../../crates/mere-host/src/graph_registry.rs).
- Current frame-layout persistence: [`crates/mere-host/src/persistence.rs`](../../../crates/mere-host/src/persistence.rs).

---

## 1. Goal + done conditions

**Goal:** when Mere quits and restarts, every session the user had is restored from disk. Closing a window doesn't lose the session; quitting the app doesn't either.

**Done when:**

- `SessionId` type exists in `mere-frame`, distinct from `GraphId`.
- `GraphSessionManifest` struct exists with serde + a `schema_version` field.
- Every session has a directory: `<data_dir>/mere/mere-host/sessions/<session_id>/`.
- On app start: that directory is scanned; for each manifest, the `Entity<Graph>` is rebuilt from its on-disk store; the registry is populated; diagnostics fire (`session.restored` or `session.restore_missing`).
- On graph mutation: the manifest's `updated_at` advances and the on-disk graph store is persisted (debounced).
- On app exit: a final flush ensures every dirty session is on disk.
- Creating a graph (Cmd-N, "summon orrery for new graph", future tear-out fork) mints both a `SessionId` and a `GraphId`, writes the manifest, registers in `ManifestStore`.
- Existing `FrameLayout` persistence keeps working; leaves' `graph_id` continues to resolve through the registry (which now sits on top of `ManifestStore`).
- Sticky-note tear-out continues to share the donor's `SessionId` (Option B from the fork-model brief; fork is its own future operation).
- Three new diagnostic events fire at appropriate times: `session.created`, `session.restored`, `session.restore_missing`.

**Explicitly NOT in scope for this plan** (each has, or will have, its own follow-up):

- `ViewIntent` sidecar — separate plan.
- User-settable session names — later UX work.
- Switcher thumbnails — blocked on cartography strategies.
- Action bus + capability gates — separate plan.
- Cross-process daemon — deferred entirely.
- Engine profile UDFs (cookies, etc.) — separate plan.
- `SessionServiceRunner` (worker manifest) — separate plan; manifest's `active_workers` field is reserved but unused in this deliverable.
- Migration tooling for existing on-disk frame layouts predating manifests — covered minimally here; not a full migration UX.

## 2. Identity changes

### 2.1 `SessionId` in `mere-frame`

New type, alongside `GraphId`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub uuid::Uuid);

impl SessionId {
    pub fn new() -> Self { Self(uuid::Uuid::new_v4()) }
    pub fn as_uuid(&self) -> &uuid::Uuid { &self.0 }
}

impl Default for SessionId {
    fn default() -> Self { Self::new() }
}
```

Mirrors `GraphId`'s shape. v0 of this plan: every `SessionId` maps 1:1 to a root `GraphId`. The two are distinct types from day one even when 1:1; later phases (sub-graphs, fork, multi-graph-per-session) rely on that separation.

### 2.2 `PaneNode::Leaf` — stay graph-keyed, not session-keyed

Important: `PaneNode::Leaf` keeps its `graph_id: GraphId` field. It does **not** gain a `session_id`. Reason: a session can have multiple graphs (root + sub-graphs); a leaf names which specific graph it renders. Session identity is resolved at the registry level — given a `graph_id`, the registry knows which session it belongs to.

This means today's saved frame layouts continue to deserialise unchanged. `#[serde(default)]` on the existing `graph_id` field stays.

### 2.3 Registry → `ManifestStore`

`GraphRegistry` is renamed conceptually (the runtime registry) and gains a backing store. New module: `mere-host/src/manifest_store.rs`. It owns:

- The map `HashMap<SessionId, GraphSessionManifest>` (manifests, in memory).
- A reverse index `HashMap<GraphId, SessionId>` (so the existing per-leaf `graph_id` lookups resolve to a session).
- Live runtime entities `HashMap<SessionId, Entity<Graph>>` (rebuilt from on-disk graph store).
- Methods: `load_all_from_disk(cx)`, `create_session(seed, cx)`, `get_graph_by_id(graph_id, cx)`, `mark_dirty(session_id)`, `flush_dirty(cx)`, `kill_session(session_id, cx)`.

`GraphRegistry` either becomes a thin facade over `ManifestStore` or is replaced entirely. Leaning toward facade-then-collapse: the facade keeps the diff small in this PR; the collapse happens after callers settle.

## 3. The manifest type

```rust
// in mere-host/src/manifest.rs (new module)

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphSessionManifest {
    pub schema_version: u32,         // currently 1
    pub session_id: SessionId,
    pub root_graph_id: GraphId,
    pub sub_graph_refs: Vec<GraphId>, // empty in v0
    pub display_name: Option<String>, // None → host derives from graph
    pub persona_id: PersonaId,        // see §3.1
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub parent_session: Option<SessionId>, // for fork (Phase 3 will populate)

    // Consolidation references (memory-tiers brief). v0: empty Vec
    // + None. Consolidation gestures populate these.
    #[serde(default)]
    pub consolidated_engrams: Vec<EngramId>,
    #[serde(default)]
    pub last_consolidated_at: Option<SystemTime>,

    // Worker manifest. v0: empty Vec. SessionServiceRunner plan
    // populates this.
    #[serde(default)]
    pub active_workers: Vec<WorkerKind>,

    // Engine profile binding. v0: PersonaScoped (default).
    // Engine-profile plan adds session/graph escalation.
    #[serde(default)]
    pub engine_profile: EngineProfileBinding,

    // Session-level capability policy. v0: SessionPolicy::default()
    // (no overrides). Capability-gate plan operationalises this.
    #[serde(default)]
    pub policy: SessionPolicy,
}
```

Adjacent enums declared minimally:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonaId(pub uuid::Uuid);

impl PersonaId {
    /// v0: a single "default" persona for everyone, fixed UUID.
    /// Persona model brief will replace this with real persona logic.
    pub fn default_persona() -> Self {
        Self(uuid::Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0001))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum WorkerKind {
    // Reserved for SessionServiceRunner. v0 has no variants.
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum EngineProfileBinding {
    #[default]
    PersonaScoped,
    SessionScoped,
    GraphScoped,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionPolicy {
    // Capability overrides. v0: empty.
    #[serde(default)]
    pub overrides: Vec<()>, // placeholder
}
```

The reserved-but-empty fields (`active_workers`, `engine_profile`, `policy`) are intentional: writing the on-disk schema now with `schema_version: 1` and `#[serde(default)]` on later additions means future plans can grow the manifest without bumping the schema or invalidating saved sessions.

### 3.1 Persona — v0 is a single default

The manifest references a `PersonaId`. v0 of this plan: every session belongs to the default persona (a fixed UUID). The persona model brief picks up multi-persona scope later. The field exists in the manifest from day one so the persona brief doesn't need a schema migration.

## 4. Storage layout

```
<data_dir>/mere/mere-host/
├── sessions/
│   ├── <session_id_1>/
│   │   ├── manifest.json
│   │   └── graph/         ← Eidetic-backed graph store (see §5)
│   ├── <session_id_2>/
│   │   ├── manifest.json
│   │   └── graph/
│   └── ...
├── frames/                ← existing per-frame-layout saves
│   └── <frame_id>.json
└── keymap.json            ← existing
```

Rules:

- One directory per session. Directory name = `session_id` UUID.
- `manifest.json` is small, JSON, hand-inspectable. Format matches the `GraphSessionManifest` serde-derive.
- `graph/` holds graph data. v0: Eidetic key-value store, identical to how eidetic already persists graphs elsewhere in the workspace. Specific layout decisions defer to eidetic.
- Killing a session moves its directory to `sessions/.trash/<session_id>/` (don't immediately delete; user might regret).

`<data_dir>` resolves via the existing `dirs::config_local_dir()` pattern Mere already uses ([`actions.rs:user_keymap_path`](../../../crates/mere-host/src/actions.rs)). Reuses that helper.

## 5. Graph data persistence

Manifest writing is a small JSON dump. Graph writing is the substantive piece.

**v0 choice:** use eidetic the same way the rest of the workspace persists graphs. Specifics:

- Each session's `graph/` directory hosts an eidetic store.
- A new `mere-host/src/session_graph_store.rs` module wraps the eidetic + `mere-kernel::graph::Graph` integration so callers don't touch eidetic directly. Methods: `load_graph(path) -> Result<Graph>`, `save_graph(path, &graph)`.
- Saves are debounced. The `ManifestStore::mark_dirty(session_id)` signal feeds a coalescing timer (default 1 second). On timer fire, the dirty sessions flush.
- On app exit (gpui's `cx.on_app_quit` hook, if exposed; otherwise `Drop` on a guard entity), a synchronous final flush.

**Eidetic specifics** are deferred to eidetic's existing patterns. If eidetic's graph persistence isn't yet a clean reusable surface for `mere-kernel::graph::Graph`, this plan turns into "either it is, or this plan owns extracting that boundary first." That sub-decision lands once the prep work for this plan begins.

## 6. Lifecycle

### 6.1 App start

1. `bootstrap::run` (instead of seeding one in-memory graph) constructs an empty `ManifestStore`.
2. `ManifestStore::load_all_from_disk(cx)`:
   - Reads every directory under `<data_dir>/.../sessions/`.
   - For each: load `manifest.json`; if valid, load `graph/` into an `Entity<Graph>`; emit `session.restored`.
   - If `manifest.json` is malformed: skip + emit `session.restore_missing { reason: "manifest_parse_error" }`. Do not crash. Leave the directory in place so the user can recover.
   - If `graph/` is missing or malformed: skip + emit `session.restore_missing { reason: "graph_missing" }`.
3. If no sessions exist (fresh install / first run), seed one default session containing the intro node (matching today's `bootstrap::open_host_window` startup path).
4. Open the first window pointed at the most-recently-updated session.

### 6.2 Session creation

`Cmd-N`, summon-orrery-for-new-graph, future tear-out fork all funnel through:

```rust
ManifestStore::create_session(seed: impl FnOnce(&mut Graph, &mut Context<Graph>), cx)
    -> (SessionId, GraphId, Entity<Graph>)
```

Effects:

- Mint `SessionId` and `GraphId`.
- Create the session directory; write `manifest.json`.
- Build the `Entity<Graph>`, run `seed`, persist initial state.
- Insert into in-memory maps.
- Emit `session.created { session_id, graph_id }`.

Existing `GraphRegistry::create_graph_seeded` becomes a thin wrapper that calls into `ManifestStore::create_session` and returns just `(GraphId, Entity<Graph>)` for backward-compat with current callers.

### 6.3 Mutation → dirty → flush

- The existing `cx.observe(&graph, |_| cx.notify())` pattern in `bootstrap.rs` is augmented: when a graph notifies, the manifest store marks its session dirty.
- A debounced background task (1s default; `manifest_flush_interval` config) flushes dirty sessions to disk.
- Manifest's `updated_at` advances on every flush.

### 6.4 Session kill

New action `KillSession(session_id)` (palette-exposed; not bound to a default shortcut to avoid accidents):

- Removes from in-memory maps.
- Moves directory to `sessions/.trash/<session_id>/`.
- Cascades to any open windows holding panels for that session: panels are closed; if the window has no remaining panels, the window itself closes.
- Emit `session.killed { session_id, reason }`.

### 6.5 App exit

Synchronous final flush before windows are torn down. If any flush fails: log + continue (don't block exit). Failed flushes mean a session's most recent edits don't survive; the diagnostic event records what was lost.

## 7. Wiring

Files affected by this plan, with the shape of the change:

- **`crates/mere-frame/src/lib.rs`** — add `SessionId` type. Unchanged otherwise.
- **`crates/mere-host/src/manifest.rs`** (new) — `GraphSessionManifest`, `PersonaId`, `WorkerKind`, `EngineProfileBinding`, `SessionPolicy`.
- **`crates/mere-host/src/manifest_store.rs`** (new) — `ManifestStore`, the in-memory + on-disk session map.
- **`crates/mere-host/src/session_graph_store.rs`** (new) — eidetic glue for graph persistence.
- **`crates/mere-host/src/graph_registry.rs`** — becomes a thin facade over `ManifestStore`, or is removed entirely once callers are migrated. Lean toward removal in this plan.
- **`crates/mere-host/src/bootstrap.rs`** — `run` loads manifests instead of seeding one in-memory graph; `open_host_window` resolves a session from the store rather than the registry.
- **`crates/mere-host/src/host_navigation.rs`** — `open_new_window` and `summon_orrery_for_new_graph` go through `ManifestStore::create_session`.
- **`crates/mere-host/src/tearout.rs`** — `tear_out_tile_to_new_graph` goes through `ManifestStore::create_session`. Sticky-note path unchanged (still shares donor; fork is future work).
- **`crates/mere-host/src/lib.rs`** — `HostRoot.registry: Entity<GraphRegistry>` becomes `Entity<ManifestStore>` (or stays as `GraphRegistry` if the facade approach is taken). `graph_display_name` reads from the manifest's `display_name` field (with fall-back to derived).
- **`crates/mere-host/src/graph_switcher.rs`** — switcher iterates manifests, shows display_name + last-updated timestamp.
- **`crates/mere-host-apparatus` diagnostics** — the three new event types added to the canonical event list (already enumerated in framing brief §8).

## 8. File-size discipline

Mere's 600-LOC ceiling is in active force. New modules' size targets:

- `manifest.rs` — ~150 LOC (types + serde).
- `manifest_store.rs` — ~400 LOC (in-memory map + disk I/O + lifecycle).
- `session_graph_store.rs` — ~200 LOC (eidetic glue).

If `manifest_store.rs` overshoots, split disk I/O into a `manifest_store/disk.rs` submodule.

## 9. Testing approach

**Unit:**

- `GraphSessionManifest` round-trips through JSON serde (sample fixtures + property test on UUIDs / timestamps).
- `ManifestStore::load_all_from_disk` tolerates malformed manifests, missing graph dirs, partial writes.
- `ManifestStore::create_session` produces well-formed on-disk artefacts (directory exists, manifest parses).

**Integration:**

- App-start scenario: pre-seed a fixture sessions/ directory; assert restored sessions match the fixture.
- Round-trip: create N sessions in one app run; quit; new app run sees all N.
- Corruption: drop a malformed manifest; assert it's skipped + diagnostic event fires.
- Kill: cascade-close window panels; assert directory moved to .trash.

**Tests do not run concurrently** (per project preference). Logs go to gitignored test-output dirs.

## 10. Migration of existing state

Mere has been used in development with only the in-memory registry. Today's persisted state is just frame layouts (`frames/<frame_id>.json`). Those leaves carry `graph_id` UUIDs that won't exist in any new manifest.

**Strategy:** on app start, if `sessions/` is empty but a saved frame layout references nonexistent `graph_id`s:

- Mint one default session (intro-seeded, exactly as fresh-install behaviour).
- Use `bootstrap::stamp_graph_id` (already exists) to rewrite the saved layout's leaves to point at the new session's `root_graph_id`.
- Save the migrated layout back to disk.
- Emit `session.created { reason: "first_run_migration" }`.

Cheap; users with existing layouts don't lose them.

## 11. Sequencing

Suggested commit-shaped milestones (one PR each):

1. **`SessionId` type + `GraphSessionManifest` struct + tests.** No wiring. Pure type addition. Lands first because everything else depends on it.
2. **`ManifestStore` skeleton.** In-memory only, no disk. Replaces `GraphRegistry` internals while preserving the public API. Existing tests stay green.
3. **`session_graph_store` + manifest disk I/O.** Pure-function read/write. Tested in isolation.
4. **Lifecycle wiring.** `bootstrap::run` loads from disk; mutation → mark_dirty → flush; app-exit final flush. End-to-end round-trip works in dev.
5. **Diagnostic events + apparatus integration.** Events fire at the right transitions; apparatus pane shows them.
6. **Migration of existing state.** First-run migration for pre-manifest layouts.
7. **`KillSession` action + cascade close.** Final cleanup.

Each milestone is a target; targets are done conditions, not weeks.

## 12. Risks

- **Eidetic graph-persistence boundary may not be clean.** If `mere-kernel::graph::Graph` doesn't have a turnkey persist/load through eidetic today, this plan grows to include extracting that boundary first. Verify on day one of implementation; if it's a big lift, file a sub-plan rather than expanding scope silently.
- **Save-on-mutation feedback loops.** A graph that notifies on every per-pixel drag would generate flush events at frame rate. The 1s debounce is the first defense; the second is making sure the `cx.observe` notify hook batches across animation frames. Watch for this in the apparatus pane's event volume.
- **Trash directory growth.** `sessions/.trash/` accumulates forever without a cleanup affordance. Out of scope for this plan; needs a "permanently delete" gesture eventually. Log a follow-up todo when this plan lands.
- **Cross-platform path semantics.** Windows path length limits + UTF-16 handling have bitten this codebase before. UUIDs as directory names sidestep most of that; explicit testing on Windows is mandatory before marking done.

## 13. Configurability

Per project preference (configurability over opinionated defaults), the following should be user-settable:

- `manifest_flush_interval` (default 1s).
- `sessions_dir` override (default `<data_dir>/mere/mere-host/sessions/`).
- `keep_trash` toggle (default true) — false means kill bypasses .trash and deletes immediately.

These live in the same JSON config the keymap already uses (see `actions::user_keymap_path`-adjacent helpers).
