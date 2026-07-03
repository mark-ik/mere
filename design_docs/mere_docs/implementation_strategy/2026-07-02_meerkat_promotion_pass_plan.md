# Meerkat Promotion Pass Plan

**Date**: 2026-07-02
**Status**: P1/P2/P3-first-slice/P4/P5/P6/P7 promoted. P8's input-snapshot seam,
first two domain moves, and roster's pure helper layer landed, but the
roster/pane-data host builders still remain in `meerkat`. The follow-up review
on 2026-07-02 tightened P3/P4/P8 before code move. A later implementation pass
on 2026-07-02 landed P1/P2/P5/P6/P7 and then re-ran the P3 seam check. The
compile witness for P1/P2 is still blocked by unrelated workspace breakage
(`session-runtime` and earlier `graph-kernel` errors), but the nearby imports
settled enough to confirm the wider P3 contract shape below.
**Scope**: Promote host-neutral domain and infrastructure modules out of the
`meerkat` crate into workspace crates, and consolidate what stays. meerkat is
54,760 LOC across ~86 modules, with ~85 of them declared in `main.rs` (the bin)
and only the small chrome view-model set in `lib.rs`. The crate's own
description says "chrome shell"; this plan makes the code match it.
**Relates to**: the
[render ladder + extraction plan](2026-06-23_render_ladder_and_extraction_plan.md)
(frames the crawl/extraction axis as corpus + agent infrastructure, not shell
code), the archived
[serval render glue extraction plan](../../archive_docs/2026-07-03_completed_plans/2026-06-11_serval_render_glue_extraction_plan.md)
(the precedent: own the seam, depend on components), the
[meerkat CLI tooling plan](../../archive_docs/2026-07-03_completed_plans/2026-06-29_meerkat_cli_tooling_plan.md) (owns the
agent_harness cluster; out of scope here), and the mere-domain layer direction
(existing pane-domain crates live under `crates/platen/domain/` today, but the
second-pass review below narrows that as historical precedent rather than the
durable home for every future data half).

---

## Meerkat Promotion Pass Plan

Ordering follows the dependency chain: crawl's one crate-internal reference is
`crate::fetch::Fetched`, and transfer's two are `fetch::Fetched` +
`card::LinkHit`, so the fetch actor moves first and unblocks both.

### P1 — fetch actor crate

Move `src/fetch.rs` (382 LOC) + `src/fetch/cookies.rs` (289) +
`src/fetch/tests.rs` into a workspace crate. It is already an armillary actor
with zero `crate::` references: routes http(s) to netfetcher and smolweb
schemes to errand, owns the eidetic-persisted cookie jar, and defines the
`Fetched` / `FetchOutcome` types the crawl and transfer modules consume.

- Home: `crates/system/fetch` (decided 2026-07-02): host infrastructure
  alongside the registry family, not a top-level crate.
- Deps it carries out: armillary, netfetcher, errand, eidetic,
  session-runtime, tokio, serde, rustls-pki-types, url.
- Done when: crate builds standalone, meerkat consumes it via
  `workspace = true`, `cargo test -p meerkat --bin meerkat` green, and no
  `mod fetch;` remains in `main.rs`.

### P2 — crawl crate

Move `src/crawl/` (mod 408 + frontier 253 + robots 151 + sitemap 67 +
tests 534 LOC) into a workspace crate depending on the P1 fetch crate.
frontier / robots / sitemap are pure (zero `crate::` refs); `mod.rs` touches
meerkat exactly once (`crate::fetch::Fetched`, resolved by P1). The render
ladder plan already frames this lane as serving the eidetic corpus and the
agent/flora lane, so it has non-shell consumers by design.

- Home: `crates/crawl`, one crate with frontier / robots / sitemap as
  submodules (per the bundle-when-lockstep rule; they have no other consumer).
  Not `crates/intel` (that is embeddings/signals).
- Done when: crate builds standalone with its policy tests, meerkat's crawl
  actor wiring consumes it, crawl tests pass outside meerkat.

