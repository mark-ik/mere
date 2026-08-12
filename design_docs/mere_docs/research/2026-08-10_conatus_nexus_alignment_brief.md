# Conatus / Nexus Alignment Brief

**Date**: 2026-08-10
**Status**: research brief, commissioned by Mark: nexus, rust-gpu, and
renderling are "the trio of note now", dimforge is in an odd transition, and
the goal is one physics solution across conatus and the isometry wing:
"i am happy to adapt conatus and the rest to fit that new direction, but i
wouldn't let rapier hold us back."
**Ruled 2026-08-12 (Mark)**: perfect determinism is not required for the
isometry wing; §4 is rewritten from a gate into an authority model, and the
rapier exit (§5 A4) no longer waits on a determinism receipt.

**Related**: the numen founding stack (numen → quint → seiche), the
[scenograph expansion brief](2026-08-10_scenograph_expansion_brief.md)
(picking moved scene-side at the 0.0.3 freeze), the burn utilization brief
(burn is the endorsed tensor direction), the esp consolidation plan (the
seam doctrine this brief mirrors).

## 1. Verified facts (2026-08-10, against manifests)

- **conatus's three roles**: numen holds field definitions as plain data
  (WASM-clean); quint evaluates them (Rhai authoring, **Burn lowering** — the
  GPU story for *field evaluation* already exists and is the endorsed
  direction); seiche integrates forces and is **rapier-backed** ("kernel-free"
  means no compute kernels, not no rapier).
- **The only rapier in the ecosystem is inside seiche.** Isometry carries no
  rapier and no physics dependency anywhere (root manifest and workspace
  greps both empty; one archived design doc touches adjudication). Its game
  loop shipped without rigid-body physics.
- **Consequence**: "losing rapier" means re-seating seiche's integrator, and
  isometry is greenfield: it *adopts* the unified stack rather than
  migrating off anything.

## 2. The trio, grounded

- **Nexus** (dimforge): cross-platform GPU multiphysics, explicitly "rapier
  on the GPU". The whole pipeline is compute shaders written in Rust via
  **rust-gpu**, compiled to SPIR-V, executed through WebGPU (or Metal, CUDA,
  CPU). Rigid bodies, colliders, joints, articulated multibodies today.
  Pre-1.0. The dimforge transition: rapier is now the CPU incumbent, nexus
  the direction (Q2 2026 technical report).
- **rust-gpu**: shaders in Rust; relevant here as nexus's substrate, not as
  a lane conatus adopts directly. quint's GPU lowering stays Burn; replacing
  it with hand kernels would cut against the burn brief for no named gain.
- **renderling**: GPU-driven rendering. That contest belongs to genet's
  pluggable `SurfaceEngine` multiplexer lane, not to conatus; named here
  only so the trio stays together in one place. Wants its own genet-side
  brief when a consumer forces it.

## 3. The decision frame: three seats, one contested

1. **Field evaluation** (quint + Burn): unchanged. Nexus does not compete
   here.
2. **The integrator seat** (seiche's internal rapier): the contested seat.
   The alignment move is a seam, not a swap: seiche's integrator becomes
   pluggable, rapier stays the working default, nexus arrives behind a
   feature with parity receipts. This mirrors the esp doctrine exactly:
   host-selected device, backend behind a feature, cooperative yield, and
   the host scheduler owns the render-versus-compute budget (the D1 rule;
   on WebGPU the physics pass contends with genet's own render queue).
3. **Game physics for isometry**: greenfield adoption of the conatus seam,
   per propagate-capability-up-the-stack. Founding an isometry-local physics
   vocabulary would be the duplication the ecosystem rule exists to prevent.

## 4. The authority model: facts, not trajectories (ruled 2026-08-12)

The first draft made determinism the deciding axis. Mark ruled it out of
that seat: perfect determinism is not necessary for the isometry wing. The
multiplayer truth is **key, replayable facts represented correctly to hosts
and guests alike**: adjudicated outcomes authored as facts and replicated
over the rails isometry already rides (murm, stickleback, codicil). The
simulation *between* facts is local color, free to diverge across devices,
and losing nonessential data for excellent GPU throughput is a good trade.
Bit-perfect representation stays valuable where it is cheap, but it gates
nothing.

What this keeps required: exactness at the fact boundary (an adjudicated
outcome is one fact, not N slightly different ones), and identical
*representation* of received facts on every peer. What it releases: lockstep
networking, cross-vendor trajectory identity, and any obligation to keep
rapier for its determinism mode. Per the ruling: do not preserve rapier for
a determinism the gameplay does not need — **unless it composes nicely into
nexus**, which turns rapier's fate into A0's composition question (shared
parry/nalgebra types, a rapier-equivalent CPU path) rather than a
determinism experiment.

## 5. Sequence, entrance-gated

- **A0**: read nexus's actual API and its type lineage: does it keep parry
  and nalgebra (seiche's colliders and mere-canvas hit-testing ride parry
  shapes; if nexus keeps them, the hit seam survives a swap), and is its CPU
  execution path a rapier equivalent (if so, rapier can exit entirely and
  "composes into nexus" is answered by construction). Cheap, unblocks
  everything.
- **A1**: seiche integrator seam; rapier default; no consumer change.
- **A2**: isometry adopts the conatus seam for its first physics slice; this
  is the receipt that the seam fits a game, not just a graph canvas.
- **A3**: nexus behind a feature, with behavioral receipts: forces within
  tolerance, stable under load, the hit seam intact, fact-boundary outputs
  exact. Bit-trajectory comparison is recorded if it is cheap and gates
  nothing (§4 ruling).
- **A4**: the rapier exit, decided on seat coverage plus A0's composition
  answer, explicitly not on determinism (§4 ruling). If nexus's CPU path
  covers the seats, rapier exits entirely; otherwise it stays only as the
  CPU fallback backend behind the seam.

## 6. Non-goals

- Replacing quint's Burn lowering with rust-gpu kernels.
- A second physics vocabulary in isometry.
- A renderling verdict (genet-side lane, separate brief).

## 7. Progress

- **2026-08-12 — A0 answered, by nexus's own manifests.** Nexus is not a
  rapier replacement; it *contains* rapier. The workspace pins
  `rapier2d`/`rapier3d` 0.34 and `parry2d`/`parry3d` 0.29, and
  `nexus_rbd3d` depends on `rapier3d` + `parry3d` directly (math is glamx;
  the GPU layer is khal + vortx, not wgpu-direct or rust-gpu-direct at the
  manifest level). Both A0 questions close at once: parry survives, so
  seiche's collider/hit seam carries over; and the CPU substrate *is*
  rapier, so Mark's "unless it composes nicely into nexus" branch is
  satisfied by construction. Consequence for A4: "rapier exits" reframes as
  "rapier stops being our direct pin and becomes nexus's internal detail";
  the A1 seam should target nexus's API surface and let it carry rapier,
  rather than maintaining two integrator wrappings. Version note for A1:
  align seiche's rapier pin toward 0.34/parry 0.29 so the two stacks don't
  hold divergent parry generations in one graph.
