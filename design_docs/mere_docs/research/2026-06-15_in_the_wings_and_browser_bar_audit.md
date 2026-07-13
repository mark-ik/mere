# In the Wings + the Browser Bar — a Wiring-Gap Audit and Roadmap

A code-verified sweep of capabilities that are **built but not reachable from a meerkat
user action**, plus the table-stakes a browser still owes, plus the sequence that turns
the first into the second. Every claim below carries a `file:line` and a check for whether
anything in `crates/meerkat` actually calls it. Produced from a 10-agent audit (8 crate-cluster
scans + a browser table-stakes pass + a completeness critic) over the 56-crate workspace.

Companion to [edge_system_audit](2026-06-13_edge_system_audit.md) (this supersedes its stale
headline; see §3) and the per-area plans it cites.

---

## 1. The two shapes the gaps take

Almost every "in the wings" item is one of two recurring shapes. Naming them makes the rest legible.

**Duplicate substrate.** A capability got built twice: once as the rich, persisted,
federation-faithful version, and once as a simpler session-local version that actually ships.
The simple half won the live path; the rich half sits with zero callers. Five genuine cases
(a 2026-06-16 code review trimmed three of the original eight as false positives, noted below):

| Concept | Wired half (ships) | Dormant half (built, 0 callers) |
|---|---|---|
| Navigation history | meerkat linear stack `nav.rs:149` + per-node lineage `pane_data.rs:179` | edge Traversal chronology `edge_ops.rs:246` |
| Browsing memory | kernel `SharedNavigationMemory` `history.rs:119` (feeds Trail) | eidetic `BrowsingMemory`/`BrowsingTrace` `browsing/mod.rs:195` + `project_lineage` bridge `browsing/lineage.rs:51` |
| misfin identity | deterministic vault-derived `lib.rs:258` | on-disk persisted + rotate/forget `lib.rs:107-121` |
| Field lifecycle | orrery session-local `hidden_fields` `orrery/lib.rs:157` | kernel `retire_field`/`activate_field` + `retract_coupling` `field_ops.rs:26,34,67` |
| Sync lane | three byte-identical copies: `tessera/sync.rs`, `moot/sync.rs`, `mesh/sync.rs` | the deferred "one-endpoint unification" (named, unbuilt) |

