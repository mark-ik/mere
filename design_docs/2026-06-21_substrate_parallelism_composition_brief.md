# Substrate / Parallelism Composition Brief

**Status:** synthesis brief. Composes two existing docs; adds the seam analysis,
the co-located-actor decision, the serialization "better way", and the web build
budget. Codebase- and doc-grounded; external facts inherited at the confidence
levels of the source docs.
**Date:** 2026-06-21.
**Scope:** how the [cross-platform parallelism strategy](mere_docs/research/2026-06-19_cross_platform_parallelism_strategy.md)
(P-doc: how the engines go fast, cross-platform) and the
[DocumentScript substrate plan](mere_docs/implementation_strategy/2026-06-21_document_script_substrate_plan.md)
(D-doc: what a script is allowed to reach, across languages, sandboxed) fit into
one stack.

One line: **they are two layers of the same stack meeting at one substrate.**
P-doc is the performance / portability layer of the engines; D-doc is the
capability layer over them. The script contract's capability profiles are grants
over the exact subsystems the parallelism work makes fast.

---

## 1. Where they physically meet

```
  UI / main thread   ┌────────────────────────────────────────────────┐
                     │ kernel (armillary, !Send) · sole GPU owner       │
                     │ netrender → WebGPU (web) / Vulkan·DX12 (native)  │  P-doc lever (e)
                     └───────────────▲────────────────────────────────┘
                                     │  Send msgs + Scene copy   ──── shared cost #1
  per-origin actor   ┌──────────────┴─────────────────────────────────┐
  (native thread /   │  content actor                                   │  P-doc lever (a)
   Web Worker;       │  ┌───────────────────────┐ ┌──────────────────┐ │
   1 per origin)     │  │ serval engine          │ │ DocumentScript   │ │
                     │  │ cascade/layout/shape   │ │ component(s)     │ │
                     │  │ + legacy JS (Nova/Boa) │ │ capability-      │ │
                     │  │   §10.1 native lane     │ │ confined         │ │
                     │  └───────────────────────┘ └────────▲─────────┘ │
                     │     P-doc owns this          ABI copy│ ── shared cost #2
                     └──────────────────────────────────────────────────┘
```

Three things straddle both docs:

- **The armillary actor boundary.** P-doc runs the serval cascade off the UI
  thread in a content actor and ships back `Send ContentUpdate::Scene`. D-doc P2
  hosts each script component in an armillary actor, "one per origin." Same
  substrate, different occupants.
- **The serialization seam.** P-doc's Scene-across-`postMessage` (cost #1) and
  D-doc's per-interaction canonical-ABI copy (cost #2, §7.2) are the same cost
  family. Both docs flag it; both leave it unmeasured. §5 is the joint answer.
- **The PWA / open-web fork.** P-doc establishes it (SAB threads are
  cross-origin-isolation-gated, so PWA-only); D-doc inherits it.

---

## 2. The apparent contradiction, resolved

P-doc: "WASI is out; Wasmtime is excluded by the no-JIT browser policy." D-doc:
"native-first on Wasmtime." These do not collide once the lane is named:

- **Native:** the component runtime *is* Wasmtime (Cranelift, or AOT `.cwasm` +
  a no-codegen executor). P-doc's "Wasmtime out" was only ever about the browser.
- **Browser:** D-doc does not ship Wasmtime either. It reaches the browser via
  the jco AOT polyfill (transpile component → core wasm + JS glue), run on the
  browser's own wasm engine. No JIT enters the browser in either doc.

Deeper agreement: P-doc §1 says WASI 0.3 is async *concurrency, not parallelism*;
D-doc §9 says the substrate is *concurrency + capability, not in-script
shared-memory parallelism*. Same distinction, stated twice. Aligned, not in
tension.

---

## 3. The parallelism axes line up one-to-one

| Parallelism | P-doc (engine) | D-doc (script) | Lane |
|---|---|---|---|
| **Across units** | inter-actor / inter-page, header-free (lever a) | "parallel across instances" (P2, §9) | both lanes |
| **Inside one unit** | SAB Rayon parallel cascade (lever c) | in-script threads (deferred, §8) | **PWA-only, hard-gated** |

"N pages render concurrently" and "N script components run concurrently" are the
same mechanism: one actor per origin, off-main-thread. A compute-heavy script that
ever wants threads *inside itself* inherits P-doc's entire SAB / COOP-COEP /
nightly-build-std apparatus, gated exactly like the parallel cascade.

The script kinds map onto the fork too: native extension / app scripts → PWA / app
lane; arbitrary legacy web JS → open-web lane, and per D-doc §10.1 it stays the
native `Runtime<Nova/Boa>`, never a component. The lane fork and the script-kind
split are the same line.

---

## 4. Decision: the script host is the content actor (co-located)

Both docs say "one actor per origin" but mean different occupants. Resolution:
**one per-origin `!Send` actor owns the serval engine *and* hosts that origin's
DocumentScript components.** The capability boundary sits at the *component* edge,
not the actor edge. On web, a jco-transpiled component runs inside the same Web
Worker as the engine. This is the first P2 design call and it is what makes §5
work (the script ↔ engine hop stays in-worker).

