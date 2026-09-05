# Receipt Artifacts Replication Plan

**Date**: 2026-08-10
**Status**: R0-R3 built 2026-08-10. R3 landed in the projection rather than
turnstone, for the reason in §5; one gap named there. See §5.
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

- **2026-08-10, later: R0-R2 built.**
  [`receipts/ingest.rs`](../../../ports/graphshell/src/receipts/ingest.rs)
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
  would be a second writer for one graph.

- **2026-08-10, the hand-off closed.** The resident host now picks the events
  up, so the boundary above is a seam rather than a gap.

  `receipts` became a module directory under the 600-line ceiling, split by
  concern: `manifest` (the producing machine's *claim* — needs no store, no
  async, no graph vocabulary), `ingest` (turning it into facts), `intake`
  (the host side), and `mod` holding the sync lane. `receipt_ingest` gained
  `--inbox` / `--data-root`; `device_sync` gained `spawn_receipt_intake`,
  polling `<data_root>/receipts/inbox` every 10s beside the existing pairing,
  card-refresh, and accept watches.

  Three decisions in the intake worth keeping:

  - **One turn per receipt.** A run is one fact; batching two runs into a turn
    would leave a later reader unable to tell which events belonged to which.
  - **Clear the file only after the turn succeeds.** Cleared before, a failed
    author loses the receipt entirely; cleared after a failure, it simply
    retries next poll. Applied files move to `inbox/applied/` rather than
    being deleted — that directory is the local record of what this device
    authored, and it is what a person reads when asking why a receipt did or
    did not arrive.
  - **A malformed hand-off is skipped, not fatal.** One bad file must not
    wedge the intake loop for every later receipt; it stays put to be looked
    at.

  The events file is still written beside the receipt as well, because it is
  what an owner reads to see what is about to be authored on their behalf.
  Verified end to end: ingest deposits into the inbox and reports it; six
  intake tests cover the absent inbox, round-trip, re-deposit, apply, the
  malformed file, and the `applied/` directory not being scanned as a
  receipt. 15 receipt tests, 196 graphshell lib tests.

  **R3 is now the only step left**, and it is a reading surface rather than
  plumbing.

  **Note for the next session:** `native::personae_host::tests::
  isolated_named_pipe_lists_and_signs_through_the_ssh_wire_protocol` fails
  (2 identities, expected 1). Verified pre-existing by stashing only this
  work and re-running at HEAD — it fails identically without it. Unrelated to
  receipts; likely the concurrent wallet-grant work or the live personae
  agent on this machine.

