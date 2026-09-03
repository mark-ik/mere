# Archived-plan tails — deferred items spun out of the archive passes

**Date**: 2026-07-03, extended 2026-08-06 and 2026-09-02.
**Status**: backlog holder. Each item below was explicitly deferred by a plan that
is otherwise complete and now lives under
[`archive_docs/`](../../archive_docs/), in the checkpoint folder named by the
section it sits under. None of these gate anything today; pick up when the
relevant lane is quiet. Items already tracked by an active plan are *not*
repeated here.

This holds the tails from every pass rather than one per pass: a deferred item
is easier to find in one backlog than across a folder of dated stubs, and the
sections say which plan each came from.

## From native_surface_compositing (complete 2026-06-21)

- **wgpu-scry non-blocking capture settle** — `start_capture`'s ~500ms settle blocks
  the UI thread on a (now rare, backed-off) stall-restart; make it non-blocking in
  wgpu-scry. Needs demo runtime verification, not a blind edit.
- **Cache-flush per-tile submit batching** (flagged cleanup).
- **Silent implicit-sync fallback** when the explicit D3D12 fence fails — should
  warn + fail the tile under D3D12.
- **`favicon_data_uri` misnomer** — it also encodes the snapshot peek; rename.

## From documentscript_net_hardening (substantially complete 2026-06-24)

- **E1 refinement, uncredentialed same-origin fetch** — a same-origin `net.fetch`
  still carries that origin's own cookies; drop even those (Mark's session-store
  domain, A1 step 1).
- **E1 refinement, cross-origin `net` declarations** — a mod manifest declaring
  extra origins beyond its page, with broad-glob guards.