### P3 — content transfer contract crate

Move `src/content/transfer.rs` (1,194 LOC, double the 600 ceiling) behind a
small contract crate, but not as a blind file move. Its own header says it is
"the browser-worker seam": flat postcard byte envelopes for content updates
plus the sender/receiver asset cache. Wire forms living in one endpoint's
crate fork silently once `meerkat-browser-worker` (today a 109-LOC bootstrap)
grows a real content transport; extract before that happens, not after.

- Reassessment result (2026-07-02, after the P1/P2 move and compile attempt):
  this should be a **content-message contract crate**, not a `transfer.rs`
  extraction. The compile attempt could not complete because unrelated
  workspace errors still stop `meerkat` before this seam, but the settled local
  imports confirm that `transfer.rs` is coupled to `ContentCommand`,
  `ContentState`, and `ContentUpdate` in addition to `Fetched` and `LinkHit`.
  The real move is therefore: (a) own the content command/update contract and
  its wire forms in one crate, while (b) leaving native content runtime logic in
  meerkat.
- Mechanical cut already verified: `crate::card::LinkHit` is not an open design
  problem. `transfer.rs` defines `LinkHitWire` with `From` conversions both
  ways; the extracted contract can take `LinkHitWire` while the conversions stay
  beside `LinkHit`.
- First slice landed 2026-07-03: `crates/system/content-contract` now owns the
  transferable content message vocabulary (`ContentCommandMessage` /
  `ContentUpdateMessage`), scene/font/image transfer caches, and the postcard
  transport. `meerkat::content::transfer` was reduced to native<->message
  adapters, so the browser-worker seam no longer lives only in one endpoint
  crate. Native runtime enums and actor logic still stay in `meerkat` for now.
- The split also retires the file's `#![allow(dead_code)]`: the transport API is
  now public in its own crate, though the old file-local tests were not carried
  over as part of this first slice.
- Done when: the post-P1/P2 reassessment chooses the contract boundary, the
  chosen crate builds for native and `wasm32`, meerkat and (when wired)
  meerkat-browser-worker both consume it, and the file-size ceiling is
  satisfied.

### P4 — web clip's pure core into the import lane

