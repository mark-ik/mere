# Engine picker + pluggability — implementation plan

**Date**: 2026-06-15
**Status**: Plan / pre-implementation. Phase 0 is the load-bearing reconcile;
everything else hangs off it. The live scrying path (multi-tile, shipped
2026-06-15) is the ad-hoc precursor Phase 0 folds into routing.
**Scope**: The user-facing engine **picker** (an inker affordance), the
**pluggability / extension model** that makes engines build-, session-, and
activation-managable, the no-handler fallback, local-file ingestion, and the
two content tiers. Stops where **verso** begins: carrying live state across a
flip is the [verso charter](../../verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md),
sequenced here as the final phase.

**Related**:

- [verso compatibility-view charter](../../verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md) — ownership split (picker = inker, flip = verso), one-hop invariant, sequencing gate. This plan is the picker half of that charter's step 2.
- [engine profile boundary plan](../../mere_docs/implementation_strategy/2026-05-14_engine_profile_boundary_plan.md) — `EngineProfileBinding` (Persona/Session/Graph scoping). The activation model here mirrors that tiering.
- [browser multiplexer framing](../../mere_docs/research/2026-05-11_browser_multiplexer_framing.md) §5.4, §7, §8 — engines as replaceable producers; "engine route override" capability; `engine.route_chosen` / `engine.route_degraded` diagnostics.
- [modular integration plan](../../mere_docs/implementation_strategy/2026-06-02_modular_integration_plan.md) §1.9, §6 — the `register-viewer` vs `inker::routing` dual-routing reconcile, gated on "meerkat first routes >1 content engine."
- [engine peers + scrying library brief](../../mere_docs/research/2026-05-11_engine_peers_and_scrying_library_brief.md) — `scrying.web` / `wry.web` as opt-in engines.

---

## 1. Goal + the ownership split

**Goal:** a session where the user can see which engine renders a given piece
of content, switch it (per node and per host), and manage which engines exist /
are active / are off, with graceful behaviour when nothing handles a scheme.

Three owners, held to the verso charter:

- **Picking** an engine = `inker` routing ([routing.rs](../../../crates/inker/src/routing.rs)). The picker UI writes pins / overrides into that policy.
- **Texture plumbing** = scry / graft / weld + netrender compose + constellation actors. Untouched by this plan.
- **The flip** (live state carried across an engine swap) = `verso`. Phase 5 only; minted at the first serval→scrying flip per the charter.

In-product vocabulary stays plain: "compatibility view", "flip", "open in
\<engine\>". `verso` / `inker` are crate names, not UI words.

## 2. Findings — what already exists

The routing backend is substantially built; meerkat just does not consume it.

**Routing (live, canonical).** [`EngineRoutePolicy::route_filtered`](../../../crates/inker/src/routing.rs) decides by precedence **pinned_engine → content-type → per-host override → scheme → fallback**, and `route_filtered(is_available)` skips any engine the host lacks, walking to the next rule. `EngineRouteRequest` already carries `pinned_engine: Option<String>` and the policy carries `per_host_overrides: HashMap<host, engine_id>`. Unhandled schemes fall to `ENGINE_EXTERNAL_PROTOCOL` (`host.external-protocol`, Headless). `scrying.web` / `wry.web` are defined but deliberately out of the default policy (opt-in via pin / override).

**Two registries = two tiers (the key structural fact).** The tier split the user described is already the two registries, and it is the same axis as the charter's glass-box / black-box fidelity axis and the wasm/native build axis:

| Tier | Seam | Output | Fidelity | Build | Engines |
|------|------|--------|----------|-------|---------|
| 1 — web platform | [`Engine`](../../../crates/inker/src/engine.rs) → `EngineRegistry` | portable `EngineDocument` | glass-box (full export → good verso donor) | wasm32-safe, always in | `nematic.*`, `serval.web` |
| 2 — native | [`SurfaceEngine`](../../../crates/inker/src/surface_engine.rs) → `SurfaceEngineRegistry` | GPU `SurfaceFrame` texture | black-box (inject most, extract little) | native only, vendored | `scrying.web` (live), `weld`, `graft` |

The scry tier-2 engine is real: [`ScryingTileEngine`](../../../crates/inker/engines/scrying-engine/src/engine.rs) implements `SurfaceEngine` (id `scrying.web`) over a host-supplied `ProducerFactory`.

**Profile scoping.** `EngineProfileBinding` (Persona / Session / Graph) and the pure `engine_profile_path` resolver already exist in `session-runtime`; this plan reuses that tiering shape for *activation* scope.

