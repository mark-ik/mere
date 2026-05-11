# Engine Peers + Scrying-as-Library — architecture decision

**Date**: 2026-05-11
**Status**: Decision brief
**Scope**: Pins the engine taxonomy after a working-tree-clean conversation
about whether serval should host nematic / scrying internally, or whether
all three sit as peers under mere. Refines the engine-profile boundary
in the [browser multiplexer framing brief](2026-05-11_browser_multiplexer_framing.md)
§5.4 with the specific decision about scrying's role.

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md)
  — the framing this brief refines.
- [`2026-05-10_cartography_layer_brief.md`](2026-05-10_cartography_layer_brief.md)
  — engines as content producers; cartography as the projection contract above.

---

## Decision

**Mere is the engine manager. Engines are content production functions.
Scrying is a library, not a peer engine.**

Three engine peers under mere:

- `serval.*` — Servo rendering. One sub-engine today (`serval.web`); future
  sub-engines per the [Hekate framing memory](memory) (`serval.smolweb` for
  reader-mode extract; etc.).
- `nematic.*` — 12 protocol engines (markdown, gemtext, gopher, feed, text,
  file, finger, knot, scroll, misfin, nex, guppy).
- `wry.compat` — mere-managed system-WebView tile, driven by scrying as a
  library. Frame capture via `webview2-com` (Win) / `objc2-web-kit` + SCK
  (macOS) / WebKitGTK+DMABUF (Linux, skeletal).

Scrying is **not on the engine taxonomy**. It's the texture-extraction
primitive `wry.compat` (and anyone else who needs it later) pulls in. The
engine name `wry.compat` covers the tile-engine *role*; the scrying crate
covers the *library* that makes that role's frame production work.

## Why this shape

### Mere as manager

The cleanest mental model:

- **State that survives engine choice lives in mere.**
  - Graph truth (URL, identity, history, classifications) in
    `mere-kernel::graph::Node`.
  - View intent (scale, scroll position, form-input drafts) in the
    per-pane sidecar (multiplexer brief §5.3).
- **State that lives and dies with a render pass lives in engines.**
  - DOM/CSSOM, JS heap, GPU textures, paint cache.
- **Engines never see graph truth directly.** Mere passes them
  `(address, view_intent)`; engines return `EngineDocument` (nematic)
  or paint frames into a `SurfaceContract` (serval, `wry.compat`).

### Scrying as library, not engine

Library boundaries should match abstraction levels. Scrying is a
*primitive* — "given a WebView2/WKWebView control, capture its frames
into wgpu-importable textures." It doesn't carry navigation semantics,
session identity, or routing rules. The *engine* — the thing inker
selects, the thing the host wires into a tile — is `wry.compat`.
Anyone who wants a Wry-shaped tile path uses scrying; if a different
"WebView-shaped" engine appears later (a Tauri-managed tile, an SDL2
WebView tile, etc.), it also uses scrying. Engine ≠ library.

### Why no `serval.compat` internal mode

An earlier framing put scrying *inside* serval as a "compatibility mode"
serval flips into when Servo can't render a site. That has appeal —
mode-flips are cheap and the engine boundary is shorter — but it makes
serval a meta-engine that quietly orchestrates scrying, which:

- Duplicates the scrying integration path (mere also has one).
- Splits engine selection across two systems (inker for cross-tile,
  serval-internal for compat-flip).
- Makes serval responsible for cookie/session continuity across engines
  (a host-level concern, not a Servo-rendering concern).

The simpler shape: **serval is pure Servo rendering.** When Servo can't
render, serval reports rendering failure upward; mere offers a tile-
engine switch from `serval.web` to `wry.compat`. The compat-flip
becomes a regular engine-selection event, using existing inker
machinery, not a parallel decision system.

## Cookie / session continuity

The key insight: **scrying inherits serval's session state via a shared
persona-scoped UDF**, not via engine-to-engine handoff.

When mere flips a tile from `serval.web` to `wry.compat`, both engines
bind the same persona-scoped UDF (see multiplexer brief §5.4):

```text
<data_dir>/personas/<persona_id>/engine-profiles/<engine_kind>/
```

Servo writes cookies, IndexedDB, localStorage into this UDF while it
was the active engine. When the WebView2/WKWebView control spins up
under scrying, it reads from the same UDF. Logins persist, cart state
persists, permissions persist — because the *storage* is shared, not
because the engines coordinate.

This works because **the cookie transfer is one-way** (serval-built
context → scrying consumes it). Scrying is the compatibility consumer,
not a co-producer; we don't need scrying-to-serval continuity, only
serval-to-scrying.

### Where the UDF model has limits

WebView2 / WKWebView / WebKitGTK each have their own session model
that *isn't* fully observable from Servo (or vice versa). The shared-
UDF approach gives continuity for the storage tier (cookies, web
storage, cache); it doesn't unify in-memory state (open WebSocket
sessions, JS heap, pending fetches). On flip:

- Storage tier → preserved (shared UDF).
- In-memory tier → reset (full reload in the new engine).

This is the right trade-off. Users who flip to compat mode generally
want a fresh reload anyway because the original render failed.

