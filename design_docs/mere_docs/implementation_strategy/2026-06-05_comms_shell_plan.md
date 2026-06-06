# Comms Shell Plan

**Date**: 2026-06-05
**Status**: Draft (for review). The realization of Mere's **comms surface** in
meerkat: a docked peripheral pane that surfaces unified communications (misfin
mail + murm cabals, with room for mooting protocols), rendered through the same
domain → host pattern as the chrome. It closes the [modular integration
plan](2026-06-02_modular_integration_plan.md)'s **gap #7** ("murm/moot
unsurfaced") and its **S5** (comms surface), and is the on-screen form of the
inherited `COMMS_AS_APPLETS` family the [protocol architecture
plan](2026-05-05_protocol_architecture_plan.md) names.
**Grounded in**: a read of the actual crates this session (misfin, errand,
nematic, murm, the chrome domain) against the 2026-06-05 tree.

---

## The one idea

Comms is a **contingent projection like gloss / apparatus / settings**: a docked
peripheral pane, never a root. It follows the chrome pattern exactly. A
host-neutral **comms domain** crate (sibling to `chrome`) holds the
protocol-agnostic model (conversations, messages, identities, compose) over a
`ProtocolAdapter` seam; meerkat renders it as a docked pane; message bodies ride
the `nematic` engines the content card already uses. The backends (misfin, murm,
later mooting) are adapters behind the seam, so the shell is general from day one
and lights up each protocol as its backend matures.

---

## Decisions (resolved with Mark, 2026-06-05)

1. **Scope: the general comms shell**, not misfin-only. The pane is the
   comms-applet shell from the start (misfin + murm cabals + room for mooting
   protocols like Matrix / Nostr). Live adapters arrive as backends mature.
2. **misfin transport moves into errand (send lane).** errand gains an async
   misfin **send** lane reusing its `tls` module + the `titan_upload` write shape
   + client certs; the standalone sync misfin transport retires.
3. **misfin receive = run a server**, with an **external-server option**. Since
   errand is a *client* fetch transport, the server is **not** errand's job: the
   misfin crate keeps identity + types and gains the server; errand owns only the
   client send lane. A user can self-host the server or point at an external one.
