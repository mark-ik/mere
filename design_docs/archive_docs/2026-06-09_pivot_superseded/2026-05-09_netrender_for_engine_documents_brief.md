# Netrender as a General Renderer for Engine Documents — Brief

**Date**: 2026-05-09
**Status**: Research / decision brief
**Scope**: Whether to wire `netrender` into Mere as a general dep — split as "vello for graph, netrender for docs" — and whether `nematic` engines should grow display-list output for netrender consumption. Or alternatively keep netrender inside Genet and skip the cross-cutting integration.

**Related**:

- Inherited: [`netrender/netrender-notes/2026-04-30_netrender_design_plan.md`](../../../../netrender/netrender-notes/2026-04-30_netrender_design_plan.md) — sole forward-looking netrender plan
- Inherited: [`netrender/netrender-notes/2026-05-04_feature_roadmap.md`](../../../../netrender/netrender-notes/2026-05-04_feature_roadmap.md) — Phase B is "consumer-pull-imminent: things nematic and genet will surface"
- Inherited: [`genet/docs/2026-05-05_genet_netrender_cut_plan.md`](../../../../genet/docs/2026-05-05_genet_netrender_cut_plan.md) — Genet's netrender adoption
- [`../implementation_strategy/2026-05-09_post_engine_layer_priorities.md`](../implementation_strategy/2026-05-09_post_engine_layer_priorities.md) — current forward plan

---

## 1. What netrender is, today

A wgpu-native 2D renderer in the form of a workspace crate (`netrender`) plus a device abstraction (`netrender_device`) and text helpers (`netrender_text`). Built fresh against wgpu's actual model — no GL retrofitting. Vello pinned at git main (`0.8.0`) is the rasterizer underneath; netrender adds the display-list machinery, capture/replay (postcard + JSON, behind a `serde` feature), and consumer-ready surface.

Consumer-ready surface (per the design plan §1):

> Hand in a `wgpu::Device`/`Queue`, configure a `wgpu::Surface`, feed display lists, get pixels. genet, Graphshell, anything else.

`Scene` is the main display-list type. It carries `SceneOp::Shape`, glyph runs, alpha modifiers, blend modes, clips, filters — a full 2D rendering vocabulary. Capture / replay exists (`Scene::snapshot_postcard`, `snapshot_json`) so authored fixtures round-trip and consumers can dump regression artifacts.

Phase B of the feature roadmap is explicit:

> Things nematic (Gemini, Gopher, Scroll, Markdown, feeds, Finger) and genet (full web) will surface as parley wiring stabilizes and graphshell-shaped consumers wire in. Nematic is the smolweb engine in the Mere workspace (`mere/crates/nematic`); each protocol surfaces…

So **netrender already plans for nematic to be a first-class consumer**. Mark's "could nematic produce display lists for netrender" isn't a stretch — it's the design intent.

## 2. The "split vello / netrender" question

The framing ("split out vello for graph and netrender for docs") suggests two parallel rendering stacks. Looking at netrender's actual structure, the picture is simpler:

- **Vello** is a 2D GPU rasterizer (Linebender). Netrender depends on it via `vello = { git = "https://github.com/linebender/vello", branch = "main", … }`.
- **Netrender** wraps vello with display-list machinery (`Scene` ops, transform stacks, clip stacks, capture/replay, the wgpu surface integration).
- A consumer can depend on **vello directly** (raw scene-builder access; small, idiomatic, no display-list intermediary) **or** on **netrender** (display lists, scene capture, full webrender-shaped pipeline).

Both routes use the same vello underneath — they just choose different abstraction levels. There's no actual "split" needed; they're stacked.

For Mere's current shape:

- **Graph canvas** (force-directed nodes / edges / labels) wants direct, frame-rate-sensitive rasterizer access. Vello directly is the right level: build a `vello::Scene`, draw the graph, rasterize. No display-list intermediary buys anything; the graph is *not* a static document.
- **Document tiles** (the rendered output of nematic / genet) want display lists if there's any point in capture / replay / inspector tooling. Netrender's `Scene` + capture is purpose-built for this case.

So the answer is: **don't split — stack**. Add vello as a direct workspace dep for the graph canvas (probably already there via `graph-canvas`); add netrender as a separate workspace dep for document tile rendering. They share a vello version through cargo unification, but they expose different API levels to different callers.

### Can vello-in-netrender do graph stuff "on the side"?

No, not without a fork. Netrender's vello is buried inside its rasterizer pipeline; you don't get raw scene-builder access through netrender's API surface. If you depend only on netrender, you can't bypass its display-list machinery to do ad-hoc graph drawing.

But adding vello as a separate workspace dep is free — cargo unifies the version (both crates pull the same git main branch / same `0.8.0` minor). One vello, two API surfaces, no duplicate compile.

