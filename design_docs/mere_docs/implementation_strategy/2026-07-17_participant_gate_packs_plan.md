# Participant Gate and Packs Plan

**Founded:** 2026-07-17 (design round with Mark; no code changed this round).
**Scope:** cross-repo (mere, merecat, chartulary, retinue, personae); canonical copy lives here per the moot/retinue precedent.
**Companions:** the archived [document script substrate plan](../../archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md) (the proven wasm host + §11 P2 roadmap this plan builds on), the [runtime mod authoring loop plan](2026-06-30_runtime_mod_authoring_loop_plan.md) (the authoring ergonomics for the same artifacts), the [commitment proof interface plan](2026-06-30_commitment_proof_interface_plan.md) (the proof surface tessera receipts ride), the [kith capability sharing plan](2026-06-30_kith_capability_sharing_plan.md) (grants, revocation/expiry, live capability refs vs offline signed proofs: the trust-widening slice participant grants should reuse), and `merecat/design_docs/2026-07-10_merecat_architecture_plan.md` (doctrine 2: one Action vocabulary; the piccolo lane).

## The decision in one line

Every non-UI actor is a **participant**: an identity (personae) holding a grant, proposing typed batches through one authority gate that validates and applies them via the journal. Participants reside in the graph as nodes bearing **nested graphs**; their portable form is a signed pack (likely an engram profile) distributed over retinue and curated by moots. Extensions, remote collaborators, agents, and scenario runners are four costumes on this one mechanism.

## Vocabulary rulings (2026-07-17, Mark)

- **nested graph**: a graph, a set of relations, contained *within* a node. This is the only containment sense; when containment is meant, say nested graph and avoid the bare word "subgraph".
- **graphlet**: the existing forme mechanism, an ego/component *scope over real kernel nodes* with a Frontier (see the [relational browse graphlet plan](2026-06-23_relational_browse_graphlet_plan.md)). A graphlet is peer-scoping, never containment, and is not the participant unit.
- **swatch**: a canvas representation surface (gnodes render "in an orrery or swatch"). A swatch may *render* a nested graph; it is a representation, never an identity. Proposed TERMINOLOGY entry, pending Mark's wording pass: "swatch: a scoped graph-canvas view; the representation of a graph or nested graph, distinct from the graph itself."
- **servitor** (DECIDED 2026-07-17, Mark): the resident helper unit, an installed extension or local agent. Chosen from the chaos-magic sense (created, named, task-scoped, dissolvable: grants plus revocation in one word); crate name reserved on crates.io the same day (`servitor` 0.0.1, repo `repos/servitor`). *Animula* is banked as the companion-flavored runner-up if a second name is needed. Scope note: a human moot peer is never a servitor, so **participant** stays the working umbrella word for any gate actor pending its own naming pass.
- **pack**: working name only. "Pack" may dissolve entirely into engram vocabulary (§4). "Command packs" already appears in the mod authoring loop plan, so the word has precedent.

- **denizen** (DECIDED 2026-07-17, Mark): the umbrella gate-actor word, replacing working *participant*: a personae identity holding a grant and the right to submit petitions. Human moot peers, servitors, and scenario runners are all denizens; the trusted UI is not. This plan keeps its founding filename; *participant* in older prose reads as the pre-ruling working word.
- **petition** (DECIDED 2026-07-17, Mark): a denizen's proposed change, replacing working *proposal*. The journal records granted petitions.
- **pack / mod** (DECIDED 2026-07-17, Mark): both kept, split by trust depth: *pack* is the plain user-facing word for shallow-rung bundles (rungs 1-2: scenario/macro data, scripts), *mod* for deeper-rung bundles (rung 3 and past) whose grant reaches further. One envelope underneath (engram profile expected, B4 confirms). Coheres with the existing `register-mod-loader` / `WasmModRuntime` naming.

