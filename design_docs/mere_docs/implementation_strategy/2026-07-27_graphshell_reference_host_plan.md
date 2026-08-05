# Graphshell Reference Host Plan

**Date:** 2026-07-27
**Status:** product boundary ruled with Mark; H0-H3 complete; H4 operational follow-ons remain; H5-H7 complete; H8-H9 not started.
**Scope:** Make Graphshell Mere's useful, WASM-safe reference host: a graph
portal, Personae identity-vault surface, browser-extension companion,
application launcher, and personal cross-device surface for addressed things.
It does not wait for Turnstone's WPT or media work.

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
- [identity vault and SSH agent](2026-07-22_identity-vault-ssh-agent_plan.md)
- [persona wallet carry layer](2026-06-25_persona_wallet_carry_layer_plan.md)

---

## 1. Product ruling

Graphshell is Mere's reference host and portal into the Mere ecosystem.

It is:

- a local graph GUI over Mere's `Container`, relations, facets, vaults,
  personae, and history;
- the permanent resident host and GUI for Personae's identity vault, SSH agent,
  signing approvals, devices, and grants;
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
| Persona master seeds, private keys, vault roots, and private epoch material | Personae vault | Host the native authority, request typed operations, and never copy secret material into the graph or browser |
| Persona profiles, public key references, device roster, signed delegations, and revocations | Personae, with carry records still partly in `session-runtime` | Project public summaries and signed evidence; invoke typed management intents |
| Signing approval policy and signing records | Local Graphshell policy plus the native Personae host | Present, enforce, append, retain, and redact under user policy |
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

### Personae is a first-class surface

Personae already supplies the useful core:

- passphrase- or OS-store-unlocked encrypted profiles and slots;
- a resident SSH agent over the standard Windows named pipe or Unix socket;
- SSH key import, listing, public-key export, removal, and signing;
- master-authorized protocol-key derivation and attestation;
- signed, attenuable, expiring, and revocable delegation certificates.

The current limits are product gaps, not hidden accomplishments. The agent is
Ed25519-only. A `PerUse` SSH slot refuses to sign because there is no
confirmation UI. `ShortTtl` currently behaves like `Session`.
`session-bind@openssh.com` is verified but does not yet constrain the key to
that session. Device roster, persona-wallet manifests, grants, and private
epoch carry also still live partly in `session-runtime` rather than Personae.

The prior identity-vault plan already ruled that the agent's permanent home is
Mere/Graphshell. Restore that ruling here:

- native Graphshell hosts the Personae vault and SSH-agent endpoint in-process;
- Graphshell's browser/PWA profile reaches that authority through an admitted
  local-device session;
- the interim standalone agent remains a recovery and dogfood tool until the
  Graphshell host proves equivalent launch, crash recovery, and real SSH use;
- browser WASM never receives a seed, private key, vault root, private epoch,
  decrypted slot payload, or unrestricted signing handle.

Graphshell projects the safe, useful identity surface into Mere as provisional
Graphshell-local facets and content classes:

- persona/profile;
- vault protection and lock state;
- references to the muniment data vaults and graph roots the persona may use;
- public key reference: protocol, public key or fingerprint, comment, lineage,
  unlock tier, and availability;
- device and enrollment state;
- signed grant, delegation, attenuation, expiry, and revocation evidence;
- signing request, decision, and result receipt.

These are graph projections, not duplicate authorities. Signed evidence remains
authoritative in Personae, Servitor, or the owning grant ledger. The graph may
explain and index it, but editing a projected grant cannot grant authority.
Secret bytes never become facets or `Container` bodies.

Keep the two uses of *vault* distinct. The Personae credential vault holds
secrets and performs cryptographic operations. Muniment data vaults hold the
content and graph roots a persona may access. Mere relates their safe public
references; it does not merge their storage or unlock domains.

This is where Mere earns its place rather than acting as a settings shell. A
persona, device, key reference, application, address, file, transport, and
signing receipt are neutral `Container`s related in one graph. A persona may
bear a nested graph for its devices and authority projections when containment
is useful; cross-app and cross-vault references remain ordinary relations.
Eidetic carries the append-only signing/decision record, Stemma carries
replacement and derivation lineage, Servitor gates application and agent
intents, and Notochord authenticates the session that requested them.

That permits useful graph queries and scenes without exposing secrets:

- which persona, device, handler, and transport were involved in an access;
- which public key reference signed an operation and under which approval;
- which devices and applications hold an active grant from this persona;
- which keys, grants, or devices are expired, revoked, unavailable, or due for
  replacement;
- which files, addresses, and sessions share an identity or signing history.

Cross-app persona linkage remains opt-in. A work face and burner face do not
become correlatable merely because Graphshell can display both.

The level-0 threat boundary remains explicit: encryption at rest protects a
closed vault on disk, not a compromised native Graphshell process. The standard
SSH-agent endpoint also accepts local clients. Per-use approval, authenticated
session context, and bounded policies reduce that authority; the GUI must not
claim they identify or sandbox every local caller.

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
- Personae vault custody, the standard SSH-agent endpoint, and signing approval
  enforcement;
- Iroh/p2panda;
- Reticulum, Tulle, Sennet, Tucket, and other native transports;
- protected key storage;
- large or background transfers.

The agent exposes projections and typed intents. Graphshell is its GUI.

## 7. Dependency direction