- **E2 — true non-blocking fiber suspension** for `net.fetch`: dispatch onto the
  off-thread fetch actor and resume the fiber, so the content actor keeps servicing
  commands during I/O (A2's 30s timeout mitigates today).
- **E3 — mod install/approval + optional signing** (the proper B1 fix; today any
  approved-capability `.wasm` in `mods/` auto-attaches).
- **B4 — `.cwasm` AOT mods invisible to discovery** (loader supports them; discovery
  matches only `*.wasm`).
- **D2 — `Fetched` carries no HTTP status** (non-2xx collapses to `Err`; guests
  can't observe 404/3xx). Touches `fetch.rs`.
- **DNS-rebinding defence** — the SSRF guard is a literal-host deny-list;
  resolve-then-pin is the follow-on.

## From find_in_page_host_ui (feature complete 2026-06-16)

- **Gemtext/document-lane find — closed 2026-07-03.** The user-facing retained-text
  slice landed in the
  [retained_text_tiled_render_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-15_retained_text_tiled_render_plan.md):
  Ctrl+F searches retained document blocks, paints block-scoped highlights, and
  page-text copy works from the retained packet. The precise glyph→char cluster
  map on `GlyphRun` remains only a fidelity follow-on, not an open backlog tail.
- **Paste into the find field** — `handle_clipboard_shortcut` routes to
  omnibar/palette only.

## From context_submenus (implemented 2026-06-25)

- **GUI feel verify pass** — pixel placement of the flyout, live hover/keyboard
  feel (Mark-runs-it; logic + DOM are tested).
- **Hover-open with delay** (cursor-move handler + an `Instant`-timed
  `submenu_hover` field).
- **Submenu mis-anchor when the root menu is scrolled** (`row_y` ignores the root's
  scroll offset; narrow case).
- **Mouse hit-test after keyboard-scroll** uses unscrolled offsets (pre-existing for
  the flat menu).
- **Depth-N submenus** — model is depth-1 by convention; deeper needs a path, not an
  index.

## From keyed_view_sequence (implemented 2026-07-02)

- **P4 — `ElementSplice` move primitive** for state-preserving arbitrary reorder.
  Gated on ui_polish finding-5 (paint-list emission cost) landing first, so
  delete+reinsert's real cost is separately visible. Plus OQ-1 (linear key lookup
  vs hash index — profile first) and OQ-2 (duplicate-key policy).

## From engram_compose_merge (P1–P3 done 2026-06-30)

- **rkyv compaction of graph engrams** (Alembic tail B6) — override
  `TypedPayload::serialize_to_bytes` for `GraphEngram` with rkyv (mind the
  read-alignment `AlignedVec` gotcha). Measure the size win first.
- **Promote `merge_snapshots` into the kernel** (`Graph::merge_from`, reusing the
  URL index + edge API) once the kernel is quiet — the snapshot-level merge was the
  non-colliding path, not the final shape.

## 2026-07-04 archive pass (12 more plans → `archive_docs/2026-07-04_completed_plans/`)

### From document_style_sheet (P0-P4 complete 2026-06-22, inker)

- **Container + non-text roles** (Quote / List) — own plan when consumer
  pressure appears; deliberately not built at the tail of P4.
- **Visual arrow check** on document-lane sheet arrows was deferred at closeout.

### From document_typography_surface (D1-D3 shipped 2026-06-22, inker)

- **D4 — per-role + per-engine typography overrides** (advanced section) and
  **full font enumeration**. The seed-palette plan's "surface per-role document
  knobs in settings" deferral is the same item; do them together.

### From surface_engine_contract_fold (complete 2026-07-04, inker)

- **`SecondaryForward` rename** — the flip shim kept the old `ProducerSurface`
  name while becoming generic over `WebSurface`; rename when touched (cosmetic).

### From retained_text_tiled_render (acceptance met 2026-07-03)

- **Image-only inline links** — an `<a>` wrapping only a replaced element
  establishes no box and is not harvested.
- **Per-band link re-harvest caching** — links re-harvest on every band
  re-emit; cache once a tall many-link page measurably bites.
- **Glyph→char cluster map on `GlyphRun`** — upgrades document-lane find/select
  from block-scoped to exact intra-line geometry (fidelity, not function).

### From seed_palette_theme_system (complete 2026-06-22)

- **TOML swap** for the theme file format (kept format-agnostic); **rescan-on-
  demand** for theme packs (startup-only today).

### From gnode_pool (landed 2026-07-02, follow-ons resolved 2026-07-03)

- No open gnode-pool debt. The residual (loaded-session `chrome_us` /
  `chrome_raster_us` on legitimately-dirty frames) is owned by genet's
  `docs/2026-07-03_shell_paint_emission_raster_plan.md`; the like-for-like
  loaded-session capture is that plan's motivating measurement.

No tails: chrome_bar_refinement (its deferred switcher cleanup was completed
in-plan), ui_dpi_scaling, gloss_scene_to_dom (follow-ups owned by the active
gloss_outline_lens plan), graphlet_wiring (cross-plan leftovers owned by the
active relational_browse plan), tearout_composability (continuation is the
active tearout_gestures plan).

## 2026-09-02 archive pass (17 retired plans → `archive_docs/2026-09-02_retired_plans/`)

Retired, not completed: each plan's subject was deleted — fifteen by the
meerkat removal of 2026-07-18 (`c5f01064`) or genet's Stylo and `genet-layout`
retirement of 2026-08-21 (`55c05d11759`) — or its status had read "in
progress" for two months with no commits. Ruled by Mark 2026-09-02 on the
active-tree audit's evidence (phase D of
[the doc policy consolidation plan](./2026-08-24_doc_policy_consolidation_plan.md)).
An evidence pass over turnstone, genet and mere found none of the work built
under another name. Items belonging to genet are marked; they want a home in
genet's `design_docs/`, not here.

### From short_term_memory_substrate (plan-only since 2026-05-14)

- **JSON-sidecar-over-fjall for short-term persistent state**, and the
  **`durable` flag for throwaway forks** — two unimplemented rulings. Their gate
  ("when branch operations land") never fired; the tear-out arc closed without
  a `branch_store`.

