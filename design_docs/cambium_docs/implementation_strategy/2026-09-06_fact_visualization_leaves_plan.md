# Cambium fact-visualization leaves

**Date:** 2026-09-06
**Status (2026-09-06):** V0 and V1 landed; V2 planned

## Decision

Cambium will add three small visualization contracts pulled by Cleromancy's
landed fact surfaces:

1. a read-only angle strip for several positions on one cyclic range;
2. a read-only dimension line for one measured relation;
3. a range-bounded scrubber with durable pins.

The first two are tier-2 Sprigging vector leaves. They paint geometry only.
Names, values, units, provenance, and alternate tabular readings remain normal
DOM owned by the application view. The scrubber is a later interactive Cambium
component over the existing pointer, keyboard, and retained-state seams.

This plan owns the reusable rendering and interaction contracts in Mere. The
first consumer remains in `cleromancy`, whose
`cleromancy/design_docs/2026-09-04_legible_reader_and_fact_surfaces_plan.md` records the
original asks and owns the astrology projection. Mere does not learn zodiac
signs, aspects, ephemerides, or millidegrees.

## Findings

- **2026-09-06:** Cleromancy R4 is landed and already exposes replay-verified
  stored chart positions and aspects through a DOM-legible grid. A vector leaf
  can replace or accompany that rendering without changing its read model or
  durable schema.
- **2026-09-06:** Sprigging's Path-A leaves already provide retained dirty
  state, clipping, resizing, portable paint commands, and AccessKit fallback
  semantics. `GraphCanvas`, `Meter`, and `Knob` establish the applicable
  pattern in `crates/cambium/sprigging/src/glyphs.rs`.
- **2026-09-06:** Cambium's component catalog is the required first acceptance
  surface for reusable components. Applications synchronize concrete leaves
  into the host-owned registry through `HostHooks::frame`.
- **2026-09-06:** The angle-strip consumer needs several simultaneous body
  positions. A scalar one-marker leaf would force overlapping leaf elements
  and would not express the product's actual comparison task.

## Ownership and stop lines

| Concern | Owner |
|---|---|
| Normalized geometry, paint commands, dirty state | Sprigging |
| Reusable DOM/component interaction contract | Cambium |
| Layout, paint splicing, input, accessibility publication | Genet |
| Stored values, labels, units, selection, provenance | Consumer application |
| Astrology calculations and interpretation | Outside Mere |

All geometry inputs are finite normalized values. The component may normalize
or clamp at its own boundary, but it does not convert application units or
derive meaning. Read-only leaves emit no action. Painted marks never become the
only representation of a fact.

## Phases

### V0. Read-only angle strip

Add a Path-A `AngleStrip` with multiple normalized marks, an intrinsic size,
plain rectangular track and marker styling, clipping, and retained dirty-state updates. Export it
through Sprigging and Cambium and add a labelled specimen to the executable
component catalog. Cleromancy then places one strip above its stored-position
grid, keeps the grid as the full semantic reading, and synchronizes the leaf
from the already-selected stored chart.

**Done when:**

- finite positions wrap to the cyclic `0..1` range and non-finite positions
  have an explicit deterministic fallback;
- paint output is clipped, orders the track before every marker, and responds
  to layout-size changes through the existing leaf cache;
- unchanged marks stay paint-clean while changed marks repaint;
- the catalog registers, renders, and labels the specimen while keeping its
  human-readable values in DOM siblings;
- Cleromancy renders every selected stored longitude in one strip, retains the
  positions grid and source block, and emits no product action;
- focused Sprigging, Cambium catalog, and Cleromancy chart receipts pass in
  default and `analytic-ephemeris` builds.

### V1. Read-only dimension line

Add one vector leaf for a measured relation between two normalized endpoints.
The consumer supplies endpoint labels, the measured value, units, and relation
kind as DOM. Cleromancy may place it beside a selected aspect while retaining
the aspects grid as the complete reading.