```text
ports/graphshell (portable application and WASM reference host)
    -> mere portable graph/canvas profile
    -> cambium + Genet DOM/layout/host seams
    -> graphshell-client + graphshell-protocol
    -> NetRender WebGPU for Graphshell's own surface

ports/graphshell (native composition)
    -> portable Graphshell application
    -> personae vault + SSH-agent library
    -> Notochord admission + native device capabilities

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
- Personae's DPAPI/passphrase vault, standard SSH-agent endpoint, SSH slot
  management, delegation certificates, and real Windows-to-Linux signing receipt
  already exist in this workspace. The vault pane, signing confirmation broker,
  real short-TTL relock, and Graphshell residency do not.
- Personae's carry-layer destination is ruled, but the live device roster,
  persona-wallet manifests, device grants, and private-epoch bridge still reside
  in `session-runtime`.
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

**2026-07-27 receipt:** H0 is complete. Mere now exposes `graph`,
`linked-data`, `canvas`, and `workbench` capabilities while retaining the full
default facade. Graphshell has explicit default `native` and opt-in `web`
profiles; the web profile selects only Mere graph + canvas. All receipt binaries
require `native`, and the standalone canvas presenter requires
`native-present`. The exact WASM checks, native compatibility checks, 44-test
Graphshell receipt, warning-denying focused Clippy, dependency-cone result, and
local-patch limitation are recorded in the
[H0 WASM product-cone receipt](../../../ports/graphshell/docs/2026-07-27_h0_wasm_product_cone_receipt.md).

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
- a selected persona/profile reference, with actual vault authority injected by
  the native host when present;
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
- one public persona, device, key-reference, grant, and signing-receipt
  projection containing synthetic test material only.

**Done when:** the fixture loads, projects, mutates through typed intents,
persists, reopens byte-equivalently at the graph/facet boundary, and retains the
unknown facet.

**2026-07-27 receipt:** H1 is complete. The Graphshell port now owns a local
Mere host adapter, injected Muniment persistence, local and remote mounts
through one portable client, typed address-open intents with configurable
handler offers, access-history facets, and safe public identity projections.
The deterministic fixture mutates through an advertised open intent, persists,
reopens, retains its foreign facet, remounts both scenes, and re-saves the
unchanged boundary document byte-equivalently. Commands, results, and the
normalization finding are recorded in the
[H1 local Mere host receipt](../../../ports/graphshell/docs/2026-07-27_h1_local_mere_host_receipt.md).

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

**2026-07-28 receipt:** H2 is complete. Graphshell now has a separate
`graphshell-web` workspace package that owns the browser-only Cambium, Genet,
NetRender, WebGPU, and DOM dependencies while the existing `graphshell` web
profile stays portable. Headed Chromium 150 and Firefox 151 passed pan, zoom,
selection, persistent node drag, detail, mounted-session switching, and
advertised-action invocation at 1280×800 and 600×800. Four screenshots and the
captured semantic trees are committed with the
[H2 browser presenter receipt](../../../ports/graphshell/docs/2026-07-27_h2_browser_presenter_receipt.md).

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

This is the first standalone graph product cut. It does not wait for extension
capture or cross-device sync.

**H3 receipt (2026-07-28):** the WASM-safe product layer now owns address and
file creation, metadata and relation editing, filtering, arrangement and
physics settings, representations, scenes, handler choice, and scoped graph
engram transfer. A native round-trip preserves ids, semantic and provenance
relations, facets, and sprite state. Headed Chromium passed the full product
path at 1280×800 and 600×800, including an exact external-handler handoff and
device-local facet exclusion. See the
[H3 local graph product receipt](../../../ports/graphshell/docs/2026-07-28_h3_local_graph_product_receipt.md).

### H4. Make Personae visible and usable

**Absorbs G8** from the
[remote projection host plan](2026-07-22_graphshell_remote_projection_host_plan.md),
which was written the same day to carry the 2026-07-22 ruling that the agent's
resident home is Graphshell. Three things that entry made explicit and this
one should not lose:

- `personae::agent` is a library module so the resident host serves the
  endpoint **in-process**, not as a separate install.
- The interim logon scheduled task fakes a lifecycle this item has to own for
  real: start with the host, survive a crash, stop when the host stops. Retire
  the task in the same change that replaces it, never before.
- `UnlockTier::PerUse` slots **refuse to sign** today, because signing without
  asking would make the tier a lie. The confirmation UI is therefore what makes
  that tier usable at all, not a polish item.

**Files:**

- `ports/graphshell/src/identity.rs` (new)
- `ports/graphshell/src/identity_endpoint.rs` (new)
- `ports/graphshell/src/identity_projection.rs` (new)
- `ports/graphshell/src/native/personae_host.rs` (new)
- `ports/graphshell/src/native/device_broker.rs` (new)
- `ports/graphshell/src/bin/graphshell_device_host.rs` (new)
- `ports/graphshell/src/bin/graphshell_native_host.rs` (new relay)
- `ports/graphshell/install-device-host-windows.ps1` (new)
- `ports/graphshell/src/session_loop.rs`
- `crates/persona/personae/src/agent.rs`
- `crates/persona/personae/src/signing.rs` (new only if the approval seam is
  independently useful outside Graphshell)
- current `crates/system/session-runtime/src/wallet_store.rs` and
  `wallet_grant.rs` carry sources, through a narrow read/intent adapter

Move the resident Personae host into native Graphshell and add a plain
**Identity** surface. It presents:

- profiles/personas and the selected face;
- vault backend, protection, lock, and agent-listener state;
- SSH key references with public fingerprint, comment, lineage, unlock tier,
  public-key export, import, generation, and explicit removal;
- devices, enrollments, grants, delegations, expiry, and revocation;
- pending signing requests and an append-only decision/result history.

Do not block the pane on the remaining carry-layer promotion. Read each fact
from its live authority through one adapter, label unavailable features
honestly, and move the adapter when the roster/grant/epoch types finish folding
into Personae.

Add an approval broker between the SSH protocol adapter and vault signing.
`PerUse` waits for an explicit visible decision. `ShortTtl` caches approval only
for its configured idle window and then relocks. `Session` retains the existing
unlocked-session behavior. Policies are configurable per key and adapter, with
bounded remember/expiry choices rather than a process-wide boolean.

The approval surface shows only facts the carrier can prove: persona, public key
reference, operation class, payload digest, time, and any authenticated
requester, process, host, or session binding. Missing context is displayed as
unknown. Graphshell does not infer a target host from ambient state or present
an unverified process label as authority.

Append one signing record for approve, deny, timeout, or failure. It links the
public key reference, persona, device, authenticated requester/target when
known, decision policy, signed-payload digest, result, and related graph object
or session when one exists. It does not contain private key material or the
cleartext payload by default.

Applications and AI use the same typed signing request and Servitor grant as
other callers. Automation may receive a bounded pre-approval policy, but it
cannot bypass the broker or silently widen its own scope.

**Done when:**

- native Graphshell owns the real standard SSH-agent endpoint and lists the
  existing vault-held key by its public fingerprint;
- the browser Graphshell surface reaches that local authority only through an
  admitted session, and a captured browser snapshot contains none of the
  vault's secret material;
- a `PerUse` request blocks, displays proven context, succeeds after approval,
  fails after denial, and both paths append signing records;
- a `ShortTtl` key signs within its configured window and requires approval
  after real idle expiry;
- a real SSH login signs through Graphshell after process restart;
- the interim resident task is removed only after launch-at-login and crash
  recovery are proved for Graphshell on the same machine;
- identity, device, grant, access, and signing projections can be selected in
  one scene and survive reopen without becoming authority.

**Stop rules:**

- never serialize a seed, vault root, decrypted slot, private epoch, or private
  key into Mere, Eidetic, a Graphshell session, browser storage, logs, or
  receipts;
- never treat an editable grant projection as authorization evidence;
- do not claim per-use confirmation when the request cannot actually wait for a
  decision;
- retain the standalone agent until the Graphshell replacement passes the real
  SSH and recovery receipts.

**H4a receipt (2026-07-28):** the first H4 authority slice is complete.
Personae now has a shared approval broker that makes `PerUse` genuinely wait,
implements bounded `ShortTtl` reuse with real idle expiry, and records only
public request facts and digests. Native Graphshell composes the vault, SSH
adapter, approval broker, public identity/carry read model, portable cards, and
typed approve/deny intents in-process. A live pending-card intent released the
real SSH adapter and appended a signed history record. The standard endpoint
and standalone scheduled task were deliberately left unchanged. Browser
admission, real SSH login after restart, lifecycle cutover, management actions,
and mixed-scene reopen remain H4 work. See the
[H4a Personae authority receipt](../../../ports/graphshell/docs/2026-07-28_h4a_personae_authority_receipt.md).

**H4b receipt (2026-07-28):** native Graphshell now generates Ed25519 keys
inside the resident authority, accepts imported parsed keys only through a
direct native handoff, and requires public-fingerprint confirmation before
removal. Portable actions contain public options only. On Windows, a guarded
nonstandard named-pipe listener let a real SSH client list the vault key and
complete a verified `PerUse` signature through the approval broker. The guard
refused the standard endpoint, and the live scheduled agent remained
untouched. Native picker wiring and admitted browser access were subsequently
closed by H4d and H4e. Standard-endpoint cutover, restart/login and lifecycle
proof, carry mutations, and mixed-scene reopen remain H4 work. See the
[H4b SSH key-management receipt](../../../ports/graphshell/docs/2026-07-28_h4b_ssh_key_management_receipt.md).

**H4c receipt (2026-07-28):** the resident authority now implements the
ordinary Graphshell endpoint traits as a memory-only portable-card projection.
Its carrier constructor binds the projection to the transcript-derived
`SessionAuthority` id. A portable client mounted every public card through
`serve_admitted_session`, reconstructed an approve-once payload from disclosed
public data, released the waiting real SSH adapter, and verified the signature.
The actual browser-to-device carrier and headed-browser receipt remain open;
this is the application path they will carry, not a substitute for them. See
the
[H4c admitted identity endpoint receipt](../../../ports/graphshell/docs/2026-07-28_h4c_admitted_identity_endpoint_receipt.md).

**H4d receipt (2026-07-28):** the first actual browser-to-device carrier now
uses WebExtensions native messaging. Exact Chromium and Firefox extension ids
select the installed native host; fresh host and extension nonces derive a
private link id; the existing signed `SessionHello` is still the sole
application admission step. Personae and all signing material stay native.
Focused tests reject launcher mismatch and captured-hello replay. A real native
host process served the identity projection, and headed Chromium loaded the
unpacked extension, reached the admitted session, rendered a real pending
`PerUse` request, approved it, and replaced it with a completed signing-history
card without browser errors. The receipt host verified the SSH signature and
the extension closed the device session cleanly. Headed Firefox remains open.
Installer manifests and one shared bridge are present for Windows, macOS, and
Linux; H5 history capture has not started. See the
[H4d browser native-carrier receipt](../../../ports/graphshell/docs/2026-07-28_h4d_browser_native_carrier_receipt.md).

**H4e receipt (2026-07-28):** the admitted extension can now request native
SSH-key import using only its projection session id and a user-selected unlock
policy. The path, encrypted key bytes, and passphrase never enter the browser,
native-messaging schema, Graphshell protocol, or receipt. A host-owned system
picker reads a bounded regular file, prompts locally only for an encrypted key,
zeroizes both buffers, and returns only a public mutation result. Headed
Chromium selected and unlocked a disposable encrypted Ed25519 key, refreshed
from two to three public cards, showed `Unlock: every use`, and closed cleanly
without browser errors. A re-entry guard reduced two immediate activations to
one native request. The receipt used an in-memory profile and left the user's
vault, standard endpoint, and live agent task unchanged. Windows is proved;
Firefox and other desktop dialog providers remain open. See the
[H4e native encrypted-key import receipt](../../../ports/graphshell/docs/2026-07-28_h4e_native_key_import_receipt.md).

**H4f receipt (2026-07-28):** Graphshell now has a resident device host that
owns one shared Personae vault, identity signer, approval broker, browser
broker, and SSH endpoint. The browser-launched native host is a vault-free
relay; `SessionHello` remains the sole application admission step. An isolated
DPAPI vault listed, signed, and verified before and after process restart, then
completed the admitted browser-card smoke path through the relay. A reversible
live rehearsal bound the real Windows OpenSSH pipe over the user's current
vault, returned the existing fingerprint, verified signatures before and after
restart, and served the browser projection from the same authority. The
interim task was restored with the same fingerprint. The known laptop was
offline, so remote login remains open. The Graphshell logon task was not
installed or used to retire the interim task because a real sign-out or reboot
receipt is still required. See the
[H4f resident device-host receipt](../../../ports/graphshell/docs/2026-07-28_h4f_resident_device_host_receipt.md).

**H4g receipt (2026-07-28):** the final mixed-scene done-condition is now
closed. Active delegated-device cards advertise one typed, native-only
revocation intent with explicit confirmation. The live `session-runtime`
roster remains mutation authority; Graphshell exposes only a public result and
refreshes the projection after acceptance. Public identity cards are
exportable only by explicit user pinning. Their local facet stores the public
portable card, source and observed revision, fixes authority to
`source-owned`, and omits live actions. A proof issued a real signed device
grant, produced a real Personae SSH signing-history record, rejected an
unconfirmed revocation, accepted a confirmed revocation through the endpoint,
then persisted and reopened profile, device, grant, signing, and access
projections in one Mere scene. Remote login, real logon recovery and interim
task retirement, headed Firefox, other dialog providers, and broader carry
management remain open. See the
[H4g carry mutation and mixed-scene receipt](../../../ports/graphshell/docs/2026-07-28_h4g_carry_mutation_mixed_scene_receipt.md).

**H4h receipt (2026-07-28):** the Windows lifecycle installer now carries an
optional data root, quotes its recovery launcher correctly, disables the
retained Personae task after cutover, and restores it after a failed install or
update. A real install listed the same SSH fingerprint before and after killing
the first Graphshell child and observing its replacement. An intentional
wrong-fingerprint update restored Personae, and a correct rerun left
`graphshell-device-host` running with `personae-agent` retained but disabled.
Firefox 153 then loaded the temporary extension against that resident
authority, reached `Admitted · 10 public cards`, displayed the new Device
access boundary, and closed after 13 answered requests. The bridge renders
confirmed SSH-removal and device-revocation actions, but the live vault was not
mutated. A real sign-out or reboot, remote SSH login, native Firefox import,
macOS/Linux dialogs, and a configured carry root remain open. See the
[H4h live cutover and Firefox receipt](../../../ports/graphshell/docs/2026-07-28_h4h_live_cutover_firefox_receipt.md).

**H4i receipt (2026-07-29):** <remote-host> returned at `<private-address>`. With the
installed `graphshell-device-host` running, `personae-agent` disabled, and the
stock Windows agent stopped, the standard endpoint listed the existing
`SHA256:d3tQ...` vault key. Batch-mode OpenSSH then authenticated
`<remote-user>@<private-address>` and executed a remote command on macOS. The
remote-login done-condition is closed. Real sign-out or reboot recovery and
final retirement of the disabled Personae task remain open. See the
[H4i remote SSH login receipt](../../../ports/graphshell/docs/2026-07-29_h4i_remote_ssh_login_receipt.md).

This is the first integrated reference-host cut: the graph, identity vault, and
native capability broker work as one product.

### H5. Add the browser-extension profile

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

**H5a receipt (2026-07-28):** the browser-storage and capture core is complete.
Graphshell owns a full IndexedDB `muniment::Backend`; the Wasm host seeds once
and reopens the same Mere document. A disabled-by-default policy now sanitizes,
filters, deduplicates, and batches browser visits into LocalOnly typed
AccessRecords, derived graph/facet state, traversal relations, and Eidetic
browsing memory. Authority queries filter by time, persona, and device.
Forgetting removes trace and AccessRecord manifests, clears dedupe state, and
can remove capture-created objects. The extension action now opens the Wasm
graph portal, requests `history` only from an explicit Enable action, sanitizes
before its bounded durable queue, and acknowledges only after graph
persistence. Native Eidetic keeps pack signing and full JSON Schema validation
by default while the Wasm consumer omits both native dependency cones. In a
fresh headed Chromium profile, the user granted `history`; one query-redacted
synthetic visit reached LocalOnly AccessRecord and browsing-trace authority,
the queue drained, the graph grew from eleven to twelve nodes, and a cold
restart retained both the permission and graph. See the
[H5a browser storage and capture-core receipt](../../../ports/graphshell/docs/2026-07-28_h5a_browser_storage_capture_core_receipt.md).

**H5b receipt (2026-07-28):** H5 is complete. The exact package passed
user-granted permission, bounded import, live `onVisited`, pre-queue redaction,
atomic acknowledgement, authority display, and cold-restart checks in Chromium
and Firefox. The portal filters stored visits by time, persona, and device and
can forget a scoped address with explicit capture-created-object removal.
Graphshell records the persona and device injected by its composing host; it
does not infer an active browsing identity from Personae's vault selection.
Proving that injection in a second host remains an integration follow-on.
Favicon intake is a separate optional `tabs` permission and retention feature,
not an H5 completion condition. See the
[H5b cross-browser capture and controls receipt](../../../ports/graphshell/docs/2026-07-28_h5b_cross_browser_capture_controls_receipt.md).

### H6. Move one selection between two devices

The G5f carrier prerequisites are complete:

- resume after a real interruption was proved across the Windows-to-<remote-host>
  physical run;
- the corrected intent-first arrangement now rejects a literal
  `IntentInvocation` after revocation through the real p2panda/QUIC carrier.

The first 2026-07-29 execution used separate local processes. A second run then
repeated the exact suspend, redial, two-diff resume, accepted intent, and
intent-first revocation refusal across Windows and <remote-host>. G5 is complete. See
the [H6a implementation receipt](../../../ports/graphshell/docs/2026-07-29_h6a_g5f_prerequisite_receipt.md)
and [H6b physical closure receipt](../../../ports/graphshell/docs/2026-07-29_h6b_physical_g5f_closure_receipt.md).

The transfer contract is implemented and verified across independent
stores. It uses a schema-typed immutable selection engram, separate verified
Muniment blobs, an explicit source-history policy, destination AccessRecords,
and a typed transfer receipt. Replicate preserves ids within one persona; copy
mints stable-per-transfer ids through Mere's existing copy primitive and keeps
`CopiedFrom` provenance. Revocation is checked before destination reads or
mutation, and resume skips an already-verified destination blob. See the
[H6c transfer-core receipt](../../../ports/graphshell/docs/2026-07-29_h6c_transfer_core_receipt.md).

**H6d receipt (2026-07-29):** H6 is complete. `TransferSourceEndpoint` carries
the prepared manifest and independently addressed blobs through the existing
snapshot, intent, resource, and resume verbs. A Windows source and <remote-host>
destination transferred one URL and one real file. The destination cached the
manifest, suspended, redialed under a fresh admission, resumed the current
projection without a new snapshot, fetched and verified the blob, and applied
two preserved ids, both tag sets, one `Cites` relation, and two new destination
AccessRecords. A second physical run revoked session 3 before its first,
literal transfer intent; the request loop served one refusal and ended
`Lapsed(Revoked)` before endpoint dispatch. See the
[H6d physical transfer receipt](../../../ports/graphshell/docs/2026-07-29_h6d_physical_transfer_closure_receipt.md).

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
whole transfer. **Met 2026-07-29** by the H6c data-contract receipt and H6d
Windows-to-<remote-host> carrier receipt.

#### H6 addendum (2026-08-02): the product path

The receipts above were earned by `h6_transfer_peer`, a rehearsal binary. No
shipped surface can invoke the lane. The 2026-08-02 sweep found the byte layer
already built and idle in the libs: `transport::blobs::BlobStore` serves the
iroh-blobs ALPN off the same endpoint the sync lane binds and fetches by hash
with native BLAKE3 verification (`fetch_from`); `eidetic::BlobSource::Iroh`
plus `eidetic-iroh-fetcher` resolve manifest blobs by `node-id/hash` ticket;
`ObserveBlobAvailability` already replicates which device holds which blob.
What is missing is composition in the resident host and a product surface.
This does not depend on the carrier seam plan's C3: stage-and-forward makes
apply local, so the sync lane plus the admitted browser broker suffice.

**Settled model (hybrid, ruled with Mark 2026-08-02):**

- Browser Muniment remains product truth, so `apply_transfer` runs intact
  where the receipts proved it. The browser owns graph-to-blob references.
- The resident host's iroh store is a durable recovery replica and serving
  cache with explicit pins. The host owns durable byte availability. It is
  not a second truth.
- An apply receipt promotes destination staging into a recovery pin rather
  than deleting it. Unapplied or unreferenced staging expires by policy.
- Pin release requires evidence of non-reference: each browser profile
  publishes its blob-reference set over the broker (new intent pair). A
  silent profile retains its pins; release on silence is data loss, release
  on explicit profile retirement is a user action. Fail toward retention.
- Blob bytes cross the broker as explicit chunks, about 512 KiB raw before
  base64, with acknowledgements and a small bounded in-flight window under
  the existing `MAX_NATIVE_MESSAGE_BYTES` frame cap. Interleaving with card
  and identity traffic is scheduled, not assumed.
- `navigator.storage.persist()` is advisory. Record whether persistence was
  granted and present unpersisted browser storage honestly; the recovery
  replica is what makes refusal survivable.
- Grant: pairing is the authorization for same-persona replicate, bound to a
  `pairing_id` minted at pair time (a new `PairedDevice` field; `node_id`
  revives across re-pair and `added_ms` is a clock, so neither can carry it).
  Unpair retires the id; re-pair mints fresh; a queued transfer under a
  retired id is refused at apply. Whether a receive-only device may send is a
  roster rule the transfer lane states explicitly.
- The manifest travels as a record on its own stickleback lane, so the
  personal graph lane's wire format does not change.

**Slices:**

- **S1 bytes.** Fs-backed option for the transport blob store; the sync host
  binds `.blobs()`; fetch by availability. Done when a blob put on one device
  is fetched byte-identical on a second through the existing pairing, over
  mDNS and over relay with the relay leg on the Mac, surviving a host restart
  between put and fetch.

  **Met 2026-08-03, except the relay leg.** An 11,317-byte file staged on O-PC
  at 22:59:54 by a one-off process was served after a restart by the
  task-managed process from 23:00:10, and fetched by the ThinkPad at 07:19:39
  and by Q-PC at 07:30:12, both naming `supplier=9b662f09…`. Each destination
  was told only a hash and resolved the holder from the replicated
  `ObserveBlobAvailability` record. Byte-identity is independent, not the
  protocol's own word: BLAKE3 of the source file computed separately is
  `734bfbcb9d7bee7a77509f66c1334678d283ca99e4a24f3e7c4d266fca73851e`, the hash
  each fetch completed against.

  **The relay leg is NOT proven, and the experiment that would have proven it
  disproved the mechanism (2026-08-03).** Q-PC's first fetch finished 0.9s
  before it had selected a home relay, so those bytes crossed a direct LAN
  path from the address in O-PC's ticket. The clean re-run, a fresh blob
  (`60a7b29c…`) and no `--sync-peer`, gave the host only what a restarted
  paired device actually has: O-PC's node id from settings, plus a relay URL.
  It selected its relay within 0.4s and then heard nothing for the full
  60-second window. No advertisement arrived and no session formed.

  The conclusion is structural, not a flake: a bare node id is not dialable.
  Registering a relay makes THIS endpoint reachable; it resolves nothing about
  the peer, because no mechanism maps a node id to an address. The address
  book is per-process, so every route learned in one run dies with it. Of the
  resolver ladder (mDNS, cached address, relay, holepunch), exactly one rung
  exists today: pairing works where multicast works, or where a fresh ticket
  is hand-carried. The Mac, whose mDNS is dead (errno 65, unsigned binary),
  has NO durable path to its siblings between restarts.

  The rung to build first is the cached address: persist the peer's last
  known relay-tagged `EndpointAddr` on the paired-device record, refreshed on
  every successful connection, so a device that has connected once can redial
  through the relay after both ends restart. That is the Syncthing shape
  (device id + cached addresses + global discovery + relays), minus the
  global-discovery server; iroh's n0 DNS discovery is the opt-in equivalent
  when true off-LAN reachability is wanted, with the third-party-visibility
  trade that implies.

  What the run actually cost was not this lane. Two Windows Firewall **Block**
  rules on the Private profile (`{A223E208-…}`, `{30891486-…}`) had been
  dropping all inbound QUIC to O-PC, while the only Allow rules were scoped to
  Public. Graph sync between these devices was dead, not just blobs, and the
  host reported `reachable=1` throughout because `known_peers` is address-book
  membership rather than a live connection. **A resident sync host that cannot
  receive a packet looked healthy for hours.** The peer directory should
  separate configured from connected, and a host with no established connection
  should say so; until it does, `reachable` is not the check it appears to be.
- **S2 manifest.** Transfer lane record plus incoming-transfer card. Done
  when a manifest prepared on one device appears as a card on a second,
  naming the selection and blob count.

  **Shape corrected 2026-08-03 before building.** "Its own stickleback lane"
  costs more than it reads, for two reasons found in the code rather than
  guessed:

  1. `MunimentStore` keys are `op/{hash}` and `log/{author}/{log}/` with no
     lane or extension namespace, so two extension types over one backend
     collide. A second lane implies a THIRD store file beside `<graph>.redb`
     and `<graph>.blobs`.
  2. `to_operation`, `from_operation`, and `stable_subject` are hardcoded to
     `PersonalGraphExt`/`PersonalGraphRecord`. A second lane means
     generalising them over `<E, R>` (a refactor of the sync core) or
     duplicating them, and duplication is against doctrine.

  Neither is required to meet the goal. The goal was **"the personal graph
  lane's wire format does not change"**, and `SetFacet { node, facet, value }`
  already carries arbitrary JSON under a per-facet selection filter, so a new
  facet id is DATA on an existing variant, not a wire change. An offer is
  therefore a facet (`graphshell.transfer-offer/v1`) carrying the manifest's
  blob hash and a card summary; the manifest itself is staged as a blob and
  moves by S1's proven, BLAKE3-verified path.

  This also keeps the lane-selection property that matters: a device that has
  not enabled the transfer-offer facet neither authors nor projects offers,
  exactly as `blob_availability` gates S1.

  A separate lane remains the right answer if offers ever need a different
  admission policy from the graph, or retention independent of it. Revisit
  then, with the two costs above priced in.

  **Met 2026-08-04**, with one hole found and closed during the build. A facet
  needs a node to hang on, and `AddNode` was the one event `projects()` did not
  gate (`_ => true`), so the carrier node for an offer would have materialized
  on every admitted device even where the offer facet was filtered out: a
  titled node with nothing on it, on devices the transfer does not concern.

  Closed with a mechanism in the sync layer rather than a transfer-specific
  branch. `SyntheticAddressRule { prefix, facet, device_scoped }` ties a
  carrier address to the facet it exists for, and `SyncSelection` learned
  `local_device`. `mere://transfer/<destination>/<source>/<id>` names both
  endpoints, so one rule covers three cases: the addressee projects it, the
  sender projects what it sent, and a third device on the same roster does
  not. Because `ReplaySetNodeFacetById` is guarded by
  `get_node_key_by_id(...).is_some_and(...)`, dropping the carrier drops its
  facet too, so the single gate leaves no orphan.

  **This is presentation, not confidentiality, and the distinction is load
  bearing.** The personal lane has no cipher: `personal_sync.rs` encrypts
  nothing, so every operation is plaintext to every device the roster admits.
  A filtered device still receives and stores the offer and could read it by
  flipping one flag in its own settings, because the filter is enforced by the
  reader about itself. **The personal graph's confidentiality boundary is the
  roster, not the projection filter.** Knot next door already runs
  `KnotSyncCipher::{Personal, CommonsData}` over a `DataKeyring`; per-recipient
  encryption here would be that pattern applied to this lane, and it is a real
  feature rather than a filter tweak. Recorded as an open lane item below.

  Wired at `PersonalSyncHost::open` rather than left to callers: the host
  derives its own device key from the transport keypair and stamps the
  selection itself, then asserts after bind that the transport named itself the
  same way. A filter configured against an identity no peer addresses would
  present as an empty inbox rather than an error, which is the failure mode
  this whole plan exists to stop finding late.

  Receipts: `an_offer_over_live_sync_reaches_its_addressee_and_not_a_third_device`
  runs three resident hosts on one roster over real transport;
  `an_offer_summarizes_the_manifest_it_names` pins the offer's advisory counts
  against what applying the same manifest actually produces.

  Two limits carried forward. A device that has not been told its own key
  projects every offer, chosen deliberately so a missed setting over-shows
  rather than silently hiding the feature. And the summary's counts are
  advisory: the manifest governs, and S3 verifies against it.
