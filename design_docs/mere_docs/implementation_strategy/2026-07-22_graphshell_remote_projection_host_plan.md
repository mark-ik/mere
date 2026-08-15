# Graphshell Remote Projection Host Plan

**Date:** 2026-07-22
**Product scope amended 2026-07-27** by the
[Graphshell reference host plan](2026-07-27_graphshell_reference_host_plan.md):
Graphshell is Mere's WASM-safe local reference host as well as its remote
projection port. The session stack, disclosure boundary, and G-series receipts
below remain; the claim that Graphshell's only local truth is remote-scene
curation does not.
**Superseded in part 2026-07-23** by the
[repo consolidation plan](2026-07-23_repo_consolidation_plan.md): Graphshell
is ruled Mere's shell and remote port, and the session protocol is Mere's
session grammar; the five crates move into the mere repository with their
dependency walls intact as CI checks. Section 2's "must not depend on Mere"
survives as a crate-linkage rule (the portable crates do not link the kernel
or products), not a repository rule. Section 3's repository layout is
superseded. Section 9's murm/moot promotion row is withdrawn and the
audio-primitives row deferred to a git pin. The four protocol planes, the
disclosure rules, the G-series proof sequence, and all landed receipts remain
in force unchanged.
**Status:** the local G0 boundary is sealed; G1's loopback presentation, G2's
diff/resume/persistence, and G3's real Turnstone endpoint proofs are complete as
of 2026-07-22. The Graphshell workspace is published on the existing
`mark-ik/graphshell` repository; its retired browser donor remains available in
the same Git history. G4, the already-proven Isometry projection, is next.
Graphshell is ruled as the Merely family's remote projection host. It is
neither the projection engine nor Mere's internal chrome layer. This plan
remains the cross-repository roadmap; Graphshell's README owns the live package
boundary.

**Companions:** the
[projection-engine prior-art brief](../research/2026-07-21_projection_engine_prior_art_brief.md),
the [projection proofs plan](2026-07-21_projection_proofs_plan.md), the
[participant gate and packs plan](2026-07-17_participant_gate_packs_plan.md),
the [one-node layer map](../technical_architecture/2026-07-18_one_node_facets_layer_map.md),
and the
[Murm peer-runtime and Moot-domain plan](2026-07-12_murm_peer_runtime_and_moot_domain_plan.md).

## 1. Product ruling

Graphshell is one web-first client for projections served by applications
running on the user's own devices. Turnstone, Woodshed, Isometry, Hocket, and a
radio-management application remain the authorities over their native data.
They expose granted projection sessions. Graphshell discovers those sessions,
opens saved views, realizes their scenes, and sends typed intents back.

Scenograph is the engine underneath this exchange:

```text
application truth + signals + granted score
    -> application adapter
    -> scenograph projection and runtime
    -> scene snapshot + diffs + presentation resources
    -> Graphshell
    -> typed intents back to the owning application
```

"Thin" describes Graphshell's authority boundary. The client may cache scenes,
run client-side arrangement, compose several remote scenes, manage resources,
and keep substantial local UI state. It does not acquire the source truth or
the right to mutate it directly.

The Graphshell workspace is real local truth, but it is **curation truth**:

- known application endpoints and device labels;
- saved scores and projection subscriptions;
- top-level composition of remote scene fragments;
- local visibility, pins, camera, representation preferences, and layout;
- local curation links between remote source references;
- cache and offline-retention policy.

A curation link can remain a Graphshell-only relationship. Turning it into a
Woodshed relationship, an Isometry game fact, a Retinue device setting, or a
Mere graph assertion requires an explicit intent accepted by that application.

## 2. Ownership model

| Layer | Owner | Rule |
|---|---|---|
| Source data and domain relationships | Each application | Graphshell receives stable references and disclosed projections, never the store itself. |
| Projection contracts | `sceno` | Scores, source references, spaces, footprints, scene snapshots, and intent references stay product-free. |
| Placement solvers | `scenomise` and `conatus` | `scenomise` owns analytic arrangements; `conatus` owns field evaluation and dynamic physics. |
| Incremental projection runtime | `scenotime` | Dependency tracking, generations, scene diffs, caches, and reverse intent resolution. Networking stays outside. |
| Remote session protocol | Graphshell | Versioning, subscriptions, resume, presentation offers, resource transfer, intent results, and status. |
| Projection endpoint | Each application over Graphshell's endpoint crate | Adapts native truth to scores and scenes, authorizes disclosure, maps returned intents to native actions. |
| Presentation | Graphshell plus codec adapters | Resolves glyphs, cards, sprites, snapshots, and live panes without teaching `sceno` how to render. |
| Authority | The application | Personae, local grants, and the application's gate decide what may be read or changed. |
| Carrier | Murm peer transport, browser carrier, loopback, or constrained adapter | Carries framed Graphshell messages without defining their meaning. |

Graphshell must not depend on Mere, Woodshed, Isometry, Hocket, Retinue, Burn,
or an application graph kernel. Product adapters depend on the Graphshell
endpoint contract in the other direction.

## 3. Repository and crates

Create a fresh sibling repository at `Code/repos/graphshell`, MIT OR
Apache-2.0, edition 2024. Start with four packages:

1. **`chirograph`**: wasm-clean message envelopes, versions,
   capabilities, projection subscriptions, resource manifests, intent
   invocations, and status. Depends on `sceno` and the diff vocabulary from
   `scenotime`; it has no renderer, transport, identity backend, or product
   dependency.
2. **`graphshell-client`**: the transport-independent client state machine,
   workspace curation, snapshot/diff application, resource cache, resume and
   stale-state behavior, local scene composition, and semantic hit results.
   Storage and carriers enter through traits.
3. **`graphshell-endpoint`**: the server-side session state machine used by
   applications. It evaluates granted subscriptions through injected source,
   authorization, and intent-sink traits. It does not import a particular gate
   or product action enum.
4. **`graphshell`**: the actual Genet/Cambium application and wasm entry point.
   It composes `graphshell-client`, Personae identity, Eidetic storage, Cambium,
   Genet, and NetRender. Split a `graphshell-genet` adapter later only if a
   second presentation host creates real pull.

Application adapters remain with their truths. The first Turnstone adapter lives
in `repos/turnstone`; an Isometry adapter lives in `repos/isometry`; the same rule
applies to Woodshed, Hocket, and radio management. An adapter is not a new repo.

The retired browser donor occupied the `graphshell` GitHub name. Publication
kept that repository and joined the new workspace to its history rather than
renaming or force-replacing it. The donor's source and design documents remain
recoverable from Git; they do not appear in the active tree. This preserves old
citations as historical evidence while making the clean portable workspace the
repository's current `main`. Keep the crates.io `graphshell` name for the new
application/facade.