4. **Pane = docked frame.** The first peripheral/docked pane, establishing the
   pattern gloss / apparatus / settings reuse (gap #6 / S4-adjacent).
5. **Pull murm Phase 2B forward** (`Cabal::send` / `subscribe` / `history`) so the
   shell opens with real two-way cabal conversations, not just misfin mail.

---

## Findings (grounded in the crates, 2026-06-05)

1. **The architecture mirrors chrome, in five layers.** Transport (errand send +
   misfin server) → identity (persona vault) → **comms domain** (new, host-neutral)
   → host pane (meerkat, docked) → rendering (nematic). `chrome` lives at
   `crates/graphshell/shell/domain/chrome`, so the comms domain is its sibling
   `crates/graphshell/shell/domain/comms` (relocates with the domain layer if §7
   cleanup moves it).
2. **Backend readiness is partial — this drove the decisions.**
   - *misfin* (`crates/murm/misfin`) is **send + identity only**: `send_message`,
     `identity_status`. There is **no receive / inbox / server** (hence decision 3).
     It is synchronous (std TCP + blocking rustls), self-contained.
   - *murm* (`crates/murm/murm`) is foundation: `SyncedCabal` (the LogSync lane)
     exists, but `Cabal::send` / `subscribe` / `history` is **Phase 2B, unbuilt**
     (hence decision 5). murm's own docs name the comms panel as a *separate*
     consumer — this pane.
   - *mooting* (`crates/moot/mooting`) is a **stub**; Matrix / Nostr / etc. are
     later adapters.
3. **errand fits misfin send, not receive.** errand is async, scheme-routed
   `fetch(url) -> Response`, with a `tls` module, a `CertRequired` status, and a
   `titan_upload` write companion. misfin **send** (a client-cert TLS write to a
   recipient host:1958) is the same shape as titan upload. misfin **receive**
   (serving a mailbox) is a server, a different shape errand should not take on.
4. **nematic already renders the body.** [`MisfinEngine`](../../../crates/inker/engines/nematic/src/misfin.rs)
   parses a gemmail body as gemtext → `EngineDocument` (`message/x-misfin`); the
   *envelope* (sender / recipient / timestamp / cert trust) is the host's job, and
   trust stays `Unknown` until the host verifies the cert. So the pane feeds the
   body to nematic and supplies the envelope + trust override.
5. **Identity belongs in the persona vault.** The protocol-arch plan reserves a
   `Misfin { keypair, handle }` vault slot; today misfin owns a standalone cert
   store. Tying the misfin cert to the persona vault makes the shell's "me" a
   persona.

---

## Phases (done-conditions, not dates)

Each phase is independently green; P1 / P2 and P4 are largely parallel, P3 gates
the inbox, P5 depends on the adapters, P6 is the visible payoff.

- **P1 — errand misfin send lane.** Add an async misfin **send** to errand
  (reusing `tls` + the `titan_upload` write shape + client certs); retire misfin's
  sync TLS transport. *Done:* errand sends a gemmail to a host and returns the
  outcome; the misfin crate's send delegates to (or is replaced by) errand's lane.
- **P2 — misfin identity in the persona vault.** Wire the `Misfin { keypair,
  handle }` slot: derive / store the misfin client cert in the identity vault, so
  send + server present a persona identity. *Done:* the shell's misfin identity is
  persona-derived, not a standalone cert file.
- **P3 — misfin server (receive).** A misfin server in the misfin crate accepts on
  `:1958`, verifies sender certs (TOFU), and lands gemmail in a local mailbox
  store; a config points at an external server instead. *Done:* a gemmail sent
  (P1) lands in the recipient's inbox, self-hosted or via an external server.
- **P4 — murm Phase 2B (pulled forward).** Fill `Cabal::send` / `subscribe` /
  `history` over the Cable protocol on the existing `SyncedCabal` substrate.
  *Done:* a cabal has a real send / history / live-subscribe API for the shell.
- **P5 — the comms domain crate.** `crates/graphshell/shell/domain/comms`: a
  host-neutral `Conversation` / `Message` / `Identity` model + compose state + a
  `ProtocolAdapter` trait, with a **misfin adapter** (mailbox → conversations,
  gemmail → messages, identity) and a **murm adapter** (cabals → conversations,
  posts → messages). Headless-tested, no host dependency. *Done:* one unified
  comms model populated from both backends.
- **P6 — the meerkat comms pane (docked).** Establish the docked peripheral-pane
  slot in the host (the gloss / apparatus / settings pattern), and render the comms
  domain into it: identity, a conversation list, a reader (envelope + nematic body),
  and a compose / send view. *Done:* open the comms pane, see your identity and
  conversations (misfin mail + murm cabals), read + compose + send.
- **Later:** mooting protocol adapters (Matrix / Nostr / …) as those backends land;
  external-server polish; gloss / apparatus / settings reusing the P6 docked-pane
  slot.

**First shippable, demoable milestone:** P1 + P2 + P3 give send + receive misfin
end to end (provable headless with two local identities / a local server); P5 + P6
make it a live shell (compose, send, read, conversation list) with murm cabals
(P4) alongside.

---

## Open design points

- **Where the docked-pane mechanism lives.** Full `frame::FrameLayout` (the tiled
  S4 machinery) vs a simpler docked-strip slot first. The general shell does not
  need BSP tiling; a single docked side panel (resizable, toggleable) is enough for
  P6 and for gloss / apparatus / settings. Lean: a minimal docked-pane slot now,
  `FrameLayout` when the workbench (S4) actually needs splits.
- **The misfin server's home.** In the misfin crate (a `server` module beside the
  client) keeps it cohesive; a separate `misfin-server` crate isolates the
  accept-loop / mailbox-store dependencies. Lean: a module in the misfin crate
  until the deps argue for a split (the 600-LOC ceiling may force the split).
- **The comms-domain model shape.** How a `Conversation` is keyed across protocols
  (misfin mailbox thread vs murm cabal vs a future Matrix room), and the identity
  model (one persona, many protocol handles). Settle in P5.
- **External misfin server config.** How a user points the shell at an external
  server (a setting, a per-identity host field). P3 detail.
- **Async move correctness.** Moving misfin send sync → async (into errand) must
  preserve cert generation + TOFU known-hosts behaviour; port the misfin
  transport tests alongside.

---

## Risks and hard parts

- **Six-phase, multi-crate, sibling-repo arc.** errand (sibling repo) + misfin +
  murm + a new domain crate + meerkat + nematic. Sequence so each phase lands
  green; do not let the pane (P6) get ahead of the adapters (P5) or the backends
  (P3 / P4).