- **S3 apply.** Chunked broker delivery, browser apply, staging promotion.
  Done when a selection chosen in the browser on one device lands merged in
  the browser on a second with blobs verified and ids per replicate
  semantics. This is a new product-path receipt: H6c/H6d stand for the lane,
  but chunk ordering, duplicate delivery, restart and resume, the recorded
  persistence grant, and staging promotion are new surface and are receipted
  fresh.

  **Device-to-device half met 2026-08-04.** `native::transfer_staging` composes
  the pieces S1 and S2 left sitting next to each other. `offer_transfer` stages
  a prepared manifest's blobs and then the manifest itself into the serving
  store, and only then authors the offer, so a destination acting the instant
  the offer lands finds bytes rather than a promise. `receive_transfer` fetches
  the manifest, decodes it, fetches every blob it names, and writes them into
  the destination's product store.

  Receipt: `a_transfer_applies_after_its_source_is_gone` closes the source sync
  host and drops its blob store **before** calling `apply_transfer`, and passes
  the destination's store as both the source and destination argument. Two ids
  preserved, both tag sets, one `Cites` relation. That is stage-and-forward
  demonstrated rather than asserted: if any byte were still being read across
  the wire the test could not pass.

  Three BLAKE3 addressings meet in this path (`eidetic::Hash` for manifest
  descriptors, `muniment::Hash` for the product store, `transport::BlobHash`
  for the iroh store). They agree, and the code checks rather than assumes it,
  because a silent disagreement would advertise blobs under hashes no
  destination can ask for.

  **A coupling surfaced and was made explicit.** Staging authors one
  `ObserveBlobAvailability` per blob, so a device with the offer facet enabled
  but the blob-availability lane off failed partway through with bytes already
  written and nothing advertised. `offer_transfer` now refuses up front
  (`StagingError::BlobLaneDisabled`), receipted by
  `offering_without_the_blob_lane_is_refused_before_any_byte_is_staged`. A
  transfer needs both lanes; the settings surface should say so rather than let
  the combination look valid.

  **Chunked delivery landed 2026-08-04 at the protocol layer.**
  `ResourceRequest` was whole-resource only, and `ResourceResponse.bytes` is a
  `Vec<u8>` that JSON renders as a number array at roughly 4x, so a resource
  over about 250 KiB could not cross a 1 MiB native-messaging frame at all.
  That is why the ruling said base64.

  Added as `CarrierRequestBody::ResourceChunk` rather than a range on the
  existing verb. `ResourceResponse.resource` is the address *of its bytes*,
  checked by `has_valid_address`; a partial reply would make that field either
  wrong or ambiguous, so chunks address themselves separately and both checks
  stay honest. A chunk carries two hashes: `resource` says which whole thing
  it belongs to, `chunk` says these bytes arrived intact. `ResourceAssembly`
  verifies each frame, refuses an out-of-order or duplicate one, and verifies
  the assembled whole before releasing it.

  **Acknowledgement is the next request.** Mark's ruling asked for
  acknowledgements and a bounded in-flight window; pull-based chunk requests
  satisfy both without separate ack frames, because asking for the chunk at
  offset N acknowledges everything below N, and the window is simply how many
  requests a client leaves outstanding. Interleaving with card and identity
  traffic comes free: each chunk is an ordinary request/response pair the
  existing loop already multiplexes by id.

  `PresentationSource::resource_chunk` is **defaulted**, so every endpoint
  written before chunking existed serves large resources correctly the moment
  a client asks, receipted by
  `an_endpoint_that_only_serves_whole_resources_still_serves_chunks`. An
  endpoint that can seek should override it.
  `base64_keeps_a_full_chunk_inside_the_native_message_frame` asserts a full
  chunk framed as a `CarrierResponse` stays under
  `browser_carrier::MAX_NATIVE_MESSAGE_BYTES`, so raising the chunk size
  without the frame cap fails in a test rather than in a browser.

  **The browser-facing seam landed 2026-08-04, with one ruling.** Serving a
  staged blob needs an `await`, and endpoints cannot: `PresentationSource` is
  synchronous and `dispatch_common` runs inside the async session loop. Three
  ways out were weighed (async `resource_chunk` on the trait; an async hook
  beside the endpoint in the session loop; the host pre-loading bytes the sync
  endpoint then serves). **Ruled with Mark: pre-load, with an explicit
  ceiling.** It matches the pattern already there, since supplemental cards are
  precomputed by the async host and served by the sync endpoint, and it commits
  to no architecture before a large transfer has proven one necessary. The
  async-trait refactor is the answer when a real transfer exceeds the ceiling.

  **Authorization is an explicit release set, also ruled.** An admitted browser
  knowing a hash is not authorization to read the bytes behind it. Only blobs an
  accepted transfer releases are servable; an unreleased hash is refused whether
  or not the device holds it. Held in their own map, apart from the identity
  resources a projection refresh replaces wholesale, so "which bytes was this
  browser granted" has an answer. `retire_released` revokes.

  `MAX_RELEASED_TRANSFER_BYTES` (64 MiB) refuses rather than truncating: a
  partial release would look to a browser like a transfer that worked.
  `resource_chunk` is overridden on `IdentityEndpoint` because the default
  re-reads the whole resource per chunk, so serving N pieces would copy the
  blob N times; `ResourceChunkResponse::from_slice` takes it by reference.
  `BlobStore::read_range` seeks rather than re-reading, sized from
  `blobs().status()`, and treats a partially fetched blob as absent because its
  content has not been verified as a whole.

  The broker's `DeviceSupplementalCards` became `DeviceSurface`, carrying cards
  and released blobs together: both are refreshed by the same host and read at
  the same moment, and a second `Arc<RwLock<_>>` per kind is how plumbing
  multiplies. The card refresh updates only `cards`, so a projection change
  cannot revoke a grant. The surface is read once at session start, so a
  transfer accepted mid-session becomes pullable on the next one, which is the
  timing cards have always had.

  **The accept gesture landed 2026-08-04.** A transfer offer already rendered
  as a card, because its carrier node is an ordinary graph node and the
  `mere.graph` adapter cards every projected node. What was missing was an
  action, and supplemental cards were action-free by construction.

  That door is now open narrowly rather than removed: `SupplementalCard.actions`
  defaults empty, and only the offer card fills it. Keeping the default empty
  is what stops "supplemental" from becoming a second, unaudited action
  surface. The transfer id is bound into the action's payload when the card is
  composed, so the decision names one transfer rather than "the" transfer,
  which would be ambiguous the moment two are waiting.

  A supplemental card's intent is routed away from `PersonaeHost::apply_intent`
  explicitly. Sending it there would either fail confusingly or grow into a
  path where a composed card can reach the vault.

  **Accepting records a decision; it does not perform one.** The gesture is
  answered synchronously and fetching bytes is not, so `invoke` pushes a
  `TransferDecision` onto a queue and returns. What the person agreed to is
  durable the moment they agree; the work it implies happens in
  `spawn_accept_watch`, where awaiting is possible. Accepting is idempotent by
  transfer id, so a double-click or a browser retrying after a dropped reply
  does not queue the same transfer twice. A failed accept is logged and
  dropped rather than retried forever: the offer stays on the graph, so the
  person can accept again once whatever blocked it has changed.

  The queue is a `std::sync::Mutex` inside `DeviceSurface`, so cloning the
  surface for a session hands that session the same queue the host drains
  rather than a copy. The watcher takes the queue handle out before locking
  it, so the surface lock is never held while the decision lock is.

  Receipts: `accepting_a_transfer_records_one_decision_and_never_reaches_the_vault`
  covers advertisement gating, idempotence, and refusal on a card that did not
  advertise it; `accepting_without_a_place_to_record_it_is_refused` covers a
  surface composed without a queue, which must refuse rather than report an
  accept nothing will act on.

  Still open in S3: the browser extension's own UI for the action, which is
  outside this repo. Then the recorded
  `navigator.storage.persist()` grant, promotion of destination staging into a
  recovery pin, and the blob-reference-set intent pair that pin release
  depends on. Also unbatched: staging authors one operation per blob, which is
  fine for a handful and worth batching before a transfer carries many.

