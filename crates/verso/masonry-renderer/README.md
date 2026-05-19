# mere-masonry

First-cut sketch of a Mere-substrate driver for the Masonry widget engine.

> **Status (2026-05-15)**: read-only sketch, written against
> [`design_docs/mere_docs/research/2026-05-15_xilem_embedding_spike.md`](../../design_docs/mere_docs/research/2026-05-15_xilem_embedding_spike.md)
> as the next concrete artefact in §7.2. **Does not compile yet** — depends on
> the (not-yet-built) renderer-registry crate and on substrate-side types
> `mere-host-runtime` hasn't exposed. The shape is concrete; the wiring is
> `TODO`. Read this against the spike report for the architectural rationale.

## What this crate is

A *renderer registry tenant* that lets Mere's spatial chrome IR substrate
display a [`masonry_core`](https://docs.rs/masonry_core) widget tree in a
single tile, with all input / output / accessibility threaded through the
substrate's own dispatch and not through `winit`.

Concretely:

- The substrate owns the OS window, the `wgpu::Surface`, and the per-frame
  `vello::Scene`.
- `mere-masonry` owns one `masonry_core::app::RenderRoot` per tile — masonry's
  composition root, which already runs all four widget passes (event / update
  / layout / paint) without needing a window or an event loop.
- The substrate hands each `MasonryTile` a rectangle (in tile-local logical
  coordinates) per frame. masonry lays out and paints into a
  `vello::Scene` via the `imaging_vello` backend; `MasonryTile::render`
  appends that scene into the substrate's vello scene at the tile's
  `Placement` transform.
- Input events flow substrate → tile via `MasonryTile::handle_*` calls
  (translated to `ui-events` vocabulary, which masonry already speaks).
  Output signals flow tile → substrate via `drain_signals()`, which yields
  the masonry `RenderRootSignal` enum mapped onto a substrate-shaped
  `TileSignal` (cursor changes, IME area moves, action emissions, etc.).
- AccessKit `TreeUpdate` is exposed via `take_accesskit_update()` for the
  caller (likely `uxtree`) to merge into Mere's accessibility tree.

The driver does not own xilem. Callers who want the reactive `View<T, A>`
ergonomic compose `xilem_core + xilem_masonry` on top of the raw masonry
tree this crate exposes; callers who want to write masonry widgets directly
can do that too. **Both paths use the same `MasonryTile`** — the xilem layer
is opt-in.

## Where this crate sits

```text
mere-domain panels (workbench, orrery, gloss, apparatus, ...)
    │
    │  describe panel UI as `xilem::View<PanelState, PanelAction>`
    ▼
xilem_core (reactive runtime)            ← upstream, unmodified
    │
    │  via xilem_masonry adapter         ← upstream, unmodified
    ▼
masonry_core widget tree                 ← upstream, unmodified
    │
    │  hosted by `MasonryTile`           ← THIS CRATE
    ▼
substrate vello scene at tile.placement.transform
    │
    │  via netrender → vello → wgpu surface
    ▼
mere-host (current: gpui via PlatformSurface; future: substrate-as-host)
```

The boundary between this crate and the substrate is one trait
([`InScenePaintRenderer`](../mere-renderer-registry/src/renderer.rs), defined in
[`mere-renderer-registry`](../mere-renderer-registry/) per the
[renderer-registry brief](../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md)).
[`src/renderer.rs`](src/renderer.rs) holds the `NodeRenderer` + `InScenePaintRenderer`
impl on `MasonryTile`.

## What's wired vs not

**Wired (concrete)**:

- `MasonryTile::new(default_properties, initial_size, scale_factor)` — constructs a `RenderRoot` with our signal sink.
- `MasonryTile::handle_pointer / handle_text / handle_window_event / handle_access_event` — forwards substrate-translated events into masonry.
- `MasonryTile::drain_signals` — yields `TileSignal` (the substrate-shaped projection of `RenderRootSignal`); full coverage of all 17 `RenderRootSignal` variants.
- `NodeRenderer` + `InScenePaintRenderer` impls on `MasonryTile` ([`src/renderer.rs`](src/renderer.rs)) — registers as `RendererId::from_static("mere-masonry")`, handles `NodeContentKind::Panel`, declares `INTERACTIVE_PANEL` capabilities.
- Text / IME / Focus translation in [`src/input.rs`](src/input.rs) — `translate_to_masonry_text` is fully implemented for `Text` / `Ime` / `Focus` and unit-tested.

**`todo!()` markers, three real ones**:

- `MasonryTile::resize` — needs `WindowEvent::Resize(size)` API verification against pinned masonry version.
- `MasonryTile::render` — needs the canonical per-frame paint method on `RenderRoot` pinned (masonry_winit's render path is the reference).
- `MasonryTile::take_accesskit_update` — needs the canonical `TreeUpdate` extraction call pinned.

**Implementation-deferred (mapping table written, construction `None`-stubbed)**:

- `translate_to_masonry_text` for `InputEvent::Key`: KeyboardEvent construction depends on `ui-events` version; mapping table in doc comments.
- `translate_to_masonry_pointer` (entirely): PointerEvent construction depends on `ui-events` version; mapping table in doc comments.

**Not wired (separate concern)**:

- The substrate-side OS-event source + translator (which produces `mere_renderer_registry::InputEvent` from raw OS events) lives in `mere-host-runtime` and isn't built yet. `mere-masonry` is the consumer side; the producer is independent work.
- The `xilem_masonry` adapter integration showing how callers compose a reactive `View<T, A>` on top. Sketched in a doctest comment in `lib.rs`, not actual code — that's the host's wiring concern, not this crate's.

## Reading order

1. [`src/lib.rs`](src/lib.rs) — module structure + the public surface (`MasonryTile`, `TileSignal`).
2. [`src/tile.rs`](src/tile.rs) — the `MasonryTile` struct and its lifecycle methods.
3. [`src/signal.rs`](src/signal.rs) — `RenderRootSignal` → `TileSignal` mapping.
4. [`src/input.rs`](src/input.rs) — registry `InputEvent` → masonry `TextEvent` / `PointerEvent` translation, with tests.
5. [`src/renderer.rs`](src/renderer.rs) — `NodeRenderer` + `InScenePaintRenderer` impls on `MasonryTile`; the registry seam.

Total LOC under 400 at this stage; well below the
[`feedback_mere_file_size_ceiling`](../../../../.claude/projects/c--Users-mark--Code/memory/feedback_mere_file_size_ceiling.md) 600-LOC ceiling per file.