All rulings (nested graph, servitor, denizen, petition, pack/mod, swatch with Mark's correction that the gloss is a pane *containing* a swatch, never a swatch itself) are in [TERMINOLOGY.md](../../TERMINOLOGY.md) as of 2026-07-17.

## Design

### 1. The gate

One authority pipeline for every petition, regardless of who petitions (transaction boundary corrected 2026-07-17 per the moot-agent review):

```text
authorize (personae proof + grant scope) -> compare revision -> commit attributed graph batch -> enqueue idempotent app effects
```

- A **petition** is a typed batch with two halves that commit on two different disciplines: graph edits, which lower to chartulary `GraphEdit`/`CapturedDelta` and commit atomically through the journal, and app effects (fetch, navigation, windows, network), which **cannot roll back** and therefore never precede the commit: they enqueue after the graph batch lands, idempotent, each carrying the batch id. The stringly forms (`dispatch(string)`) are rejected by construction; the WIT and script surfaces mirror the typed Action vocabulary (merecat's piccolo lane already proves the emit-typed-Actions-apply-later half at `src/script.rs`).
- **Revision check** is compare-and-append against the journal. Conflicts return the current revision so the proposer can rebase (adopt `document-core`'s `turn-error::revision-conflict(u64)` shape, already proven in `mere/crates/script/document-host`).
- **Attribution**: every committed batch records the denizen's identity in the journal envelope; stemma carries provenance; scholia carries the user's own annotations on what a denizen did. Honesty bound (2026-07-17): today's `GraphEdit` retains no inverses (a `RemoveNode` keeps neither the payload nor its edges), so the promise at B1 is **auditability plus compensating actions**, never universal undo; universal undo waits for a journal that records pre-state or reversible operations. Prefix replay reconstructs any historical state, but selective inversion of one batch with later edits retained is not available today.
- **Trust tiers, one authority model, more than one code path**: the live UI is the maximally trusted actor and skips the petition queue and optimistic retry (it cannot pay round-trips per keystroke), but it still writes through the **same attributed commit path**, so every journal entry has an author regardless of tier. Scripts, components, remote moot peers, and agents petition through the gate. The *vocabulary* is uniform (doctrine 2); the mechanism is tiered.
- **Journal ownership, chartulary vs mere (clarified 2026-07-17)**: chartulary's `commit` module owns the envelope *semantics* (attributed batch, expected-revision compare, refusal carrying the current revision, effects post-commit) and is the concrete journal for graphs speaking `GraphEdit`: denizen nested graphs and generic consumers. Mere's kernel graph journals through its own `GraphJournal` (a codicil of `CapturedDelta`, whose content vocabulary chartulary's topology-only `GraphEdit` deliberately cannot express, per that module's own doc), and today it records single unattributed deltas. B1's gate therefore lands against `GraphJournal` by **adopting the same envelope over `CapturedDelta`**, never by unifying the two edit vocabularies. Recommended path, decided at B1: promote the envelope shape into codicil (an attributed batch generic over the entry type) so both journals share one envelope type; that chains a codicil push, which is why it waits.
- **Scope**: topology and content only. Spatial influence (positions, layout) is deliberately excluded; a participant that wants layout influence writes a numen *field*, a separate capability designed after the aether field extraction, never raw positions.

### 2. Residency: the participant node

A participant lives in the graph as a node bearing a nested graph.

- **chartulary containment capability** (working name `GraphBearing`, following the `caps.rs` pattern where each trait unlocks a feature):

  ```rust
  // illustrative only, not implementation-ready
  pub trait GraphBearing {
      /// The contained graph's id, or None if the node bears no graph.
      fn nested(&self) -> Option<GraphId>;
  }
  ```

  Landed in B0/B0.5 (see the build order): a nested graph's identity is its `LogId` (no `GraphId` newtype), the slot convention is the registry, one muniment slot per nested journal, lifecycle owned by the bearing node (removal archives journal-safely and never orphans; interrupted removals recover idempotently on load; self-bearing is rejected at commit). Nested graphs are ordinary `chartulary::Graph`s: journaled, queryable, syncable.
- **Denizen node anatomy**: the node body (`ContentBearing`) is the signed manifest blob. The nested graph holds: **grant projections** (readable statements projected by the gate, never the authority source: authority materializes from signed personae/moot/kith evidence per the kith capability sharing plan, and the graph is the explanation and local index. A self-editable authority root would let a servitor grant itself power, so the gate refuses petitions that touch grant projections), the storage root (a muniment namespace keyed by denizen id), one journal-cursor node per event subscription (crash-restart resumes from the cursor), and registered commands/panes as nodes.
- **The payoff**: the palette is a query over denizen nested graphs; "what can act on my data, what did it do, why do I trust it" is a browsing question; a swatch over the denizen's nested graph *is* its inspection UI. The orrery examines itself.
- **Uninstall means archive/disable, never deletion** (2026-07-17): B0's archive move is the mechanism, and the denizen's storage namespace and referenced blobs get an explicit retention rule under the one deletion/retention law (see the deletion, retention and native drop plan) rather than an implied purge.

### 3. The power ladder

| Rung | Form | Runtime | Distributable | Web-safe |
| --- | --- | --- | --- | --- |
| 1 | Action macro / scenario data | none (data) | yes | yes |
| 2 | piccolo/rhai script | script engine | yes | yes |
| 3 | wasm component | wasmtime (native), jco later | yes | later |
| 4 | native crate | compiled in | no | n/a |

Every rung emits the same proposals through the same gate and surfaces in the same palette. Rung 1 is the cheapest extension and needs zero new runtime ("open my research trail" is a rung-1 pack). Rung 3 rides the substrate plan's P2.3 (linker policy) and P2.4 (`WasmModRuntime` bridge) unchanged, plus a typed merecat world beside `document-core`.

### 4. The envelope: packs are (probably) engrams

**engram** is already canon: the portable contribution payload, a `TransferProfile` envelope plus typed `EngramMemory` items ("Gist" was retired *for* it). The design intent is that an installable pack **is an engram profile**, not a second envelope.

**Spec re-read DONE 2026-07-17, both sources** (first the donor `graphshell/.../engram_spec.md`, TransferProfile v1, 2026-02-28, from the GitHub archive; then, on Mark's catch, the **current implementation**: `eidetic-core/src/engram.rs` + `schema.rs`, which supersedes it). The current `Engram` is already most of what packs need: `{ schema: SchemaRef, payload: Vec<u8>, content_hash, privacy, provenance, trust, bounds, envelope_version }`, immutable (refresh mints a new engram), identity = the BLAKE3 content hash (multicodec-tagged for hash migration), three classification axes kept deliberately orthogonal, serde-default forward compatibility, and crucially **schema-by-reference**: `SchemaRef` is a content-addressed pointer to a schema engram, so a "profile" is a schema, never an envelope variant. Verdict, corrected accordingly:

- **No envelope work exists for B4.** A pack is an eidetic `Engram` whose `SchemaRef` names the **pack schema** (a schema engram, content-addressed like any other). The donor-v1 "sibling envelope family" framing is dead; the current envelope already carries content-hash identity (which v1 lacked at the envelope level) and personae-compatible trust/provenance axes.
- **The donor's one genuinely good multi-part idea imports as pack-schema vocabulary, not envelope structure**: the typed part inventory (kind + location + hash + `required_for_application` + redaction state, with Embedded vs ContentAddressed locations) lives *inside the pack payload*, referencing large parts (wasm components, media) as muniment blobs by hash. Blob transfer rides murm-replication's existing domain-driven blob collection and content hydration (per the deletion/retention plan), and later retinue resource transfer.
- **The v1 rejections stand**: the model-adaptation required fields and closed adaptation memory-kind enum stay out of the pack schema. The composition payoff survives and gets cleaner: an agent servitor's pack and its model customization are two schemas (or one composite pack schema whose inventory references both), under one envelope type that already exists in code.

So B4 shrinks to: define the pack schema (part-inventory vocabulary + contribution manifest), bind personae signing into `TrustEnvelope`, and build the adapter to the runtime manifest.

**How the pack schema is defined (grounded 2026-07-17 in `eidetic-core/src/schema_def.rs`)**: eidetic already has the schema machinery. A schema engram's payload is a `SchemaDefinition { format, schema_id, body }` over three formats (`MereNative`, `JsonSchema`, `JsonLd`), with a self-terminating meta-schema and a `TypedPayload` trait (a Rust type declares its `schema_ref()`; `load_typed`/`save_typed` round-trip it). So the pack schema is **authored, not built**: a `SchemaDefinition` with `schema_id = "mere.pack/v1"` (MereNative or JsonSchema), and the pack payload type implements `TypedPayload`. No new envelope, no new schema system, just one schema engram plus one payload struct. The part inventory and contribution manifest are that struct's fields; large parts reference muniment blobs by hash.

Envelope contents: manifest (publisher identity, target worlds/rungs, capability requests, static contributions: commands, panes, event subscriptions), content (documents, assets), optional content-addressed wasm components, optional scenario data. Static contributions in the manifest mean the palette can list a pack's commands without instantiating anything (lazy activation).

Signing is personae. Trust states reuse the existing `DocumentTrustState` ladder: Trusted (signed envelope verified), Tofu (first-contact publisher), Broken (signature mismatch), rather than a new enum.

**Pack trust is not install authority** (2026-07-17): `Trusted` means the publisher's signature verifies, nothing more. Installation mints a **local grant** after a visible review of the requested capabilities, and an upgrade that widens its requests goes through the same review; a narrower or unchanged upgrade does not re-prompt.

**The pack-manifest to runtime-manifest adapter (defined 2026-07-17)**: the pack manifest is envelope-tier data: publisher identity and signature, target worlds and rungs, capability *requests*, contributions, content list. The runtime's `ModManifest` (register-mod-loader: `mod_id`, `display_name`, `capabilities`, `document_script_origins`) is **derived state**, minted by the adapter at install and re-derived on any grant change: `mod_id` from the pack id (content hash, publisher-scoped), `display_name` from the pack title, `capabilities` from the **granted** subset (never the requested set), `document_script_origins` from granted origins where the pack binds document scripts. One direction only: pack manifest plus local grant in, runtime manifest out; a runtime manifest is never hand-edited and never a source of authority.

**The existing wasm grant bridge, scoped (2026-07-17)**: register-mod-loader plus document-host's linker policy grant *runtime capabilities*: which imports link at instantiation, which origins a document-script binds. That is import-level enforcement for the document lane, and it is not the gate: it validates no petitions and holds no journal authority. Until B3, wasm mods have no app-graph authority at all, because their world has no graph imports to link. At B3 the one local grant projects into both enforcement points: the linker/profile table at instantiation (ungranted means unlinked means unreachable) and the gate's petition validation at commit. The bridge stays; it becomes the import-level face of the same grant.

### 5. Distribution: retinue rails, moots as stores

- A publisher is a personae identity reachable as a retinue destination. Publishing announces the pack id (content hash) plus manifest hash; fetching is a retinue resource transfer.
- **The moot hand-off already exists at the wire level (found 2026-07-17 in `gemot/src/moot/records/wire.rs`)**: `MootEvent::Shared { manifest_id: [u8;32], schema_id, title, at_ms }` shares an engram into a moot's fauna by CID plus its claimed schema, as a signed p2panda operation the roster folds. A pack shared into a moot is exactly this event with `schema_id = "mere.pack/v1"`; the moot store and its trust/tessera layer wrap it. The wire comment notes "blob transfer is a later milestone; the reference is the hand-off", which is precisely the retinue-R4 gate B5 already carries. So B5's moot-curation half is largely wiring an existing event to the pack schema, and only the blob transfer waits on R4.
- **Hard gate**: retinue R4 resource transfer is partial (advertisement codec done and verified; the windowed transfer state machine remains). B5 is blocked on R4 completion. Until then: file-based install, and murm-lane transfer between kith.
- **Curation is flora**: a moot's flora accumulates its endorsed packs; tessera receipts attach to pack ids and validate across coalitions (proof mechanics per the commitment proof interface plan). Moots are the stores; there is no central registry to run or to trust.
- Isometry campaign packs ride these same rails (same envelope, isometry's own world inside).
- **Revocation is an open design** (sketch: personae key revocation plus moot-distributed denylists); it gets its own round before B5 ships.
- **Web**: retinue and wasmtime both thin out in the browser; packs degrade to rungs 1-2 content there.

### 6. Agents as participants (the MAG dynamic, 2026-07-17 addendum)

Mark's framing: the participant concept is the PSO MAG, a small resident companion that is fed, grows through how it is raised, augments passively, and acts autonomously only in bounded bursts. This is more than a mascot metaphor; each MAG behavior names a design requirement, and the gate is the harness models/agents require:

- **Resident, not invoked**: an agent is a participant node with a nested graph, persistent across sessions, hosted as an armillary actor, running on vates (or a granted-net remote model; the harness is model-agnostic).
- **Feeding is granting plus memory**: what you feed it is what it may see (query grants, event subscriptions) and what it retains (scoped storage; sibylla embeddings over what it has been shown). Its nested graph is its stomach and its memory, and a swatch over that graph is the inspection UI: everything it knows and everything it did is browsable.
- **Evolution is earned trust**: grant widening over time, driven by demonstrated behavior, the existing DocumentTrustState ladder (Tofu upward), and tessera receipts. A helper's capabilities grow the way a MAG evolves: from how it is raised, not from a version bump.
- **Photon blast is the prompt tier**: routine proposals flow under standing grants; large or dramatic actions sit behind the Prompt posture from the §11.4 grant mechanism, requiring an explicit user trigger.
- **It cannot hurt you**: the gate's guarantee, identical to every other denizen: it is physically unable to exceed its grant, every act is attributed in the journal, and every act is auditable with compensating actions (universal undo arrives only with a pre-state-recording journal; see §1's honesty bound).
- **A raised helper is sharable**: manifest + model config/prompt + optional wasm tools + grant requests is just a pack; a well-raised helper circulates through a moot's flora with tessera endorsements, like MAG-raising culture.
- **Personal vs collective**: this section names the personal helper. The moot-level collective agent is a different creature (Mark's word list already earmarks *egregore* for moot group-mind); it would be a participant owned by a moot rather than a person.

### 7. What this plan does not change

The near-term build order stands: P2.3, P2.4, the typed merecat world. This plan wraps residency, envelope, and distribution around them, and *deletes* future work by unification: moot needs no separate ACL system (a remote peer is a participant), agents need no separate sandbox (an agent is a participant), apps need no per-app package format (one envelope, many worlds).

## Build order (targets, not durations)

- **B0, nesting spike** (chartulary): **DONE 2026-07-17.** `GraphBearing` capability trait (caps.rs, table row added), `Container.nested: Option<LogId>` (serde-defaulted, pre-nesting data loads unchanged), and a new `nested` module: slot convention (`nested/<log-id>/{log,snap}`, archive under `archive/nested/...`), `nested_of` / `live_nested` lookups, codec-agnostic atomic `archive_nested` (raw-backend `WriteOp` batch), and `remove_bearing_node` (archive lands before the node leaves; a failed archive leaves the node in place). Receipts: 25 tests green (22 prior + 3 new: persist/replay round-trip following the bearing, archive-never-orphan lifecycle incl. replay agreement, pre-nesting serde compat); clippy clean on the new code (5 pre-existing warnings elsewhere untouched). Two follow-up rules from the moot-agent review (2026-07-17), owned by B0.5: (a) **crash window**: the nested archive commits durably before the parent's removal edit persists (a separate, later log save), so a crash can leave a parent bearing an id whose live slots are empty and archive slots populated; that state is defined as *archived-pending-removal* and recovery completes it idempotently on load. (b) **attachment guards**: `nested: Option<LogId>` permits self-reference and cross-graph cycles today; self-reference rejection lands with B0.5's commit API, cross-graph cycle rejection is TOCTOU-bound across independently-editable graphs, so traversal visited-sets stay the primary guard with a best-effort gate-side check at petition time.
- **B0.5, the live gate seam** (chartulary + first consumer): the review's keystone insertion, because the gate is a design until the journal has a transactional, attributed batch API. Contents: (1) revision-checked batch commit on the journal (envelope: batch id + denizen identity + expected seq; refusal carries the current revision for rebase; the log entry type gains an attributed envelope, and migration of existing bare-`GraphEdit` logs is decided here, see open question 7); (2) the post-commit effect queue discipline (idempotent app effects carrying the batch id, drained only after the graph batch lands); (3) authority materialization (signed personae/moot/kith evidence in, grant projections out into the denizen's nested graph; petitions touching projections refused); (4) the B0 follow-ups: archived-pending-removal recovery and self-reference rejection at attach. Done when: a stale-revision batch is refused with the current revision; a committed batch reads back attributed to its denizen; effects demonstrably drain only post-commit; a self-bearing attach is rejected; a simulated crash between archive and parent-save recovers idempotently.
  **Substrate half DONE 2026-07-17** (chartulary `2ced0fb`): journal entry type is now `Batch { author, edits }`; `commit_batch(author, expected, specs)` prechecks every spec against graph + batch-local state before anything mutates, mints edge ids during lowering, refuses stale wholesale with the current revision; convenience mutators became attributed single-spec commits at current revision (the trusted-UI path: same envelope, no optimistic retry); pre-gate logs migrate on load (single-edit batches, author `pre-gate`, identity carried); `commit_bearing_batch` rejects self-bearing; `recover_archived_bearings` completes archived-pending-removal idempotently. Receipts: 33 tests (8 new, covering every done-condition above), clippy clean on the new files. **Remaining half, moves with B1**: authority materialization (signed personae/moot/kith evidence in, grant projections out) is gate-side and cannot live in chartulary (which attributes but does not authenticate), so it lands with the first consumer.
- **B1 grounding (2026-07-17), the keystone interlock**: exploring mere for the residency home turned up `session-runtime/src/wallet_grant.rs` (2767 LOC), the **device-tier** of the kith capability-sharing plan: `DeviceGrantPayload { personas, scopes: Vec<String>, attenuations, expires_at_ms, wrapped_private_epochs }`, Ed25519-signed over canonical CBOR, with `issue_device_grant` / `verify_device_grant` / revocation-with-epoch-rotation and per-persona `capability_slots`. This is precisely the signed-evidence-to-grant shape the denizen gate needs, one tier up (subject = a denizen, not a paired device; scopes = graph/app capabilities, not `identity.act`/`private.read`). `persona/identity` supplies `PersonaId` + `Ed25519Keypair::{sign,verify}`. mere's `Node` (graph-kernel, 179 LOC) does not implement `GraphBearing` yet; adding it plus a `nested` field is the residency hook. **Note (corrected below)**: `DeviceGrantPayload` is the device tier and does NOT umbrella denizens; the grant model resolved against prior design (open question 8) is the three-layer capability stack + the `gemot::MootAuthorizationProvider` seam, not a wallet_grant derivation. The residency data-model (denizen node + grant *projection* into the nested graph + the gate refusing projection edits, authority read from the provider's `capability_covers`) is the proposed first commit and needs no wallet_grant edit.
- **B1 residency CORE landed 2026-07-18** (servitor `1af0c91`, the reserved crate's first real content, built headless so it touches neither mere's tree nor wallet_grant): `Subject` (32-byte keyholder, the shape gemot's authorization seam speaks), `Grant` + `AuthorityProvider` (scoped structural cap + replaceable coverage boundary mirroring `gemot::MootAuthorizationProvider`; `PrefixAuthority` the minimal stand-in until the meadowcap layer is built; the gate depends on the trait so the richer provider drops in unchanged), and the `Gate` (one pipeline over a denizen's nested chartulary `GraphLog`: refuse petitions touching a grant projection → check authority → check scope → attributed revision-checked `commit_batch`; grants project read-only, gate-authored, so authority is browsable from the graph). 9 tests green, clippy clean. Remaining for B1: the host residency binding (below), then the merecat palette + install slice.
- **B1 residency BINDING landed 2026-07-18** (mere `953bf09`): reading the kernel `Node` corrected the approach. `Node` is a web-page anchor (rkyv-archived; favicon/thumbnail/mime/addresses), and its own doc records the slice-C doctrine that browser-runtime state *left* the struct in 2026-07-09 for a host sidecar keyed by node id, "the graph library holds graph facts; what the host knows about a node rides beside the graph." A denizen binding is exactly that host knowledge, so it is a **sidecar mirroring `browser_node_state`**, not a `GraphBearing`/`nested` field on the kernel Node (which would force rkyv and every-constructor churn and mismodel a servitor as a web page). `session-runtime::denizen_bindings`: `DenizenBinding { subject (keyholder hex), nested_log (the node's inner chartulary graph's LogId), kind }`, keyed by node UUID in `denizen_bindings.json` beside `graph.json`, atomic write, prune-empty, forward-compatible kind. 8 tests green, clippy clean. The plan's earlier "GraphBearing + nested on the kernel Node" wording is superseded by this (the `GraphBearing` trait remains chartulary's mechanism for `Container`; mere's web Node does not implement it). Remaining for B1: the merecat palette + install slice (the user-facing half).
- **B1, rung-1 residency** (mere/merecat; depends on B0.5): denizen nodes, grant projections, palette populated from denizen nested graphs, attributed commit. Done when a local scenario pack installs from file after a visible grant review, runs from the palette, shows attributed journal entries, and its effects are compensable (auditability plus compensating actions; universal undo is explicitly not the B1 bar).
- **B2, gate the piccolo lane** (merecat): grants read from the participant node instead of feature-flag config. Done when the existing four piccolo tests pass rerouted through the gate, plus an attribution test.
- **B3, wasm rung** (mere document-host P2.3/P2.4 + merecat world): done when a sample component proposes a batch, the revision-conflict path is exercised, and an ungranted import fails at instantiation.
- **B4, pack schema freeze** (renamed from "envelope freeze": the envelope already exists as eidetic's `Engram`): spec re-read DONE 2026-07-17 against both the donor spec and the current implementation (verdict in §4). Remaining: the pack schema (typed part inventory + contribution manifest as payload vocabulary, large parts as muniment blobs by hash), personae signing bound into `TrustEnvelope`, the adapter to the runtime manifest. Done when a signed pack round-trips and a tampered pack is rejected with `Broken`.
- **B5, retinue distribution** (gated on retinue R4): publish/announce/fetch/install between two instances, tessera receipt visible via a moot's flora. Done when that works on a LAN pair.

Coordination: the moot (gemot) refactor **settled 2026-07-17** (mere `a4da519` "Complete upstream sync and Moot delegation"; tree clean but for this plan's own doc edits), so B1/B2 are unblocked. Verified the same day: mere resolves chartulary via its gitignored `.cargo/config.toml` path patch to the local working tree (now B0/B0.5), so no re-pin is needed and `mere-kernel` compiles clean against it (mere consumes `chartulary::Graph` + `stemma` over its own `Node`, never `GraphLog`/`Container`, so the breaking commit changes do not reach it). B0/B0.5 did not wait.

## Findings

- `document-host` P2.0 is green: per-instance Store, revision-checked atomic apply, typed turn-errors, capability-by-linker (unlinked imports fail instantiation), `call_async` fiber foundation. The gate generalizes this contract; it does not replace it.
- `register-mod-loader` (mere, `crates/system/registry`) is deliberately runtime-free with the `WasmModRuntime` DI trait: the seed of the eventual substrate extraction (sibling repo, per the armillary/numen precedent; extraction happens when the merecat world becomes the second consumer, not before).
- chartulary at plan founding (pre-B0 observation, since landed): capability traits `Identified`/`Addressed`/`ContentBearing`/`Labeled`/`Classified`/`Predicated`; Container/Relation payloads; stemma present; nothing for containment. `GraphBearing` extended the existing pattern rather than adding a new mechanism kind, and B0/B0.5 built containment plus the attributed commit on it.
- TERMINOLOGY already supplies: engram, flora, tessera, kith/kin, the DocumentTrustState trust ladder, orrery, gnode, link-as-statement. This plan coins as little as possible and flags what it must.
- The piccolo control lane landed in merecat 2026-07-16 (typed Actions, capability denial, step budgets, four tests): the cheapest place to prove the gate before wasm arrives.
- retinue: R0-R7 routing complete and oracle-verified; R4 resources partial (advertisement codec done; windowed transfer + RNS hash/compression derivations remain). Distribution is designed against the spec but gated on that work.
- hocket is out of scope by doctrine (V1 ships zero plugin hosting); if it ever takes participants they are control-scope, never audio devices.
- **Moot-agent review (2026-07-17, relayed by Mark), five corrections accepted, citations verified**: (1) `GraphLog` mutators append single edits with neither a revision-checked batch API nor an attributed envelope (spine.rs), so "atomic apply" needed the B0.5 transaction boundary, and app effects (unrollbackable) move strictly post-commit; merecat's `src/script.rs` already proves the emit-typed-Actions half. (2) Grants in a self-editable nested graph would be self-escalation: authority materializes from signed evidence, the graph holds projections. (3) `GraphEdit` retains no inverses (edit.rs), so B1 promises auditability + compensating actions, not universal undo. (4) B0's archive-then-remove has a crash window (nested.rs), and attachment permits self-reference/cycles: recovery invariant + attach guards recorded, owned by B0.5. (5) Pack `Trusted` = signature verifies only; install mints a local grant after visible review, widening upgrades re-review.
- Capability-vocabulary confirmation (Mark, 2026-07-17): the once-suspected split inside `ContentBearing` (externally addressed content vs content stored on the node, like notes) is dissolved by construction. A node's stored body is content-addressed (a muniment blob hash) and external references are scheme-addressed (`Addressed`), so all content is addressed and one body capability suffices; no `StoredContent`/`AddressedContent` fork is wanted.

## Open questions

1. RESOLVED by B0 (2026-07-17): there is no `GraphId` newtype; a nested graph's identity IS its `codicil::LogId` (the graph is the replay of its log, so the log's identity is the graph's, and fork/provenance ride along for free). The registry is the slot convention itself: `keys("nested/")` enumerates live nested graphs, `keys("archive/nested/")` the archived; no registry struct exists until a consumer proves the need.
2. RESOLVED 2026-07-17 (corrected same day against the current implementation, on Mark's catch): a pack is an **eidetic `Engram` whose `SchemaRef` names the pack schema**; the envelope already exists in code with content-hash identity and orthogonal privacy/provenance/trust axes, so B4 defines a schema (part inventory + contribution manifest) and the personae signing binding, never an envelope. The donor TransferProfile v1's multi-part inventory imports as pack-schema vocabulary; its model-adaptation vocabulary stays out.
3. Revocation design (own round before B5; start from the kith capability sharing plan's revocation/expiry and offline-proof machinery rather than a fresh scheme).
4. RESOLVED (Mark, 2026-07-17): **no cap.** Nesting depth is unbounded by design; recursion is a feature of the substrate, not a hazard to fence there. Any practical guard (cycle detection when *following* bearings, render depth limits) belongs to consumers of the graph, never to chartulary. Note for followers: a bearing cycle (graph A's node bears B, B's node bears A) is representable, so traversal code must track visited `LogId`s.
5. RESOLVED in full (Mark, 2026-07-17): umbrella word = **denizen**, proposal = **petition**, **pack** kept for shallow rungs with **mod** for rung 3+, swatch wording landed in TERMINOLOGY (with the correction that the gloss is a pane containing a swatch), and `GraphBearing` blessed as final. Candidate pools and availability are logged in the naming-round notes. RESOLVED for the helper unit: **servitor** (Mark, 2026-07-17; reserved on crates.io as 0.0.1). Runner-up banked: animula. Rest of the checked pool: free = paredros, ushabti/shabti, fylgja, wolpertinger; taken (all by agent/daemon frameworks) = famulus, duende, karakuri, holon, familiar.
6. Moot-side alignment: when the moot refactor settles, confirm the peer-apply path can adopt the gate's validation without a second implementation.
7. LARGELY RESOLVED by B0.5 (2026-07-17): existing bare-edit logs **migrate on load** (one single-edit batch per entry, synthetic author `pre-gate`, one-way: the next save writes batch format). Residue: a migrated log's **fork provenance is dropped** (codicil exposes no parts-constructor, and adding a `map` API there chains pushes across repos); pre-gate logs that are also forks are rare, but a lossless path via a codicil `Codicil::map` is available if wanted.
8. **RESOLVED against the existing design (Mark pointed to prior docs, 2026-07-17): the denizen grant is NOT a new type; it rides the already-designed three-layer capability stack.** My earlier "extract a kind-agnostic core from `DeviceGrantPayload` / adopt the Meadowcap shape into personae" was reinventing work already done. The canonical design is the **event-DAG substrate brief §8.8** (resolved 2026-06-03) plus the **graph-cluster-namespaces brief** (2026-05-10):
   - **Layer 1, structural caps** = meadowcap-shaped, mere-native, over `(subspace_set, path_prefix, time_interval, mode)` with recursive delegation. The novel twist is **graph-cluster-derived namespaces**: the path prefix is the graph's own Leiden community structure, not admin paths, and **caps bind to leaf node IDs while cluster-paths are routing hints** (§4a). The Willow mapping already carries the owned/communal axis I was rediscovering: `NamespaceId = Personal(owned) | Moot(communal) | SharedSession | Coalition`.
   - **Layer 2, policy** = tessera facts (built) + preset authorizer (`OpenWithFloor`/`VouchedOrScore`/`MembersOnly`), Biscuit Datalog the deferred backend.
   - **Layer 3, group/key** = p2panda-encryption (fit proven).
   - **The seam already exists in code**: `gemot::MootAuthorizationProvider` takes a `MootAuthorizationRequest { subject: [u8;32], capability_path: String (opaque, Meadowcap or other provider gives it meaning), at_ms }` and returns `MootAuthorizationInputs { capability_covers: bool, facts: TesseraFacts }`. This is subject-agnostic (subject is any keyholder) and provider-agnostic (path opaque) — it *is* the kind-agnostic core I was going to build, already abstracted.
   - **`DeviceGrantPayload` (wallet_grant) is the orthogonal device tier** (which personas a device may act for), not the graph-area cap; it does not umbrella denizens and should not be generalized to.
   - **Reality of the code**: the structural-cap layer is designed and the moot provider seam exists, but the mere-native meadowcap cap type is **not built** (no `willow-cluster-cap` probe despite the doc note; no `MerePath`/`McCapability` types; `mere-namespace` is prose). So B1's authority materialization uses the **provider seam** with a minimal `capability_covers` now (a denizen holds a scoped structural cap over its granted graph area, checked as prefix/leaf-id coverage), and the full graph-cluster meadowcap layer lands when `mere-namespace` is built. **Denizen residency's grant projection = a readable render of the denizen's structural cap into its nested graph; the gate's authority = the provider's `capability_covers` for the denizen subject.** No new grant model, no wallet_grant edit.

## Progress

- **2026-07-23 (B3 COMPLETE — the wasm lane runs end to end)**: the ruling
  above is now plumbed. **mere `app-host`** (new crate, `crates/script/app-host`,
  sibling of document-host over the same WIT package): bindgens `app-core`,
  backs `emit` with a host-supplied `ActionSink`, drives
  activate/on_event/deactivate (async + a blocking face for sync hosts), and
  contains a bad component the way document-host does (epoch interruption +
  `Watchdog` + `StoreLimits`). It is app-AGNOSTIC by construction — it has no
  action vocabulary, so the ring policy cannot drift into two places. A guest
  fixture crate builds via build.rs to `wasm32-wasip2`; 4 integration tests
  drive a REAL component (accepted emissions queue, an ungranted ring is
  denied and never queues, gate management is refused even under a total
  grant, misfires are typed and loud).
  **merecat**: `component.rs` implements `ActionSink` as the ring gate
  (decode → classify → `emit_allowed` → queue) behind a new `wasm` feature
  (opt-in like piccolo); `RunDenizen` branches on what a resident IS (a
  script's source facet vs a component's file pointer) while what it may DO
  stays the grant's business; both lanes share `lower_denizen_actions` (one
  attributed lowering path). A dropped `.wasm` stages like a `.lua`:
  `PackBody::{Scenario,Component}` (blake3 over the bytes either way, so
  identity does not care which lane runs it), the component's bytes are
  stored beside the worlds at `denizens/<subject>.wasm` with the facet as the
  pointer, and **install grants one path per REVIEWED ring** — the blanket
  `app/` grant is gone, so an unnamed ring is an ungranted ring.
  **Ring preselection**: `default_rings()` = navigate + panes + dispatch;
  the session ring (fork/close/delete/recover) is destructive and never
  preselected; host-only is not a profile choice at all. The review row names
  them and is length-checked to fit one palette row (the first headed run
  clipped the ask — a review you cannot read is not a review).
  Receipts: merecat 117 green (both features), app-host 4; headed
  `denizen_wasm.scn` RESULT ok — the review capture reads
  `Install app_core_guest (wasm) — grants: navigate, panes, dispatch, own
  world — Confirm`, and the run log shows `caps.granted()` reporting exactly
  the three rings, `open-address`/`fit-view` accepted, `close-session`
  refused (`session: not covered`), `confirm-install-denizen` refused
  (`host-only: no grantable path exists`), with the accepted emission's node
  minted and attributed to the subject.
- **2026-07-23 (the merecat world RULED: total surface, ring-gated envelope —
  Mark)**: the open "which Actions may a component emit" question dissolves.
  A component may potentially emit ALL Actions; what decides is the action's
  **ring** — a capability-path family checked against the denizen's grant at
  emission, the same place piccolo denies. Curating the wit surface would
  store authority twice and version the ABI per grant; a total surface is one
  stable ABI, packs compile once, grants vary per install. Landed: the wit
  `actions` envelope interface + `app-core` world (mere `wit/world.wit`;
  `{name, payload}` record, `denied`/`unknown`/`malformed` errors,
  unknown-forward) and merecat's `ring` module — `Ring`
  {navigate `app/navigate`, panes `app/panes`, dispatch `app/dispatch`
  (incl. the omnibar: driving the command surface IS dispatch), session
  `app/session` (new path), **host-only** (NO grantable path: gate
  management — install/confirm/cancel/run — is structurally unemittable
  under ANY authority, the self-escalation floor)}; `ring_of` is an
  exhaustive match with no catch-all (a new Action variant fails to compile
  until classified); `emit_allowed` is the single deny point (denials name
  the ring); `decode_envelope` grows incrementally (decode is NOT authority
  — an undecodable name is a loud unknown). Default profiles per kind/pack
  shape the install review's PRESELECTION only; the visible review stays the
  sole place an ask becomes a grant. Tests: host-only resists a total
  `app/` grant; coverage passes/denies by ring; envelopes decode with loud
  misfires (merecat 106 green). REMAINING leg: the wasmtime plumbing —
  instantiate `app-core` in merecat, back `emit` with
  decode → `emit_allowed` → attributed lowering, an app-core guest fixture,
  and `app/session`/ring preselection in the review UI.
- **2026-07-23 (node-tier archive-never-orphan COMPLETE)**: the
  `DeleteFocusedNode` trigger existed after all (the recycle-bin lane).
  The tombstone (`eidetic DeletedNode`, serde-defaulted so old tombstones
  load) now carries the node's borne world id + its whole facet bundle;
  delete moves the world file to the archive slot
  (`denizens/archive/<log>.json`, the file-level echo of chartulary's
  `archive/nested` convention) BEFORE the node leaves — a failed archive
  aborts the delete — and the live facets go to the tombstone. Recovery
  restores full residency: facets back whole, world file back live, pointer
  re-borne through the spine, resident rebuilt (same revision). The forget
  paths complete it: emptying the bin and athanor's retirement pass purge
  each tombstone's archived world. Receipts: the delete/recover round-trip
  test asserts every slot state; merecat 101→106 green, eidetic 88,
  athanor 9.
- **2026-07-22 (fork CARRIES worlds)**: the follow-on to the containment
  ruling, at file granularity. `Graph::bear_nested` (kernel, public) sets
  `nested` directly WITHOUT the delta spine — for copy/load paths building a
  graph with no journal yet. merecat's `fork_session_from` re-bears each
  carried world on the fork's copy and copies the world file into the fork's
  own `denizens/` — donor and fork evolve independent worlds thereafter, and
  the fork's denizen rebuilds as a full resident with no legacy heal.
  Receipts: `fork_carries_denizen_worlds_as_real_copies` (merecat 100 green),
  headed `denizen_fork.scn` RESULT ok with both sessions verified on disk
  bearing the same world identity in separate files. Archive-never-orphan at
  the SESSION tier already holds by construction (trash moves the whole
  session dir, worlds ride along); the NODE tier stays gated on a node-delete
  affordance merecat does not have yet, and the deeper chartulary
  slot-convention storage (worlds inside the parent's muniment store,
  `nested/<log-id>/{log,snap}`) waits for sessions to move off JSON files.
- **2026-07-22 (containment RULED structural — `Node.nested` + `GraphBearing`
  on the kernel Node)**: Mark's read held: graph-bearing containment is
  STRUCTURE, so the world pointer moved off the `denizen.binding` facet onto
  the kernel `Node` itself — `nested: Option<LogId>` (rkyv `LogIdAsString`
  adapter, serde-defaulted `PersistedNode` twin so old snapshots load
  unchanged), `impl chartulary::GraphBearing for Node`, and a
  `SetNodeNested` delta through the apply/capture spine so residency
  journals attributed like every other graph edit. The 2026-07-18 rejection
  (rkyv/constructor churn; "mismodels a servitor as a web page") is
  REVERSED: the one-node ruling landed the same day and dissolved the
  modeling objection (denizen-ness = facet bundle = agency; containment =
  structure; orthogonal), and the churn was three constructors. What the
  facet keeps is pure agency: `{subject, kind}`; a legacy `nested_log`
  reads for a one-time adopt heal (`Denizens::legacy_heals`) and is never
  written again. A cross-graph copy deliberately does NOT carry `nested`
  (two nodes must not bear ONE world file); the fork-carries-worlds move
  is the slot-convention follow-on. This closes the two pointer-bridge
  gaps: archive-never-orphan (the world rides the node through archive)
  and fork-shares-world. Receipts: kernel 277 / session-runtime 217 /
  merecat 99 green; headed `denizen_b1.scn` RESULT ok on the new shape,
  with `graph.json` carrying `nested` on the denizen node and the binding
  facet persisting as `{subject, kind}` only.
- **2026-07-22 (B1 COMPLETE — mere `e54ca8cf`, merecat `8b3ad31`)**: the
  user-facing half landed and the done-condition is met with receipts. mere's
  `GraphJournal` adopted the attribution envelope (`AttributedDelta { author,
  delta }`; `user` / subject-hex / `pre-gate`; replay strips the envelope).
  merecat: a dropped `.lua` stages an install with a **content-derived
  subject** (blake3 of source); nothing mints until the palette's VISIBLE
  review confirms (the ask is the first, highlighted row — dynamic rows now
  lead the actions lane); confirm mints node + `denizen.binding` +
  `scenario.source` facets + a nested chartulary world with the servitor
  Gate's read-only grant projections, persisted per denizen; **authority
  derives from the projections on adopt** (never stored twice); Run evaluates
  via piccolo and lowers Actions through the spine with the journal scoped to
  the subject, read back in the Inspector's new Journal section. Receipt
  `denizen_b1.scn` green (review + attribution captures); the resident-gate
  round trip (in-scope commits attributed / out-of-scope refused) headless.
- **2026-07-22 (B2 COMPLETE — merecat `9727081`)**: the piccolo lane's
  `ScriptCapabilities` derive from the denizen's structural caps (each class a
  path under `app/`: read/dispatch/navigate/panes), install grants + projects
  `app/` beside the world scope, and the denial surfaces by capability name.
  The six existing lane tests pass unchanged; derivation + denial +
  attribution tests on top (104 with piccolo).
- **2026-07-22 (B3 bars VERIFIED on today's tree)**: document-host's suite
  green end-to-end (21 tests) — sample component proposes batches, the
  revision-conflict path exercised (`eight_turns` outcome 5), an ungranted
  import fails at instantiation (`grants.rs`), quotas/guarded intact. These
  bars were built at P2.3/P2.4 (2026-06-22). **Same day, the one-grant seam
  landed** (mere, `Grant::from_authority`): document-host's import grant now
  derives from a servitor authority (`doc/log|document|net` paths, linked only
  under coverage), so one authority decides both the piccolo face (B2) and
  the wasm face — the real component instantiates for a covered subject and
  fails at instantiation for an uncovered one. B3's remaining substance is
  the **merecat world** — an Action-emitting wit world so a component can BE
  a merecat denizen end-to-end; which Actions a component may emit is a
  surface decision for a design round with Mark.
- **2026-07-22 (B4 COMPLETE — mere `a3a246a8`)**: `mere.pack/v1` frozen in
  eidetic (`pack.rs`): `PackManifest` typed payload (part inventory by content
  hash + contribution manifest with author pubkey + requested scopes), the
  personae signing binding in `TrustEnvelope.signatures`
  (`personae:ed25519:<pub>:<sig>` over canonical bytes — the SignatureRef
  "lands with identity" note, landed), and `verify_pack`:
  Trusted/Unsigned/**Broken** (tampered fields, swapped part hashes, widened
  asks, forged authors all Broken; foreign schemes not ours to judge).
  Signed round trip through the typed store re-verifies Trusted.
- **2026-07-22 (B5 pair + curation PROVEN — mere `5a33157f`; the R4 wall
  OPENED)**: retinue's windowed resource transfer landed upstream (real-RF
  milestones), so the `pack-distribution` probe ferries a signed pack
  publisher → subscriber over a real in-process retinue link (advertise →
  request/HMU → serve → recover), re-verifies **Trusted** on arrival, and
  proves a mid-flight scope-widening tamper reads **Broken** and is refused.
  Curation: a `MootEvent::Shared` under `mere.pack/v1` lists in the moot's
  fauna (gemot roster test) — the flora is pack discovery, no new wire.
  **2026-07-22, the physical pair LANDED — over RF, stronger than the LAN
  bar**: `rf_pack_pair` (pack-distribution probe bin) ran the signed pack
  between the two bench RNodes (COM6 → COM5, fw 1.86, the tulle_headed LoRa
  params): destination announced over the air, 591 pack bytes transferred
  bit-equal, signature re-verified **Trusted** on the subscriber, widened-ask
  tamper reads **Broken**. `RESULT ok`; log at
  `testing/mere/rf_pack_pair/run2.log`. Field finding: the resource path
  stalls to timeout without `set_reliable_max_window(1)` on both endpoints —
  half-duplex pacing is load-bearing, exactly as the tulle_headed acceptance
  set it. Remaining B5 residue: the tessera receipt on a live moot over the
  pair (the fauna half is proven in gemot; the live-pair join/share ride is a
  bench session with the mesh stack up).

- **2026-07-17 (B4/B5 grounding)**: read eidetic's schema machinery and gemot's moot wire. Two findings fold into §4/§5: the pack schema is *authored* as a `SchemaDefinition` (`schema_id = "mere.pack/v1"`) plus a `TypedPayload` struct, not a new envelope or schema system; and the moot hand-off already exists as `MootEvent::Shared { manifest_id, schema_id, title }`, so B5's curation half is mostly wiring that event to the pack schema, with only blob transfer gated on retinue R4. B4 and B5 both shrank materially.
- **2026-07-17 (push + B4 pre-work, corrected)**: chartulary pushed (`2ced0fb` on GitHub; the remote already carried `3361f0e`), so mere can re-pin whenever B1 starts. Donor engram spec (TransferProfile v1) re-read in full from the GitHub archive and an initial donor-only verdict recorded; **Mark caught that the current implementation had not been checked**, and the verdict was corrected against `eidetic-core` (486 engram references across 58 files in mere, incl. gemot records, meerkat export, session-runtime athanor): the current `Engram` envelope already exists with content-hash identity and schema-by-reference, so a pack is an `Engram` under a **pack schema**, B4 defines a schema not an envelope, and the donor's multi-part inventory imports as payload vocabulary. §4 and open question 2 rewritten accordingly.
- **2026-07-17 (review round 2, moot agent)**: "the plan survives review" with four tightenings, all applied: journal ownership clarified in §1 (chartulary owns envelope semantics + the `GraphEdit` journal; mere's `GraphJournal` adopts the envelope over `CapturedDelta` at B1; vocabularies stay separate; envelope promotion into codicil recommended, decided at B1), the existing wasm grant bridge explicitly scoped in §4 (import-level enforcement, document lane, zero app-graph authority until B3, becomes the import-level face of the one grant), the pack-manifest to runtime-manifest adapter defined in §4 (ModManifest is derived state from pack manifest + granted subset, one direction, never authority), and the stale pre-B0 lines in §2/Findings updated.
- **2026-07-20 (B1 binding SUPERSEDED by the facet convergence)**: `session-runtime::denizen_bindings` (the `953bf09` sidecar) was removed before any host wrote one; the binding is now the **`denizen.binding` facet** (`session_runtime::denizen_facets`, mere `10084b3`): `{subject, nested_log, kind}` as one coherent record in `facets.json`, per the one-node ruling that denizen-ness is a facet bundle, not a node class. Same fields, same host-knowledge doctrine, one store.
- **2026-07-18 (B1 residency binding, historical)**: added `session-runtime::denizen_bindings` (mere `953bf09`), the host sidecar binding a node to its denizen (subject + nested-graph LogId + kind), mirroring `browser_node_state` per the slice-C doctrine. Reading the kernel `Node` corrected the plan: no `GraphBearing`/`nested` on the web Node (rkyv + constructor churn + mismodeling); the binding is host knowledge in a sidecar. 8 tests, clippy clean, 188 existing session-runtime tests unaffected. Remaining for B1: the merecat palette + install slice.
- **2026-07-18 (B1 residency core)**: after reading the prior capability design (Mark's steer), built the denizen residency core in the reserved `servitor` crate (`1af0c91`): `Subject`/`Grant`/`AuthorityProvider`/`Gate` over a chartulary nested graph, 9 tests, clippy clean. Headless, zero mere-tree or wallet_grant impact. The grant model rides the existing three-layer stack through an `AuthorityProvider` seam mirroring `gemot::MootAuthorizationProvider`; `PrefixAuthority` stands in until the meadowcap structural-cap layer lands. Servitor moves from name-reservation placeholder to real first content.
- **2026-07-17 (B0.5 substrate)**: the live gate seam landed in chartulary (`2ced0fb`, committed, push pending with `3361f0e`). Attributed `Batch` journal, `commit_batch` with full precheck + stale refusal carrying current revision, attributed convenience mutators (UI path), pre-gate log migration on load, self-bearing rejection, archived-pending-removal recovery. 33 tests green, clippy clean on new files. Authority materialization deliberately left gate-side, lands with B1. OQ7 largely resolved (migrate-on-load; fork-provenance residue documented). Next release is 0.2.0 (entry type + Container field are breaking).
- **2026-07-17 (nomenclature + review)**: Mark ruled the remaining names: **denizen** (umbrella, replacing working *participant*), **petition** (replacing *proposal*), **pack** kept plain for rungs 1-2 with **mod** for rung 3+, swatch wording finalized with the gloss correction (a pane containing a swatch). All entries landed in TERMINOLOGY. Same session, the moot-agent review's five corrections were verified against the cited code and folded in: **B0.5 (the live gate seam) inserted as the keystone before B1** (attributed revision-checked batch commit + post-commit effect queue + authority materialization with grant projections + B0 lifecycle follow-ups), undo language softened to auditability + compensating actions, install review separated from pack trust, uninstall retention rule recorded. Open question 7 (journal-envelope migration) opened.
- **2026-07-17 (post-B0)**: `GraphBearing` blessed as final. chartulary spike committed (`3361f0e`; push pending, and mere's git dep only sees it after the push). Back pointer added to merecat's architecture plan progress log. ContentBearing all-content-is-addressed confirmation recorded in Findings.
- **2026-07-17 (depth ruling)**: Mark ruled **no cap** on nesting depth (open question 4 resolved): unbounded recursion is substrate-legal; guards live consumer-side, and bearing-cycle traversal must track visited `LogId`s.
- **2026-07-17 (B0)**: nesting spike landed in chartulary (uncommitted, Mark's tree). `GraphBearing` + `Container.nested` + `nested` module (slot convention, atomic archive, `remove_bearing_node`). 25 tests green. Decisions: no `GraphId` newtype (identity = `LogId`); the slot convention is the registry; archive is a raw-backend atomic move, so nothing is ever destroyed or orphaned. Nesting depth (open question 4) note: recursion is now possible (a nested graph's own nodes can bear graphs); the sanity cap remains undecided and unenforced.
- **2026-07-17 (naming)**: Mark chose **servitor** for the resident helper; reserved on crates.io (`servitor` 0.0.1, MIT/Apache, published from the new `repos/servitor` placeholder). Animula banked as runner-up. Vocabulary section and open question 5 updated.
- **2026-07-17 (later)**: §6 added: agents as participants via Mark's MAG framing (feeding = grants + scoped memory, evolution = earned trust, photon blast = prompt tier, raised helpers as packs). Naming round logged in open question 5.
- **2026-07-17**: Plan founded after a three-round design conversation (extension host shape, substrate placement, free-imagining round). Vocabulary rulings recorded (nested graph / graphlet / swatch). Grounding verified against document-host, chartulary caps, TERMINOLOGY, retinue README, and the merecat architecture plan. No code changed. Next concrete step: B0 spike in chartulary.
