# Graphshell Reference Host Plan

**Date:** 2026-07-27
**Status:** product boundary ruled with Mark; H0-H8 not started.
**Scope:** Make Graphshell Mere's useful, WASM-safe reference host: a graph
portal, browser-extension companion, application launcher, and personal
cross-device surface for addressed things. It does not wait for Turnstone's WPT
or media work.

This plan amends the product center of the
[Graphshell remote projection host plan](2026-07-22_graphshell_remote_projection_host_plan.md).
That plan's portable session crates, projection/presentation/intent planes,
admission boundary, and G1-G5 receipts remain. Its statement that Graphshell's
local truth is only remote-scene curation does not: Graphshell now also owns and
hosts the user's local Mere graph.

It absorbs the live parts of the
[capture-first browser lane](2026-06-24_orrery_browser_lane_plan.md) and the
[extension/companion plan](2026-06-23_browser_extension_companion_plan.md).
Those plans remain historical evidence for capture APIs, browser delivery, and
the companion split; their old Merecat/orrery product framing is superseded.

Related boundaries:

- [one node, atomic facets](../technical_architecture/2026-07-18_one_node_facets_layer_map.md)
- [Mere as the unifying graph](../technical_architecture/2026-07-08_mere_as_the_unifying_graph.md)
- [repo consolidation](2026-07-23_repo_consolidation_plan.md)
- [Knot port](2026-07-25_knot_port_plan.md)
- [low-power radio and managed network](2026-07-24_low_power_managed_network_plan.md)

---

## 1. Product ruling

Graphshell is Mere's reference host and portal into the Mere ecosystem.

It is:

- a local graph GUI over Mere's `Container`, relations, facets, vaults,
  personae, and history;
- a Cambium and Genet consumer;
- a host for Mere scenes, sprites, arrangements, physics, and eventually
  attributed inference;
- a client for granted projections from Turnstone, Knot, Retinue, Woodshed,
  Isometry, Hocket, and other applications;
- a handler router that opens an addressed thing in an owning or compatible
  application;
- a cross-device surface for copying or moving addresses, files, graph
  selections, and scenes while preserving their relations and provenance;
- a WASM application suitable for a browser extension or PWA.

It is not a web browser. Turnstone, formerly Merecat, owns browsing: Genet
engine hosting, page lifecycle, HTML/CSS/script behavior, WPT, media, downloads,
and browser security.

Graphshell may use its own wgpu/WebGPU render surface through NetRender. It does
not embed Turnstone or import the wgpu surface-embedding family
(`grafting`, `scrying`, `welding`), Servo/WebView hosting, or
`genet-winit-host` in its WASM profile.

The shortest product description is:

> Graphshell is the graph where the things you encounter keep their addresses,
> relations, tags, provenance, and access history across applications and
> devices.

## 2. Authority and ownership

| Truth | Owner | Graphshell's role |
|---|---|---|
| Containers, user-authored relations, tags, local scenes, handler preferences | Local Mere graph in Graphshell | Read, write, persist, sync, and present |
| Access and transfer records | Local Mere/Eidetic store | Append, project, retain, redact, and sync under user policy |
| Web document/runtime state | Turnstone or the current host browser | Hold an address and disclosed facets; request `Open` |
| Files-in-place and document merge state | Knot or another file authority | Hold references and disclosed metadata; request read/write/copy intents |
| Radio paths, peers, power, queues, routing | Retinue agent | Mount projections and invoke typed management intents |
| Application-native facts | Owning application | Mount only what its endpoint discloses |
| Remote scene cache | Graphshell client | Retain according to the session's cache policy |
| Inference output | Producing rule/model | Store as derived facets/relations with source and confidence |

The old remote-host rule still applies to mounted domains. A Retinue route or
Turnstone page runtime does not become Graphshell truth because Graphshell can
display it. The new rule is that Graphshell is itself a real Mere application,
so its local graph is not merely a cache of other applications.

## 3. One object, several representations and handlers

The neutral object remains `chartulary::Container`. Optional character remains
in atomic facets. A URL, file, note, device, application object, person, session,
or saved scene does not require a new node base type.

Graphshell selects the cheapest useful representation:

1. label, type-coded glyph, favicon, or sprite;
2. facet-derived semantic card;
3. content-addressed thumbnail or snapshot;
4. a scene or specialized local representation;
5. an `Open` intent delegated to a registered handler.

Unknown content remains useful through identity, address, metadata, relations,
and available actions. Supporting a file format does not require Graphshell to
render it.

Double-click is not hardwired to Turnstone. It resolves a user-configurable
handler offer by:

- content class;
- address scheme;
- required capabilities;
- current profile and device;
- persona/grant;
- explicit per-type or per-address preference.

The browser-extension profile may open a normal host-browser tab. A
device-connected profile may ask a local agent to launch Turnstone or another
application. Both are the same typed `Open` intent with different handlers.

## 4. History is an append-only record, not a node timestamp

The current seams are close but insufficient:

- `VisitHistoryFacet` carries only `last_visited_ms` and
  `last_session_visited`. It is a useful summary.
- `eidetic::BrowsingTrace/v1` records chronological page traversals, referrers,
  transitions, dwell, and candidates, but it is page-shaped and does not name a
  device, application, stable container, or address choice.

Graphshell first defines a provisional `AccessRecord` in the port. It is an
append-only Eidetic typed payload containing at least:

- stable record id;
- addressed `Container` id plus the address actually used;
- action such as examine, open, import, or preview;
- persona, device, and application/handler references when known;
- timestamp and optional dwell;
- referring container/scene and transition;
- capture source and privacy class.

The `visit.history` facet becomes a derived summary cache over those records.
It is never the chronology's authority.

Keep the provisional schema in Graphshell until a second live producer uses the
same fields. The intended second producer is Turnstone. At that point:

1. move the record and schema into Eidetic;
2. convert both consumers;
3. delete the Graphshell-local definition;
4. migrate `BrowsingTrace/v1` as an import projection rather than maintaining
   two competing histories.

Access on another device appends another record. It does not rewrite or discard
the original observation.

## 5. Scenes, sprites, physics, and inference

- A **scene** is a saved projection over a graph selection: score/query,
  arrangement, camera, representation preferences, and optional mounted remote
  fragments. Saving it creates curation truth, not new domain truth.
- A **sprite** is a content-addressed representation resource referenced by a
  facet. It can travel without changing the container it depicts.
- **Physics** is live arrangement state through Seiche/Conatus. Persist only an
  intentional arrangement or scene checkpoint; frame-by-frame positions remain
  derived.
- **Inference** produces derived facets or relations carrying producer,
  model/rule version, input identity, time, confidence, and revocation or
  replacement lineage. It cannot silently overwrite user assertions.

Graphshell is the reference host where these graph primitives become one usable
system. Shared abstractions still require a second consumer. A Graphshell-only
Cambium adapter or browser storage adapter stays in the port until another
application proves the same boundary.

## 6. Delivery profiles

### Browser extension

The primary capture profile:

- Graphshell WASM application;
- extension tab and optional sidebar;
- WebExtension history/navigation intake under an optional user grant;
- OPFS/IndexedDB-backed local Mere/Eidetic store;
- host-browser open handler;
- optional connection to user-owned device agents.

The extension can observe browser-wide visits after permission. The standalone
web application cannot, so ambient capture remains an extension capability.

### PWA

The same Graphshell application without browser-wide capture:

- inspect and edit the local/synced graph;
- import an engram or user-selected history export;
- open ordinary web addresses;
- connect to device agents through an admitted browser carrier.

### Device agent

A native, headless authority for capabilities browser WASM cannot own:

- arbitrary filesystem access;
- local application launching;
- Iroh/p2panda;
- Reticulum, Tulle, Sennet, Tucket, and other native transports;
- protected key storage;
- large or background transfers.

The agent exposes projections and typed intents. It is not another GUI.

## 7. Dependency direction

```text
ports/graphshell (WASM reference host)
    -> mere portable graph/canvas profile
    -> cambium + Genet DOM/layout/host seams
    -> graphshell-client + graphshell-protocol
    -> NetRender WebGPU for Graphshell's own surface

Turnstone
    -> mere + Genet browser engine + native presentation/embedding
    -> graphshell-endpoint for its disclosed projections

Retinue agent
    -> retinue/tulle/sennet/tucket/outrider
    -> graphshell-endpoint + graphshell-protocol

mere-transport --optional reticulum--> retinue
```

There is no Cargo cycle in the Retinue shape. Cargo cycles are package-level:

- the core `retinue` crate has no Mere dependency;
- `mere-transport` may depend on core `retinue`;
- the composition-leaf Retinue agent may depend on the leaf
  `graphshell-endpoint` and `graphshell-protocol` crates;
- those Graphshell leaf crates do not depend on `mere-transport` or Retinue.

Repository arrows may point both ways while the package DAG remains acyclic.
Verify that mechanically with `cargo tree`.

### Retinue ruling

Keep the radio and transport implementations in the Retinue workspace. Moving
them into Mere would erase Retinue's independent protocol/hardware boundary and
would not solve a package cycle that does not exist.

