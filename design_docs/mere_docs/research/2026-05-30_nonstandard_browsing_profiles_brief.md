# Nonstandard Browsing Profiles and Semantic-Web Donors

**Date:** 2026-05-30
**Status:** Research brief. Refreshes May substrate research against current upstream code and specifications.
**Scope:** Optional browsing profiles and semantic-web donor code above Mere's native event DAG, Iroh transport, and Genet/Xilem host boundary.

---

## 1. Executive decision

Mere should keep its native graph vocabulary closed where behavior depends on it, while preserving open predicate IRIs for lossless JSON-LD and RDF interchange. The linked-data surface should be a projection and materialized query layer, not a replacement for the kernel graph, Eidetic engrams, or Moot contribution flow.

NextGraph is useful in that architecture, but as a donor and comparison target rather than as a substrate:

- evaluate its `engine/oxigraph` fork, internally packaged as `ng-oxigraph`, for RDF materialization, SPARQL, and RDF-CRDT behavior;
- study its verifier boundary for turning accepted changes into materialized RDF patches and reactive query updates;
- study its ShEx-driven typed object and reactive SDK work for applet tooling;
- do not adopt its repository, broker, networking, DID, or branch-capability stack wholesale.

Lofire is the historical predecessor to NextGraph. It may be easier to read as architectural archaeology, but it is not the primary fork target while the NextGraph codebase is active.

The browsing-profile work should proceed as explicit host-level capabilities:

1. semantic projection: JSON-LD, RDF, SPARQL, SHACL, and schema engrams;
2. annotated temporal snapshots: Web Annotation, Memento, and WACZ;
3. portable miniapps: webxdc-style packages;
4. linked notifications: Webmention, LDN, and selected Solid notification patterns;
5. compound media and spatial objects: IIIF and Web of Things profiles.

---

## 2. Stable Mere boundary

The current linked-data plan already defines the right split:

- `EdgeFamily` and `SemanticSubKind` remain the small, curated vocabulary that drives Mere behavior.
- A semantic edge may also retain an open predicate IRI.
- Recognized external predicates map to canonical Mere IRIs and retain their curated subkind.
- Unrecognized predicates remain round-trippable as raw interned IRIs.
- Remote JSON-LD contexts are not fetched by default.

That is compatible with serious semantic-web support. It rejects the part that would be harmful: allowing an external RDF vocabulary, sync engine, or CRDT representation to silently redefine kernel behavior.

A suitable architecture is:

```text
Mere kernel graph
  closed behavioral families + open predicate IRIs
        |
        v
linked-data crate
  JSON-LD ingest/export
  RDF 1.2-compatible projection
  schema engram handling
        |
        v
derived RDF materialization
  quad store
  SPARQL query
  SHACL validation and UI metadata
  optional reactive subscriptions
```

The materialized RDF graph is rebuildable. It can carry broader statements than the kernel understands without becoming the source of identity or graph truth.

---

## 3. Freshness corrections

### 3.1 Willow and Iroh-Willow

The May substrate notes are stale in a consequential way.

