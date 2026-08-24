# Lease-Bound Remote Sessions

**Date**: 2026-08-10

**Status**: Implemented 2026-08-23 in mere `7fb07225`; the required exact-ALPN
endpoint seam is in p2panda `9f2c2a01`. This is the completed remote gate of the
[mesh host lanes plan](../implementation_strategy/2026-08-09_mesh_host_lanes_plan.md).
Stable Burn 0.22 repinning remains release-gated.

`support/patches/burn-remote` vendors `burn-remote 0.22.0-pre.2` from upstream
Burn `89bcc85f`, with source, unchanged MIT/Apache licensing, and a removal
condition in `MERE-PATCH.md`. The pre.2 re-audit found that exposing the old
manager close alone was insufficient: the duplex pump retained another task
sender. The landed seam instead gives each session a pump-observed close signal
and reserves it before application authorization, so reclaim cannot miss a
session between authorization and worker binding.

**Related**:
[`../testing/2026-08-10_burn_0_21_baseline.md`](../testing/2026-08-10_burn_0_21_baseline.md),
[`2026-06-30_mesh_lease_scheduler_plan.md`](../../archive_docs/2026-08-09_completed_plans/2026-06-30_mesh_lease_scheduler_plan.md)

M3 gave the mesh one non-negotiable rule: a device's owner can take their
hardware back, and the work stops. Burn Remote authorizes a compute session
**once, at admission**. Those two facts do not compose, and this is how to make
them.

Everything below was read from `burn-remote 0.22.0-pre.1` source in the local
registry cache, not from release notes. It is the design baseline, not evidence
for the selected pre.2 row.

---

## 1. Correction: there is no `RemoteTicket`

Both the Burn migration plan and the host lanes plan say Burn Remote offers a
`RemoteTicket` plus an authorization callback. **`RemoteTicket` does not exist**
in `0.22.0-pre.2`. The real surface is:

| what | where |
|---|---|
| `RemoteSecret` | the *server's* stable identity — its address knob, not a grant |
| `PeerAuthorizer` / `AuthorizationRequest` / `AllowAll` | server-side admission policy |
| `RemoteRunner::iroh_authorized(endpoint, peer, device_index, authorization: Vec<u8>)` | client carries an **opaque credential** |
| `IrohRemoteProtocol` | an iroh `ProtocolHandler`, documented as registerable in an existing Router |
| `RemoteNode::from_endpoint(Endpoint)` / `.endpoint()` | mounts on an **application-owned** endpoint |
| `BURN_REMOTE_ALPN` | the ALPN to route |

```rust
pub struct AuthorizationRequest<'a> {
    pub peer: EndpointId,      // authenticated iroh identity
    pub device_index: u32,
    pub credential: &'a [u8],  // opaque, application-defined
}

pub trait PeerAuthorizer: Send + Sync + 'static {
    fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), String>;
}
```

Two of the plan's open questions are therefore already answered **yes**: Burn's
protocol can mount on Mere's existing endpoint (`from_endpoint`, plus the
`ProtocolHandler` registration Mere already does for iroh-blobs), and the
credential is opaque enough to carry anything we like. The remaining question is
the hard one.

## 2. Which side holds the lease

Stating this explicitly because getting it backwards makes the whole design
incoherent.

**The lending device runs the server and holds the lease.** A device offering
its GPU claims a job whose resource is remote execution, grants itself a lease
under the poster's signed envelope, and *then* starts serving. The job's poster
connects as the Burn Remote **client** and drives tensors across.

That orientation is the one that matches M3: the lease holder is the device
whose hardware is being used, "owner reclaim" is that device's own human wanting
it back, and the session is the thing that must stop — exactly as a local run
stops today. Under the other orientation (worker-as-client, GPU device outside
the lease) the GPU device has no lease at all, and reclaim has nothing to act
on.

## 3. The credential is a lease reference, not a bearer token

The client presents a `SessionClaim`, CBOR-encoded into `authorization`:

```rust
struct RemoteSessionClaim {
    version: u8,
    mesh_id: [u8; 32],
    job: JobId,
    lease: LeaseId,
    epoch: u32,
    client: [u8; 32],      // the client's mesh author key
    server_peer: [u8; 32], // server transport identity this claim is for
    device_index: u32,
    signature: Vec<u8>,    // by `client`, over every preceding field
}
```

The server's `PeerAuthorizer` closure holds a read handle on its own supervisor
and admits only when **all** of these hold:

1. the signature verifies for `client`, and `client` is the author who posted
   this exact job, so another ring member cannot drive the poster's lease;
2. the directory maps `client` to `request.peer`, which is Burn's authenticated
   connecting client identity;
3. `server_peer` is this local endpoint, and the directory separately maps this
   server's mesh author to it, so a claim captured from one device cannot be
   replayed at another;
4. the claim's mesh, resource, epoch, lease id, and device index exactly match
   the current board, the host's active run, and a device this server offers;
