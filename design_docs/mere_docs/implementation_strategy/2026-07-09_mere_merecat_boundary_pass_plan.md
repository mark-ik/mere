# Mere/Merecat Boundary Pass Plan

**Date**: 2026-07-09
**Status**: **Landed** (same day). Slices A/B/C all implemented and verified;
see Progress for the receipts and the two scope refinements implementation
forced (mime_hint stays kernel-side; lifecycle removed outright, no sidecar
runtime enum). Remaining follow-ons are listed in the decisions, not phases.
**Scope**: Sharpen the mere/merecat boundary ahead of the meerkat port:
(A) finish the armillary promotion (delete the in-mere copy), (B) finish the
vates promotion (move the decoder and the actor), and (C) split browser-runtime
state out of `kernel::graph::Node` into a host-owned sidecar. Also records the
boundary decisions this pass fixes, including the correction to the
[merecat founding](../../../../merecat/design_docs/2026-07-08_merecat_founding.md)
target shape.
**Relates to**: the merecat founding doc (amended this session), the
[generic graph substrate plan](../technical_architecture/2026-07-08_generic_graph_substrate_plan.md)
and [G5 rebase progress](../technical_architecture/2026-07-08_g5_mere_rebase_progress.md)
(the library seam this boundary rides), the
[node image externalization plan](2026-07-06_node_image_externalization_plan.md)
(owns the favicon/thumbnail fields; deliberately NOT folded into slice C),
the [meerkat promotion pass plan](2026-07-02_meerkat_promotion_pass_plan.md)
(the module-promotion precedent and audit method), the
[murm/moot sibling posture plan](2026-07-08_murm_moot_sibling_posture_plan.md)
(governs murm/moot; untouched here), and vates's own
`design_docs/2026-07-07_vates_founding_proposal.md`.

---

## Boundary decisions (fixed by this pass)

Recorded 2026-07-09 from the merecat-boundary review with Mark. Each was
verified against the tree, not carried from docs.

1. **Merecat is a new host architecture fed by promotions, not a copied
   crate.** meerkat is 174 files / 57,904 lines; its largest modules
   (`command_drain` 1,196, `render/cards` 948, `shell_eval` 884,
   `graph_delta_log` 864) are imperative crossroads. Donor material, wrong
   shape to preserve. repos/merecat stays a stub until the seam holds
   (founding doc sequencing, unchanged).
2. **orrery and platen do not become permanent parts of mere.** The `mere`
   facade re-exports them today (`crates/mere/src/lib.rs:14-15`); that is
   compatibility scaffolding for the in-workspace host, not the library
   boundary. orrery ships a winit bin over serval/netrender git deps; platen
   depends on document-canvas, pelt-core, and the netrender paint vocabulary.
   Both are application surfaces and move out with merecat. The founding
   doc's target shape ("mere = ... forme/platen, orrery ...") is amended
   accordingly this session.
3. **Moving orrery splits `crates/orrery/` three ways.** quint and seiche
   leave for their own repos (already-decided numen-stack publish follow-on,
   not this plan); the orrery scene host goes to the merecat canvas lane;
   arrangements and cartography travel with platen (platen depends on both;
   they are layout strategies and projection, same lane).
4. **The verso family is the heart of the merecat web lane.** verso,
   verso-api, verso-scry, verso-serval, and meerkat-browser-worker are the
   engine multiplexer and belong to merecat's engine-routing boundary. Any
   merecat crate sketch that omits them is drawing around the hole. The
   port's first vertical path (open address to visible surface) routes
   through verso-api from day one.
5. **eidetic stays mere-side, over muniment.** eidetic-core carries its own
   storage-backend-agnostic blob store trait and does not sit on muniment.
   Promoting "eidetic core/backends as a storage sibling" would found a
   second storage seam overlapping muniment days after muniment was founded
   with the explicit decision that eidetic stays mere-side. The eventual
   move is the inverse: rebase eidetic's backends onto muniment's seam (the
   way `mooting` already rides muniment). Out of scope here; recorded so it
   is not re-proposed.
