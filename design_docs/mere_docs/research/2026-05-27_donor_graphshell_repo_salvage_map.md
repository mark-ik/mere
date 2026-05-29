# Donor `repos/graphshell` Repo Salvage Map

**Date**: 2026-05-27
**Status**: Code-level salvage inventory of the archive-bound donor repo at `repos/graphshell`.
**Scope note**: this maps the **donor repo's crates and legacy monolith** (the pre-Mere prototype kept as historical reference). It is the code-side companion to three docs that already cover adjacent ground:

- the [graphshell harvest brief](2026-05-17_graphshell_harvest_brief.md) covers the donor's `design_docs/` (concepts from ~70 docs), not its code.
- the [supercrate salvage map](2026-05-22_graphshell_supercrate_salvage_map.md) covers the **in-workspace** `crates/graphshell/` supercrate, a different tree.
- the [external deps topology brief](../../2026-05-24_external_deps_topology_brief.md) flagged the donor repo as archive-bound but did not inventory its members.

This doc fills that gap: for each donor crate / cluster, is it **already salvaged**, **latent salvage**, or **cut / reference-only**.

---

## Headline

Most of the donor's load-bearing code is already in mere. The biggest-looking target, the `middlenet-*` protocol family, is fully absorbed by `inker/engines/nematic`; `graph-tree` became `forme`; `graph-canvas` and `register-diagnostics` already live in the workspace. What actually remains is a thin residual: two unbuilt protocols, a cluster of latent registries, a comms substrate that partly maps to `murm`, and a ~180 KLOC egui monolith that is throwaway as code (its *ideas* were harvested separately). The repo is genuinely archive-ready as a unit; the residual list below is what to pull before it goes cold.

**Repo state**: root `Cargo.toml` still names `graph-cartography` / `graphshell-core` / `graphshell-runtime` as deps, but those crate directories were already deleted (the [lineage/forme rename plan](../implementation_strategy/2026-05-17_lineage_forme_rename_plan.md) cascade-removed them). The repo does not build as-is and is not meant to. Classification below is by salvage value, not build state.

---

## Already salvaged — donor crate has a canonical home in mere

These were the real value. They are ported (clean reimplementations, not copies). Cut from any future build; keep the donor only as a reading reference for edge cases.

| Donor crate(s) | ~LOC | Canonical home in mere | Note |
|---|---|---|---|
| `middlenet-core` + `-adapters` + `-render` + 12 protocol crates | ~5.7k | `inker/engines/nematic` | nematic ships all 12 lanes (markdown, gemtext, gopher, feed, text, file, finger, knot, scroll, misfin, nex, guppy) as `inker::Engine` impls. Donor's `SemanticDocument` model is superseded by `inker::EngineDocument`. |
| `middlenet-netrender-bridge` | ~0.9k | `inker/document-canvas` (parley → netrender) | same role: semantic blocks → parley layout → netrender display list. See [component fit map](../technical_architecture/2026-05-26_component_fit_map.md). |
| `middlenet-engine` | ~2.4k | superseded by `inker` registry | facade aggregator; inker's `EngineRegistry` is the canonical aggregation point. |
| `graph-tree` | ~5.0k | `forme/forme` | renamed per the [lineage/forme rename plan](../implementation_strategy/2026-05-17_lineage_forme_rename_plan.md); "tree" was the output shape, forme is the arrangement authority. |
| `graph-canvas` | ~14k | `graphshell/graph/graph-canvas` (~9.6k) | the in-workspace crate is the salvaged descendant; covered by the [supercrate map](2026-05-22_graphshell_supercrate_salvage_map.md). |
| `register-diagnostics` | ~2.8k | `crates/graphshell/.../register-diagnostics` | keystone diagnostics registry; already extracted into the workspace (supercrate map, latent cluster). |

**Residual gaps inside this group — investigated 2026-05-27:**

- **JSON Feed 1.x → taken.** The donor `-feed` had a full JSON Feed parser; nematic's `feed.rs` covered only RSS + Atom. Ported the JSON path into the `FeedEngine` (flavour auto-detected by content type, body-sniff fallback), mapped onto the existing `Parsed`/`Entry` → `DocumentBlock` pipeline so all three flavours share one block builder. `serde`/`serde_json` added to nematic. 6 new tests, green.
- **Spartan and Titan protocols → not salvage.** The donor crates are 43-LOC error-returning stubs; there is no code to take. Implementing these lanes is net-new feature work against nematic's `inker::Engine` contract (Spartan fits cleanly as a gemtext-bodied fetch lane; Titan is a write/upload companion that does not fit a content-render engine without a request-submission seam). File a plan if/when those lanes are wanted; do not treat as extraction.

