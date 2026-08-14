# Receipt Artifacts Replication Plan

**Date**: 2026-08-10
**Status**: R0, R1 and R2 built 2026-08-10. R3 (a turnstone receipts lens)
deferred as planned. See §5.
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

- **2026-08-10, later: R0-R2 built.** `ports/graphshell/src/receipts.rs`
  (the ingest) plus `src/bin/receipt_ingest.rs` (the CLI). Nine tests green.

  **R0 — the schema and ingest.** A receipt directory becomes a node id, an
  address, and a list of `PersonalGraphEvent`s. Three decisions worth keeping:

  - **The node id is a v5 UUID over the receipt's address**, and the address
    is built from the run's own facts (repo, host, scenario, timestamp) rather
    than the directory name — so the same run ingested from a copied or
    renamed directory is still the same receipt, and re-ingest reaches the
    same node instead of minting a second.
  - **Ingest reads no clock.** Every timestamp comes from the manifest,
    because the facts are about when the *run* happened. That is also what
    makes the events byte-identical across re-ingests, which is what the
    idempotency test actually asserts.
  - **Ingest verifies rather than trusts.** The manifest's per-artifact
    SHA-256 was computed on the producing machine; the arriving bytes are
    re-hashed and a mismatch is a hard error, so a receipt corrupted in
    transit cannot enter the graph looking sound. Blobs then store under
    blake3, and the dedup test proves two machines capturing identical pixels
    yield one blob.

  **R1 — replication, which was NOT free after all.** The plan said "nothing
  new to build if the inventory is right"; checking `SyncSelection::projects`
  found otherwise. `SetFacet` projects only for facets the selection lists, so
  receipts would have replicated as bare titled nodes carrying none of their
  provenance — the exact failure this lane exists to prevent, and silent.
  Fixed by naming the lane once in `receipts::sync_facets()` /
  `sync_address_rule()` and merging it in `device_sync.rs`. Receipt addresses
  are synthetic, so they follow their facet the way the transfer carrier does:
  a device that declines the lane materializes no receipt nodes at all rather
  than a list of empty titles. A test pins the regression (any emitted facet
  missing from the lane fails the build). `blob_availability` is deliberately
  left as the owner set it: with the lane off a receipt still replicates whole
  — the artifacts facet carries every blake3 hash — and only *which device
  holds the bytes* goes unsaid.

  **R2 — the lane feeds itself.** `receipt_ingest` verified end to end on a
  real directory: same node, same blob, byte-identical events across three
  runs. `genet/scripts/remote-receipt.ps1` gained `-IngestBin` /
  `-IngestStore` / `-IngestDevice`, so a run on the ThinkPad becomes a graph
  fact in the motion that fetched it. Genet takes a path rather than learning
  where mere lives. Ingest failure is warned, not fatal: a fetched receipt is
  worth keeping even when the graph is unavailable.

  **One boundary held deliberately.** The CLI writes the authored events to
  `graph-events.json` beside the receipt rather than pushing them into a
  running replica. The resident host owns the authoring turn (it holds the
  signing identity and the log); a CLI writing operations behind its back
  would be a second writer for one graph. Wiring the host to pick these up is
  the small remaining step before R3.

  **Note for the next session:** `native::personae_host::tests::
  isolated_named_pipe_lists_and_signs_through_the_ssh_wire_protocol` fails
  (2 identities, expected 1). Verified pre-existing by stashing only this
  work and re-running at HEAD — it fails identically without it. Unrelated to
  receipts; likely the concurrent wallet-grant work or the live personae
  agent on this machine.
