# Knot Publishing Protocol Plan

**Date**: 2026-08-07
**Status**: Phase A implemented and physically receipted, including a
public-client renewal on 2026-08-19. Direction remains **A then B** (§4). The
existing K2 Graphshell projection rehearsal is useful precedent, not this
protocol. Phase B has not begun: the Phase-A grammar needs real product use and
an intended independent implementer before it is promoted into a compatibility
commitment.

**Scope**: Let one persona explicitly share selected, versioned Knot documents
with another persona without giving that reader a paired-writer grant, a vault
key, or a replica. Phase A is authenticated, read-only publishing. It does not
mean anonymous public hosting.

---

## 1. What prompted this

A survey of [Demarkus](https://www.demarkus.io/) — "versioned markdown served
over QUIC", read-only by default, capability tokens for writes, nothing ever
deleted. The initial verdict was that it did not fit this workspace. That was
wrong, and worth recording as wrong: it conflated *"does not fit the smolweb
crate shape"* with *"does not fit us"*. Only the second matters.

Against what already exists here:

| Demarkus claims | Mere already has |
| --- | --- |
| Versioned, nothing deleted | codicil's append-only log |
| Capability-based access | the participant gate, personae, paired writers |
| QUIC transport | iroh, via p2panda |
| Markdown-native documents | djot knot bodies, `text/x-knot` |

So this is not a capability gap. It is a **publishing** gap, and it is
narrower and sharper than "implement Demarkus".

## 2. The actual gap, stated precisely

Four facts are important, because they keep this from duplicating something
that is already real:

- `knot://vault/{id}` is the local address emitted for a vault document by
  `ports/knot/src/endpoint.rs`. Its `vault` authority is not a remotely
  resolvable host identity.
- `knot_endpoint` is a local Graphshell projection endpoint. It gives a host a
  card/resource view; it does not expose raw Knot source over a network
  protocol.
- `KnotSyncHost` joins a personal vault across paired writer devices. It is
  replication, not a read capability for a different persona.
- `examples/k2_peer.rs` rehearses a different path: a holder serves a Graphshell
  projection to an admitted visitor. Its paired in-process fixture proves the
  non-physical layers, and its 2026-08-08 physical receipt passed from a Q-PC
  holder to a Windows visitor over an explicit ticket. The visitor gets a
  Graphshell session, and the example deliberately gives both machines the
  owner secret so it can mint a fixture grant. It is neither a shareable
  recipient capability nor a raw document protocol.

The gap is therefore specific: a holder cannot issue a restricted reader a
share artifact, then serve that reader a selected source document and a pinned
historical version through Knot itself.

## 3. What this should and should not reuse

The admission sequence in Graphshell is correct:

    transport.accept(alpn)
      -> read the acceptance facts (never application bytes)
      -> secure the stream
      -> admit_session(policy, revocation ledger, facts)
      -> check the principal actually serves this action
      -> spawn, and return to accepting

`graphshell::carrier::accept_projection_session` is **not** the reusable
function. It hard-codes `mere/graphshell/v1`, the Graphshell service path, and
the `connect` action, while `ports/knot` only dev-depends on Graphshell. Making
Knot depend on it would reverse the dependency graph.

The reusable seam is lower:

- `transport::AcceptedSession::into_session` is the one audited conversion from
  carrier acceptance into facts.
- `transport::noise` composes the encrypted inner stream.
- `notochord::admit_session` owns bounded handshake framing, chain evaluation,
  refusal completion, and the admitted conclusion.

Knot gets a service-specific wrapper around those primitives. It must not copy
chain validation or hand-build `SessionFacts`.

There is one prerequisite extraction. Graphshell's `SessionAuthority` retains
the admitted certificate chain and rechecks revocation and expiry, but its
projection-session and score vocabulary make it the wrong type for Knot. Move
the neutral part to `crates/system/notochord` as a retained admitted authority:
principal plus verified chain, `lapse(ledger, now)`, and scoped
`covers(path, action, now)`. Graphshell becomes its consumer. Knot then uses
the same retained conclusion rather than reproducing the lifecycle logic.

Likewise settled and not to be re-litigated:

- **Transport is iroh.** "iroh is the byte plane. Nothing below moves blobs off
  iroh" (reachability plan). Not raw QUIC or a second carrier abstraction.
- **Noise is the inner confidential stream.** In Phase A its proved identity is
  deliberately the same Personae/device key as the carrier identity. Notochord
  currently enforces that the claimed subject equals an authenticated carrier
  initiator. A distinct Noise persona or bearer identity is a later Notochord
  design, not an exception this host may smuggle in.
- **The served source is authored bytes.** The publisher sends the selected
  djot/Knot source plus declared media type, not a Graphshell card, an
  `EngineDocument`, a derived cache, or a rendered projection. A recipient
  opens it with `KnotEffectMode::Never` unless their own local trust settings
  later say otherwise.

## 4. The direction: A, then B

**How much of a protocol does this want to be?** Both, in that order. A is not
a trial of whether B is worth doing — B is where this is going. A is how B gets
written from something that has carried traffic rather than from a guess.

Stating that plainly matters for a reason this workspace has been bitten by
before: a phase recorded as "an option we might take" reads, months later, as a
thing that was considered and dropped. This is a sequence.

### Phase A — private Knot publishing over an ALPN

Knot accepts an ALPN of its own, performs the shared Notochord/Noise admission
sequence, and serves exactly three read operations: list an explicitly
published catalog entry, fetch its current causal head, and fetch one retained
version by opaque operation digest. There is no new URI scheme and no new
crate.

The reader learns the route and the narrowly scoped grant from a
`KnotShareTicket` passed out of band. A local `knot://vault/{id}` remains a
local source address; it does not pretend to contain a peer identity, a route,
or a credential. The ticket is the Phase A remote-resolution object. Its
endpoint ticket is reachability data, not a durable identity, and its reader
certificate is a capability, not a URL parameter.

This is private between Mere instances. It is useful on its own: two personas
on separate machines can share a note without becoming paired devices or
replicating the vault.

**Done when:** an explicitly selected, conflict-free djot note is fetched by a
different, unpaired persona on a second physical machine using a holder-issued
share ticket; its bytes, media type, content digest, and causal head verify;
and revoking the same reader prevents a later request from revealing any
document byte.

**Status (2026-08-19):** Complete. The original Windows-to-ThinkPad direct-LAN
mDNS read and revocation receipt was renewed with a ThinkPad holder and Q-PC
reader. The Q-PC runner was built from `162be7a9` and used
`fetch_published_document`, rather than its former parallel Noise, Notochord,
and wire sequence. It read the selected `text/vnd.knot` source; after holder
revocation, the same ticket produced the reader's non-disclosing refusal while
the holder recorded `NotAdmitted(Delegation(Revoked))`. The two fixture
identities remained distinct.

### Phase B — publish the stable contract

The behaviour that survived Phase A is written as an implementable
specification and receives a dependency-light `knot-protocol` crate with
parsers, writers, and conformance vectors. The crate does not know a vault,
filesystem path, encryption key, Graphshell endpoint, or UI.

The cost is real and should not be understated. A published protocol is a
compatibility promise, and this workspace's rule has been that a crate ships
only where a specification exists to be faithful to. Here **we are the ones
writing that specification**, which is a different and larger undertaking than
implementing someone else's. Every ambiguity becomes one an implementer has to
guess at.

What Phase A must hand over is the request grammar as it actually settled, the
refusal vocabulary as it actually got used, and the versioning semantics the
causal store can actually support. A specification written before those are
known describes something nobody wanted.

**Enter Phase B when:** Phase A's grammar has stopped changing under real use
and a second party intends to implement it.

### Phase B decision gate — Demarkus Mark interoperability

This was rechecked. Demarkus now links a public [Mark Protocol working
draft](https://github.com/latebit-io/demarkus/blob/main/docs/SPEC.md), so the
old "no wire specification" blocker is closed. It is not automatically our
protocol: the draft specifies `mark` ALPN over QUIC/TLS, CommonMark plus YAML,
numeric immutable versions, and write verbs. Phase A instead has an inner
Noise session, Notochord delegation, a ticketed iroh route, djot/Knot media
types, causal operation heads, and no writes.

Before authoring a competing published protocol, Phase B produces a compact
compatibility matrix covering transport, identity, addressing, content,
versioning, caching, error semantics, and licensing. It then makes one recorded
choice:

1. implement a bounded Mark read adapter beside the native Knot lane;
2. publish the native Knot contract; or
3. adopt a later revised external specification if it preserves the required
   facts.

An adapter may project a Knot document into Mark's contract. It may not leak a
Personae delegation as a raw Mark token, flatten causal conflicts into a false
linear version, or import Demarkus's AGPL implementation. The published Mark
specification being CC0 does not change the implementation licence boundary.

## 5. Phase A contract

### 5.1 Publication is an explicit catalog, not discovery

`KnotPublishCatalog` is an owner-controlled map from an opaque
`PublicationId` to one source document and its read policy. A source document
is absent until the owner adds it. `LIST` enumerates that catalog after scope
filtering; it never walks a directory, searches a vault, exposes conflict ids,
or turns a share capability into a vault index.

The first supported source is a causally retained vault document. A directory
file has a current byte sequence but no immutable history API, so it cannot
honestly implement version fetch yet. A directory adapter belongs in a later,
separate plan once it can name and retain immutable revisions.

Only a document with exactly one causal current head is eligible. Pending
history, a delete, an unresolved conflict, and a synthesized automatic merge
are not silently published in Phase A. They return the same non-disclosing
`NotAvailable` result as an unknown publication. This is intentionally narrow:
publication does not get to invent its own conflict policy.

Unpublishing removes the catalog entry and stops **all** current and historical
reads through this host. It withdraws access; it does not claim to erase facts
the holder has retained or bytes a reader already received.

### 5.2 Version identity

The current causal head in `KnotDocumentProjection::document_heads` is the
version selector. It is a 32-byte operation digest, encoded in the wire format
as fixed bytes, not as an integer, timestamp, plaintext hash, or "latest".

`GetVersion` names both the publication and one operation digest. The source
adapter must materialize that exact retained `Put` or explicit `Resolve`
document, verify that it belongs to the publication's local document id, and
return its authored body. An operation that is a deletion, another document,
pending, unavailable to the holder, or not a document-producing event is
`NotAvailable`.

This needs a narrow history query in `ports/knot/src/sync.rs`; the existing
projection already collects causal history internally but exposes only current
documents and heads. Add the query there. Do not reverse-engineer history from
`KnotEndpoint` snapshots.

Every successful document response carries both the causal operation digest,
which identifies the retained version, and a BLAKE3 digest of the exact body,
which lets the reader verify transport and cache bytes without presenting a
plaintext digest as causal authority.

### 5.3 Ticket and authority shape

`KnotShareTicket` is a versioned, serializable handoff containing the
publisher's stable transport identity and endpoint ticket, the Notochord
network and service path, the one `PublicationId`, the recipient-bound signed
delegation chain for `read`, and an optional pinned causal head for a
reproducible share.

The holder issues the certificate to the reader's Personae/device public key.
The reader supplies it only in Notochord's signed session hello, never as a
document request field. Ticket distribution is out of band in Phase A. A
ticket is secret material where its grant is secret, so it must not enter
address-bar history, analytics, request logs, or a document link.

The host checks the admitted retained authority against the entry's read path
before every response. A service-wide grant may list the entries it covers; a
one-entry grant may fetch only that entry. Prefix comparisons use the existing
slash-aware Personae scope rule.

### 5.4 Phase A wire constraints, deliberately provisional

Phase A proves three semantic operations: list only the publications covered by
the reader's authority, fetch one current eligible document, and fetch one
retained causal version. It does **not** yet choose their portable grammar,
encoding, enum names, or stream lifetime. Those are the things Phase A is meant
to learn before Phase B promises them.

The prototype must run only after Noise and Notochord admission; bound request
and response parsing before allocation; keep raw source bytes in the document
response alone; return the causal operation and BLAKE3 body digest with a
successful document; and make every unavailable source state
non-disclosing. Unknown, unpublished, conflicted, deleted, out-of-scope, and
unavailable historical documents must be observationally equivalent to the
reader.

P2 may use a versioned serde/postcard envelope and one request/response per
admitted stream as its first implementation. They are useful experiments, not
protocol commitments. If real use exposes a different framing or session shape,
replace them and retain the captured fixtures as evidence for Phase B.

The owner configures document-byte, catalog-entry, concurrent-session, and
request/response ceilings within compile-time hard caps. A candidate response
that would exceed a ceiling fails before any body byte is written. Its parser
must reject malformed identifiers, duplicate or trailing data, and oversized
frames.

Phase A emits the live native media type `text/vnd.knot`; it may also serve a
selected `text/djot` document. `text/x-knot` is currently accepted by the
renderer but is not the emitted type in `ports/knot`. Phase B must settle one
canonical public registration or alias policy before calling either portable.

### 5.5 Request sequence

For each accepted `mere/knot-publish/v1` transport stream:

1. Call `AcceptedSession::into_session` and retain its observed facts.
2. Run `transport::noise::secure_responder` with the holder's device identity;
   require its encrypted ALPN and proven peer key to agree with the outer ALPN
   and authenticated carrier peer.
3. Run `notochord::admit_session` over the resulting `NoiseStream`. The client
   binds its hello to the outer ALPN and carrier peer exactly as the existing
   carrier does.
4. Retain the admitted authority and check it before reading the request and
   after source selection. For the final check, acquire a read guard on the
   revocation ledger and hold it through the complete response write.
5. Validate the one request, check its publication path against the retained
   authority, materialize the eligible source, send the response, and shut down
   the stream.

The host keeps an `Arc<RwLock<RevocationLedger>>`; admission snapshots it only
for the handshake, while the serving task rereads it for the later checks.
Response and revocation linearize at that lock: if a revocation obtains the
write guard first, the response is denied; if the final response read guard is
already held, that bounded response is in flight and the revocation governs the
next request. Expiry is evaluated when the response guard is acquired. A RAII
live-session guard, as in `ResidentProjectionHost`, releases capacity if a task
errors or panics.

## 6. Phase A work plan

### P0. Extract retained admission authority

**Status:** Complete. `notochord::RetainedAuthority` owns retained-chain
expiry, revocation, and scoped coverage; Graphshell consumes it for its
projection-specific lifecycle status.

**Files:** `crates/system/notochord/src/*` and
`ports/graphshell/src/lifecycle.rs`.

Add the neutral retained-authority type to Notochord and migrate Graphshell's
expiry, revocation, and slash-bound scope tests to it. Keep Graphshell's
projection-session and client-status mapping in Graphshell.

**Done when:** Graphshell receives identical lifecycle outcomes, while a
non-Graphshell caller can retain an `AdmittedSession` and prove revocation,
expiry, and one-path-versus-neighbour scope checks without importing
Graphshell.

### P1. Add the publication read model

**Status:** Complete. Knot has an explicit catalog, eligibility checks,
historical materialization, reader-bound ticket, and non-disclosing absence
behaviour under focused tests.

**Files:** `ports/knot/src/publish.rs` (new), `sync.rs`, `lib.rs`, and focused
unit tests.

Implement the explicit catalog, source eligibility, `KnotShareTicket`, pinned
head check, response materialization, and exact historical operation lookup.
This layer owns no socket, does not issue a Graphshell projection, and never
exposes a vault key or `KnotVault` outside the holder.

**Done when:** an owner can select one current causal document, list and fetch
only that document through correctly scoped authority, fetch a retained prior
operation, and receive no bytes for a deleted, pending, conflicted, unlisted,
or unrelated operation.

### P2. Add the private wire codec

**Status:** Complete for Phase A. The bounded versioned candidate codec remains
an implementation detail rather than a published grammar.

**Files:** `ports/knot/src/publish_wire.rs` (new), `Cargo.toml`, and codec
tests.

Build and test a Phase-A-only candidate codec: start with postcard if it keeps
the prototype small, but do not treat its envelope, error names, or stream
shape as stable. Record successful and refused exchanges as a fixture corpus,
alongside their semantic outcome and limit behavior. Phase B chooses a public
grammar from that evidence.

**Done when:** the candidate exchanges cover list, current fetch, retained
version fetch, non-disclosing absence, malformed input, and limits; all
oversized input fails closed; body digests verify; and the captured corpus is
sufficient to compare a revised candidate without treating the first one as
the protocol.

### P3. Add the Knot carrier and resident host

**Status:** Complete. The host performs the Noise and Notochord admission path,
retains authority through response selection, and observes later revocation.

**Files:** `ports/knot/src/publish_carrier.rs` and
`ports/knot/src/publish_host.rs` (new), `lib.rs`, normal dependencies on
`notochord` and `transport` with its `noise` feature.

Implement the sequence in section 5.5. `KnotPublishHost` owns policy,
publication catalog, revocations, and session accounting; it borrows the
transport through each accept. It is a library host, not a change to
`KnotSyncHost`: personal sync's paired-writer transport and a reader-facing
publication service have different authority and lifetime.

**Done when:** a memory-transport receipt proves no application request reaches
the catalog before admission, and a p2panda/iroh receipt proves the literal
Noise-plus-Notochord path with separate holder and reader identities.

### P4. Compose an explicit holder and reader

**Status:** Complete. `knot_publish_peer` uses distinct holder and reader
identities, issues the holder's ticket out of band, and has both endpoint and
direct-LAN mDNS receipt routes. Turnstone supplies explicit owner Publishing
and recipient Shared Knot panes without treating personal sync as publication.

**Files:** a focused `ports/knot/examples/knot_publish_peer.rs` receipt runner
first; the resident Mere/Turnstone host when product exposure is chosen.

The receipt runner creates distinct holder and reader seeds, has the holder
issue the reader's certificate, exchanges the resulting share ticket, and
reports only safe receipt facts. It must not repeat K2's shared-owner fixture
shortcut. Product composition later exposes explicit publish, unpublish, and
share-recipient controls; it does not infer publication from opening a document
or starting personal sync.

**Done when:** a ticket pasted to a second process is sufficient for the reader
to add the peer, complete the secured session, and fetch the selected source.

### P5. Run the acceptance ladder

**Status:** Complete through the Phase-A stop line. Focused unit/codec and
in-memory Noise/Notochord receipts precede both the Windows-to-ThinkPad
direct-LAN read and its post-revocation refusal, and the 2026-08-19
ThinkPad-to-Q-PC renewal through `fetch_published_document`. Relay routing
remains a later deployment receipt, not a prerequisite for LAN publishing.

Run the following in order and preserve the distinction between them:

1. deterministic unit and codec receipts;
2. p2panda/iroh loopback with actual Noise and Notochord admission;
3. two local processes with a ticket exchanged manually;
4. two physical machines, first on LAN and then through the configured relay
   route if that is part of the intended deployment;
5. a live revocation after one successful fetch, followed by a refused second
   request.

K2's paired in-process Graphshell proof is regression evidence for the shared
carrier pattern. Its physical two-machine runner passed from a Q-PC holder to a
Windows visitor on 2026-08-08. Neither K2 receipt is a Phase A receipt, because
K2 carries a Graphshell projection rather than this raw document protocol and
uses a shared fixture owner secret.

**Done when:** the Phase A done-condition in section 4 has a physical-machine
receipt and every earlier rung is green. Generated ticket and receipt output
stays on disk but out of Git.

## 7. Required Phase A receipts

- A reader with no valid delegation is denied during Notochord admission and
  sees no application response.
- A reader whose valid delegation covers publication `a` cannot list or fetch
  neighbouring publication `a-private` or `b`.
- A malformed ticket, mismatched carrier/Noise identity, wrong encrypted ALPN,
  or malformed request reaches no source adapter.
- `GetCurrent` returns the precise source bytes, `text/vnd.knot` or
  `text/djot`, the current operation digest, and a matching BLAKE3 digest.
- `GetVersion` returns only the exact retained document-producing operation;
  an operation for another document or a deletion returns `NotAvailable`.
- Pending, conflicted, auto-merged, unpublished, and unshared documents return
  no document metadata or bytes.
- An effect-fetcher/evaluator spy records no resolution or evaluation while the
  host serves source. The reader's default effect policy is `Never`.
- A revocation that acquires the ledger's write guard before the response guard
  prevents that response. Once the response guard is held, the bounded response
  is in flight; subsequent requests observe the revocation. Expiry is checked at
  the same response linearization point.
- Two admitted readers are served concurrently, and a third is refused at the
  configured capacity without leaking catalog contents.
- The final two-machine run uses distinct holder and reader identities, rather
  than a shared fixture owner, and exercises both successful fetch and later
  refusal.

## 8. Deliberately out of scope

- **Writes, and the petition path.** Read-only first, as above.
- **Unknown-attribute preservation in djot knots.** A real open tail (§10.5
  Phase 3), but it needs a place on `inker::Block` for arbitrary attributes,
  and `Block::FeedEntry` is also produced by the RSS/Atom engine where djot
  attributes are meaningless. That is a shared-type decision of the same shape
  as the `predicate` sweep (15 sites, 7 crates) and deserves its own note.
- **Retiring pulldown-cmark.** Not a migration leftover. It serves foreign
  markdown and the pinned `nematic.knot` compat engine, which the design doc
  keeps on purpose: "preserves 'paste anything' for import while djot is the
  native author format."
- **Anonymous public hosting.** An open reader policy, abuse and resource
  controls, discovery, and a retention promise are another product decision.
- **Directory-file history.** A mutable file is not made versioned by exposing
  its current bytes twice.
- **Automatic causal text merge as a public version.** It needs an explicit
  durable/public version rule before it becomes addressable.
- **Moot read projections, dictionaries, and wiki templates.** Those remain
  separate from this single-holder, single-reader publishing slice.

## 9. Phase B specification work

**Status:** Not entered. Do not turn the Phase-A candidate frames into a
portable contract until product use stabilizes them and a second implementer
has a concrete reason to consume the resulting vectors.

Once its entry conditions hold, Phase B proceeds in this order:

1. Freeze Phase A packet fixtures, refusal fixtures, and source/version
   receipts as the candidate corpus.
2. Complete the Mark comparison gate in section 4 and record the chosen
   ownership boundary.
3. Write the protocol document before moving implementation into a portable
   crate. It must define transport and ALPN, Noise and Notochord roles,
   addressing and ticket distribution, media types, frame grammar, limits,
   catalog visibility, version identity, digest algorithm, cache rules,
   refusal privacy, revocation semantics, and inert document handling.
4. Extract `knot-protocol` only for the specified codec and conformance
   vectors. `ports/knot` adapts its causal store to it; the crate does not
   absorb the store.
5. Obtain a second implementation against the vectors and correct every
   ambiguity it exposes before promising compatibility.

The published document must distinguish a causal history from a linear
revision series. If an external format needs linear numbers, its adapter owns
that projection and its loss statement; native Knot never lies that its
concurrent facts are a single ordered chain.

## 10. Stop rules

- Do not make a document public because it was opened, synced, rendered, or
  added to a directory.
- Do not hand a reader a paired-writer key, vault key, group epoch, sync store,
  source filesystem path, or raw endpoint authority.
- Do not route remote source through `KnotEndpoint` or Graphshell's projection
  protocol just because those already render it locally.
- Do not depend on Graphshell from the Knot production crate, copy Notochord
  admission/lifecycle code, or hand-build carrier facts.
- Do not use a distinct Noise identity while Notochord's carrier-subject
  binding remains the live rule.
- Do not fetch transclusions, run code, or persist a derived result merely
  because source crossed this protocol.
- Do not report a causal operation digest as a sequential document version.
- Do not start Mark interoperability code, public discovery, write verbs, or a
  second carrier before the physical Phase A read-and-revocation receipt.
