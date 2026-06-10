# Retired Host-Stack Salvage Map

**Date**: 2026-05-22
**Status**: Reference for the substrate-as-host demolition. Precedes deletion.
**Companion**: the [app re-scaffold](../technical_architecture/2026-05-21_app_architecture_rescaffold.md) (which retired this stack) and the [graphshell supercrate salvage map](2026-05-22_graphshell_supercrate_salvage_map.md).

---

## Why this exists

The re-scaffold replaced substrate-as-host (a renderer registry + substrate scene + action bus + hand-rolled winit loop on top of Xilem) with one idiomatic Xilem app, `mere/app`. That leaves a cluster of host and renderer crates with no consumer on the live path. This map records what is worth reviving from them before deletion, so the cut loses nothing.

**Operating principle:** every reusable piece below maps to a slice we already deferred, and all of it is **git-revivable**. So the call is *delete now, revive by `git show` when the slice comes up*, rather than carry ~15k LOC of dead weight or extract prematurely (each piece needs adaptation to the masonry path anyway). This is the same "git-revivable, pending removal" stance used for forme's parked machinery.

## Crates being cut

| Crate | LOC | What it was |
|---|---|---|
| `mere/host` (bin `host`) | 3364 | the substrate-as-host binary |
| `mere/host-substrate` | 2012 | substrate ↔ runtime bridge, FrameLayout projection, drop zones |
| `graphshell/shell/host-ports` | 3239 | host-abstraction port traits + frame projection |
| `graphshell/graph/spatial-substrate` | 3444 | the SubstrateScene IR, renderers, LOD, external-texture composite |
| `graphshell/shell/system/registry/register-renderer` (+`-types`) | 1704 | renderer-registry trait + dispatch contract |
| `verso/masonry-renderer` | 1666 | renders masonry tiles to a wgpu texture for substrate compositing |
| `verso/scrying-renderer` | 346 | `EmbeddedFrameRenderer` adapter plugging scrying into the registry |
| `graphshell/shell/system/control-plane` | 665 | the action bus + permission gates (cut deferred; see note) |

`control-plane` is left for a small follow-up because cutting it means editing a survivor (`session-runtime`'s compatibility re-export). This pass stays pure deletion + workspace-manifest edits.

## Worth salvaging (git-revivable; each maps to a deferred slice)

| Module (in a cut crate) | Substance | Revive for |
|---|---|---|
| `spatial-substrate/external_texture.rs` | composite a wgpu texture into a vello scene with no CPU copy (vello `register_texture` + `Scene::draw_image`), same-device | the **scrying.web tile widget** and any engine-texture tile |
| `verso/masonry-renderer/embedded_renderer.rs` | the producer side of that composite (tile → wgpu texture) | same, as reference for the texture round-trip |
| `spatial-substrate/lod.rs` | per-node LOD promotion given the camera | GraphCanvas **LOD** slice |
| `mere/host/cartography_projection.rs` + `strategy_registry.rs` + `view_preset.rs` | host-side wiring of `cartography::LayoutStrategy` (which strategy per pane, named by preset) | GraphCanvas **real cartography layout** slice |
| `mere/host/graph_registry.rs` | `GraphId → Graph`, frame leaves carry `graph_id` | **multi-graph windows** |
| `host-substrate/frame_layout.rs` | FrameLayout tree → per-leaf bounds + splitter-drag state | a real **resizable / persistable FrameTree** (mere/app uses raw `split()` today) |
| `control-plane` | permission / capability gates (`PermissionGate`, `check_permission`, target-scoped dispatch) | **capability gating** (the bus *mechanism* is retired; the permission model is the keeper) |

## No salvage (delete freely)

- `mere/host` runtime / setup / render / main / panels / diagnostics / splitter / node painters: replaced by `mere/app` + the GraphCanvas widget.
- `host-ports` abstraction traits: masonry is the paint/input/surface port now.
- `register-renderer` (+types): the masonry `Widget` trait replaces the registry contract.
- `spatial-substrate` scene / recording + solid-rect renderers / accessibility / thumbnail: substrate scene replaced by the masonry scene; a11y via AccessKit; thumbnails overlap `session-runtime::switcher_thumbnail`.
- `masonry-renderer`'s substrate round-trip: exactly the layer the re-scaffold eliminated.
- `scrying-renderer`: the registry adapter only; substance lives in the kept `inker/engines/scrying-engine` (the WebView producers + texture import).

## What is kept (so it is clear we lose wiring, not substance)

- **Layout intelligence:** `cartography`, `graph-layout`, `graph-canvas` (the algorithms) stay. Only the host-side *orchestration* of them is cut.
- **The WebView engine:** `inker/engines/scrying-engine` stays. `scrying-renderer` was only its registry shell.
- **Session persistence:** `session-runtime` (manifests, view-intent, engine-profile, switcher-thumbnail) stays; it goes consumer-less until the new host rewires it.

## Reviving

The pre-deletion commit is the source. To revive a module: `git show <pre-deletion-rev>:<path>`. The re-scaffold §7.2 deferred-slice list carries one-line pointers back to the rows above, so each future slice (cartography layout, LOD, scrying.web tile, multi-graph, FrameTree) finds its prior code.
