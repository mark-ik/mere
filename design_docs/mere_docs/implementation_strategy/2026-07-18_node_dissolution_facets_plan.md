# Node Dissolution + Facets Plan

**Founded:** 2026-07-18, executing the
[one-node ruling](../technical_architecture/2026-07-18_one_node_facets_layer_map.md)
(Mark, 2026-07-18). Three lanes: **F** (the facet store), **S** (spatial
completion), **D** (the kernel `Node` dissolution ladder). Lanes are
independently workable; D depends on F0 for its facet destinations and on the
D-gate measurement for its hot fields.
**Companions:** the [image externalization plan](2026-07-06_node_image_externalization_plan.md)
(D0 executes its phase 2; its phase 1 store is built), the
[boundary pass plan](2026-07-09_mere_turnstone_boundary_pass_plan.md) (slice C
invented the sidecar pattern), the
[participant gate + packs plan](2026-07-17_participant_gate_packs_plan.md)
(packs ship custom content classes; facet grants join its scope vocabulary),
and the north star as amended.
**Status:** complete 2026-07-27. Lane F, Lane S, and D0-D4 are closed; F2
remains deliberately trigger-gated in the participant plan.

**2026-07-22 boundary amendment:** S0's extracted numen/quint/seiche stack has
since consolidated into `repos/conatus`. The trigger-gated wholesale canvas
promotion is replaced by the
[Graphshell remote projection host plan](2026-07-22_graphshell_remote_projection_host_plan.md):
arrangements migrate to `scenomise`, kernel-aware canvas stays in Mere, and
shared interaction moves through Cambium/Sprigging under the Woodshed consumer.

## The decision in one line

`chartulary::Container` is the one node; every optional metadatum becomes an
atomic **facet** (typed record keyed by node id + facet id, schema-validated);
mere's kernel `Node` dissolves facet-by-facet until its remainder is Container,
and "web page" becomes turnstone's content class defined with the same machinery
a modder would use.

## Lane F — the facet store (LANDED 2026-07-18, chartulary `0051d7c`)

Home decided with code in hand: **chartulary-generic** (the lean held). The
facet store and the content-class model are node-metadata / node-typing, so
they sit with the node in chartulary; the eidetic `SchemaDefinition` validator
is the **mere-side adapter** that injects into the seam (thin follow-on, built
when a real class wires into mere). F1's earlier "mere-side" filing meant the
*validator wiring*, not the model.

- **F0, the facet store — DONE.** `chartulary::facet`: a facet is a typed record
  keyed by node id + `FacetId`, payload an opaque `serde_json::Value`;
  `FacetStore<Id>` holds them and persists as one muniment slot beside the
  graph. Schema-agnostic via the `FacetValidator` seam (only `AcceptAll` ships;
  the host supplies the real validator). Receipts: a facet round-trips
  persist/replay beside the graph, the validator seam refuses an invalid facet
  and stores nothing, an unknown facet id survives a round-trip untouched, an
  absent slot loads empty. 5 tests.
