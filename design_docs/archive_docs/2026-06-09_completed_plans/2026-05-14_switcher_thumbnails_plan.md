# Switcher thumbnails via MinimapDescriptor — implementation plan

**Date**: 2026-05-14
**Status**: Implementation plan — v0a thumbnail descriptor + builder helper landed; v0b host render path pending
**Scope**: Replace the graph-switcher's text-only rows with 200×120 thumbnails rendered from each session's projection, per the framing brief §5.5. Cartography's [`MinimapDescriptor`](../../../crates/cartography/src/minimap.rs) is the in-canvas overlay contract; this plan defines the *standalone* thumbnail contract sibling-aware of it.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) §5.5 — the framing brief.
- [`../research/2026-05-10_cartography_layer_brief.md`](../research/2026-05-10_cartography_layer_brief.md) — cartography contract types; `FormFactor::Minimap` is the per-strategy small-scale projection mode this leans on.
- [`crates/cartography/src/minimap.rs`](../../../crates/cartography/src/minimap.rs) — the live-canvas minimap descriptor (Sublime-style viewport-window). Adjacent but distinct from a standalone switcher thumbnail.
- [`crates/mere-host/src/graph_switcher.rs`](../../../crates/mere-host/src/graph_switcher.rs) — current text-only switcher (`render_graph_row` at line 132).

---

## 1. Goal + done conditions

**Goal:** Each row in the graph switcher carries a small projection of the session's graph at thumbnail scale (200×120 default; configurable). The thumbnail is built from the same cartography contract a live pane uses — same strategy, same projection request, smaller `target_size` plus `FormFactor::Minimap`.

**v0a done when (this turn):**

- `switcher_thumbnail` module in `mere-host-runtime` exposes:
  - `SwitcherThumbnail` — the typed bundle the host renders (size + positioned node/edge geometry).
  - `SwitcherThumbnailOptions` — caller config (size, max-nodes cap for very large graphs).
  - `build_switcher_thumbnail(graph, options)` — pure function that walks `graph.nodes()` / `graph.relations()` and produces a `SwitcherThumbnail`. No cartography dependency yet; layout is "axis-aligned bounding-box fit" so the thumbnail works *today* without waiting for cartography strategies to wire into the host.
- Tests cover: empty graph, single node, multi-node fit-to-bounds, family colouring through the relation tag, and the node-cap behaviour.
- No `graph_switcher.rs` changes — host wiring is v0b.

**v0b done when (follow-up):**

- `graph_switcher::render_graph_row` calls `build_switcher_thumbnail` (or a cartography-driven equivalent) and paints the result inside the row.
- Hover-enlarges to a multi-pane preview (tmux's `choose-tree` analogue).
- When cartography strategies are live in the host, the thumbnail switches from the v0a fit-to-bounds layout to the strategy's `FormFactor::Minimap` projection.

## 2. Why two adjacent contracts

The cartography crate already has `MinimapDescriptor`. That's the *Sublime-style minimap inside a live canvas* — viewport-window overlay, retained overlay kinds, embedded into the projection's own scene. The switcher thumbnail is different:

| Concern | `MinimapDescriptor` (cartography) | `SwitcherThumbnail` (this plan) |
| ------- | --------------------------------- | -------------------------------- |
| Lives where? | Inside a `Projection.minimap` for an existing live canvas | Stand-alone, no parent canvas |
| Viewport window? | Yes — represents where the parent canvas is looking | No — whole-graph thumbnail |
| Strategy-driven? | Yes — same strategy, `FormFactor::Minimap` | Eventually yes (v0b); v0a does bare fit-to-bounds |
| Overlays retained? | Caller picks from `MinimapOverlayKind` | None in v0; can grow into the same vocabulary |

They share the *what-fits-at-small-scale* principle but answer different host questions. The switcher row doesn't need a viewport window because there's no parent canvas to point at; it just needs the projected geometry.

## 3. v0a shape

```rust
pub struct SwitcherThumbnail {
    pub width: u32,
    pub height: u32,
    /// Projected node positions in thumbnail-local coordinates.
    pub nodes: Vec<ThumbnailNode>,
    /// Projected edges as (from, to) thumbnail-local segments,
    /// coloured by family.
    pub edges: Vec<ThumbnailEdge>,
}

pub struct ThumbnailNode {
    pub position: PortablePoint,
    pub radius: f32,
}

pub struct ThumbnailEdge {
    pub from: PortablePoint,
    pub to: PortablePoint,
    /// Family-tag ordinal from `EdgeFamily`. The host paints with the
    /// same `family_color` palette platen uses for the orrery.
    pub family_tag: u8,
}
```

v0a is *deliberately* gpui-free and Color-free — the host's renderer maps `family_tag` to the existing `platen::family_color` palette so the switcher and the orrery look consistent.

`PortablePoint` lives in `mere-kernel::geometry` (already used by cartography); reusing it keeps the type portable.

## 4. The v0a layout — fit-to-bounds

Until cartography strategies wire into the host, v0a uses a straight fit-to-bounds:

1. Compute the axis-aligned bounding box of `graph.nodes()`'s positions.
2. Scale to fit the thumbnail box (preserving aspect ratio).
3. Optionally cap the node count (`SwitcherThumbnailOptions::max_nodes`) — when exceeded, sample evenly via stride; further refinement (importance-weighted sampling, halo summarisation) is v0b.

This is intentionally *cheap* — cartography strategies are richer and will replace this when they're host-side. The fit-to-bounds version is honest about the gap: it's a thumbnail of *positions*, not a thumbnail of *meaning*.

## 5. Family colouring

Relations carry a `RelationKind::tag()` ordinal (from the §6.3 work). The thumbnail boils that down to the top byte (the family ordinal) and passes the resulting `u8` as `ThumbnailEdge::family_tag`. The host's renderer maps it via the existing `platen::family_color` palette. No new colour decisions to make at thumbnail scale; this enforces a single colour story across the app.

## 6. Test plan

**v0a (this turn):**

- Empty graph → empty thumbnail (no nodes / edges, dimensions intact).
- Single node → centered in the thumbnail box (no division by zero on degenerate bounds).
- Multi-node fit preserves aspect ratio; nodes stay inside the thumbnail box.
- A hyperlink edge produces a `ThumbnailEdge` with `family_tag = EdgeFamily::Semantic ordinal (0)`.
- A multi-relation pair yields multiple `ThumbnailEdge`s with distinct `family_tag`s — same shape the orrery relies on.
- `max_nodes` cap drops nodes (and any edges that no longer have both endpoints) deterministically.

**v0b (follow-up):**

- `render_graph_row` paints the thumbnail at 200×120 with crisp anti-aliased lines.
- Hover state enlarges to multi-pane preview.
- Thumbnails update when the underlying graph mutates (re-paint, no caching tier yet).

## 7. Open questions

1. **When to cache.** Repainting every frame is cheap for small thumbnails but wasteful when the switcher is closed. Likely cache the `SwitcherThumbnail` per graph and invalidate on `graph.notify()` (gpui's reactive pattern handles this naturally). Defer the caching tier until profiling shows it matters.
2. **Cartography handoff.** Once cartography strategies are live in the host, the thumbnail strategy should match the orrery's active strategy so the switcher *recognises* a session at a glance. v0a doesn't try — fit-to-bounds is strategy-blind. v0b adds the strategy-driven path as soon as the host actually runs cartography.
3. **Importance-weighted sampling.** When `max_nodes` is hit, dropping low-degree nodes preserves the graph's "skeleton" better than stride-sampling. Lands as part of the strategy-driven v0b path — cartography is the natural home for importance signals.