Add a small `ports/retinue-agent` package to the Retinue repository when the
lane starts. It owns:

- projection adapters for devices, interfaces, peers, announces, routes,
  queues, power policy, transit policy, and service offers;
- intent lowering for settings and bounded send/transfer actions;
- local stdio first, then admitted remote carriers.

It depends on published or git-pinned Graphshell leaf contracts, never on the
`mere` facade or `ports/graphshell`.

A new repository is warranted only if the Retinue agent later becomes a
standalone product with coherent utility apart from both Retinue and Mere.

## 8. Measured starting point

Checked in the live workspaces on 2026-07-27:

- `graphshell-protocol` and `graphshell-client` compile for
  `wasm32-unknown-unknown`.
- `cambium` compiles for `wasm32-unknown-unknown`, including its Genet DOM
  seam.
- `mere-canvas --lib` compiles for `wasm32-unknown-unknown`, including
  NetRender's WebGPU backend, Genet layout, Seiche, and the Mere kernel.
- the `mere-canvas` package fails a whole-package WASM check only because Cargo
  also tries to build its native `canvas` binary, whose winit,
  `genet-winit-host`, and native wgpu imports are unavailable on WASM.
- the composed `mere` facade fails its WASM check earlier through
  `mere-linked-data -> oxrdf -> rand -> getrandom 0.3`, which has no selected
  browser backend.
- the WASM `mere` dependency tree does not contain `grafting`, `scrying`, or
  `welding`.
- the current `ports/graphshell` is still a native receipt/session host. It has
  no Cambium, Genet, Mere kernel, browser storage, or browser presenter.
- NetRender provides portable async device boot and a WebGPU target, but its
  deferred browser-canvas demo has no real consumer. Graphshell is now that
  consumer.
- the OPFS backend is still a promised `muniment::Backend`, not an implemented
  crate.

This selects feature-cone repair and host adapters. It does not select a graph
canvas rewrite.

## 9. Implementation sequence

### H0. Seal the WASM product cone

**Files:**

- `crates/mere/Cargo.toml`
- `crates/mere/src/lib.rs`
- `crates/canvas/canvas/Cargo.toml`
- `ports/graphshell/Cargo.toml`
- `ports/graphshell/src/lib.rs`
- `ports/graphshell/README.md`
- `scripts/check_port_boundaries.py`

Split the `mere` facade by capability rather than target:

- portable graph/domain exports;
- optional linked-data;
- optional canvas;
- optional workbench/native composition.

Keep Turnstone's current default behavior until its manifest explicitly names
the features it needs. Graphshell selects the smallest graph + canvas profile.
Do not make RDF block the first web host; either give `getrandom` its explicit
browser backend later or leave linked-data out of the first WASM profile.

Mark the native `mere-canvas` binary with a native-present feature or
target-appropriate required feature so `--lib` is not the only honest WASM
command.

Move Graphshell's `graphshell-stdio`, Notochord, native transport, and Tokio
dependencies behind native target/features. Add common dependencies only after
their WASM check passes. Disable automatic binary discovery or declare the
native G1/G4/G5 receipt/session binaries explicitly behind the native feature;
otherwise Cargo will still attempt to compile them during a WASM package check.

Add a dependency-cone check rejecting Turnstone, Servo browser runtime crates,
`genet-winit-host`, `grafting`, `scrying`, and `welding` from the Graphshell
WASM profile.

**Done when:**

```powershell
cargo check -p graphshell-protocol -p graphshell-client --target wasm32-unknown-unknown
cargo check -p mere-canvas --target wasm32-unknown-unknown
cargo check -p graphshell --target wasm32-unknown-unknown --no-default-features --features web
cargo tree -p graphshell --target wasm32-unknown-unknown --no-default-features --features web
```

are green, and the tree check contains none of the forbidden embedding/browser
crates.

**Stop rule:** do not start extension packaging while the application cone is
red or while a native-only dependency is merely hidden by an untested cfg.

### H1. Make Graphshell a local Mere host

**Files:**

- `ports/graphshell/src/app.rs` (new)
- `ports/graphshell/src/mere_host.rs` (new)
- `ports/graphshell/src/access.rs` (new)
- `ports/graphshell/src/handlers.rs` (new)
- existing `crates/graphshell/*` unchanged except proven protocol needs

Compose:

- a local Mere graph and facet store;
- local Personae/profile selection;
- Eidetic/muniment storage through an injected backend;
- Graphshell client mounts for remote endpoints;
- an in-process Mere projection endpoint so local and remote scenes traverse
  the same projection/presentation/intent vocabulary;