- **F1, content classes as data — DONE.** `chartulary::content_class`: a
  `ContentClass` (class id + required facets + each facet's schema ref) is data;
  `admits()` checks presence + validity through the same seam; a `ClassRegistry`
  resolves a node's declared class (the reserved `chartulary.class` facet) and
  reports `Unknown` (inert) for a class this build has no definition for.
  Receipts: a class admits a valid member and rejects a missing/invalid facet,
  a node's class is queryable, an unknown class is inert not an error, a class
  round-trips as data. 4 tests.
- **F2, facet grants** (pointer, not built here). Which facets a denizen may
  read/write joins the structural cap's scope vocabulary; owned by the
  participant plan's gate lane. Trigger: the first mod-defined facet that a
  denizen petitions to write.
- **F-follow, the eidetic validator adapter — DONE 2026-07-27.**
  `session-runtime::SchemaFacetValidator` preloads eidetic
  `SchemaDefinition`s behind chartulary's synchronous `FacetValidator` seam.
  Registered facet schemas and `ContentClass` definitions persist and reload as
  content-addressed typed engrams; unknown facets remain forward-compatible.

## Facet convergence of the per-node sidecars (found 2026-07-18, Mark)

Grounding Lane S surfaced that the position sidecar wants to be a facet, and
that generalizes: **every bespoke per-node sidecar mere holds is a facet avant
la lettre.** `browser_node_state` (`web.*`), `denizen_bindings` (`denizen.*`),
the per-node bits of `view_intent`, and cartography's per-node arrangement data
(position, size, sprite, sprite-hull, material, face → `arrangement.*`) are all
typed metadata keyed by node id, which is the facet store's definition. The
facet work unlocks one mechanism, many namespaces, replacing N hand-rolled JSON
sidecar files.

Boundaries that hold the convergence honest:

- **Graph-scoped canvas flags are not *per-node* facets.** `size_by_degree`,
  `size_by_importance`, `importance_metric` are scene-scoped, not per-node.
  (2026-07-19 refinement: they are **container** facets — `scene.*` on the
  root graph's id — not homeless view settings; see the scene.* progress
  entry.)
- **Live vs durable.** The *live* position lives in seiche (runtime, never
  persisted, not a facet); only the *durable* save-time position becomes an
  `arrangement.position` facet. So the facet holds cold data and seiche keeps
  the hot loop — the bulk-numeric perf worry does not arise.
- **Stored vs derived facets.** mere already uses "facet" for the PMEST
  projection (`facet_projection.rs`) that *derives* facets (in-degree, domain)
  from node data. chartulary facets *store* them. Both are real; the facet
  surface should admit stored and derived through one lens. (Term collision
  noted; keep the two senses distinct in prose.)
- **No big-bang.** Working sidecars migrate opportunistically or as explicit
  rungs. `denizen_bindings` and `browser_node_state` are transitional-bespoke
  (built to the pattern that existed pre-facet-store) and converge later.

**The enabling rung: wire `chartulary::FacetStore` into mere's session
persistence — LANDED 2026-07-18** (mere `b85aeea`). `session-runtime::facet_store`
holds a `NodeFacetStore = chartulary::FacetStore<Uuid>` and persists it to
`facets.json` beside the session graph (atomic write, `Ok(None)` absent, the
browser_node_state pattern), re-exporting chartulary's facet types; chartulary
added as a session-runtime dep. Schema-agnostic persistence; validation is a
write-time host concern (eidetic-backed later). 4 tests, clippy clean, 193
existing session-runtime tests unaffected; `FacetStore<Uuid>` round-trips through
JSON. This is the durable home the bespoke sidecars converge onto (`web.*`,
`denizen.*`, `arrangement.*`); the migrations follow behind it, and it is S2's
real destination and D2/D3's shared store.

## Lane S — spatial completion

Sequence (Mark, 2026-07-18): **S0 first**, then S1 + review, then the careful S2.

- **S0, extract quint + seiche** to sibling repos + MPL→MIT/Apache relicense
  (the numen-treatment follow-on). **The wrinkle**: both carry an optional
  `kernel-bridge` feature depending on mere's `kernel` crate, so a naive
  extraction makes `mere → seiche → mere/kernel` a cycle. So S0 first **severs
  kernel-bridge in-workspace** (the mere-`Graph` glue — quint's
  `commit_to_graph`, seiche's `sync_with_graph`/`write_positions_to`/
  `CouplingForce::from_coupling` — moves to a mere-side adapter that calls the
  kernel-free API), verifies mere still builds, *then* lifts the now-kernel-free
  crates out. Done when: `repos/quint` and `repos/seiche` are green standalone
  (MIT/Apache, publishable), mere consumes them as git siblings (local checkout
  via the gitignored `.cargo` patch), and the mere canvas suite passes.
- **S1, positions through the sidecar** (then review). Audit every
  `Node.position` / `Node.velocity` read/write; route durable reads through the
  arrangement-facet path (the convergence above; the cartography sidecar is the
  transitional store until the facet store is wired) and live simulation through
  seiche. Done when: kernel + canvas tests pass with the fields untouched by
  anything but a shim, **and the audit is reviewed** before S2.
- **S2, retire the fields** (careful pass). Remove `position`/`velocity` from
  `Node`; the durable destination is the `arrangement.position` facet (via the
  wired facet store), the cartography sidecar being the transitional bridge.
  rkyv snapshot migration: legacy snapshots load, positions absorb one-time (the
  browser_node_state migration is the pattern). Done when: the fields are gone,
  legacy sessions load with arrangements intact, full mere suite green.

## Lane D — the dissolution ladder

**D-gate (before any hot field moves): the rkyv measurement.** Benchmark
snapshot load + hot graph ops for the current rkyv `Node` vs Container +
facet-sidecar equivalents, on a representative large session. If Container +
facets cannot match within an acceptable envelope, the web node persists as a
**physical representation** of a Container profile (a conceptual, not
structural, dissolution) and the ladder's remaining rungs move only cold
fields. A measurement, not a principle. Done when: numbers exist in this
plan's Findings and the verdict is recorded.

The field-by-field map (from the 2026-07-18 read of `graph/node.rs`):

| `Node` field | Destination | Rung |
| --- | --- | --- |
| `id` | Container identity (Uuid) | stays (is Container) |
| `title`, `tags` | Container `title`/`tags` | stays (is Container) |
| `mime_hint` | Container `media_type` | D1 (direct home exists) |
| `addresses` (`AddressClaim` roles) | Container `Addressed` (role mapping decided at move) | D1 |
| `body` (inline djot) | Container body; inline-vs-blob decided at move (body is a content-addressed blob hash; small inline bodies may become blobs, or Container gains an inline capability) | D1 |
| `thumbnail_png`/`_width`/`_height` | `ImageRef` via image_store (image plan phase 2) | **D0** |
| `favicon_rgba`/`_width`/`_height` | `ImageRef` via image_store (image plan phase 2) | **D0** |
| `cached_host` | derived; recompute at read or a web-class cache facet | D1 |
| `position`, `velocity` | Lane S (geometry sidecar + seiche) | S1/S2 |
| `is_pinned` | arrangement facet | D2 |
| `frame_layout_hints`, `frame_split_offer_suppressed` | arrangement facets | D2 |
| `tag_presentation` | presentation facet | D2 |
| `last_visited`, `last_session_visited` | visit-history facet (memory-levels reads it) | D2 |
| `import_provenance` | provenance facet | D3 |
| `classifications` | semantic facet (scholia ring) | D3 |
| `properties` (open literals) | semantic facet (scholia lane) | D3 |
| `derivations` | chartulary `DerivationRecord` (spine derivations exist) | D3 |

- **D0, images out — DONE 2026-07-27** (executes the image plan's phase 2, which is already
  designed: `images: BTreeMap<ImageRole, ImageRef>`, migration included).
  Done when: no pixels ride `Node` or the graph snapshot; sessions migrate
  one-time; switcher/preview surfaces render unchanged.
- **D1, the web content class founded — DONE 2026-07-27** (turnstone-side definition, mere-side
  storage): mime/addresses/body land in their Container homes; cached_host
  and viewer-adjacent metadata become the **web-page facet bundle**, the
  first content class defined as F1 data. The dogfood rung: turnstone defines
  it exactly as a modder would. Done when: turnstone browses normally with the
  class in force and a second toy class (a note class) coexists in one graph.
- **D2, arrangement + presentation + visit facets — DONE 2026-07-27.** Done when: the fields
  leave `Node`, their consumers read facets, sessions migrate.
- **D3, semantic facets — DONE 2026-07-27.** Done when: provenance/classification/property/
  derivation data live in facets or chartulary-native records, and the RDF
  projection reads them there.
- **D4, the remainder audit — DONE 2026-07-27.** Done when: what remains of `Node` is
  Container's surface (or the D-gate verdict's physical-representation
  residue, explicitly listed), and the north star's OQ 5.1 is marked closed.

