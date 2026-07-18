# Node Dissolution + Facets Plan

**Founded:** 2026-07-18, executing the
[one-node ruling](../technical_architecture/2026-07-18_one_node_facets_layer_map.md)
(Mark, 2026-07-18). Three lanes: **F** (the facet store), **S** (spatial
completion), **D** (the kernel `Node` dissolution ladder). Lanes are
independently workable; D depends on F0 for its facet destinations and on the
D-gate measurement for its hot fields.
**Companions:** the [image externalization plan](2026-07-06_node_image_externalization_plan.md)
(D0 executes its phase 2; its phase 1 store is built), the
[boundary pass plan](2026-07-09_mere_merecat_boundary_pass_plan.md) (slice C
invented the sidecar pattern), the
[participant gate + packs plan](2026-07-17_participant_gate_packs_plan.md)
(packs ship custom content classes; facet grants join its scope vocabulary),
and the north star as amended.

## The decision in one line

`chartulary::Container` is the one node; every optional metadatum becomes an
atomic **facet** (typed record keyed by node id + facet id, schema-validated);
mere's kernel `Node` dissolves facet-by-facet until its remainder is Container,
and "web page" becomes merecat's content class defined with the same machinery
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
- **F-follow, the eidetic validator adapter** (mere-side, when D1 wires): a
  `FacetValidator` impl over eidetic's `SchemaDefinition` engrams, plus
  persisting class definitions as schema engrams. Small; the seam is already in
  place.

## Lane S — spatial completion

- **S0, extract quint + seiche** to sibling repos + MPL→MIT/Apache relicense
  (the recorded numen-treatment follow-on; both are kernel-free by design
  already). Done when: `repos/quint` and `repos/seiche` are green standalone,
  mere consumes them as git siblings (local checkout via the gitignored
  `.cargo` patch, per convention), and the canvas suite passes.
- **S1, positions through the sidecar.** Audit every `Node.position` /
  `Node.velocity` read/write; route all durable reads through the
  cartography-geometry sidecar (exists) and live simulation through seiche
  state. Done when: kernel + canvas tests pass with the fields untouched by
  anything but a shim.
- **S2, retire the fields.** Remove `position`/`velocity` from `Node` (rkyv
  snapshot migration: legacy snapshots load, positions absorb into the
  sidecar, one-time, the browser_node_state migration is the pattern). Done
  when: the fields are gone, legacy sessions load with arrangements intact,
  full mere suite green.

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

- **D0, images out** (executes the image plan's phase 2, which is already
  designed: `images: BTreeMap<ImageRole, ImageRef>`, migration included).
  Done when: no pixels ride `Node` or the graph snapshot; sessions migrate
  one-time; switcher/preview surfaces render unchanged.
- **D1, the web content class founded** (merecat-side definition, mere-side
  storage): mime/addresses/body land in their Container homes; cached_host
  and viewer-adjacent metadata become the **web-page facet bundle**, the
  first content class defined as F1 data. The dogfood rung: merecat defines
  it exactly as a modder would. Done when: merecat browses normally with the
  class in force and a second toy class (a note class) coexists in one graph.
- **D2, arrangement + presentation + visit facets.** Done when: the fields
  leave `Node`, their consumers read facets, sessions migrate.
- **D3, semantic facets.** Done when: provenance/classification/property/
  derivation data live in facets or chartulary-native records, and the RDF
  projection reads them there.
- **D4, the remainder audit.** Done when: what remains of `Node` is
  Container's surface (or the D-gate verdict's physical-representation
  residue, explicitly listed), and the north star's OQ 5.1 is marked closed.

## Trigger-gated tail (not scheduled here)

- **Canvas family promotion** (canvas/cartography/arrangements/swatch as one
  unit): trigger = a cambium-side consumer actually wires (woodshed-graph
  already proved the pattern).
- **The pool surface** (personae-indexed vaults, cross-vault queries, p2p
  share): the north star's step 3; its own plan when it fires.
- **Merecat palette/install slice**: owned by the participant plan; proceeds
  in parallel, unblocked by all of the above.

## Findings

- Image externalization is half-executed prior art: `ImageRef`/`ImageRole`
  exist in kernel `types.rs` and the content-addressed `image_store` is
  built, but `Node` still carries raw bytes. D0 is that plan's phase 2, not
  new design.
- quint and seiche live at `crates/canvas/{quint,seiche}` (verified
  2026-07-18); their extraction is recorded but unstarted. The geometry
  sidecar exists in `canvas/cartography`.
- The sidecar pattern has two proven instances (browser_node_state,
  denizen_bindings) and one designed migration precedent (absorb-legacy).
- chartulary's capability-trait table is the compile-time facet tier; the
  runtime tier reuses eidetic `SchemaDefinition` (three formats, meta-schema,
  `TypedPayload`), per the pack-schema round.

## Open questions

1. RESOLVED 2026-07-18: facet-store home is **chartulary-generic** (F0/F1
   landed there; eidetic validator is a mere-side adapter over the seam).
2. `body` inline-vs-blob at D1.
3. `AddressClaim` role mapping onto Container's primary-first address list.
4. The D-gate acceptance envelope (what load-time regression is tolerable).

## Progress

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
