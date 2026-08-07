# Notochord Session and Policy Spine Plan

**Date:** 2026-07-26
**Status:** Complete. N0-N4 landed on 2026-07-27.
**Decision:** grow the former `network-policy` crate into the common
session-admission spine, then promote it to `notochord` after two real service
carriers consume the shared context.
**Refines:** the
[low-power radio and managed-network plan](../../mere_docs/implementation_strategy/2026-07-24_low_power_managed_network_plan.md)
and Graphshell's G5 admission lane in the
[remote projection host plan](../../mere_docs/implementation_strategy/2026-07-22_graphshell_remote_projection_host_plan.md).

## Current evidence

The substrate is real:

- Retinue preserves the accepted interface and link.
- `mere-transport::AcceptedSession` preserves protocol, an authenticated peer
  when the transport can prove one, and local ingress context.
- `notochord` has a deterministic local evaluator, Personae chain
  validation, revocation, bounded handshake framing, transcript proofs, and
  `AdmittedPrincipal`.
- one `mere-transport` adapter turns accepted carrier state into
  `SessionFacts`;
- Memory and Reticulum/TCP run the handshake over real transport
  implementations;
- Murm and Graphshell consume the same `AdmittedSession`, with Graphshell's
  carrier proved over real p2panda/Iroh;
- admitted sessions retain their verified claims, so expiry and later
  revocation use the authority that actually opened the connection;
- the host persists versioned owner policy and verified revocations without
  persisting carrier facts, principals, streams, or live session counts.

That is what Notochord now names. The package and path promotion followed the
N2 two-consumer receipt.

## The boundary

Notochord answers one question:

> Given facts observed by the carrier, claims proved by this handshake, and
> the owner's local rule, what principal and limits may this service accept?

It does not decide domain operations after admission. It does not become a
global network mode.

```text
Retinue / p2panda / Memory
          |
          v
mere-transport AcceptedSession
          |
          v
Notochord SessionFacts + ProofBinding
          |
          +--> owner LocalNetworkPolicy
          +--> Personae delegation + revocation
          +--> bounded session proof
          |
          v
AdmittedSession<Stream>
          |
          +--> Murm authorization and protocol
          +--> Graphshell projection authorization
          +--> future service-specific authorization
```

## Separate facts, claims, and conclusions

Before N0, `SessionRequest` contained `transport_peer`, even though callers
were instructed to populate it only from transport truth. The rule was sound;
the type made it easy to violate. Notochord now separates the channels.

```rust
pub struct SessionFacts {
    pub protocol: Vec<u8>,
    pub transport: CarrierKind,
    pub authenticated_initiator: Option<[u8; 32]>,
    pub ingress: IngressFacts,
}

pub struct IngressFacts {
    pub local_interface: Option<u64>,
    pub shared_link: Option<[u8; 16]>,
}

pub struct SessionClaims {
    pub wire_version: u16,
    pub network: NetworkId,
    pub profile: ProfileRef,
    pub action: RequestedAction,
    pub class: TrafficClass,
    pub subject: SubjectId,
    pub delegations: Vec<SignedDelegationCertificate>,
}

pub struct ProofBinding {
    pub protocol: Vec<u8>,
    pub initiator_transport_identity: Option<[u8; 32]>,
    pub shared_link: Option<[u8; 16]>,
}

pub struct AdmittedSession<S> {
    pub stream: S,
    pub principal: AdmittedPrincipal,
    pub facts: SessionFacts,
    pub claims: SessionClaims,
    pub limits: HandshakeLimits,
}
```

`SessionFacts` and `AdmittedSession` are local conclusions and are not
serializable. `SessionClaims` is decoded from bounded application bytes.
`ProofBinding` contains only facts both peers can derive independently and
sign into the transcript. For an authenticated transport, the initiator
derives `initiator_transport_identity` from its own transport identity and the
responder derives the same value from the authenticated remote peer. It must
not mean "the remote endpoint" on both sides. A local interface id remains
available to the owner's policy, but never goes on the wire and never enters a
transcript the remote peer cannot reconstruct.

The policy API becomes:

```rust
policy.evaluate(&facts, &claims, &revocations, now, active_sessions)
```

The caller cannot place an application claim in
`facts.authenticated_initiator`. The accepting carrier adapter constructs that
field.

## Ownership