**The gap.** meerkat does **not** route through `EngineRoutePolicy` at all. It registers `nematic::engines()` only for snapshot cards, drives live content through the constellation, and runs scry through an ad-hoc `compat_pins: HashSet<GraphMemberId>` bool (the path the 2026-06-15 multi-tile work extended). The [modular integration plan §6](../../mere_docs/implementation_strategy/2026-06-02_modular_integration_plan.md) names the same gap: `register-viewer` (mime→viewer) duplicates `inker::routing`; reconcile when meerkat first routes >1 content engine. That moment is Phase 0.

## 3. Architecture decisions

1. **Three activation levels collapse to one predicate.** `is_available(id) = registry.contains(id) && enabled(id)`.
   - *Not in build* → not registered (cargo feature off / not vendored).
   - *In build, deactivated* → registered but `enabled(id) == false`: routing never picks it, no actor / producer spawns, the binary still carries it.
   - *Active* → registered + enabled.
   The routing already takes the `is_available` closure; only the enable set is new.
2. **Activation scope = global default + per-session override** (user decision, 2026-06-15). A global `EngineEnableSet` (app setting) is the default; a session manifest field overrides per engine id. Mirrors `EngineProfileBinding`'s persona/session tiering. Graph scope is reserved, not built.
3. **Build tier via cargo features.** Tier 1 (`Engine` document engines) is the always-present portable baseline (ships to wasm/PWA). Tier 2 (`SurfaceEngine`) crates are feature-gated and vendored per platform; absent on wasm, where the no-handler fallback covers the hole. One feature per surface-engine crate (`scrying`, `weld`, `graft`).
4. **No-handler is already correct, just invisible.** Keep `host.external-protocol` as the fallback; make it *legible*: emit `engine.route_degraded`, and have the picker offer "open externally" or "enable an engine that handles \<scheme\>".
5. **Local files are not special.** A `file://` address feeds the same routing; the only twist is no server MIME, so a content-sniffer (extension + magic bytes) populates `EngineRouteRequest.content_type`, and "serval renders anything web-standard" becomes the http rule applied to local bytes. nematic lanes and markdown re-route by sniffed type exactly as network content does.
6. **Engines are the first instance of a general extension mechanism.** The GUI itself (serval rendering host chrome) is the bootstrap engine. The enable/registry/build model here is the extension model; later extension classes (protocols, scripting) reuse `is_available` + the build/session/active levels rather than inventing their own.

## 4. Phases (sequence)

Done-conditions, not dates (per house rule).

**Phase 0 — routing reconcile + meerkat onto `EngineRoutePolicy`** *(foundation; unblocks all)*

