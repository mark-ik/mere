# Moot Object M1 — a moot you can declare, join, and share into

**Date**: 2026-06-12
**Status (2026-09-06)**: Historical M1 landed as recorded below. The active
continuation is [Community collections and author-offline publishing](#community-collections-and-author-offline-publishing-2026-09-04). Its same-machine live-peer process proof passed with stable-Persona binding and current Gemot command authority. Production publication/hosting records, historical authority proof, the Persona-to-device adapter, and a two-machine receipt remain open.
The original M1 body preserves its dated vocabulary and ownership. Current owners
are Gemot for community authority and recognition, Commons for shared graph
operations, and Stickleback for accepted-operation replication. Historical
references to flora as the artifact catalog mean fauna; FLORA is the separate
federated adaptation lane. Possession of a moot id does not replace current
admission and delegation checks.
**What this is**: the moot tier's missing product object. The reputation
lane is proven (`moothold::tessera`: signed per-moot ops, LogSync, two-peer
convergence) but nothing yet *is* a moot — no declaration, no visible
membership, no flora. M1 makes the smallest honest moot: **declare** it
(name + charter), **join** it (announce yourself), **share** into its flora
(engram references), all converging deterministically on every member.
**Naming correction recorded**: this was provisionally called "mooting M1"
in conversation, but `mooting`'s charter (its own crate docs) is the
*protocol-adapter selection* layer (Matrix / Nostr / IRC / ATproto /
ActivityPub adapters over a unified social-primitive API) — not the moot
object. The object lane lands in **`moothold::moot`**, beside the tessera
lane it composes with; `mooting` keeps its adapter charter untouched.
**Related**: the [mesh M1 plan](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md) *(archived — M1 done)*
(this is the third lap of the proven wire/state/sync recipe);
the [eidetic browsing derivation plan](../../eidetic_docs/implementation_strategy/2026-06-12_eidetic_browsing_derivation_plan.md)
(the flora is where a shared `SearchIndex` reference would land — the
federation demo seed, and the consume half's eventual trigger);
the communal-compute tiers brief (a moot is ring 2's container).
**Conflict posture**: pure mere lane (moothold + docs); no genet, no
meerkat. Shell adoption is post-reshape, as everywhere today.

---

## Design (the third lap of the proven recipe)

- **Wire** (`moot/wire.rs`): signed `Operation<MootExt>`; `MootExt
  { moot_id: [u8; 32] }` is the signed addressing extension (cross-moot
  replay fails verification). Events, plain words:
  - `Declared { name, charter, at_ms }` — the founding statement.
  - `Joined { name, at_ms }` — a member announcing themselves (their key is
    the operation's author; `name` is just a display label).
  - `Shared { manifest_id: [u8; 32], schema_id, title, at_ms }` — an engram
    **reference** into the flora (the CID + what it claims to be). Blob
    transfer is deliberately out (M2 rides iroh-blobs; eidetic's
    consume-half work picks up from exactly this reference).
- **State** (`moot/roster.rs`): a deterministic, order-independent fold:
  - Competing declarations resolve by **lowest declaring-op hash** (the
    claim-race rule from the mesh board) — every member sees the same
    founding.
  - Membership: first `Joined` per author wins; members are keys, labels
    are decoration.
  - Flora: entries ordered by `(at_ms, op_hash)` — stable everywhere.
- **Sync** (`moot/sync.rs`): `SyncedMootSpace`, the `SyncedMesh` shape
  (which improved on tessera's receive-only session): LogSync catch-up +
  live lane, an `author()` path (sign at next seq/backlink, persist,
  publish), `roster()` folding the store, real `SyncStatus`, settle-watch
  `resync`.
- **Store**: p2panda-store's SQLite backend behind a `MootStore` mirror of
  `MeshStore` (one transactional insert that persists + sync-indexes).
  The tessera lane keeps its proven redb store; convergence of the two is
  the unification step below, not M1 churn.
- **The peer bin** (`examples/moot-peer.rs`): `declare <name> <charter>` /
  `join <name>` / `share <manifest-hex> <schema-id> <title>` / `show`,
  with the mesh-peer transport shape (env-derived identity + space,
  tickets on stdout/stdin, real status). The two-machine run mirrors the
  mesh milestone: declare on one device, join + share from the other,
  both rosters agree.

## The one-endpoint composition finding (recorded, deferred)

p2panda-net 0.6.1 registers LogSync under a **constant**
`LOG_SYNC_PROTOCOL_ID` — two LogSync instances cannot share one endpoint,
and one instance is monomorphic in its extension type. So when meerkat
eventually runs tessera + mesh + moot lanes on one transport, the shape is
**one LogSync, one shared extension type, lanes separated by topic** —
and the lanes are already structurally ready for it (`TesseraExt`,
`MeshExt`, `MootExt` are all `{ 32-byte id }`). The natural end-state for
a moot specifically: **one moot = one topic**, its log carrying tessera
receipts *and* object events as one vocabulary, the folds separating
concerns. That unification (or an upstream patch making the protocol id
configurable) is its own slice with its own plan; M1 builds standalone
exactly as mesh M1 did.

## Tests

1. Wire: round-trip, signature verification, cross-moot replay fails.
2. Roster fold: declaration race identical in both fold orders; duplicate
   joins collapse; flora order stable; foreign-moot ops skipped.
3. Two-peer convergence: A declares + shares, B joins live; both rosters
   agree (declaration, two members, one flora entry); status counters real.
4. The two-*machine* run is Mark's verification, via `moot-peer`.

## Done conditions

- `cargo test -p moothold` green including the new `moot` module's 1-3.
- `moot-peer` round-trips declare/join/share between two in-process peers.
- Workspace untouched beyond moothold (the crate already exists and is a
  member); cross-repo smoke stays green.
- No blob fetching, no tessera coupling, no protocol adapters (that is
  `mooting`'s charter, untouched), no economy.

## Out of scope (named)

M2: flora blob transfer over iroh-blobs + the eidetic consume-half
hand-off; invitation/capability gating (M1 is the trust-ring rule: holding
the moot id is membership eligibility — the kith ring's definition);
moderation/removal events; the one-endpoint unification above; shell
adoption (post-reshape).

## Progress

- **2026-06-12** — Plan written after the survey that redirected it:
  `mooting` is 27 lines because its charter is the adapter layer, not the
  object; tessera is per-moot but receipt-shaped; nothing declares or
  joins a moot today. Recipe and rules lifted from the mesh M1 lap
  (deterministic races, one write path, author+publish, real status).
- **2026-06-12** — **M1 landed: `moothold::moot` with the full suite green
  (moothold 72 tests; the module's 11 across wire/roster/store/sync,
  including both two-peer convergence lanes) and the `moot-peer` rehearsal
  run end to end.** `wire.rs` (signed `Operation<MootExt>`; cross-moot
  replay fails; p2panda validator compatibility), `roster.rs` (the
  order-independent fold: declaration race resolves by lowest op hash in
  both fold orders, duplicate joins collapse to the earliest, flora stable
  by `(at_ms, op_hash)`, foreign-moot ops skipped), `store.rs` (`MootStore`
  over p2panda-store sqlite, one transactional persist+index write path),
  `sync.rs` (`SyncedMootSpace`: catch-up + live lanes, `author()`,
  `roster()`, real `SyncStatus`, settle-watch `resync`). The rehearsal
  (durable sqlite stores so one identity authors across invocations):
  founder declared `printing-circle`; the friend synced the declaration
  (real status: 1 round, 1 op), joined as `alex`, and shared a
  `eidetic.SearchIndexSpec/v1` reference into the flora; the founder's
  final roster converged on all three — declaration, member, flora entry.
  The flora reference is the literal hand-off eidetic's deferred consume
  half picks up from. **Remaining**: Mark's two-machine run (`moot-peer`,
  the mesh-peer recipe: same `MOOT_SPACE`, distinct `MOOT_SEED`s, tickets
  both ways); then M2's named scope.

## Community collections and author-offline publishing (2026-09-04)

**Status:** active. This continuation follows Mark's Eidetic/Fleece, application
co-op, community lineage and voluntary distribution discussion. It replaces the
old M2 implementation assumptions above; it does not reopen completed M1 work.

### First product proof

An author publishes a small gemtext page into a moot. A different member accepts
a bounded hosting commitment, receives and verifies the bytes, and retains them.
The author process is stopped. The host is restarted from its durable store. A
third reader, with an empty content cache and no access to the author's storage,
retrieves the exact published revision from that host. The reader can inspect
authorship, the moot submission, and the host's distinct commitment.

This is the first execution target. It makes community distribution useful without
waiting for new ranking, reputation economics, FLORA training or a universal graph
editor. The three roles use independent Personae roots and distinct stores.

### Shared model and ownership

The following are semantic requirements, not new wire types. Before adding a type,
map it to the existing owner and record any extension needed there.

| Fact | Meaning and owner |
|---|---|
| Space identity and accepted history | A mere is share-ready; adding participants does not migrate its format. Gemot owns community admission and authority; Commons retains graph edits; Stickleback accepts and replicates canonical operations. |
| Publication and revision | A continuing publication names immutable content revisions and their parents. Eidetic stores typed manifests/payloads; an existing domain authorizes updates. Current host location is separate from publication identity. |
| Contribution | A signed submission to a moot references a revision and carries its own context. Republishing preserves the origin and adds a contribution. It does not replace authorship with the host or moot identity. |
| Hosting commitment | A host names the content, audience, byte limit, retention bound and applicable policy revision it accepts. Inspect Mesh availability/lease contracts for reuse; their existing job scope is not automatically a publication-hosting implementation. |
| Availability observation | Retrieval and integrity checks establish observed availability. A promise, an accepted transfer and demonstrated service are separate facts. |
| Extraction and search | Fleece extracts supplied DOMs; the host owns acquisition and snapshot custody. Eidetic retains the extraction contract; search is a derived projection of admitted, selected content. |
| Application interaction | Woodshed and other apps own domain facts/actions. Graphshell exposes granted views and intents. Live presence and playback coordination have distinct lifetimes from retained material. |

Retain separate content identity, capture identity, publication identity and
contribution identity. Identical payload bytes may be shared by different captures
or contributions. A snapshot's source URL, time and extraction contract are not
deducible from its payload hash. The earlier experiment's HTML-hash snapshot key is
a fixture simplification, not the production identity contract.

Independent personae may build independent histories without publishing links to
other roots. Device identities name offers and observations, with owner/community
authority checked separately. Reputation and rewards are community-issued
assessments over particular acts. Recognition between moots is explicit policy;
ancestry does not transfer membership, obligations or governing power. Fili remains
the named community-descent lane, not a new storage engine for this work.

Regional interest, optional geolocation and measured radio reach can inform a moot.
RF reception alone grants no membership. Personal compute can be offered outward
through successive trust rings under explicit scope, quotas and owner ceilings.
This continuation consumes those contracts; it does not implement the economy.

### Phases and done-conditions

**P1: author-offline publication.** Inventory existing publication addressing,
Gemot contribution records, Eidetic blob resolution and host-serving adapters.
Write down the exact stable publication/update authority and how advertised hosts
are resolved before wiring them. Start with a native peer receipt; then expose the
same accepted bytes through the existing Gemini-facing host adapter. If a serving
adapter is absent, record and implement that bounded adapter in its protocol owner.

Done when the three-role sequence above passes, including host close/reopen;
changed/corrupt bytes are refused; a foreign-moot or unauthorized hosting/update
operation is refused; and an ordinary Gemini client retrieves the public page from
the volunteer host while the author is stopped. A signed revision and raw Gemini
response have separate checks: conventional clients do not inherently verify our
publication signatures. Record process identities, store paths, content hashes,
operation references and actual retrieval source. Distinguish same-machine process
proof from a later two-machine receipt. A self-issued fixture secret on both peers
does not satisfy independent-persona admission.

The 2026-09-05 proof now satisfies the same-machine form of this gate. One
NativeDrop bootstraps constitution, membership, contribution, revision, and content
into a cold host. Attested protocol-derived keys resolve contribution and Standing
events to stable Persona roots. Current constitutional or delegated capability,
write membership, and admission policy authorize the local contribution and hosting
commands; refused commands leave their stores unchanged. The restarted host rebuilds
that authority before serving. The production publication and full hosting record
owners, authority-at-publication frontier, atomic cross-lane import, device adapter,
and two-machine transport receipt remain open.

Production ownership is now ruled. Eidetic owns the signed immutable publication
revision as a typed artifact; the existing Gemot `Shared` event contributes its
manifest without duplicating authorship. Gemot owns the full hosting promise as a
Standing fact. Mesh's compute leases and Stickleback's carrier records remain
unchanged. The production hosting command derives `moot/hosting/<audience>` from
the requested audience itself, intersects community bounds with the device's
local ceilings, and binds the observed authority frontier before signing. The
generic Standing command remains a lower-level primitive and does not choose
hosting authority on the caller's behalf.

Historical authority is a signed causal cut, not a wall-clock claim. Contribution
and hosting records bind the constitution revision plus observed constitution,
membership, and delegation heads. Resolution reports `proven`, `denied`,
`pending missing evidence`, or `legacy unbound`; an incomplete imported prefix is
never treated as denial. Current authority remains the moderation and serving
view, so later revocation can hide or stop serving an historically valid record
without rewriting its publication-time status.

**P2: updates and honest retention.** Publish a second revision under the same
publication identity. Show exact-version retrieval and a policy-selected current
revision; detect competing heads rather than silently treating arrival order as
authority. Expose requested/accepted/available/expired or withdrawn state in the
existing Graphshell surface. Re-evaluate visibility and serving authority when
relevant grants or policies change, including after restart.

Done when an old exact reference remains identifiable, accepted update authority
is checked, an expired promise is not reported as live hosting, an offline host is
not reported as measured availability, and withdrawn content leaves the current
search/serving projection. Withdrawal does not claim to erase copies already held
by recipients. Availability floors, erasure policy and checkpoint pruning remain
separate under the existing deletion/retention plan.

**P3: captured collections.** Move the useful behavior from the isolated Fleece
experiment into the retained consumer: explicit capture, separate contributions,
collection versions/forks, and searchable body text. Retention settings distinguish
records, canonical/reader extraction plus anchors, and replay resources. Preserve
Fleece version and normalization; selector positions refer to canonical DOM text,
not the shorter reader text. Any missing anchor remains explicitly missing.

Fleece 0.4 already supplies canonical DOM text, paired quote/position selectors,
reader structure, structured data, metadata links, and semantic tables. Its 0.5
gate is preservation plus the declared Web Annotation `text/plain` selector
profile: every extract carries the text and reader profile, quote context, and
implementation version; a versioned record validates anchors on decode; pure
range-to-anchor plus anchor-resolution operations support human selections; and
RFC 5147 Fragment, Text Quote, and Text Position selectors resolve to the same
immutable canonical-text resource. Eidetic owns the complete Annotation JSON-LD
envelope and wraps the payload with source URL, capture time, response and DOM-mode
facts, plus raw or replay blob identities. Fleece retains no fetch, storage,
replication, or Moot policy. The complete cross-standard ledger lives in Genet's
`genet/design_docs/2026-09-05_fleece_preservation_contract_plan.md`.

Done when real selected pages survive peer transfer/reopen, a body-only query finds
them, duplicate submissions preserve both contributors while results group content,
and a changed page cannot silently retarget an old annotation. Record unique
content/capture counts, extraction and replay bytes, index build/update cost, heap
use and query latency. Search-engine selection additionally needs held-out relevance
cases and browser validation; the four-fixture receipt does not admit a replacement
for Tantivy. The unchanged consumer seam remains the migration boundary.

**P4: application co-op and lineage.** Reuse the active Woodshed/Graphshell co-op
work, after checking its final implementing receipt. A friend joins a selected Set
or comparison space; independent local views issue granted domain intents. Retain
material, edits and chosen analysis records after both applications close. Record
analysis input revisions, relation selection/depth, musical constraints, method or
model version and accepted result. Personal chronological trails remain separately
selected for retention and sharing.

Done when a second participant reopens the shared work without the original host,
a fork retains ancestry without inheriting ungranted membership/resources, and
Woodshed continues to own musical meaning. A published edition can use P1's hosting
path. Verify actual live coordination separately from retained edit exchange.

**P5: addressed delivery through a mesh.** Add bounded acceptance of addressed
content for later delivery using the existing Stickleback/native-drop path. Keep
receive-for-delivery, retain-for-a-period and publish-to-an-audience explicit.

Done when an intermediary restarts while the recipient is absent, then delivers
the same canonical addressed record once the recipient returns; wrong-recipient,
duplicate and over-budget handling is demonstrated. The intermediary gains no
right to publish or edit by carrying bytes. LoRa airtime/range and Signalman device
management require their own subsequent consumer receipts.

### Findings (2026-09-04)

- `crates/moot/commons/src/lib.rs` exposes `Replica::accept` and
  `projection_with_authority`; use the retained/effective split rather than adding
  an arrival-ordered shared graph.
- `crates/eidetic/eidetic-core/src/manifest.rs` (`BlobManifest`) already separates
  content hash, sources, privacy, provenance and trust. Source advertising and
  community admission still need the P1 consumer proof.
- `crates/mesh/mesh/src/retention.rs` (`AvailabilityPolicy`) distinguishes a hosting
  promise from erasure. `lease.rs` describes author-offline job grants; inspect its
  job-specific scope before claiming publication hosting is implemented.
- The local `Code/experiments/eidetic-moot-20260904/RESULTS.md` receipt exercised
  real Fleece/Personae/Muniment on four synthetic captures and five submissions:
  16 anchors, a changed paragraph, independent journal forks and body-text queries.
  It exercised neither p2panda transport nor service commitments. Its sources and
  reproducible command are local artifacts, not checked-in shipping evidence.
- Concurrent untracked `crates/moot/commons/examples/commons_practice_peer.rs` *(planned target)* <!-- doc-audit: planned-path -->
  describes a line-JSON retained Woodshed space with one redb store per process and
  explicitly no transport implementation. `ports/graphshell/web/co_op.*` and
  related projection work are another active lane. Their presence is not a landed
  co-op receipt and this planning pass does not modify or absorb them.

### Progress

- **2026-09-04:** scoped the continuation and selected P1 as the next implementation
  target. Reconciled the smolweb publishing brief: a moot can coordinate hosting
  for a single author. All P1-P5 implementation and acceptance gates remain open.
- **2026-09-05:** the separate-process P1 rehearsal passed over the live scoped
  iroh blob path. An admitted host fetched the exact Stickleback NativeDrop; an
  unadmitted peer was refused and retained no bytes. The author then exited; a new
  host process reopened the durable ingress, Moot and Standing stores; and an
  isolated fresh reader retrieved the exact page through an ordinary Gemini TLS
  exchange. Distinct Personae roots signed the publication/contribution and
  hosting facts. Corrupt carrier bytes and a foreign-Moot operation were refused;
  proof-local policy rejected unauthorized publication and hosting candidates;
  an unpublished path returned Gemini `51`. The concurrent proof artifact is
  not yet committed; its target and exact ids are:
  [author-offline community publication proof](../research/2026-09-05_author_offline_publication_proof.md) *(planned target)* <!-- doc-audit: planned-link -->
  P1 remains partial: candidate publication/hosting records and explicit fixture
  policy must become production authority. Current `Shared` and Standing folds
  also need attested outer-signer binding to stable Personae roots. The
  Persona-to-device-key adapter and a two-machine receipt remain open.

- **2026-09-06:** the first P3 preservation slice passed and its cross-repository
  adoption landed. Genet Fleece 0.5 at `221415af6643e7b31510547963217973ada6332b`
  carries document-level canonical-text identity, arbitrary
  range mint/resolve operations, the RFC 5147/quote/position selector triple,
  ordered mixed-language and direction evidence, lossless embedded JSON-LD
  blocks, and validated optional wire records. Mere's opt-in
  `mere-document-lanes/eidetic-bridge` binds the selected immutable
  `text/plain; charset=utf-8` resource to caller-supplied capture evidence and
  canonical-page scope, saves it as an Eidetic typed payload, closes Fjall, and
  reopens the same validated Annotation envelope. An independent offline
  `oxjsonld`/`oxrdf` oracle expands the envelope with the official W3C context and
  verifies the expected dataset up to blank-node identity. All 28 root-workspace
  Mere Genet dependency pins move together to that revision. The bridge embeds
  and validates Fleece's `CanonicalTextRecordV1` directly. Fleece's preserved JSON-LD blocks now
  feed linked-data through a document-lanes adapter, and linked-data's duplicate
  HTML string scanner is retired. The detailed adapter retains document order,
  element id, declared media type, and either the contribution or its parse or
  expansion failure; a convenience projection keeps the former best-effort
  successful-results behavior. Existing callers of linked-data's removed
  `from_html*` helpers must extract with Fleece and call this adapter. Re-ingesting
  an old HTML fixture can also change blank-node skolem IRIs where DOM text
  normalization changes the exact JSON-LD bytes; migrations must treat those as
  derived identities and regroup by durable source facts. The Mere adoption gate
  passed 46 focused native tests, strict Clippy for both changed crates, and a
  `wasm32-unknown-unknown` check using the workspace's established `wasm_js`
  getrandom backend. The full workspace resolver still retains Fleece 0.4 through
  the externally pinned `knot-editor`; that repository must widen its Fleece
  requirement before the transitive graph can collapse to one revision. Official
  JSON-LD test suites, captured response/DOM evidence, peer transfer, and
  capture-state preservation remain open.
