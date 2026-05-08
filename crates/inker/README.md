# inker

`inker` is the engine/renderer controller for the
[mere](https://crates.io/crates/mere) browser. It owns the question "*which
engine should handle this content,*" taking into account the URI scheme,
the content type, which engines are actually available on the host, and any
user preference. The output is a host-neutral surface contract that
[`verso-tile`](https://crates.io/crates/verso-tile) renders against.

Inker is the right home for arbitrating among engines when several are valid
for the same input. For full-web pages, both the Servo/wgpu fork (Serval) and
a Wry system webview can serve `https://`; only one of them may be built or
installed on a given host, and the user may prefer one over the other for a
specific domain or node. Resolving that is inker's job.

In the printing-press metaphor: inker pairs each engine to its protocol,
ready to ink the platen.

## What's in the crate

- **`routing`** — the route-decision vocabulary and default policy.
  - `EngineRouteRequest` / `EngineRouteDecision` — input / output types.
  - `WorkspaceRouteId` — opaque workspace identity carried with each request.
  - `EngineRoutePolicy` + `EngineRouteRule` — pluggable rule set with a
    fallback. Today's rules are scheme-based (one engine per scheme); richer
    selection (availability filtering, content-type, per-domain / per-node
    user preference) lands behind this same vocabulary.
  - `SurfaceContract` / `SurfaceContractMode` — host-neutral handoff
    (`CompositedTexture`, `NativeOverlay`, `EmbeddedHost`, `Headless`).
  - `address_scheme()` — utility to extract a scheme from a URI.
- **Engine ID constants**: `ENGINE_SERVAL_WEB`, `ENGINE_NEMATIC_SMOLWEB`,
  `ENGINE_NEMATIC_FILE`, `ENGINE_GRAPHSHELL_INTERNAL`,
  `ENGINE_EXTERNAL_PROTOCOL`.
- **Default policy routing** (the current single-engine-per-scheme slice):
  `http`/`https` → Serval; smolweb schemes (`gemini`, `gopher`, `finger`,
  `spartan`) → nematic; `file` → nematic file viewer; internal schemes
  (`about`, `graphshell`, `mere`) → headless internal; everything else →
  headless external-protocol handoff (unknown schemes never get guessed at).

## How it relates to other workspace crates

inker sits between [`graphshell`](https://crates.io/crates/graphshell) (which
issues route requests) and the engines themselves;
[`verso-tile`](https://crates.io/crates/verso-tile) owns the surface identity
inker hands back.

```text
       graphshell::app_state
              │ EngineRouteRequest
              ▼
            inker  ──────►  EngineRouteDecision
              │             (engine_id + SurfaceContract)
              │
              ▼
       engine_id selects: serval | nematic | wry | internal
                                                       │
                                                       ▼
                                                  verso-tile
                                              (SurfaceTargetId)
```

- [`graphshell`](https://crates.io/crates/graphshell) — emits
  `EngineRouteRequest` effects via its `EngineRouter` service trait; consumes
  the returned `EngineRouteDecision`.
- [`verso-tile`](https://crates.io/crates/verso-tile) — `SurfaceContract.target`
  is `verso_tile::SurfaceTargetId`, re-exported through `inker::routing` for
  convenience.
- [`nematic`](https://crates.io/crates/nematic) — referenced by engine ID
  (`nematic.smolweb`, `nematic.file`); concrete dispatch happens in host
  glue.
- **Serval** (Servo/wgpu fork) — referenced by engine ID `serval.web`; lives
  outside the mere workspace.
- **Wry** (system webview, third-party) — available as an alternative engine;
  not in the default policy but a custom `EngineRouteRule` can target it.

## Status

Pre-1.0. The route-decision vocabulary and default scheme policy are in
place. Planned expansions, in roughly the order they'll matter:

- **Engine availability filtering** — drop candidates whose engine isn't
  built / installed on this host before applying preference.
- **Multi-engine arbitration** — when both Wry and Serval can serve `https`,
  pick by user preference (default + per-domain / per-node override).
- **Content-type / MIME dispatch** — route by Content-Type once the response
  is known, not just by URI scheme.
- **Per-node engine pinning** — let the user lock a specific node to a
  specific engine.

## License

MPL-2.0.