## Latent salvage — not yet rewired, real Xilem host will want it

The donor's `registrar/` is the *origin* of the workspace registry extractions. `register-diagnostics` already came across; the rest are candidates the host and the [renderer registry contract](2026-05-15_renderer_registry_contract_brief.md) will plausibly want. Keep the donor readable until those slices land; pull per-slice rather than wholesale.

| Donor crate | ~LOC | Registers | Revive for |
|---|---|---|---|
| `register-viewer` | ~760 | viewer-id → capability (MIME/ext routing, render mode, conformance) | the `NodeRenderer` registry's content-kind → renderer routing |
| `register-protocol` | ~220 | URI scheme → handler (late-bound, mod-extensible) | scheme dispatch in the host / inker routing |
| `register-theme` | ~1.2k | theme registry + edge-style vocabulary (stroke pattern, endpoint marker, a11y mode) | GraphCanvas edge styling + theming |
| `register-lens` | ~1.1k | projection presets (physics + layout + theme + filters as named lenses) | cartography/platen lens presets |
| `register-knowledge` | ~410 | UDC tag validation, hierarchical-distance scoring, fuzzy search | knowledge/tag classification (kernel `tags` adjacency) |
| `register-input` | ~1.4k | keyboard/mouse/pad bindings keyed by action-id | host keybinding registry |
| `register-layout` | ~860 | canvas/surface/profile primitives + conformance vocabulary | surface capability declarations |
| `register-mod-loader` | ~1.5k | native + WASM mod discovery / manifest / dependency resolution | the native/WASM mod lane (per browser taxonomy translation) |
| `graphshell-comms` | ~4.3k | Nostr + WebFinger + Misfin + TLS/cert substrate | partly maps to `murm` (transport is iroh-based today); Nostr/WebFinger pieces are a possible `murm`/`persona` pull. See [cable migration plan](../../murm_docs/implementation_strategy/2026-05-04_cable_migration_from_verso_plan.md). |

## Cut / reference-only

| Donor crate(s) / area | ~LOC | Why cut |
|---|---|---|
| `verso` + `verso-host` | ~1.8k | donor `verso` is engine-**routing** (middlenet/servo/wry decision tree); inker now owns engine routing, and mere's `verso-core`/`tile-state` are surface-lifecycle (different charter, see [verso adoption plan](../implementation_strategy/2026-05-27_verso_adoption_plan.md)). The route-decision-with-reasons pattern is reference, not a pull. |
| `iced-middlenet-viewer`, `iced-graph-canvas-viewer`, `iced-wry-viewer`, `graphshell-iced-widgets` | ~3.1k | iced host lane; mere is on Xilem. Reference-only patterns: the wry-overlay mount/unmount/sync state machine and the TileTabs/Modal interaction logic. |
| `vendor/iced`, `vendor/cryoglyph` | ~96k | vendored upstream forks; not mere salvage. |
| Legacy monolith: `shell/desktop/` (~125k), `render/` (~19k), `app/`, `domain/`, `model/`, `graph/`, `registries/`, `services/`, `input/`, root `graph_app.rs`/`prefs.rs` | ~180k total | egui host. The *concepts* (intent-as-sole-mutation, composition pass order, focus tracks, etc.) were already harvested at the doc level into mere-kernel and the [harvest brief](2026-05-17_graphshell_harvest_brief.md); the *code* is superseded by mere-kernel's mutation model. Reference-only. Thinnest concrete residual worth a look if a need arises: `services/import` (history/bookmark ingest) and the `GraphIntent`/`GraphMutation` vocabulary. |

---

## Taken into mere (2026-05-27)

Directive this pass: **don't wait for consumers** — get the salvageable code moving and visible now rather than gating on a slice that might never name it. Per the [consumer-pull check](../research/2026-05-18_node_identity_and_duplicates_brief.md) reasoning, "wait for a consumer" gates nothing in a single-consumer codebase like Mere. Each pull below compiles and tests green in isolation.