## 4. Four protocol planes

### 4.1 Session plane

The session plane establishes what each side can speak and what the client may
request. Its minimum vocabulary is:

- protocol version and supported scene/diff versions;
- endpoint identity, application kind, instance label, and availability;
- supported projection and presentation capabilities;
- authenticated principal and grant reference;
- open, close, suspend, resume, and resynchronize;
- live, stale, disconnected, expired, and revoked status.

Discovery is carrier-specific. The protocol begins once a byte stream exists.
A LAN announcement, Iroh ticket, QR code, web link, or Retinue address must all
lead to the same session handshake.

### 4.2 Projection plane

The client sends a versioned score or a reference to a saved score. The
endpoint returns either a complete scene snapshot or a diff from an acknowledged
revision.

Scene identity needs one rule before diffs are credible. `sceno::InstanceId` is
a dense index today. Graphshell adopts an **epoch plus revision** contract:

- every diff-addressed scene table gains an explicit index type;
- indexes remain stable and are never reused for the life of one scene epoch;
- additions allocate new slots and removals tombstone runtime slots; visual
  order is data, not vector order;
- diffs name their base revision and are rejected when the client lacks it;
- compaction or incompatible recomputation begins a new epoch and sends a full
  snapshot;
- diffs are idempotent within an epoch;
- reconnect resumes from the last acknowledged revision or falls back to a
  snapshot.

This preserves the data-oriented index model without pretending a vector index
is durable identity across arbitrary rebuilds.

### 4.3 Presentation plane

The current `sceno::Scene` correctly carries representation slots and
footprints rather than rendered content. Graphshell adds resolution without
putting a renderer into Scenograph:

- a projected item keeps its representation class and names an optional
  presentation key;
- a separate resource manifest maps that key to ordered representation offers;
- each offer names a versioned codec, content hash, byte size, semantics, and
  required client capability;
- resources are content-addressed, requested independently, cached according
  to disclosure policy, and reusable across scene generations;
- the client picks the richest supported offer within the score's LOD ceiling.

Initial offer classes:

1. **Native glyph**: label, icon key, color channels, accessible name, and
   intent references. Rendered entirely by Graphshell.
2. **Portable card**: a deliberately small semantic card vocabulary for labels,
   values, badges, media references, and actions. It is data, not a serialized
   Cambium callback tree.
3. **Image or sprite**: content-addressed image assets.
4. **Rendered scene fragment**: a versioned NetRender scene plus fonts and
   images, using a transport extracted from Mere's existing content contract.
5. **Live pane**: a separately negotiated interactive surface with its own
   input, focus, resize, and lifecycle channel. It is not required for the
   first remote proof.

Every offer carries at least a label, role, bounds relationship, and advertised
actions. A rendered fragment may carry a semantic tree beside its paint data.
The existing Mere `uxtree` is not adopted as the wire contract: it depends on
Inker and projects browser documents. A neutral semantic payload must be proven
by two applications before extraction into a shared crate.

### 4.4 Intent plane

Graphshell returns semantic invocations rather than application enums or raw
graph mutations. An advertised intent includes:

- a stable intent reference scoped to the projection session;
- target source or scene instance;
- a schema reference for its payload;
- the observed scene epoch and revision;
- whether it changes only curation, changes domain truth, or requests an
  external effect;
- a plain explanation suitable for permission and accessibility surfaces.

The endpoint resolves the reference, validates its schema and revision,
authorizes the principal, and hands a native typed action to the application.
The result is accepted, rejected, conflicted, or expired, with an attributed
receipt where the application gate supports one.

Pointer and keyboard events for a live pane belong to that pane's input
channel. They do not become a loophole for changing application truth outside
the intent path.

## 5. Disclosure, grants, and offline behavior

A scene is disclosed data. Authorization must happen before selection,
derivation, presentation-resource resolution, and subscription. Filtering a
scene after compilation is too late because undisclosed facts may already have
influenced labels, relationships, layout, or Burn-produced signals.

Graphshell holds a revocable grant; it cannot mint or widen one. Applications
may adapt Servitor's denizen gate, a product-specific gate, or a simpler
read-only authorizer behind `graphshell-endpoint`'s traits.

Revocation stops future disclosure and intents. It cannot retract pixels or
bytes already received. Cache policy therefore travels with the session:

- memory-only, encrypted persistent, or explicitly exportable;
- expiry and purge-on-revocation hints;
- visible source, observation time, and stale state;
- a client action to forget all cached material from one endpoint.

Disconnected scenes remain useful when policy permits. Graphshell marks them
stale and retains their last acknowledged revision. Read-only curation can
continue locally. Domain-changing intents remain staged until reconnection and
must pass the endpoint's revision and grant checks; external effects are not
silently replayed.

Burn remains behind the application adapter as a stamped signal producer.
Graphshell receives the resulting disclosed channels and provenance, never a
requirement to load the originating model.

## 6. Composing applications and devices

Each remote projection enters Graphshell as a separately keyed mounted scene.
The mount supplies its parent space and transform without rewriting the
fragment's endpoint-scoped indexes. A Graphshell workspace can then:

- arrange several fragments in a grid, spiral, board, or map;
- pin a radio-management scene over an Atlas projection;
- place an Isometry board beside a campaign reference scene;
- place a Woodshed Set beside a harmony relationship scene;
- show a Hocket phrase history beside the files or people that supplied it;
- draw local regions and curation links across fragments.

Graphshell may run `scenomise` locally for the top-level composition and for
latency-sensitive rearrangement over already-disclosed items. It does not run
an application's private selector, theory engine, game rules, graph queries,
or inference model.

The first version composes whole scene fragments. Cross-fragment relations use
stable source references plus endpoint identity. Fine-grained fusion of two
applications' projection graphs waits for a concrete product need and an
explicit disclosure contract.

## 7. Carrier profiles

The protocol is carrier-neutral, but each profile has an honest ceiling:

- **Loopback:** in-memory duplex stream. Required for deterministic tests and
  the first headed proof.
- **Full peer:** the promoted Murm peer transport over Iroh for native peers.
  The Graphshell protocol is an application protocol over its stream, not a
  Murm conversation.
- **Browser:** WebTransport, WebRTC, or a user-owned bridge remains a carrier
  decision. WebGPU rendering evidence does not prove browser peer transport.
  Claim this profile only after a headed browser-to-native session reconnects.
- **Constrained radio:** management facts, compact cards, status, and bounded
  diffs. Full NetRender scenes and live panes are outside this profile. Retinue
  can carry it only after byte budgets and fragmentation are measured on real
  links.

## 8. Implementation sequence

