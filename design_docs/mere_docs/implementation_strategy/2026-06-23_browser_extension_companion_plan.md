# Browser Extension + Companion Node Plan

**Date**: 2026-06-23
**Status**: Planning (with Mark). Net-new delivery target; no code yet.
**Node + delivery framing superseded 2026-06-24** by
[orrery_browser_lane_plan](2026-06-24_orrery_browser_lane_plan.md) (capture-first,
favicon-body nodes not "DOM cards", gloss sidebar + orrery discrete tab, baseline
cross-browser, no-sync v1). The companion / smolweb / p2p / federation half below
stands as the forward arc beyond that v1.
**Scope**: Deliver Mere as a browser **extension / PWA** that reuses the portable
orrery core and leans on the host browser for what it already does (HTML layout,
text, tabs, navigation), while a paired **native companion node** carries what a
browser tab cannot (raw-socket fetch, the full engines, iroh p2p, heavy compute).
The product line is the part a browser does *not* do: p2p, smolweb, co-op
browsing, notetaking, and consented capture of the user's own browsing into a
portable, shareable orrery.
**Related**:

- [`../research/2026-06-19_cross_platform_parallelism_strategy.md`](../research/2026-06-19_cross_platform_parallelism_strategy.md) — the browser-delivery lanes (open-web no-SAB vs PWA SAB), Wasmtime-out / Boa-in-browser, "netrender is structurally ready, never compiled for web."
- [`../../2026-06-21_substrate_parallelism_composition_brief.md`](../../2026-06-21_substrate_parallelism_composition_brief.md) — the web build budget (3 builds / 2 toolchains) and the jco AOT path for components in the browser.
- [`2026-06-17_unified_document_host_plan.md`](2026-06-17_unified_document_host_plan.md) + [`2026-06-18_node_representation_arrangement_plan.md`](2026-06-18_node_representation_arrangement_plan.md) — the orrery-as-element work that built `render_as_cards`; this plan is its browser consumer.
- [`2026-06-23_gloss_outline_lens_plan.md`](2026-06-23_gloss_outline_lens_plan.md) — renames `mere-orrery` to `glossary`; this plan must not collide (the browser orrery core is the `orrery` crate, never `mere-orrery`).
- [`2026-06-21_document_script_substrate_plan.md`](../../archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md) + [`2026-06-23_document_script_followons_plan.md`](../../archive_docs/2026-07-03_completed_plans/2026-06-23_document_script_followons_plan.md) — the DocumentScript substrate, now **shipped native** (P0→P2.5 + all four follow-ons): the `mere:script` WIT world (`log` / `caps` / `net` / `document-host`), a **Wasmtime** host (the `document-host` crate — it deps wasmtime, so it is **native-only**, does not compile to wasm), `.cwasm` AOT, the fiber-async `net.fetch`, and the `kernel::permissions` → `Grant` model. Untrusted in-browser extension scripting rides the **same WIT contract** via **jco AOT** (no JIT in the tab): one shared *contract*, a per-target *runtime*. See §2 (scripting) for the harmonization. The contract lives at the shared, runtime-neutral `crates/script/wit/world.wit` (extracted out of the native crate 2026-06-23), so the browser jco path consumes the same world.
- [`../research/2026-05-31_murm_p2p_landscape_brief.md`](../research/2026-05-31_murm_p2p_landscape_brief.md) — iroh + noq is native QUIC; the browser tab needs WebTransport or WebRTC, which motivates the companion split.
- [`../research/2026-06-15_in_the_wings_and_browser_bar_audit.md`](../research/2026-06-15_in_the_wings_and_browser_bar_audit.md) — the dormant eidetic `BrowsingMemory` / `BrowsingTrace` (federation-faithful, zero live callers) this plan activates.
- [`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md) — the contributor-capability tier (browser tab = light surface, native helper = heavy lifting) the companion split reuses.

---

## 0. Thesis

In a browser you stop shipping *a browser*. The host already renders HTML, lays
out text, manages tabs and windows, and exposes navigation history. So the entire
"be a web engine" stack drops, and what is left is the part that was always the
product: the orrery, notetaking, smolweb, p2p, and consented capture. The browser
is not a constraint to fight; it is free infrastructure to mount on.

Two render targets, composited by the browser's own compositor:

- **DOM** renders node cards (the existing `render_as_cards` mode) and smolweb
  pages (nematic to HTML). This is what the browser already does well.
- **WebGPU (Canvas2D fallback)** renders only the graph underlay: edges, fields,
  physics, demoted off-screen dots. The one surface the browser does not give you.

The trade taken knowingly: the native pelt/serval host composites everything in
one wgpu scene; the browser stacks DOM cards over a canvas underlay. We give up
some pixel control and take on DOM-over-canvas z-ordering, in exchange for free
layout, text shaping, and accessibility. For the extension/PWA target that is the
right trade.

---

## 1. The companion split, and the two relay roles

The native companion is the same "old laptop running a mooting server" native
helper the resource-coordination brief already defines. The browser surface is a
thin client of it. The companion has two distinct doors, and only one is a relay:

- **Co-located (localhost), not a relay.** The extension talks to *your* node over
  native-messaging or a localhost WebSocket; that node simply *is* your p2p
  presence and does iroh on your behalf. A local daemon, not a relay.
- **Remote / pure-PWA (WebTransport ingress), a permissioned relay.** A browser tab
  cannot speak QUIC, so a remote tab reaches the iroh mesh through a server endpoint
  that bridges WebTransport to QUIC. That endpoint is a relay in the precise sense,
  but a **permissioned** one: only your authenticated personas/devices, capability
  scoped. This differs from iroh's holepunch relays, which are dumb and trustless
  (they forward encrypted packets and never see content). Your node-as-ingress sees
  your own browser-origin traffic in plaintext (fine, it is yours) and stays E2E for
  every other peer. The same companion serves both doors.

Fetch is the other reason the split is load-bearing: the browser cannot open a
gemini socket (no raw TCP; gemini's TLS-on-1965 with TOFU certs is unreachable from
a tab), so smolweb and other raw-socket fetches happen on the companion (reusing
`errand`), and the browser renders the result. Render in the tab, fetch outside it.

**This "fetch outside the tab" is exactly the shipped `net.fetch` capability** (the
DocumentScript substrate's follow-on #4b): a sync-signature `fetch` whose *backend* is
host-supplied. Native, the content actor backs it with `netfetcher`/`errand`; in the
browser, the `net.fetch` backend routes raw-socket schemes (gemini/gopher/…) to the
**companion** and plain http(s) to the browser's own `fetch()`. So the companion split
is the browser's *implementation* of `net.fetch`, not a separate mechanism — same WIT
seam, per-target backend, gated by the `net` grant (default-denied, §6 consent).

---

## 2. Three buckets (the crate map)

**Drop (the host browser already does it):**

- `serval` (the HTML engine: Stylo cascade, box-tree, taffy layout). The big weight.
- `inker` (engine controller), the scry/graft/weld multiplexer, `verso-tile` /
  `platen` / `pelt` insofar as they tile or compose *web* surfaces. Browser tabs and
  windows replace pelt's tiling.
- `meerkat`'s desktop window/event-loop scaffolding.

**Adapt (keep the capability, swap the substrate):**

- Storage: `eidetic`'s logic is portable; its fjall/redb backend becomes IndexedDB /
  OPFS in the browser (the kernel's `store` feature is already off-by-default for
  exactly this).
- Compute: Burn-wgpu already reaches the GPU via WebGPU. Keep.
- The graph underlay: netrender to WebGPU (Canvas2D fallback for v0).
- Scripting: Boa in-browser (Nova native), declarative policy data, and DocumentScript
  components via **jco AOT** from the shipped `mere:script` WIT. Wasmtime stays native-only
  (the `document-host` crate; the no-JIT rule). **Harmonization with the shipped substrate** —
  one *contract*, two *runtimes*:
  - **Native** = Wasmtime (`document-host`), `.cwasm` AOT (follow-on #4a). **Browser** = jco,
    its own AOT — symmetric, both no-JIT.
  - The `document-host` *interface* (`inspect`/`apply`) is render-engine-neutral. The native
    impl (`dom_view`) is over serval's `ScriptedDom`; the **browser impl is over the browser
    DOM** (serval is dropped here). The native meerkat content-actor wiring (P2.5c mirror +
    `ScriptInstance`) is native-only — the browser is a separate jco integration over the same
    contract, not a reuse of that wiring.
  - `net.fetch` (#4b) takes a browser backend (companion raw-socket / browser `fetch()`, §1).
  - The `kernel::permissions` → `Grant` model + the per-capability / per-origin stores
    (`script_permissions` in settings, `script-bindings.json`) **carry over** to the
    extension's consent + per-site model (§6), persisted to IndexedDB instead of disk. The
    `net` capability stays default-denied; consent grants it per origin.

**Keep (the portable core, code-verified wasm-clean):**

- `kernel` ([graph-kernel/Cargo.toml](../../../crates/graph/graph-kernel/Cargo.toml)) describes itself "Portable identity, authority, and mutation kernel," keeps native persistence behind an off-by-default `store` feature, and already splits UUID by target (v5/SHA-1 every target; v4/RNG gated `cfg(not(wasm32))`).
- `gyre` (rapier2d, pure-Rust physics, runs in wasm), `aether` (field math), `arrangements` (deterministic layouts, pure serde), `cartography` (contracts). All plain compute.
- `nematic` (smolweb): the host browser cannot speak gemini/gopher/spartan, so this is literally what the browser does not do. Render target is HTML.
- The event-DAG grammar plus tessera/reciprocity/moot logic: pure data model and projections; the portability and sharing substrate.

---

## 3. The orrery seam is already cut

The host-DOM-card fork is not new work to invent. `crates/orrery/orrery/src/lib.rs`
already carries `render_as_cards` (field at :228, setter at :954) with the full set
of per-node accessors a host needs to draw cards: `node_color`, `node_state_color`,
`node_selected`, `node_shape`, `node_position`, `node_representation`. Its own doc:
"the host renders those nodes as DOM cards in the shell document instead ... edges +
demoted dots stay as the underlay." A browser host is a new consumer of that mode,
where "the shell document" becomes the browser DOM.

The crate is also already wasm-aware: the native present stack (winit + wgpu +
serval-winit-host) is gated `cfg(not(target_arch = "wasm32"))`, the in-thread physics
backend is called out as "the future no-threads wasm profile," and "the wasm present
path (canvas + WebGPU)" is noted as a planned step (P2, 2026-06-06).

The one non-gated friction: the serval-backed gnode pool (`build_pool_dom`,
`node_dom`, `node_layout` over `serval_scripted_dom` / `serval_layout`) is a
hard dependency. `render_as_cards` skips it at *runtime*, but it still *compiles* in.
The first refactor feature-gates that pool so a cards-only wasm build does not pull
serval-layout.

---

## 4. Phases (done-conditions, not dates)

Inner-out; the first four phases ship the capture / visualize / portable story with
zero networking.

**P0 — Confirm the seam, build the core to wasm.** Feature-gate the gnode pool
(`build_pool_dom` / `node_dom` / `node_layout`) behind a `gnode-pool` feature (on for
native). A `dom-cards` profile forces `render_as_cards` and gates the pool out.
*Done when* `cargo build --target wasm32-unknown-unknown -p orrery --no-default-features --features dom-cards`
is green and the dep tree shows no serval-layout / inker / platen.

**P1 — Orrery in a tab, DOM cards, no network.** A thin wasm shell crate presents the
netrender underlay to a WebGPU canvas (Canvas2D fallback) and draws node cards as DOM
positioned from `node_position` + camera, wiring pointer/wheel to the orrery's existing
semantic input methods. *Done when* a seeded Graph renders in a browser tab, cards are
real DOM, and pan/zoom/select/drag work.

**P2 — Consented capture into the federatable trace.** Activate the dormant eidetic
`BrowsingMemory` / `BrowsingTrace` ([browsing/mod.rs:136](../../../crates/eidetic/eidetic-core/src/browsing/mod.rs)) as the
sink (`record_traversal`, with `adopt` for ingest), over a wasm subset on IndexedDB /
OPFS. MV3 extension: a consent surface, then `history` / `tabs` / `webNavigation`
capture into the trace, projected to a `kernel::graph::Graph` and into the orrery.
*Done when* real consented browsing appears as an arrangeable orrery, recorded in the
federatable `BrowsingTrace` rather than the session-local `SharedNavigationMemory`.

**P3 — Smolweb in the tab.** nematic renders gemtext / gopher to HTML; the tab displays
it; fetch comes from the companion or an HTTP-to-smolweb gateway. *Done when* opening a
gemini node shows its content as native DOM, with no pelt and no wgpu page surface.

**P4 — Portable export.** Export a captured orrery + trace as an engram
(event-DAG / CBOR / BLAKE3), re-importable elsewhere. *Done when* a session round-trips
into another instance with identity preserved.

**P5 — Companion bridge (localhost).** The native node exposes a local API
(native-messaging or localhost WebSocket); capability/persona pairing; captures and
engrams flow to full eidetic; the companion serves enriched content back (reader text,
thumbnails, smolweb fetch, full serval HTML on demand). *Done when* a paired companion
ingests captures (`adopt`) into full eidetic and serves back requested content.

**P6 — p2p via the companion.** The companion runs iroh (already built): replication,
moots, pinning, mesh. The "share" button is backed by a real RBSR round reporting its
count (the no-placebo rule). *Done when* a shared trace reaches another user through the
companion.

**P7 — Companion-less browser p2p.** The node exposes a permissioned, capability-gated
WebTransport ingress so a remote or pure-PWA tab reaches the iroh mesh at the browser's
reduced capability tier (T0/T1 per the resource-coordination brief). *Done when* a PWA
with no localhost companion participates in p2p through a permissioned ingress.

---

## 5. New crate boundaries (proposed, open)

Respecting the 600-LOC file ceiling and the no-second-monorepo rule:

- A wasm shell crate (working name `orrery-web`) holding the wasm orrery present
  (WebGPU/Canvas underlay + DOM-card glue + the JS event bridge). Native-free.
- The extension package (MV3 manifest + JS glue + a `wasm-bindgen` capture lib);
  capture lib is the only Rust there.
- A companion local-API surface, likely a module on the native host rather than a new
  crate at first (localhost server + pairing + the request/serve protocol).

`mere-orrery` is **not** reused: it is the consumer-less a11y `project_graph` facade
being renamed to `glossary` (gloss-outline plan). The browser orrery core is the
`orrery` crate plus the wasm-clean substrate (kernel / gyre / aether / arrangements /
cartography).

---

## 6. Configurability (settings, not hardcodes)

- Capture scope and redaction: which APIs, which origins, retention, per-site opt-out.
- Underlay backend: WebGPU vs Canvas2D.
- Storage backend: IndexedDB vs OPFS.
- Ingress mode: localhost companion / WebTransport ingress / community relay / none.
- Which content formats render as DOM vs (rare) canvas.

---

## Findings

- **The host-DOM-card seam already exists.** `render_as_cards` + the per-node accessors
  (`crates/orrery/orrery/src/lib.rs:228, :954, :903-952`) are the orrery-as-element work;
  the browser is a new consumer, not a new fork.
- **The portable core is wasm-clean by design.** `kernel` is self-described "Portable"
  with `store` off-by-default and per-target UUID handling; gyre/aether/arrangements/
  cartography are plain compute. The friction is entirely the render/composition path
  (serval-layout, inker, platen), which the browser replaces with DOM.
- **The capture substrate is built and dormant.** eidetic `BrowsingMemory` /
  `BrowsingTrace` (record_traversal / adopt / recent_corridor / co_occurrence) is the
  federation-faithful half the in-the-wings audit flags with zero live callers. The
  extension is its natural driver, and its requirements (portable, shareable browsing)
  select it over the session-local `SharedNavigationMemory` the native path ships.
- **Smolweb renders in the tab.** nematic to HTML to browser DOM is the lightest path;
  pelt-in-a-popup solved a problem the browser already solves. Fetch stays on the
  companion (raw TCP/TLS), render in the tab.
- **The node is a relay only in the WebTransport-ingress role**, and a permissioned one;
  the localhost role is a local daemon. Distinct from iroh's dumb holepunch relays.
- **`mere-orrery` is being renamed to `glossary`** (gloss-outline plan, settled
  2026-06-23). This plan deliberately targets the `orrery` crate instead and must not
  collide with that rename.
- **The DocumentScript substrate shipped native (2026-06-23) and harmonizes cleanly** —
  no conflict, three concrete connections rather than one hand-wave. (1) `document-host`
  is **native-only** (deps wasmtime 45), so the browser genuinely needs the **jco** path,
  not the crate: one `mere:script` WIT *contract*, two *runtimes* (Wasmtime `.cwasm` AOT /
  browser jco), both no-JIT. (2) The companion's "fetch outside the tab" **is** the shipped
  `net.fetch` capability with a per-target backend (companion raw-socket / browser `fetch()`),
  not a separate mechanism. (3) The `kernel::permissions` → `Grant` model + the
  `script_permissions` / `script-bindings.json` stores carry straight into the extension's
  consent / per-site model (IndexedDB-backed); `net` stays default-denied. The render-neutral
  `document-host` interface maps to the browser DOM (the serval-coupled `dom_view` impl is
  native-only).

## Open risks

- **netrender on wasm** is "structurally ready, never compiled for web." P0/P1 flush it
  out; Canvas2D is the underlay fallback (the underlay is only lines + dots).
- **MV3 service-worker ephemerality** fights persistent capture and connections; use
  offscreen documents, and keep persistent p2p on the companion, not the worker.
- **WebGPU availability in extension contexts** needs confirming per target browser.
- **iroh's actual WebTransport / relay surface** must be verified before P7 (the
  landscape brief documents native QUIC only).
- **BrowsingTrace wasm-portability**: the type is portable, but its storage and any
  segment machinery need a wasm subset over IndexedDB/OPFS.
- **The `mere:script` WIT is now a shared contract** (extracted 2026-06-23): moved to
  `crates/script/wit/world.wit` (a runtime-neutral dir, not inside the native `document-host`
  crate); the native host (`bindgen!` `path: "../wit"`) and both guests (`generate!`
  `path: "../../wit"`) consume it from there, so the browser **jco** path points at the same
  dir. **Residual watch**, not a blocker: keep the native host and the jco bindings on the same
  world (no browser-only fork — layer any browser divergence, e.g. a `net.fetch` backend contract,
  without editing the shared world), or the contract silently splits. The stale probe copy at
  `crates/probes/document-script-p0/wit/` is gitignored and not load-bearing.

## Progress

### 2026-06-23

- Plan drafted with Mark from the extension/companion design conversation. Decisions
  banked: host-DOM cards (Option B), the companion split plus the permissioned
  WebTransport-ingress fallback, the drop/adapt/keep crate buckets. Code-grounded
  against the orrery lib (`render_as_cards` + accessors), the kernel manifest (portable
  by design), gyre/arrangements/platen/aether manifests (wasm-clean core; platen is the
  heavy composition path that drops), and the eidetic browsing module (BrowsingMemory /
  BrowsingTrace API confirmed). No code. DOC_README index entry added the same session.
- **Harmonized with the now-shipped DocumentScript substrate** (P0→P2.5 + all four follow-ons,
  same day). Verified against code: `document-host` deps wasmtime 45 (native-only, so the browser
  needs jco, not the crate); the `mere:script` WIT (`log`/`caps`/`net`/`document-host`) lives in
  `document-host/wit/world.wit` (not yet shared). Sharpened the substrate cross-reference (§Related)
  and §2 scripting (one contract / two runtimes; the render-neutral `document-host` interface maps to
  the browser DOM; the permission model carries over to consent); connected the §1 companion/fetch
  split to the shipped `net.fetch` capability (the companion is the browser's `net.fetch` *backend*,
  not a separate mechanism); added a Findings note + an Open risk (the WIT needs a shared,
  runtime-neutral home before the browser jco path, to avoid contract drift). No conflicts found;
  the harmonization is additive. No code.
- **Extracted the shared WIT (acting on the harmonization's action item).** Moved
  `mere:script` (`world.wit`) out of the native crate to `crates/script/wit/world.wit` — a
  runtime-neutral home. Updated the native host `bindgen!` (`path: "../wit"`) and both guests'
  `generate!` (`path: "../../wit"`); regenerated the guests; `cargo test -p document-host` green
  (19 tests). The browser jco path now has a shared contract dir to point at. The Open risk drops
  from "not yet shared" to a residual "keep native + jco on one world (no browser fork)".