- **2026-08-10, R3: the reading surface.** `receipts::card` plus a branch in
  `supplemental_cards`. 22 receipt tests, 204 graphshell lib tests.

  **Which half was built, and a correction (Mark, same day).** R3 was written
  as "a receipts view in turnstone", and a first pass here claimed turnstone
  was an endpoint *provider* and not a consumer. That was wrong, and the
  correct picture matters because it makes the destination closer rather than
  further. Turnstone is **both**: `remote_projection` publishes its own graph
  as an endpoint, *and* it carries working `graphshell-client` machinery —
  `ClientState::apply_snapshot`, presentation resolution, and the full
  `NeedsResource` to `resource` to `apply_resource` fetch loop in
  `resolve_all`. That client is exercised loopback in the G3 canary, against
  turnstone's own endpoint. What turnstone lacks is an **outbound session to
  another mere**, not the ability to consume one; browsing other meres is its
  design intent (the 2026-07-22 remote-lens ruling names a "remote turnstone
  lens" explicitly), and it keeps its own graph regardless.

  So this phase built the **publishing half**, and it belongs where it is on
  its own merits: the card is produced by whoever holds the receipts. A
  turnstone receipts lens is then "open a session to the resident host and
  render its supplemental cards" — no new dependency edge, no `personal-sync`
  in the app, and no client code that does not already exist and pass its
  canary.

  Without this a receipt replicated as a generic "Synced graph node" card
  showing a title and an address, dropping every fact in its facets. That is
  the failure the whole lane exists to prevent, so the card is the point of
  the exercise rather than decoration. It reports repo, scenario, machine,
  system, session type, commit, when, and result, and carries each capture's
  blake3 in `PortableCardV1::media`, the field the protocol already resolves
  through `ResourceRequest`.

  Two deliberate touches: **a dirty checkout is said twice**, in the commit
  line and as a badge, because it is the single fact that decides whether the
  pixels are evidence about a commit and a reader skimming badges must not
  miss it. And **missing or malformed artifacts degrade rather than vanish** —
  a bad hash is dropped rather than guessed, and the receipt still reads as a
  receipt.

  **The remaining gap is purely server-side**, which the correction above
  sharpens. The client half is done and proven: turnstone's `resolve_all`
  already turns a `NeedsResource` into a fetch and applies the response. What
  is missing is that `IdentityEndpoint::bytes_for` resolves only its
  in-memory `resources` and `released` maps, so a capture living in the redb
  blob store answers `MissingResource`. Clicking through therefore needs a
  store-backed resource path on the publishing side — the same seam any graph
  node's content needs, not a receipt-specific one, and worth designing
  rather than bodging, since loading every capture into memory is exactly
  what the bounded `released` map exists to avoid.

  **So the two steps left for a lens are small and separable**: a store-backed
  `bytes_for` on the host, and an outbound session from turnstone. Neither is
  receipt-specific.

- **2026-08-10, the store-backed read — and a defect it uncovered.** Asking
  for read-through turned up something worse than the missing read: **R0 put
  the captures in the wrong store.**

  There are two blob stores in play. `receipt_ingest` wrote to a muniment
  `BlobStore<RedbBackend>`, while the personal graph replicates through
  `PersonalSyncHost::blobs()`, a `transport::BlobStore` over iroh, and that is
  what `fetch_blob_by_availability` reads when a peer acts on an availability
  fact. So every receipt was authoring `ObserveBlobAvailability` facts saying
  "this device holds blob H" for blobs that were never in the store a peer
  fetches from. The claim was one this device could not honour, and it would
  have failed at the far end, on another machine, long after the run — the
  worst possible place to find it. R1's "blobs ride transfer staging" was not
  actually true for receipts.

  **The fix moves byte-staging to the host**, which is where it always
  belonged: only the resident host can put bytes in the store its peers fetch
  from. The inbox hand-off is now an `InboxEntry` carrying the source
  directory as well as the events; `device_sync::stage_captures` reads each
  capture named by the artifacts facet, puts it into `host.blobs()`, and
  **verifies the resulting hash against what the graph is about to claim**
  before authoring. Staging happens *before* the turn, and a capture that
  cannot be staged leaves the receipt pending rather than authoring a promise
  it cannot keep. `captures_in` reads the list back out of the facet rather
  than taking it alongside, so there is one statement of what a receipt's
  captures are.

  **Then the read-through itself.** `IdentityEndpoint` gained an optional
  `ResourceReader` and a byte-budgeted cache (64 MiB, oldest-first eviction);
  `bytes_for` answers from `resources` and `released` first, so transfers cost
  nothing extra, and reads through only on a miss. The endpoint deliberately
  does not know what a store is — it lives in the `native` cone while the
  stores live in `web`/`personal-sync`, and the resource trait is synchronous
  while a store read is not. So the composer supplies the closure:
  `DeviceSurface` carries a `blob_reader`, `device_sync` builds it over
  `host.blobs()` with `block_in_place` (the resident host is already on a
  multi-thread runtime, so the blocking read moves off the async worker), and
  `browser_host` hands it to the endpoint.

  **Verified once the tree went green.** The concurrent `session-runtime`
  `wallet_grant` refactor landed, and `cargo test -p graphshell --features
  personal-sync --lib` is **207 passed, 0 failed**, up from 204 by exactly the
  three read-through tests: read-through serves a stored blob, absence stays a
  miss with no reader, and the cache evicts oldest-first inside its budget.
  Twenty-five receipt and read-through tests in all. The CLI was re-run end to
  end against the changed hand-off: the inbox entry now carries `source`
  beside `events`, and the node id, blob hash and event count are unchanged
  from before the format moved.

## 6. The lens, and the one decision in front of it

R3's publishing half is done and verified. What remains is turnstone opening
an **outbound session to the resident host**, and investigating it turned up a
gate that is a design choice rather than wiring, so it is recorded here rather
than guessed at.

**The client machinery is not the problem.** Turnstone already carries
`graphshell-client` and drives the whole loop — `apply_snapshot`, resolve,
`NeedsResource` to `resource` to `apply_resource` — in the G3 canary. The
transports exist too: `StdioCarrier::spawn` speaks the carrier protocol to a
child endpoint, and `relay_browser_native_messages` shows the
connect-and-hello shape for the resident device endpoint.

**The gate is admission.** The resident host's device endpoint admits
*browser extensions only*: `AllowedExtensions` is a chromium/firefox extension
id allowlist, and the broker hello carries a `BrowserLauncher`. A first-party
native app has no identity there. And this matters specifically because the
receipts live on the **resident** host's surface — the one holding the
personal graph and the replicating blob store — not on any endpoint a client
can spawn for itself.

Three ways forward:

1. **Widen the browser allowlist** to admit a first-party native launcher.
   Cheapest, and the worst: that allowlist's entire job is to be narrow, and
   putting a native app through it weakens the one check keeping arbitrary
   extensions out.
2. **A second endpoint on the resident host for first-party apps**, with its
   own admission, reusing the same session protocol and the same
   `IdentityEndpoint` composition (including the read-through reader already
   wired). **Recommended.** It keeps the browser gate exactly as narrow as it
   is, and it says plainly that a first-party app and a web extension are
   different kinds of caller with different trust.
3. **Turnstone spawns its own `graphshell_native_host`.** Works today with no
   new admission at all, and is the reason to check before building: that
   endpoint serves the identity projection, *not* the resident host's
   personal-sync surface, so it would show no receipts. It is a working
   session to the wrong authority.

Option 2 wants a short plan of its own (what admits a first-party app, and how
it proves it is one) before code. Everything it needs on the publishing side
already exists and is tested.

### 6.1 Decided: a second endpoint (Mark, 2026-08-14)

Option 2. A first-party application and a web extension are different kinds
of caller, so they get different doors; the browser allowlist stays exactly as
narrow as it is.

**The design, in one paragraph.** `native::app_admission` defines a distinct
endpoint (`GRAPHSHELL_APP_ENDPOINT`, defaulting to a per-user pipe on Windows
and a socket under `XDG_RUNTIME_DIR` elsewhere), a distinct hello schema so a
client at the wrong door is refused with a reason instead of half-speaking the
other protocol, an `AppId` label, and a default-deny `AllowedApps` whose
default set is `turnstone`.

**What proves "first-party" is the endpoint's permissions, not the name.** The
socket and pipe are the owner's, so reaching them at all means running as the
owner. The app id is a label: it tells the host and the operator which
application is connected and lets an owner turn one off. It is deliberately
not treated as a credential, and this gate is deliberately only the first of
two, the same shape the browser path has: the ordinary session admission
behind it still decides what the app may see.

**Landed:** the admission module with seven tests, including that a browser
hello is refused by schema with a message naming the right endpoint, that an
unknown app is refused, that a device may admit none, and that the default
endpoint is never the browser one.

### 6.2 Served (2026-08-15)

The door is open and end-to-end verified. Serving it turned out to be mostly a
deduplication job, because the browser door held two things worth sharing and
one thing worth keeping apart.

**Shared, extracted:**

- `native::local_endpoint` — the owner-only listener and connector. This is the
  piece it would have been worst to copy: it carries the Windows same-user SID
  check, which named pipes need because, unlike a `0600` socket in the runtime
  directory, they are reachable by other users on the machine by default. A
  second hand-rolled listener would have been a second copy of a security
  check, and copies drift. `device_broker` fell from 494 to 301 lines and now
  has one `serve` rather than one per platform.
- `native::local_session` — what a local session *is*: the minted local grant,
  the closed policy admitting exactly the projection service, and the endpoint
  composed over the resident surface (cards, decisions, read-through reader,
  released blobs). Both doors call it, so they cannot drift into two answers
  about what an admitted local client holds. `browser_host` fell from 807 to
  691 lines.

**Kept apart, in `native::app_broker`:**

- *Its own wire.* `AppMessage` / `AppHostMessage` carry the identical carrier
  payload in a separate envelope, so this door versions independently of a
  shipped browser extension.
- *No identity actions.* The browser door carries SSH import because a browser
  has no other route to the resident identity UI. A first-party application
  runs on this device and can open that UI itself, so the wire has no such
  variant at all — nothing to forward, rather than something forwarded and then
  refused by a session check.

**A fact worth recording.** Admission never touched the browser launcher; it
used only the nonces and the transcript binding. The machinery was already
client-agnostic and only the *label* was browser-shaped. That is now `LocalLink`
(`browser_carrier`), which binds whatever label it is given — an extension
origin for a browser, an application name for an app. So a link minted for one
client is not replayable as another, and there is a test pinning exactly that.

**Wired:** `graphshell_device_host` serves both doors, off one surface handle,
so a browser and an application on the same device see one set of cards rather
than two. `--app-endpoint` overrides, mirroring `--browser-endpoint`.

**Verified.** 208 graphshell lib tests pass, 13 of them on this lane. The one
that matters is `turnstone_opens_a_session_and_reads_a_capture`: a full session
over a duplex — hello, challenge, connect, open, snapshot, then reading a
capture's bytes back byte-for-byte through the store read-through the receipts
lane depends on. Also pinned: a refused application never sees a challenge, and
a browser connect frame does not open this door.

### 6.3 The turnstone client (2026-08-15)

`native::app_client::AppBrokerClient` is the client half: the whole handshake
(hello, challenge, connect, connected) plus typed calls for the three things a
card reader does — open, snapshot, resource — and `read_cards()` composing
them. Deliberately sequential against `&mut self`: the wire is strict
request/response and a card reader is human-paced, so a demultiplexer would be
machinery without a customer.

Verified over the real transport: `the_client_reads_cards_over_the_served_endpoint`
starts `serve_app_broker` on a uniquely named pipe and connects with the exact
code turnstone runs — finds the receipt card among the Personae cards the
endpoint also offers, reads the capture byte-for-byte, closes. 14 lane tests.

Turnstone's side landed in the turnstone repo: `DeviceReceiptsService` (worker
thread + runtime on the `KnotShareReaderService` pattern; a refresh opens a
fresh session, which is also how new receipts become visible, since the host
reads its surface at session start) and a `DeviceReceiptsPane` registered as
`turnstone.device-receipts` with a palette entry. The pane shows each card's
title, badges, value rows, and per-capture byte sizes — a size is shown only
after the bytes were actually read through the resident store, so the pane
reports reachability, never an advertisement.