Graphshell does not start its implementation sequence at G0 merely because
the repository boundary has been described. The audit in §9 is a boundary map:
it names the direction and the evidence that permits a move; it is not a
cleanup queue that Graphshell waits to exhaust.

The actual gate is the projection proof sequence below. It establishes a
portable score and scene boundary with two product consumers before Graphshell
tries to use it.

### P3. Pane spiral

- Move the generic Spiral solver from Mere's `arrangements` crate into
  `scenomise` as part of the proof.
- Keep Mere's kernel graph selection, source mapping, and scene adaptation in
  `mere-cartography`; `mere-canvas` remains the product realization host.
- Prove measurement/footprint-aware placement and recency scaling.
- Realize the same projected item at glyph, card, snapshot, and focused
  live-pane LOD. The local P3 proof does not imply a Graphshell live-pane
  codec.
- Persist and reload the product-free score, then emit a headed receipt.

**Done when:** a persisted score drives a footprint-clear Mere pane spiral,
its realization changes with LOD while the focused pane stays live, and the
receipt shows the reloaded score.

### P4. Isometry two-product proof

- Adapt the overmap and one tile board to the same Scenograph score and scene
  contract used in P3.
- Delete `Overmap::layout`; do not preserve the hand-rolled layout behind a
  Scenograph-shaped wrapper.
- Keep campaign data, authored pins, rules, and paint realization in Isometry.
- Serialize the score and scene used by the receipt, then audit `sceno` and
  `scenomise` so neither contains Mere or Isometry types.

**Done when:** the two headed receipts use the same product-free serialized
vocabulary and no portable crate has either product in its dependency graph.

### Boundary consolidation

- Finish the generic-arrangements removal from Mere now that P4 identifies
  what is genuinely portable.
- Tighten `mere-cartography` and `mere-canvas` around the remaining
  graph-specific selection, adaptation, and realization work.
- Extract Cambium/Sprigging interaction only where the actual consumers agree
  on behavior and acceptance. A tempting single-product helper stays local.

**Done when:** `scenomise` owns the portable arrangements, Mere's remaining
canvas crates have explicit product responsibilities, and each promoted UI
interaction has two matching consumers.

Only then does Graphshell begin at G0. P5 follows the initial Graphshell
foundation as the geographic exercise of the consolidated boundary.

### G0. Reclaim the name and freeze boundaries

- Resolve the archived-donor GitHub name without breaking harvest citations.
- Found the clean repository and four-package workspace.
- Record dependency and license rules in its README.
- Add protocol compile checks for native and `wasm32-unknown-unknown`.

**Done when:** a fresh clone builds all portable crates without Mere, Genet,
NetRender, a network runtime, or an application dependency, and the retired
donor remains recoverable in repository history without entering the active
dependency graph.

**Published 2026-07-22:** `repos/graphshell` is an independent Git
workspace containing `chirograph`, `graphshell-client`,
`graphshell-endpoint`, and the `graphshell` facade. The protocol serializes
only Scenograph scores/scenes plus session/status/intent envelopes; client
state is endpoint-scoped scene curation; endpoint traits are injected beside
application truth. The facade intentionally contains no renderer or carrier.
Its manifest rules out Mere, Turnstone, Isometry, Genet, Cambium, NetRender, and
network runtimes. PR `mark-ik/graphshell#308` unarchived the existing repository
and made the portable workspace its active tree through merge commit
`175084d2`, while preserving the complete donor lineage. Its native workspace
tests, `wasm32-unknown-unknown` check, and warning-denying Clippy wall pass. The
boundary begins at root commit `693fad8`.

### P5. Geographic projection

- Exercise the consolidated Scenograph boundary with fixture-owned geographic
  facts and a map underlay.
- Keep map data selection and product facts in the adapter; let the portable
  score/scene express placement and representation only.
- Add Retinue/Tulle/Sennet radio location facts only after the fixture receipt
  is headed and their source contract exists.

**Done when:** a headed fixture scenario produces a geographic scene through
the same score/scene seam, and later radio facts add data without widening the
portable contract.

### G1. Prove representation resolution over loopback

- Add presentation keys and resource offers without embedding bytes in
  `sceno::Scene`.
- Implement native glyph, portable card, and image offers.
- Use an in-memory endpoint to send one snapshot to the Graphshell client.
- Exercise capability fallback: card to glyph and image to labeled placeholder.
- Project advertised actions into the client's accessibility tree.

**Done when:** one Graphshell view renders the same scene under two capability
profiles, remains usable when the richest resource is absent, and contains no
product-specific rendering code.

**Implemented locally 2026-07-22 (Graphshell `2bc5b59`):**
`ProjectionSnapshot` carries a Graphshell presentation sidecar rather than
widening `sceno::Scene`. The manifest binds
instances to ordered glyph/card/image offers; resource bytes are fetched
independently, checked by BLAKE3 address and advertised size, and cached within
the disclosing session. One in-memory endpoint resolves the same scene as a
portable card plus image under the rich profile and as a native glyph plus
labeled image placeholder under the compact profile. Both preserve their
advertised actions in a renderer-neutral accessibility tree and ordinary
keyboard-focusable buttons. The committed
`ports/graphshell/docs/receipts/g1_loopback.html`
receipt is generated from the real endpoint/client/view path and compared
byte-for-byte by test. Headed inspection passed at 1440 × 1000 and 390 × 844
with zero overflow or browser errors. NetRender fragments, live panes, intent
invocation, and Genet/Cambium application composition remain later proofs.

### G2. Land `scenotime` diffs and resume

- Define epoch, revision, snapshot, and idempotent diff types in `scenotime`.
- Apply add, update, tombstone, layer/order-field, resource-change, and status
  diffs in the client without reusing an index inside an epoch.
- Add acknowledgement, missing-base resync, disconnect, stale display, and
  resume tests.
- Persist one permitted cached scene through the injected store seam.

**Done when:** a randomized snapshot/diff oracle reaches the same final scene as
a full rebuild, and a disconnected client resumes without duplicating or losing
an item.

**Implemented locally 2026-07-22 (Scenograph `eba39e3`, Graphshell
`fb5e690`):** Scenotime now owns typed epochs, revisions, stable slot tables,
serialized tombstones, transactional idempotent diffs, and a deterministic
96-revision oracle. Graphshell applies scene, presentation-resource, and status
changes as one client transaction; acknowledges revisions; keeps the prior
display on missing-base resync; and accepts replay, current acknowledgements,
or a full epoch-preserving snapshot. The loopback resume fixture disconnects
at revision 2, replays revision 3, and matches the endpoint's complete scene
with two active items and the removed slot still tombstoned. A permitted cache
and advertised resource restore through an injected encrypted-at-rest store as
stale; memory-only policy refuses persistence. Native workspace tests, Wasm
checks, warning-denying Clippy, the G1 byte receipt, and the product-dependency
audit pass. Encryption is supplied by the injected store contract; a durable
host store, authenticated carrier, offline intent queue, and product adapter
remain later work.

