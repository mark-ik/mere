# Graphshell Reference Host Plan

**Date:** 2026-07-27
**Status:** product boundary ruled with Mark; H0-H3 complete; H4 in progress; H5-H9 not started.
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
untouched. Native picker wiring, admitted browser access, standard-endpoint
cutover, restart/login and lifecycle proof, carry mutations, and mixed-scene
reopen remain H4 work. See the
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

### H6. Move one selection between two devices

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
`Q-PC.local` to 192.168.4.105 without trouble, so basic mDNS is not blocked
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
  192.168.4.26 and .27 within seconds. Port contention with the OS responder is
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

None of these decisions changes the product boundary or requires a new
repository.