**V1 rulings (2026-09-06):**

- The caller supplies ordered endpoints, `Direct` or `Wrapped` traversal, and
  an optional absolute normalized target tick. Cambium clamps coordinates but
  does not infer direction, shortest paths, units, or relation meaning.
- A filled start marker and ring end marker preserve endpoint order. Wrapped
  traversal paints from start to the upper boundary and from the lower
  boundary to end.
- Cleromancy displays the shortest stored separation in increasing direct
  order or increasing across the `360/0` boundary. That convention is a
  product projection and does not change stored astrology facts.

**Done when:**

- endpoint order and wrap policy are inputs rather than inferred domain rules;
- the leaf paints endpoints, the measured span, and an optional exact target
  without text or domain vocabulary;
- a catalog specimen and Cleromancy aspect consumer cover ordinary, wrapped,
  and coincident endpoints;
- the component is read-only and the DOM remains sufficient by itself.

### V2. Range scrubber with pins

Add a Cambium-owned, controlled scrubber over an application-provided finite
range. Pins are labelled application values, not persisted component state.
The application owns range selection and any fetch, calculation, or storage
effect after a scrubber event.

**Done when:**

- pointer, arrows, Home/End, PageUp/PageDown, and assistive `SetValue` follow
  one clamped stepping policy;
- minimum, maximum, current value, step, page step, disabled reason, and every
  pin label are exposed semantically;
- a range change reconciles local focus safely and cannot leave an out-of-range
  value;
- the component catalog covers empty pins, dense pins, disabled state, and
  keyboard routing;
- the first product adoption proves application-owned side effects and
  persistence.

## Verification wall

```powershell
cargo test -p sprigging --lib --locked
cargo test -p cambium --example component_catalog --all-features --locked
cargo run -p cambium --example component_catalog --all-features -- --write-receipts
```

Consumer verification is run in Cleromancy with its own isolated target:

```powershell
$env:CARGO_TARGET_DIR = 'C:\t\cleromancy-cambium-angle'
cargo test --test chart_surface_dom --offline -j 1
cargo test --test chart_surface_dom --features analytic-ephemeris --offline -j 1
```

## Progress

- **2026-09-06:** Plan founded from Cleromancy's accepted Cambium asks after
  R4 landed. V0 selected as the first bounded implementation; V1 and V2 remain
  independent later phases.
- **2026-09-06:** V0 landed. Sprigging now provides a read-only multi-mark
  `AngleStrip` with cyclic normalization, deterministic non-finite fallback,
  clipping, resize-aware retained paint, and a dirty gate. Cambium exports it
  and the component catalog carries a six-mark specimen with DOM sibling
  values. Cleromancy's Chart surface is the first consumer and retains its full
  positions grid and source identity. Sprigging passes 23/23; the catalog
  passes 2/2 and its narrow and regular receipts were regenerated; Cleromancy
  Chart passes 3/3 in default and `analytic-ephemeris` builds; adjacent UI
  regressions pass 3/3; and the `portable-core` check passes. Independent
  review found and closed visibility, cache-retention, second-selection,
  track-contract, and resize-receipt gaps. Final review is clean.
- **2026-09-06:** V1 landed. Sprigging now provides a read-only
  `DimensionLine` with caller-ordered endpoints, explicit direct or wrapped
  traversal, an optional absolute target tick, clipped endpoint styling, and
  retained dirty/resize behavior. The catalog carries ordinary, wrapped, and
  coincident specimens with DOM sibling values. Cleromancy's Chart surface
  adds a view-local aspect selector and supplies the documented increasing
  shortest-path convention while retaining the complete aspects grid and all
  source identity. Sprigging passes 28/28; the catalog passes 2/2 and both
  receipts were regenerated; Cleromancy's projection tests pass 3/3; and Chart
  passes 3/3 in default and `analytic-ephemeris` builds. Independent review
  found and closed chart-switch focus and selector-reconciliation gaps; final
  review is clean.
