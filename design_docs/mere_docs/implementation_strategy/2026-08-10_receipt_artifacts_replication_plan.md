# Receipt Artifacts Replication Plan

**Date**: 2026-08-10
**Status**: Planned, nothing executed. Substrate inventory code-verified.
**Scope**: receipts — scenario receipts, frame captures, screenshots, and
their provenance manifests — as content-addressed artifacts in the personal
graph, replicated across the owner's own devices by the stack itself.

**The ruling this encodes (Mark, 2026-08-10):** when a capability is one the
stack intends to own, dogfooding the in-stack implementation beats adopting a
mature external tool, even where the external tool is good. Syncthing was
considered for exactly this and declined on those grounds: adopting it would
deter the dogfooding that makes the owned capability real. Mature tools stay
technique donors.

**Related**:

- `genet/scripts/remote-receipt.ps1` — produces the artifacts this plan
  replicates: receipt, captures, and a `manifest.json` carrying remote OS,
  session type, commit + dirtiness, environment, exit code, and a SHA-256 per
  artifact.
- `Code/testing/<repo>/` — the working tree these land in today, per-machine
  and unreplicated, which is the gap.
- graphshell's resident host (`ports/graphshell/src/native/`) — pairing,
  owner settings, `PersonalSyncHost`, transfer staging. The replication rail.
- `crates/mesh` — M2: signed job ops, deterministic board, namespaces,
  leases, device policy, and shipped adapters including `esp.embed.lexical/v1`.
- muniment (`BlobStore`, blake3 content addressing), codicil, chartulary
  (container + facets), personae (roster, pairing identity).
- The 2026-06-15 contact brief §6: file transfer is content- or
  capability-addressed; persona and contact enter only for authorization and
  verification. Same posture here, one tier earlier (own devices).

---

## 1. What a receipt is, in stack terms

A capture without provenance is a JPEG nobody can place in six months. The
remote-receipt lane already knows this: its manifest binds every artifact to
a commit, a machine, a session type, and a scenario. This plan makes that
binding a graph fact instead of a JSON file in an unversioned directory.

- **A receipt is a container** (chartulary), one per run, whose facets carry
  the manifest fields: repo, package, scenario, target machine, platform,
  session type, commit, dirtiness, exit code, ran-at.
- **Artifacts are content-addressed blobs** (muniment `BlobStore`, blake3),
  referenced from the receipt container by hash. Identical captures across
  machines dedupe to one blob with no design work — that is what content
  addressing is for.
- **The receipt joins the personal graph**, so replication is not a new
  system: it is the personal-graph sync the resident host already runs, plus
  blob transfer the staging path already handles.
- **Provenance is append-only** (codicil): a receipt is never edited, only
  superseded by a later run's receipt.

Deliberately *not* a moot, not shared, not federated: this is tier-1, the
own-devices ring, the simplest case the substrate has. The mesh doc says it
plainly — every peer holding the id is trusted; sharing is scheduling and
permissions, never verification markets.

## 2. What already exists (code-verified 2026-08-10)

More than expected, and the plan is mostly wiring because of it:

- **Pairing and the resident host are live.** `device_sync.rs` brings up a
  `PersonalSyncHost` from owner settings, polls for newly paired devices,
  and names graphs by blake3 domain hash. `transfer_staging` has
  `receive_transfer` / `released_blobs_for`. personae supplies the identity
  the pairing writes.
- **mesh M2 shipped** with the deterministic board, job namespaces, lease
  terms, device policy with owner reclaim, and the `esp.embed.lexical/v1`
  adapter — so the esp consolidation plan's "first MeshResource" step is
  already real, and the compute lane and this replication lane share rails
  without sharing a design.
- **The manifest is halfway to the schema.** `remote-receipt.ps1` already
  emits per-artifact SHA-256 and the provenance fields; the facet mapping in
  §1 is a transliteration, not an invention.

## 3. Phases

- **R0 — the schema and the ingest.** A receipt-ingest lane: take a receipt
  directory (local `testing/<repo>/<host>-<stamp>/` or fresh from the
  remote-receipt script), write blobs into the store by content, author the
  receipt container with manifest facets and blob refs. Headless, tested
  against a fixture directory. Home: a module in the graphshell resident
  host's lane first, per module-before-crate; it touches the store and the
  personal graph, both of which the resident host already holds open.
  **Done when** ingesting the same directory twice is a no-op (content
  addressing proves itself) and a receipt's facets round-trip.
- **R1 — replication.** Nothing new to build if §2 is right: the receipt
  container replicates with the personal graph, blobs ride transfer staging.
  **Done when** a receipt ingested on this machine is browsable on a second
  paired device with every blob byte-identical (hash-verified on arrival),
  and deleting the local copy does not orphan the replica.
- **R2 — the lane feeds itself.** `remote-receipt.ps1` gains an ingest step:
  a receipt fetched from the ThinkPad or the iMac lands in the graph in the
  same motion that fetched it. The screenshots-unification wish becomes a
  side effect: every capture from every machine, one graph, deduplicated.
  **Done when** a remote run on one machine is visible on all paired
  machines without a manual step beyond the run itself.
- **R3 — seeing them.** A receipts view in turnstone (a gloss lens or a
  swatch listing receipts by repo/scenario/machine, opening captures from
  blob refs). Deferred until R2 has produced enough receipts that browsing
  them is a real need rather than a demo. **Done when** the 2027 question
  "what did the frame look like on Mint X11" is answered by clicking, with
  provenance visible beside the pixels.

## 4. Boundaries

- `testing/` stays the working directory and stays unversioned; ingest is
  the promotion step. Nothing writes back from the graph into `testing/`.
- No general file-sync ambition. This replicates *receipts*: artifacts with
  provenance. A generic synced folder is a different product with different
  trust questions, and naming it out of scope is what keeps this small.
- No new crate until a second consumer exists (isometry's captures and
  cleromancy's headed receipts are obvious candidates; they arrive at R2's
  price, not before).
- The compute mesh is a sibling, not a dependency: replication uses the
  sync rails directly and posts no jobs.

## 5. Progress

- **2026-08-10**: planned. Substrate inventory verified against
  `device_sync.rs`, `mesh/src/lib.rs`, and the transfer staging surface;
  the dogfood-over-adopt ruling recorded here and in workspace memory.