- **Retinue** owns interface, link, packet class, transit scheduling, and
  forwarding policy.
- **mere-transport** owns carrier setup and `AcceptedSession<S>`. An optional
  Notochord integration converts its own acceptance record into
  `SessionFacts` once.
- **Notochord** owns session facts vocabulary, transcript binding, owner
  service rules, Personae-chain evaluation, revocation, handshake limits, and
  the admitted conclusion.
- **Personae** remains the delegation and signing grammar.
- **Gemot** remains the authority for Moot membership and constitution.
- **Murm, Graphshell, Mesh, and other services** authorize operations after
  admission under their own action vocabulary.
- **The host** persists owner-configurable rules and capacity settings.

Discovery, service admission, transit, replication, compute, path choice, and
capacity commitments remain independent settings.

## Packaging

`network-policy` remained the package name through N0-N2 so the work was
judged on use rather than naming. N3 promoted it after the two-consumer gate.

The promoted target is:

```text
crates/system/notochord
  facts.rs
  policy.rs
  owner.rs
  chain.rs
  handshake.rs
  io.rs
  types.rs
  lib.rs
```

The base crate remains sans-I/O. Its `tokio` feature continues to supply only
bounded framing. It never depends on `mere-transport`, iroh, p2panda, or
Retinue.

`mere-transport` may expose an optional `notochord` integration feature that
depends in the other direction and owns the conversion from
`AcceptedSession`. This keeps the policy core carrier-neutral and prevents a
dependency cycle.

## Build order

### N0. Split facts from claims

**Files:**

- `crates/system/network-policy/src/{types,handshake,policy,lib}.rs`
- new `crates/system/network-policy/src/facts.rs`
- existing matrix and handshake tests

Introduce `SessionFacts`, `IngressFacts`, and `ProofBinding`. Remove transport
observations from the serialized request shape. Make evaluation accept facts
and claims separately. Derive the responder's transcript binding from accepted
facts. Give the initiator a distinct constructor that uses its transport's
local identity, so the two roles independently derive the same initiator
identity.

Keep a private compatibility constructor only long enough to move all
in-workspace callers in N1. Do not publish both request shapes.

**Receipts:**

- a decoded hello cannot contain an authenticated peer or local interface;
- a claimed subject cannot overwrite a carrier-authenticated peer;
- Reticulum may admit a proved subject with `authenticated_peer: None`;
- replay on a different shared link fails;
- the local interface may affect local policy without changing transcript
  bytes;
- the existing V5/V6 matrix remains green.

### N1. One transport adapter

**Files:**

- `crates/murm/transport/Cargo.toml`
- `crates/murm/transport/src/{accepted,lib}.rs`
- new focused integration module behind an optional feature
- `crates/murm/transport/tests/session_policy.rs`

Add one inbound conversion owned by the carrier crate:

```rust
impl<S> AcceptedSession<S> {
    pub fn into_notochord(self) -> (S, SessionFacts);
}
```

The exact method name may change. There must be one audited construction site,
not a five-line copy in every service. Initiating streams receive a
role-specific proof-binding constructor from their local transport identity,
protocol, and shared link. They do not pretend the responder is the same peer
the responder authenticated.

Delete `binding_for` and equivalent hand-built adapters. Keep interface ids
local and opaque. Preserve p2panda's authenticated peer and Reticulum's honest
`None`.

**Receipt:** Memory, p2panda, and Reticulum fixtures show that the adapter
reports only carrier-observed facts. A forged application frame cannot alter
them.

### N2. Two real service carriers — PASSED 2026-07-27

Wire the shared context through:

1. Murm's real accept path, before any Murm application bytes; and
2. Graphshell G5d over `P2pandaTransport`, before `SessionOpen`.

Both paths return or retain `AdmittedSession<S>`. Each domain then applies its
own authorization:

- a Murm grant cannot open Graphshell;
- a Graphshell projection grant cannot open Murm;
- a valid principal denied by the owner's local service rule receives no
  application bytes;
- disconnect, expiry, and revocation update the service's session lifecycle
  without manufacturing a new identity.

Run the existing Memory and Reticulum/TCP arms, add the p2panda arm, and keep
the direct-PHY/RF proof under the low-power plan's bench gate.

**Promotion gate:** Notochord is earned only when both service paths use the
same facts, handshake, and admitted conclusion with their distinct action
triples.