### 6.4 Headed receipt (2026-08-16)

Taken, on this machine, with the whole pipeline live:

1. The installed resident host (scheduled task `graphshell-device-host`) was
   rebuilt with the app door and swapped in place; its log shows both doors:
   `door="browser"` and `door="first-party"` on their own pipes.
2. Turnstone ran `scenarios/device_receipts.scn` (self-drive, no OS input),
   opening the Device Receipts pane over the live pipe: "Connected. 9
   card(s) on this device." with the sync card and synced nodes.
3. That run's own captures became the receipt: a truthful `manifest.json`
   (real commit, real dirty count, `target: localhost`), `receipt_ingest
   --data-root` filed it in the host's inbox, and the host's log shows
   `staged receipt captures into the replicating store staged=2` then
   `authored a receipt into the personal graph events=9`.
4. A second scenario run then showed the receipt card itself:
   `turnstone · device_receipts on localhost · ok · Receipt · Passed ·
   Dirty checkout`, with `capture 1: 105058 bytes, readable` and
   `capture 2: 105058 bytes, readable` — sizes read through the resident
   store over the first-party door on that refresh, plus the blob
   availability cards naming `laptop`.

The receipt is self-referential on purpose: the card turnstone renders is the
receipt of its own first run. Captures live in
`testing/turnstone/images/device-receipts/` (run4 is the finished one); the
receipt directory with manifest and `graph-events.json` is
`testing/turnstone/device-receipts-local-2026-08-16/`.