**Not in scope:** cross-persona copy authorization UX; the carrier seam
plan's C3 (independent, about projection sessions); Turnstone (holds no
MereHost; revisit after the carrier plan's C2 rules on where the session
machinery lives); a hocket-handoff-style signed offline envelope (manifest
plus bytes in one addressed artifact) as a carrier-free fallback, noted for
later.

### H7. Add continuous personal-device sync

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

Only user-selected public identity projections may ride this generic graph
sync. Credential slots, seeds, credential-vault root keys, private epochs, and
decrypted payloads are categorically excluded. Personae carry and secret sync
remain a separate high-assurance lane with their own device enrollment,
wrapping, revocation, and recovery receipts.

**Done when:** two offline devices edit tags/relations and record accesses,
reconnect, converge deterministically, expose any unresolved domain conflict,
and retain per-device chronology without last-writer loss.

**H7a receipt (2026-07-29):** the native personal-sync core implements a
selected, secret-free Graphshell event grammar over p2panda operations,
Stickleback policy-before-storage intake, and LogSync. Two partitioned
in-process peers converge tags, one relation, two source-attributed access
histories, a selected facet, a saved scene, and a handler preference. A
separate concurrent scalar edit remains visible as a conflict. Arbitrary Mere
facets travel through the ordinary `GraphDelta` capture/replay journal, and
Commons-spine consumes the promoted Stickleback writer binding while Knot
retains its text-specific fold. See the
[H7a personal-sync core receipt](../../../ports/graphshell/docs/2026-07-29_h7a_personal_sync_core_receipt.md).