## Non-cookie continuity (scroll / form drafts)

Scroll position, form-input drafts, navigation cursor — these aren't
in the UDF; they're per-pane render-time state.

v1: full reload, accept the cost. The page may render differently in
WebView2 anyway; preserving scroll position to a different layout is
of questionable value.

v2: serval emits a `TileSnapshot` of serializable view state at engine-
flip time; mere stashes it in the per-pane `ViewIntent` sidecar (§5.3);
scrying restores what it can on load. No new architecture — just one
more field on the sidecar.

## Inker engine taxonomy after this decision

```text
serval.web                  → http(s)://* default
nematic.markdown            → text/markdown, *.md
nematic.gemtext             → gemini://, text/gemini
nematic.gopher              → gopher://
nematic.feed                → application/rss+xml / application/atom+xml
nematic.text                → text/plain
nematic.file                → file:// (not a smolweb protocol; native dir browser)
nematic.finger              → finger://
nematic.knot                → .knot files (Mere's polyglot note format)
nematic.scroll              → scroll://
nematic.misfin              → misfin://
nematic.nex                 → nex://
nematic.guppy               → guppy://
wry.compat                  → mere-managed system WebView; opt-in per tile

graphshell.internal         → about://, mere://, graphshell:// (unchanged)
external.protocol           → fallback (unchanged)
```

The deprecated `wry.webview` peer is dropped — its role is now
`wry.compat`, which differs in two ways: it explicitly belongs to the
"compat / fallback" semantic (not the default for http(s)), and it's
mere-managed (no internal-to-serval indirection).

### Auto-fallback rule (future, not v1)

A planned inker policy: when `serval.web` reports a rendering failure
(layout never settled / paint errored / JS exception flood / explicit
"this site requires a different engine" signal), the host offers
`wry.compat` for that tile via the user's existing engine-pin
machinery. Implementation lands later; the routing surface already
supports per-node engine pins (`pinned_engine` in `EngineRouteRequest`),
so no new machinery needed in inker — just a new offer-the-switch UI
gesture in mere-host.

## What changes in code

Small surface:

1. **`inker::routing`**: engine ID constants gain `wry.compat`, drop
   `wry.webview` (or rename in place — the latter is a smaller diff
   if `wry.webview` isn't referenced by any default rule).
2. **`scrying` dependency**: pulled into `mere-host` (or a thin
   `mere-host-scrying-tile` crate). Not pulled into `serval`.
3. **`serval`**: no change. Servo rendering only; reports failure
   upward through the existing `SurfaceContract` failure path (which
   already exists for crashed webviews per the current code).
4. **`mere-host`**: gains a `wry-compat-tile` module structurally
   analogous to existing tile types — bootstraps a WebView2/WKWebView
   via scrying's producer trait, captures frames, hands a wgpu
   texture into the surface placement plan.
5. **`mere-kernel::graph::Node`**: no schema change. The existing
   `viewer_override` field already supports per-node engine pinning.
6. **Persona-scoped UDF binding**: documented as a contract that
   `serval.web` and `wry.compat` both honor. Where the UDF path comes
   from is a host-config concern (multiplexer brief §5.4).
7. **`TileSnapshot` (deferred)**: small new type for v2 continuity;
   serializable view state at engine-flip time, stored in the
   `ViewIntent` sidecar.

Estimated landing size: low hundreds of LOC for the v1 path
(`wry.compat` engine wiring + UDF binding + inker rename). The
`TileSnapshot` v2 path is its own small slice when continuity becomes
worth the work.

## Open questions left for follow-up

1. **`wry.compat` vs. naming the broader role.** Today the engine is
   Wry-shaped because that's where scrying's WebView producers are
   wired. If future surfaces (a hypothetical Tauri-tile or SDL2-tile)
   land, the engine name space needs rethinking. Probably `system.web`
   (engine kind) + per-platform sub-engine (`system.web.webview2`,
   `system.web.wkwebview`, `system.web.webkitgtk`). Defer until a
   second non-Wry system-tile actually appears.
2. **Sub-engine sub-modes.** `serval` may eventually grow
   `serval.smolweb` (reader extract) as a peer to `serval.web`. The
   `engine.sub-engine` shape supports this without churning the
   taxonomy.
3. **Failure-signal vocabulary.** What exactly counts as "serval failed
   to render"? Layout never settled? Paint errored? JS exception
   threshold? The auto-fallback heuristic needs a concrete signal
   schema. Skip until manual flip is wired and we have data.

## Connection to other briefs

- [Multiplexer framing](2026-05-11_browser_multiplexer_framing.md) §5.4
  (engine profile boundary): this brief refines the persona-scoped UDF
  model into a concrete cross-engine continuity story.
- [Cartography brief](2026-05-10_cartography_layer_brief.md): engines
  are content producers; cartography projects truth into views.
  Cartography doesn't care which engine renders a tile — it consumes
  graph state regardless. This decision keeps that separation clean.
- [Post-engine-layer priorities](../implementation_strategy/2026-05-09_post_engine_layer_priorities.md):
  §2.2 donor-area rebuilds includes "runtime / webview lifecycle wiring";
  this brief makes the `wry.compat` integration concrete.
