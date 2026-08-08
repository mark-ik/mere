# Knot Publishing Protocol Plan

**Date**: 2026-08-07
**Status**: Scoped, not started. Direction settled 2026-08-07: **A then B**
(§4). Nothing is implemented; this is the plan, not a queue item in progress.

**Scope**: Give knot documents a documented way to be fetched by *someone who
is not you*. Everything knot does today is intra-persona.

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

Three facts, each verified in the tree rather than assumed:

- `knot://vault/{id}` is already knot's addressing scheme (`endpoint.rs`), but
  it resolves only **locally**. There is no remote form.
- `knot_endpoint` is a **local projection endpoint**: it opens a directory and
  serves a projection to a host that mounted it. It speaks no network protocol.
  (An earlier reading of this file mistook its HTTP *test fixtures* for its
  serving; they are fixtures.)
- `knot_sync_host` replicates a vault over p2panda/iroh, but **only across one
  persona's own paired devices**. Admission is by paired writer key.

So: a persona can reach their own knot documents from any of their own
machines, and nobody else can reach them at all. Publishing a note to another
person requires leaving knot entirely.

## 3. What this should not invent

The pattern already exists one layer up, committed days ago as Graphshell's
K2 (`ports/graphshell/src/native/projection_host.rs`, and
`carrier::accept_projection_session`). It is:

    transport.accept(alpn)
      -> read the acceptance facts (never application bytes)
      -> admit_session(policy, revocation ledger, facts)
      -> check the principal actually serves this action
      -> spawn, and return to accepting

That is admission, revocation, concurrency, and refusal-with-a-reason, already
built and already reasoned about. **Knot should reuse this seam rather than
grow a second one.** A second admission path is how two divergent policies
happen.

Likewise settled and not to be re-litigated:

- **Transport is iroh.** "iroh is the byte plane. Nothing below moves blobs off
  iroh" (reachability plan). Not raw QUIC, not a new listener.
- **Confidentiality and layered identity are Noise**, composed *over* the iroh
  stream, as landed 2026-08-06. That is also where a capability distinct from
  the device identity naturally lives — the Noise identity is a parameter, not
  the carrier's key.
- **The document format is djot.** §10.5 of the polyglot knot design finished
  on 2026-05-31; `text/x-knot` routes to `nematic.knot-djot` by default.

## 4. The direction: A, then B

**How much of a protocol does this want to be?** Both, in that order. A is not
a trial of whether B is worth doing — B is where this is going. A is how B gets
written from something that has carried traffic rather than from a guess.

Stating that plainly matters for a reason this workspace has been bitten by
before: a phase recorded as "an option we might take" reads, months later, as a
thing that was considered and dropped. This is a sequence.

### Phase A — a knot ALPN on the existing projection seam

Knot's resident accepts on its own ALPN, admits through
`accept_projection_session`, and answers a small request vocabulary: fetch a
document by id, list what is published, fetch a version. No new scheme, no new
crate. `knot://vault/{id}` gains a remote resolution through an existing
carrier.

Smallest, reuses everything, and needs no specification written in advance. It
is deliberately *not interoperable with anything yet* — a private protocol
between Mere instances, which is exactly the right shape for something whose
wire grammar is still learning what it needs to carry.

Useful on its own terms, not merely as groundwork: it closes the publishing gap
for the users who exist today, which is Mere instances talking to each other.

**Done when:** two personas on two machines, not paired to each other, and one
fetches a djot note the other published — with the refusal path exercised too,
because an admission seam that has never refused anything has not been tested.

### Phase B — write the specification, publish `knot-protocol`

The same wire behaviour, written down to the standard the smolweb crates hold,
published, and implementable by someone who is not us. This is what makes it a
protocol rather than an internal RPC, and it is the destination.

The cost is real and should not be understated. A published protocol is a
compatibility promise, and this workspace's rule has been that a crate ships
only where a specification exists to be faithful to. Here **we are the ones
writing that specification**, which is a different and larger undertaking than
implementing someone else's — every ambiguity we leave is one an implementer
has to guess at, and this session has spent a good deal of effort refusing to
guess at other people's.

What Phase A must hand over for Phase B to be writable: the request grammar as
it actually settled, the refusal vocabulary as it actually got used, and the
versioning semantics as codicil actually exposed them. A specification written
before those are known describes something nobody wanted.

**Enter Phase B when:** Phase A's grammar has stopped changing under real use,
and there is a second party who wants to implement it. The second condition is
not decoration — a specification with no prospective implementer is
documentation wearing a costume.

### Not a phase: Demarkus's Mark Protocol (interoperability)

Kept here so it is not re-researched from scratch. Implementing the Mark
Protocol for interop with Demarkus is **blocked, and separate from A and B**:
demarkus.io publishes a feature list and a GitHub link, not a wire
specification. Reading their repository to establish whether a real spec exists
is the prerequisite; until one does, this falls under the rule that ruled out
Mercury — no guessing a wire format.

Note the relationship, which is not competition: if their spec turns out to be
real and good, Phase B could adopt it instead of authoring one. That would be a
better outcome, not a worse one. It just cannot be decided from a landing page.

## 5. Sketch of Phase A, for costing only

Not a design; a size estimate. The design is written when the work starts.

- A knot ALPN constant, beside the projection one.
- A request grammar. Small: `GET <document-id>`, `LIST`, `GET <id>@<version>`.
  This is ours to define — both ends are this code — the same licence the Noise
  framing took, and the same limit: it is not licence to guess anyone else's.
- A `KnotPublishHost` shaped like `projection_host.rs`: accept, admit, spawn,
  return to accepting. Sessions served concurrently, because a shared document
  with one reader is not the case worth building for.
- Read-only first. Writes need the participant gate's petition path and a
  conflict story that codicil has but knot has not surfaced; Demarkus's own
  posture ("read-only by default, no writes without explicit auth tokens") is
  the right default here too, arrived at independently.
- Serving reuses `KnotEndpoint`'s existing projection, so a remote reader gets
  exactly what a local one does.

The done-condition is stated with Phase A in §4.

## 6. Deliberately out of scope

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