**H7b receipt (2026-07-29):** the resident device host owns a Personae-bound
Redb replica and LogSync session; explicit close/reopen retains the projection
and author head. Selected blob availability folds independently of graph
metadata and carries no bytes. The browser receives public sync cards only
through its existing challenge and `SessionHello`-admitted device session.
Windows and <remote-host> each reopened independent offline edits, exchanged endpoint
tickets in both directions, and converged to byte-identical receipts retaining
both tags, both chronological access records, one relation, the explicit title
conflict, both blob locations, four writer heads, and zero pending history.
See the
[H7b personal-sync closure receipt](../../../ports/graphshell/docs/2026-07-29_h7b_personal_sync_closure_receipt.md).

### H8. Add the Retinue agent and constrained profile

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

### H9. Open the same surface to applications and AI

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

### H10. Surface the local network as nodes

Mark's ask, 2026-07-27: "I'd like to be able to easily find printers and
surface them as local network devices in my mere."

**Two capabilities that share a protocol and nothing else.** Conflating them is
the main risk in this item:

1. **Peer discovery.** p2panda's `MdnsDiscoveryMode`, already wired in
   `mere-transport::P2pandaTransportBuilder::mdns` and exercised in that
   crate's own tests, but **used nowhere in production**. Turning it on lets one
   Graphshell find another on the LAN without a pasted ticket, which is what
   closes G5's "discovery **or** ticket exchange" clause. Small: a builder call
   plus a connect path that waits for the address book to populate.
