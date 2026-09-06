# Smolweb Host Integration Plan — the genet native lane in meerkat

**Date**: 2026-06-28
**Status:** **P1–P3 landed 2026-06-28** (reconciled 2026-07-01; genet `1bbbfdb`, `0b7ca87`,
`5c07ad5`; mere `476880b`, `0dd0c3e`, `3eed418`, `8dc3683`) — render, theme, scroll,
link nav all wired, but **compile-verified only, no headed run yet**. The 2026-07-01
review left open items (see Open questions): theme hard-coded to `App` against the
settled Site-default design, band-scroll cadence untested, and the trust-posture gap
(owned by the smolweb fidelity plan's Workstream 2). P4 optional, unstarted.
Separately, the scripted-live follow-on's `ResourceFetcher` trait mismatch at
`content/actor.rs:33` was **fixed 2026-07-01** (render-ladder plan's lane — see its
progress log; `--features scripted` now compiles, 5/5 tests). The smolweb feature
itself builds green.

> **Historical note (2026-09-05):** This is a Meerkat integration receipt. Its
> phase and path names preserve the landed context, but do not establish current
> Smolweb ownership; verify the active Genet and Turnstone documentation first.

**Thesis**: render a focused smolweb capsule (gemini/gopher/feed/…) in the Mere host
through the **genet lane** — the native `smolweb-views` render we built and shipped
in pelt (`SmolwebDocument`) — instead of the block-card reader path. The block lane
stays for cards/orrery; the genet lane is the focused tile. This is one new rung on
meerkat's content render ladder, not a new subsystem.

**Builds on**: the native smolweb rendering effort
(../../nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md (`design_docs/nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md`))
— errand transport+parse, `smolweb-views` (gemtext/gopher/feed views + per-site/app
theming), and `pelt_desktop::SmolwebDocument` (parse → view → `ScriptedDom` →
genet-layout → `Scene`, with scroll + chrome-browser link nav). Extends the render
ladder ([2026-06-23_render_ladder_and_extraction_plan.md](2026-06-23_render_ladder_and_extraction_plan.md)).

---

## The two lanes (settled)

Same fetched source, two distinct renderers — not two LODs of one render:

- **Block lane** (nematic → `Block` → `document_canvas` → `Scene`): the capsule as the
  semantic block model, shown as an **orrery card / reader**. Unchanged by this plan.
- **Genet lane** (errand parse → `smolweb-views` → `ScriptedDom` → genet-layout →
  `Scene`): the **native focused capsule** — themed, scrollable, link-navigable. New
  to meerkat (it already runs a genet lane for *HTML* via `StaticDocument`; this adds
  the *smolweb* genet lane).

Per-surface, automatic (Mark's call A): the focused tile uses the genet lane; cards
keep the block lane. No user toggle in v1 (the engine-activation system can add one
later).

## Why it fits meerkat as-is (findings, verified)

- The **content actor** ([crates/meerkat/src/content/mod.rs](../../../crates/meerkat/src/content/mod.rs) *(historical citation)* <!-- doc-audit: historical-link -->)
  runs off the UI thread, owns the genet cascade + nematic engines + a subresource
  cache, runs `render_content_scene`, and **ships a `Send` `Scene` back**; the kernel
  composites and stays sole GPU owner. So a lane that produces a `Scene` already has a
  home — we just produce it via `SmolwebDocument`.
- meerkat **already dispatches lanes**: `card::is_genet_html_lane` routes HTML to the
  genet `StaticDocument` path; everything else falls to the nematic block path. The
  smolweb branch slots in beside `is_genet_html_lane`.
- meerkat **already deps the pieces**: `pelt-desktop` (`tile-surface`) + `xilem-serval`
  + `genet-scripted-dom` (git). `SmolwebDocument` lives in pelt-desktop behind the
  `smolweb` feature; turning it on is the dep step.
- **Thread safety**: `SmolwebDocument` is `Rc`-based (`GenetAppRunner` /
  `ScriptedDom`), so it is **not `Send`** — but it is built, framed, and dropped
  entirely on the actor thread (only the `Scene` crosses), exactly like the existing
  genet cascade the `cascade-offthread` probe blessed. No boundary change.

## Phases

- **P1 — the lane.** Enable `pelt-desktop/smolweb` (+ `errand`) in meerkat. In the
  content actor's render dispatch, add: if the node is a smolweb scheme/engine, render
  via `SmolwebDocument::parse(url, body, theme).frame(w, h)` → `Scene` (retain the
  `SmolwebDocument` per tile in the actor's `current` state so scroll persists). Done:
  a focused gemini node renders the native capsule (glyphs + per-site theme) instead
  of the block card.
- **P2 — host theme.** Map meerkat's tinct theme to a `SmolwebPalette` and pass
  `SmolwebTheme::App(palette)` so capsules match the app chrome; honor the OS scheme
  for `System`. (The seam exists in `smolweb-views`; this is the host-side map, the
  illume `SyntaxKind→SyntaxRole` pattern.)
- **P3 — input.** Scroll the retained `SmolwebDocument` from the tile's wheel/keys;
  route a link click (`SmolwebDocument::click_at → Option<String>`) to a node
  navigation (mint/open the target the way meerkat opens any content node), so
  in-Mere link-following matches the pelt chrome browser.
- **P4 — activation (optional).** Register the genet smolweb lane in
  `engine_activation` so it can be disabled per session (falling back to the block
  lane), turning Mark's "A automatic" into a selectable rung if wanted.

## Open questions

- **Retention shape**: where the per-tile `SmolwebDocument` lives in the actor's state
  and how navigation/resize invalidates it (mirror the HTML lane's `ContentLayout`
  retention). *Settled in P1: a `Content` field, built lazily, left to `frame`'s own
  size-change detection (no explicit invalidation needed).*
- **Focused-vs-card selection**: confirm the trigger for "focused tile → genet lane"
  vs "card → block lane" in the existing focus model (`compute_focus_cards` /
  `collect_cards`). *Not yet confirmed against that model — P1's `is_smolweb_lane`
  branches on the content actor's own dispatch, which the actor only serves for the
  focused tile; cards render through the separate `render_content_scene` path
  entirely, so no explicit cross-check was needed, but this is worth a second look.*
- ~~Click→navigation~~ — resolved, see Progress (P3b landed).
- **Theme is hard-coded to App (2026-07-01 review) — contradicts the settled theming
  design.** P2 forces `SmolwebTheme::App` unconditionally, but the agreed design
  (native rendering plan; Mark 2026-06-28) is **Site (per-site hue) as the default**,
  with Plain/Light/Dark/App/System as user overrides — and the
  configurability-over-defaults rule points the same way. Pelt honors that (Site
  default); meerkat currently forecloses it. Wants a user setting whose default is
  revisited with Mark (Site vs App in-app is a real product question: capsule
  identity vs chrome cohesion), not a hard-coded pick.
- **Band-scroll cadence untested (2026-07-01 review).** The smolweb lane emits
  `band_h = viewport height` exactly, so *any* scroll leaves the band and triggers a
  `Scroll` round-trip + full re-frame; the HTML lane amortizes by requesting
  taller-than-viewport bands. Never runtime-tested (the whole meerkat lane is
  compile-verified only) — the first headed pass should watch for scroll jank, and
  the cheap fix if it appears is emitting a taller band the way the HTML lane does.
- **Trust posture (new, unresolved)**: the genet lane bypasses `Block`, so it drops
  the `DocumentTrustState` the block/card lane carries. A spartan (unauthenticated by
  design), gemini (TOFU), and misfin (signed sender) tile currently render with the
  same neutral chrome. Producing trust at the errand transport, carrying it on
  `SmolwebDocument`, and surfacing it as tile chrome is Workstream 2 of the
  smolweb fidelity plan (`design_docs/nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md`);
  it lands against this plan's P3/P4.

## Progress

- **2026-06-28**: Plan created. Design settled (genet lane = focused tile, block lane
  = cards; fits the content-actor `Scene` model).
- **2026-06-28**: **P1 landed.** `meerkat` `smolweb` feature → `pelt-desktop/smolweb`;
  `Content` gains a retained `SmolwebDocument` (feature-gated), built lazily in the
  content actor for a smolweb-scheme node with a ready body (`ensure_smolweb` /
  `is_smolweb_lane` in `content/handlers.rs`); `render` frames it to one viewport and
  emits `ContentUpdate::Scene` **exactly like the scripted lane** (internal scroll;
  host-band scroll deferred to P3). pelt-desktop re-exports `SmolwebTheme` /
  `SmolwebPalette` so the host can name the theme. Builds green both ways (`meerkat
  --features smolweb` and the base build; all additions cfg-gated). genet `1bbbfdb`,
  mere `476880b`. Compile-verified; runtime/headed check pending (mirrors the proven
  scripted lane). **Remaining**: P2 (host tinct → `SmolwebTheme::App`), P3 (scroll via
  the `band_y` delta → `SmolwebDocument::scroll_by`; link click → node navigation),
  P4 (optional activation rung).
- **2026-06-28**: **P2 landed** (mere `0dd0c3e`). The smolweb lane builds
  `SmolwebDocument` with `SmolwebTheme::App`, mapping the host theme-derived document
  colours (`sheet.colors`) + `CARD_BG` to a `SmolwebPalette` (rgb strings), so a native
  capsule themes like the app chrome instead of the per-site default. Rebuilt on
  `Retheme` (the actor clears the retained doc so the new sheet re-themes). Builds green.
- **2026-06-28**: **P3a (scroll) landed** (genet `0b7ca87`, mere `3eed418`).
  `SmolwebDocument` gained `content_height(w, h)` (viewport height + the layout
  session's `scroll_range` max — 2 new tests) and `scroll_to(y)` (absolute offset via
  `set_viewport_scroll`, the counterpart to the existing delta `scroll_by`/`scroll_at`/
  `scroll_for_key`). The smolweb render branch now reports the real content height
  and scrolls to `content.band_y` before framing, echoing `band_y`/`band_h` back —
  which plugs the lane into the **host's existing HTML-lane band protocol
  unchanged**: `constellation::request_scroll` already gates on `content_height >
  viewport` and dedupes `Scroll` commands against any Scene-based lane (it checks
  `activation.packet.is_none()`, which is already true for smolweb), so **no
  host-side change was needed** — only the actor-side content_height/scroll_to.
  Builds green (smolweb feature + base).
- **2026-06-28: P3b (link nav) — corrected and landed** (genet `5c07ad5`, mere
  `8dc3683`). The prior entry's "empty until Phase 5 lane parity" read was a stale
  doc comment, not the real state — a fresh investigation (dedicated agent pass,
  cross-checked by hand) found the mechanism **already works** for the block lane
  (`DocumentRenderPacket`'s static interaction list) and the HTML/genet lane
  (`genet_layout::link_harvest::harvest_link_rects`, wired through
  `ContentLayout::emit_band`, 4 passing tests). It is **not** a per-click round trip:
  the actor harvests every `<a href>`'s hit rect once at render time, ships the list
  alongside the scene (`ContentUpdate::Scene.links`), and the host caches + does its
  own point-in-rect test locally (`ConstellationOps::link_at`) — the same shape
  `ContentCommand::Find`/`FindMatches` uses for search. Only two lanes never called
  it: scripted-live and smolweb, both hardcoding `links: Vec::new()`. The harvester
  needs exactly the three fields `IncrementalLayout` already retains (`fragments`,
  `built`, `text_ctx` — the same session type `SmolwebDocument` *and* the
  scripted-live rung's `ScriptedDocument` both hold), so the fix was exposing it as
  `IncrementalLayout::link_rects` (was `pub(crate)`) and wiring
  `SmolwebDocument::links()` through the existing `LinkHit` field — no new
  `ContentCommand`, no round trip, architecturally identical to the working lanes.
  1 new genet test (8 total). Builds green. **Follow-on, not done here**: the
  scripted-live rung (`content.scripted_doc`) has the identical gap and the identical
  fix is now available (it retains an `IncrementalLayout` session too) — belongs to
  the render-ladder plan's lane, not this one. P4 unchanged.
- **2026-06-28: scripted-live follow-on landed too** (genet `1856486`, mere
  `737e0cd`). `ScriptedDocument::links()` reads the dom + retained cascade it already
  shares with the `getComputedStyle` bridge and calls the same
  `IncrementalLayout::link_rects`; 1 new test (37/37 in `genet-scripted`, incl.
  confirming a boxed anchor harvests both a text-line and a border-box rect — matches
  `link_harvest`'s own precedent, not a bug). meerkat's scripted-live render branch
  wired the same way as smolweb's. **Verification gap at the time, since resolved**:
  this meerkat edit could not be compiled — `meerkat --features scripted` failed on
  an `icu_calendar`/`temporal_rs` version mismatch, which turned out **not** to be
  boa's Temporal support at all.
- **2026-06-28: the actual dependency break, root-caused and fixed** (genet
  `5f50134`). `script-engine-nova` was an unconditional dependency of
  `genet-scripted` on every 64-bit target (`[target.'cfg(...)'.dependencies]`, no
  `optional`), even though every use of it in the crate (a Nova-specific
  `run_script` helper, its test module) was already `#[cfg(feature =
  "scripted-nova")]`-gated in the *code* — the crate never gated the *dependency* to
  match. So any Boa-only consumer still compiled `nova_vm`; on a top-level workspace
  without genet's own local-checkout override for it (mere, building meerkat), that
  resolved the **published crates.io `nova_vm 1.0.0`**, whose `temporal_rs = "0.1.2"`
  pin doesn't compile against the `icu_calendar` version unified into the graph —
  nothing to do with boa's own (perfectly fine) `temporal_rs 0.2.3`. Fixed by making
  `script-engine-nova` optional and `scripted-nova = ["dep:script-engine-nova"]`,
  plus extending the gate onto two spots the original code-level `#[cfg]` missed
  (`mod native`'s public `run_script` re-export — confirmed unused by any caller —
  and a test module gated on `test`+pointer-width but not the feature). Verified:
  default build clean (35/35 tests), `scripted-nova` clean (65/65, Nova included),
  `pelt --features scripted`/`scripted-nova` both clean, and **the icu_calendar
  error is completely gone from `meerkat --features scripted`**. One caveat found
  along the way, not fixed here (out of scope, dependency-graph-independent): a
  separate, pre-existing bug in `content/actor.rs` now surfaces — it passes a
  `pelt_core::ResourceFetcher` where `ScriptedDocument::from_body` wants
  `genet_scripted::ResourceFetcher`, two genuinely distinct traits. That's meerkat's
  own fix to make.
- **2026-07-01: scripted-feature break re-verified, then fixed same day.** `cargo
  check -p meerkat --features scripted` failed with the E0308 trait mismatch at
  `content/actor.rs:33` (`pelt_core::ResourceFetcher` at `ports/pelt-core/lib.rs:125` *(historical citation)* <!-- doc-audit: historical-path -->
  vs `genet_scripted::ResourceFetcher` at `components/genet-scripted/lib.rs:36`; no
  bridging impl existed — pelt-desktop's `LocalFetcher` implements both traits
  separately). Fixed in the render-ladder plan's lane (see its 2026-07-01 progress
  entry): pelt-desktop re-exports the genet trait as
  `pelt_desktop::ScriptResourceFetcher`, and meerkat's whole scripted-fetch seam
  (`ScriptFetcher`, `build_scripted`, test mocks) switched to it. `--features
  scripted` compiles, 5/5 scripted tests pass. The smolweb lane was never affected;
  theme remains hard-coded to `SmolwebTheme::App` (`content/handlers.rs:185`),
  confirming the open-question review item.