- **murm Phase 2B is real protocol work**, not glue: the Cable `send` / `history` /
  `subscribe` API over the per-author-log substrate. Budget it as its own slice.
- **The docked-pane pattern is new host architecture.** gloss / apparatus /
  settings inherit it, so get the slot's shape (placement, resize, toggle, input
  routing, separate-roots discipline) right the first time.
- **misfin server is a network listener** inside the app: bind lifecycle, cert
  verification, mailbox persistence, and the never-fatal-to-the-shell discipline
  (a server failure disables receive, not the shell), mirroring the sync subsystem.

---

## Progress

- **2026-06-05** — Plan drafted (no code). Read the crates (misfin = send +
  identity only, no receive; murm = foundation, send/history is Phase 2B; mooting =
  stub; errand = async client fetch with `tls` + `titan_upload`; nematic
  `MisfinEngine` renders bodies, envelope is the host's job; chrome domain at
  `crates/graphshell/shell/domain/chrome`). Resolved four decisions with Mark
  (general comms shell; misfin send into errand; receive = a misfin server with an
  external-server option; docked frame pane; pull murm 2B forward). Sliced the work
  P1 (errand send) → P2 (vault identity) → P3 (misfin server) → P4 (murm 2B) → P5
  (comms domain crate) → P6 (docked meerkat pane), with mooting adapters later.
  Next: Mark's steer on the first phase to build.
- **2026-06-05 — P1 (errand misfin send lane) built + green (local errand clone).**
  Added a transport-only misfin **send** to errand: a new `misfin` module
  (`misfin://mailbox@host <message>\r\n` over a client-cert TLS connection, reusing
  `gemini::parse` for the gemini-format response), a `client_connector` in `tls.rs`
  (client-auth, reusing the TOFU-permissive `AcceptAny` server verifier), and
  `misfin_send` / `ClientIdentity` / `MISFIN_PORT` exports. **Transport-only by
  design:** the caller supplies the cert DER (`ClientIdentity`), so `rcgen` +
  identity stay out of the clean public errand crate (no new deps; reuses errand's
  rustls / tokio). **32 errand tests pass** (4 new misfin `request_parts` tests:
  request line, default + explicit port, scheme + mailbox validation), doc-test
  green, zero new clippy warnings (the one `ptr_arg` warning is pre-existing in
  `guppy.rs`). **Push-gated:** errand is a git dep in meerkat now, so the change
  lives in the local clone until pushed — an outward step held for Mark's OK (and
  his call on the remote: errand's own Cargo.toml names `sgtmark/errand`, meerkat
  deps `mark-ik/errand`). Worked inline/foreground.
- **2026-06-05 — P1 pushed to `mark-ik/errand` main** (`acaa059..2f500b3`, Mark's
  go-ahead). `errand::misfin_send` is live on the remote; meerkat consumes it via a
  Cargo.lock bump when P5/P6 wire it.