## Trigger-gated tail (not scheduled here)

- **Projection/canvas split:** generic arrangements migrate to `scenomise`;
  the shared Cambium/Sprigging graph view waits for the Woodshed consumer;
  Mere's graph adapter and kernel-aware canvas remain Mere-side.
- **The pool surface** (personae-indexed vaults, cross-vault queries, p2p
  share): the north star's step 3; its own plan when it fires.
- **Turnstone palette/install slice**: owned by the participant plan; proceeds
  in parallel, unblocked by all of the above.

## Findings

### D-gate: the rkyv measurement (2026-07-26) — VERDICT: structural dissolution passes

Run with `cargo run --release -p mere-kernel --example dissolution_gate`
(`crates/graph/graph-kernel/examples/dissolution_gate.rs`), 50,000 nodes, the
scale the node-image plan measured at. Best of three, release, one machine.

| arm | MiB | load ms | hot ms |
| --- | --- | --- | --- |
| A — fat node, inline images, rkyv (today) | 302.6 | 185.8 | 0.78 |
| A' — fat node, image refs, rkyv (post-D0) | 16.8 | 48.3 | 1.39 |
| B — container + facet sidecar, image refs, rkyv | 27.3 | 81.5 | 1.11 |
| C — container + facet sidecar, JSON facets | 25.7 | 134.6 | 1.48 |