6. **Browser-runtime state leaves `Node` (slice C).** As landed: four tenants
   moved to the host-owned sidecar keyed by node id (`session_scroll`,
   `session_form_draft`, `viewer_override`, `compat_mode`), and `lifecycle`
   was **removed outright** rather than moved. Implementation findings that
   refined the original six-tenant cut:
   - **mime_hint stays kernel-side.** Its live consumers are mere-domain
     (roster content bucketing, note-format detection) plus snapshot
     persistence; it is a fact about the content, not about the browser's
     handling of it. (The original boundary review never named it; adding it
     to the cut was overreach.)
   - **lifecycle had no consumer anywhere.** `set_node_lifecycle` was never
     called; meerkat's display lifecycle is forme's member `Lifecycle`
     (host-fed); `NodeLifecycle::Tombstone` was never constructed. Removed
     enum, field, setter, and the facet row. `Action::NodeMarkTombstone`
     stays declared (ux-events gates it); when the ghost feature lands it
     adds a kernel-truth tombstone fact then, deliberately.
   - **viewer_override / compat_mode never actually persisted.** The Node
     doc claimed sync survival, but `PersistedNode` never carried them and
     the delta log is env-gated diagnostics. The sidecar store makes their
     persistence real for the first time; compat routing still runs on the
     session-local `engine_pins` map, with the sidecar as the durable home
     the engine-picker X3 step wires up.
7. **Images are NOT part of slice C.** The
   [node image externalization plan](2026-07-06_node_image_externalization_plan.md)
   owns `thumbnail_png`/`favicon_rgba`: bytes go to the content-addressed
   blob store, and the small `ImageRef` map deliberately stays on `Node` so
   fork/sync/tear-out keep node visuals (content-addressed refs are
   sync-portable). The two plans compose; neither blocks the other.
8. **Spatial state stays on `Node`, deliberately.** `position`, `velocity`,
   `is_pinned` are the same kind of tenant in principle, but pinning and
   user arrangement are durable graph facts for a spatial graph app, and the
   numen extraction already gave physics its portable home. Recorded so the
   boundary reads as chosen, not unfinished. Revisit only if a second
   consumer needs a spatial-free kernel.
9. **Sync consequence, named.** `viewer_override` and `compat_mode` are
   user-authored and today travel with graph snapshots. After slice C they
   persist per-vault in the sidecar store and stop traveling with the graph.
   That is the intended reading (browser preference, not graph fact); a
   sidecar sync lane is a murm-side follow-on, not this plan.
10. **Timing window for the delta cut.** The five browser-state delta pairs
    ride `GraphDelta`/`CapturedDelta` today, and G5's production journal
    persistence has not landed yet, so retiring the variants breaks no
    durable log. Do slice C before prod journal persistence lands; the
    window is open now.