### G3. Serve Turnstone through a real endpoint

- Add a Turnstone adapter over one existing score and scene path.
- Reuse Turnstone's current source references and presentation-resource work;
  decompose `content-contract` only as far as the proof demands.
- Map one harmless action and one graph-changing action through the live gate.
- Add a headed loopback scenario before adding network variability.

**Done when:** the Graphshell application displays a live Turnstone projection,
changes its local layout without changing Mere truth, and receives both an
accepted and a rejected intent receipt.

**Implemented locally 2026-07-22 (Graphshell `5f30502`, Turnstone `7996af2`):**
Graphshell's product-neutral receipt view now realizes disclosed Scenograph
item origins and relations, resolves the presentation payloads that fill those
placements, and changes to a semantic card stack on narrow screens. Turnstone is
now a library plus thin desktop binary; its endpoint reads the live Mere graph,
uses the existing Mere-cartography Spiral score and scene lowering, transfers
three content-addressed cards, and retains the two graph relationships as
routed scene relations. Both advertised actions return through one Servitor
gate. The projected `projection/layout/` grant accepts `FitView`; the
graph-changing `OpenAddress` petition under `graph/open/` is rejected, leaving
Mere graph revision 5 and its three nodes unchanged. The accepted audit commit
is attributed to the endpoint subject. Turnstone's 98 library tests and both
binaries pass; the G3 executable receipt is byte-checked; Graphshell's native,
Wasm, warning-denying Clippy, and product-dependency walls pass. Headed checks
at wide and narrow browser sizes found keyboard-reachable actions, responsive
collapse, no horizontal overflow, and no browser errors. This remains a local
loopback proof over the Spiral score. The authenticated carrier, negotiated
grants, revocation, diffs, and durable host store remain later work, and the
committed Turnstone Git dependencies now resolve from Graphshell `main` at
`175084d2`; `cargo check --lib` passes with Turnstone's local Graphshell patch
disabled.

### G4. Serve the already-proven Isometry projection

- Add an Isometry endpoint for the overmap and one tile-board scene.
- Drive it from the same serialized score vocabulary used by Turnstone.
- Keep Isometry rules, campaign data, and rendering adapters inside Isometry.
- Run the browser-sized Graphshell view as an Isometry player surface.

**Done when:** the same Graphshell binary switches between Turnstone and Isometry
sessions without either product in its dependency graph, proving endpoint and
client neutrality over the P4 vocabulary.

**Implemented locally 2026-07-22:** Graphshell now owns a product-neutral local
stdio carrier, endpoint discovery catalog, and generic session switcher.
Turnstone advertises its existing browsing-graph projection through a thin
endpoint binary. Isometry owns a new endpoint adapter over its campaign world,
overmap, and tile-board truth; both player scenes lower through the same
Scenograph score vocabulary already exercised by Turnstone. One Graphshell
process mounted all three advertised sessions from the two endpoint processes,
resolved their product-owned presentations, and returned both accepted
curation actions and rejected product actions. The committed Graphshell receipt
records the exact cross-process run. Graphshell's dependency graph contains
neither product. Focused native tests and warning-denying Clippy checks pass;
Graphshell's Wasm check also passes. A headed interaction check remains because
the execution environment exposed no controllable browser. The local carrier
remains unauthenticated, so G5 is the next proof rather than a production
transport claim.

### G5. Add an authenticated remote carrier

- Bind Personae identity and a local grant to the session handshake.
- Add one full-peer carrier through the promoted Murm transport.
- Exercise reconnect, expiry, revocation, cache purge, denied score, and stale
  intent paths.
- Prove the browser carrier separately; do not infer it from the native run.

**Done when:** two processes on different devices complete discovery or ticket
exchange, open a granted projection, resume after interruption, and reject a
revoked intent.

### G6. Compose several applications

- Persist a Graphshell workspace with at least two endpoint fragments.
- Arrange those fragments locally with `scenomise`.
- Add one local cross-fragment curation link and one explicit promote-to-domain
  action.
- Verify independent disconnect and stale state for each fragment.

**Done when:** one saved Graphshell workspace reopens two applications, keeps
their truths separate, and preserves its own arrangement and links.

### G7. Add product pulls and constrained management

- Woodshed: Set sequence, focused relationships, and circle-of-fifths views.
- Hocket: phrase history and handoff state after its arrange-view canary.
- Radio management: device status and Atlas facts through a compact profile.
- Add a live-pane codec only when one of these consumers cannot meet its task
  with cards, images, and rendered fragments.

**Done when:** each adapter has a headed or hardware-backed task receipt and the
constrained profile has measured byte budgets rather than desktop assumptions.

### G8. Host the identity agent as a resident service — FOLDED INTO H4

Written 2026-07-27 to carry Mark's 2026-07-22 ruling that **the agent's
resident home is Graphshell**, which until then lived only in the
identity-vault-ssh-agent plan's progress log and was being counted on without
being planned or assigned.

**Superseded the same day** by
[H4, "Make Personae visible and usable"](2026-07-27_graphshell_reference_host_plan.md),
which covers the same ground in more detail: the resident host moves into
native Graphshell, with an Identity surface for profiles, vault backend and
protection, agent-listener state, SSH slots, devices and grants, and pending
signing requests. Two items claiming one piece of work is how a piece of work
gets done twice or not at all, so this one defers rather than duplicating.

Three specifics from this entry that H4's file list does not spell out, kept
here so they are not lost in the fold:

- `personae::agent` is a library module precisely so a resident host can serve
  the endpoint **in-process** rather than as a separate install.
- The lifecycle the interim scheduled task currently fakes is real work: start
  with the host, survive a crash, and stop for real when the host stops. That
  task's own plan calls it "interim dogfood scaffolding only, removed when the
  host adoption lands", and retiring it belongs in the same change that
  replaces it, not before.
- `UnlockTier::PerUse` slots **refuse to sign** today, because signing silently
  would make the tier a lie. So that tier is unusable rather than merely
  unpolished until a host exists to ask, which makes the confirmation UI a
  correctness item and not a nicety.

## 9. Repository boundary audit

Repo count is not the target. Put crates together when they share release
cadence, invariants, and acceptance. Split a crate when its public contract is
stable, a second consumer exists, or license and dependency direction require
independence. Keep product adapters beside their source truth.