The duplicate tax is real two ways: each pair is maintenance overhead, and the half that ships
is usually the *less* faithful one (session-local display state, not the persisted/federatable
record). Wiring the easy half deeper entrenches the wrong one. These are decisions, not just
chores; see §8. (Sync lane is the exception: the three copies are deliberate, "the third lap of
the proven recipe", with the unification already named for when the moot object wires into the host.)

**Reclassified after the 2026-06-16 code review** — three rows read as duplicates but are clean
layering or forward contracts, not forks:

- **Tiling** is three layers, not two competitors: forme `Arrangement` (the *intended* authority,
  geometry-free, currently unwired) → platen `Workbench` (the live tiling logic, its own `Pane`
  split-tree) → pelt `TileTree` (the render surface, downstream, "presentation vocabulary only").
  pelt did not replace anything; `Workbench::to_tile_tree` projects onto it. The only real fork is
  whether forme `Arrangement` becomes the single authority both the orrery and the workbench
  project from, deferred to window_composition P4 as its forcing function. forme's *parked*
  submodules (graphlet/lens/parity/reconciliation/pressure/topology/tree, flagged "pending
  removal" in its own lib.rs) are cleanup, not a fork — but **not a mechanical delete**: chrome's
  `frame_model.rs` still consumes the parked `tree`/`layout` types (`OwnedTreeRow` / `SplitBoundary`
  / `TabEntry`), so scope any deletion to the genuinely-dead submodules (graphlet/lens/parity/
  pressure/reconciliation) or migrate chrome off `tree`/`layout` first (2026-06-16 handoff finding).
- **Minimap**: cartography `MinimapDescriptor` is a *forward contract* in a deliberately
  contracts-only crate (its own status: "v0 ships contract types only"), "a thumbnail of **any**
  swatch". orrery `minimap_geometry` is the one concrete minimap, built direct. Not competing
  copies; reconcile only if a second minimap kind (document outline, workbench thumbnail) appears.
- **Field AST / coupling**: aether's *evaluator* is live (gyre `CouplingForce` + platen
  `coupling_paint` consume it); only its *authoring* API (`FieldProjection`/`commit_to_graph`) is
  unwired. aether is the field math, the kernel holds the field data. Clean layering, not a copy.

**Dev-example-only chains.** A whole lane (parse, index, embed, fuse, recall) is fully built
and tested, but its only driver is a standalone example bin, and meerkat does not even carry the
dependency edge. `import` + `intel/embed` + `eidetic-search` + `fusion` exist to run one 674-line
`eidetic-recall.rs`. "Shipped" at the crate level reads as "usable" only if you skip the dep-edge
check. It is not usable; there is no surface and, for five crates, no dep edge.

The practical consequence for sizing: meerkat today depends on `eidetic`, `eidetic-fjall`, `forme`,
`identity`, `linked-data`, `moothold`, `orrery`, `platen`, `scrying`. It does **not** depend on
`embed`, `eidetic-search`, `gazette`, `arrangements`, `cartography`, `aether`, `mesh`, `mooting`,
or `import`. Anything in those crates needs the dep edge added first, a real cost that separates
the S-effort sidequests from the L-effort synergy features.

---

## 2. In the wings — the inventory

Tiered by distance to a user, not by crate.

### Tier A — glue-only (the dep edge already exists; only a Command or surface is missing)

These are the cheapest real wins. No new dependency, no model change.

| Capability | Where | Effort | What's missing |
|---|---|---|---|
| **JSON-LD graph export** (ingest is wired, export is dormant) | `linked-data/src/lib.rs:56,66,192` | S | A `Command::ExportGraph` mirroring the wired ingest drain, writing `to_jsonld_string` to a file. `linked-data` is already a meerkat dep. |
| **Recover a deleted node** | `node_ops.rs:36,45` + `pane_data.rs:208` | S | Tombstones already record + list under Trail › Removed, and `focused_tombstone()` already builds a `DeletedNode` with url+title+tags "to re-mint later". Add a Recover context action feeding those back into the add-node path. Rows are currently inert. |
| **Relation-kind picker** (17-variant vocabulary, reachable only by typing `relate("cites")`) | map at `command_drain.rs:25`; context menu hardcodes `UserGrouped` at `menus.rs:193` | S–M | A submenu over the existing string→`SemanticSubKind` map. Every point-and-click edge is currently undifferentiated grouping. |
| **Tessera earned standing on the chip** | `Ledger::score`/`fold_moot` in `moothold/tessera`; meerkat syncs the log but never folds it | S | One `.ledger(default).score(persona)` read to replace the raw "tessera: 3 ops" counter. The whole point of tessera is invisible today. |
| **Show the tessera ticket in UI** | ticket only `tracing::info!`-logged at `sync.rs:133` | S | Surface the dialable ticket in the sync chip. The connect-by-ticket flow is live; a user must read logs to share it. |
| **Trail pane discoverability** | `command.rs:156`; no shellbar button (`views.rs:523`), no key (`input.rs:952`) | S | A shellbar button and/or key binding to match the other panes. Today it is palette/`>trail`-only. |
| **Steward per-row controls** | rows render at `pane_data.rs:158`; verbs exist at `node_ops.rs:115,143,169` | M | Retry/stop/pin are palette/omnibar-only; the live-operations pane has no click handler. Wire rows to the existing methods. (Touches the real-sync-feedback preference: controls exist as verbs, not buttons.) |
| **Barnes-Hut repulsion** (O(n log n) on big graphs) | `gyre/barnes_hut.rs`, exported `gyre/lib.rs:58` | S | One `sim.add_force` in `build.rs` plus tuning; the lib comment itself calls it "a tuning step". Internal payoff. |
| **Visual coupling overlays** (Halo/Tint node glow) | paint pass already runs every frame at `platen/coupling_paint.rs:71` | S | The pass is starved: the only field gesture creates a force response, never a `visual/*` coupling. A gesture/menu that adds a halo/tint coupling lights up a pass already firing. |
| **gloss a11y document outline** | `gloss/src/lib.rs:26 project_outline` | S | The gloss *domain* crate is not a meerkat dep and its a11y outline never stitches into the uxtree. Screen-reader users get no document outline. |

### Tier B — needs a dep edge plus a surface (the synergy features live here)

Each is a built lane whose payoff is large but whose wiring includes adding the dependency and a UI surface. These compose; see §7.

| Capability | Where | Effort | Note |
|---|---|---|---|
| **Browser bookmark/history import** | `import/src/lib.rs:238`, `parser.rs` (Chrome JSON + Netscape HTML) | M–L | Parser-complete; seeds lineage via `seed_linear`. No dep edge, no import command/file-picker. The single most user-legible dormant capability. |
| **Durable cross-session browsing memory** | `eidetic-core/browsing/mod.rs:195` + `project_lineage` `browsing/lineage.rs:51` | M | The bridge from live lineage to durable traces has zero callers; E5 (shell surfacing) is gated "do not start". |
| **Private full-text recall over your trail** | `eidetic-search` BM25 `index.rs:174`, reports `top_domains`/`visits_histogram`, hybrid `fusion.rs:39` | L | Downstream of browsing memory; needs both lands plus the (heavy) tantivy dep. "Where did I read about X" with no search engine. |
| **Semantic node search + canvas heatmap** | `embed` `search.rs:52`, `canvas_search.rs:41`, `field_bridge.rs:62` (BERT is a real forward pass) | L | No meerkat dep on `embed`. "Type a query, the matching region of the graph lights up." |
| **Contact rollup + handle resolution** | `gazette` `lib.rs:189` (fans `acct:user@host` into typed endpoints) | M | No dep edge, and no `Contact` type in `comms/model.rs` for resolved endpoints to land in (only the per-protocol `Identity` leaf). |
| **File send in a conversation** | `transport/blobs.rs:160`, `bind_with_blobs` `p2panda_transport.rs:238` | L | BlobStore transfer is built (sole consumer is the iroh fetcher, a different lane). murmuring has no attachment PostKind; comms `Draft` has no attachment field. |
| **Knot as a live note format** | engine registered `nematic/lib.rs:97`; eval `evaluate.rs:215`; transclude `transclude.rs:113`; exporters `export.rs:39` | M | One content-type entry point (file:// MIME sniff or a "new note" flow) unlocks eval + transclude + export at once. The most-developed unreachable area. |
| **Per-session / per-host engine choice** | `engine_activation.rs:50` (`#[allow(dead_code)]`), `routing.rs:144` (`per_host_overrides`, empty map) | M | The 3-level picker's session and host layers are built and tested; the only live toggle flips the global default. |
| **Layout / arrangement picker** | `arrangements` catalog (penrose/l-system/phyllotaxis/kanban/timeline/radial/grid/semantic); `LayoutRegistry` `registry.rs:220`; cartography `project_with` `cartography_scene.rs:118` | L | `arrangements` is a dev-dep-only in platen; meerkat has no edge to it. Needs the dep, a picker, and a gyre seed from the chosen `Projection` (`gyre::seed_positions` exists). |
| **Graphlet derive / reveal / crystallize** | `GraphletKind` shapes in `forme/graphlet.rs:72`; classifier + choreography unbuilt | L | "Select nodes, see the structure they form, crystallize it." Substrate (overrides, shapes, binding) is real; the shape classifier and canvas dim/reveal are not. `crystallize` appears in zero `.rs`. |
| **Built-but-dormant registries** | viewer `register-viewer/lib.rs:99` (~30 mime seed), knowledge `register-knowledge/lib.rs:116` (UDC tags + coloring), input `register-input/registry.rs:25` (rebindable keys), protocol `register-protocol/contract.rs:47`, mod-loader `register-mod-loader` | M–L | Finished registries no host instantiates. Viewer-per-content-type, tag autocomplete/validation, and user-customizable keybindings are each one host-owned registry from being real. |
| **Session background workers** | `SessionServiceRunner` `session_service_runner.rs:91`; `manifest.active_workers` always empty | L | Nothing constructs a runner; no `FetcherPool` worker. Background prefetch/index while you browse. |
| **Identity vault / persona switch** | `IdentityVault` `vault.rs:292` (profiles, slots, lineage, unlock tiers) | L | meerkat uses only the keypair for signing; the whole vault is dormant. Multi-identity / per-site credentials / persona separation. |
| **Capability-gate enforcement** | `kernel::permissions::resolve_permission` `permissions.rs:137`; `SessionPolicy.overrides` empty | L | No action bus calls the gate; cross-session attach, engine override, clip capture are catalogued, none enforced. |
| **Moot object (declare/join/share)** | `moothold::moot` `sync.rs:72`, `roster.rs:65` | L | meerkat imports only `moothold::tessera`; nothing builds a `SyncedMootSpace` or renders a roster. Two-machine run still pending. |
| **Compute mesh** | `mesh` `sync.rs:94`, `board.rs:59` | L | M1 scopes meerkat wiring out by design; job kinds are Echo/Blake3 until a real `MeshResource` (M2). |

### Tier C — designed, not built (flag honestly; not "in the wings")

The audit caught several items that read as substrate but are doc-only. Calling them "built-but-unwired" would mis-locate the work; they need building, not wiring.

- **Constitution primitive + amendment rule** — zero code (`grep ConstitutionEvent/AmendmentRule/...` returns nothing); the brief is an explicit design probe.
- **Contact struct, hashcash (misfin code-64), murm ephemeral burn/TTL** — the contact-identity brief says "No code proposed"; none exists.
- **`misfind` daemon** — there is a client and an in-process server (`comms_host.rs:352 start_misfin_server` on :1958), but no standalone daemon binary.
- **Alembic memory pane + Athanor distillation** — doc-only; zero code. (The audit brief conflated this with `armillary`, which is the actor-kernel and *is* wired. Different thing.)
- **`mooting` adapters** (Matrix/Nostr/IRC/ATproto) — a 28-line name-reservation stub.
- **`wry.web` engine** — a routing constant plus two UI labels with no backend; `engine_present` can never return true for it. Either build it or drop the dead vocabulary.
- **murm co-op sessions** — only the ALPN constant `mere/coop/v1` is reserved; `host_coop`/`join_coop` do not exist.

---

## 3. Already shipped (docs understate — do not re-plan these)

Several plan headlines are stale; the code shipped past them. Listed so a top-down reader does not put done work on the roadmap.

- **Edge create / retract / traverse between two nodes** is fully wired (`command_drain.rs:97-112`, context menu, `>relate`/`>unrelate`, roster edge-row click). The edge_system_audit's opening line ("no way to draw an edge") is contradicted by its own status footnote and the live code. Only the kind picker (Tier A) remains.
- **Two graphs side by side** is reachable: Shift+click a switcher tile pushes `OpenGraphBeside` (`input.rs:345` → `session_ops.rs:470`), each Orrery leaf renders from its own pooled orrery. This delivers what `multi_graph` MG6 ("far-B") still lists as remaining. The remaining piece is per-pane pointer-select/nav.
- **Omnibar command shell** (`>verb`) is live (`shell_eval.rs:111`), with completion and the `>` sigil; the omnibar plan is framed "planned" but the privileged lane ships. (The *sandboxed knot-note* lane is the unwired one.)
- **Theme switching** is fully wired including orrery palette threading; only per-theme node fills remain. The "A2 pending" note is stale.
- **comms core** (misfin send/receive, networked murm cabal, share/join invite) is live end-to-end.
- **Engine routing through `EngineRoutePolicy`** is wired (`node_ops.rs:290`, `card.rs:258`); the picker plan §2 narrative ("routes nothing through the policy") is contradicted by its own Progress log.

The lesson, which matches the standing rule to verify against code: trust the code over the doc headline. Four plans had headlines that would have mis-prioritized this roadmap.

---

## 4. The browser bar — gaps to table-stakes

From the dedicated browser pass, ordered by severity. "Severity" is "how much it undercuts the claim to be a browser," not effort.

### Blocker

- **Large / tall page rendering.** Confirmed as a hard size gate, not a transient state. A big gemtext page fetches (`smolweb ok`) and is stored `Ready`, but `bound_document_body` truncates the body to 12 KB at a line boundary *before* layout (`card.rs:296`), and `render.rs:766-770` then clamps the texture to 8192 px / 30 MB area. The scene viewport is still built at the *full* `content_height` (`card.rs:159`), so a scene authored for ~19000 px rasterized into an 8192 px texture is the "laid out to ~19000px and rendered nothing" failure the in-code comments document. The 12 KB cap is a partial mitigation; gemtext with many short lines defeats the ~1.7 bytes/px assumption. This is the reported "no fetched media yet" bug. Root cause is foundational; see §5.

### Major

| Gap | State | Where |
|---|---|---|
| **Find-in-page** | absent | no Ctrl+F, no retained searchable text model (page is glyph runs in a GPU texture) |
| **Downloads / save-as** | absent | `fetch.rs` decodes for render only; no write-to-user-file path |
| **Text selection + copy from page** | absent | clipboard copy acts only on omnibar/palette buffers `input.rs:1185`; no selectable text layer |
| **Reload / stop** | partial | "Retry" re-fetches focused node (no F5, no Esc=stop); "Stop" reaps the actor but the network fetch keeps running (`fetch.rs` has no cancel) |
| **Error pages** | partial | one-line card message; no styled page, no retry-on-error, no cert UI; gemini status 10 (input) is a dead-end string |
| **Interactive gemini input (status 1x)** | absent | `fetch.rs:206` returns an error instead of prompting; breaks gemini search/forms |
| **TLS / security indicator** | absent | no lock/scheme styling; HTTP uses `permissive()`; gemini client-cert (TOFU) flow unimplemented |
| **Bookmarks** | absent | no add/list/store; dovetails with import (§7) |
| **Settings breadth** | partial | the Settings overlay exposes exactly one control (tab cap); no homepage, search-engine choice (DuckDuckGo hardcoded), zoom, download location |
| **Zoom / text scaling** | absent | no Ctrl+±/0; font sizes are fixed constants |
| **HTML-lane scroll + links** | partial | the genet/HTML lane reports `content_height == viewport` and empty link rects (`card.rs:337`), so web pages clip to one screen and their links are inert |
| **Content-type beyond gemtext** | partial | good text coverage; a top-level `image/*` falls to a synthesized card dumping the binary as lossy UTF-8; no video/audio/PDF, no unknown→download |
| **Link affordances** | partial | document-lane links navigate, but no hover/status URL, no cursor change, no right-click "copy link / open in new" |

### Minor

Back/forward (works but is per-node-surface, not one session timeline), redirects (gemini followed/capped; http delegated, never surfaced), tabs (the orrery graph *is* the tab model; no linear strip), history UI (Trail rows are inert text, not clickable; gloss Recent is clickable but only to existing nodes), address-bar suggestions (session-memory only, no frecency, URL-substring only), page title/favicon (titles parsed but not shown as the surface title; no favicon anywhere), keyboard coverage (rich pane shortcuts; missing Ctrl+L/F, F5, Ctrl+±, Alt+←/→), progress feedback (a "Fetching…" card, no throbber/byte-count, a 30 s hang looks stuck with no cancel).

---

## 5. The foundational pitfall: pages are baked into a texture

Four separate table-stakes trace to one architectural decision: **rendered page content is a
single full-height GPU texture with no retained, queryable text model.** That single fact is why:

- tall pages cannot render past one texture (the blocker bug),
- there is no find-in-page (nothing to search; the text is glyph runs),
- there is no text selection or copy (no caret, no selectable layer),
- and document scroll is capped at the texture height rather than the real page height.

The clamps added so far (`bound_document_body` in bytes, `MAX_CARD_TEX_H` in px) are two
independent size gates for the same failure, with inconsistent units and an unstated
~1.7 bytes/px coupling. They do not enforce the real invariant (scene height ≤ texture height);
the scene viewport is still authored at full `content_height`.

The decision behind the roadmap is whether to keep patching the gates or to do the foundational
fix: **clamp the scene viewport to the texture window and tile/virtualize tall content, backed by
a retained laid-out-text model the host can query.** That one change is the unlock for the blocker
plus find-in-page plus selection plus true scroll. It is L-effort but it pays for four table-stakes
at once, and it is the single highest-leverage item on the board. (The HTML/genet lane needs its
own version: report full laid-out height and harvest `<a href>` rects, which also fixes dead web-page
links.)

---

## 6. Other pitfalls

- **Duplicate-substrate entrenchment.** Each of the eight pairs in §1 invites wiring the
  already-shipped (simpler, less faithful) half deeper. Doing so makes the federation-faithful
  half harder to ever adopt. Prefer resolving the pair (§8) before building more on top of the
  session-local copy.
- **"Shipped" ≠ "reachable."** The embeddings stack, the recall stack, and import are all
  "landed" at crate level and tested, yet none is reachable and five crates lack the dep edge.
  Sizing must charge the dep edge plus a surface, not just the call.
- **Manifest fields are inert and their round-trip is unverified.** `active_workers`,
  `consolidated_engrams`, `engine_profile`, `policy.overrides`, `parent_session`, `persona_id`
  are all defaulted and never read. Before any manifest-touching feature, confirm the field
  actually serializes and restores across a session reload (flagged unverified by the critic).
- **Dead vocabularies mislead greps and menus.** `ActionId::EdgeConnect*` (resolve to nothing),
  `ENGINE_WRY_WEB` (no backend), `PaneContent::System`/`Tile`/`Custom` (never constructed in
  production). They read as features. Decide build-or-delete.
- **Stale doc headlines** (§3) mis-prioritize anyone reading top-down. Worth a doc-hygiene pass
  to reconcile the four with their own progress logs.

---

## 7. Synergies — features that fall out of pieces already built

These are the spine of the roadmap. Each is mostly-built parts plus thin glue, landing on a surface that already exists.

1. **One history/recall feature from four built pieces.** import (`parse_bookmark_items`) seeds
   lineage via `seed_linear`; the kernel's live `recent_visited` + per-node lineage already render
   in the Trail pane; eidetic tombstones already render under Trail › Removed; `project_lineage`
   converts live lineage into durable `BrowsingTrace`; `eidetic-search` (BM25) + embed (BERT) +
   `fusion` give recall over it. Wire import→seed, lineage→`project_lineage`→`save_trace`, and an
   index over the trace, and "bring your history, then search everything you've visited across
   sessions" composes. The Trail pane is the single surface they all feed.

2. **Contact rollup from three built pieces plus a 5-field struct.** gazette already fans a handle
   into typed misfin/gemini/gopher/ActivityPub/http endpoints; comms already sends/receives misfin
   and runs the networked cabal; misfin already has the rotate/forget lifecycle. The only missing
   glue is a `Contact` landing-zone type in `comms/model.rs` plus an "add contact by handle" route.
   "Paste a handle, get a messageable + navigable contact with rotate/forget controls."

3. **Semantic search lights the canvas, onto a paint pass already firing.** embed's
   `CanvasSearchSurface` + `register_query_similarity_field` builds a similarity field; aether's
   eval feeds gyre's `CouplingForce`; platen's `visual_overlays` (Halo/Tint) **already runs every
   frame** but is starved because no gesture creates a `visual/*` coupling. Route a query →
   similarity field → visual coupling and the glow lands on a live pass.

4. **Knot as a live note format unlocks four passes from one wire.** The knot engine is in the
   actor registry; it only needs a content-type entry point (file:// sniff or "new note"). The
   moment knot content is reachable, the eval pass (with the built sandboxed `RhaiEvaluator`), the
   transclude pass, and the exporters all attach to the same render path. One wire turns the
   most-developed unreachable area into Mere's native scriptable note format.

5. **Tessera earned standing from one fold call.** meerkat already joins, seeds, and persists the
   tessera log; the whole ledger/gate/concord stack is built and tested. One
   `.ledger(default).score(persona)` turns the raw op counter into a real standing.

---

## 8. Contradictions — decisions (worked through with Mark, 2026-06-16)

The §1 forks, mostly settled in the 2026-06-16 walk-through:

- **History source of truth — DECIDED.** Per-node lineage is the spine (it already holds within-node
  history *and* the cross-node spawn genealogy in one tree). The edge Traversal family is the
  cross-node chronology layer to *write* from the nav path, not a third truth. The linear `nav.rs`
  stack is node-blind (a flat URL `Vec` feeding omnibar suggestions) and retires into a
  lineage+traversal-derived trail; it can still back the shellbar back/forward buttons as a
  projection, but is not a source of truth.
- **Browsing memory — DECIDED: one corpus.** `project_lineage` becomes the live bridge (kernel
  lineage = working memory, eidetic `BrowsingTrace` = its durable tier); no second visited-set.
- **Field lifecycle — DECIDED: add the delete.** Give placed fields/couplings a real retract
  (`retract_coupling` + field removal); route hide/show through the kernel lifecycle so a hide is a
  federatable retire, not session-local display state.
- **Sync-lane unification — DECIDED: keep the copies, schedule the merge.** The three lanes are
  deliberate; fold the named "one-endpoint unification" into the moot-wiring plan and do it when the
  moot object wires into the host, not before.
- **Tiling — REFRAMED, not a two-way fork** (see §1). Three layers: forme `Arrangement` (intended
  authority, unwired) → platen `Workbench` (live tiling) → pelt `TileTree` (render surface). Action:
  forme submodule cleanup is **not mechanical** (chrome's `frame_model.rs` consumes the parked
  `tree`/`layout` types — `OwnedTreeRow`/`SplitBoundary`/`TabEntry`); scope deletion to the
  genuinely-dead submodules (graphlet/lens/parity/pressure/reconciliation) or migrate chrome first,
  and keep the `CollapsedGraphlet`/derived-arrangement hook. Defer the "forme as single authority"
  decision to window_composition P4 as its forcing function. Workbench→pelt stays the live path.
- **Dead vocabularies — `wry.web` DELETED** (commit 51504fc, 2026-06-16). The rest are kept as
  potentially-useful: `PaneContent::Tile` is the pinned-tile / split-view pane worth *building*,
  `Custom` is the extension seam, `System` a possible distinct diagnostics pane;
  `ActionId::EdgeConnect*` stays as vocabulary.

---

## 9. Proposed roadmap

Five lanes. Lane 0 is a momentum sprint; Lane 2 is the highest-leverage foundation; Lane 3 is
menu-choosable (pick the demo you want). Lanes are roughly sequential but 0 and 1 can interleave.

**Lane 0 — Sidequest sprint (all Tier A, deps already present).** JSON-LD export · recover deleted
node · relation-kind picker · tessera score + ticket display · Trail shellbar button + key ·
Barnes-Hut · Steward per-row controls · visual coupling overlay · gloss a11y outline. Each is S
(a couple are S–M). Real user-visible wins, zero architecture, no dep churn. Done condition: each
is reachable from a normal affordance, not just the palette.

**Lane 1 — Browser-bar majors that do not need the rewrite.** Reload/stop with F5 + Esc and a real
fetch-cancel · error pages incl. the gemini input (1x) prompt (this is genuine smolweb breakage,
not polish) · TLS posture + a security indicator + the gemini client-cert/TOFU flow · bookmarks
(folds into Lane 3's import) · settings breadth (homepage, search engine, zoom default) · top-level
`image/*` viewer + unknown→download · link hover/status + right-click menu · make Trail rows
clickable-to-navigate. Done condition: the browser table-stakes list in §4 loses its "absent"
majors that do not depend on §5.

**Lane 2 — The retained-text / tiled-render foundation (the §5 fix).** Clamp the scene viewport to
the texture window; tile or virtualize tall content; introduce a retained laid-out-text model the
host can query; give the HTML/genet lane true height + link rects. This clears the **blocker** and
unlocks **find-in-page**, **text selection/copy**, **true scroll**, and **live web-page links** in
one foundation. Highest leverage on the board. Done condition: a 166 KB capsule renders and scrolls
fully; Ctrl+F highlights; a paragraph is selectable and copyable; a link on a fetched HTML page
navigates.

**Lane 3 — Synergy features (Tier B; each adds a dep edge + a surface).** Pick by what you want to
demo. The history/recall stack (import → lineage → durable trace → search) is the most
user-legible and reuses the Trail surface. Contact rollup, semantic-canvas, and knot-as-live-doc
are each a self-contained composition from §7. Done condition: one synergy lands end-to-end on an
existing surface before starting the next.

**Lane 4 — Resolve the duplicates (§8 decisions).** Pick the history spine, the browsing-memory
corpus, the field lifecycle path, the sync-lane unification, the tiling authority; build-or-delete
the dead vocabularies. This stops the duplicate tax from compounding and makes federation
faithful. Best done as decisions land, not as one big refactor. Done condition: each fork in §8 has
a recorded decision and the losing half is either subordinated or deleted.

**Recommended first move:** Lane 0, starting with JSON-LD export (cheapest, closes the one-way-bridge
asymmetry) and recover-deleted-node (the data model was shaped for it; the rows are already there).
Then the relation-kind picker, which turns the already-shipped edge system into the thing that makes
a graph browser worth using.

---

## 10. Open evidence gaps (all five closed 2026-06-16)

The critic flagged where the audit's confidence was thinner. All five are now verified against code:

- **Manifest field serialization round-trip — VERIFIED GREEN.** `manifest.rs` has a real round-trip
  test (`manifest_round_trips_through_serde_json`) plus a legacy-shape test; every late field is
  `#[serde(default)]` and the schema is versioned (`MANIFEST_SCHEMA_VERSION`). The reserved fields
  serialize and restore safely; the only gap is that the host never *writes* most of them. Safe to
  build on.
- **probes/knot-lua — VERIFIED: a Lua eval lane parallel to rhai.** `probes/knot-lua` runs
  `lua eval` knot fences end to end via genet's **piccolo** backend (`script-engine-piccolo` over
  `script-engine-api`), driven through the *same* `inker::evaluate_blocks` seam the rhai note lane
  uses. So note-scripting is two backends (rhai + lua/piccolo) over one `BlockEvaluator` seam, both
  unwired in the shell; lua is further out (quarantined as an excluded probe so piccolo/gc-arena
  stays out of the workspace until the knot eval pass lands). Not a duplicate to resolve; a
  deliberate two-backend design that folds into synergy #4 (knot-as-live-doc).
- **Fetch actor protocol reach — VERIFIED: http/https + 7 smolweb schemes.** `fetch.rs` routes by
  scheme: http(s) through netfetcher (WHATWG Fetch, `FetchContext::permissive()`), and
  gemini / gopher / finger / spartan / nex / guppy / titan through `errand`. Smolweb redirects
  follow up to 5; http 30x is followed inside netfetcher (not surfaced to the address bar). The
  reach is a real strength; the gaps are the *interactive/security* ones the browser-bar section
  already named (gemini input 1x is a dead-end, TLS posture is permissive/unsurfaced, no cert/TOFU
  UI) plus `file://` (engine_picker Phase 4). Not missing protocols.

- **Settings persistence — VERIFIED: every knob survives restart.** All five `PersistedSettings`
  fields (tab_cap, theme_id, shellbar_edge, physics_damping, disabled_engines) are serde-default,
  written atomically, and applied on launch (main.rs: tab_cap 810/886, disabled_engines 889/1038,
  physics_damping 1025/1057, shellbar_edge 1056, theme_id 919-923). The engine `set_global` worry is
  resolved: `disabled_engines` persists and restores. No gap here.
- **Session restore — VERIFIED PARTIAL.** The last-active session *does* restore on launch
  (`bootstrap_sessions` opens the most-recently-updated manifest; the window-scoped frame layout
  persists at the mere root and restores, MG5). But only that one session reloads (the others stay
  cold in the switcher, not reopened); there is **no reopen-closed** (close → `move_to_trash`,
  disk-recoverable only, no UI); and secondary / torn-out windows are not persisted or restored.
  Crash recovery is per-tile only (`recovering_card_scene`); sessions persist on switch/exit. So
  "restore on launch" holds for the active graph, but the browser-style "reopen everything I had
  open / reopen a closed tab / restore my windows" is **absent** — a real table-stake gap, matching
  §4's minor note.

---

*Method: 8 crate-cluster scans + a browser table-stakes pass + a completeness critic, each
evidence-bound to `file:line` with a check for a `crates/meerkat` caller. Grounded against the live
tree, not the docs; where a doc claim and the code disagreed, the code was taken as truth (§3).*