---

## 5. The serialization cost, and the better way

The fear: a scripted page on web pays the Scene clone (worker → main, cost #1)
*and* the component-ABI copy (script ↔ engine, cost #2) per interaction. The
correction:

- **Co-location (§4) makes cost #2 an intra-worker memcpy**, not a second
  `postMessage`. Scripting does not add a second cross-thread hop; it adds a cheap
  in-worker copy between linear memories.
- **Cost #2 is irreducible but small.** The canonical ABI copies by design (that
  is the isolation price; the model forbids shared heaps, D-doc §2.2). The
  per-turn batched contract (§10.2) + coarse mutation variants + a cached
  interpreter mirror (§10.3) make it one small copy per turn, not many per DOM op.
- **Cost #1 is the only expensive hop, and it pre-dates scripting.** Every page
  ships its Scene to the main-thread GPU owner whether or not it is scripted. So
  scripting does not worsen it. Cure it independently: encode the Scene as a flat,
  pointer-free buffer and *transfer* the `ArrayBuffer` (zero-copy ownership move)
  instead of structured-cloning it; ship incremental damage (band-scroll +
  per-tile generation cache already exist host-side); use SAB in the PWA lane.
- **One wire discipline serves both seams.** Both want a flat, position-independent
  representation (the component ABI forbids object graphs anyway). A single
  flat-buffer format makes the Scene transferable *and* the component copy a single
  contiguous memcpy, and it unifies P-doc's missing `ContentUpdate`-`Serialize`
  pass with D-doc's canonical-ABI shape rather than maintaining two regimes.

Do **not** fuse the two seams into one hop. They serve different purposes (one is
the isolation boundary, one is the thread boundary). Make each cheap in its own
idiom: co-locate so the isolation hop is a memcpy, transfer so the thread hop is
zero-copy.

---

## 6. The web build budget

The feared three-axis matrix ({threaded vs fallback} × {jco-component vs not} ×
{std vs no_std guest}) collapses, because two axes are not host-build multipliers:

- **jco component support is not SAB-gated.** Transpiled core wasm runs
  single-threaded on the baseline engine, so it rides both web builds.
- **std vs no_std is a per-extension property + a host capability policy**, not a
  Mere artifact (D-doc P0 finding: a std guest imports the WASI world and must be
  granted it; a no_std guest carries only its declared capabilities).

What remains:

| Build | Target | Toolchain | Threads | jco components |
|---|---|---|---|---|
| **native** | desktop (pelt) | stable | native OS threads | n/a (native Wasmtime) |
| **web-baseline** | open-web + non-isolated PWA | stable | off (sequential fallback) | yes (single-threaded) |
| **web-threaded** | cross-origin-isolated PWA | **nightly + -Zbuild-std + atomics + shared-memory link** | SAB Web Workers | yes (single-threaded) |

- **3 builds, 2 toolchains.** The only expensive CI burden is the nightly
  std-rebuilding lane for the one threaded web artifact, which P-doc already owns.
- **Runtime selection** picks web-baseline vs web-threaded via
  `wasm-feature-detect threads()` and the presence of COOP/COEP. Threading and
  off-main-thread are the same move (the Rayon-driving wasm must live in a Worker;
  the main thread may never `Atomics.wait`).
- **Per-extension** is a separate, smaller table: each DocumentScript component is
  its own artifact, built once by its author (std or no_std); the host runs a
  capability-policy table (which profiles + whether the WASI floor is granted) per
  trust tier. This is D-doc §10.4 / §10.5 territory, not core CI.

---

## 7. What to measure / decide next

- **The flat transferable Scene** (cost #1): add `Serialize`/flat-encode to the
  `ContentUpdate` DTOs and measure per-frame transfer vs structured-clone of a
  representative Scene. This is the P-doc de-risking step, now also the thing that
  makes scripted pages cheap on web.
- **Cost #2 per turn** on native and web (jco): memcpy of a representative
  mutation batch through the canonical ABI. Confirm the per-turn batched contract
  keeps it negligible next to relayout.
- **The capability-policy table** (D-doc §10.4): how a grant becomes a linking
  decision (stubbed vs unlinked import), and the std-WASI-floor question per trust
  tier (D-doc §10.5 runtime-sharing tension).
- **The web-threaded artifact** stays an opt-in, app-lane-only experiment behind
  feature detection plus a homogeneous-core check (P-doc lever c). It is never the
  baseline, for engine cascade or in-script threads alike.

---

## Grounding / links

- [cross_platform_parallelism_strategy](mere_docs/research/2026-06-19_cross_platform_parallelism_strategy.md)
  (P-doc: levers a-e, the PWA / open-web fork, the Scene serialization seam, WASI-out).
- [document_script_substrate_plan](mere_docs/implementation_strategy/2026-06-21_document_script_substrate_plan.md)
  (D-doc: the capability profiles, the per-turn contract §10.2, the transaction
  contract §10.3, the capability-grant mechanics §10.4, runtime-sharing tension §10.5).
- [actor_constellation_plan](mere_docs/implementation_strategy/2026-06-03_actor_constellation_plan.md)
  (the shared armillary actor substrate both ride on).