- handler offers and typed open intents.

The portable Graphshell session crates remain free of the Mere kernel. The
adapter belongs in the port.

Build one deterministic fixture containing:

- web and non-web addresses;
- one file reference;
- tags and several relation kinds;
- one saved scene;
- access records from two devices;
- a mounted remote projection;
- an unknown facet namespace that must survive load/save.

**Done when:** the fixture loads, projects, mutates through typed intents,
persists, reopens byte-equivalently at the graph/facet boundary, and retains the
unknown facet.

### H2. Present the host in a browser

**Files:**

- `ports/graphshell/src/web.rs` (new)
- `ports/graphshell/web/` (new loader, HTML, CSS)
- Graphshell-local Cambium/Mere-canvas composition modules

Build the first real NetRender browser consumer:

- boot the WebGPU device asynchronously;
- attach it to an `HTMLCanvasElement`;
- drive Mere Canvas frames and semantic input;
- render Graphshell controls with Cambium over Genet's neutral DOM/layout
  seams;
- expose keyboard and accessibility semantics;
- use Canvas2D only if a measured target needs the fallback.

Keep the browser presenter inside Graphshell. Promote it into NetRender or Genet
only after another real browser host consumes the same seam.

**Done when:** a headed Chromium run and a headed Firefox run can pan, zoom,
select, drag, open the detail surface, switch a mounted session, and invoke an
advertised action at wide and narrow sizes, with committed screenshot and
semantic-tree receipts.

**Stop rule:** a WASM compile or generated HTML receipt is not a headed-browser
receipt.

### H3. Ship the local graph product

Add the daily graph operations:

- create from an address or user-selected file;
- edit title, tags, facets, and relations;
- search/filter and select relation families;
- save and reopen scenes;
- choose arrangement and physics settings;
- select sprites/representations;
- configure handlers and open the object externally;
- export and import a selected graph engram.

File locations are device-local facets. Portable identity uses container ids,
content hashes, and logical addresses; an absolute local path is not disclosed
to another device unless the user explicitly includes it.

Transfer/export scope is a setting:

- object only;
- object plus direct relations;
- selected subgraph;
- saved scene and its selection.

**Done when:** a user can build and reopen a useful mixed graph, inspect an
unknown file through metadata, open web content in a configured handler, and
round-trip a selected subgraph without losing ids, relations, facets, sprites,
or provenance.

This is the first standalone product cut. It does not wait for extension
capture or cross-device sync.

### H4. Add the browser-extension profile

**Files:**

- `ports/graphshell/web/extension/`
- Graphshell-local OPFS or IndexedDB `muniment::Backend`
- `ports/graphshell/src/capture.rs` (new)
- `ports/graphshell/src/access.rs`

Package one MV3-oriented codebase for Chromium and Firefox:

- optional `history`, tab, and navigation permissions;
- one-time history import chosen by the user;
- live visit intake;
- title, favicon, transition/referrer, and modest metadata capture;
- AccessRecord append and derived graph/facet update;
- retention, origin exclusion, private-window exclusion, and forget controls;
- batching that survives service-worker suspension.

Do not capture full page bodies or screenshots by default. Make each richer
capture class an explicit setting with a visible retention cost.

Implement the browser store adapter in Graphshell first. Promote it into
muniment after a second browser consumer proves the same contract.

**Done when:** real consented browsing appears in the graph, survives browser
restart, can be filtered by time/device/persona, can be forgotten, and the same
extension package passes headed Chromium and Firefox receipts.

### H5. Move one selection between two devices

Complete the two open G5f clauses first:

- resume after a real interruption;
- reject an actual `IntentInvocation` after revocation.

Then use the existing admitted p2panda/Iroh carrier for the first transfer:

- graph/scene selection encoded as a versioned engram;
- content-addressed blobs transferred and verified independently;
- access records included according to user policy;
- transfer receipt records source, destination, route, hashes, and result.

Distinguish two operations:

- **replicate within one persona/pool:** preserve Container ids and append to
  the same history;
- **copy into another graph/vault/persona:** mint new ids and retain
  `CopiedFrom` provenance.

`Move` is replicate/copy plus an explicit, separately authorized source
retirement. A successful copy never silently deletes the source.

**Done when:** a URL node and a real file selection move from one physical
device to another, content hashes verify, the chosen relation closure and tags
survive, the destination examination appends its own AccessRecord, revocation
blocks a transfer intent, and interruption resumes without restarting the
whole transfer.

### H6. Add continuous personal-device sync

