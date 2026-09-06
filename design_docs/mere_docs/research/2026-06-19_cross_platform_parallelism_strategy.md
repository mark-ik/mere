# Cross-Platform Parallelism + Performance Strategy: genet / pelt / meerkat / mere

**Status:** research / strategy synthesis. Codebase-grounded; external upstream
facts (vello-on-WebGPU, WASI roadmap) cited at the confidence levels stated.
**Date:** 2026-06-19.
**Scope:** how genet (web engine), netrender (wgpu renderer), pelt (desktop
port), meerkat (host), and mere (umbrella) parallelize for good performance
*cross-platform including the browser*, under the no-JIT browser policy
(Wasmtime out; JS via Nova native / Boa in-browser).

Every load-bearing claim carries a confidence level. Numbers borrowed from the
wider ecosystem (Quantum CSS speedups, SIMD ratios) are marked as **borrowed,
unmeasured in our stack** — do not treat them as bankable for genet/wasm.

---

## 0. The problem, stated honestly

Established empirically this session: genet-layout lays out a 578 KB real page
(cold, full document) in **~100 ms release / ~600 ms debug**, against a 16.7 ms
frame budget (~6x over). That cold path is single-threaded **except** a Rayon
text-shaping pre-pass (`box_tree.rs:1076`, threshold >24 inline leaves). The
Stylo cascade is driven sequentially on purpose (`cascade.rs:696`,
`traverse_dom(&traverser, token, None)` — the `None` is the Rayon-pool slot);
box-tree build is sequential.

**Unmeasured and load-bearing:** we do **not** know what fraction of that ~100 ms
is cascade vs box-tree build vs shaping. This matters because the levers attack
different fractions:

- **Parallel cascade** attacks only the *cascade* fraction.
- Box-tree construction is sequential, pointer-chasing, and **not** trivially
  parallelizable — if it is half the 100 ms, a *perfect* parallel cascade still
  caps the achievable cold-cost win well below 2x.

**De-risking step 0 (prerequisite for the whole parallel-cascade thesis):**
instrument genet-layout to split the 100 ms into cascade / box-tree / shaping /
fragment phases on the 578 KB page, native release. Until that breakdown exists,
"parallelism owns the cold cost" is a hypothesis, not a result.

### Step 0 result (2026-06-21) — measured on synthetic AND real pages

Instrumented `layout_via_box_tree` with env-gated phase timers and built a
breakdown harness (`genet/components/genet-layout/examples/phase_timing.rs` *(historical citation)* <!-- doc-audit: historical-path -->;
`GENET_LAYOUT_TIMING=1`, native release, fresh `TextMeasureCtx` per pass = a true
cold first paint). Two pages, shares of **cold paint (cascade + layout)**:

| Phase | Real 1.2 MB (Wikipedia, 16 author sheets) | Synthetic 539 KB (1 trivial sheet) |
|---|---|---|
| **cascade** | **~30 ms (~32%)** | ~4 ms (~5%) |
| box-tree build | ~8 ms (~9%) | ~4.5 ms (~6%) |
| **inline shaping** | **~45 ms (~48%)** | ~70 ms (~82% of layout) |
| taffy compute | ~6 ms (~6%) | ~8 ms (~10% of layout) |
| fragment readback | ~4 ms (~4%) | ~0.8 ms (~1% of layout) |
| **cold paint (cascade+layout)** | **~93 ms** | ~80 ms |
| one-time: parse / font | ~28 / ~1.5 ms | ~9 / ~2 ms |

**The real page corrected the synthetic-only conclusion — this is why §0 demanded a
real page.** The first pass (synthetic, trivial CSS) put cascade at ~5% and shaping
at ~82%, which read as "parallel cascade is the wrong lever." The real page refutes
that: with real author CSS (16 sheets, complex selectors) **cascade is ~32% of cold
paint**, because the synthetic under-weighted it ~6x precisely by having trivial CSS.
Corrected reading:

- **Shaping is the single largest phase (~48% real)** and is *already* parallelized
  (Rayon pre-pass, native). It degrades to serial on web without the SAB shim, so
  keeping shaping parallel on web is lever #1.
- **Cascade is a strong second (~32% real), and is still serial.** Parallelizing it
  (lever (c): the `Cell`→atomic fix + Stylo parallel mode, native-first) attacks a
  real ~32% — **worthwhile; the synthetic-only dismissal of lever (c) is retracted.**
- Cascade cost scales with author-CSS complexity (the synthetic lacked it); shaping
  cost scales with text volume (the synthetic over-had it). Real pages sit between.

**Shaping parallel vs serial (real page, the web-without-SAB penalty).** Forcing the
shaping pre-pass serial (`GENET_SHAPE_SERIAL`, all other phases unchanged): parallel
**~46 ms** vs serial **~61 ms** — a **1.32x** speedup, ~15 ms saved. So the Rayon
pre-pass buys only ~1.3x on this page (sublinear: the merge is single-threaded, the
font-context clone is per worker, per-leaf work is uneven). This is **well below the
[borrowed] 2-3.5x** §4 cites — that figure stays unverified for our stack, and the one
phase we *can* measure parallel comes in at 1.3x. Lane consequences: the open-web lane
(no SAB) pays the full serial ~61 ms shaping (~+16% on cold paint); the PWA lane can
reclaim ~15 ms via the SAB shim. Net SAB headroom on the PWA lane ≈ shaping ~15 ms +
some fraction of the still-serial ~30 ms cascade; **off-main-thread (lever a) remains
the bigger, universal web win** — it removes freeze regardless of the cold ms, and
needs no SAB.