2. **Service browsing (DNS-SD).** Finding `_ipp._tcp` printers, `_http._tcp`,
   `_smb._tcp`, AirPlay, and friends. **No crate in the tree does this today** --
   p2panda's mDNS finds p2panda peers, not arbitrary services. This is the half
   that makes a printer a node, and it is new dependency surface.

**What a discovered service becomes.** One Container per service instance,
carrying service type, instance name, host, port, addresses, and TXT records.
Its provenance is *observed on this interface at this time*: a device's TXT
records are that device's claims about itself, not facts, and the node must not
present them as though the graph asserted them. A device seen on two networks
is not the same node unless something stable says so.

**Owner-controlled, defaulting off.** Browsing is passive but not invisible: it
puts queries on the wire and builds a picture of a household. Announcing,
browsing, and which service types are surfaced are three separate settings,
consistent with this plan's rule that discovery, service admission, and transit
stay independent axes.

**Handlers.** A printer node's default handler opens its `ipp://`/`ipps://`
address through section 3's handler routing. That is the payoff: the device is
reachable *from* the graph rather than merely listed in it.

**Done when:** a real printer on the LAN appears as a node with its address and
advertised capabilities; a second run recognises it as the same remembered
device rather than minting a duplicate; the node greys when it stops responding
instead of silently persisting as live; and browsing can be turned off with
nothing left listening.

**Stop rules:** do not promote a TXT record to a graph assertion; do not
identify a device across networks without a stable identifier; do not let
service *browsing* imply service *admission*, which stays Notochord's.

#### Measured 2026-07-28: peer discovery works locally and not across the LAN

`g5_peer --discover` dials a peer id known in advance and adds nothing to the
address book, so a successful dial means mDNS resolved the address by itself.

- **Same machine: works.** Two processes on the Windows laptop completed the
  full three-session flow with no ticket and no `add_peer`.
- **Across the LAN: fails, both directions.** Windows to iMac and iMac to
  Windows both end in `address book does not have any iroh address info for
  node id ...`, consistently, with the server up for 12s, 37s, 62s and 87s. Not
  a timing problem.

So the code path and API use are right, and something between the two hosts is
not carrying it. Two facts narrow it further: the Windows OS resolver resolves
`<remote-host>.local` to <private-address> without trouble, so basic mDNS is not blocked
outright; and the Windows host is multi-homed (three Wireless LAN pseudo
adapters, Wi-Fi, Bluetooth PAN, Teredo). A userspace mDNS socket joining the
multicast group on the wrong interface is the leading hypothesis and matches
both the same-machine success and the two-way cross-host failure.

**Measured 2026-07-28. Two earlier conclusions here were wrong.** The network
was blamed and the network is fine. What follows is instrumented rather than
inferred.

- **Multicast crosses both ways.** A neutral group (239.255.42.99, TTL 1) sent
  from Windows reached the iMac 7 times in 8, and from the iMac reached Windows
  8 in 8. Link-local multicast is carried between these hosts.
- **Windows userspace can receive real mDNS from the wire.** A socket bound to
  5353 with `SO_REUSEADDR`, joined to 224.0.0.251, received live mDNS from
  <private-address> and .27 within seconds. Port contention with the OS responder is
  not the problem.
- **The firewall permits it.** Two enabled Private-profile rules named
  `g5_peer.exe` allow inbound UDP and TCP for exactly that binary path.
- **Both peers advertise.** p2panda calls `.advertise(mode.is_active())` and
  both sides ran `Active`.
- **And nothing is on the wire.** With the iMac peer serving continuously
  (confirmed still running afterwards), Windows saw **zero** packets containing
  `irohv1` in 40s of IPv4 mDNS and 25s of IPv6 mDNS. IPv6 mDNS is nearly silent
  on this network anyway: 2 packets total.

**Conclusion: the announcement never reaches the LAN.** It is visible only to
sockets on the same host, which is why the same-machine ticketless connect
completes the full three-session flow while both cross-host directions fail.
That is a send-side interface problem inside
`iroh-mdns-address-lookup`/`swarm-discovery`, not multicast handling by the
access point, not IPv4-versus-IPv6, not the firewall, and not Mere's code.
`IpClass::Auto` with no interface pinning remains the most likely mechanism,
and p2panda exposes no knob for it.

**Measured 2026-07-28, later the same day. The conclusion directly above is
wrong, and so is the instrument that produced it.** Three hosts were used this
time (Windows, the iMac, and the Fedora ThinkPad as a neutral observer), with a
raw multicast socket rather than avahi or `dns-sd`, so nothing depends on a
service-name filter being right.

- **The service name is `p2pandav1`, not `irohv1`.** `iroh-mdns-address-lookup`
  defaults to `irohv1`, but p2panda overrides it; a runtime probe reports
  `service=p2pandav1`. Every earlier capture filtered for a string this stack
  never emits, which is why "zero packets" was recorded. `avahi-browse` also
  fails to surface the service even when the records are demonstrably on the
  wire, so it is not a usable instrument here either.
- **Windows advertises correctly and reaches the LAN.** From the Fedora box,
  37 to 60 packets per 20s from <private-address>, alternating queries and
  well-formed responses (`QR=RESPONSE`, `an=4`, `ar=4`) carrying instance,
  hostname and TXT. The announcement leaves the host.
