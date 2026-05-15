# Daemon-split research brief

**Date**: 2026-05-14
**Status**: Research only — no implementation v1; lists what stays remoteable, what doesn't, and what the actual split would need
**Scope**: Mere v0 ships as a *single-process logical daemon* — one OS process owns the registry, manifest store, `SessionServiceRunner`, and every window (framing brief §5.9). The trait shapes are designed *as if* they could be remote so a future split doesn't rewrite session semantics. This brief audits what's actually remote-ready today, what isn't, and what a split would buy.

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) §5.9 — sets the single-process daemon framing.
- [`../implementation_strategy/2026-05-14_session_service_runner_plan.md`](../implementation_strategy/2026-05-14_session_service_runner_plan.md) — the SessionServiceRunner trait is deliberately sync + owned-id + value-error so it remotes cleanly.
- [`../implementation_strategy/2026-05-11_typed_action_bus_plan.md`](../implementation_strategy/2026-05-11_typed_action_bus_plan.md) — the action bus is the natural IPC envelope.

---

## 1. Why this is research-only

A daemon split is *expensive* and the v1 win-loss math doesn't pencil:

- **Wins**: one window crashing doesn't kill the others; headless server deployments are possible without a window; `mere attach` from a CLI becomes a thing.
- **Losses**: process-boundary marshalling for every action, every notify, every paint signal; gpui's existing model assumes in-process state; double the test surface; new failure modes (daemon-not-running, daemon-version-mismatch, IPC corruption).

For a single-user desktop tool, the wins land late. Headless deployments and crash-isolated windows are real but rare. v1 stays single-process; this brief logs the shape so v2-or-later has a clear migration path.

## 2. What's already remote-ready

The framing brief committed to these shapes specifically so a daemon split is *transport-only*:

| Surface                          | Why it remotes cleanly                                                                  |
| -------------------------------- | --------------------------------------------------------------------------------------- |
| `GraphSessionManifest` (JSON)    | Hand-inspectable, schema-versioned, no in-process pointers.                             |
| `ManifestStore` (path → manifest)| File-system-backed; the daemon owns the directory.                                       |
| `SessionServiceRunner` trait     | Sync, owned ids, value-typed errors. `RemoteRunner` over IPC drops in.                  |
| Action bus (`BusAction`)         | Targets are owned ids; kinds derive `Clone + Debug + PartialEq` — JSON-serialisable.    |
| `view_intent_store` JSON sidecars | Same property — no pointers, just `(session_id, frame_id, pane_id)` keys.              |
| Engine profile path resolver     | Pure function over identity ids; the daemon hands paths to the engine, not bytes.       |

## 3. What isn't remote-ready

| Surface                          | What blocks remoting                                                                    |
| -------------------------------- | --------------------------------------------------------------------------------------- |
| `Entity<Graph>` (gpui)           | gpui's reactive entities live in-process. Cross-process notifies don't exist.            |
| `EngineRegistry` / live engine handles | Each engine binds resources (WebView2 instance, Servo process, model handle) that don't survive moves. |
| Hit-test proxies / paint deltas  | Per-frame `ProjectedScene` is 100s of KB; not free to ship over a pipe at 60fps.        |
| AccessKit tree                   | Tied to the active window's a11y backend; cross-process AT is its own protocol problem.  |

The pattern: **session truth remotes; runtime presentation doesn't.** Sessions, manifests, view intents, gates, workers — all daemon-side. Windows, engines, scenes, a11y — all client-side. The client *holds* the engine; the daemon *names* it.

## 4. The conceptual model — daemon + windows

```text
┌────────────────────────────────────────────────────────────────┐
│  meredaemon (one OS process)                                  │
│  ─────────────────────────────                                │
│  - registry (Entity<GraphRegistry>)                           │
│  - ManifestStore + view_intent_store + persona store          │
│  - SessionServiceRunner (workers as threads/tasks)            │
│  - action bus + permission gates                              │
│  - diagnostic event buffer                                    │
└────────────────────────────────────────────────────────────────┘
                       ▲           ▲          ▲
                       │ IPC       │ IPC      │ IPC
                       │           │          │
              ┌────────┴───┐  ┌────┴─────┐  ┌─┴────────┐
              │  window A  │  │ window B │  │ cli/curl │
              │  (gpui)    │  │  (gpui)  │  │ attach   │
              │  engines:  │  │  engines:│  │          │
              │  WebView2  │  │ WebView2 │  │          │
              │  Servo     │  │          │  │          │
              └────────────┘  └──────────┘  └──────────┘
```