Routing in meerkat is genuinely **two-altitude**, and the policy was already
designed for exactly that (its comment: "the same address can re-route after the
host learns the MIME type from a response"). So Phase 0 splits:

- **0a — UI-thread tier decision (DONE, d4a1350).** At nav time the host knows the
  url (scheme) and the node's pin but not yet the content-type. `compat_pins:
  HashSet<member>` became `engine_pins: HashMap<member, engine_id>`; the host holds
  a `route_policy`; each node routes via `route_filtered(request{pinned_engine},
  is_available)`, and `is_surface_engine(decision.engine_id)` picks the scrying lane
  vs the constellation. The compatibility view is a pin to `scrying.web`. Added
  `inker::routing::is_surface_engine` as the canonical tier-2 classifier.
  *Verified:* a `>compat_view` node renders through WebView2 via the route (producer
  spawns, no `EngineNotFound`); normal http/gemini unchanged.
- **0b — actor content-type pass (NEXT).** The off-thread actor's
  `engine_id_for` + `is_html` in [card.rs](../../../crates/meerkat/src/card.rs) is the
  second altitude (post-fetch, content-type known) and the `register-viewer`-shaped
  duplicate. Fold it into the same `route_policy` (the content-type rules already
  exist there), dispatching the decision to the document `EngineRegistry` (tier 1)
  or the serval html lane. Harvest `register-viewer`'s capability/conformance
  declarations into `inker::routing`; retire the duplicate (integration plan §1.9 / §6).
- *Phase 0 done when:* both altitudes consult one `route_policy`, and the only
  bespoke per-content `match` left is gone.

**Phase 1 — activation model**
- `EngineEnableSet` (global default, app setting) + a per-engine session-manifest override field; `is_available = contains && enabled`.
- Deactivating a live engine reaps its actors/producers and re-routes its nodes (which then fall through to the next available rule).
- *Done when:* turning `scrying.web` off in settings makes a scry-pinned node fall back to serval (or external), no producer spawns, and flipping it back restores it.

**Phase 2 — engine manager (apparatus pane)**
- A panel listing registered engines with build/active/off state, the `per_host_overrides` table, and the default-policy editor. Reuses the apparatus settings surface.
- *Done when:* a per-host override ("example.com → scrying.web") set in the panel takes effect on next navigation.

**Phase 3 — per-node picker**
- "Open in \<engine\>" on the tile / card, listing only `is_available` engines, writing `pinned_engine` on the node. The single most direct expression of the user's ask.
- *Done when:* right-clicking a node and choosing an engine re-renders it through that engine and persists the pin.

**Phase 4 — no-handler UX + local files**
- Surface `route_degraded` in the picker; "open externally" affordance. Content-sniffer for `file://` feeding `content_type`; serval as the web-standard default for local files.
- *Done when:* opening a local `.md` routes to nematic.markdown, a local `.html` to serval, and an unhandled scheme shows the explicit "no engine / open externally" state.

**Phase 5 — verso flip** *(charter step 3; separate doc owns detail)*
- Mint `verso` at the first serval→scrying flip: one carrier, flip choreography on one tile, state carried (URL, scroll, cookies, snapshot), tile identity + lineage intact, one-hop invariant enforced.
- This plan's Phases 0–4 are the prerequisite (engines must be user-visible and pinnable before a flip means anything). Detail lives in a verso_docs plan when minted.

**Build-tier track (parallel).** Feature-gate the tier-2 engine crates; confirm a tier-1-only build (no scry/weld/graft) compiles and runs, routing tier-2 schemes to the fallback. This is the wasm/PWA shape proven on the desktop build.

## 5. Background / dependencies

- Phase 0 is the gate the integration plan already named ("resolve when meerkat first routes >1 content engine"). It is also the honest home for the multi-tile scry path: that work is correct but lives outside routing, and Phase 0 is where it rejoins.
- The picker (Phases 2–3) depends only on Phase 0–1, not on verso. Verso (Phase 5) depends on the picker existing.
- weld (CEF) and graft (Servo) producers are tier-2 `SurfaceEngine` impls parallel to `ScryingTileEngine`; their build-out is tracked separately but slots into this model as additional registered ids with feature gates. They do not block the picker.
- Activation reaping must route through the capability-gated action bus when that lands (multiplexer §7: "engine route override" and "profile escalation" are gated actions); until then it is a direct host op with a diagnostic.

## Findings

- 2026-06-15: Routing precedence, `pinned_engine`, `per_host_overrides`, `route_filtered`, and the `host.external-protocol` fallback are all already implemented in [routing.rs](../../../crates/inker/src/routing.rs). The two registries ([engine.rs](../../../crates/inker/src/engine.rs), [surface_engine.rs](../../../crates/inker/src/surface_engine.rs)) cleanly encode the user's two-tier model, and that split coincides with the charter's glass/black-box fidelity axis and the wasm/native build axis. The single missing piece for a picker is consumption: meerkat routes nothing through the policy today.
- 2026-06-15: The verso charter (Mark, 2026-06-10) already assigns the picker to inker and reserves verso for the flip. The user's "verso = engine switcher" framing resolves to picker (inker) + flip (verso) composed.
- 2026-06-15: meerkat's content routing is **two-altitude**, and that is inherent, not accidental: at nav time the host has the url (scheme + pin) but not the content-type, which only the off-thread actor learns post-fetch. The policy is built for this (scheme/pin first pass, content-type second pass), so Phase 0 splits cleanly into 0a (UI-thread tier + pin) and 0b (actor content-type), both consulting one `route_policy`. The `is_available` closure must report *true* for the lanes meerkat handles without a document-registry entry (serval html, mere:// internal, external-protocol, linked-data) or an http node would wrongly fall through to the OS hand-off.

## Progress

- 2026-06-15: scry-in-tile (single focused tile) shipped + verified (meerkat 0adca6e); multi-tile scry shipped + verified (06b6ac7) — two independent WebView2 panes on one shared `CompositionRoot`, per-pane input. This advances the verso charter's P4 (scrying tile as a live, interactive actor) but through the ad-hoc `compat_pins` path; Phase 0 folds it into routing.
- 2026-06-15: Plan authored from a read of routing.rs, engine.rs, surface_engine.rs, scrying-engine, the verso charter, the engine-profile-boundary plan, the browser-multiplexer framing, and the modular-integration plan. Activation scope decided: global default + per-session override.
- 2026-06-15: **Phase 0a shipped + verified (meerkat d4a1350).** `compat_pins` retired into `engine_pins: HashMap<member, engine_id>` + a host `route_policy`; the UI-thread tier decision routes via `route_filtered(request{pinned_engine}, is_available)` → `is_surface_engine` picks the scrying lane. Added `inker::routing::is_surface_engine`. `>compat_view` is now a pin to `scrying.web`. Headed-verified: pinned node renders through WebView2 via the route (producer spawns, no `EngineNotFound`); http/gemini nodes render unchanged through the constellation. Phase 0b (fold card.rs `engine_id_for`/`is_html` into the policy + harvest register-viewer) is next.
