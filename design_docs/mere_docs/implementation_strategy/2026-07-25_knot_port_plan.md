# Knot Port Plan

**Date:** 2026-07-25
**Status:** implementation complete locally 2026-07-27. K0 through K7 are
executable. The remaining boundary is narrower than the old plan: concurrent
writers for one document are explicitly refused until a convergence rule
lands. K7 also depends on the completed Cambium text primitive still present as
uncommitted Genet workspace changes; that slice must land on Genet `main`
before a clean remote Mere checkout can reproduce the local K7 build.

**Companions:** genet's
[pelt and knot direction](https://github.com/mark-ik/genet/blob/main/docs/2026-07-24_pelt_knot_direction.md)
(the text-editing ruling and knot's destination) and its
[pelt port boundary](https://github.com/mark-ik/genet/blob/main/docs/2026-07-24_pelt_port_boundary.md),
the [application prospects brief](../../2026-07-24_application_prospects_brief.md)
(the composition thesis this port instantiates), the
[shared-engram commons brief](../research/2026-07-24_shared_engram_commons_brief.md)
(knot as page content class; the two unowned decisions), and the
[Graphshell remote projection host plan](2026-07-22_graphshell_remote_projection_host_plan.md)
(the protocol this port serves).

## 1. Ruling

**Knot is a Mere port at `ports/knot`, beside `ports/graphshell`.** It ships a
host plus a `knot_endpoint` binary, the shape Turnstone and Isometry already
use and G4 already proved.

Not a standalone repository: it has no identity apart from Mere, consuming
chartulary, muniment, eidetic, stickleback, personae, and sibylla. A separate repo
means git dependencies back into `mere.git`, which is what the 2026-07-23
consolidation removed. Not a Genet port either: Genet's cone witness enforces
one-way direction, and Pelt's graph-free discipline is deliberate, so anything
carrying a vault and a graph is disqualified from living there.

A port is also the reversible choice, which is what an incubator wants. It can
be promoted into Turnstone as a pane, or spun out to its own repository if it
graduates into a product. Founding a repository intended to dissolve is the
expensive direction.

**Name:** plain `knot`, per Mark 2026-07-25, favoring brevity over the word
pool. The port shares its word with the document format (`.knot`,
`DjotKnotEngine`); accepted as livable ambiguity.

## 2. What it is

The second half of Turnstone, incubated behind a protocol boundary instead of
inside the app. Turnstone browses; Knot holds and authors. Five properties, from
the originating question of what a local-first alternative to Obsidian and
Anytype would take:

1. read files in place, unencrypted and unmoved;
2. seal a personal vault, with cheap analysis over it;
3. sync across devices over Stickleback and p2panda;
4. choose the format a document is saved as;
5. FOSS and moddable.

The endpoint shape earns four of these for free. `ProjectionSource`
implementations "authorize selection before they disclose a score or scene and
retain ownership of native source data", which is property 1 stated as a
protocol boundary. The vault key lives in the endpoint, so Turnstone mounts
disclosures without ever holding it, which is a better security shape than a
vault inside the app rather than merely a cheaper one.

## 3. Findings: verified stack survey, 2026-07-25

**Already built, reused as-is.**

- **Sync.** Stickleback is the promoted deduplicated core, not the messaging
  domain: `SyncedSpace` is a generic reconciling-log drain whose injected seam
  is an `accept` closure deciding whether a received operation counted. Direct
  exchange, Moot, and Mesh already ride it, so a Knot space is another
  `accept`, not a fork. `drop_io` carries plain and protected drop export with
  receipts and prune proofs. Transports are p2panda and Reticulum.
- **Vault.** personae carries `vault.rs`, `seal.rs`, `passphrase_root.rs`,
  `sealed_record_storage.rs`, `startup_unlock.rs`; eidetic-core has `seal.rs`
  and session-runtime has `engram_seal.rs`.
- **Analysis.** sibylla, `intel/embed`, and `eidetic-search`, all in-process.
  This is the argument against the alternative of bridging an existing notes
  app over MCP, which taxes every session with tool schemas; local embeddings
  over a sealed store cost nothing per session.
- **Storage.** muniment mandates no wire format (pluggable `Codec`, JSON and
  postcard shipped) over backends redb, fjall, zip, and memory.
- **Typing.** chartulary content classes are data, not code: a class is
  `class_id` plus required facets plus schema references, and an unknown class
  reads back as `Unknown` rather than erroring. Turnstone now exercises the
  substrate with built-in web-page and note profiles. Knot adopts it; no class
  registry belongs in this port.
- **Editing.** The former Meerkat readout stack now lives in Genet:
  `knot-editor-host` derives Illume highlights, outline/folds, and Nematic's
  `DjotKnotEngine` preview from one host-owned source buffer. Cambium and
  Genet/Parley now supply the shared edit, IME, selection, movement, and
  geometry primitives. K7 is integration, not a new editor core.
- **Boundary.** graphshell-protocol, -client, -endpoint, and -stdio, with the
  stdio carrier giving a child-process JSON boundary. The G4 receipt mounted
  Turnstone and Isometry sessions into one host with neither product in
  Graphshell's dependency graph.

**Previously missing, now filled.**

- **Disk lane.** `DirectorySource` now discovers a real
  folder, preserves identity across ordinary renames using the filesystem file
  id, keeps bodies/content absent, applies a configurable ignore policy, and
  refreshes on Graphshell snapshot/resume. `DirectoryWatcher` receives native
  recursive OS events and collapses each drained burst into one Servitor-gated,
  watcher-attributed journal transition. Its key is derived from the host
  identity; revoking its grant freezes the accepted directory revision while
  the endpoint keeps serving. A durable facet sidecar remains future storage
  work rather than disk-authoritative file content.
- **File/note classes are filled.** Knot now ships `knot.file` and `knot.note`
  as chartulary class data with Eidetic-compatible facet schemas. This was a
  missing item in the 2026-07-25 survey and is now K2's completed adoption.
- **Writers.** `AuthoredFile` now selects `.knot`, Markdown, Djot, or JSON
  codecs over Genet's Inker/Nematic document model. Untouched source files
  bypass the write path, caller-selected Save As is explicit, foreign formats
  reach a fixed point after one parse-write pass, and canonical `.knot`
  round-trips byte-exactly.

## 4. Gates

- ~~**Text editing blocks the editor half entirely.**~~ **Cleared
  2026-07-27.** Cambium now owns grapheme-correct selection, edit commands,
  composition, and bounded undo; Genet/Parley owns visual movement, affinity,
  hit-testing, and caret/selection geometry; `cambium-winit` supplies key and
  IME translation. Woodshed and Isometry exercise both host-routing shapes.
  Knot remains a consumer through `knot-editor-host`.
- **Multi-writer convergence and group keys block concurrent editing.** Both
  are named unowned in the commons brief. One writer per device is fine today;
  two devices editing one container offline is undecided, and that is exactly
  the case that motivated this port. The watcher sharpens it (Mark,
  2026-07-25): an autonomous disk-side writer makes collision-resistant
  editing matter before any multi-user scenario. The endpoint serializes the
  writers it owns, so the gate stays multi-device, but the decision's
  priority rises.
- ~~**Livery is the declared long pole.**~~ **Cleared 2026-07-27.** The Genet
  workspace build is green. Knot's remaining work is its own storage,
  authority, writer, and integration ladder.

## 5. Sequence

Done-conditions, not dates. Every rung is complete locally. The open
same-document multi-writer rule remains a product boundary rather than hidden
inside any rung.

- **K0. Port scaffold. Complete locally 2026-07-27.** `ports/knot` is a
  workspace member and `knot_endpoint` discloses a fixed fixture through the
  resumable stdio carrier. `g4_sessions` mounted it beside Turnstone and
  Isometry: four sessions from three endpoint processes. The committed receipt
  regenerated byte-identically.
- **K1a. Files-in-place projection. Complete locally 2026-07-27.**
  `DirectorySource` recursively indexes a caller-selected folder under a
  configurable ignore policy. Containers carry `file:` addresses, titles,
  media types, and facets while `body` and `content` stay absent. Native file
  identity preserves the container id and foreign facets across rename. A disk
  edit increments the projection revision and returns a replacement snapshot
  on the next resume.
- **K1b. Autonomous watcher and authority. Complete locally 2026-07-27.**
  The recursive native watcher acts
  autonomously under its own servitor identity: its journal ops carry
  attribution, and revoking its grant is the pause switch. The disk stays
  authoritative for file-backed containers; graph-side facets are
  journal-only, and body edits (once the editor exists) write through to the
  file, with the endpoint serializing the two writers it owns. Bulk churn
  (checkouts, sync tools) is debounced behind the existing ignore policy. Done
  when an OS event produces one attributed journal transition; revoking the
  watcher grant pauses observation without stopping the endpoint; and a burst
  collapses into one revision. Tests cover all three.
- **K2. File and note content classes. Complete locally 2026-07-27.**
  `knot.file` requires `file.document`; `knot.note` requires both
  `file.document` and `note.document`. Their schemas use the existing
  `SchemaFacetValidator` seam. Tests prove a known note admits, malformed known
  facets fail, and an unknown class remains inert and discoverable.
- **K3. Vault lane. Complete locally 2026-07-27.** Personae sealing lives
  inside the endpoint. Tests assert that stored bytes are sealed, lock drops
  decrypted state, a wrong key cannot reopen it, and Graphshell disclosures
  contain neither the key nor authored body.
- **K4. Analysis. Complete locally 2026-07-27.** A Sibylla index spans the
  disk and vault lanes. The vault
  lane's index is derived content and lives under the seal, because
  embeddings invert. Queries are served in-process by the unlocked endpoint
  under grants, so agents keep the cheap read path: one unlock at startup,
  zero marginal crypto per query, and selectivity is a grant scope rather
  than a second crypto tier (a denizen with a vault-scoped grant gets vault
  hits; one without gets disk hits only). Tests span both lanes, prove grant
  selectivity, assert sealed index bytes, and remove vault hits on lock.
- **K5. Sync. Complete locally 2026-07-27.** Knot supplies its own encrypted
  event grammar, admitted-device policy, store, and Stickleback `accept`
  closure. Two memory-backed instances converge through that seam, then two
  real p2panda peers reconcile independently authored logs. A second writer
  touching the same document returns `ConcurrentWriter`; the port does not
  improvise last-writer-wins while the commons decision remains open.
- **K6. Writers. Complete locally 2026-07-27.** Per-format serializers render
  to `.knot`, `.md`, `.djot`, or `.json`. Untouched files never enter the
  write path. Tests prove foreign-format fixed points, byte-exact canonical
  `.knot`, and caller-selected output format.
- **K7. Editor half. Complete locally 2026-07-27.** `KnotEditor` consumes
  Cambium's `TextInput` and `TextCommand` as its only source buffer, preserves
  byte-plus-affinity layout selections, and keeps `KnotReadout` derived for
  highlights, outline, folds, and preview. IME preedit stays outside committed
  source, undo restores every readout, and committed source writes through to
  `.knot`. Reproducibility still waits on the corresponding uncommitted Genet
  primitive slice reaching `main`.

**Carrier note.** The stdio carrier is pull-only: every frame is
host-initiated (`CarrierRequestBody` is Discover, Snapshot, Resource, Resume,
Intent) and every response is keyed to a request id, so an endpoint cannot
volunteer a diff. K1 therefore ships on revision-addressed resume polling,
which G2 already made cheap. The destination, proposed 2026-07-25, is a
revision bell: one endpoint-initiated frame, `CarrierNotice { session, epoch,
revision }`, carrying no scene payload; the host marks the mounted scene
stale and re-resumes. Disclosure authorization is untouched, since content
still flows only through `ProjectionSource` on request, and the leak surface
is "something changed" alone. Additive, minor protocol version bump; the
endpoint writer needs line-atomic interleaving between responses. A held-open
watch request was considered and rejected: the serial serve loop would
head-of-line block intents behind it. Recorded here because knot forces the
need; the change itself is graphshell-protocol work.

## 6. Non-goals

Not a new repository. Not a Turnstone fork, and not a rival to it: the protocol
boundary is what keeps this a pressure vessel that promotes stable pieces
rather than inverting into a second product, the same rule Strophe holds for
the audio layer. Not an IDE, per the Genet ruling that knot's destination is
the authoring browser. No separate notes product competing with Turnstone for
the same users.

## Progress

- **2026-07-25.** Port home ruled, name chosen, stack surveyed against the
  actual tree, gaps and gates recorded. No code. Same session: the open
  question in Genet's pelt/knot direction doc resolved (the schema is the
  shared basis; content classes unify, stores diverge, settings excepted).
- **2026-07-25, review pass with Mark.** Carrier verified pull-only against
  `graphshell-protocol` (K1 ships on resume polling; the revision bell is
  proposed in the carrier note). K1 gains rename-preserves-identity, the
  disk-is-authoritative and write-through rules, the servitor identity for
  the watcher, and debounce over bulk churn. The multi-writer gate records
  that an autonomous disk writer raises the convergence decision's priority.
  K4 seals the vault-lane index and routes agent reads through grants at
  zero marginal crypto cost. K6 drops universal byte-identity: foreign
  formats round-trip idempotently, `.knot` byte-exact.
- **2026-07-27.** The Genet text-editing primitive completed T0 through T4 and
  cleared K7's implementation gate. Fullweb forms and contenteditable remain
  later consumers of the same primitive and do not block Knot.
- **2026-07-27, live-tree recut and first execution.** The old plan was stale
  in three places: replication had promoted to Stickleback, content classes had
  landed in chartulary and Turnstone, and the Knot readout/editor support had
  moved into Genet. K0 through K2 landed in that first pass, where `cargo test
  -p knot` passed twelve tests and warning-denying `cargo clippy -p knot
  --all-targets --no-deps` passed. The dependency-inclusive warning-denying
  Clippy gate stopped in pre-existing `numen::FieldId`/`CouplingId`
  `new_without_default` findings.
- **2026-07-27, completion pass.** K3 through K7 landed locally. `cargo test -p
  knot` passes 28 tests, including real p2panda convergence.
  `cargo clippy -p knot --all-targets --no-deps -- -D warnings` and `cargo
  check -p knot --all-targets` pass. The independent G4 receipt still mounts
  four sessions from Turnstone, Isometry, and Knot and regenerates at
  `28459BF5591CFB67CCCAD09BF5D2AFCD1F829FADE644C724F1E4DEBC6A076E60`.