### N3. Name and path cutover — COMPLETE 2026-07-27

After N2:

- move `crates/system/network-policy` to `crates/system/notochord`;
- rename the Cargo package and Rust library to `notochord`;
- update the workspace dependency, Murm transport integration, Graphshell
  port, docs, and lockfile;
- keep wire-domain separator constants versioned and stable unless a deliberate
  protocol-version migration is included;
- remove `network-policy` and `network_policy` from current build references.

Before deleting the old package name, check for a published clean-checkout
consumer. Use one deprecated forwarding release only if that evidence exists.
Historical receipts retain the old name with a promotion note.

**Receipt:**

```text
cargo test -p notochord
cargo test -p mere-transport
cargo test -p mere-transport --features reticulum,notochord
cargo test -p murm
cargo test -p graphshell
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
```

Use the actual feature and package spellings fixed by N1. A scoped search finds
no current build reference to the old name.

### N4. Owner-facing policy projection — COMPLETE 2026-07-27

Expose the independent settings through the host without flattening them into
one public/private switch:

- accepted network and profile revisions;
- per-service access, admitted domain/actions, and transport-identity
  requirements;
- per-service session capacity and per-network handshake ceilings;
- revocation state;
- discovery and Retinue transit as separate settings owned by their existing
  subsystems.

Serialize the owner's policy, not `SessionFacts` or `AdmittedPrincipal`.

**Receipt:** a headed scenario changes Murm admission without changing
Graphshell or transit; a transit change does not expose a service; restart
restores the same owner rules without restoring stale live conclusions.

## Stop rules

- Stop if application bytes can populate an authenticated-peer or ingress
  field.
- Stop if a local-only interface id enters a signed transcript.
- Stop if Notochord duplicates Personae delegation, Gemot membership, or a
  domain's operation authorization.
- Stop if the package gains a mandatory transport dependency.
- Stop if an owner setting becomes a global public/private or client/router
  mode.
- Do not rename before the N2 two-consumer receipt.
- Keep RF, power, range, and multi-hop claims under their headed evidence
  gates.

## Done condition

`notochord` is the sole session-admission package; carrier facts, wire claims,
proof binding, local policy, and admitted conclusion are separate types; one
transport adapter constructs the facts; real Murm and Graphshell carriers use
the same spine with distinct action vocabularies; domain authority stays in
the domains; the old package and hand-built bindings are gone; and owner
settings remain independent and configurable.

## Progress

### 2026-07-26 — N0 landed (facts split from claims)

`crates/system/network-policy/src/facts.rs` introduced `SessionFacts`,
`IngressFacts`, `CarrierKind`, and `ProofBinding`. `SessionRequest` is gone,
replaced by `SessionClaims`; `evaluate` now takes facts and claims as separate
arguments, so a caller cannot place an application claim where a carrier fact
belongs. `respond` and `admit` take `SessionFacts` and derive the responder's
binding internally via `facts.proof_binding()`, so there is no path by which a
frame supplies its own binding.

Receipts, all tested: a decoded hello cannot carry an authenticated peer or a
local interface (neither field exists on `SessionClaims`); a claimed subject
cannot overwrite a carrier-authenticated peer; Reticulum admits a proved
subject with `authenticated_initiator: None`; replay on a different shared link
fails; a differing local interface does not change transcript bytes; and the
V5/V6 matrix stayed green. At this point the package still had its historical
name: 35 `network-policy` tests, 18 Graphshell tests, both transport arms.

**Two deviations from the plan text, both deliberate:**

1. The carrier enum is `CarrierKind`, not `TransportKind`. This crate cannot
   depend on `mere-transport`, so the name would otherwise collide with
   `mere-transport::TransportKind` in exactly the adapter that has to import
   both. The mapping is one match arm.
2. `ProofBinding::initiator(...)` is the role-specific constructor the plan
   asks for; the responder's comes from `SessionFacts::proof_binding()`. The
   asymmetry the plan warns about is enforced by having only these two ways to
   build one, and a unit test asserts both roles derive identical bytes.

### 2026-07-26 — N1 landed (one audited adapter)

`crates/murm/transport/src/session_policy.rs`, then behind an optional
`session-policy` feature, introduced the only conversion from an acceptance record
into carrier facts:

```rust
impl<S> AcceptedSession<S> {
    pub fn session_facts(&self) -> SessionFacts;
    pub fn into_session(self) -> (S, SessionFacts);
}
```

