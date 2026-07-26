# Knot Port Plan

**Date:** 2026-07-25
**Status:** ruled, no code yet. Records the port's home, its shape, a verified
survey of what the stack already provides, the three gaps, and a sequence with
done-conditions. The editor half is gated; the endpoint half is not.

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
chartulary, muniment, eidetic, murm, personae, and sibylla. A separate repo
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
3. sync across devices over Murm;
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

- **Sync.** `murm/replication` is the deduplicated core, not the messaging
  domain: `SyncedSpace` is a generic reconciling-log drain whose only injected
  seam is an `accept` closure deciding whether a received operation counted.
  Direct exchange, Moot, and mesh already ride it, so a knot space is a fourth
  `accept`, not a fork. `drop_io` carries plain and protected drop export with
  receipts and prune proofs, a sneakernet lane no comparable product has.
  Transports are p2panda and reticulum.
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
  reads back as `Unknown` rather than erroring. A modder ships a class in a
  pack.
- **Boundary.** graphshell-protocol, -client, -endpoint, and -stdio, with the
  stdio carrier giving a child-process JSON boundary. The G4 receipt mounted
  Turnstone and Isometry sessions into one host with neither product in
  Graphshell's dependency graph.

**Missing, and the reason this plan exists.**

- **No disk lane.** Every `impl Backend` in the tree is redb, fjall, zip,
  memory, or a Murm conversation shim. The `file` scheme appears only inside
  `register-protocol`'s own contract tests, so the seam exists and the provider
  does not. `crates/import` is browser data only: bookmarks, history, sessions.
- **No note or file content class.** `mere.note` appears in the tree only as a
  doc-comment example. The commons brief names the shared vocabulary as page,
  post, place, person, and file, so the slot is declared and unfilled.
- **No writers.** `glossary` projects djot outlines of a graph and `scholia`
  exports JSON-LD and N-Quads. Neither is "serialize this node as a `.md`
  file", which is what property 4 needs.

## 4. Gates

- **Text editing blocks the editor half entirely.** Ruled in the Genet
  direction doc: one primitive at the cambium/genet layer, three consumers
  (toolkit `text_input`, fullweb forms then contenteditable, the knot editor).
  `knot-editor-host` is the lexer and readout half only; real selection, IME,
  and undo are unbuilt. This is the largest single item the port depends on and
  it is not ours to build here.
- **Multi-writer convergence and group keys block concurrent editing.** Both
  are named unowned in the commons brief. One writer per device is fine today;
  two devices editing one container offline is undecided, and that is exactly
  the case that motivated this port. The watcher sharpens it (Mark,
  2026-07-25): an autonomous disk-side writer makes collision-resistant
  editing matter before any multi-user scenario. The endpoint serializes the
  writers it owns, so the gate stays multi-device, but the decision's
  priority rises.
- **Livery is the declared long pole.** This port is queued behind it the same
  way the agent-drives-pelt receipt is, deliberately, so it does not compete
  for focus.

## 5. Sequence

Done-conditions, not dates. K0 through K2 clear no gate and can start whenever
focus allows; K7 waits.

- **K0. Port scaffold.** `ports/knot` exists as a workspace member with a
  `knot_endpoint` binary that discloses a fixed fixture. Done when
  `g4_sessions` mounts it beside the Turnstone and Isometry endpoints and the
  receipt is committed and byte-compared like G1's.
- **K1. Disk lane.** A directory `Backend` plus a watcher. The watcher acts
  autonomously under its own servitor identity: its journal ops carry
  attribution, and revoking its grant is the pause switch. The disk stays
  authoritative for file-backed containers; graph-side facets are
  journal-only, and body edits (once the editor exists) write through to the
  file, with the endpoint serializing the two writers it owns. Bulk churn
  (checkouts, sync tools) is debounced behind an ignore policy. Done when a
  real folder discloses as chartulary containers whose `Addressed` primary
  address is the file path, with no file copied, moved, or encrypted; a
  rename on disk preserves the container and its facets, changing only the
  address; and a disk edit is visible to a mounted host on its next resume.
- **K2. File and note content classes.** Class documents plus facet schemas,
  filling the slot the commons brief declares. Done when an unknown-class file
  reads back `Unknown` and a known one admits.
- **K3. Vault lane.** personae sealing inside the endpoint. Done when the host
  mounts disclosures while the key never leaves the endpoint process, asserted
  in the test rather than merely arranged.
- **K4. Analysis.** sibylla index across the disk and vault lanes. The vault
  lane's index is derived content and lives under the seal, because
  embeddings invert. Queries are served in-process by the unlocked endpoint
  under grants, so agents keep the cheap read path: one unlock at startup,
  zero marginal crypto per query, and selectivity is a grant scope rather
  than a second crypto tier (a denizen with a vault-scoped grant gets vault
  hits; one without gets disk hits only). Done when a query returns hits
  spanning both lanes, and with the endpoint locked, vault hits are absent
  and the vault index bytes on disk are sealed, both asserted.
- **K5. Sync.** The port's own `accept` closure over `SyncedSpace`. Done when
  two instances converge over Memory, then p2panda, reusing the managed-network
  plan's admission matrix. Concurrent edits to one container stay out of scope
  until the commons brief's decision 1 lands.
- **K6. Writers.** Per-class serializers, so a class renders to `.md`, `.djot`,
  or `.json`. Untouched files are never rewritten; files-in-place gives that
  outright. Done when foreign formats round-trip idempotently (one
  parse-write pass is a fixed point) and `.knot`, where both directions are
  ours, round-trips byte-exact.
- **K7. Editor half.** Gated on the cambium text primitive. Not scoped here.

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