| Current boundary | Ruling | Trigger or consequence |
|---|---|---|
| `repos/scenograph` | **Keep and fill.** Move Mere's generic `arrangements` algorithms into `scenomise`; move only kernel-neutral scene contracts into `sceno`; build diffs in `scenotime`. | The family already has Turnstone consumption and Isometry pull. |
| Mere `canvas/{canvas,cartography,arrangements}` | **Withdraw the old wholesale-promotion plan.** `mere-canvas` is visibly Mere-specific: it owns a kernel graph, signals, seiche bridge, Genet DOM, NetRender paint, and a native bin. Keep that graph surface in Mere. `mere-cartography` becomes the Mere-to-Scenograph adapter. | Scenograph now owns the portable scene/layout boundary. |
| Shared graph-view UI | **Promote through Cambium/Sprigging only after a second consumer, initially Woodshed, agrees on the interaction contract.** Reuse camera, selection, semantic children, and scene realization; keep Mere graph mutation adapters in Mere. | Woodshed currently has its own graph swatch and is the real second consumer. |
| Mere `content-contract` | **Decompose instead of exporting.** Move NetRender scene/font/image serialization to NetRender; move generic document-worker messages to Genet; keep Mere graph contributions in Mere. Graphshell consumes the renderer wire through a presentation codec. | The crate currently depends on `document-canvas`, Mere kernel, linked-data, and NetRender, so it is not a portable remote contract. |
| `repos/conatus` | **Keep.** It already corrected the former numen/quint/seiche sibling sprawl into one physics family. `scenomise` may call its solvers without absorbing it. | Dynamic physics has a different runtime and release surface from analytic placement. |
| `repos/cambium`, `repos/genet`, `repos/netrender` | **Keep their present repo boundaries.** Cambium owns reactive UI, Genet owns document engine and host integration, NetRender owns paint realization. | Their license and dependency directions are already explicit. Graphshell composes them only in its application crate. |
| Mere `eidetic` core and backends beside `repos/eidetic` | **Move the generic typed-memory lane into the Eidetic family.** Move the `eidetic` crate, the Fjall backend under a `muniment`-appropriate name, and the generic HTTPS fetcher. Keep browsing search and the Iroh cross-family adapter with their product/integration owners until the Murm move settles. | The July 9 anti-sibling ruling predated the July 21 Eidetic family repo. The current `eidetic` crate calls itself host-agnostic and depends only on Eidetic-family primitives. |
| Mere `crates/persona/identity` beside `repos/personae` | **Delete the in-Mere duplicate after consumer checks.** The workspace already aliases `identity` to `personae`; the old package is still listed as a member but has no live consumer. | One trust root matters for Graphshell session identity. |
| Mere `murm`, `murm-replication`, transport, Moot crates | **Execute the existing promotion plan.** Create `repos/murm` and `repos/moot`; Graphshell uses the lower peer transport, not the conversation facade. | Isometry already consumes these reusable crates from the Mere repository. Graphshell adds another non-Mere consumer. |
| `repos/retinue`, `repos/tulle`, `repos/sennet`, `repos/tucket` | **Merge into one permissive radio-family workspace**, preserving crate names, histories, firmware targets, and provenance files. Retinue can remain the repository name. | The current sibling path links contradict the stated one-workspace goal and make one hardware acceptance span four releases. GPL-derived personalities remain downstream image/process integrations. |
| Woodshed `audio-primitives` consumed by Hocket | **Promote to a standalone `audio-primitives` repository or a deliberately named audio-family workspace.** Do not put it in Wavicle: DSP primitives and a codec have different contracts. | It is pure `std`, product-neutral, and already has two real consumers. Hocket currently reaches into Woodshed by sibling path. |
| Mere `register-*` microcrates | **Do not promote this donor cluster as Graphshell infrastructure.** Move live contracts to owners: layout descriptors to Scenograph, viewer selection to Inker, presentation codecs to Graphshell/Genet, app lenses/themes/knowledge to Mere. Retire islands with no callers. | `register-layout` is currently used only through `register-viewer`; `register-renderer-types` has no live consumer; several others are workspace roots rather than integrated services. |
| `uxtree` | **Keep in Mere for now.** It is browser-shaped and depends on Inker. Extract a renderer-neutral semantic core into Meristem only after Graphshell and another app prove the same structure. | Accessibility reuse is desirable; rebranding an application projection as a wire protocol is not reuse. |
| `armillary`, `personae`, `servitor`, `sibylla`, `vates`, `wavicle`, `netfetcher`, `misfin` | **Keep standalone.** Each has a narrow dependency direction, an independent protocol or runtime contract, and more than one plausible consumer. | Graphshell consumes some directly but does not become their family repo. |
| Turnstone, Woodshed, Hocket, Isometry | **Keep product workspaces.** Their model, driver, view, and host crates share product acceptance and should move together. | Shared substrate leaves through proven public seams; product truth stays local. |
| `merely-made.github` | **Keep as the brand/site repository.** It has no place in the Rust dependency graph. | Product and architecture docs may cite it; crates do not depend on it. |
| `wgpu-graft`, `wgpu-scry`, `wgpu-weld` | **Keep as low-level interop repositories.** Graphshell reaches them only through Genet/NetRender adapters. | Pulling experiments or foreign-surface backends into Graphshell would invert the renderer boundary. |

This table is a boundary map, not a cleanup queue. P3/P4 and their immediate
arrangements consolidation gate Graphshell because they establish its portable
input. The other rows move only when their stated consumer, dependency, or
license trigger is real; Graphshell does not wait for a workspace-wide
reorganization.

## Findings

### 2026-07-25: G5 readiness, measured — the identity substrate arrived from another lane

Examined because the capability round wanted the projection endpoint to
sub-delegate to a remote viewer, and that turned out to be blocked on
Graphshell rather than on capabilities. Written against the tree, not against
the plan's prose.

**Correction to how this was being reasoned about.** Graphshell's skeleton was
briefly read as settled shape — "no client identity, stdio only" stated as if
it were a designed boundary. It is a young tree with holes, and holes do not
fill themselves: nothing below happens because it is needed, only because it
is planned and assigned. G5 exists in §8 and is unstarted; this entry makes it
orderable.

**What G5 needs that already exists** (more than expected — a windfall from
the low-power lane, landed the same week):

- `crates/system/notochord` (V5/V6): a **bounded session handshake** that
  is exactly G5's "bind Personae identity and a local grant to the session
  handshake". The initiator signs a canonical transcript with a personae-derived
  key attested to its master identity; the responder rebuilds the transcript
  from what it independently observed, so a captured hello does not verify on
  another connection. It carries a delegation-chain evaluator and a revocation
  ledger. Sans-io by design, so it drops in above any carrier.