5. the board projects that lease as `Held` by this server at the reading of the
   server's clock.

Rule 3 is the whole design: authorization is a *projection of live mesh state*,
not a token that was true once. `job.lease_at(now_ms, &policy)` is already
exactly that query.

Note what this does **not** need: no new key material, no ticket format, no
expiry inside the credential. The lease's own signed window is the expiry, and
it is already replicated.

## 4. Why admission was not enough

Rule 3 is evaluated once, when the session opens. After that, Burn Remote has no
reason to ask again — and a session that started legitimately keeps running
through a reclaim.

The pre.2 source did contain a manager close, but deleting the manager entry did
not end the live pump: the pump retained another task sender and kept accepting
work. It also had no application-visible enumeration or targeted control.

The vendored seam therefore closes at the pump boundary. A reserved or active
session carries a close watch the pump selects against, and its opaque
credential remains visible to the host. Reserving before the synchronous
authorizer runs closes the admission race: every possibly admitted session is
visible to `close_session`, including one whose worker has not been bound yet.

## 5. The landed narrow patch

```rust
impl<B: BackendIr> IrohRemoteProtocol<B> {
    pub async fn sessions(&self) -> Vec<ServedSession>;

    pub async fn close_session(&self, session_id: SessionId) -> bool;
}
```

With those two, revocation is a loop the supervisor already knows how to drive:
on `WorkerAction::Reclaim`, decode each served credential, close every session
whose `RemoteSessionClaim` names the ending lease, *then* author
`LeaseRevokedByOwner` — the same stop-before-you-say-so ordering H0 already
enforces for local runs.

A periodic re-authorization hook would still be useful upstream for policies
that can change without a host cancellation event. Mere does not need it for
owner reclaim because the supervisor's cancellation path has exact session
control.

The patch remains additive and documented; a Mere-owned Burn Remote is still
not on the table. Remove it when an upstream release exposes equivalent
reserved-session enumeration and pump-observed targeted close.

## 6. The retired interim

The one-remote-lease-per-device fallback is no longer required for revocation
correctness. The adapter keys active runs and served sessions by exact
`(JobId, LeaseId)`, and reclaim closes only matching credentials. Owner policy
may still choose `DevicePolicy::max_concurrent_jobs = 1` for memory or thermal
reasons.

The rejected alternatives remain rejected:

- **A second endpoint or router per lease.** Closing it would revoke precisely,
  but it means a second identity per lease, which breaks the H1 directory
  (`master_of` maps one author to one endpoint) and contradicts the standing
  rule that Burn mounts on Mere's transport authority.
- **Denying on reconnect only.** A live session is exactly the case that
  matters; a policy that only bites on reconnection is a policy that does not
  bite.

## 7. Receipts

- pure claim tests cover wrong mesh, wrong server, wrong client transport,
  non-poster clients, wrong device/resource, and stale lease identity;
- a real Burn tensor round trip succeeds over the same application-owned
  p2panda/Iroh endpoint;
- the vendored Burn test closes a session while its authorizer is still
  in-flight, proving the reservation fence;
- owner reclaim first yields `AwaitingStop`, closes the session, and only a
  later tick authors `LeaseRevokedByOwner`; and
- the reclaimed client receives an error within the bounded receipt rather
  than hanging; and
- a clean two-peer MiniLM run executes ESP numerically on native WGPU, reclaim
  interrupts a live 512-row request and closes the only session, and a fresh
  epoch/session reproduces all 384 values exactly.

## 8. Progress

- **2026-08-10**: designed against `burn-remote 0.22.0-pre.1` source. Corrected
  the `RemoteTicket` name both plans carried; confirmed application-owned
  endpoint mounting and an opaque credential; located the existing but
  unreachable `SessionManager::close`; specified the credential as a live-lease
  projection rather than a bearer token; and named the one-lease-per-device
  interim with its cost.
- **2026-08-23**: re-audited against pre.2 and implemented. Corrected two
  assumptions from the design: `AuthorizationRequest.peer` is the connecting
  client, not the server, and the old manager close did not terminate the pump.
  Added the reservation fence, exact raw-ALPN admission in p2panda, a
  transport-neutral signed claim in `mere-mesh`, host-authored `RunContext`, and
  the Distillery resource/service. Mere `7fb07225`; p2panda `9f2c2a01`.
- **2026-08-23, forcing consumer**: clean Mere `176c31e8` drove the pinned
  MiniLM through the adapter over two distinct p2panda peers. Maximum error was
  `1.4901161e-7` against ESP NdArray and `1.4156103e-7` against the browser
  reference prefix. Reclaim interrupted active model work, closed the session
  to zero, and a new lease recovered identical output. The passing server is
  plain native WGPU. Burn Fusion panicked in fused-matmul autotune/ordering and
  remains a backend sidequest; physical GPU allocation release remains
  unmeasured.