`src/web_clip.rs` (970 LOC, over ceiling) is a split, not a whole-file move:
the audit correction (see Findings) found one host seam, `use
super::{WindowCtx, fetch, render}` feeding a single `impl WindowCtx` block
(`finish_clip_pick` / `cancel_clip_picker`). Everything else is a pure core:
  `ClipFragment`, `parse_web_clip`, `web_clip_script`, `attach_cropped_visual`,
  on kernel / forme / inker / image / serde. It turns a page into graph
  material, the same shape as `crates/import` ("portable page seeds for the
  graph").
- One extra utility cut is required: the "pure" half still calls
  `render::base64_encode` when `attach_cropped_visual` mints the PNG data URI.
  That helper must move with the pure core (or be replaced in import) so P4 is
  not blocked on a hidden host dependency.
- Landed 2026-07-03: `import::web_clip` now owns the clip fragment model,
  script/result parsing, document-body fallback, image crop + PNG/data-URI
  helper, knot emission, and graph-write helper. `meerkat::web_clip` keeps the
  `WindowCtx` wrapper and cache/live-body lookup only, and the old hidden
  `render::base64_encode` dependency was replaced inside `import`.

- Home: `crates/import` (decided 2026-07-02). Cost accepted: import gains
  inker + image deps it does not have today.
- Done when: the pure core lives in import with its tests, meerkat keeps only
  the `impl WindowCtx` wrapper, both halves under the ceiling.

### P5 — graphlets into the graph family

`src/graphlets.rs` (400) + `src/graphlet_classifier.rs` (260) +
`src/graphlets_tests.rs` (299): pure forme + kernel + serde derivation, zero
`crate::` refs. Pure graph-shape derivation belongs with the graph crates.

- Home: `crates/graph/graphlets` beside glossary / graph-kernel /
  linked-data / node-lineage.
- Done when: derivation + classifier + tests build there; meerkat's
  roster_view_graphlets consumes the crate.

### P6 — History and suggest into chrome

`src/nav.rs` (381, pure) and `src/suggest.rs` (324, one ref: `crate::nav`)
are host-neutral chrome view-model material. `lib.rs` itself documents the M4
contract ("host widgets render from these types through the view-model");
meerkat growing its own `History` beside `crates/shell/chrome` is drift from
that contract. graphshell projected `can_go_*` from a servo viewer; meerkat's
linear `History` is the host-neutral replacement and should live where the
toolbar state lives.

- Home: `crates/shell/chrome` (nav + suggest as modules there).
- Done when: chrome owns History / NavTarget / suggestion generation, meerkat
  imports them, existing suggest + nav tests move across.

### P7 — theme editing's core into register-theme

`src/theme_edit.rs` (217 LOC): same split shape as P4. One host seam (`use
super::{WindowCtx, apparatus}`, one `impl WindowCtx` block); the core (HSL
seed nudges, theme forking) is register-theme + tincture only, and
register-theme already depends on tincture, so the fold carries no new dep.

- Home: into `register-theme` itself (bundle bias; one consumer).
- Done when: meerkat's theme editor views call the promoted API through the
  retained `WindowCtx` wrapper.

### P8 — pane data halves into a new crates/domain/ tree

`src/roster_data.rs` (558) + `src/roster_facet_data.rs`, `src/pane_data.rs`
(294), `src/gloss_outline_data.rs` (58) are data/projection halves with 1-2
`crate::` refs each, and those refs point *into view modules*
(`roster_facet_data`, `gloss_outline_view`, `list_pane::PaneItem`), meaning
view types currently leak into data.

- Home: a new `crates/domain/` tree (decided 2026-07-02, second pass): the
  mere-domain layer made real as a neutral top-level directory,
  `domain/{roster, gloss, ...}`. The first-pass answer (`platen/domain/`,
  following the apparatus precedent) was reversed on review: platen's
  doctrine is tile composition only, and the two existing tenants are not
  alike. **workbench** is platen's own projection domain and stays;
  **apparatus** merely landed there and migrates to `crates/domain/` whenever
  next touched. The phase had to start by dealing with the pre-existing
  `crates/platen/domain/gloss` occupant rather than pretending the name was
  free. Roster and gloss data have no platen affinity (kernel/forme and
  glossary material respectively), so sending them to platen would grow it into
  the grab-bag the accessory-orchestrator doctrine forbids. The old "wait for
  the mere-domain layout" gate stays dropped: every mere-domain reference in
  design_docs is archived gpui-era material, so this plan sets the layout
  rather than waiting on one.
- Prerequisite 1: add an input-snapshot seam. The current modules are not just
  bad output-type placements; they are `impl WindowCtx` builders over live host
  state (selection, workbench openness, content cache, store access, gloss row
  caps). A promoted data crate needs explicit input structs first.
- First slice landed 2026-07-02: `meerkat::pane_input_snapshot` now owns the
  read-only window selection/open/state snapshot, and `gloss_outline_data` plus
  roster's node-row/detail projection consume it instead of ad hoc live reads.
  That is intentionally an in-crate seam, not the final `crates/domain/` move;
  `pane_data.rs`, store-backed utility rows, and the remaining roster/gloss
  builders still read broader host state directly.
- Second slice landed 2026-07-02: `pane_data.rs` now consumes the same snapshot
  for its focused-member and presentation reads (Inspector, Trail history, the
  Alembic eviction-policy label, and pending-compose marker). The remaining
  direct host coupling in that file is the store-backed deleted-node/engram
  listing and the live content cache lookup, which stays in `meerkat` for now.
- Third slice landed 2026-07-02: the remaining `pane_data.rs` cache/store reads
  are now routed through named `meerkat` helpers (`inspector_pane_input`,
  deleted-node listing, engram listing) instead of direct `shared.content`
  traversal inside the row builders. That still is not a promoted domain
  contract, but it leaves the builder body talking to explicit inputs rather
  than host internals.
- Prerequisite 2: after the snapshot seam exists, invert the data-to-view
  imports so the data half defines the vocabulary and views consume it.
- First inversion slice landed 2026-07-02: the gloss outline row/snapshot
  vocabulary and row-capping logic now live in `gloss.rs`, with both
  `gloss_outline_data.rs` and `gloss_outline_view.rs` depending on that neutral
  module. That is the first real move from "data imports a view type" toward the
  P8 target shape.
- First domain slice landed 2026-07-03: the old `crates/platen/domain/gloss`
  crate was moved to the new top-level `crates/domain/gloss` home, and the
  host-neutral gloss vocabulary/geometry from `meerkat::gloss` moved into that
  crate. `meerkat` now depends on `gloss` directly for the outline snapshot
  types, pane-section math, and minimap helpers, while the host snapshot builder
  (`gloss_outline_data.rs`) and render/input wiring still stay local.
- Follow-up gloss slice landed 2026-07-03: the actual outline projection
  (`glossary::outline_rows` + node-state enrichment + row capping) now lives in
  `gloss::build_outline_snapshot`, so `gloss_outline_data.rs` is down to a thin
  `WindowCtx` wrapper that supplies per-window selection/state callbacks.
- Second domain slice landed 2026-07-03: `roster_model.rs` moved to the new
  `crates/domain/roster` crate, and `meerkat::roster` was reduced to the local
  CSS/view wrapper plus re-exports. The roster data builders, facet builders,
  and views now read the same vocabulary through the crate boundary without
  changing their host-local behavior.
- Third domain slice landed 2026-07-03: the roster crate now owns the pure
  helper layer that used to sit at the bottom of `roster_data.rs` and
  `roster_facet_data.rs` too: content bucketing, relation/graphlet/field
  labels and selectors, member-label formatting, and the roster facet/card
  helpers. `roster_data.rs`, `roster_facet_data.rs`, the graphlet roster view,
  and apparatus now consume those helpers through `crates/domain/roster`,
  while the actual `WindowCtx` graph/store walks still stay local pending a
  later builder-input contract cut.
- Second inversion slice landed 2026-07-02: the roster row/card/subject
  vocabulary now lives in `roster_model.rs`; `roster.rs` keeps the stylesheet
  surface plus re-exports, while `roster_data.rs`, `roster_facet_data.rs`,
  `roster_view.rs`, and the roster render/window glue now depend on the neutral
  module directly. That leaves the actual crate move for later, but the
  data-to-view ownership now matches the P8 direction.
- Done when: data halves build host-free with their tests; meerkat keeps the
  views.

### Cross-cutting: bin-to-lib for what stays

Modules that stay in meerkat but are host-neutral should move from `main.rs`
to `lib.rs` opportunistically as they are touched. The 85-bin / 14-lib split
is why `cargo check -p meerkat --lib` is false-clean; every module promoted
out or moved into the lib shrinks that blind spot. This is not its own phase;
apply it whenever a phase touches a neighbor module.

### Explicitly staying (host by nature)

`app_handler/`, `input/`, `render/`, `window_view/`, `session_ops/`,
`scrying_host/`, `constellation/`, `steward`, `node_ops`, `command_drain`,
the pane and roster *views*, settings views, menus, ime, viewport, titlebar.
These own the window, the event loop, or shell mutations. `agent_harness`
(470 + 1,726 test LOC) is owned by the
[CLI tooling plan](../../archive_docs/2026-07-03_completed_plans/2026-06-29_meerkat_cli_tooling_plan.md), not this one.
`command.rs` / `shell_eval.rs` stay because the `Command` enum names shell
mutations: it is host vocabulary by nature, not a portable domain waiting on
a consumer.

---

## Findings

### Import audit (2026-07-02)

Verified by grepping each candidate's `use` lines. "crate-refs" counts
`use crate::` plus `use super::` occurrences (for a top-level bin module,
`super::` is the crate root; the first audit pass missed this, corrected
2026-07-02). Test-module `use super::*` is exempt.

| Module | LOC | crate-refs | External deps | Verdict |
| --- | --- | --- | --- | --- |
| fetch.rs + cookies | 382 + 289 | 0 | armillary, netfetcher, errand, eidetic, session-runtime, tokio | promote (P1) |
| crawl/ (4 files + tests) | 1,413 | 1 (`fetch::Fetched`) | armillary, frame, linked-data, tokio | promote (P2) |
| content/transfer.rs | 1,194 | 5 (`fetch::Fetched`, `card::LinkHit`, `ContentCommand`, `ContentState`, `ContentUpdate`; `LinkHitWire` mirror already exists) | document-canvas, netrender, parley, linked-data, postcard | reassess contract (P3) |
| web_clip.rs | 970 | 1 (`super::` WindowCtx impl block) | kernel, forme, inker, image | split (P4) |
| graphlets + classifier + tests | 959 | 0 | forme, kernel, serde | promote (P5) |
| nav.rs | 381 | 0 | (std only) | promote (P6) |
| suggest.rs | 324 | 1 (`nav`) | chrome | promote (P6) |
| theme_edit.rs | 217 | 1 (`super::` WindowCtx impl block) | register-theme, tincture | split (P7) |
| roster_data.rs | 558 | 1 (view module) | kernel, forme | promote after inversion (P8) |
| pane_data.rs | 294 | 2 (view module) | kernel | promote after inversion (P8) |
| gloss_outline_data.rs | 58 | 1 (view module) | glossary | promote after inversion (P8) |

### Structure facts

- 54,760 LOC total (with tests), 168 `.rs` files. ~85 modules declared in
  `main.rs`, 14 in `lib.rs`. The bin-not-lib memory exists because of this
  split; promotion shrinks it from both ends.
- File-size ceiling (600) violations among candidates: transfer.rs 1,194,
  web_clip.rs 970. Elsewhere: agent_harness/tests.rs 1,726 (CLI plan),
  tests.rs 827, command_drain.rs 764, shell_eval.rs 638, window_view/mod.rs
  636, settings_lane/pages.rs 607, node_ops.rs 604, command.rs 603. The
  promotions clear the first two; the rest need ordinary splits, out of scope
  here.
- Existing homes checked before proposing new crates: `crates/intel` is
  embeddings/signals (wrong home for crawl); `crates/import` is browser-data
  import producing page seeds (right shape for web_clip);
  `crates/shell/chrome` is the designed home for chrome view-models;
  `crates/platen/domain/{apparatus,gloss,workbench}` is the current pane-domain
  layout the second-pass P8 answer deliberately narrows; `meerkat-browser-worker`
  is a 109-LOC bootstrap that does not yet consume the transfer wire form (the
  fork risk P3 still preempts once its contract boundary is chosen).
- Precedent: the serval render glue extraction (2026-06-11) established the
  pattern this plan repeats: own the seam, depend on components, zero
  upstream changes.

### Risks / notes

- P1 changes the `Fetched` import path in several modules (crawl, transfer,
  content actors); mechanical but wide.
- P8 is gated on an input-snapshot seam first, then the import inversion; lowest urgency.
- Workspace `Cargo.toml` gains members + `[workspace.dependencies]` entries
  per phase; keep git-dep pins as they are (no churn).
- `DOC_README.md`'s supercrate table is the canonical workspace map: P1 adds
  `fetch` to the `system` row, P2/P3 add their crates, P8 adds a `domain`
  row. Update the table in the same session each phase lands.

---

## Progress

- **2026-07-02**: Initial audit. Per-module import scan, LOC counts, existing-
  home check (intel / import / chrome / platen-domain / browser-worker), phase
  ordering derived from the `Fetched` dependency chain. No code moved.
- **2026-07-02** (same session): Open-questions review with Mark. Corrections:
  the audit's crate-ref counts had missed `use super::` (web_clip and
  theme_edit each carry one `impl WindowCtx` host seam; both reshaped from
  whole-file moves to pure-core splits), and P3's `LinkHit` question dissolved
  (`LinkHitWire` + conversions already exist in transfer.rs). Decisions: P1
  home = `crates/system/fetch`; P4 core = extend `crates/import`; P8 = a new
  `crates/domain/` tree (first-pass `platen/domain` answer reversed on Mark's
  "panes go in platen?" challenge: workbench is platen's own domain and
  stays, apparatus migrates when touched, roster/gloss have no platen
  affinity; the mere-domain gate cited only archived gpui-era docs, so this
  plan sets the layout). Reworded the command.rs rationale from consumer-pull
  to host-vocabulary-by-nature.