- Its action vocabulary is **open**: `RequestedAction { domain, path, action }`
  (`mere.network` / `/services/murm` / `connect` is only the first). A
  Graphshell service is a new triple, not a change to that crate.
- `murm-transport::P2pandaTransport` — `builder`/`bind`/`sync_parts` over Iroh,
  i.e. G5's "promoted Murm transport", plus `ReticulumTransport` for the
  constrained profile later.
- The protocol already has §4.1's full status vocabulary:
  `SessionStatus { Live, Stale, Disconnected, Expired, Revoked }`.
- The capability round supplies the grant side: typed `Cap`, signed delegation,
  read-time revocation.

**What is genuinely absent** (verified by reading the crates):

1. **Graphshell has no principal.** `ProjectionSession` is a `String` label and
   `EndpointDescriptor` is `{label, projections}`. No subject, no grant
   reference anywhere in `chirograph`, though §4.1 lists
   "authenticated principal and grant reference" in the session plane's
   *minimum* vocabulary.
2. **`graphshell-client` holds no identity at all** — a grep for
   identity/keypair/personae finds only `Transform2::IDENTITY`, a matrix
   constant.
3. **No session lifecycle on the wire.** `CarrierRequestBody` is
   `{Discover, Snapshot, Resource, Resume, Intent}`; §4.1 requires open, close,
   suspend, resume, resynchronize, and there is no authenticated *open* at all.
4. **Stdio is the only carrier**, so "remote" is still aspirational and a
   stdio viewer is a subprocess of the same user — least-privilege hygiene, not
   a trust boundary.

**Ordered work for G5, none of it assigned:**