11. **`verso_address.rs` is another app tenant in the kernel** (verso:// and
    graphshell:// settings/tool/clip address parsing). Follow-on, not this
    pass; noted so the next boundary sweep starts there.

## Slices

Done-conditions throughout; no durations.

### Slice A: armillary cutover

repos/armillary (founded 2026-07-07, relicensed 2026-07-08) is a strict
superset of `crates/armillary` (untouched since 2026-07-03; the diff is the
license header plus doc edits, verified per-file). Make the sibling repo
canonical.

- Point the workspace dep at the sibling: git dep on
  `https://github.com/mark-ik/armillary.git` in `[workspace.dependencies]`,
  plus a local `[patch]` entry in the gitignored `.cargo/config.toml`
  (the sibylla/netfetcher/errand convention).
- Remove `crates/armillary` from workspace members and delete the directory.
- Consumers stay untouched API-wise: fetch, crawl, orrery, meerkat, and
  intel/infer's `actor` feature all consume `armillary.workspace = true`.
- **Done when:** `crates/armillary` is gone, `cargo check -p meerkat` from
  mere's own cwd is green, and no manifest references the in-tree path.

### Slice B: vates finisher (decoder + actor)

repos/vates (founded 2026-07-07) holds the provider seam and canned provider;
mere's `intel/infer` still holds the whole burn decoder (10 files) and the
armillary actor. vates's manifest already declares the target feature plan
(`decoder`, `endpoint`, `actor`).

- Move `intel/infer/src/decoder/` and `actor.rs` to vates behind `decoder` /
  `decoder-wgpu` / `actor` features, carrying the optional deps (burn,
  serde_json, safetensors, half, tokenizers; armillary for `actor`, as a git
  sibling dep). Strip MPL headers on the way (vates is MIT/Apache, ed2024).
- Reconcile `provider.rs` / `canned.rs` / `lib.rs` divergence between the
  copies (vates was ported 07-07; mere's copy may have moved since).
- Reduce `intel/infer` to mere glue over vates, mirroring intel/embed over
  sibylla: git dep + local patch, `pub use` re-exports, forwarded features
  (`actor = ["vates/actor"]` etc.). The eidetic corridor test stays mere-side
  (it exercises mere's store, not the seam).
- meerkat's `infer = { workspace = true, features = ["actor"] }` keeps
  working unchanged.
- **Done when:** vates builds and tests green standalone (default,
  `decoder`, `actor`); `intel/infer` contains no decoder/actor bodies; mere
  builds green from its own cwd.

### Slice C: browser-state sidecar (the kernel boundary cut)

The change, in one picture (illustrative only, not compile-ready):

```rust
// kernel::graph::Node keeps graph truth:
//   id, addresses, title, tags, classifications, properties, body,
//   derivations, provenance, frame hints, position/pinning (decision 8),
//   images map (decision 7), tombstoned (the durable half of lifecycle)
//
// the host owns, keyed by node id:
BrowserNodeState {
    scroll: Option<(f32, f32)>,        // was Node::session_scroll
    form_draft: Option<String>,        // was Node::session_form_draft
    mime_hint: Option<String>,         // was Node::mime_hint
    viewer_override: Option<String>,   // was Node::viewer_override
    compat_mode: bool,                 // was Node::compat_mode
    runtime: RuntimeLifecycle,         // Active | Warm | Cold (Tombstone stays kernel)
}
```

The sidecar follows the established family pattern (stemma rides beside the
graph as lineage authority; numen fields ride beside chartulary), and
session-runtime already persists exactly this kind of node-adjacent browser
state (`content_store`, `engine_profile_store`, `view_intent_store`), so C
moves fields into a lane that exists rather than inventing one.

#### C1: vocabulary + store (additive)

- `BrowserNodeState` + `RuntimeLifecycle` in session-runtime (new
  `browser_node_state.rs`, mirroring `engine_profile_store.rs`): serde
  types keyed by node `Uuid`, save/load over the eidetic store, plus the
  live-map type the host holds.
- **Done when:** a round-trip unit test over the in-memory `MemStore`
  pattern is green (save, load, absent-key default).

#### C2: kernel cut

- `Node` drops the six fields; `NodeLifecycle` reduces to the durable
  tombstone fact (shape decided at implementation: a `tombstoned` flag or a
  two-state presence enum; `Action::NodeMarkTombstone`, filters, and queries
  keep working).
- The five delta pairs retire from `GraphDelta`/`CapturedDelta`:
  `SetNodeMimeHint`, `SetNodeViewerOverride`, `SetNodeCompatMode`,
  `SetNodeFormDraft`, `SetNodeSessionScroll` and their `Replay*ById`
  mirrors (`graph/capture.rs`, `graph/apply.rs`; the two files are the cost
  center, 2,127 + 1,841 lines).
- Persistence: `PersistedNode` keeps the legacy fields as deserialize-only
  shadows (the image plan's migration shape); `from_snapshot` surfaces the
  drained values to the host as an opaque legacy payload so the kernel
  never interprets them. Saves stop writing them.
- `node_props.rs` setters for the six fields move out or become sidecar
  concerns; kernel tests updated.
- **Done when:** graph-kernel tests are green; `Node` has no browser-runtime
  field; a legacy snapshot with all six fields loads and surfaces its
  payload losslessly; a re-saved snapshot carries none of them.

#### C3: host cut-over + migration

- meerkat holds the live `BrowserNodeState` map in shared state (the
  content-cache pattern) and reads/writes it where it read/wrote node
  fields: viewer routing and compat toggle (verso lane choice), scroll and
  form-draft save/restore, gnode lifecycle transitions, inspector and card
  surfaces. `graph_delta_log` drops the retired lanes.
- Session load wires the one-time migration: legacy payload from C2 seeds
  the sidecar store, then the snapshot re-saves clean.
- session-runtime callers (`graph_engram`, `memory_levels`, snapshot merge)
  cut over.
- **Done when:** `cargo check -p meerkat` from mere's cwd is green; the
  agent-harness / scenario tests that exercise scroll restore, viewer
  override, and compat routing pass; a pre-split `graph.json` migrates
  losslessly (manual receipt logged in Progress).

## Findings

Verified against the tree, 2026-07-09:

- meerkat: 174 `.rs` files, 57,904 lines (exact).
- `Node` browser tenants confirmed at `graph/node.rs:94-167`: favicon rgba +
  dims, thumbnail png + dims (image plan's), session_scroll,
  session_form_draft, mime_hint, viewer_override, compat_mode, lifecycle.
- Browser-field references: 226 hits across 43 files workspace-wide, but
  forme's hits are its OWN display `Lifecycle` (host feeds transitions; name
  coincidence, no kernel coupling), and `capture.rs` (28) + `apply.rs` (35)
  + `node_props.rs` (22) dominate the kernel side.
- Delta plumbing exists for all five movable value fields, each as a
  `GraphDelta` + `CapturedDelta::Replay*ById` pair.
- armillary divergence: both copies 5 files; mere side last touched
  2026-07-03 (checkpoint), repos side founded 07-07 + relicensed 07-08 with
  doc edits. Sibling is a strict superset; cutover is a delete-and-repoint,
  not a merge.
- vates: `src/{lib,provider,canned}.rs` only; decoder (10 files) + actor
  remain in `intel/infer`. intel/embed-over-sibylla is the exact glue
  precedent, including the gitignored `.cargo/config.toml` local patch.
- session-runtime module list confirms the mixed-concern claim (graph
  engram/session stores + wallet/identity + frame layout/tearout + browser
  content/engine-profile/image stores + settings + scripts); its split is a
  separate follow-on, but C1's store lands beside the browser-state stores
  it will leave with.
- Prod journal persistence has not landed (G5 follow-on list), so the delta
  vocabulary can still change without a durable-log migration.

## Progress

- **2026-07-09**: Plan authored from the merecat-boundary review session
  (extraction reframed as promotions + new host architecture; orrery/platen
  correction; sidecar design; slice ordering). Facts above verified in-tree
  this session. merecat founding doc amended (target shape: orrery/platen
  move out with the app; verso family named). No code yet.
- **2026-07-09 (same session): slice A landed.** `crates/armillary` deleted
  (verified code-identical to repos/armillary modulo license header and doc
  edits; the sibling is strictly newer), workspace dep switched to the git
  sibling with a local `.cargo/config.toml` patch. `cargo check -p meerkat`
  green.
- **2026-07-09 (same session): slice B landed.** Decoder (10 files) +
  actor.rs + the tinyllama receipt test moved to vates behind `decoder` /
  `decoder-wgpu` / `actor` (MPL headers stripped; armillary as an optional
  git-sibling dep; `MERE_TINYLLAMA_DIR` renamed `VATES_TINYLLAMA_DIR`).
  vates: 52 tests green under `decoder,actor` (2 ignored real-checkpoint
  receipts). `intel/infer` reduced to glue re-exporting vates with forwarded
  features, keeping the eidetic corridor test mere-side (green through the
  glue). meerkat unchanged and green.
- **2026-07-09 (same session): slice C landed.** C1: `browser_node_state.rs`
  in session-runtime (fs JSON sidecar beside graph.json, the
  view_intent_store shape), 8 tests green including the one-time legacy
  migration off a pre-split graph.json. C2: Node dropped
  scroll/draft/viewer/compat + the whole `NodeLifecycle` enum; the four
  delta pairs retired from `GraphDelta`/`CapturedDelta`; snapshots keep
  `session_state` as a legacy read for migration and write it empty;
  facet projection no longer invents a lifecycle facet (test now pins the
  omission). Kernel: 274 tests green. C3: meerkat holds the live map in
  `shared.content.browser_nodes`; boot + session-switch load through
  `load_or_migrate_browser_node_states`; `save_session` folds live
  `view.scroll` into the sidecar (fixing the previously-dead cross-restart
  last-viewport lane: nothing ever wrote `session_scroll` in production)
  and saves scoped to the focused graph's nodes; inspector reads
  viewer/compat from the sidecar. meerkat: 248 tests green.
- **Incidental fixes forced along the way**, worth their own record:
  - **Duplicate muniment.** codicil's manifest pulls muniment from GitHub
    while everything else path-deps the sibling checkout; the slice-A/B lock
    re-resolution split the build into two muniments and broke chartulary's
    spine with cross-crate type mismatches. Fixed with a
    `[patch."…muniment.git"]` entry in the local `.cargo/config.toml`.
    Durable fix candidate: point codicil's muniment dep at the same
    convention as its consumers.
  - **wallet_grant time-bomb fixture.** `sample_pairing_ticket_request` used
    `expires_at_ms: Some(1_800_000_001)` — a seconds-scale value in a ms
    field, i.e. 1970, so both ticket-consuming tests failed as "expired"
    deterministically (pre-existing; unrelated to this pass). Bumped to the
    2100 value the neighboring fixtures use; the deliberately-expired test
    overrides to `Some(1)` itself and still passes. The sibling grant/spec
    fixtures still carry the same seconds-scale value at four sites; their
    paths do not clock-check today, but the same bomb is latent there.
  - Session-runtime: 188 tests green after the fixture fix.