### From accesskit_screen_reader_verification (never run)

- **Separate adapter installation from OS traversal** when VoiceOver cannot
  enter the window tree (the macOS step). The checklist's purpose was met by
  Turnstone `648bf19`, the headed OS screen-reader receipt; a new checklist
  would target `ports/graphshell`.

### From workbench_staging (no code, 2026-06-09)

- **Where the latent staging relation lives** — gloss-owned graphlet store
  versus a kernel edge family flagged latent — explicitly Mark's call,
  unanswered; with it the chain-versus-bus default. The set primitive exists
  unconsumed (`platen/src/workbench.rs:182 open_split`, `:189 open_stack`);
  turnstone's workbench opens one node at a time.
- **Check the 2026-06-27 toolbar-clipping issue** against
  `turnstone:src/workbench_tiling.rs`; it may still reproduce.

### From lane0_sidequests (five of seven open)

- **Refreshed list** (evidence pass 2026-09-02): items 1 and 2 shipped; item 5,
  the Trail affordance, is met by the palette entry at
  `turnstone:src/panes/registry.rs:342-358`; **item 6, Barnes-Hut, is one
  `add_force` line from live** — `BarnesHutRepulsion` is built and exported in
  seiche, unwired at `crates/canvas/canvas/src/seiche_bridge.rs:58` (check
  whether "tuning" is the real blocker); items 3 (relation-kind picker — every
  `assert_selected_relation` caller passes `UserGrouped`), 4 (tessera score on
  the chip — `Ledger::score` exists, no consumer) and 7 (Steward per-row
  controls — the pane is a read-only downloads projection) are unbuilt with
  their substrate present.
- **Forme dead-submodule cleanup, never done**: `graphlet`, `lens`, `parity`,
  `pressure`, `reconciliation` still exist at `crates/forme/forme/src/`. A
  deletion; wants a yes.

### From command_registry_configurable_menus (P1–P5 landed, deleted with meerkat)

- **The registry-as-one-seam thesis and the S1–S3 searchable-menu design**,
  with three corrections from the evidence pass: turnstone re-decided P2-rest
  as flat palette rows, not a picker (`action.rs:554-566`); P4's configurable
  menu is absent (the catalog is hardcoded, `palette.rs:1-6`); the thesis is
  partly realized in turnstone's single `Action` catalog read by palette,
  snapshot and automation alike.
- **Orphaned persisted fields**: `PersonaSettings.menu_actions` and
  `command_usage` at `crates/system/pandect/src/persona_settings_store.rs:39`
  survive with a round-trip test and zero consumers in mere or turnstone.
  Reconsume in turnstone or drop from the schema — a code ruling.

### From object_card (P0 done, host deleted)

- **The Widget / Preset / type-scoped-card model and P1–P4** — unimplemented
  design. Widget 1's logic lives on in canvas (`SIZE_TIERS` at
  `crates/canvas/canvas/src/lib.rs:182`, `node_size_tier`).

### From layout_phase_split_probe (never built; genet)

- **The parallel-cascade thesis has no mechanism left.** It rode Stylo's rayon
  traversal (`genet:docs/2026-06-13_parallel_cascade_scope.md`, deferred), and
  Stylo and `genet-layout` were retired in genet `55c05d11759`; buckram,
  genet-livery and cambium carry no `rayon` and no timing. The root
  `2026-06-21_substrate_parallelism_composition_brief.md` still presents the
  thesis as live and needs a note. A measurement of the new stack is a new
  genet plan.

### From meerkat_render_perf (meerkat only)

- **Three host-agnostic principles**: M2's dirty-gate rule, M3's settled-scene
  per-key caching, M4's per-`(member, viewport)` scene set. M2 speaks directly
  to the live turnstone perf item, the command-palette lag recorded in the
  device resident consolidation plan on 2026-08-22.

### From host_scroll_engine_adoption (never started; genet)