**Cascade drill-down (real page, de-risking lever (c); genet commit 95622930).** Split
the ~32 ms cascade (`cascade.rs`): traverse_dom (selector match + cascade + computed-value
apply) ~25.7 ms (~80%), serial setup (Stylist build + snapshot/context wiring) ~5.8 ms
(~18%), marker + rule-tree GC ~0.8 ms. The dominant traverse_dom is exactly the slice
Stylo parallelizes (Rayon pool / parallel mode); the serial floor parallelism cannot
touch is only ~6.5 ms. So **parallel cascade is well-targeted, not Amdahl-trapped** — its
ceiling is parallelizing ~26 ms against a ~6.5 ms floor. The open unknown is the *actual*
Stylo-parallel-traversal speedup in our stack (the borrowed 2-3.5x is unverified; the one
parallel phase we measured, shaping, hit only 1.32x). Measuring it needs the `Cell`→atomic
fix + ThreadSanitizer on Fedora (the scope doc forbids the Windows box), so it stays the
next native-first step, not done here.

Method notes: images are not decoded (empty `ImagePlane`), so replaced-element
sizing is approximate; shaping is the *post-parallelism* native number (serial on
web would be higher — the next thing to measure for the web lane). Single Windows
laptop; the relative split is the deliverable, not the absolute ms. (Confidence:
high that cascade and shaping are both first-order and shaping is the largest;
medium on exact percentages — they move with page composition.)

---

## 1. Landscape verdict — the three real levers, and the one distraction

| Lane | Verdict | What it parallelizes |
|---|---|---|
| **Web Workers + SharedArrayBuffer + wasm atomics** (Rayon via a `wasm-bindgen-rayon`-shaped shim) | **The** in-browser CPU-parallelism path — **but app/PWA lane only** (see §5, COOP/COEP) | Shared-memory thread parallelism: the cold cascade/shaping fraction |
| **wgpu → WebGPU (+ WebGL2 fallback)** | **The** GPU path; already portable in netrender's *code shape*, not yet *built* for web | GPU rasterize/composite (vello runs the rasterizer as GPU compute) |
| **WASM SIMD (`simd128`)** | **Secondary** — cheap, baseline, but the *wrong axis* for the headline cold-layout cost | Per-core data-parallel inner loops (CPU raster/blend fallback, UTF-8, shaping inner loops, format conversion) |
| **WASI threads / shared-everything-threads / WASI 0.3 async** | **Distraction** for the browser goal | Nothing usable in-browser |

### WASI is a dead end *for the browser*, by its own charter

WASI defines itself as "WebAssembly's use *outside* the browser." Every WASI
threading mechanism either does not run in browsers, or — where the future
Component-Model `thread.spawn` builtins land — is **explicitly polyfilled to Web
Workers** in a browser, so it yields nothing you don't already get from a
`wasm-bindgen-rayon`-style shim. WASI 0.3.0 (shipped 2026-06-11) delivers async
*concurrency*, not *parallelism*; threads are deferred to a later 0.3.x with no
ship date. WASI's reference runtime is Wasmtime, which the no-JIT browser policy
excludes anyway. (Confidence: high. Sources: wasi.dev/roadmap; shared-everything-threads
Overview; WASI 0.3 release notes; uno-platform "State of WASM" 2025/2026.)

WASI's only legitimate Mere relevance is a **separate, optional non-browser
SSR/edge lane** (genet-on-Wasmtime/Spin doing server-side parallel layout,
shipping baked frames). Park that as a future product question; it is not part of
the cross-platform *browser* build.

### Why only threads touch the headline cost

The ~100 ms is sequential Stylo cascade + sequential box-tree build — branchy,
pointer-chasing, irregular control flow. Therefore:

- **SIMD cannot touch it.** SIMD vectorizes regular, branch-free loops, not
  selector matching or tree walks. SIMD is the right lever for netrender's
  raster/blend and the text/UTF-8 path; the *wrong* lever for cold layout.
  (Confidence: high.)
- **WebGPU cannot touch it.** "Bake to texture" is the GPU rasterize step; it
  does not avoid the CPU layout. The WebGPU spec also forbids multithreaded use
  of a single `GPUDevice`. (Confidence: high.)
- **Only shared-memory threads attack the cold cost directly** — and even then
  sublinearly. (Borrowed figures: Linebender measured diminishing returns past
  ~4 threads; Mozilla caps Stylo at 6; can *regress* on heterogeneous mobile
  cores — Android big.LITTLE, Mozilla bug 1834977.) Threads **stack with, do not
  replace,** incremental layout and off-main-thread.

---

## 2. Portable abstractions — one codebase, two backends