The official Willow Rust implementation is now [`willow25`](https://docs.rs/willow25/latest/willow25/). It implements the Willow25 data model and Meadowcap. Its payload digest is `bab_rs::TreeRoot`, reflecting the WILLIAM3/Bab line rather than the older BLAKE3-default shape assumed in the May estimate. The official [Willow implementations page](https://willowprotocol.org/more/botanical-garden/implementations/index.html) still lists storage and sync work as in progress.

[`n0-computer/iroh-willow`](https://github.com/n0-computer/iroh-willow) is also active again. The previous "dormant pre-alpha" conclusion should be retired.

The conceptual fit remains:

- Willow namespaces and Meadowcap remain strong models for scoped replication and authorization.
- Iroh remains the right Mere transport floor.
- Mere should still keep transport, authorization, and durable graph identity separate.

The implementation estimate changes:

- do not assume a zero-cost BLAKE3 mapping into Iroh blobs;
- measure whether Willow25 interop requires storing both a WILLIAM3 payload digest and an Iroh BLAKE3 blob address;
- compare current `willow25`, active `iroh-willow`, and a Mere-native Meadowcap-shaped scope layer before selecting a crate boundary.

### 3.2 Iroh

Iroh itself is moving quickly. The [Iroh 1.0 release candidate](https://www.iroh.computer/blog/iroh-1-0-release-candidate) was announced on 2026-05-19 and includes WebAssembly support. Mere currently pins the 0.98 family in `murm`.

This does not require an immediate transport rewrite. It adds a useful experiment: determine whether a browser-Wasm peer lane can complement the native Genet/Xilem host and existing fetch boundary.

### 3.3 NextGraph and Lofire

The claim that NextGraph is inactive is stale. Its authoritative repository is the actively developed [NextGraph Gitea workspace](https://git.nextgraph.org/NextGraph/nextgraph-rs), not its older GitHub mirror. The current tree groups its Rust engine crates under [`engine/`](https://git.nextgraph.org/NextGraph/nextgraph-rs/src/branch/main/engine), including Oxigraph, verifier, repository, network, broker, and storage implementations.

The [`engine/oxigraph` fork](https://git.nextgraph.org/NextGraph/nextgraph-rs/src/branch/main/engine/oxigraph), internally packaged as `ng-oxigraph`, is especially relevant. Its README says that it adds CRDT behavior to RDF/SPARQL plus encrypted RocksDB support. [`engine/verifier`](https://git.nextgraph.org/NextGraph/nextgraph-rs/src/branch/main/engine/verifier) provides a useful example of a boundary that materializes signed repository changes into queryable state.

NextGraph's own [protocol documentation](https://docs.nextgraph.org/en/protocol) also shows why wholesale adoption is the wrong move for Mere. NextGraph defines its own content-addressed encrypted blocks, signed commit DAG, repository branches, overlays, broker distribution, and read capabilities. Those overlap directly with Mere's event DAG, Eidetic, Iroh blobs and gossip, and Moot authorization work.

[`LoFiRe`](https://github.com/LoFiRes/lofire-rs) is NextGraph's historical predecessor. Keep it as a simpler reference implementation where useful; do not fork it as the primary semantic-web base.

### 3.4 W3C semantic-web stack

The stable interchange anchor remains [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/).

The broader stack is active:

- [RDF 1.2 Concepts](https://www.w3.org/TR/rdf12-concepts/) reached Candidate Recommendation Snapshot on 2026-04-07.
- [SPARQL 1.2 Query](https://www.w3.org/TR/sparql12-query/) published a Working Draft on 2026-05-25.
- [SHACL 1.2 Core](https://www.w3.org/TR/shacl12-core/) is under active development.
- [SHACL 1.2 UI](https://www.w3.org/TR/shacl12-ui/) published its first Working Draft on 2026-05-26.

SHACL 1.2 UI is particularly relevant to the Genet/Xilem authoring layer: accepted schema engrams could describe validation constraints and editing affordances, while Rust-native views render the actual interface.

---

## 4. NextGraph donor audit

| Component | Mere value | Disposition |
|---|---|---|
| `engine/oxigraph` (`ng-oxigraph`) | RDF quad storage, SPARQL surface, RDF-CRDT experiments | Strong spike target. Compare its patch series against upstream Oxigraph before forking or vendoring. |
| `engine/verifier` | Materialization boundary from accepted changes to queryable RDF state | Strong design donor. Rebuild the boundary around Mere graph contributions and engrams. |
| ShEx reactive SDK work | Typed graph objects, generated app APIs, reactive updates | Study alongside SHACL 1.2. Use as tooling inspiration, not as kernel schema authority. |
| `engine/repo` | Signed commit DAG, branches, overlays, repository read capabilities | Do not adopt. It duplicates Mere's durable graph and capability design. |
| `engine/net`, `engine/broker` | Distribution and relay layer | Do not adopt. Iroh is already Mere's transport floor. |
| NextGraph identity work | NextGraph-specific identity model | Do not adopt as identity substrate. Keep identity above sync and transport. |
| LoFiRe crates | Older, smaller implementation of the same design lineage | Read selectively for clarity. Do not make it the maintained fork base. |

RDF-CRDT behavior should remain specialized. It may be valuable for collaborative linked-data documents or derived projections. It should not become the identity model for every graph contribution. That would recreate the sync-layer-as-identity mistake in a more sophisticated form.

---

## 5. Browsing profiles worth targeting

| Priority | Profile | Standards and donor projects | Mere fit |
|---|---|---|---|
| P0 | Semantic projection | JSON-LD 1.1, RDF 1.2, SPARQL 1.2, SHACL 1.2, `engine/oxigraph` | Lossless export, interoperable ingest, derived query, validation, and authoring metadata. |
| P0 | Annotated temporal snapshot | [Web Annotation](https://www.w3.org/TR/annotation-model/), [Memento RFC 7089](https://www.rfc-editor.org/rfc/rfc7089.html), [WACZ 1.2](https://specs.webrecorder.net/wacz/1.2.0/) | Natural fit for engrams, provenance, glosses, archived pages, and time-aware browsing. |
| P1 | Portable miniapp | [webxdc](https://webxdc.org/docs/spec/index.html) | Network-isolated packages with host-mediated updates map cleanly to primitive Moot nodes and separate Genet roots. |
| P1 | Linked notification | [Webmention](https://www.w3.org/TR/webmention/), [LDN](https://www.w3.org/TR/ldn/), [Solid Notifications](https://solidproject.org/TR/notifications-protocol) | External inbox and subscription adapters for mentions, replies, and graph-change notices. |
| P1 | Reproducible bundle | [RO-Crate 1.2](https://www.researchobject.org/ro-crate/1.2/introduction.html) | JSON-LD package shape for research objects, datasets, provenance, and engram bundles. |
| P2 | Compound media | [IIIF Presentation 3.0](https://iiif.io/api/presentation/3.0/) | Browseable canvases, annotations, and manifests for image, document, and audiovisual collections. |
| P2 | Spatial scene | [IIIF Presentation 4.0 Beta](https://iiif.io/api/presentation/4.0/), glTF, OpenUSD, WebXR | Track IIIF 4 `Scene` as a standards-facing spatial object profile; keep the runtime profile experimental. |
| P2 | Action-bearing object | [Web of Things TD 1.1](https://www.w3.org/TR/wot-thing-description11/) | Browseable properties, actions, events, and protocol bindings for devices and service-like nodes. |
| P3 | Live fragment and patch stream | [Turbo Streams](https://hotwire.io/documentation/turbo/handbook/streams), [Datastar SSE events](https://data-star.dev/reference/sse_events), [Braid HTTP draft](https://datatracker.ietf.org/doc/draft-toomim-httpbis-braid-http/) | Donor patterns for an explicit opt-in host patch profile. Do not interpret arbitrary page fragments as trusted native UI. |

---

## 6. Additional enriching ideas

### 6.1 WACZ-on-Iroh packaging

WACZ 1.2 includes [content-aware chunking guidance for IPFS](https://specs.webrecorder.net/wacz/1.2.0/ipfs.html) so archive readers can retrieve indexes and individual records without downloading the whole package. The same packaging principle is worth adapting to Iroh blobs: preserve random-access archive structure and content addressing without inheriting IPFS as a substrate.

### 6.2 SHACL-driven Rust authoring

SHACL 1.2 UI is early, but it is unusually well aligned with Mere's current direction. A schema engram could carry:

- a JSON-LD context or canonical predicate set;
- SHACL validation shapes;
- SHACL UI metadata or a Mere-specific constrained profile;
- an optional generated Rust/Xilem editor or applet binding.

The host remains in control of rendering, permissions, and persistence. The external shape describes data and editing intent rather than injecting arbitrary UI code.

### 6.3 IIIF 4 scenes

IIIF Presentation 4.0 is not production-ready, but its `Scene` model is a concrete standards-facing bridge between scholarly media, annotations, timed state, and 3D or spatial nodes. It is a better near-term spatial target than treating the Spatial Web Foundation stack as an immediate implementation dependency.

### 6.4 RCAN capability research

[`n0-computer/rcan`](https://github.com/n0-computer/rcan) is an experimental delegated-capability system for Iroh protocols. It is worth monitoring beside Meadowcap, Biscuit, and UCAN. It should be treated as a capability donor, not as a reason to collapse identity into the transport layer.

---

## 7. Recommended spikes

### Spike A: derived RDF projection

Build the smallest useful `linked-data` experiment:

1. export a curated Mere graph contribution as JSON-LD;
2. materialize it into upstream Oxigraph;
3. query it with SPARQL;
4. validate it with SHACL;
5. repeat against NextGraph's `engine/oxigraph` fork and identify the exact patches worth carrying;
6. prove that deleting and rebuilding the RDF store does not affect kernel identity.

This spike should explicitly evaluate ShEx-style generated bindings versus SHACL-based validation and form metadata.

### Spike B: annotated temporal snapshot

Represent one archived page as:

- a WACZ package stored through Iroh blobs;
- an engram with capture time and provenance;
- a Memento-aware browseable snapshot;
- Web Annotation targets for Gloss highlights and commentary.

### Spike C: Willow refresh

Replace the old line-count estimate with a current integration measurement:

1. model one namespace and Meadowcap authorization path with `willow25`;
2. measure dual-address storage between WILLIAM3 payload digests and Iroh BLAKE3 blob addresses;
3. compare active `iroh-willow`;
4. decide whether Mere needs Willow25 interop, a Mere-native Meadowcap-shaped layer, or both.

### Spike D: webxdc-style miniapp

Render one signed, network-isolated package in its own Genet root. Limit it to host-mediated state updates and explicit capabilities. This tests the boundary for future applets without prematurely inventing a full extension platform.

---

## 8. Sources

Primary upstream sources used for the refresh:

- [Mere linked-data plan](../implementation_strategy/2026-05-22_linked_data_ingest_export_plan.md)
- [Willow implementations](https://willowprotocol.org/more/botanical-garden/implementations/index.html)
- [`willow25` crate docs](https://docs.rs/willow25/latest/willow25/)
- [`n0-computer/iroh-willow`](https://github.com/n0-computer/iroh-willow)
- [Iroh 1.0 release candidate](https://www.iroh.computer/blog/iroh-1-0-release-candidate)
- [NextGraph authoritative workspace](https://git.nextgraph.org/NextGraph/nextgraph-rs)
- [NextGraph protocol docs](https://docs.nextgraph.org/en/protocol)
- [NextGraph `engine/` crates](https://git.nextgraph.org/NextGraph/nextgraph-rs/src/branch/main/engine)
- [`engine/oxigraph` (`ng-oxigraph`)](https://git.nextgraph.org/NextGraph/nextgraph-rs/src/branch/main/engine/oxigraph)
- [`engine/verifier`](https://git.nextgraph.org/NextGraph/nextgraph-rs/src/branch/main/engine/verifier)
- [`LoFiRe`](https://github.com/LoFiRes/lofire-rs)
- [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/)
- [RDF 1.2 Concepts](https://www.w3.org/TR/rdf12-concepts/)
- [SPARQL 1.2 Query](https://www.w3.org/TR/sparql12-query/)
- [SHACL 1.2 Core](https://www.w3.org/TR/shacl12-core/)
- [SHACL 1.2 UI](https://www.w3.org/TR/shacl12-ui/)
- [Web Annotation](https://www.w3.org/TR/annotation-model/)
- [Memento RFC 7089](https://www.rfc-editor.org/rfc/rfc7089.html)
- [WACZ 1.2](https://specs.webrecorder.net/wacz/1.2.0/)
- [RO-Crate 1.2](https://www.researchobject.org/ro-crate/1.2/introduction.html)
- [IIIF Presentation 4.0 Beta](https://iiif.io/api/presentation/4.0/)
- [`n0-computer/rcan`](https://github.com/n0-computer/rcan)
