# Conatus / Nexus Alignment Brief

**Date**: 2026-08-10
**Status**: research brief, commissioned by Mark: nexus, rust-gpu, and
renderling are "the trio of note now", dimforge is in an odd transition, and
the goal is one physics solution across conatus and the isometry wing:
"i am happy to adapt conatus and the rest to fit that new direction, but i
wouldn't let rapier hold us back."

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

## 4. The deciding axis: determinism

A P2P VTT wants lockstep or at least replay-stable simulation. GPU float
ordering is classically non-deterministic across vendors; rapier documents a
cross-platform determinism mode; nexus's determinism story is unrecorded in
what we have read. **No GPU backend becomes authoritative without an
empirical receipt**: the same scene stepped N times on two GPUs and CPU,
trajectories compared. Until that receipt exists, the CPU path (seiche as
today) stays authoritative and nexus is an accelerator lane (effects,
fast-forward, soft-body flourish), which is also the honest reading of a
pre-1.0 engine.

## 5. Sequence, entrance-gated

- **A0**: read nexus's actual API and determinism posture (docs + source),
  and rapier's current determinism feature state. Cheap, unblocks everything.
- **A1**: seiche integrator seam; rapier default; no consumer change.
- **A2**: isometry adopts the conatus seam for its first physics slice; this
  is the receipt that the seam fits a game, not just a graph canvas.
- **A3**: nexus behind a feature, CPU/GPU parity receipts, the determinism
  experiment from §4.
- **A4**: the rapier exit decision, made on A3's receipts rather than on
  sentiment about dimforge's transition.

## 6. Non-goals

- Replacing quint's Burn lowering with rust-gpu kernels.
- A second physics vocabulary in isometry.
- A renderling verdict (genet-side lane, separate brief).