- **2026-06-05 — P2 (vault-derived deterministic Ed25519 misfin identity) built + green.**
  First validated the approach (Mark's pressure-test on "leverage the p2panda
  precedent"): the [Misfin spec](https://github.com/JCLemme/misfin/blob/master/specification.gmi)
  mandates **no key algorithm** (any self-signed x509), and a **live server**
  (`satch.xyz`) accepts an Ed25519 client cert — a bogus-mailbox probe through
  `errand::misfin_send` returned status 51 (mailbox doesn't exist = cert accepted,
  nothing delivered; `crates/probes/misfin-ed25519`). So the precedent is safe on
  all three axes: spec (agnostic), interop (live-confirmed), privacy (per-address
  salt + the standard master-derivation tradeoff). The crux: only **Ed25519** gives
  a *reproducible* cert (deterministic signatures, RFC 8032); the crate's current
  ECDSA P-256 randomises its signature, so it must be persisted. Then built it:
  `misfin::deterministic_identity(seed, spec)` imports an Ed25519 key from a 32-byte
  seed (PKCS#8 v1) and mints a self-signed misfin cert with a **fixed serial** over
  the existing fixed validity + DN (USER_ID = mailbox, CN = blurb, SAN = host), so
  the same seed + address reproduce a byte-identical cert and SHA-256 fingerprint:
  a **vault-reproducible identity, no on-disk cert**. Plus `identity_salt(address)`
  (per-address, domain-separated derivation salt — the privacy guardrail keeping a
  persona's addresses unlinkable) and a public `MisfinIdentityMaterial { certificate_der,
  private_key_pkcs8_der }` that wraps straight into `errand::ClientIdentity`. **The
  send path is now complete at the identity level:**
  `derive_keypair(identity_salt(&addr)).to_seed()` → `deterministic_identity` →
  `errand::ClientIdentity` → `errand::misfin_send`. Decoupled: the misfin crate
  takes a raw seed (no identity-vault dep). 11 misfin tests green (3 new:
  byte-reproducibility, different-seed-different-identity, per-address salt); zero
  new clippy warnings (the crate's pre-existing unused-import warnings are
  unrelated). Worked inline/foreground.
- **2026-06-05 — P3a (retire misfin's synchronous send) done.** With errand owning
  the send transport (P1), misfin's own blocking-TCP send lane was redundant, so it
  is removed. Gone from the crate: `send_message` / `send_message_for_tests` /
  `trust_status` / `forget_known_host` / `parse_misfin_response`, the wire types
  (`MisfinRequest` / `MisfinResponse` / `MisfinSendOutcome` / `MisfinTrustStatus`),
  the TOFU known-hosts machinery (`MisfinKnownHostRecord` / `MisfinKnownHostsStore` /
  `MisfinTofuVerifier` + its `ServerCertVerifier` impl), the transport-layer socket
  helpers (`connect` / `resolve_socket_addrs` / `read_misfin_response` / redirect
  + authority helpers), the known-hosts persistence helpers, and the connect/IO/port/
  redirect consts. What the misfin crate now **is**: identity (`ensure` / `rotate` /
  `forget` / `identity_status` + the random ECDSA persisted identity *and* the
  deterministic vault-derived Ed25519 identity), the types (`MisfinAddress` /
  `MisfinSender` / `MisfinGemmail` / `MisfinIdentitySpec` / `MisfinIdentityMaterial`),
  and gemmail parsing (`parse_gemmail`). Builds clean (no warnings; dropped the
  `#![allow(unused_imports)]` that had masked the dead transport imports). 5 tests
  green (down from 11 — the 6 dropped tests covered the retired send/TOFU paths).
  No workspace consumer referenced the removed API (webfinger only matches the
  `misfin://` URL scheme as strings). Worked inline/foreground.
- **2026-06-05 — P3b (misfin receive server) built + green.** First confirmed the
  server-side wire format against the [spec](https://github.com/JCLemme/misfin/blob/master/specification.gmi):
  the request is a **single** CRLF-terminated line, ≤2048 bytes,
  `misfin://<mailbox>@<host> <message>` (message = remainder after the first space,
  up to CRLF — no multi-line read), the reply is gemini-shaped `<status> <meta>\r\n`,
  and a sender's identity *is* its SHA-256 cert fingerprint. Then built the server in
  the misfin crate behind a new opt-in **`server` feature** (so the lightweight
  identity/send path does not pull tokio + redb): `tokio` + `tokio-rustls` (aws-lc-rs,
  matching the crate's existing rustls backend) + `redb`. Two new modules:
  + `mailbox.rs` — a redb-backed `MailboxStore` (one file, all mailboxes): a
    `messages` table (monotonic seq → JSON `ReceivedMessage`), a per-recipient
    `mailbox_index` multimap (the inbox-read path `list(mailbox)`), a `meta` seq
    counter, and a `senders` table recording first-seen fingerprints (`record_sender`
    → `First` / `Known`). `open(path)` + `in_memory()`, both `Clone` (shared `Arc<Database>`).
  + `server.rs` — `MisfinServer::new(config, store)` builds a `tokio_rustls`
    acceptor whose `AcceptAnyClient` verifier **requests but does not require** a
    client cert (no-cert clients still complete the handshake, so the server replies
    60 at the app layer rather than dropping the handshake). `bind(addr).await` →
    `BoundMisfinServer` (exposes `local_addr()` for `:0`), `serve(shutdown)` runs a
    select-loop accept, one task per connection, until the shutdown future resolves —
    host-neutral, so a daemon-side `SessionServiceRunner` worker just spawns it. The
    TLS-free `Dispatcher::dispatch` is the testable core: **60** no cert, **59**
    malformed, **53** host not served, **51** mailbox unknown, **20** delivered (META
    = the recipient mailbox's own fingerprint, so the sender can pin it), **40** on a
    storage fault.
  + **Status: 15 tests green** (3 mailbox, 6 dispatch, 1 response-encode, **1 real-TLS
    round-trip** — binds `127.0.0.1:0`, an in-test client presents a vault-derived
    Ed25519 cert, sends, and the message lands in the inbox with a `20 <fingerprint>`
    reply). Default (no-feature) build + its 5 tests still green; new code is
    clippy-clean (the 4 remaining warnings are pre-existing in `parse_gemmail` /
    `decode_hex`). server.rs 544 LOC, mailbox.rs 215 — under the 600 ceiling.
  + **Deferred to P3b′ (one clean follow-up, needs an x509 DN parse):** status **63**
    ("you're a liar" — a known identity presenting a changed fingerprint) and a
    human-readable `mailbox@host` from-line. Both require resolving the sender's
    *claimed* address from the cert's USER_ID + SAN; v1 tracks by fingerprint (which
    the spec says *is* the identity) and stores `sender_address: None`. Also deferred:
    the redirect/rate-limit/authorization codes (30/31/40-series/61) and the
    `WorkerKind::MisfinServer` host wiring (the manifest vocabulary + a concrete
    `SessionServiceRunner`, a host-layer task that lands with the pane in P6).
    Worked inline/foreground.
- **2026-06-06 — P4 (murm Phase 2B, pulled forward) built + green.** Audited murm
  first: **send + history already existed** (`CabalHandle::send_text`/`send_*` +
  `history`; `SyncedCabal` adds the gossip + LogSync lanes over them). The genuine
  gap was **`subscribe`** — a live post stream so the shell updates without polling
  `history()`. Built it at the **engine** (the single chokepoint every post funnels
  through — `post_with_kind`, `ingest_post`, and `ingest_operation`→`ingest_post`
  all end in `store.insert`), so one mechanism covers every arrival path:
  + `murmuring::CableEngine` — each `CabalSession` gains a per-cabal
    `tokio::sync::broadcast::Sender<Post>` (capacity 256). `post_with_kind` and
    `ingest_post` now capture the store's first-insert bool and fan out the post
    **once** on a genuinely-new insert, so a post landing on both the gossip and
    LogSync lanes emits once (not twice). New `CableEngine::subscribe(cabal_id) ->
    broadcast::Receiver<Post>`. murmuring gains `tokio` with **only the `sync`
    feature** (runtime-free: `send` is non-blocking; only a consumer's
    `recv().await` needs an executor), so the crate stays sync-core.
  + `murm` — `CabalHandle::subscribe()` (delegates to the engine) and
    `SyncedCabal::subscribe()` (delegates to the handle; since the sync lanes ingest
    into the same engine, a SyncedCabal subscriber sees local + gossip + LogSync
    posts). `tokio`'s `sync` feature added. Semantics documented: emits posts stored
    *after* subscribe (pair with `history` for the backlog), dedup by `PostId` is
    cheap (content-addressed), and on `Lagged` re-read history.
  + **Status: murmuring 76 tests green** (3 new: subscribe emits authored posts,
    only-after-subscribe, and once-across-duplicate-ingest), **murm 15 tests green**
    (2 new: a handle-level authored+ingested round-trip, and a `SyncedCabal::subscribe`
    firing end to end over a real `P2pandaTransport`). Both crates clippy-clean.
    Deferred (later phases, not blocking the shell): the host-led co-op session flows
    (`host_coop`/`join_coop`). Worked inline/foreground.
- **2026-06-06 — P5 (the comms domain crate) built + green.** New crate
  `comms` at `crates/graphshell/shell/domain/chrome`'s sibling
  `crates/graphshell/shell/domain/comms` (registered in workspace members +
  `[workspace.dependencies]`; `misfin` added to the deps table). Built as chrome's
  twin: a **WASM-clean core** (the model + the seam, no tokio/egui/platform I/O,
  headless-tested) with the heavy backends behind features.
  + **Model** (`model.rs`) — `ProtocolKind` (Misfin / Murm), `Identity`
    (protocol + address + optional display name), `ConversationId` /`MessageId`,
    `Direction`, `MessageBody` (Gemtext for misfin / PlainText for murm — so the
    pane knows when to run nematic), `Message`, `Conversation` (lightweight list
    metadata; messages load on demand), `Draft`.
  + **Seam** (`adapter.rs`) — `ProtocolAdapter`, an `async` object-safe trait (via
    `async_trait`): `protocol` / `identity` / `conversations` / `messages` / `send`,
    with `AdapterError`. Object-safe so the host holds a heterogeneous
    `Vec<Box<dyn ProtocolAdapter>>` and lights each protocol up as it matures.
  + **Aggregator** (`comms.rs`) — `Comms` holds the adapters and is the unified
    model: `inbox()` merges every backend's conversations (recency-sorted) and
    returns an `Inbox { conversations, failures }` that **surfaces** a down backend
    rather than blanking the list or hiding it; `messages` / `send` route to the
    owning adapter by protocol.
  + **murm adapter** (feature `murm-adapter`) — a `CabalSink` seam (implemented for
    both `CabalHandle` and `SyncedCabal`, so the host hands it gossiping cabals and
    tests hand it light ones) maps cabals → conversations and `Text` posts →
    messages (control posts filtered), with direction from the cabal author key and
    send through the sink. Pulls murm (tokio + iroh), so it is opt-in.
  + **misfin adapter** (feature `misfin-adapter`) — reads the receive server's
    `MailboxStore`, groups mail by correspondent (by resolved address when present,
    else fingerprint), and parses each gemmail for its subject + gemtext body.
    Read-only for now: `send` reports `Unsupported` (misfin sending rides errand +
    a vault identity, wired in the pane). Pulls misfin's `server` feature.
  + **Status: green in all four configs** (no-feature core, `murm-adapter`,
    `misfin-adapter`, both): 7 core tests + 4 murm-adapter + 4 misfin-adapter + a
    doctest, plus an `InMemoryAdapter` test double (the `test-support` feature) that
    drives the headless core tests (merge, recency sort, routing, capability,
    failure surfacing). comms is clippy-clean. The core compiles to wasm32 (the PWA
    path); adapters that can't are opt-out. Deferred to P6: wiring the adapters to a
    live host (a running misfin server's store, the user's synced cabals, the
    persona's misfin identity) and the misfin send path (errand + vault).
    Worked inline/foreground.
- **2026-06-06 — P6 started: P6a done (verified), P6b written (verification blocked).**
  Approach (chosen with Mark): render the pane first with **placeholder data**,
  then run meerkat and iterate the visuals; live backends after. The peripheral-panes
  architecture (`technical_architecture/2026-06-06_peripheral_panes_architecture.md`)
  frames comms as the **first** docked pane, so P6 establishes the dock pattern.
  + **P6a — the comms pane view-model (done, headless-verified).** `comms::pane`:
    `DockSide` / `DockState` (the minimal dock contract — open / side / size / focus)
    and `CommsPane` (dock + the inbox snapshot + surfaced failures + selection + the
    open thread + the draft) with its logic (toggle, select aims the draft + clears
    the thread, `set_inbox` keeps/drops the selection, send-readiness). 6 new tests
    (comms now 13 green); clippy-clean. This is what the host renders, the way
    meerkat renders `chrome`'s `ToolbarState`.
  + **P6b — the meerkat docked pane (written, mirrors the settings overlay).**
    `comms` dep added to meerkat; `Chrome` gains `comms: CommsPane` + a `comms_draft`
    editing buffer (seeded with placeholder conversations + threads); a `ToggleComms`
    palette command; a `comms_pane` view in `views.rs` (header + close, a surfaced
    failures line, a conversation list whose rows select, a thread view with
    in/out message bubbles, and a lensed compose field + Send); CSS in `CHROME_SHEET`
    for a right-edge docked panel. Send echoes the draft into the thread (placeholder).
  + **Blocked: meerkat compile-verification.** The working tree has **pre-existing,
    unrelated WIP** in `graph-kernel` — a mid-flight `NodeNavigationMemory` →
    `SharedNavigationMemory` restructure (per-node memory → a shared snapshot with
    owners; `graph/history.rs` renamed + restructured the type, but `node.rs` / `mod.rs`
    / `query.rs` / `snapshot` / `node_props` / `import_records` still import the old
    name, and the rkyv archived shape changed). `kernel` fails to compile, which
    blocks the whole meerkat build *before* the comms code is reached. The P6b code
    touches no kernel; it could not be compiled to confirm. Next step: finish (or
    stash) the kernel refactor, then compile-verify P6b and run meerkat to tune the
    pane geometry + interaction; then wire live backends. Worked inline/foreground.