plus the two role-specific initiator constructors, `initiator_binding` (a
carrier that authenticates peers, from the node's **own** identity) and
`initiator_link_binding` (a link-oriented carrier that cannot, so the shared
link carries the weight). The hand-built `facts_for` in
`tests/session_policy.rs` was deleted; both arms exercised the real adapter.

The dependency ran mere-transport -> network-policy, as the plan required, so
the policy core still knows nothing about iroh, p2panda, or retinue and the
default `mere-transport` build does not pull it in at all.

**Receipt.** Seven unit tests over Memory, p2panda, and Reticulum fixtures:
p2panda and Memory carry their authenticated initiator, Reticulum keeps its
honest `None` and its bearer detail, the local interface is carried but two
sessions differing only in interface produce identical bindings, both roles
derive the same binding, a different link is a different binding, and
`into_session` returns the same facts as the borrowing accessor. A forged
application frame cannot alter any of it structurally: facts are built from
the acceptance record before a single application byte is read, and
`SessionFacts` has no deserializer.

**Deviation:** the method is `session_facts()` / `into_session()` rather than
`into_notochord()`, since the crate was still `network-policy` through N2 and
naming the method for an unearned promotion would be the same mistake the plan
warned about. The feature was `session-policy` for the same reason. N3 renamed
the module and feature to `notochord`; the domain-neutral method names stayed.

Remaining for N2: Murm's real accept path, and Graphshell G5d over
`P2pandaTransport`.

### 2026-07-27 — `AdmittedSession<S>` and `admit_session`, ahead of the N2 split

Pulled forward out of N2 at the servitor's request, and it was the right call:
both N2 halves have to return or retain this type, and two services defining
it against their own carriers would rebuild the duplication N1 just removed,
one level up. It is carrier-neutral, so it lives here.

`AdmittedSession<S>` carries the stream, the `AdmittedPrincipal`, the
`SessionFacts` the carrier observed, and the bounds the responder holds the
session to. `network_policy::admit_session` was the io-level shape both halves
share: it takes the stream, runs the bounded handshake, and returns either an
`AdmittedSession` or the `DenyReason`. A refusal **consumes** the stream, so
there is no way to hand a refused one to an application by accident.

Deviation: the plan sketches `limits: SessionLimits`. No such type exists and
inventing an empty one would be a shape with no content, so the field is
`HandshakeLimits` today, documented to move when session-level limits (rate,
byte budget, duration) become real.

### Refusals are finished, not merely flushed

Both carriers happen to deliver a flushed-then-dropped refusal: a retinue
relay reads its duplex to EOF before closing the link, and quinn's
`SendStream::drop` calls `finish` (verified in the servitor's read of quinn
0.11.11 `send_stream.rs`, correcting an earlier guess that it reset). Relying
on that is relying on two unrelated `Drop` impls staying correct.

It is also thinner than it looks, and in the same place on both arms: quinn's
`Drop` bails out early when the connection already has an error, which is
exactly what dropping the endpoint underneath it produces, and retinue's relay
is aborted by endpoint teardown for the same reason. **The rule holds on both
arms by different mechanisms: a short-lived accept task must not own the
carrier, and the carrier must outlive its sessions.**

`accept_session` and `admit_session` now call `poll_shutdown` on a refusal.
Worth noting what the servitor found while checking: nothing in the tree called
`poll_shutdown` anywhere before this. It is implemented on both stream types
and had zero callers, so every writer flushed and dropped and got away with it.
That was an accident, not a choice.

### 2026-07-27 — N2, Murm half: the session lane

**A finding first, because it changes what N2 asked for.** Murm had no accept
path to wire admission into. Its peer runtime is topic-shaped — posts arrive on
a gossip overlay, gaps reconcile by LogSync — and its `accept` is the
accepted-*operation* path, not session acceptance. `/services/murm` existed
only as an example string in this crate's own docs and tests. Nothing in the
tree called `Transport::accept` in production at all.

So the Murm half was not wiring; it was new surface, and Mark ruled on which
surface. What justifies it is not the promotion gate: it is that **V7 wants
this service over Reticulum and direct PHY, and a radio bearer has no gossip
overlay to ride.** A Reticulum link is two peers and a stream. Without a
session lane a cabal cannot move out there at all, so the lane is what Murm
needs for the radio arm regardless of Notochord.

`crates/murm/murm/src/session_lane.rs`, behind an optional `session-lane`
feature, is that lane. `serve_session` admits an inbound session through
`admit_session` and then ingests posts via the same
`ConversationEngine::ingest_post` the gossip lane uses — same verification,
same idempotence, so a post arriving on both lands once. `push_posts` is the
initiator half. `SessionOutcome` reports the admitted principal beside the
counts, so traffic is attributed to the subject the proof established rather
than to whatever the frames claim.

**Receipt (the low-power plan's V6 done-condition, finally literal).** An
admitted member's post reaches the conversation and is attributed to its
proved subject; a peer refused by the owner's rule leaves the cabal holding
**zero** posts. That is "an owner rule admits or rejects one real Murm
connection before Murm receives application bytes", against a real engine
rather than a fixture.

**One half of the cross-service check is closed too**, and it needed nothing
from the Graphshell side: a grant for `mere.graphshell` /
`/services/projection` — valid, signed by a trusted root, issued to the very
peer connecting — is refused by Murm's lane with `ActionNotCovered`, and
nothing it sent reaches the conversation. The triple is spelled out in murm's
test rather than imported, since murm must not depend on a port. G5c already
proved the mirror (a Murm grant does not open projections), so both directions
of "one service's grant is not authority in another" now hold.

What remained at this point was Graphshell G5d over `P2pandaTransport`
(the servitor's lane). Both carriers had to be on the same construction site,
which since N1 means `AcceptedSession::session_facts` and `admit_session`;
Murm's lane already was.

### 2026-07-27 — N2, Graphshell half: the projection carrier

`ports/graphshell/src/carrier.rs` (`115904b5`). `accept_projection_session`
runs the accept path before a `SessionOpen` byte is read, on the same
construction site the Murm lane uses: `AcceptedSession::into_session` for the
facts, `network_policy::admit_session` for the framing and the conclusion. It
hand-builds no `SessionFacts` and owns no framing of its own.

The transport is borrowed, never owned, which is this lane's independent
arrival at the rule the Reticulum arm found. On p2panda the mechanism is
different: quinn's `Drop for SendStream` finishes the stream rather than
resetting it, *except* when the connection is already errored, which is what
dropping the endpoint underneath it produces. Two carriers, two unrelated
drain mechanisms, one shared escape hatch. The rule generalizes; the
mechanism does not.

**The finding, which the policy layer cannot fix from where it stands.**
`LocalNetworkPolicy` keys service rules by *path* and checks that the chain
covers the requested triple. `ServiceRule` has no action vocabulary at all, so
a chain covering a *different* action at `/services/projection` clears
admission with that action intact. `DenyReason::ActionNotOffered` exists in
this crate with **no decision site**, which is the tell. So each service must
judge the admitted action against what it serves, which is not admission
duplicated but exactly the ownership this plan assigns: services authorize
operations after admission under their own vocabulary. Proved rather than
argued: an admitted `administer` grant at the projection path is refused as
`ActionNotServed` (graphshell 20).

This was closed in the final consolidation: `ServiceRule` now persists its
admission domain and action allow-list, and `LocalNetworkPolicy::evaluate`
returns `ActionNotOffered` before accepting an unoffered triple. Graphshell
still checks its compiled service vocabulary after admission, so a permissive
owner document cannot make the implementation serve an action it does not
support.

The first carrier receipts in `115904b5` exercised the generic accept path
through `MemoryTransport`. That proved the shared construction site, but it
did not meet N2's literal `P2pandaTransport` requirement. The follow-up adds
two real p2panda-net/Iroh receipts:

- an authenticated viewer is admitted, its p2panda peer is retained in
  `AdmittedSession::facts`, and application bytes cross only after admission;
- a valid Murm grant belonging to that same authenticated peer is refused with
  `ActionNotCovered`, and zero projection bytes cross.

Together with Murm's listener refusing a Graphshell projection grant while
leaving its conversation empty, both cross-service directions now hold over
the real service paths. **The N2 promotion gate is met.** N3 is unblocked, but
the name and path cutover remains a separate slice. The real refusal also
exposed a p2panda stream-lifetime race: finishing a small final frame and then
dropping the stream could close its last QUIC connection handle before the
peer received the refusal. `P2pandaStream::poll_shutdown` now keeps that handle
alive until QUIC acknowledges every final byte, with a transport-level
regression receipt. Verification: Graphshell 35; mere-transport 40 plus its
`session-policy` integration test.

### A transport bug this uncovered

The Reticulum arm of `session_policy.rs` was intermittently timing out with
the responder having decided `accept` and written its reply. The cause is not
in the policy layer.

The first diagnosis here was wrong and is corrected: dropping a retinue
`LinkStream` is **safe**. Its outbound relay reads the duplex to EOF, so
buffered bytes are drained and sent before the link closes. Verified by
dropping the stream immediately while keeping the transport alive: everything
written still arrives.

The actual hazard is one level up. **Dropping a `ReticulumTransport` tears
down its endpoint and aborts the relay tasks it is tracking, discarding
outbound bytes that `flush()` already reported as written.** In the failing
version the responder task owned `server`, so returning dropped the endpoint
the instant the reply had been handed to the relay.

The consequence for real services: a short-lived accept task must not own the
transport, and an endpoint being shut down needs a graceful path that drains
in-flight relays before aborting them. Retinue has no such path today; that is
the fix worth making, and it is filed here because N2 wires real services onto
exactly this seam.

Note for whoever writes the next transport test: bound every await. An
unbounded `read_to_end` on a link that never signals EOF is indistinguishable
from a slow build, and cost this lane a night.

### 2026-07-27 — N2 lifecycle closure

The carrier conclusion originally retained the principal and facts but dropped
the verified claims. That left carrier-admitted Graphshell sessions unable to
notice a later revocation because the delegation chain had been consumed with
the hello. `AdmittedSession<S>` now retains `SessionClaims` after the accepted
bounded frame is decoded. `SessionAuthority::retain_admitted` derives the live
deadline and delegation ids from that exact conclusion, and the lifecycle
receipt proves a later ledger update lapses the session as revoked.

### 2026-07-27 — N3 landed (package promotion)

The package and directory are now `notochord`; the workspace dependency,
Murm, mere-transport, Graphshell, current docs, features, modules, and tests use
that name. A scoped sibling-repository search found no clean-checkout build
consumer of the old package, so no forwarding release was justified.

The two existing transcript-domain byte strings deliberately retain
`mere/network-policy/.../v1`. They are wire compatibility constants, not
package identifiers, and changing them would invalidate existing proofs.
Historical progress entries above retain the old package spelling to keep the
sequence honest.

### 2026-07-27 — N4 landed (owner policy and headed projection)

`OwnerPolicySet` and `OwnerNetworkPolicy` contain only owner choices and
verified revocations. Service rules now name the domain and admission actions
the owner offers, which gives `ActionNotOffered` its missing decision site.
That schema change founded owner-policy document v2 rather than silently
reusing v1. Handshake limits clamp at the edit boundary. Deserialization
rejects unknown document or network-policy versions, duplicate networks, and
nondeterministic network order.

Session-runtime persists the document at
`personas/<persona>/settings/notochord.json` using replacement saves. Its
restart receipts prove the rules return while carrier facts, admitted
principals, streams, session ids, and live counts have no serialized field.

Graphshell projects the same model into plain owner-facing rows. The committed
headed receipt changes Murm without changing Graphshell or transit, changes
transit without changing either service, and reloads the same choices from
disk:

[`ports/graphshell/docs/receipts/n4_owner_policy.html`](../../../ports/graphshell/docs/receipts/n4_owner_policy.html)

Final focused verification on the completed shape:

- Notochord: 42 tests with async framing enabled;
- Graphshell: 38 tests, including real p2panda admission, retained-authority
  revocation, policy-axis independence, and byte-exact receipt regeneration;
- session-runtime: 3 focused owner-store tests;
- mere-transport: Memory and Reticulum/TCP Notochord arms;
- Murm: 4 focused session-lane tests, including unoffered-action refusal
  before the engine receives a post;
- warning-denying Clippy for Notochord and Graphshell's own targets;
- `cargo check --workspace --all-targets`;
- targeted rustfmt checks and `git diff --check`.

The repository-wide `cargo fmt --all -- --check` remains red on pre-existing
formatting drift outside this slice. Every Rust file touched here passes
`rustfmt --check`. The browser harness refused local `file:` navigation, so
desktop and narrow-width visual inspection of the generated HTML remains
unclaimed and is recorded in the receipt.