The window process owns engines and renders. The daemon owns truth. Actions flow window → daemon (RPC). Notifies flow daemon → window (publish/subscribe).

## 5. What the IPC layer has to carry

Three message classes, all bus-shaped:

1. **`Dispatch(BusAction)`** — window asks daemon to apply an action. Returns `BusDispatchOutcome` (already an enum: Allowed / Denied / TargetMissing).
2. **`Subscribe(EventFilter)`** — window registers interest in a diagnostic stream or a manifest-change feed. Daemon pushes events back.
3. **`Query(StateQuery)`** — window pulls "current state of this thing" (active sessions, manifest of session X, view-intent of pane Y). Synchronous; rare; mostly at attach time.

All three already exist as in-process patterns; the daemon split just adds a transport. Candidate transports (decision deferred to v2):

- **Unix socket / named pipe** for local-only deployments — simplest, fastest, no networking risk.
- **HTTP/JSON-RPC** for headless server deployments — slower, but `curl` works.
- **gRPC** for both — heavier dependency footprint, structured.

## 6. Failure modes the split introduces

| Mode                                | Mitigation                                                                                 |
| ----------------------------------- | ------------------------------------------------------------------------------------------ |
| Daemon not running when client opens | Client auto-spawns daemon on first connect (the `dbus`/`tmux` pattern); explicit `--no-spawn` for headless. |
| Version mismatch                    | Manifest schema_version + bus protocol version handshake at connect. Refuse incompatibly-aged daemons. |
| Daemon crashes                      | Manifests are durable; on restart the client reconnects, re-attaches sessions. View-intent + worker state may be lost if not flushed. |
| Window crashes                      | Daemon notices the disconnect, marks the window's engine handles for cleanup, drains workers tied to that window. Other windows unaffected. |
| Network partition (HTTP transport)  | Out of scope for v1 — local Unix sockets only.                                              |

Two of these — daemon-crash and window-crash — are exactly the wins the split exists to provide. The mitigations are real engineering work but tractable.

## 7. Single-process forward-compat — what's still owed

The single-process v1 already keeps the trait shapes daemon-ready. What's *not* yet daemon-ready and should be cleaned up *before* a split is considered:

1. **In-process `&'static str` capability names.** Persisted overrides use `String`; runtime checks use `&'static str`. Across IPC, the runtime side has to accept `String` too. Not blocking, but the gate's hot path takes a stable-string-table indirection eventually.
2. **`PermissionDecision::RequireConsent` modal path.** The modal lives in the window. Daemon dispatches `RequireConsent`; the window paints, awaits user, replies with `Allow` or `Deny`. The protocol has to model a two-phase decision.
3. **Engine handles named, not held.** Currently the engine instance lives next to the registry. In a split, the daemon names the engine kind + profile path; the window instantiates the actual handle. Refactor the engine factory pattern when scrying integration matures.
4. **Diagnostic event subscription.** v1 has an in-process `EventBuffer`; for split, daemon hosts the buffer and clients subscribe. Adding a subscription API to the buffer now (even if there's only one consumer) keeps the contract honest.

None of these block v1 features. Each is small enough to land alongside other work as the surfaces touch them.

## 8. What this brief commits to

- **v1 stays single-process.** Daemon split is research, not roadmap.
- **Trait shapes stay remote-friendly.** Existing decisions (sync `SessionServiceRunner`, owned ids on `BusAction`, value-typed errors, JSON manifests) hold.
- **Four forward-compat threads stay open** (§7) — each landed opportunistically as nearby surfaces evolve, not as a single migration project.
- **No transport pick yet.** Unix sockets are the obvious local-first choice; the decision sits until a concrete user need (headless deployment, crash-isolation friction) lands.

## 9. Open questions

1. **What kills the "v1 stays single-process" stance?** Plausible triggers: a real headless deployment ask (corporate / multi-user-shared-machine), or a recurring "lost work due to window crash" pattern. Until one of those is concrete, single-process wins.
2. **Per-window engine instances vs daemon-side engine pool.** Stays per-window in the split. Cross-window engine pooling is a separate optimisation that doesn't interact with the daemon split.
3. **Macro-process model on macOS.** macOS's window-process model is different (Cocoa lives in one process; child views are in-process). A split there means more in-process state than on Linux/Windows. Pin when macOS support actually materialises.
