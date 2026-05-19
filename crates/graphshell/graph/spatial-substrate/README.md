# spatial-substrate

Phase-3 proof harness for the spatial chrome IR substrate. First real
adopter of [`register-renderer`](../register-renderer/).

See [`design_docs/mere_docs/implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../../design_docs/mere_docs/implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md)
§ Phase 3 for the framing. This crate is the "if crate isolation is
useful" branch of the suggested home — sibling to `host` rather
than nested inside `host-runtime`, because the registry pulls vello
+ wgpu and `host-runtime` is wasm32-clean.

## Scope (v0a, 2026-05-16)

Wires the dispatch loop end-to-end and proves the registry contract is
usable for real frame production:

- `SubstrateScene` — flat list of placed scene nodes (Phase-3 shape;
  edges and spatial index land in later phases).
- `SubstrateHost` — owns a `RendererRegistry`, walks the scene per
  frame, dispatches paint + input through the registry's
  composition-mode-aware helpers.
- `RecordingRenderer` — built-in test fixture that records what it was
  asked to paint / handle input for; used in this crate's tests and
  reusable as a stub in downstream tests.

What's **not** here yet:

- No winit / wgpu device / window — the prototype produces a
  `vello::Scene` but doesn't rasterise it. GPU rasterisation lands when
  `SubstrateHost` gets a window backend (Phase-3 done condition).
- No edges / relations — Phase-3 done conditions name a relation-edge
  pass, but the IR brief leaves edges as an open question (§10.1); v0a
  just exposes node dispatch.
- No accessibility tree projection — the IR brief's
  `accessibility-tree-emit` system (§5) is named but not implemented
  yet.
- No spatial input router — `SubstrateHost::deliver_input` targets a
  node by identity directly; the hit-test layer that maps host
  coordinates to a node identity is a separate piece.

## Why this exists

> "The architectural arc is rich with briefs but has no running prototype
> demonstrating the substrate-as-host shape; risk of paper architecture."
> — pitfall listed in the 2026-05-16 status briefing.

This crate is the smallest artifact that disproves "paper architecture"
for the registry contract: a real dispatch loop, real tests, exercising
the same trait surface a future substrate-as-host will use.