- **P3's nested scroll-into-view gap** and **P4's "engine owns the scrollbar
  thumb, host does not paint one"** — engine-side asks for genet's
  `design_docs/`.

### From tracing_reach_and_quality (T1, T1.5, T2-substrate landed; spine deleted)

- **Lesson worth a workspace note**: a tracing layer must never panic, and
  per-span extension writes must be idempotent.
- **T3 in-tree engine and graph-kernel spans, T4 correlation, T5 registry
  sampling plus error-chain capture** — re-scopable against turnstone and
  djinn; the armillary and register-diagnostics halves survive (`spawn_named`
  at `crates/armillary/src/actor.rs:128`).

### From notification_subsystem (Phase 0 deleted; Phases 1–4 never built)

- **The model**: the log is the Steward's, the toast is the chrome's, actions
  ride the toast, continuous chips are not notifications; dedupe and
  rate-limit before rollout. Turnstone has no notification concept (its
  `AppEvent` stream is telemetry for the a11y and app arms).
- **Dead code**: `ToastSpec` / `ToastSeverity` / `FrameViewModel.toasts` at
  `crates/shell/chrome/src/frame_model.rs:404-416`, egui/iced-era, no producer,
  no consumer. A code ruling.

### From graph_object_roster_detail_cards (model migrated, views deleted)

- The model lives on verbatim — `RosterTab`/`RosterSubject`/`GraphletSpec` in
  `crates/domain/roster`, `EdgeCell`/`EdgeFamily` and `visible_relation_edges`
  in canvas, the selectors in `crates/graph/graphlets`. Open: **sub-kind
  selector editing**, the **P5 `GraphDefault < GraphViewOverride <
  SelectionOverride` stack**, **true parallel edge instances**; the plan's
  §Contradictions and §Pitfalls are durable design notes.

### From kith_capability_sharing (gate cleared 2026-08-09, never started)

- **Owed by name**: `crates/mesh/mesh/src/lease.rs:22-26` — "the kith plan,
  which widens the ring beyond one owner, has to revisit it"; gemot is still
  at the ring rule with capability gating a later milestone. **Re-scope onto
  the shipped vocabulary** — `crates/capability` (`Cap::{Power,Scope,Facet}`),
  gemot `typed_authorization.rs` (ruled 2026-07-24), personae delegation
  certificates — keeping only the mesh-specific parts: claim validation in the
  board fold, epoch revocation, the six done-conditions. Meadowcap-shaped
  grants exist in `crates/servitor/src/grant.rs`; notochord may already
  answer "was this chain valid at T".

### From ui_polish (S1–S4 executed 2026-07-05, status never advanced)

- **P5 canvas-text-scaling policy** — fixed labels versus zoom-scaled versus
  hybrid, as a setting with an LOD floor — and **OQ-1**, whether Ctrl+zoom
  reaches content. Turnstone-relevant.
- **Finding 5's retained/fragment-keyed paint-list ask** belongs to genet.

### From comms_gating_and_key_addressing (never built; successor differently shaped)

- **Default-off comms as a product requirement** — belongs in the djinn family
  resident services plan, since djinn owns network runtimes. Turnstone's
  `src/place/` already delivers G1/G2's outcome by construction: every network
  action is a user verb.
- **G3–G6 untouched**: the refresh-ticket versus rotate-identity verb split;
  the Windows/firewalld bind posture; the `mere/misfin/v1` ALPN; the
  blueprint-over-protocol ruling and the LXMF finding (store-and-forward tier).

### From signet_trust_plane (S0, S1 and the grant half landed elsewhere; premise reversed)

- **OQ1** the trust-plane umbrella name; **OQ4** meadowcap versus Biscuit grant
  format (with kith above); **S3** a first non-mere consumer, the one open
  rung. OQ2 and OQ5 are answered in code (`personae::carry`,
  `wallet_grant/epochs.rs`).

### From irc_mod (moothold; design step only, 2026-05-05)

- No open tail. Its shape remains the template for future T1 protocol mods
  (Nostr, Matrix, ATproto).