- **G5a — protocol. LANDED 2026-07-25.** `SessionPrincipal { subject, grant }`
  with both fields **opaque** (`grant` is bytes; the protocol crate's deps stay
  blake3/sceno/scenotime/serde, no personae, no policy), `SessionOpen` /
  `SessionOpened`, and carrier verbs `Open` / `Close` / `Suspend`.
  *Corrected while building:* the plan said to add all five §4.1 verbs, but
  `resume` already existed as `Resume` (reconnect from a client's last ack) and
  `resynchronize` is what `Snapshot` already does — a fifth variant would have
  been a second spelling. Three added, not five; the mapping is documented on
  the enum and covered by a test.
  *stdio refuses two of them rather than stubbing:* it cannot authenticate a
  same-user subprocess over inherited pipes, so `Open` returns a failure
  instead of answering `Opened` (a carrier that cannot verify a principal must
  not reply as though it had), and `Suspend` is refused because over stdio the
  session IS the process, so nothing would survive to resume. `Close` is
  honoured — it ends the loop. The exhaustive matches meant the compiler
  demanded a decision at every consumer, which is why these are decisions
  rather than defaults. Receipts: protocol 8 tests, turnstone 145, graphshell
  port and stdio green.

- **G5a.1 — corrected after review, 2026-07-25.** The first cut added
  *vocabulary*, not an authenticated runtime principal, and four things were
  wrong:
  1. **Admission was duplicated.** `SessionOpen` repeated a claimed subject and
     grant, bound to nothing, beside `notochord`'s handshake, which
     already binds subject, attested signer, delegation chain, action, nonce,
     and transport to *this* connection and withholds the stream until that
     verifies. A second, weaker admission path is how an identity ends up
     trusted for having been asserted twice rather than proved once — the same
     duplication mistake as building a delegation algebra beside personae's,
     one layer up. **Fixed:** `notochord` is the sole admission handshake
     and now yields the conclusion — `AdmittedPrincipal { subject, class,
     session_id, action }` from a new additive `admit()` (`respond` unchanged
     for callers that only gate the stream). It is deliberately non-`Serialize`:
     a conclusion drawn from a verified transcript, never something that
     travels. `SessionOpen` negotiates version and capabilities only, and a
     test asserts the words `subject` and `grant` do not appear on that wire.
  2. **The resynchronize mapping was wrong.** Recorded as
     `resynchronize → Snapshot`; it is `resynchronize → Resume`.
     `graphshell-client` already names its recovery path
     `Resynchronize(ResumeRequest)` and emits it when an epoch or base revision
     disagrees, while `Snapshot` is the projection plane's request and carries
     a score. The error came from mapping the plan's prose onto the protocol
     enum without reading the client that implements the concept.
  3. **`endpoint_subject` was unauthenticated and unconsumed.** Removed rather
     than kept: a key asserted in that frame is unverified by construction, so
     a client pins the peer its carrier proved. G5b owns the endpoint-key proof.
  4. **The stdio judgment was untested.** "stdio green" meant only that nothing
     regressed. Now behavioural: `Open` and `Suspend` fail with reasons, and
     `Close` is answered and *ends the loop* (a request after it is never
     served).

  Receipts: notochord 11, protocol 8, stdio 4 (3 new), client/endpoint/
  port green, turnstone 145.

  **Sequence recut** (review): G5a.1 as above → **G5b** selected Graphshell
  profile plus explicit endpoint-key proof → **G5c** `notochord` producing
  the admitted principal under the Graphshell action triple → **G5d** carrying
  that accepted context over `P2pandaTransport`.

  **G5b's open question is answered by §3 and needs no invention:**
  `graphshell-client` stays transport-independent and gains **no** personae
  dependency; the Graphshell *application* (`ports/graphshell`) composes
  Personae, loading a user-selected profile from the shared vault, defaulting
  to `default` while remaining configurable.
  *Boundary rule:* the protocol crate should carry the principal and grant as
  **opaque bytes**, never a `notochord` or personae dependency. §3 sealed a
  portable boundary; admission belongs to the endpoint/host crate. (The same
  distinction gemot got wrong in prose — posture is about source, not the build
  graph — so state it in the manifest comment when the dep lands.)
- **G5b — identity. LANDED 2026-07-25.** `ports/graphshell::profile`:
  `GraphshellIdentity` loads a **user-selected** profile
  (`GRAPHSHELL_PROFILE`, defaulting to `default` — the profile the Personae
  SSH agent serves, so Graphshell speaks as the user rather than as a second
  identity minted behind their back) from the shared vault. `graphshell-client`
  and `chirograph` still declare **zero** personae dependencies, per
  §3; the application composes identity, the protocol crates never see it.
  **It fails closed, deliberately unlike Turnstone.** Turnstone falls back to an
  unsealed seed because a browser refusing to start over a key store is worse
  than one that says what protects its key. Graphshell must not copy that: it
  serves peers who *pin* the key they reached, so an invented identity is
  indistinguishable from an impostor. A vault that will not open is an error.
  **The endpoint-key proof** replaces the `endpoint_subject` field G5a.1
  deleted: `attest_session_key` returns a master-signed
  `DerivedKeyAttestation` binding a per-session derived key to the profile, and
  `verify_session_key` checks it against a master **the carrier authenticated**
  — never one from the same frame, or an impostor supplies both halves and they
  agree with each other. Receipts: a session key derives stably and differs per
  session; its proof verifies, and fails for another session and against
  another master; an unopenable vault yields no identity. The attestation test
  is `expect`-hard on Windows rather than skippable, so "the proof verifies" is
  never reported without evidence. graphshell 14 tests.
- **G5c — admission. LANDED 2026-07-25.** `ports/graphshell::admission`: the
  triple `mere.graphshell` / `/services/projection` / `connect`, a
  `PROJECTION_PROTOCOL` label that rides the signed transcript, `open_session`
  (initiator) and `admit_session` (responder, returning the
  `AdmittedPrincipal`). The first real consumer of the capability round outside
  turnstone, and it required **no change to `notochord`'s vocabulary** —
  adding a service really was a new triple, as its open `RequestedAction`
  promised.
  `open_session` returns a `SessionHello` rather than encoded bytes so it
  composes with `notochord::initiate_session` instead of standing up a
  second encode path beside it — that crate already owns the framed stream
  handshake (`initiate_session` / `accept_session`), which G5d builds on.
  `admit_session` checks the action after admission. This entry originally said
  the check was defence in depth because "the policy already evaluates it";
  **that was wrong, and G5d proved it wrong.** See G5d below.
  Receipts (graphshell 18): a granted viewer is admitted **and named** (the
  principal is the peer the handshake established, not a claim); an ungranted
  stranger is refused; **a Murm grant does not open projections** for the same
  subject, which is the point of a per-service triple; and a captured hello
  replayed onto a different connection fails, so admission is per-connection
  rather than per-credential. `graphshell-client` and `chirograph`
  still declare zero personae and zero notochord dependencies.
- **G5d — carrier. LANDED 2026-07-27.** `ports/graphshell::carrier`:
  `accept_projection_session` runs the full accept path before a single
  `SessionOpen` byte is read. It is Notochord **N2's second service carrier**
  (Murm's real accept path is the first), and it owns none of the machinery it
  uses: carrier facts come from `AcceptedSession::into_session` (N1's one
  audited adapter), framing and the admitted conclusion from
  `notochord::admit_session`, delegation from Personae, owner rules from
  the policy the host supplies. What is Graphshell's and lives here: the ALPN,
  the offered service path (`projection_policy`), and which admitted actions
  projections serve.
  **The transport is borrowed, never owned.** A short-lived accept task owning
  its transport discards the reply it already wrote, on either arm: quinn's
  `Drop for SendStream` finishes the stream *except* when the connection is
  already errored, and retinue's outbound relay drains to EOF *unless* the
  endpoint was torn down and the relay aborted. Two unrelated mechanisms, one
  escape hatch, both opened by dropping the carrier. Taking `&T` makes it
  unspellable.
  **The action-vocabulary finding is closed.** G5c exposed that the first
  `ServiceRule` keyed only by path, so `ActionNotOffered` had no policy decision
  site. The completed Notochord rule now persists the owner-offered domain and
  admission actions and refuses an unoffered triple during the handshake.
  Graphshell still applies `admission::serves_action` after admission because
  the implementation, rather than a serialized owner document, is final
  authority over what it actually serves. The receipt deliberately lets the
  owner allow `administer`, then proves Graphshell refuses it as
  `ActionNotServed`. The memory carrier authenticates its counterparty, so rule
  D6 forces the fixture's claimed subject to be the peer the carrier proved.
  The first cut proved the generic path through `MemoryTransport`, not the
  plan's literal p2panda arm. The N2 follow-up runs the same listener over real
  `P2pandaTransport`: an authenticated viewer reaches application bytes only
  after admission, while a valid Murm grant for that peer is refused with
  `ActionNotCovered` and exposes zero projection bytes. That refusal uncovered
  and fixed a p2panda stream-lifetime race: graceful shutdown now retains the
  last QUIC connection handle until the final frame is acknowledged. Current
  receipts: graphshell 35; mere-transport 40 plus its Notochord
  integration test.
- **G5e — lifecycle. LANDED 2026-07-27.** `ports/graphshell::lifecycle` joins
  admitted authority to the session states the protocol already modelled.
  **No new vocabulary was needed**: `SessionStatus::{Expired, Revoked}`,
  `CachePolicy::purge_on_revocation`, and `IntentResult::Stale` were all
  already there from G1; what was missing was anything that *drove* them from
  the authority the carrier proved. `graphshell-client` gained two status
  mutators beside its existing `mark_stale`/`mark_disconnected` and still
  declares zero personae and zero notochord.
  - **reconnect** — `projection_session` derives the id from the
    transcript-bound `AdmittedPrincipal::session_id`, so the client cannot
    name the session it mounts under. A second admission lands on a different
    id and cached scenes from the first are unreachable by construction rather
    than by an enforced rule. Resume *within* one admission is untouched (G2).
  - **expiry** — `deadline_ms` is the earliest expiry in the chain: a session
    lives no longer than the shortest-lived link that opened it. `Lapse::
    Expired` carries the deadline, not the observation time.
  - **revocation** — checked ahead of expiry, because both can hold at once
    and an owner's withdrawal is the more useful thing to report.
  - **cache purge** — `apply_lapse` honours the mounted scene's own
    `CachePolicy`: purge forgets the session outright (scene *and* every
    resource cached under it), retention keeps it under a status that cannot
    still read `Live`.
  - **denied score** — scores hang below the service path, so Personae's
    existing scope grammar expresses "this viewer may see one score" with no
    new mechanism. The `/` boundary in `path_covers` is what makes it safe.
  - **stale intent** — a lapsed session is refused *without* being told the
    current revision; replying `Stale` would disclose to a peer whose
    authority has ended what the current revision is.
  Receipts (graphshell 33, graphshell-client 7): all six paths above, plus
  revocation outranking expiry, a per-score grant not reaching a neighbour
  sharing its prefix (`spiral` vs `spiral-admin`), and an intent naming
  another session rejected.
  **The carrier revocation seam is closed.** `AdmittedSession` retains the
  verified claims consumed from the bounded hello. `SessionAuthority::
  retain_admitted` derives the deadline and delegation ids from that exact
  conclusion, so a carrier-admitted session can notice later revocation without
  reconstructing authority from application data.

- **G5f — the two-device run. COMPLETE 2026-07-29.**
  `ports/graphshell/src/bin/g5_peer.rs`, modelled on `mesh-peer`. **Windows
  laptop → iMac (<remote-host>, macOS 15.7.7) over p2panda/iroh QUIC on the LAN**, both
  at `0ffe8a62`, distinct identity seeds, ticket pasted between them.
  - *Granted run.* Client: `dialling f541dd2c`, `admitted`, `#1 -> opened,
    status Live, expires Some(1785188082892)`, `#2 -> descriptor "graphshell
    fixture" with 1 projection(s)`, `#3 -> closed`. Server: `admitted subject
    04a11951 for connect`, `served 3 request(s); ended Closed`.
  - *Revoked run.* Same pair, server started `--revoked`. Client: `admitted`,
    then `#1 -> refused: session authority was revoked`, then `#2 -> the
    endpoint closed without answering`. Server: `grant revoked`, `served 1
    request(s); ended Lapsed(Revoked)`.
  So the delegation chain now verifies across a real link, and G5e's
  per-request authority check demonstrably ends a live session on a second
  machine rather than only in a unit test.
  **Two bugs a single-process test could not have found**, both in the bin:
  a certificate with a real `issued_at` and `not_before: 0` violated
  `issued_at <= not_before`; and seeding the transport with
  `master_public_key()` rather than the seed that derived it made the carrier
  and the Personae identity different keys, so D6 refused with
  `SessionProofInvalid` three layers from the cause. Both are now back-dated
  by a minute for cross-machine clock skew and checked by `assert_same_key`
  at bind time.
  **Resume and intent, added the same night.** The first pass used the G1
  fixture, which answers "this fixture does not resume", so no interruption was
  survived. `ResumeFixtureEndpoint` already had the hard half — a two-diff
  history and an epoch-preserving current snapshot — and only lacked the three
  traits `dispatch_common` needs; it now has them, with `IntentSink` doing real
  adjudication (an intent on a superseded revision returns `Stale` with the
  endpoint's current position). `serve` loops against **one** endpoint, because
  an endpoint that dies with its connection can only ever answer a resume with
  a fresh snapshot.
  *Granted run, Windows → <remote-host>.* Session 1: `opened, status Live`, `snapshot of
  2 item(s)`, `suspended` (server: served 3, ended `Suspended`). Interruption:
  a new dial, a new handshake, a new admission. Session 2: `opened`, **`resumed
  by replaying 2 contiguous diff(s), revisions 1->2, 2->3`**, `intent
  Accepted`, `closed` (server: served 4, ended `Closed`).
  *Revoked run, same pair.* Session 1 served in full and ended `Suspended`;
  the grant was revoked while the peer was away; session 2 was **admitted** and
  then refused at its first request with `session authority was revoked`,
  ending `Lapsed(Revoked)`, so the client's resume and intent were never
  served.
  The reply summary names which `ResumeReply` arrived. Before that it printed
  only "resume", which cannot distinguish a real resume from a fresh snapshot
  under a nicer name — a receipt that would have proved nothing.
  **2026-07-29 correction and executable receipt.** The current script adds a
  third admission whose first request is the literal `IntentInvocation`. The
  server folds the signed grant revocation after admission and before that
  request enters `serve_admitted_session`. Two separate local processes then
  proved session-one suspend, session-two redial and two-diff replay, an
  accepted good-path intent, and session-three refusal of request `#8` with
  `session authority was revoked`; the server ended `Lapsed(Revoked)`.
  A request-loop regression test now pins the literal intent shape.
  A second run then closed the physical composition requirement. Windows used
  the current split client; <remote-host> at `<private-address>` used corrected commit
  `86d77d41` as server. Session one suspended, session two redialed and replayed
  both contiguous diffs before accepting intent `#6`, and session three's first
  request was intent `#8`, refused after the owner revocation. <remote-host> ended
  `Lapsed(Revoked)` and both processes exited successfully. Ticket exchange
  satisfies G5's stated discovery alternative; mDNS remains convenience
  coverage. The browser carrier remains separate by G5's own instruction. See
  the [H6a implementation receipt](../../../ports/graphshell/docs/2026-07-29_h6a_g5f_prerequisite_receipt.md)
  and [H6b physical closure receipt](../../../ports/graphshell/docs/2026-07-29_h6b_physical_g5f_closure_receipt.md).

**Only after G5b+G5c does the capability round's deferred sub-delegation become
buildable**: the endpoint's certificate goes to depth 1 and issues a narrower
one to the viewer (read-only presentation of one scene). Recorded there as "one
line when the viewer has a key"; the honest distance is this list.


- `sceno::Representation` currently names a slot while `ProjectedItem` carries
  no presentation reference. That is the right portable posture, but a remote
  client needs the separate offer/resource resolution designed in section 4.3.
- `sceno::InstanceId` is a vector index. Scenotime needs the epoch rule before
  a wire diff can claim stable identity.
- Mere's `content-contract` already serializes a NetRender scene plus font and
  image assets. It proves the representation lane is feasible, while its Mere
  kernel and document dependencies prove the crate itself is the wrong shared
  boundary.
- Mere's current canvas is a product composition, not a portable canvas core.
  Its manifest joins kernel truth, signals, Seiche, Armillary, Genet DOM,
  NetRender, native winit, and wgpu.
- Cambium is a reactive UI toolkit, not a serialized remote component tree.
  Graphshell should first standardize small cards and renderer resources.
- The standalone family pattern is working: Conatus, Eidetic, Scenograph,
  Cambium, and NetRender group lockstep crates without forcing products into
  one workspace.
- The clearest misplaced live crate is `audio-primitives`: Hocket already
  imports it from inside Woodshed. The clearest redundant crate is Mere's old
  `mere-identity`: the workspace's live `identity` alias already points to
  Personae.
- The radio repositories describe themselves as one family and already use
  sibling paths. One workspace would make the shared radio and firmware
  acceptance boundary literal.
- The Murm/Moot extraction is already designed. Graphshell supplies another
  reason to finish it, not a reason to design another transport layer.

## Progress

### 2026-07-22

- Ruled Graphshell as the remote projection host above Scenograph and distinct
  from Mere's internal shell/chrome family.
- Audited the live `Code/repos` package and cross-repository dependency graph.
- Defined the four protocol planes, presentation-resource boundary, scene-epoch
  rule, Graphshell-owned curation state, and seven implementation proofs.
- Recorded repository corrections from live consumers.
- Founded the independent local four-crate workspace and sealed its portable
  boundary in root commit `693fad8`.
- Published that workspace through `mark-ik/graphshell#308`; the active tree is
  small and portable while the retired browser and its documents remain in the
  same Git history.
- Completed G1's loopback presentation proof: sidecar offers, independent
  content-addressed resources, two capability profiles, semantic fallback,
  accessibility actions, deterministic receipt, and headed responsive check.