## 3. Producing display lists from nematic engines

Two ways to think about this:

### (a) Nematic engines emit `Scene` ops directly

Each `nematic.*` engine produces a `vello::Scene` (or a netrender `Scene` — they're related but distinct types) instead of (or in addition to) an `EngineDocument`. The host renders the scene via netrender; no intermediate layout.

**Tradeoffs:**
- ➕ One less layout pass; the engine controls placement directly.
- ➖ Engines now own *layout*, which is the platen layer's job. Breaks the printing-press separation. Engines lose portability — they need a font system, a measure pass, knowledge of viewport.
- ➖ Round-trip / re-render becomes harder; you can't go from `Scene` back to `EngineDocument` for editing or knot serialization.
- ➖ Loses the semantic block intent we just built (`FeedEntry`, `MetadataRow`, etc. — these are *meaning*, not paint commands).

This route trades the engine layer's portability for rendering shortcuts. **Don't take it.**

### (b) Nematic engines stay structural; platen produces display lists

The current shape is: nematic engines → `EngineDocument` (structural + semantic) → platen lays out → platen emits a renderable form. Today "renderable form" is undefined; the host (mere-host's gpui code) does the layout and rendering itself.

Adding netrender slots cleanly here: `platen` grows a "render to `netrender::Scene`" lane. Engines stay portable; platen owns layout (using parley for text); the resulting `Scene` is a faithful representation of the document's semantic content — every paragraph, link, code block, feed entry rendered by platen's rules.

**Tradeoffs:**
- ➕ Preserves the printing-press separation. Engines stay wasm32-portable.
- ➕ Round-trip via `to_knot()` / `to_markdown()` / `to_gemini()` continues to work — semantic structure is preserved at the document level, layout is downstream.
- ➕ Capture / replay via netrender works for free (debug + regression fixtures).
- ➕ Lets the gpui host *consume* `Scene`s directly via netrender's wgpu integration — no per-host layout duplication.
- ➖ Platen needs a real layout engine (parley). Bigger commitment than today's "host does its own layout."

This is the right shape if and when platen grows up. **It's the right answer to "produce display lists from the protocols for netrender as faithful representations for our knots."** The faithful representation is the *EngineDocument's blocks rendered by platen's layout into a Scene*, not a Scene authored by the engine itself.

## 4. Recommendation

Three options, smallest first:

1. **Status quo: keep netrender inside Genet.** Genet consumes netrender for its own HTML rendering; nematic / mere-host render documents themselves (currently gpui-shaped). Lowest commitment; Mere adds no new deps; the renderer cross-pollination Mark imagined doesn't happen yet.
2. **Add vello as a workspace dep for the graph canvas, only.** Probably already present via `graph-canvas`; if not, this is a one-line addition. No netrender involvement. Keeps the graph canvas using vello directly without layering display-list machinery on a non-document workload. **Trivial regardless of (3).**
3. **Add netrender as a workspace dep + grow platen toward `Scene` output.** This is the "produce display lists from the protocols" path Mark asked about. Substantial commitment: needs parley wiring for text layout, platen growing real layout passes, mere-host learning to consume `Scene`s instead of bespoke gpui rendering. Pays off when:
   - Document tiles need consistent rendering across hosts (gpui / iced / future / wasm)
   - Capture/replay debug tooling becomes valuable
   - Genet and nematic share a rendering pipeline (matches the netrender Phase B intent)

**My pick: (1) + verify (2) is already done, defer (3) until the gpui host's bespoke layout starts hurting.** The gpui host is the current bottleneck; growing platen + netrender integration *now* would race the host without making it land sooner. Once the host is real and a consistent rendering pipeline becomes valuable, (3) is well-prepared by netrender's design and Phase B roadmap — and the engine layer's `EngineDocument` is the right input shape.

The "three-head Hekate Genet" framing is independent of (3): Genet already consumes netrender for its own rendering, regardless of whether Mere-side document tiles do.

## 5. Open questions worth tracking

- **Parley state in mere-host**: the gpui host's text rendering — does it already use parley, or is gpui's built-in text? If gpui's text is already adequate, that affects whether option (3) is worth the migration. (The earlier project memory `[Parley over cosmic-text for middlenet text layout]` flagged parley as the host-agnostic choice.)
- **Vello version drift**: netrender pins vello at git main / `0.8.0`; if `graph-canvas` pins a different version, cargo can't unify. Worth checking before committing to "two consumers, one vello."
- **When does scene capture / replay matter?**: if it's just debug, the cost of (3) is high relative to the win. If it's part of the test infrastructure or accessibility tooling, it's much more valuable. Lean on this when deciding the (3) timing.