Found and fixed along the way: the pane's capture rows sat below the fold
behind the card's value rows; they now come first, because reachability is
the fact the pane exists to show.

### 6.5 Cross-machine (2026-08-16)

**Fedora ThinkPad — done.** `handoff_circle.scn` ran on `192.168.4.28`'s own
Wayland screen, its PNG came home, and the local pane now shows
`hocket · handoff_circle on 192.168.4.28 · ok · Receipt · Passed` with
`capture 1: 125077 bytes, readable`. That byte count is the whole point: it
was read back out through the first-party door from the replicating store,
so it is evidence rather than a claim copied from the manifest.

**Intel iMac — done.** `cambium-genet-winit-host`'s `smoke.scn` ran on the
iMac's own Aqua session and its receipt came home and was authored:
`RESULT ok`, `frames: 3 captured, 0 blank, 3 distinct digests, 2 distinct
sizes`. The example's own guard is what makes that worth something — it
refuses to report ok unless a frame had real pixels *and* the frames around
a state change differ.

The first attempt appeared to hang in the winit event loop and was recorded
here as a genet-side defect. It was not one: the run was waiting on a macOS
permission prompt on the machine's own screen, which nothing on this side
could see. Accepting it and re-running the identical command passed. Worth
keeping as a lane property rather than an anecdote — **a headed run on a
macOS machine can block on a prompt that is invisible to the driver**, so a
first run there wants someone at the screen, and a "hang" is a prompt until
ruled out.

