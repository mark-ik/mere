# Engine profile boundary — implementation plan

**Date**: 2026-05-14
**Status**: Implementation plan — v0a path-resolution primitive landed; v0b per-engine wiring pending

> **Reconcile note (2026-07-03 archive pass):** `engine_profile_store.rs`
> (`crates/system/session-runtime/` *(historical citation)* <!-- doc-audit: historical-path -->) now carries `engine_profile_path` /
> `engine_profile_path_for_session` with tests, so the primitive is live; per-engine UDF
> wiring status (scrying/graft/weld each reference engine-profile types) was not
> re-verified engine-by-engine. The engine list here predates the genet multiplexer
> framing (scry/graft/weld as `SurfaceEngine`s); read alongside the current inker code.
**Scope**: Make the line between *graph truth* and *engine profile bytes* explicit. Engines (WebView2 via scrying, Wry, Servo via netrender, nematic, future others) own cookies, permissions, cache, IndexedDB, localStorage; the session references a profile binding, never owns those bytes. Land the path-resolution primitive now; per-engine UDF wiring (scrying first, then siblings) follows.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) §5.4 — the framing brief that defines the tiered scoping.
- [crates/system/session-runtime/src/manifest.rs](../../../crates/system/session-runtime/src/manifest.rs) *(historical citation)* <!-- doc-audit: historical-link --> — `EngineProfileBinding` enum (PersonaScoped / SessionScoped / GraphScoped), `PersonaId`, `GraphSessionManifest::engine_profile` are already in place.
- [crates/inker/src/engine.rs](../../../crates/inker/src/engine.rs) *(historical citation)* <!-- doc-audit: historical-link --> — `Engine::engine_id() -> &str` is the stable engine identifier that names the UDF directory.

---

## 1. Goal + done conditions

**Goal:** Given a session's manifest, a persona, and an engine identifier, produce a stable UDF path that the engine writes its state into. The path scheme is durable across restarts and supports the three escalation tiers from the framing brief.

**v0a done when (this turn):**

- `engine_profile_store` module in `system/session-runtime` exposes `engine_profile_path` — a pure function returning the UDF directory for the given inputs.
- File layout matches the brief: persona-scoped under `<data_root>/personas/`; session/graph-scoped under `<data_root>/sessions/<session_id>/`.
- Invalid combinations (e.g. `SessionScoped` without a `session_id`) return `None` rather than silently fabricating a path.
- Round-trip tests cover all three scopes + missing-id rejection + path-component sanity.

**v0b done when (follow-up):**

- `scrying-tile-engine` consults `engine_profile_path` when constructing its WebView2 environment, replacing the current hardcoded / default UDF location.
- Other live engines (when they exist) follow the same pattern.
- An integration test (or hand check) verifies two sessions sharing a persona share cookies; flipping one to `SessionScoped` isolates its cookies.

## 2. Path layout

```text
<data_root>/
├── personas/
│   └── <persona_id>/
│       └── engine-profiles/
│           └── <engine_id>/   ← UDF for persona-scoped sessions
└── sessions/
    └── <session_id>/
        ├── manifest.json
        ├── graph.json
        ├── views/             ← view_intent_store
        └── engine-profiles/
            └── <engine_id>/   ← UDF when SessionScoped
            (or:)
        └── graphs/
            └── <graph_id>/
                └── engine-profiles/
                    └── <engine_id>/   ← UDF when GraphScoped
```

- `<data_root>` is the parent of both `personas/` and `sessions/`. It's one level above `ManifestStore::root` — the manifest store knows it implicitly; the engine-profile resolver takes it as an explicit argument so the contract is pure.
- `<engine_id>` is `Engine::engine_id()` (or `SurfaceEngine::engine_id()`) — matches the stable string each engine advertises.

## 3. Resolver signature

```rust
pub enum EngineProfileScope {
    Persona,
    Session,
    Graph,
}

impl From<EngineProfileBinding> for EngineProfileScope { ... }

pub fn engine_profile_path(
    data_root: &Path,
    persona_id: PersonaId,
    engine_id: &str,
    scope: EngineProfileScope,
    session_id: Option<SessionId>,
    graph_id: Option<GraphId>,
) -> Option<PathBuf>;
```

Returns:

- `Persona` → `<data_root>/personas/<persona_id>/engine-profiles/<engine_id>/`.
- `Session` → `<data_root>/sessions/<session_id>/engine-profiles/<engine_id>/` (or `None` if `session_id` missing).
- `Graph` → `<data_root>/sessions/<session_id>/graphs/<graph_id>/engine-profiles/<engine_id>/` (or `None` if either id missing).

Convenience wrapper:

```rust
pub fn engine_profile_path_for_session(
    data_root: &Path,
    manifest: &GraphSessionManifest,
    engine_id: &str,
    graph_id: Option<GraphId>,
) -> Option<PathBuf>;
```

The manifest carries `engine_profile`, `persona_id`, `session_id`. The convenience form lets callers skip the manual scope assembly.

## 4. Why pure-function, no I/O

v0a doesn't create the directory, write files into it, or check existence. Three reasons:

1. **Engines own the bytes.** scrying / WebView2 know how to initialise a UDF (it's their proprietary layout); the resolver just hands them a path.
2. **Tests are trivial when there's no filesystem.** Path composition is pure; integration with real I/O lives in the per-engine wiring.
3. **Bind is decoupled from create.** A session can declare `SessionScoped` long before the first navigation; the directory should materialise when the engine actually launches, not at manifest write time.

`fs::create_dir_all` is the engine's call when it boots.

## 5. Per-engine wiring (v0b deferred)

Each engine that runs in Mere has different UDF semantics; the wiring is engine-by-engine, not a single hook:

- **scrying-tile-engine** (WebView2 on Windows / WKWebView on macOS / WebKitGTK on Linux): WebView2 takes an explicit `userDataFolder` per Microsoft's audit guidance. scrying's producer-construction path needs the resolved `engine_profile_path` for the active session.
- **Wry** (fallback engine): similar — Wry's `WebViewBuilder` takes a `with_data_directory(...)` hook.
- **Servo via netrender**: opt-in profile dir on Servo's `Opts`. Open question: Servo profile is heavier (per-process), so SessionScoped Servo may need a separate process boundary.
- **nematic** (smolweb document engine): no UDF — purely stateless renderer.

The v0b sequencing: scrying first (it's the active engine with the biggest leak surface), then Wry, then Servo when netrender lands in meerkat, then anything else.

## 6. Manifest hooks

`GraphSessionManifest.engine_profile` already exists with `EngineProfileBinding::default() = PersonaScoped`. The resolver consumes it directly. No manifest schema change required for v0a.

Future fields the manifest might want, tracked here so they don't get lost:

- `engine_profile_overrides_per_engine: HashMap<String, EngineProfileBinding>` — when one engine should escalate (e.g. servo SessionScoped) while siblings stay PersonaScoped. Not in v0; add when a concrete use case arrives.

## 7. Test plan

**v0a (this turn):**

- Persona scope composition matches `<data_root>/personas/<persona_uuid>/engine-profiles/<engine_id>/`.
- Session scope composition matches `<data_root>/sessions/<session_uuid>/engine-profiles/<engine_id>/`.
- Graph scope composition matches `<data_root>/sessions/<session_uuid>/graphs/<graph_uuid>/engine-profiles/<engine_id>/`.
- Session scope returns `None` when `session_id` missing.
- Graph scope returns `None` when either id missing.
- Convenience wrapper round-trips manifest → path.
- `From<EngineProfileBinding> for EngineProfileScope` covers all three.

**v0b (follow-up):**

- Two sessions with `PersonaScoped` + same persona share an engine UDF (visible via shared cookies after navigation).
- One session set to `SessionScoped` gets its own UDF (cookies isolated from peers).
- Engine-launch path creates the UDF directory lazily on first navigation.

## 8. Open questions

1. **Where does `data_root` come from?** The host's `ManifestStore` already has a `root: PathBuf` pointing at the sessions directory. The engine-profile resolver wants the *parent*. Either pass it explicitly, add `data_root: PathBuf` to `ManifestStore`, or store it once on `HostRoot`. Filed for v0b.
2. **Profile binding mutation.** If the user flips `PersonaScoped → SessionScoped` mid-session, the cookies should *not* silently migrate. Either the change takes effect on next session restart, or the engine relaunches against the new path with a clear "your previous cookies stayed in the persona pool" message. Filed for v0b.
3. **Engine UDF deletion on session kill.** A killed session's UDF should be cleaned up if it was session/graph-scoped; persona-scoped UDFs survive. Tracked alongside the manifest store's `.trash/` workflow.