**The plan's wording needed sharpening before it could be measured.** "Container
+ facet-sidecar equivalents" compares two variables at once, because today's
`PersistedNode` is rkyv while `Container` is serde-only and `NodeFacets` holds
`serde_json::Value`. Measured naively the dissolved shape loses on *codec
choice* and says nothing about dissolution. The arms above hold the codec
constant (A' vs B) and price the codec separately (B vs C).

**The structural question (B vs A'): 1.63x size, 1.69x load, 0.80x hot.**
Dissolution genuinely costs something on load, and *saves* on hot ops, because
a tag scan over thin containers touches less memory than over fat nodes. Hot
ops run constantly and load runs once, so the trade leans further toward
dissolution than the load ratio alone suggests.

**The ratio is the wrong frame, though, because D0 dominates everything.**
Removing inline images is an 18x size cut and a 3.8x load cut on its own. Post
D0 *and* dissolved, load is 81.5 ms against today's 185.8 ms: the combined path
is **2.3x faster than the status quo** even though dissolution taxes the
post-D0 baseline. There is no envelope in which D0 lands and dissolution then
makes things worse than today.

**Verdict: proceed with structural dissolution.** The physical-representation
fallback is not needed. In absolute terms the dissolution tax is 33 ms on a
50k-node session load, which is not user-visible, and it buys the class system.

**Secondary finding: JSON facets cost ~1.65x load over closed-typed binary
facets** (C vs B) at roughly equal size. Not gating, since C still loads faster
than today.

**Follow-up (2026-07-26, arms D-G): the codec race.** The candidate codec for
an *open* facet map has to be self-describing (unknown-forward foreign facets
rule out postcard and rkyv). The first attempt (ciborium) lost to JSON, which
raised the right suspicion: bad codec, or hopeless idea? The race answers it —
five codecs deserializing the **identical** payload (containers +
`serde_json::Value` facet maps), so the only variable is codec engineering.
Same run, 50k nodes:

| codec | MiB | load ms | vs JSON load |
| --- | --- | --- | --- |
| serde_json | 25.7 | 139.5 | 1.00x |
| ciborium (CBOR) | 23.3 | 180.2 | 1.29x |
| cbor4ii (CBOR) | 23.3 | 123.5 | 0.89x |
| minicbor-serde (CBOR) | 23.3 | 122.7 | 0.88x |
| rmp-serde (MessagePack) | 23.3 | 115.7 | 0.83x |
| *(closed-typed rkyv, arm B)* | *27.3* | *73.3* | *0.53x* |

Three conclusions, in order of importance:

1. **Ciborium was indeed letting CBOR down**: cbor4ii and minicbor decode the
   same CBOR bytes ~1.46x faster than ciborium and beat serde_json by ~11%.
   If the sidecar ever flips to binary, the picks are rmp-serde or cbor4ii.
2. **But the ceiling is structural.** The best self-describing codec is still
   **1.58x slower than closed-typed rkyv** into thin rows. Codec engineering
   recovered barely half the JSON-vs-closed gap; the rest is the per-node
   `Value`/`BTreeMap` allocation, which no parser can remove. A 17% load win
   does not justify a format migration, so `facets.json` stays JSON until the
   sidecar measurably hurts.
3. **An own codec is not the alternative worth building.** The headroom above
   rmp-serde for a heroically optimized parser into the same open map is
   bounded by that allocation floor, well short of B. If the family ever
   builds something here, the superior alternative is architectural, not a
   parser: **lazy per-node raw-byte rows** (facet bytes decoded on first
   access), which turns sidecar load into a near-memcpy and charges `Value`
   cost only for nodes actually touched. That is a shape change with a real
   design cost (two representations, invalidation), recorded here so it is
   reached for deliberately, not speculatively.

Run-to-run noise caveat: cross-run load numbers drift ±15-30% on this machine
(concurrent builds), so only same-run ratios are quoted; the run-1 table above
stands as the order-of-magnitude gate.

**What this does not prove.** Synthetic data through mirrored types, not the
real `PersistedNode`/`Container`. The enum-typed columns (`tag_presentation`,
`import_provenance`, `classifications`, `frame_layout_hints`, `properties`,
`derivations`) are omitted; they are empty in a default session and would add
to both arms. It measures snapshot encode/decode and flat scans, not live
petgraph traversal. It is an order-of-magnitude gate, not a regression budget.

### Other findings

- Image externalization is half-executed prior art: `ImageRef`/`ImageRole`
  exist in kernel `types.rs` and the content-addressed `image_store` is
  built, but `Node` still carries raw bytes. D0 is that plan's phase 2, not
  new design.
- quint and seiche were extracted on 2026-07-18 and consolidated with numen
  into `repos/conatus` on 2026-07-21. The geometry sidecar remains in
  Mere's canvas/cartography boundary.
- The sidecar pattern has two proven instances (browser_node_state,
  denizen_bindings) and one designed migration precedent (absorb-legacy).
- chartulary's capability-trait table is the compile-time facet tier; the
  runtime tier reuses eidetic `SchemaDefinition` (three formats, meta-schema,
  `TypedPayload`), per the pack-schema round.

## Open questions

1. RESOLVED 2026-07-18: facet-store home is **chartulary-generic** (F0/F1
   landed there; eidetic validator is a mere-side adapter over the seam).
2. RESOLVED 2026-07-27: Container has one content capability with two
   representations. Small authored text stays in `Container.body`; durable
   binary/large content uses `Container.content` as a muniment hash. The
   builders keep the two mutually exclusive.
3. RESOLVED 2026-07-27: kernel `Address` is Container's host-native address
   representation; list order carries the role, primary first. No parallel
   `AddressClaim` role column remains.
4. RESOLVED 2026-07-26 by the D-gate measurement above: structural
   dissolution proceeds. The combined post-D0 dissolved path beats the former
   status quo; the physical web-node fallback is unused.

## The overmap — sessions as container nodes (Mark, 2026-07-19)

Ruling-shaped direction, sketched while resolving how a fork "opens": **a graph
fork is node lineage one level up.** Sessions are themselves container nodes in
a graph of graphs (working name: the overmap), and the machinery re-uses
itself at that level rather than growing a parallel session subsystem:

- **The nesting primitive already exists.** chartulary B0 `GraphBearing`: a
  node bears a nested graph. A session's root graph is exactly that — a
  container node in the overmap bearing the session graph. No new structure.
- **ManifestStore is the flat precursor.** `GraphSessionManifest` already
  carries the overmap's adjacency without the graph: `root_graph_id` (the
  container id — the same id `scene.*` facets key on), `parent_session` (a
  lineage edge), `sub_graph_refs` (containment edges). Destination: the
  overmap IS a graph and the manifest table is derived from (or replaced by)
  it.
- **Fork = mint container node + derivation edge.** The same `CopiedFrom`
  provenance shape nodes already carry, applied to containers. The switcher
  stops being a list and becomes a graph view of the overmap; "open a fork" is
  *navigating to its container node* (the enter-nested-graph gesture), not a
  window operation. Windows stay lenses (one_state_n_windows holds).

**The taxonomy under it** (Mark's six, mapped to what exists — each a
different aspect of the one node, not competing node classes):

| Aspect | What it is | Existing home |
| --- | --- | --- |
| **container** | the node itself; may bear a nested graph | chartulary `Container` / `GraphBearing` |
| **content** | what a container bears (addressed bytes, typed records) | content classes, content_store, facets |
| **relation** | typed edges between containers | GraphLog statements / kernel edges |
| **arrangement** | where/how containers present; never graph truth | forme + `arrangement.*` facets, seiche live |
| **history** | the append-only edit spine | GraphJournal / codicil |
| **stream** | the live short-term flow; consolidates or is forgotten | engrams (`graph_engram`/`engram_seal`) + athanor/memory_levels |

**"Are graphs denizens?" — the category dissolves.** Denizen is not a node
class; it is a *facet bundle* (a personae subject + grants — the
`denizen.binding` facet, since 2026-07-22 `{subject, kind}` only; the nested
world hangs on `Node.nested` itself). Containment is
structure; denizen-ness is agency; they are orthogonal facets on the one node.
So: a graph is a container, and it is *also* a denizen exactly when it is
subject-bound (a servitor's home graph, a pack's graph). Most session graphs
are containers with no subject — not denizens.

**Scripts / daemons: content at rest, denizen in action.** A script's source
is content (addressed bytes some container bears). Its *running instance* is a
denizen (a subject acting through the participant gate). A daemon is a
servitor whose residency persists (nested log + standing grants). Same object,
two aspects — no third category needed.

**Open (not ruled):** where the overmap itself persists (a chartulary graph at
the profile root, with ManifestStore derived from it?); overmap-level deletion
semantics (deleting a session-node = athanor sweep of its stream + optional
consolidating engram first?); whether cross-session edges in the overmap are
the sharing seam murm/moot address. Fork's G4-R plan (tearout doc) takes the
v0 that does not gate on any of this: mint + session-switch.

## Progress

- **2026-07-27 (PLAN COMPLETE — D0-D4 + F-follow):**
  - **D0:** node/snapshot pixels are gone. References stay on the node, decoded
    pixels use the bounded byte-LRU paint cache with demand reload, and original
    PNG bytes are durable by digest. Eidetic and Turnstone file-sidecar stores
    deliberately remain two adapters over the same digest/hex/PNG contract.
    Legacy RGBA migrates before materialization and re-saves immediately;
    role-aware orphan GC is proposal/apply in session-runtime and mark/sweep in
    Turnstone.
  - **D1/F-follow:** kernel `Node` embeds
    `Container<Uuid, kernel::Address>`; body is the Container inline-text
    capability, addresses are primary-first, and `cached_host` is derived.
    Turnstone reconciles web-page and note classes in one graph, validates their
    required facets through `SchemaFacetValidator`, and the generic adapter
    persists facet schemas plus content-class definitions as typed Eidetic
    engrams.
  - **D2/D3:** `Graph` owns the one live `FacetStore<Uuid>`. Pin, frame layout,
    split suppression, tag presentation, visit clocks, import provenance,
    classifications, literal properties, and derivations left `Node`; kernel,
    canvas, memory eviction, fork/copy, and RDF consumers read typed graph
    accessors. `graph.json` writes the former columns empty. A legacy snapshot
    imports them once; canonical `facets.json` overlays and wins, then Turnstone
    persists the merged store and stripped graph immediately. Graph engrams
    advance to v2 (`snapshot + facets`), still opening v1 and remapping facets
    during composition.
  - **D4:** the live physical remainder is exactly
    `Node { container, images }`. `images` is the explicitly retained D0
    experience-handle map, not pixel storage or semantic metadata. The old
    columns survive only in the deserialize-compatible `PersistedNode` DTO and
    the one-time importer. RDF projection reads semantic facets. North-star OQ
    5.1 is closed.
  - **Final receipts:** 844 tests pass across the eight affected Mere package
    suites (seven test-bearing packages plus `content-contract`), Turnstone's
    full 139-test suite passes, and the committed Graphshell G3 endpoint
    receipt matches the live projection. The last regression exposed by the
    full run was same-millisecond visit recency inheriting local graph order;
    spiral scores now use stable node identity as their portable tie-breaker.
- **2026-07-22 (CONTAINMENT IS STRUCTURE — `Node.nested` lands on the kernel
  Node):** the orthogonality ruling above got its structural half. The kernel
  `Node` gains `nested: Option<LogId>` (rkyv adapter + serde-defaulted
  `PersistedNode.nested`, old snapshots load unchanged) and implements
  chartulary's `GraphBearing`; a `SetNodeNested` delta runs the apply/capture
  spine so bearing a world journals attributed; canvas exposes
  `set_node_nested_for`. The `denizen.binding` facet slims to pure agency
  (`{subject, kind}`; legacy `nested_log` reads once for the adopt heal,
  never writes back). A cross-graph copy does not carry `nested` — a fork's
  copy is un-resided until the slot-convention world move makes forked worlds
  real copies. Receipts: kernel 277 / session-runtime 217 / turnstone 99;
  headed `denizen_b1.scn` RESULT ok with `nested` in `graph.json` and the
  slimmed facet in `facets.json`. Full narrative in the participant plan's
  progress log.
- **2026-07-20 (SIDECAR CONVERGENCE COMPLETE — mere `10084b3`, turnstone
  `ed75c79`):** the last two bespoke per-node sidecars joined the facet store.
  **denizen.*: born as facets** — `denizen_bindings.json` removed before any
  host ever wrote one (the cartography path again); `denizen_facets` holds the
  `denizen.binding` facet (`{subject, nested_log, kind}`, one coherent record
  per the `arrangement.material` precedent), with DenizenBinding/DenizenKind
  unchanged (kebab-case, unknown-forward) and empty-binding-writes-as-remove.
  **web.*: live migration** — browser_node_state persistence converges onto
  atomic `web.scroll` / `web.form_draft` / `web.viewer` / `web.compat` /
  `web.content` facets (non-default only; whole-set rewrite clears stale);
  the in-memory `BrowserNodeStates` stays the host working set. turnstone saves
  web facets beside arrangement + scene and adopt reads them back, seeding
  unseen nodes once from a pre-convergence `browser_nodes.json` (facet values
  win; the stale file is left inert); content-on respawn now flows from the
  `web.content` facet (asserted at the adopt seam). **Every per-node sidecar
  is now one store**: facets.json carries `arrangement.*` + `scene.*` +
  `web.*` + `denizen.*` + foreign namespaces; the bespoke documents remaining
  beside it are non-per-node (frame/workbench/windows/settings/tombstones).
- **2026-07-20 (HEADED RECEIPT: the facet layout arc survives a restart):**
  scenario pair `turnstone/scenarios/facet_layout{,_verify}.scn` (self-drive
  harness, fresh profile): part 1 settles the sample ring + three `mere://`
  nodes, `act Save session`, captures; part 2 relaunches on the profile and
  captures the restore. Green both runs
  (`testing/turnstone/images/scenarios/facet_layout{,_verify}/`). Verified
  three ways: the restored capture reproduces the saved arrangement (not a
  fresh spiral); `facets.json` holds all 15 `arrangement.position` facets at
  settled coordinates + the four `scene.*` container facets; `graph.json`
  contains zero positions. Two explained deltas: ~5-frame drift (part 1
  captures after the save), and the restored frame draws per-family relation
  cells (traversal teal) because boot re-selects the last-visited node
  (rung-6 restore) while the live session drew the flat underlay — a
  selection-state render difference, not persistence. This is the missing
  save-half receipt (the adopt half was already unit-tested).
- **2026-07-19 (scene.* container facets LANDED — mere `579b5e1`, turnstone
  `a832ba6`):** the atomic-facets lens reached the *graph-scoped* view settings
  the geometry sidecar carried alongside the per-node data. `size_by_degree`,
  `size_by_importance`, `importance_metric`, and the physics `damping` are not
  per-node, so not `arrangement.*` — but the graph IS a container node
  (one-node model), so they are **facets of the container**, `scene.*`, keyed
  by `root_graph_id`, in the same `facets.json`. session-runtime `scene_facets`
  (a `SceneFacets` bundle over four atomic facets, per-field fallback);
  `retain_present_nodes` keeps the container id through the prune. `physics_damping`
  **left `PersistedSettings`** (it was scene-scoped and had no reader there) →
  `scene.physics_damping`, host-held, applied on adopt. This is the first
  **container** facet — the same mechanism now spans leaf and container nodes,
  which is the atomic-facets endgame the one-node ruling pointed at. Follow-on:
  the fork must carry `scene.*` donor→fork container (planned, tearout G4-R).
- **2026-07-19 (arrangement.* family COMPLETE — mere `89e3ad5`, turnstone
  `86d5d25`):** the remaining five per-node families (size / sprite /
  sprite-hull / material / face) landed on the shared rewrite-clears-stale
  helpers (`rewrite_family` / `read_family`); payload shapes match the canvas
  `apply_cartography_*` seams one-to-one (a hull with any malformed point is
  skipped whole — a partial polygon is a wrong collider). turnstone saves all
  six from `cartography_geometry()` and re-dresses on adopt in the canvas's
  documented seam order (positions seed first, sprites before hulls, faces
  after sprites); sizing flags reset on adopt like the rest of the unpersisted
  view state. **The cartography convergence is done**: `CartographyGeometry`
  remains only as the canvas's save-time read surface; its
  `to_persisted_json`/`from_persisted_json` sidecar half is now dead code (a
  cleanup candidate), and the graph-scoped flags (`size_by_degree`,
  `size_by_importance`, `importance_metric`) await a view-settings home
  (turnstone does not yet wire the view-intent store).
- **2026-07-19 (`arrangement.position` facets LANDED — mere `16fa15a`,
  turnstone `ddf066e`):** the first arrangement-family convergence rung, and it
  came out cleaner than the plan assumed: the bespoke cartography sidecar
  (`CartographyGeometry` → `cartography.json`) **was never wired by any host**
  (`seed_cartography` / `apply_cartography_*` had zero production callers), so
  the durable layout store is **born as facets** — no transitional file, no
  migration. session-runtime `arrangement_facets`: the `arrangement.position`
  id + `{x, y}` payload, `write_arrangement_positions` (rewrite-clears-stale) /
  `read_arrangement_positions` (skips malformed), `retain_present_nodes`
  (departed node takes its whole facet record). turnstone: `App.facets` holds
  the session's `NodeFacetStore`; save writes `cartography_geometry()`
  positions as facets, adopt prunes to the live graph and re-places via
  `seed_cartography`; round-trip tested at the adopt seam. Remaining family
  (size / sprite / sprite-hull / material / face) follows the same pattern;
  graph-scoped flags (`size_by_degree`, metric, …) stay view settings per the
  ruling. The no-op `commit_positions_to_graph` seam resolves the same way:
  the tear-out fork copies the donor's `arrangement.*` facets.
- **2026-07-19 (S2 COMPLETE; `Node.position` retired `631b852`):** the kernel
  `Node` carries no geometry. Field, accessors (`projected_position` /
  `set_node_position` / `set_node_projected_position` /
  `node_projected_position`), and the rkyv `Point2DAsTuple` adapter removed;
  `add_node`'s position parameter is accepted-and-ignored (`_position`; caller
  cleanup is a follow-on). Reroutes as mapped, plus what the pass surfaced:
  (1) underlay live-path fallbacks → `position_of(k)` only;
  `projection_from_graph` is now the **structural-only** projection (nodes at
  origin) with `projection_from_positions` the placed path. (2) cartography
  write-back `commit_positions_to_graph` → **no-op seam** (the tear-out fork
  should carry layout by copying the cartography sidecar; retire the seam when
  the fork path does that). (3) `switcher_thumbnail` → only
  `build_switcher_thumbnail_with(graph, position_of, opts)` remains (the
  graph-default variant deleted; no production caller — turnstone/genet/isometry
  grep clean). (4) cross-graph copy no longer carries position. (5)
  `Canvas::{with_graph, set_graph}` park at origin + halt; restore goes
  through `seed_cartography` (tests now drive that seam). (6) arrangements
  `LeaveInPlace` fallback parks at `config.origin`; the eight per-adapter
  `graph_truth_positions_are_never_mutated` tests retired with the field.
  Full workspace green (mere-kernel 273, mere-canvas 133, arrangements 86;
  gemot excluded — its `moot-peer` example has an unrelated pre-existing
  async-call error). **Lane S is done**; the `arrangement.position` facet
  remains a Lane F consumer (durable save-time position), not a Lane S gate.