- **2026-07-02** (follow-up live-code review): tightened the implementation
  order before code move. P3 no longer claims `transfer.rs` can move as a
  standalone wire file: it also depends on `ContentCommand`, `ContentState`,
  and `ContentUpdate`, so the post-P1/P2 compile will choose whether the new
  crate owns the content contract or just the wire forms. P4 now names the
  hidden `render::base64_encode` helper cut in `attach_cropped_visual`. P8 now
  names the live `crates/platen/domain/gloss` occupant and adds an explicit
  input-snapshot prerequisite before the data/view inversion.
- **2026-07-03**: P3's first implementation slice landed. Added
  `crates/system/content-contract` for the transferable content message
  contract and scene asset transport, wired `meerkat` to consume it through a
  thin adapter layer, and verified `cargo check -p content-contract --lib` plus
  `cargo check -p meerkat --lib`. The fully native `ContentCommand` /
  `ContentUpdate` enums still live in `meerkat`; that larger move is no longer
  required for the worker seam itself.
- **2026-07-03**: P4 landed. Added `import::web_clip` for the host-neutral clip
  core, moved the clip tests with it, replaced the old
  `render::base64_encode` dependency with a local import-side helper, and
  reduced `meerkat::web_clip` to the `WindowCtx` host wrapper plus cache/live
  document lookup. Verified with `cargo check -p import --lib` and
  `cargo check -p meerkat --lib`.