| Pulled | Landed at | Notes |
|---|---|---|
| **JSON Feed 1.x** | `inker/engines/nematic/src/feed.rs` | content-type/body-sniff dispatch onto the existing block builder; +6 tests |
| **Spartan + Titan engines** | `nematic/src/spartan.rs`, `titan.rs` | Spartan = gemtext/markdown body (like scroll); Titan = gemtext response body + a diagnostic that upload/request is the transport layer's job. Registered in `engines()`; full nematic suite 144 green. The donor crates were 43-LOC stubs, so these are fresh impls, deliberately landed (even Titan as a thin render-side) so the lane is visible when it matters. |
| **Browser import** | new crate `crates/import` | relocated `services/import/mod.rs` verbatim, visibility widened, crate-doc added; no graph dependency, pure ingest model + Chrome-JSON/Netscape-HTML parsers. 3 tests. |
| **register-input / -layout / -protocol** | `crates/graphshell/shell/system/registry/` | dependency-clean; relocated as-is beside the existing `register-diagnostics`. 33 tests (15/13/5). |
| **register-knowledge** | same registry dir | adapted: `graphshell_core::color::Color32` → `kernel::color::Color32` (kernel's own doc names this crate as an intended consumer); UDC seed asset copied. 10 tests. |
| **register-viewer** | same registry dir | `AddressKind`/`address_kind_from_url` → `kernel::address`; dropped the retired `VersoAddress` (`verso://settings`/`verso://frame`) internal-routing branch + its 3 tests — that's host/mere-domain chrome now, not the portable registry's job. 25 tests. |
| **register-mod-loader** | same registry dir | no graphshell-core coupling; its `register-diagnostics` event surface (`DiagnosticEvent`/`emit_event`/`install_global_sender` + `CHANNEL_MOD_*`) and `register-viewer` deps all resolved in mere as-is. 18 tests. |
| **register-lens** | same registry dir | made self-contained: the donor sourced its physics-preset value types from `graph_canvas::physics_config`, which mere's graph-canvas doesn't have (different `scene_physics` model), so lens now **owns** them in a local `physics_config` module (pure data); `overlay`/`color` → `kernel`. Dropped only the `apply_to_state`/`ForceDirectedState` runtime bridge (a consumer maps `GraphPhysicsTuning` onto the active runtime). 12 tests. |
| **register-theme** | same registry dir | `Color32` → `kernel::color`; its `register_lens` imports (`THEME_ID_*`, `ThemeData`) resolved against the just-landed lens. Edge-style vocabulary intact. 8 tests. |
| **misfin** (Misfin TLS mail client) | `crates/murm/misfin` | self-contained; dropped the one `body_document()` convenience that pulled the donor render model (rendering is nematic's `MisfinEngine`). 8 tests. |
| **webfinger** (de-nostr'd) | `crates/murm/webfinger` | RFC 7033 resolver; stripped the `nostr_identities` field + `nostr:` branches (no `nostr` crate dep — pure string handling). 4 tests. |

**nostr: dropped.** No load-bearing rationale in Mere (identity/federation runs on iroh + the `cable` protocol + ed25519 `persona` + `moothold`); nostr is an orthogonal external network whose only value is ecosystem interop, a speculative additive lane. So `graphshell-comms/identity.rs` (nostr/ActivityPub identity resolution) and the nostr dep are **not** pulled.

All eight donor `register-*` crates are now in mere (the ninth, `register-diagnostics`, was already there). The two reconciliation calls worth remembering: viewer's retired `VersoAddress` internal-routing was *dropped* (not reconstructed) as a host concern; lens's physics-preset types were *internalised* (lens owns its preset value types; the runtime bridge to the deleted `ForceDirectedState` was dropped, since mere's physics model differs and a consumer maps the exported tuning onto it).

## Also not pulled (comms)

- **`graphshell-comms/transport.rs`** (~1.2k, generic TLS transport) and **`capabilities.rs`** (~320, MiddlenetProtocol/freshness vocabulary) — `misfin` and `webfinger` had zero coupling to them, so they were not needed for this pull. Revisit if a future comms consumer wants the shared transport/capability layer; `murm/transport` (iroh/QUIC) is the current transport authority.

## Trajectory

- **Now**: everything self-contained or reconcilable is pulled and green — the full `register-*` cluster (8 crates), `misfin` + `webfinger`, `crates/import`, JSON Feed, and Spartan/Titan. Full workspace builds clean. The donor repo can be archived as a unit.
- **Wiring (future, consumer-side)**: the pulled registries and presets have no live host caller yet. When the Xilem host / renderer-registry slice lands, it wires `register-viewer`/`-protocol`/`-mod-loader` into dispatch and maps `register-lens` `GraphPhysicsTuning` onto mere's `scene_physics` runtime. That's integration, not salvage.
- **Reference-only remainder**: the iced lane, vendored forks, and the egui monolith stay donor-side; their concepts were harvested at the doc level. The donor `graphshell-comms` `transport.rs`/`capabilities.rs` and the nostr/ActivityPub `identity.rs` stay donor-side too (not needed by what was pulled).