- **2026-07-19 (S2 started; velocity retired, position audited):** the migration
  wall is **gone** — `PersistedNode` (the graph snapshot's node) carries neither
  position nor velocity (positions live in the cartography sidecar since the
  boundary pass), so retiring these `Node` fields is a pure in-memory refactor,
  no rkyv/snapshot migration. **`Node.velocity` RETIRED** (mere `511883d`): it
  was dead (set to zero in constructors, asserted zero in one test, never read;
  seiche's rapier bodies hold live velocity). Field + 3 constructors + the rkyv
  `Vector2DAsTuple` adapter + the test gone; mere-kernel 274 tests green,
  mere-canvas green. **`Node.position` audit**: NOT dead — it is the graph-side
  projected-position store canvas writes seiche into (`cartography.rs`
  `set_node_projected_position`, replacing seiche's removed `write_positions_to`)
  and reads as a fallback for not-yet-placed nodes
  (`position_of(k).or_else(|| graph.node_projected_position(k))`, underlay.rs
  ×5, cartography_scene.rs, build.rs seed). Retiring it = reroute those ~8
  canvas sites onto seiche + the cartography sidecar (and later the
  `arrangement.position` facet), with the unplaced-node fallback/seed semantics
  preserved. A careful canvas-side pass (the hot concurrent zone); no snapshot
  migration needed. That plus the accessor cleanup is the remaining S2.
- **2026-07-19 (position consumer map complete; `projected_centroid` retired
  `192d497`)**: the *committed*-position path
  (`underlay::{canvas_paint_list, projection_from_graph}`) is **test-only** —
  production already uses the seiche-live path
  (`canvas_paint_list_from_positions`), and that code comment names the
  transition ("positions transition from committed to seiche-live"). So the
  production reroutes are narrow: (1) the live-path **fallback**
  `position_of(k).or_else(|| node_projected_position(k))` (underlay ×4) →
  `position_of(k)` (seiche-only; invariant: every graph node is synced to
  seiche); (2) the **cartography write-back** `set_node_projected_position` →
  removed (seiche is the store); (3) **`switcher_thumbnail`** (session-runtime),
  which defaults to `node_projected_position` → the host passes seiche positions
  — a **cross-crate API change**, likely reaching turnstone/genet callers, which
  is why this is a careful pass and not a slice; (4) **cross-graph copy** drops
  the position carry (the copy is re-laid-out). Then remove the kernel accessors
  (`projected_position` / `set_node_position` / `set_node_projected_position` /
  `node_projected_position`) and the `Node.position` field. `projected_centroid`
  (dead, no production caller) already removed.
- **2026-07-18 (S0 COMPLETE):** quint + seiche extracted to sibling repos
  (`github.com/mark-ik/{quint,seiche}`, MIT/Apache, relicensed from MPL,
  publish-ready), pushed; mere drops them from workspace members and consumes
  them as git deps via `[workspace.dependencies]`, local edit loop through the
  gitignored `.cargo` patch (numen/chartulary pattern). quint 35 tests + seiche
  50 tests green standalone; mere-canvas green on the siblings (`a163fc1`). The
  portable physics stack numen → quint → seiche is fully extracted. **Lane S
  remaining: S1 + S2** (retire `Node.position/velocity` onto the
  `arrangement.position` facet — the facet store is wired, `b85aeea`).
- **2026-07-18 (S0 severance COMPLETE; extraction remaining):** both crates are
  now kernel-free and portable. quint (`c5f0106`), then seiche in three commits:
  the canvas de-coupling (`5b9ac84`, adapters `build::{sync_sim_with_graph,
  coupling_force_from_graph}` + drop the `kernel-bridge` feature), the
  seiche-internal migration (`2235e3f`, six test modules moved to the kernel-free
  `sync_nodes`/`sync_edges` API + drop the `kernel` dep/feature/dev-dep), and doc
  polish (`91cb126`). seiche depends only on rapier/petgraph/quint/numen/euclid;
  50 tests pass; mere-canvas green. **Remaining S0**: lift quint+seiche to their
  own repos (MIT/Apache, publish-ready) and repoint mere (git siblings + `.cargo`
  patch, drop from workspace members). **Coverage follow-up**: the three removed
  `from_coupling` graph-resolution tests want a canvas-side test for
  `build::coupling_force_from_graph` (deferred: build.rs is at 563 LOC, near the
  600 ceiling, so the adapters + tests want their own `seiche_bridge` module).
- **2026-07-18 (S0 started; quint severed, seiche paused on concurrency):**
  quint's `kernel-bridge` removed — `commit_to_graph` (the unused inverse bridge)
  deleted, the optional `kernel` dep and feature gone, so quint's only substrate
  dep is numen and it is portable/publishable. 35 quint tests green. The change
  landed swept into another agent's commit `c5f0106` (the meerkat funeral), which
  is itself the signal: **`canvas/canvas` is under a hot concurrent refactor**
  (meerkat funeral + "canvas absorbs the guts" + orrery rename, three commits in
  ~40 min). seiche's severance must edit `canvas/canvas` (the adapter + the
  `sync_with_graph`/`from_coupling` call sites in build.rs/fields.rs), so it is
  **paused until the canvas refactor settles** rather than colliding. The seiche
  plan is unchanged: move `sync_with_graph`/`write_positions_to`/`from_coupling`
  to canvas-side adapters over the kernel-free core (`sync_nodes`/`positions`/a
  resolved `CouplingForce` ctor), migrate the in-crate kernel-bridge tests to the
  kernel-free API, then remove seiche's `kernel` dep + feature.
- **2026-07-18 (Lane F complete):** `chartulary::{facet, content_class}` landed
  (`0051d7c`, pushed): F0 facet store + `FacetValidator` seam, F1 content-class
  model + registry, both generic and schema-agnostic. 42 chartulary tests (9
  new), clippy clean. Home resolved chartulary-generic (open question 1); the
  eidetic validator is the mere-side adapter (F-follow). `serde_json` promoted
  dev-dep → dep for the facet value type. Remaining lane-F item is F2 (gate
  lane) and F-follow (mere wiring at D1).
- **2026-07-18:** Plan founded from the one-node ruling. Field-by-field map
  drawn from the same-day read of `node.rs`; D0 identified as the image
  plan's phase 2; lanes and gates set. No code changed.