- **The advertised id is the right id.** The instance label base32-decodes to
  exactly the node id the dialling side seeks, so the `G5_PEER` derivation was
  never at fault.
- **The iMac receives and surfaces those records.** 24 to 28 discovery
  callbacks in 22s, on stock upstream crates.
- **The iMac's egress works. Its responder stalls.** An earlier reading here,
  that the iMac never puts records on the wire, was an artefact of arming the
  listener too late. With the listener armed first, Fedora received 34 packets
  from <private-address> in one window, including its own
  `tavio6zxv..._p2pandav1._udp.local` instance. Instrumented `swarm-discovery`
  shows every `send_to` returning `Ok` with the full byte count from
  `0.0.0.0:5353`.
- **What actually fails is that the sender stops.** Runs are not deterministic.
  One run sent 14 then 36 packets over six seconds and stayed healthy; another
  sent exactly 12 (6 queries of 39 bytes, 6 responses of 596) and then emitted
  **nothing for the next 20 seconds**, including no answer to the queries
  Windows was actively sending throughout. Windows by contrast sustains 37 to 60
  packets per 20s. A stalled responder is invisible and unreachable, which is
  why `--discover` fails while a ticket works.
- **Two mechanisms are ruled out.** `IpClass::Auto` reports `v4=OK v6=OK`, so the
  `.ok()` error-swallowing path is not taken. And an unsigned third-party C
  binary replicating the exact socket setup (bind `0.0.0.0:5353`, join
  `224.0.0.251`, no pinning) reaches the wire from this host, so neither macOS
  Local Network privacy nor binary identity is responsible.
- **Interface pinning is NOT a dead hypothesis. That earlier note was wrong and
  is retracted.** It was dismissed on an A/B that measured whether packets
  reached the wire, when the thing it fixes is *receive* membership. Measured
  2026-07-29 on Windows with a raw listener: a default join to `224.0.0.251`
  lands on the WSL/Hyper-V adapter `<private-address>` and sees **no LAN traffic at
  all**; a join pinned to the Wi-Fi address `<private-address>` immediately receives
  it. With `with_multicast_interfaces_v4` supplying the interface list,
  `join_group_on_main_v4` reports `JOIN OK on interface <private-address>` (and a
  harmless `JOIN FAILED ... os error 10022` on the virtual adapter). That is a
  real bug on any multi-homed Windows box, worth landing on its own merits as a
  `mark-ik` fork patched by branch. It is necessary, not sufficient: Windows
  still discovers only itself, because the iMac is not reaching the wire.

**The blocker for `--discover` is a client-side race.** In `connect` mode,
`spawn_discoverer` runs *after* session 1 begins: p2panda builds the endpoint
and its mDNS actor lazily on the first dial, then reads the address book
synchronously, so discovery has no opportunity to answer. It wins that race on
loopback and loses it on any real link, which is exactly the same-machine
success and two-way cross-host failure. The earlier "not a timing problem" note
measured *server* uptime, which the race is indifferent to; the race is on the
client, which always dials immediately after starting. `g5_peer` now forces
endpoint construction with `carrier.ticket()` before dialling and retries to
`DIAL_DEADLINE`.

**Everything above the transport is proven across two machines.** Mac to
Windows over a hand-carried ticket completed the full flow: three admissions,
snapshot, suspend, a reconnect resumed by replaying 2 contiguous diffs
(revisions 1->2, 2->3), `intent Accepted`, close. G5's cross-machine done-when
is met by the ticket path.

**The sender does stall on macOS, and the loopback test proves it with the
network removed.** An instrumented five-minute soak ran perfectly steadily (78
`send_msg` calls per 30s, no guardian exit, no receiver death), which briefly
looked like a refutation; that was simply a healthy run. The decisive instrument
is to have the iMac listen to **itself**: multicast loopback is on, so a sending
peer sees its own records without any AP, band, or mesh involvement.

- A freshly started peer emits **40 packets in 15s** to loopback, well-formed,
  instance `tavio6zxv...`.
- Two to three minutes later the same process, still alive and still printing
  `waiting for session 1`, emits **zero** to loopback over 15s.

No network can explain that. `swarm-discovery`'s sender simply stops. Use the
loopback listener to reproduce it: it is cheap, needs one host, and removes
every variable that produced wrong answers earlier in this section.

**A "C reaches the wire, Rust does not" reading was recorded here and is
retracted.** It came from runs minutes apart on a link whose delivery varies
over tens of seconds. Interleaved inside one window, both arms delivered, and on
another occasion the verdict inverted entirely. When the channel itself is
intermittent, arms must be interleaved in a single window or the comparison is a
coin flip. Language, runtime, binary identity, code signature and macOS Local
Network privacy are all excluded.

**Disabling AWDL did not fix it.** `sudo ifconfig awdl0 down` (flags drop from
`8843<UP,...,RUNNING,...>` to `8802`) left ticketless discovery failing, so AWDL
time-slicing is not the explanation, or not the whole one.

**Linux is healthy.** Fedora's `g5_peer` put **64 packets in 25s** on the wire,
full `_p2pandav1._udp.local` records, observed from the iMac. Whatever this is,
it is not the crate on every platform.

**The stalled peer ignores a direct query, so the actor tree is down, not a
timer.** A stalled peer was poked with five PTR queries for
`_p2pandav1._udp.local` and answered none, while both processes stayed alive. A
query arrives as `MdnsMsg::QueryV4` and is answered irrespective of any timer, so
its receiver is not delivering either. That excludes timer coalescing, App Nap
and any sender-only parking. `caffeinate -i -s` also failed to prevent the stall,
though note it suppresses sleep rather than App Nap, so it was the wrong lever
rather than a refutation.

**The mechanism that fits every symptom** is `guardian`'s supervision loop: it
`break`s, tearing the entire service down, when *any* supervised actor stops, and
announces that only through `tracing::warn!`. `g5_peer` installs no subscriber,
so the message goes nowhere. Alive process, no announcements, no query answers,
no error output: all of it falls out of that one line.

**REFINED, same day, after catching it live with tracing on:** the flap is a
*trigger*, not the boundary of the bug. A fresh PR-#7 peer on a stable network
was watched announcing (34 packets/12s on loopback), and at t+2m32s its only
IPv4 interface socket began returning the same errno on every send —
`error sending mDNS on interface <private-address>: No route to host` then
`failed to send mDNS on any IPv4 interface in multi-interface mode` — and never
recovered (+28 failures per 10s, indefinitely), route table healthy throughout.
In the same minutes, an unpinned C sender on the same host delivered to Windows
(8 packets observed in a properly overlapped window) while the peer's socket
failed every send.

The unifying statement, which fits every observation across both days: **on this
iMac, some routine Wi-Fi state transition (every few minutes on the eero mesh;
always on a link flap) permanently wedges UDP multicast sockets that existed
before it — subsequent sends return `EHOSTUNREACH` forever — while sockets
created after the transition work.** Every ad-hoc probe (C, Python) used a
fresh socket and so always "worked"; every long-lived peer socket eventually
wedged. `swarm-discovery` keeps its sockets for the life of the process and
treats send errors as log-and-continue, so one transition silences a peer
permanently. PR #7's netwatch arm does not help: it reacts to interface
add/remove, and the interface set never changes.

The upstream-shaped fix is socket re-creation: on persistent send failure (or
on link/route events, not just interface add/remove), rebuild the socket and
re-join the group. Nobody has written that yet, in either crate.

**FINAL, later the same day: socket age was also wrong. The denier is macOS
per-binary local-network policy, and no socket-level fix can work.** The
rebuild fix was written (mark-ik/swarm-discovery branch `mere`), deployed, and
fired 272-350 times against a live wedge: every freshly rebuilt socket failed
identically, killing the fresh-socket theory. The decisive split, same host,
same SSH launch context, same socket shape, same seconds: an Apple-signed
`/usr/bin/python3` sender ran 72/72 successful multicast sends over six
minutes, while `g5_peer` hit `EHOSTUNREACH` on its first send and logged 1130
errors. The denial follows the *binary*: an ad-hoc-signed copy at a fresh path
died at t=32s, and the same binary as a launchd user agent died at t=32s. The
System Settings > Privacy & Security > Local Network panel lists nothing to
grant: on Sequoia, processes without a responsible GUI app can be denied
without ever becoming grantable. The earlier grace periods (2m32s of healthy
announcing before death) match asynchronous policy evaluation; the eventual
instant-death matches a cached deny; every short-lived probe binary finished
inside the evaluation window, which is why probes kept "refuting" the peer's
failure.