- **2026-07-03**: P8's first domain move landed. Created the real
  `crates/domain/gloss` home by moving the old platen-side `gloss` crate there,
  folded the host-neutral `meerkat::gloss` vocabulary/geometry into it, deleted
  the local `meerkat` module, and rewired `meerkat` to consume the crate
  directly. Verified `cargo check -p gloss --lib` and `cargo check -p meerkat --lib`;
  then followed up by moving the pure outline-snapshot projection into
  `gloss::build_outline_snapshot`, leaving only the per-window wrapper in
  `meerkat`. The remaining P8 work is still roster/pane-data promotion.
- **2026-07-03**: P8's second domain move landed. Added `crates/domain/roster`
  for the neutral roster snapshot vocabulary, deleted `meerkat`'s local
  `roster_model.rs`, and rewired the roster data/view glue to consume the new
  crate through the existing `meerkat::roster` wrapper. Verified
  `cargo check -p roster --lib` and `cargo check -p meerkat --lib`.
- **2026-07-03**: P8's third domain move landed. `crates/domain/roster` now
  owns the roster helper layer too: content buckets, relation/field/graphlet
  labels and selectors, member-label formatting, and the facet/card helper
  builders. `roster_data.rs`, `roster_facet_data.rs`, the graphlet view, and
  apparatus were cut over to the crate boundary; the remaining work is the
  heavier builder-input extraction from `WindowCtx`.
- **2026-07-02** (later implementation pass): P8's next inversion slice landed.
  `roster_model.rs` now owns the roster snapshot vocabulary, `roster.rs` was
  reduced to stylesheet plus compatibility re-exports, and the roster
  data/view/render glue was pointed at the neutral module. The remaining P8
  work is crate placement and any further host-input slimming, not view-owned
  roster types.