**Four defects in `remote-receipt.ps1`, each found by running it rather than
reading it** (its first real cross-machine use):

1. `systemd-run --user` starts a transient unit that does **not** inherit the
   ssh shell's cwd, so cargo ran in `$HOME`. Fixed with `--working-directory`.
2. `launchctl asuser` is root-only and fails over ssh as an ordinary user. It
   is also unnecessary: the preflight already refuses unless the ssh user IS
   the console user, and that user's ssh session can open a window on their
   own screen. The app's blank-frame checks remain the backstop.
3. A non-interactive ssh shell sources no login profile, so `cargo` was not on
   PATH on macOS even though installed.
4. **The lane stopped one step short of its own purpose.** `-IngestStore`
   stored the blobs but nothing filed the authored events, so a fetched
   receipt never became a fact in the graph and no card ever appeared — while
   the run reported success. `-IngestDataRoot` completes the hand-off.

A `-CargoProfile` parameter was added so a warm debug target can be used, with
the profile recorded in the manifest — a receipt that does not say which
profile it ran under invites the reader to assume the shipping one.

Also exposed: the Device Receipts pane has **no scroll container**. With a real
graph the host offers 35 cards and only the first few are reachable at all.
Receipts now sort first (by the badge the endpoint sends), which makes the pane
usable, but the missing scroll is a real gap.