Consequences:
- The socket-rebuild branch stays but is NOT upstreamed; its premise did not
  survive contact. It is harmless (tests pass, rebuilds are defensive) and the
  committed manifest keeps it only because the branch also pins the fork pair.
- **Product requirement, not a dev quirk: a macOS resident Mere peer must ship
  as a signed app with the local-network usage declaration, or it will be
  silently denied multicast egress with no user-visible recourse.** An unsigned
  dev binary under sshd or launchd is not a viable macOS peer.
- Dev-box paths still open, untested: running the peer once from Terminal.app
  (a responsible GUI app makes the prompt possible), a real Developer ID
  signature, or `sudo log stream` on the NECP/TCC subsystems during a death to
  capture the verdict line naming the policy.

**Ticketless discovery receipts on the committed fork stack (2026-07-29):**
Fedora -> Windows and Windows -> Fedora each completed the full three-session
flow (snapshot, suspend, resume replaying 2 contiguous diffs, intent accepted)
with no ticket and no `add_peer`. Windows -> Fedora is the direction that
requires the multi-homed receive fix, so PR #7's adoption is validated in both
roles. Mac -> Windows also passed earlier in the day; only Windows -> Mac
remains blocked, by the macOS policy above, since the Mac cannot be heard while
denied egress.

Also observed on the fresh peer: every IPv6 send fails the same way from
process start (`error sending mDNS on IPv6: No route to host`), hours after the
flap, so the host-level IPv6 multicast state never recovered. Separate, lower
priority; IPv4 carries discovery here.

The original account of the flap experiment follows, kept because its
measurements stand:

**2026-07-29 (earlier): the multicast socket does not survive a link flap.**
Toggling the iMac's Wi-Fi off and on, with a peer running under `RUST_LOG`,
reproduced the stall immediately and named it:

```
WARN swarm_discovery::socket: error sending mDNS: No route to host (os error 65)
```

`errno 65` is `EHOSTUNREACH`. The socket was created and joined `224.0.0.251`
while the interface held its old state; after the interface went down and came
back, that binding is stale and every send fails forever. Measured after full
recovery, with `en1` holding `<private-address>`, the default route via
`<private-address>`, and both `224.0.0/4` and `224.0.0.251` routing to `en1`: **1732
accumulated errors and 42 more in 15 seconds.** Routing is correct; the socket
is dead. `swarm-discovery` never re-creates it or re-joins, because it does not
watch interfaces.

This accounts for every observation. The process stays alive and keeps printing
`waiting for a peer...` because the sender loop is healthy at roughly 2.8 calls
a second; it is only the socket underneath that is dead. Loopback sees nothing
because the send fails before any delivery. Direct PTR queries go unanswered
because response sends fail identically. And it explains the correlation with
the failing Wi-Fi.

**The "heisenbug" was a coincidence, and the retraction is worth keeping.** Four
instrumented runs and one subscriber run never reproduced it, which looked like
probes suppressing a timing bug. They did not: every instrumented run happened
to be on a stable network. Nothing about instrumentation was ever relevant.

**The fix already exists upstream.** `n0-computer/iroh-address-lookups` PR #7,
"maintain multicast sockets for all usable IPv4 interfaces", watches the host's
interfaces via `netwatch` and adds and removes multicast sockets at runtime as
interfaces come and go, which is exactly what a link flap needs. Its companion
is `rkuhn/swarm-discovery#25` (pin egress with `IP_MULTICAST_IF`, join per
interface). Track PR #7's branch rather than authoring a fork; the existing
`mark-ik/iroh` fork does not contain this crate, which lives in its own repo.

Superseded hypothesis, kept because the reasoning still reads plausibly and the
code path is real: an emptied address set.

1. `iroh-mdns-address-lookup` handles every address change by calling
   `address_lookup.remove_all()` **first**, then re-adding whatever the new
   `EndpointData` contains (`lib.rs`, the `addrs_change.updated()` branch).
2. `RemoveAll` in `swarm-discovery`'s sender does
   `discoverer.peers.remove(&discoverer.peer_id)`, dropping the **local** peer.
3. With no local peer, `make_response` returns `None`, logging that line.
4. A peer in that state announces nothing *and* answers nothing, because the
   response loop only sends `if let Some(response)`.

If the new address set is empty, as it is while the link is down, the peer is
left permanently silent: nothing restores it until another address change
arrives. Alive process, no announcements, no query answers, no error output.
That is every symptom, and it explains the poke result without requiring any
actor to have died.

**The timing correlation supports it.** Both clean reproductions happened around
the period when the Wi-Fi was failing. Since the network was restored, the stall
has not reproduced once in roughly 40 minutes across five runs, including fully
stock builds with no instrumentation at all in the exact configuration that
stalled twice before.

**Caveat: this is inference from code plus correlation, not yet a controlled
reproduction.** The test that would settle it is to flap the iMac's link (Wi-Fi
off, a few seconds, on) while a peer runs under `RUST_LOG=info`, and check for
`no addresses for peer, not announcing` followed by permanent silence. That
needs someone at the machine, since it is a system network action.

**Do not try to catch this with in-process instrumentation.** Four instrumented
builds and one `warn`-level subscriber run, roughly 40 minutes total, never
reproduced it. Installing any subscriber also swaps tracing's global dispatcher
from a no-op to a real one, so even `warn`-only filtering adds work at every
`debug!`/`trace!` site in the hot path. Observe from outside the process
(loopback listener, then `sample <pid>` once it is already silent).

**Benign, but worth an upstream note:** the stale-timer race is real and fires
regularly, always off by one and always in the response loop
(`RESP-LOOP dropped stale timeout 278 (want 279)`). Peers ran straight through
it, so it is not this bug.

Two latent hazards were read in `swarm-discovery` while looking, neither
observed firing: `guardian` breaks its loop and tears down the whole service
when *any* supervised actor stops, reporting it only at `tracing::warn`; and the
sender drops a stale `MdnsMsg::Timeout` whose count no longer matches, which
would leave it awaiting a timer that will never arrive.

**Method note for whoever picks this up: arm the observer before starting the
peer, and A/B on the host that exhibits the fault.** Most of the wrong turns in
this section came from sniffing a window that had already passed, or from
measuring egress when the fix was about reception. One
unexplained one-off is worth recording: a ~10 minute stale ticket produced
`Delegation(NotYetValid)` although all three clocks agree to 0.1s, and it did
not reproduce with a fresh ticket.

**Consequence for the printers half.** Even the "easy" capability does not
deliver a device list: `mere-transport` exposes no way to enumerate what
discovery found, so mDNS resolves an address for a peer id you already hold. A
device list needs a new accessor on the transport, and a *service* list needs
the DNS-SD browser this item scopes.

## 10. Settings that remain user-controlled

- captured browser APIs and origins;
- private-window behavior;
- history retention and redaction;
- synchronized facet namespaces;
- identity projections visible per persona and application;
- vault backend and startup unlock mode;
- signing approval mode, remembered scope, and expiry per key/adapter;
- retention and redaction of signing records;
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
3. real vault-held SSH signing through the Graphshell resident host;
4. per-use approval/denial, short-TTL relock, and secret-exclusion audit;
5. consented extension capture and forget;
6. two-device admitted transfer, interruption/resume, and revocation;
7. offline convergence;
8. real Retinue agent management;
9. measured RF constrained transfer.

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
6. Which non-Windows OS-protected vault backend follows DPAPI. Until it lands,
   expose the portable passphrase backend as the honest profile rather than
   claiming equivalent auto-unlock.
7. How much caller and target context each SSH-agent carrier can authenticate.
   The approval UI degrades to explicit unknown fields; it does not manufacture
   attribution.
8. Which identity facet vocabulary promotes out of Graphshell after a second
   application consumes it. Keep the first projection local and delete it when
   the shared replacement lands.
9. Whether the personal lane gets per-recipient encryption. Today it has none,
   so the roster is its only confidentiality boundary and every admitted device
   can read everything (made explicit while building H6's S2). This is
   tolerable for facets a device already syncs, and it is worth deciding
   deliberately before S3, because the blobs a manifest names carry file
   contents and any paired device can already fetch any advertised blob by
   hash. Knot's `KnotSyncCipher` over a `DataKeyring` is the pattern to apply;
   the cost is key distribution on pairing and re-encryption on roster change.
   Do not let a projection filter stand in for this: filters decide what a
   device shows, not what it can read.

None of these decisions changes the product boundary or requires a new
repository.
