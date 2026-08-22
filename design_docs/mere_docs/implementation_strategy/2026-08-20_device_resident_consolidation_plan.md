# Device Resident Consolidation Plan

**Date:** 2026-08-20
**Status:** R1 through R4 and C1 through C3 complete. R5 and C4 are the next
independent slices; V1 follows their convergence.
**Scope:** Put Knot's personal-vault authoring, replication, and referenced
artifacts under the existing device-resident authority; replace private
content identifiers with a standards-oriented portable reference; remove the
duplicate host and store compositions this exposes.

**Related:**

- [Knot product floor and cuts](../research/2026-08-19_knot_lane_brief.md)
- [Graphshell reference host](2026-07-27_graphshell_reference_host_plan.md)
- [Knot in Graphshell](2026-08-02_knot_in_graphshell_plan.md)
- [receipt artifact replication](2026-08-10_receipt_artifacts_replication_plan.md)
- [reachability and privacy lanes](2026-08-03_reachability_rungs_and_privacy_lanes_plan.md)
- [RFC 6920 Named Information URIs](https://www.rfc-editor.org/rfc/rfc6920.html)
- [p2panda client and node topology](https://aquadoggo.p2panda.org/specifications/aquadoggo/networking/clients-nodes/)

## 1. Ruling

One logical device resident owns the selected persona's durable stores and
network runtimes.

- On desktop it is a user-scoped background process.
- In a mobile or sandboxed application it is embedded in that application.
- In tests it may be an in-memory composition.

The invariant is one owner for a durable resource, not one executable shape.
Turnstone, standalone Knot, Graphshell, and later first-party applications are
clients of that resident when they use persona-held state. An application may
still embed the same resident implementation when the platform has no useful
daemon lifecycle.

The existing `graphshell_device_host` is the first desktop composition root.
It already owns the selected Personae profile, local application door,
personal graph sync, iroh blob store, and store-backed Graphshell resources.
Knot is the second real consumer that earns a product-neutral resident
composition. A binary or crate rename is not an entrance gate; first make the
shared composition true.

The p2panda reference is a topology donor, not a dependency ruling. Mere keeps
its narrow authenticated-stream transport seam and its Stickleback domains.
This plan does not adopt aquadoggo's GraphQL API or decide that
`p2panda::Node` should replace the current implementation.

## 2. Target topology

```text
Knot UI         Turnstone         other first-party clients
    \               |                         /
             Graphshell local session
              owner-only pipe/socket
                       |
                 device resident
                 |- Personae authority
                 |- endpoint catalog
                 |- Knot resident source
                 |- Stickleback joins
                 |- Murm endpoints
                 `- scoped content store
                       |
              p2panda logs + iroh blobs
```

The local boundary carries interactive projections and intents. Remote
document history still travels as signed p2panda operations through
Stickleback. Artifact bytes still travel through iroh-blobs. Graphshell does
not become a replication protocol, and p2panda does not become an editor
session protocol.

One resident process may own several network endpoints. Per-space or
per-graph identities remain distinct where unlinkability requires them.
Process consolidation must not quietly turn every place, personal graph, and
Knot vault into one observable node identity.

## 3. Ownership after consolidation

| Concern | Owner |
|---|---|
| Process lifetime, store locks, route catalog, orderly shutdown | Device resident |
| Local and remote interactive session grammar | Graphshell |
| Djot, document revisions, clips, evidence meaning, merge | Knot |
| Authenticated streams, discovery, relay configuration, blob transfer | Murm |
| Signed operation joins, reconciliation, retained history | Stickleback |
| Personal identity, device pairing, secret derivation | Personae |
| Communal membership and capability projection | Gemot |
| Portable byte identity | Standards-oriented content reference |
| Fast storage and transfer address | iroh BLAKE3 blob hash |

Murm supplies resident services and reusable owners. It does not decide which
Knot document a reference belongs to or whether a Gemot participant may read
it. Gemot supplies authority facts. It does not own the blob store or process
lifetime.

## 4. Invariants

1. A persistent redb or iroh blob store has one live process owner.
2. Persona-vault authoring and its sync host use the same Knot store handles.
3. A Graphshell session gets its route only after local admission, and the
   admitted application must be allowed to reach that route.
4. A shared physical content store authorizes `(scope, peer, hash, mode)`, not
   merely `peer`.
5. An artifact is exposed only after its byte count, transport BLAKE3 hash,
   and portable digest have been verified.
6. New portable references use a registered form. Existing
   `urn:blake3:<hex>` references remain readable.
7. Consolidating process lifetime does not consolidate unrelated network
   identities.
8. Personal authority comes from Personae pairing. Communal authority comes
   from a Gemot projection. Route hints remain reachability, not authority.
9. Arbitrary directory editing can remain embedded in Knot or Turnstone. It
   does not require the persona resident unless it opts into resident-held
   sync or content.
10. The stdio carrier remains a protocol-conformance and diagnostic
    deployment. It ceases to be the production owner of a persona vault.

## 5. Implementation slices

The two tracks can start independently. Resident composition depends on R1
through R4. Physical content-store consolidation depends on C1 through C3.
R5 and C4 meet in the final receipts.

```text
R1 -> R2 -> R3 -> R4 -> R5
                 \
C1 -> C2 -> C3 -> C4 -> V1
```

The first code slice is R1. C1 can proceed beside it. R2 then supplies the
source boundary R3 needs; C2 remains the hard gate before any physical store
sharing in C3.

### R1. Make the first-party application door route-aware

The local application door currently admits Turnstone and then always opens
the resident identity endpoint. The browser host already proves that an
admitted session can open a route from `ResidentEndpointCatalog`.

Tasks:

- add a versioned application hello that requests a resident route;
- retain the current hello as the `identity` default for installed clients;
- replace the flat application allowlist with application-to-route grants;
- open the catalog route only after same-user and Graphshell admission pass;
- teach `AppBrokerClient` to request a route;
- keep the browser-extension door and its allowlist unchanged.

Done when:

- an admitted Turnstone client can open an in-memory Knot fixture route;
- the same client can still open the identity route through the old hello;
- an allowed application requesting an ungranted route is refused before any
  product endpoint opens;
- browser and first-party tests prove the two doors still cannot speak each
  other's hello.

### R2. Separate Knot resident source state from session state

`ResidentEndpointCatalog` creates one endpoint per session. Simply putting one
`KnotEndpoint` behind a mutex would be wrong: presentation caches and notice
cursors are session state, and one visitor could consume another visitor's
revision bell. Reopening the same redb files per session would merely move the
ownership defect.

Tasks:

- introduce a resident Knot source that owns the unlocked vault, signed
  operation store, signing material, evidence service, and revision stream;
- make each `KnotEndpoint` a session adapter over that shared source, with its
  own disclosed resources, snapshots, and notice cursor;
- serialize authoritative mutations at the source and let every session
  observe the resulting revision;
- give the resident source an async content-retention port;
- remove `BlobClipEvidenceStore`'s private-runtime escape hatch once all
  resident evidence retention uses that port;
- define orderly close so joins and blob actors flush before stores drop.

Done when:

- two local Graphshell sessions edit one persona document without stale
  in-memory vault copies or stolen notices;
- an edit from either session rings and refreshes the other;
- authoring and sync hold clones or borrows from one opened source authority,
  rather than reopening its files;
- dropping the resident releases every store so it reopens cleanly in the
  same test process.

### R3. Compose Knot into the desktop resident

Tasks:

- add optional Knot persona-vault settings to the existing device host;
- startup-unlock the Knot authority once for the selected persona;
- open one async Knot evidence store and pass the same handle to authoring and
  `KnotSyncHost`;
- register a stable Knot route in the resident endpoint catalog;
- start personal Knot replication beside personal-graph replication;
- apply pairing changes live to writer admission, evidence admission, and
  dial routes;
- keep each lane's derived network identity distinct unless an explicit
  privacy ruling says otherwise.

Done when:

- the resident stays joined with every UI closed;
- Turnstone can open, edit, close, and reopen the resident Knot route while
  sync stays online;
- the process owns the Knot redb and blob stores exactly once;
- live pair and unpair changes affect document intake and artifact access
  without restart;
- two configured spaces retain distinct externally visible endpoint ids.

### R4. Migrate product clients and retire duplicate production hosts

Tasks:

- route Turnstone's persona-vault authoring through the resident application
  door;
- keep Turnstone's arbitrary-directory mode embedded through `LocalCarrier`;
- make standalone Knot embed the same resident library when it owns the whole
  application process;
- move `knot_sync_host` management operations to settings or admitted
  resident intents that do not open the stores behind the resident's back;
- retain `knot_endpoint` stdio modes as protocol fixtures and explicit
  isolated deployments;
- remove the production persona-vault paths that independently open a sync
  host or evidence store.

Done when:

- desktop Turnstone never opens the resident persona vault files itself;
- standalone Knot works with the resident embedded and no Graphshell GUI
  process present;
- directory editing still works with the resident absent;
- attempting to start a second owner fails clearly instead of hanging on a
  database lock.

### R5. Reduce repeated transport-host machinery

`PersonalSyncHost` and `KnotSyncHost` both configure mDNS, relays, stored
routes, overlay topics, blob serving, pairing refresh, and shutdown. The
domain joins and identities remain different.

Tasks:

- compare the two live hosts after R3 and extract only their common lifecycle
  into Murm;
- centralize relay parsing, discovery policy, route seeding and refresh,
  endpoint reporting, and clean shutdown;
- let each caller provide its signing key, overlay topics, protocol handlers,
  and authority policy;
- retain separate endpoints when identities must remain unlinkable;
- avoid adopting `p2panda::Node` merely to obtain the daemon shape.

Done when:

- Graphshell personal sync and Knot sync use one shared Murm host vocabulary;
- neither port repeats relay, mDNS, hint, or shutdown loops;
- transport tests still prove authenticated byte streams keyed by peer and
  ALPN;
- identity tests prove two domains are not linked by a shared node id unless
  configured to be.

### C1. Replace private portable identifiers

`urn:blake3:<hex>` and Graphshell's `urn:sha256:<hex>` are private spellings.
For new portable content, use an RFC 6920 Named Information URI with SHA-256.
Keep the iroh BLAKE3 hash beside it as a transport address.

The model is conceptually:

```text
portable_id:   ni:///sha-256;<base64url>
transport:     blake3:<hex>
byte_size:     ...
media_type:    ...
```

Tasks:

- census Knot, Graphshell transfers, receipts, and publication for existing
  SHA-256 and BLAKE3 representations;
- add one versioned shared content-reference type at the lowest justified
  layer after that census;
- parse RFC 6920 `ni` names and emit the mandatory SHA-256 form;
- author a new Knot evidence-reference version while retaining the current
  reference decoder;
- keep canonical source URI, artifact role, and media type as provenance,
  rather than pretending they are part of byte identity;
- verify portable SHA-256 after iroh has verified the BLAKE3 transfer.

Done when:

- a new clip writes an RFC 6920 identifier and an iroh transport hash;
- a legacy `urn:blake3` clip still resolves and verifies;
- conflicting portable and transport hashes fail closed;
- Graphshell and Knot use the same parser and serializer rather than two new
  private URI spellings.

### C2. Add hash-scoped blob authorization

The present `BlobPeerAuthorizer` admits a peer to the blob protocol as a
whole. That is adequate for an isolated per-space store. It is inadequate for
a physical store shared across spaces, because a peer who learns a hash could
ask for bytes retained only under another authority.

Tasks:

- replace the connection-only decision with an authorization policy that can
  decide `(scope, peer, hash, read)`;
- determine whether the current iroh-blobs server can apply that policy per
  request without forking its protocol implementation;
- if it cannot, keep stores or protocol routers isolated by scope until an
  upstreamable request hook exists;
- bind every retained hash to one or more custody scopes;
- make revocation live for new requests while preserving already-retained
  local bytes.

Done when:

- a peer admitted to space A cannot fetch a hash retained only for space B,
  even when it knows that hash;
- a hash intentionally retained by both scopes is physically stored once and
  readable through either valid grant;
- removing the last read grant refuses the next request without deleting the
  owner's bytes;
- authorization is exercised on the serving side, not only before a caller
  chooses a source.

### C3. Consolidate physical content custody

Only enter this slice after C2 is green. Until then, isolated stores are a
security boundary rather than waste.

Tasks:

- let the resident own one physical iroh blob store;
- add scoped, named retention leases over hashes;
- standardize tag vocabulary across receipts, transfers, and Knot evidence;
- make collection release remove a lease and collect bytes only after the
  final lease disappears;
- change `PersonalSyncHost` and `KnotSyncHost` to borrow the resident store;
- migrate existing per-lane stores with verified copy, lease creation, and a
  restart-safe completion marker.

Done when:

- identical bytes retained by receipts and Knot occupy one physical blob;
- releasing either consumer leaves the other consumer's bytes intact;
- an interrupted migration resumes without losing the old store or claiming
  an unavailable blob;
- one resident restart reopens the shared store and every custody lease.

### C4. Unify authority materialization

Tasks:

- define a revisioned space-authority snapshot with separate writers,
  evidence readers, evidence sources, and route hints;
- materialize personal snapshots from Personae pairing;
- materialize communal snapshots from Gemot capabilities;
- let Knot operation admission, blob serving, blob fetching, and route refresh
  read the same snapshot revision;
- remove copied `paired_writers`, writer sets, reader sets, and source sets
  where they are merely stale materializations of that snapshot.

Done when:

- one pairing or Gemot change advances one revision and every consumer
  applies it;
- reader and writer rights remain independently testable;
- route loss changes reachability without changing authority;
- a communal writer without evidence-read permission cannot fetch evidence,
  and a reader without write permission cannot author operations.

## 6. V1 verification receipt

### Automated

- versioned local-app hello and route admission;
- two concurrent local Knot sessions over one resident source;
- one opened Knot blob store shared by authoring and sync;
- legacy and RFC 6920 reference decoding;
- SHA-256, BLAKE3, size, and conflicting-reference metadata failures;
- cross-scope known-hash fetch refusal;
- live pair, unpair, Gemot grant, and Gemot revocation;
- shared-hash lease and final-release collection;
- clean resident shutdown and reopen;
- distinct per-domain network identities.

### Two-device

1. Device A runs the desktop resident and authors a Djot clip through
   Turnstone.
2. The resident retains the artifact and writes its portable reference.
3. Device B receives the signed document operation before the artifact.
4. Device B asks the authorized Device A identity for the transport hash.
5. Device B verifies BLAKE3, byte count, and portable SHA-256, then exposes the
   artifact.
6. Both devices restart with the artifact still available offline.
7. Unpairing Device B prevents a fresh fetch while leaving Device A's retained
   bytes intact.

### Headed

- Turnstone edits the resident Knot route while the standalone sync status is
  visible and remains live after the pane closes.
- Standalone Knot embeds the same resident composition and performs the same
  edit, restart, and evidence-open flow without Turnstone.
- A directory-only document remains editable with the desktop resident
  stopped.

## 7. Stop line

This plan does not:

- introduce a `.knot` file format;
- fold artifact bytes into p2panda operations;
- replace Graphshell with GraphQL or HTTP;
- adopt aquadoggo or `p2panda::Node` as an unreviewed bundle;
- force one network identity across all resident domains;
- share a physical blob store before hash-scoped serving authority is proven;
- turn Gemot into a local process supervisor;
- delete the stdio carrier's conformance role;
- require arbitrary local directories to join a persona vault.

## 8. Progress

- **2026-08-20:** Mark approved the resident/client topology, RFC-oriented
  content identity, and redundancy cuts. Plan written from the live
  `graphshell_device_host`, application broker, resident endpoint catalog,
  `PersonalSyncHost`, `KnotSyncHost`, and evidence-store seams.
- **2026-08-21, R1:** `91f1297e` made the first-party door route-aware. The
  version-one hello still selects `identity`; version two requests a
  host-granted route. Product factories open only after local and Graphshell
  admission. The focused broker suite passed 18 tests, and a real resident
  integration opened Knot's fixture through a Turnstone grant.
- **2026-08-21, C1:** `7655a625` added the shared
  `PortableContentRefV1` and canonical `ni:///sha-256;<base64url>` parser and
  serializer, checked against RFC 6920's `Hello World!` vector. New Knot
  evidence writes NI plus `blake3:<hex>`, legacy `urn:blake3` still verifies,
  and conflicts fail closed. Graphshell authors the same NI type and no longer
  repeats its digest as private SHA-256 hex; transfer verification retains a
  legacy-hex read fallback.
- **2026-08-21, C1 consumer:** Turnstone `1464043` injects the Murm blob store
  for clip evidence and updates its ignored source-evidence receipt to reopen
  the blob store and verify the retained bytes. Formatting and staged-diff
  checks pass. Running that receipt remains open: Turnstone's current lock and
  retired Genet path patches reject the local Mere metadata, while a disposable
  resolution advances 101 same-source packages. That dependency update is not
  part of C1.
- **2026-08-21, R2:** `f13ebf09` added `KnotResidentSource`, which owns one
  unlocked vault, signed operation store, signing material, and source-owned
  content-retention actor. Each `KnotEndpoint` now has its own Graphshell
  session, disclosure caches, derived state, and notice cursor over that shared
  authority. Vault saves recheck their base token and serialize authoring plus
  rematerialization under the source lock. The receipt alternates valid edits
  between two sessions, rejects the stale intervening invocation, observes two
  independent bells in both sessions, and reopens the vault, redb store, and
  evidence store after final resident drop. The actor replaces the former
  private Tokio runtime and joins after flushing iroh-blobs. All 98 Knot library
  tests pass, and `cargo check -p knot --all-targets --locked --offline` passes.
- **2026-08-21, C2:** `72b7fdb2` added serving-side
  `(scope, authenticated peer, hash, read)` authorization around the public
  iroh-blobs request handlers. A hash can be retained by several scopes in one
  physical store without widening either reader set. Releasing custody or a
  reader grant refuses the next request while the owner's bytes remain. Knot
  evidence retention and fetch verification now bind custody to the space,
  and resident startup replays references from Djot only when the bytes exist.
  All 42 `mere-transport` library tests pass, including the real-iroh
  cross-scope and live-revocation receipt; `cargo check -p knot --lib` also
  passes. The full Knot lib-test build remains blocked in its endpoint fixtures
  by the local Genet/Inker removal of `Fetched::text`, outside this slice.
- **2026-08-21, R3:** `71a4b81f` composed one startup-unlocked
  `KnotResidentSource` and its `KnotSyncHost` into
  `graphshell_device_host`. The process registers the stable `knot` application
  route, grants it to Turnstone, shares one evidence actor between authoring
  and serving, stays joined across UI close and reopen, and reconciles Knot
  pairing settings live. The focused receipt edits Djot through the Graphshell
  route, reopens it while sync remains resident, applies pair and unpair to
  both operation intake and exact-hash reads, preserves retained bytes, and
  proves two spaces use distinct endpoint ids. That receipt, all 12 owner
  settings tests, and the feature-gated device-host check pass.
- **2026-08-22, C3:** `90a1a66d` made the device resident the owner of one
  configurable collecting iroh store under `content/blobs`. Murm's versioned
  `BlobLease` tags separate scope, lane, and subject while identical bytes keep
  one physical hash. Personal transfer staging and fetching plus Knot evidence
  now borrow that store and rehydrate exact-hash serving custody from durable
  leases. Old per-graph and per-persona stores are copied without deletion,
  digest-checked, flushed, and covered by a restart-safe marker that is
  reverified before it is trusted. The two shared-lease tests and two resident
  migration tests pass, including independent release, final collection,
  interrupted-copy replay, and reopen.
- **2026-08-22, R4:** Mere `90a1a66d` added a blocking first-party
  `AppRouteCarrier`, carried revision notices across the owner-only application
  door, and made standalone pairing facts derive from Personae without opening
  resident-owned Knot files. A Windows integration receipt proves a second
  store owner is refused promptly while pairing facts remain available. The
  five application-broker integrations and the notice receipt pass; Graphshell
  with `personal-sync` and both Knot binaries check clean. Turnstone `d6c4bdc`
  removes every direct persona-vault open and reaches only the resident `knot`
  route in persona mode. Directory mode remains an in-process `LocalCarrier`,
  and a configured `knot_endpoint` remains an explicit isolated fixture. The
  Turnstone check passes against the local Mere source. Knot's full unit-test
  cone still stops in unrelated endpoint fixtures at the removed
  `inker::Fetched::text`; the dedicated ownership receipt bypasses that drift
  and passes.