Evaluate the existing Chartulary, Stickleback, and Commons-spine seams against
the Graphshell event grammar. Reuse them where their contracts match; do not
rename a Commons-specific fold into a generic graph sync layer.

Promotion gate:

- Graphshell and the existing Commons/Knot consumers must prove the same
  causal authoring, LogSync drain, and storage boundary;
- promote only the common bridge;
- delete each consumer's replaced implementation in the same slice.

Sync:

- graph mutations and facets;
- access records;
- saved scenes and handler preferences selected for sync;
- blob availability separately from metadata.

**Done when:** two offline devices edit tags/relations and record accesses,
reconnect, converge deterministically, expose any unresolved domain conflict,
and retain per-device chronology without last-writer loss.

### H7. Add the Retinue agent and constrained profile

**Retinue repository files:**

- `ports/retinue-agent/Cargo.toml` (new)
- `ports/retinue-agent/src/main.rs` (new)
- `ports/retinue-agent/src/endpoint.rs` (new)
- `ports/retinue-agent/src/projection.rs` (new)
- `ports/retinue-agent/src/intents.rs` (new)

Start as a local stdio endpoint. Project:

- attached radios and interfaces;
- identities/peers and announces;
- paths and reachability;
- queue, airtime, power, and transit state;
- service offers;
- recent bounded activity.

Expose typed intents for settings, announce/discovery, connect, and bounded
send/transfer operations. Retinue remains the authority and validates each
intent against its current state.

Use Iroh/p2panda for the first large-file path. The Reticulum/direct-PHY profile
starts with addresses, compact graph records, management facts, and deliberately
small payloads. Increase the payload class only from measured byte, fragmentation,
airtime, resume, and energy receipts.

**Done when:** browser Graphshell mounts a real Retinue agent, displays live
radio facts, changes one owner setting through an attributed intent, transfers
an address plus one measured bounded file over the constrained path, and the
resulting graph/access/transfer records agree with the Iroh profile.

**Stop rule:** a TCP Reticulum run does not prove RF, and the existing 4 KiB
one-hop receipt does not prove arbitrary file transfer or routing.

### H8. Open the same surface to applications and AI

Expose the same projection, query, and intent grammar through:

- Graphshell sessions for applications;
- an optional MCP adapter;
- browser extension messaging;
- local automation.

An AI participant receives a Personae/Servitor grant, reads only disclosed
projections, and submits typed proposals. Accepted mutations and derived facts
carry attribution and receipts. Rejected proposals do not appear as graph
truth.

**Done when:** a user, an application, and a granted agent can inspect the same
scene and invoke the same action through three adapters, with equivalent
authorization and attributed results.

## 10. Settings that remain user-controlled

- captured browser APIs and origins;
- private-window behavior;
- history retention and redaction;
- synchronized facet namespaces;
- transfer relation closure;
- handler choice by scheme/content class;
- device and carrier preference;
- scene/camera/physics persistence;
- sprite and representation fallback;
- inference providers, visibility, and confidence threshold;
- local-only, selected-devices, or shared-vault scope.

Defaults may be supplied, but the data boundary is not hardcoded.

## 11. Verification wall

### Compile and dependency receipts

- Graphshell portable crates on native and `wasm32-unknown-unknown`.
- Graphshell web profile on `wasm32-unknown-unknown`.
- warning-denying Clippy on changed first-party crates.
- dependency-cone denylist for browser engine and embedding crates.
- `cargo tree` proof that Retinue's package graph is acyclic.

### Behavioral receipts

1. deterministic local Mere fixture and reopen;
2. headed Chromium and Firefox graph-host interaction;
3. consented extension capture and forget;
4. two-device admitted transfer, interruption/resume, and revocation;
5. offline convergence;
6. real Retinue agent management;
7. measured RF constrained transfer.

The evidence ladder stays explicit: compile, unit, native integration, headed
browser, two-device network, and real RF are different claims.

## 12. Open decisions

1. The final names of the Mere facade features. The capability split is ruled;
   `graph`, `linked-data`, `canvas`, and `workbench` are working names.
2. Whether the browser store starts on OPFS or IndexedDB. Measure transaction
   behavior and service-worker recovery; do not decide from API fashion.
3. Whether Graphshell's browser presenter directly owns the wgpu surface or a
   thin Genet host adapter does. Keep it local until the first headed receipt
   shows the real seam.
4. The provisional AccessRecord schema fields beyond the minimum above.
   Freeze them through Graphshell plus Turnstone evidence, not conversation
   alone.
5. Which Retinue projection is first: attached-radio management or Reticulum
   route/service state. Choose the one with a real device receipt available.

None of these decisions changes the product boundary or requires a new
repository.