Each lever has a single Rust abstraction lowering to native on desktop and a web
primitive in the browser. **One correction up front, because it is the most
misleading line in earlier drafts:** Rayon is *not* a transparent drop-in on web.

| Abstraction | Native lowering | Browser lowering | Honest status |
|---|---|---|---|
| **Rayon** (`par_iter`, `join`) | OS-thread work-stealing pool | Web Worker pool over SharedArrayBuffer — **only with a `wasm-bindgen-rayon`-style shim wired** | **No *call-site* change.** But the *crate build, toolchain, dependency set, and entrypoint* all change: separate nightly atomics build, new dep, async pool init, **two shipped artifacts**. See the three-state note below. |
| **wgpu** | Vulkan/Metal/DX12/GL | WebGPU (+ WebGL2 fallback) | netrender's *code* is web-aware (async boot, external device, baseline features); the **`webgpu`/`webgl` wgpu features are absent from every manifest** — it cannot target WebGPU until added. |
| **Actor model** (`armillary`) | `std::thread` per actor + `Send` boundary | Web Worker per actor + `postMessage` | `Send` boundary is correct and ~90% done **on native**. On web the message DTOs must additionally be `Serialize` (they are not today) and pay a per-frame structured-clone cost. See §3 meerkat. |
| **SIMD** | SSE/AVX / NEON | `simd128` | Use the **`wide` crate** (stable, cross-target), **not** nightly `std::simd`, **not** hand-ported native intrinsics. Still needs `-C target-feature=+simd128` at build; `wide`'s wasm backend is narrower than its x86 backend (some ops scalarize). tiny-skia already uses this pattern. |

### The Rayon three-state correction (must not be fudged)

There are **three** states, not two:

1. **Plain `rayon`, no atomics (today's green wasm build):** `par_iter` runs
   **serial, inline on the calling thread**. Compiles, correct, **zero browser
   parallelism**. The unguarded shaping pre-pass at `box_tree.rs:1112` is exactly
   this — a green wasm build proves portability, not parallel performance.
   (Confidence: high.)
2. **`rayon` built *with* `+atomics`, no shim:** rayon tries to spawn OS threads;
   `wasm32-unknown-unknown` has none, so it **traps at runtime**. (Confidence:
   high — this is the trap that catches people who think atomics alone buys
   threads.)
3. **`rayon` + a `wasm-bindgen-rayon`-style shim + atomics build:** the shim
   injects the Worker-spawn path rayon-core calls; *now* you get real Web Worker
   parallelism. Requires nightly + `-Zbuild-std`, a new dep, `await
   initThreadPool(...)` once, and a dual-artifact ship (threaded + fallback).

So "Rayon lowers to a Web Worker pool in the browser" is true **only in state 3**,
and state 3 is the expensive one. Earlier "zero API change beyond `initThreadPool`"
framing is **deleted** as inaccurate.

### The expensive-to-retrofit seam is already designed in (native)

`armillary::KernelThread` is `!Send` by construction, so the compiler refuses to
move kernel authority onto an actor thread; actors carry only `Send
ActorHandle<C>`; pinned state (Stylo, Nova, a DOM) stays on its thread because
`spawn()` *builds* it on the actor thread (`armillary/src/actor.rs:96-141`,
`lib.rs:5-32`). The content actor already renders the genet cascade off the UI
thread and ships back `Send ContentUpdate::Scene`
(`meerkat/src/content/mod.rs`, `meerkat/src/content/actor.rs`). The
**`Scene` is the serialization seam** between worker-side layout and main-thread
present — exactly what a browser build needs. (Confidence: high on native; the
web caveat is the missing browser Worker backend around the landed byte envelope,
§3 meerkat.)

---

## 3. Per-layer strategy

### genet (cascade / layout / shaping) — the parallelism owner

- **Today:** builds for `wasm32-unknown-unknown` (exit 0, verified 2026-06-06) —
  **but in an isolated, uncommitted worktree (`wasm-fonts-cut`), not mainline.**
  "genet builds for wasm" is a worktree fact, and it rests on de-IPC and
  de-randomness cuts that are **unlanded**. (Confidence: high; this is a
  provenance caveat, not a contradiction.)
- One active parallel path: the Rayon inline-text shaping pre-pass
  (`box_tree.rs:1087-1146`, threshold 24 at `:1076`). The cascade is deliberately
  sequential (`cascade.rs:696`) — a **first-class Stylo mode**, not a broken
  config (Servo book: "sequential alternatives … for cases where the overhead of
  a parallel traversal is not justified").
- **The cascade parallelism blocker is data races, not missing wiring.**
  `style.rs` uses single-thread shortcuts where Servo uses atomics:
  `selector_flags: Cell<ElementSelectorFlags>` (`:54`), `dirty_descendants:
  Cell<bool>` (`:77`), `handled_snapshot: Cell<bool>` (`:83`), `stylo_data:
  UnsafeCell<…>` (`:48`). A child propagating flags up to a parent's `Cell` is a
  confirmed cross-thread RMW race. Fix = convert the three `Cell`s to atomics
  (`AtomicU16 fetch_or` / `AtomicBool`). Plus the `CASCADE_CTX` TLS panic: worker
  threads see `None` and panic, so `pool.broadcast(...)` must install the `Copy`
  ctx before any parallel `traverse_dom`. **Verify native-first** (Fedora 44 +
  ThreadSanitizer); the scope doc explicitly forbids flipping it on from the
  Windows box — reftests cannot catch a data race
  (`2026-06-13_parallel_cascade_scope.md`). (Confidence: high on the race
  diagnosis; the fix is unlanded.)
- **Three browser blocker classes beyond threads** (the first is widely missed):
  1. **No clock.** On `wasm32-unknown-unknown`, `Instant::now()` / `SystemTime`
     **panic**. genet-layout itself is clean (no `std::time`), but the host path
     is not — meerkat animation scheduling, debounce/throttle, and `gyre` physics
     use `Duration`/timing (`orrery/physics.rs:25`). All time must route through
     `web_time` / `performance.now()`. Pervasive, easy to miss. (Confidence:
     high.)
  2. **Randomness.** genet's own 2026-06-06 plan names `uuid` v4 as wasm-hostile;
     `getrandom` needs the `js` feature to bind `crypto.getRandomValues`. The
     green worktree build depends on these cuts, which are unlanded. (Confidence:
     high.)
  3. **Fonts.** The core layout path is already filesystem-free (sheets as `&str`
     + an `ImageLoader` trait, `host_loader.rs:5-10`). But `FontContext::new()`
     discovers *system* fonts via fontique's `system` feature
     (`Cargo.toml:23`, a hardcoded non-optional feature) with **no browser
     equivalent**. The byte-blob path works (Ahem via `register_fonts(Blob)`,
     `text_measure.rs:347`). Web font story: make `system` a per-target/cfg
     feature (a real manifest edit — workspace deps don't take per-target features
     easily), feed font bytes from the host (`@font-face`/fetch → `register_fonts`),
     and **ship at least one embedded fallback font in the wasm bundle** — without
     system fonts *and* without loaded web fonts, text renders **nothing**.
     (Confidence: high.)
- **Native (pelt):** flip Stylo to its **parallel mode** for the cold
  full-document pass — proven tech (Quantum CSS, Firefox 2017), the cleanest,
  lowest-risk place to reclaim the 100 ms→sub-budget gap, and the place to prove
  the `Cell`→atomic fix *before* it ever rides a wasm thread pool. (Confidence:
  high that native is the right first proving ground; the speedup magnitude is
  borrowed, §4.)

### netrender (wgpu → WebGPU) — least in the way, but never compiled for web

- **Structurally web-aware:** no `std::thread`/rayon/`pollster`/SAB in production
  code; every blocking entry is gated behind `#[cfg(not(target_arch="wasm32"))]`
  with an async twin (`boot_async`, `core.rs:80`). `REQUIRED_FEATURES =
  Features::empty()` (`core.rs:17`) boots on baseline/software adapters. The
  embedder owns the device via `WgpuDevice::with_external` (`adapter.rs:52`);
  netrender composes into supplied textures (no `wgpu::Surface` ownership) — the
  right shape for a host-owned canvas/OffscreenCanvas. (Confidence: high — but
  "structurally ready, never compiled for the web" is the accurate framing.)
- **GPU parallelism is the WebGPU compute pipeline itself** — vello runs the
  rasterizer as GPU compute (`use_cpu:false`, `vello_tile_rasterizer.rs:137-146`),
  parallelizing rasterization across the GPU *regardless* of the single-threaded
  CPU layout. (Confidence: high on native; web-backend status is the upstream
  unknown below.)
- **Two concrete browser blockers in netrender's own code:**
  1. **`max_inter_stage_shader_variables: 28`** requested in `boot_async`
     (`core.rs:103`). WebGPU's guaranteed baseline is **16** (confirmed:
     `wgpu::Limits::default()` == 16). This **fails `request_device`** on a
     baseline WebGPU adapter. The "production `with_external` bypasses
     `boot_async`" comfort is **false** if the 28 is *vello's* requirement: then
     the embedder's externally-supplied device must *also* grant 28, and a
     baseline WebGPU browser won't. **The real question is vello 0.9's required
     limits on WebGPU, not netrender's boot path.** Must probe per browser/GPU.
     (Confidence: high that it's a real blocker; the vello-requirement question
     is the open part.)
  2. **No `webgpu` (or `webgl`) wgpu feature in any manifest** (today:
     `dx12/metal/vulkan/wgsl` across `netrender` + `netrender_device`). A browser
     build must add them. (Confidence: high — verified absent in source and
     manifests.)
- **The renderer gate is upstream, not netrender.** vello 0.9 is compute-centric
  → works on WebGPU (Chrome/Edge/Safari-26) but **not WebGL2**. The WebGL2 tier
  needs **Vello Hybrid / sparse-strips** (fragment-shader rasterizer, ~beta,
  version-coupled to wgpu), not yet wired. Plan for **capability-based renderer
  routing**: WebGPU-with-compute (vello 0.9) → WebGL2 (Vello Hybrid).
- **Version coupling is four-way, not three.** Three netrender crates pin wgpu
  independently **and** vello pins its own wgpu transitively. netrender pins
  **wgpu 29 + vello 0.9** locally; vello's public docs reference wgpu 28 — these
  can both be true (newer local track) but **do not assert "wgpu 29 + vello 0.9 is
  a clean WebGPU pair" without verifying it**. (Confidence: high on the pins;
  medium on the pairing's web-readiness — upstream unverified, see §6.)

### pelt (desktop port)

- Keeps **native Rayon and native wgpu** unchanged — no browser tax.
  `pelt-desktop` is the winit/native port. The browser host is a *different,
  unbuilt port* consuming the wasm `genet-layout` lib. **pelt is where parallel
  Stylo cascade gets proven first** (native, ThreadSanitizer-verifiable) before
  any wasm thread pool. (Confidence: high. Correction: winit is *not*
  native-only — it has a working web backend; pelt uses the native one by choice,
  and the browser host is a separate port, not a winit limitation. See meerkat.)

### meerkat / mere — host concurrency, off-main-thread, the worker/actor boundary

- The **single-threaded-kernel + `Send`-message-actor shape is the idiomatic
  web-native concurrency model.** Content actor → Web Worker; kernel-on-main /
  sole-GPU-owner → browser main thread + WebGPU; the `EventLoopProxy` wake →
  worker `postMessage`. (Confidence: high architecturally.)
- **Off-main-thread is ~90% there *on native only*.** On web the move is real but
  **not header-free in cost**: Web Workers have **separate linear memory** (absent
  SAB), so a worker content actor must **serialize the DOM + sheets in and the
  Scene out via `postMessage`**. The outgoing scene side now has a postcard byte
  envelope in `meerkat/src/content/transfer.rs`: `ContentUpdate::Scene` is wrapped
  through a wire DTO, and Scene font/image bytes are deduped by id across frames.
  The remaining Web-Worker actor backend still has to wire actual
  `postMessage(buffer, [buffer])` delivery and quantify browser boundary cost.
  Native mpsc moves a pointer; Worker transfer moves a copied `ArrayBuffer`.
  **Quantify or flag this — it is a real, unquantified per-frame web tax the
  "header-free / 90% there" framing hides.** (Confidence: high that the boundary
  cost is real; the magnitude is unmeasured.)
- **Two distinct browser-parallel levers — do not conflate:**
  - **Inter-actor / inter-page** (Web Worker actors via `postMessage`): N pages
    render concurrently, each cold cost paid off-main-thread. **Header-free** (no
    SAB), works everywhere — *modulo* the serialization cost above. This is the
    realistic, broadly-available web win.
  - **Intra-layout** (Rayon-over-wasm-threads inside one worker): parallelizes a
    *single* page's cold cost. **Gated** on SAB + COOP/COEP + nightly build-std —
    and on the PWA-only fork (§5). Opt-in; no hot path may *require* it.
- **The thread substrate is the structural blocker.** `armillary::Pool` is
  `std::thread::spawn` + `Mutex/Condvar`; `spawn()/spawn_on()` use `std::thread`
  (`actor.rs:116,139`, `pool.rs`). On wasm there are no OS threads. There is an
  `Inline` no-threads physics backend (`orrery/physics.rs:11`, named as the
  no-threads wasm32 browser/PWA path) but **no `Inline` content-actor backend** —
  so in-browser the content cascade either runs on the main thread (reintroducing
  freeze) or needs a Web-Worker actor binding that does not yet exist.
  (Confidence: high.)
- **Per-target bindings already anticipated** (all behind the `Send`/message
  boundary — swaps, not rewrites): **Boa** (v0.21.1, ~94% Test262, no JIT)
  in-browser, **Nova** native (Nova's `Value` is usize-sized → hits the wasm 4 GB
  ceiling absent Memory64, which Safari lacks; engine is actor-local so swapping
  is not a boundary change); fjall → IndexedDB/OPFS (already cfg-anticipated in
  `eidetic-core`, `session-runtime`); on-disk JSON sessions → OPFS; HWND WebView
  scrying tile → cross-origin iframe; AccessKit → ARIA; custom Win32 titlebar →
  n/a. (Confidence: high — these are documented swaps.)
- **Incremental seams already exist host-side** to make the cold cost survivable:
  band-scroll (one vertical band per Scroll, `meerkat/src/content/mod.rs`), find-in-page
  offloaded to a worker (full layout ~1-2 s, can't run per keystroke,
  `main.rs:919-926`), per-tile texture cache keyed by scene generation
  (`main.rs:775`).
- **winit-on-web correction:** winit *has* a web backend (canvas + `requestAnimationFrame`).
  The browser-host gap is **not** "winit can't" — it is meerkat's host
  customizations: Win32 borderless/titlebar tricks + native AccessKit bridges
  that the web backend doesn't carry. The browser host is a minimal canvas + rAF +
  DOM-event port driving `boot_async` from `wasm-bindgen-futures` over an
  OffscreenCanvas — host plumbing, not a genet/netrender rewrite. (Confidence:
  high.)

---

## 4. Levers in priority order (cost vs payoff against the ~100 ms cold cost)

Payoff numbers borrowed from the ecosystem are tagged **[borrowed]** — none are
measured in genet/wasm.

**(a) Off-main-thread, single worker — DO FIRST**
- *Payoff:* eliminates UI freeze regardless of the 100 ms. The
  OffscreenCanvas-in-a-dedicated-Worker pattern is the converged ecosystem answer
  (Bevy, egui, Makepad, Ruffle all ship single-threaded-on-web + off-main-thread).
- *Cost:* **no SAB, no nightly, no COOP/COEP.** Architecture ~90% there on
  native; the web work is the Web-Worker actor substrate + OffscreenCanvas wiring
  **plus the DTO-`Serialize` pass and per-frame scene structured-clone** (§3
  meerkat). Highest leverage, lowest *toolchain* cost; the scene-copy cost is the
  one real unknown — measure it.

**(b) Incremental layout + dormancy/snapshot — MOSTLY THERE**
- *Payoff:* pay the cold cost *once*; per-frame work drops to repaint-only;
  suspended tabs cost nothing.
- *Cost:* low — genet has `IncrementalLayout` (repaint vs relayout, restyle
  damage); meerkat has band-scroll + per-tile generation cache. Mostly
  finishing/wiring.

**(c) SAB Worker-pool Rayon for parallel cascade/shaping — THE unexploited lever, HARD-GATED**
- *Payoff:* attacks the cold cost *itself*. **[borrowed]** 2–3.5x on the parallel
  fraction — **native Quantum-CSS provenance, never demonstrated under
  wasm-bindgen-rayon by anyone.** **Scoped by the §0 result (2026-06-21, synthetic +
  real): on a real page cascade is ~32% of cold paint (the synthetic's ~5% was an
  artifact of trivial CSS), so parallel cascade IS worthwhile. But shaping is larger
  (~48%) and already parallel on native, going serial on web without the SAB shim.
  So this lever has two real targets: keep shaping parallel on web (#1), and
  parallelize the still-serial cascade native-first (#2). Both attack first-order
  fractions.** The cascade drill-down (genet 95622930) confirms the cascade is
  well-targeted: traverse_dom (the Stylo-parallelizable slice) is ~80% of the ~32 ms
  cascade with only a ~6.5 ms serial floor, so parallel cascade is not Amdahl-trapped.
  Do not cite the magnitude as bankable (the Stylo-parallel speedup is unmeasured; the
  one parallel phase we measured, shaping, hit 1.32x).
- *Cost:* **highest, and forked.** COOP/COEP cross-origin isolation (a
  deployment/hosting constraint — see §5, **PWA-only**), nightly + `-Zbuild-std`,
  async pool init, main-thread-no-block, two artifacts. Plus the cascade
  `Cell`→atomic fix must land **native-verified first**. Can *regress* on
  heterogeneous mobile cores. **Native-first on pelt; treat browser as an opt-in,
  app-lane-only experiment behind feature detection + a homogeneous-core check.**

**(d) SIMD inner loops — CHEAP, but wrong axis for the headline**
- *Payoff:* **[borrowed]** 1.5–3x on raster/blend/compositing, text-shaping inner
  loops, UTF-8 (`simdutf8`), color/format conversion, memcpy/memset. **Does ~nothing
  for the cascade/box-layout cold cost.** Narrower than it looks: netrender's hot
  raster is **GPU/vello**; the CPU `tiny_skia` path is a *fallback*, so SIMD's
  raster payoff applies to a fallback path.
- *Cost:* near-zero toolchain — `simd128` is browser-baseline in 2026 (single
  `+simd128` build, no headers, no nightly). Use the **`wide` crate** (mind the
  narrower wasm backend; some ops scalarize). Relaxed SIMD: opportunistic /
  feature-gated only (still Safari-flagged).

**(e) wgpu → WebGPU — ALREADY PORTABLE (code shape), NOT BUILT**
- *Payoff:* GPU rasterize/composite off the layout thread.
- *Cost:* low in netrender's own code; the real work is the two manifest/limit
  fixes (§3) + capability-based renderer routing for WebGL2. **Does not relieve
  CPU layout cost** (spec forbids multithreaded single `GPUDevice`). (Render
  bundles are a *future, unbuilt* option for the repaint-replay path — netrender
  does not use them; do not count their payoff.)

---

## 5. Hard constraints + deployment reality

### The decisive constraint: the browser target forks in two

**SAB-threaded parallel cascade (lever c) is viable for the PWA / first-party app
lane only. The arbitrary-web-browsing lane cannot use it. This is a hard
architectural fork, not a tax.** (Confidence: high.)

Cross-origin isolation requires `Cross-Origin-Opener-Policy: same-origin` +
`Cross-Origin-Embedder-Policy: require-corp` (or `credentialless`). Without it,
**SharedArrayBuffer is silently not exposed** — the whole thread stack degrades to
no-threads with no error. For *a web engine that loads arbitrary third-party
content*:

- `require-corp` means **every** subresource (images, fonts, scripts, iframes from
  CDNs) must send `Cross-Origin-Resource-Policy` or valid CORS, or it **fails to
  load**. You cannot demand the whole web serve CORP headers.
- `credentialless` (Chromium) relaxes this for *no-credential* fetches but
  **strips cookies** (breaks authenticated images/CDNs), and **Safari does not
  support `credentialless`** (as of 2026) — so on Safari you're back to hard
  `require-corp`.

Conclusion: cross-origin isolation is **fundamentally incompatible with being a
general-purpose browser**. Under the no-Wasmtime policy, SAB is the *only* route
to in-browser CPU parallelism, so:

- **PWA / app lane:** *may* use SAB threads (lever c), behind feature detection.
- **Open-web-browsing lane:** *cannot*, full stop. It relies on levers (a)
  off-main-thread, (b) incremental, (d) SIMD, (e) WebGPU — none of which need SAB.

This also qualifies §1: SAB+Workers is "the in-browser CPU-parallelism path" for
the **app lane**, not the browser lane.

### Other constraints (with their cost)

- **Nightly + `-Zbuild-std`** — `wasm32-unknown-unknown` threading is still
  nightly-only in 2026 (`rust-src` + `-Zbuild-std=panic_abort,std` +
  `+atomics,+bulk-memory` + shared-memory link args). A **nightly-pinned,
  std-rebuilding build lane distinct from the stable desktop build** — real CI
  burden; conflicts with any "one simple stable wasm artifact" assumption.
  (Confidence: high.)
- **Async pool init** — `await initThreadPool(navigator.hardwareConcurrency)` once
  before any `par_iter`/`join`.
- **Main thread may never `Atomics.wait`** — the Rayon-driving wasm **must live in
  a dedicated Worker.** Threading and off-main-thread are the **same
  architectural move.** (Confidence: high.)
- **Two artifacts** — threaded + sequential fallback, branched via
  `wasm-feature-detect threads()`. Doubles web build/CI complexity. SAB memory
  also can't shrink (pins peak memory — risky on mobile background tabs).
- **wasm 4 GB linear-memory ceiling** — confirmed to bite Nova (hence Boa
  in-browser); Memory64 absent on Safari. Budget peak linear memory (SAB can't
  shrink). (Confidence: high.)
- **`wgpu` churn** — breaking majors ~quarterly (pinned at 29); vello version-couples
  (four-way lockstep, §3) — constrains when sparse-strips can be adopted.

---

## 6. Real unknowns / risks + concrete de-risking next steps

| Unknown / risk | Confidence | De-risking step |
|---|---|---|
| **Cold-cost phase breakdown** (cascade vs box-tree vs shaping) | **Measured 2026-06-21, synthetic + real (§0 result)** — real page: cascade ~32%, shaping ~48%, build ~9% of cold paint | **Done** (`examples/phase_timing.rs`). Both cascade and shaping are first-order; shaping is largest (already parallel native), cascade ~32% (still serial — lever (c) is worthwhile). Shaping parallel-vs-serial measured at 1.32x (~15 ms, web-without-SAB penalty), below the borrowed 2-3.5x. Cascade drilled: traverse_dom is ~80% of cascade (the Stylo-parallelizable slice), ~6.5 ms serial floor — lever (c) well-targeted, magnitude pending a Stylo-parallel measurement on Fedora. |
| **Stylo parallel cascade actually *running* in wasm** | Low — compiles (servo/stylo work landed) but **no one has tested it runs**; Blitz (closest sibling: stylo+taffy+parley+vello) ships Stylo **single-threaded on web** | (1) Land `Cell`→atomic, prove parallel cascade **native** on Fedora + ThreadSanitizer. (2) Only then a minimal `wasm-bindgen-rayon` harness driving `traverse_dom(Some(&pool))` in a Worker, and measure. Research-grade. |
| **Off-main-thread scene transfer on web** | Medium — Rust-side envelope landed; browser boundary unmeasured | `ContentUpdate` now has a postcard byte envelope, Scene asset bytes are deduped by id, and `wasm32` selects the transfer stream. Next: wire the actual Web Worker actor backend with `postMessage(buffer, [buffer])` and measure the browser boundary. |
| **`max_inter_stage_shader_variables: 28` vs baseline 16** | High it's a real blocker | Probe `request_device` on a baseline WebGPU adapter (Chrome/Firefox/Safari-26); **determine whether 28 is vello's requirement** (if so it travels through `with_external` too); add a downlevel path or gate the limit. |
| **vello 0.9 on a WebGPU browser backend with wgpu 29** | Medium — upstream **unverified** | Verify vello 0.9 compute over WebGPU in-browser before assuming the render half is free; scope Vello Hybrid/sparse-strips for the WebGL2 tier (accept beta + wgpu coupling). |
| **Clock panics** (`Instant`/`SystemTime`) in the host path | High | Route all host timing (animation, debounce, gyre tick) through `web_time`/`performance.now()`. Pervasive porting tax. |
| **Randomness** (`uuid` v4 / `getrandom`) | High | Land the de-random cuts (currently in the uncommitted worktree); `getrandom` `js` feature on wasm. |
| **fontique-in-wasm + fallback font** | Medium-high | Make `system` a per-target feature (manifest edit); confirm the blob path; **ship an embedded fallback font in the bundle** or text renders nothing; wire host font-feeding. |
| **winit-on-web host port** | Medium-low — winit *has* a web backend; the gap is meerkat's Win32/AccessKit host code | Stand up a minimal canvas + rAF + DOM-event host driving `boot_async` over an OffscreenCanvas. Host plumbing, not an engine rewrite. |
| **Parallel speedup materializing** | Medium — sublinear, plateaus ~4 threads, can regress on big.LITTLE | Benchmark per-target; gate browser parallel cascade behind homogeneous-core detection; never let a hot path *require* SAB. |
| **wasm build is worktree-only** | High | The green `wasm32` build lives in `wasm-fonts-cut`, not mainline, and depends on unlanded IPC/random/font cuts. Land them before treating "genet builds for wasm" as a repo fact. |

---

## 7. Net architectural message

- **Parallelism owns the cold-cost fix, in a strict order:** off-main-thread (a)
  → incremental (b) → SAB-Rayon parallel cascade (c). **SIMD (d) owns per-frame
  raster/shaping throughput** (wrong axis for cold layout). **WebGPU (e) owns GPU
  rasterize** (does not relieve CPU layout). They stack; each is expressed once via
  a portable abstraction (Rayon / `wide` / wgpu / the actor boundary).
- **Cheapest, highest-value first move:** the Web-Worker actor substrate
  (off-main-thread) — meerkat is ~90% there *on native*; the web delta is the
  OffscreenCanvas wiring + the DTO-`Serialize` pass + measuring the per-frame
  scene-copy cost.
- **The browser target forks.** SAB-threaded parallel cascade is a **PWA/app-lane**
  capability; the **open-web-browsing lane cannot use it** (COOP/COEP
  `require-corp` is incompatible with loading uncontrolled third-party content;
  Safari lacks `credentialless`). Prove the parallel cascade **native-first on
  pelt**; treat the browser case as an opt-in, app-lane experiment behind feature
  detection — never the baseline.
- **WASI is out** for the browser goal (it is by charter non-browser; its
  in-browser thread future is just polyfilled Workers; runtime is Wasmtime).
- **netrender is the part least in the way** — structurally ready, never compiled
  for the web; the work is two manifest/limit fixes plus capability-based renderer
  routing, with the render-half web-readiness gated on **upstream** vello-on-WebGPU,
  which we have **not** verified.
- **Three porting-tax classes sit alongside fonts** and are easy to miss: clock
  panics (`web_time`), randomness (`getrandom`/`uuid`), and the embedded-fallback-font
  requirement.

---

## 8. Confidence summary

- **High:** the three-lever landscape and WASI-out verdict; the portable-abstraction
  map *with* the Rayon three-state correction; every codebase-grounded per-layer
  fact (the cited `file:line` claims — cascade `None`, the `Cell`/`UnsafeCell`
  races, the unguarded shaping pre-pass, the `28` limit and absent `webgpu`
  feature, the `std::thread` actor substrate, the `Inline`-only physics backend,
  the missing `Serialize` on the Scene DTOs); the COOP/COEP / Safari-`credentialless`
  facts and the resulting PWA-vs-open-web fork; the clock/random/font porting taxes.
- **Medium:** the magnitude of any in-browser speedup; the per-frame
  scene-serialization cost; whether the `28` limit is vello's (and so survives
  `with_external`).
- **Low / unverified:** that Stylo's parallel cascade actually *runs* under
  wasm-bindgen-rayon (untested by anyone, Mere included; Blitz ships it
  single-threaded on web); that vello 0.9 runs on a WebGPU browser backend with
  wgpu 29 today (**upstream, not verified here**).
- **Borrowed, unmeasured in our stack:** the 2–3.5x parallel-cascade figure
  (native Quantum-CSS) and the 1.5–3x SIMD figure. Do not treat as bankable;
  the cold-cost phase breakdown (§0) must come first.

---

## Key grounding files

- `genet/components/genet-layout/`: `box_tree.rs:1076-1146`, `cascade.rs:696`,
  `style.rs:48-83`, `host_loader.rs:5-10`, `text_measure.rs:347`, `Cargo.toml:23`
- `genet/docs/`: `2026-06-06_wasm_enablement_and_crate_rename_plan.md`,
  `2026-06-13_parallel_cascade_scope.md`
- `netrender/netrender_device/src/core.rs:{17,80,103}`,
  `netrender/src/{vello_tile_rasterizer.rs:137-146, external_texture.rs}`,
  `netrender/netrender_device/src/adapter.rs:52`, `netrender/Cargo.toml`
  (+ `netrender_device/Cargo.toml`)
- `mere/crates/armillary/src/{actor.rs:96-141,116,139, lib.rs:5-32, pool.rs}`,
  `mere/crates/meerkat/src/content/{mod.rs,actor.rs,transfer.rs}`,
  `mere/crates/meerkat/src/main.rs:{775,919-926}`,
  `mere/crates/orrery/orrery/src/physics.rs:{11,25}`
- `mere/design_docs/mere_docs/implementation_strategy/2026-06-03_actor_constellation_plan.md` *(historical citation)* <!-- doc-audit: historical-path -->