## Progress

- **2026-07-03** — created during the archive/reconcile pass; 11 completed plans
  moved to `archive_docs/2026-07-03_completed_plans/`, their deferred items
  collected here.
- **2026-07-04** — reconciled the find-in-page carry-over: the document-lane
  retained-text find/copy acceptance slice is now closed in the retained-text
  plan, while paste into the find field remains open.
- **2026-07-04** — second archive pass: 11 more completed plans moved to
  `archive_docs/2026-07-04_completed_plans/` (joining the concurrently-archived
  misfin promotion plan); their tails added in the 2026-07-04 section above.
- **2026-09-02** — third archive pass, the first for *retired* rather than
  completed plans: 17 moved to `archive_docs/2026-09-02_retired_plans/` on
  Mark's ruling over the active-tree audit; tails in the 2026-09-02 section
  above. Four of them are code rulings rather than doc tails: the orphaned
  `PersonaSettings` fields, the forme submodules, `ToastSpec`, and the one-line
  Barnes-Hut wire.

## From stickleback_replication_promotion (complete 2026-07-27, archived 2026-08-06)

- **A sibling repository for `stickleback`, gated on a real external consumer.**
  The promotion deliberately stopped inside Mere: `stickleback` 0.1.0 is
  published from this repository under MIT OR Apache-2.0, and S3 passed the
  publishable-boundary review, so nothing technical blocks the move. What is
  missing is a reason. The plan's own rule was that a domain-neutral crate earns
  a sibling repo when someone outside this workspace depends on it, not when it
  merely could. Revisit when an external consumer appears, and not before.

## From forest_dom (landed 2026-07-18, archived 2026-08-06)

- **F4, per-window multi-DPI.** Per-window DPI, viewport, and cascade, deferred
  on purpose rather than missed: the plan's own instruction was not to
  gold-plate the per-window cascade before F3 had banked the topology, and F3
  has. Revisit when a real multi-monitor case wants it, which is also the only
  situation that can say whether the cascade needs to be per-window at all.

## From eidetic_on_muniment (landed 2026-07-12, archived 2026-08-06)

- **mooting adopting `eidetic-fjall`.** It already rides muniment, so this is a
  manifest change rather than a port. The plan's third done-condition also
  named meerkat's suite as pending the genet ring-3 rename. That suite has no
  standing home to be pending *in*: meerkat was **decomposed**, not discarded,
  so the check it described now belongs to whichever of mere's crates or
  turnstone inherited the code it covered. The fourth, a correction owed to the boundary-pass plan's
  point 5, was already recorded there and needs nothing.

## From mere_turnstone_boundary_pass (landed 2026-07-09, archived 2026-08-06)

- **Splitting `session-runtime`'s mixed concerns.** The plan named the whole
  list: graph engram and session stores, wallet and identity, frame layout and
  tearout, browser content, engine profile and image stores, settings, and
  scripts. Only the settings slice is tracked today, by the
  [configuration ownership plan](./2026-08-06_configuration_ownership_settings_projection_plan.md);
  the rest is unowned. Worth naming before the crate is treated as settled.
- **Production journal persistence** (the G5 follow-on list). Until it lands,
  the delta vocabulary can still change without forcing a durable-log
  migration, which is the reason it was safe to defer and the reason it stops
  being safe once a real log exists.

## From host_wiring_grabbag (genet side complete 2026-06-12, archived 2026-08-06)

- **Four genet seams with no caller yet: G1.1 `on_wheel`, G1.2 transform
  hit-test, G1.3 pointer cancel, G2.3 keyboard escapes.** All eight seams
  landed genet-side; four were runway whose done-condition was adoption by
  meerkat. That condition did not evaporate when meerkat was decomposed, it
  moved: the adopter is now whichever surface inherited those callers, which is
  turnstone for the app-shell ones. Re-read against turnstone before assuming
  any of the four is still unadopted.

