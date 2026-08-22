# Lease-Bound Remote Sessions

**Date**: 2026-08-10

**Status**: Design. Nothing here is implemented; the remote adapter is the last
gate of the [mesh host lanes plan](../implementation_strategy/2026-08-09_mesh_host_lanes_plan.md).
The Burn 0.22.0-pre.2 production migration executed on 2026-08-20, while the
stable repin remains release-gated. Before implementation, this design must be
re-audited against pre.2 and the targeted session-close seam must be exposed
upstream or through a narrow documented patch.

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
in `0.22.0-pre.1`. The real surface is:

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
struct SessionClaim {
    job: JobId,
    lease: LeaseId,
    epoch: u32,
    client: [u8; 32],      // the client's mesh author key
    server_peer: [u8; 32], // who this claim is for
    signature: Vec<u8>,    // by `client`, over the four fields above
}
```

The server's `PeerAuthorizer` closure holds a read handle on its own supervisor
and admits only when **all** of these hold:

1. the signature verifies for `client`, and `client` is in the board's
   [`DeviceDirectory`](../../../crates/mesh/mesh/src/directory.rs) — so the
   claimant is a ring member, not an arbitrary dialler;
2. `server_peer` is this device, and matches `request.peer` via the directory —
   so a claim captured from one device cannot be replayed at another;
3. the board's lease for `job` at epoch `epoch` **is held by this device** and
   is `Held` at the reading of this device's clock; and
4. `request.device_index` is one this device's policy actually offers.

Rule 3 is the whole design: authorization is a *projection of live mesh state*,
not a token that was true once. `job.lease_at(now_ms, &policy)` is already
exactly that query.

Note what this does **not** need: no new key material, no ticket format, no
expiry inside the credential. The lease's own signed window is the expiry, and
it is already replicated.

## 4. The gap: admission is not enough

Rule 3 is evaluated once, when the session opens. After that, Burn Remote has no
reason to ask again — and a session that started legitimately keeps running
through a reclaim.

Reading the source, the machinery to end a session **already exists**:

- `server/service/mod.rs:51` — `fn close(&self, session_id: SessionId)`,
  documented "Drop the session, letting its worker drain and exit."
- `server/session.rs:214` — `SessionManager`'s implementation of it.
- `server/pump.rs:136` — the only caller, on client-initiated teardown.

What is missing is **reach**. `SessionManager` is not exported from
`server/mod.rs` (which publishes only `Channel`, `RemoteServerBuilder`, the
custom-op types, and the iroh protocol types), and `IrohRemoteProtocol` exposes
no way to enumerate or end the sessions it is serving. So an application can
decide *who may start*, and nothing else.

## 5. What to ask upstream for

Deliberately minimal — it exposes existing behaviour rather than adding any:

```rust
impl<B: BackendIr> IrohRemoteProtocol<B> {
    /// Sessions currently served, with the credential each was admitted under.
    pub fn sessions(&self) -> Vec<(SessionId, Arc<[u8]>)>;

    /// End one session. A no-op for an unknown id, mirroring `close`.
    pub async fn close_session(&self, session_id: SessionId);
}
```

With those two, revocation is a loop the supervisor already knows how to drive:
on `WorkerAction::Reclaim`, decode each served credential, close every session
whose `SessionClaim` names the ending lease, *then* author
`LeaseRevokedByOwner` — the same stop-before-you-say-so ordering H0 already
enforces for local runs.

A re-authorization hook (`PeerAuthorizer` consulted periodically) would be
strictly better and is worth mentioning in the issue, but it is a bigger ask and
the two accessors are sufficient.

**If upstream declines**, patch rather than fork: the change is additive, and
the migration plan's rule applies — name the upstream commit, the reason, the
removal condition, and the licence. A Mere-owned Burn Remote is not on the table.

## 6. The interim, and its honest cost

Until targeted close lands, revocation has exactly one blunt instrument: drop
the `IrohRemoteProtocol` handler, which ends **every** session on it.

That is acceptable only under a rule that makes "every" mean "one":

> A device offering remote execution serves **at most one lease at a time**.

`DevicePolicy::max_concurrent_jobs = 1` already expresses it, and
`conservative()` already sets it. The cost is real and should be written into
the adapter's descriptor rather than discovered: a device cannot lend its GPU to
two jobs at once, and reclaiming one would otherwise kill the other's work
without a revoke fact to explain it.

What must **not** be used as the interim:

- **A second endpoint or router per lease.** Closing it would revoke precisely,
  but it means a second identity per lease, which breaks the H1 directory
  (`master_of` maps one author to one endpoint) and contradicts the standing
  rule that Burn mounts on Mere's transport authority.
- **Denying on reconnect only.** A live session is exactly the case that
  matters; a policy that only bites on reconnection is a policy that does not
  bite.

## 7. Receipts this owes

Whenever it is built:

- a session opened under a live lease succeeds, and one naming a lease this
  device does not hold is refused with a reason;
- a claim captured from one server is refused at another (rule 2);
- owner reclaim **ends the session before** `LeaseRevokedByOwner` is authored,
  asserted in that order, as `supervised_reclaim.rs` does for local runs; and
- the client observes the termination as a transport error rather than a hang —
  a revoked worker that blocks forever is not a reclaim.

## 8. Progress

- **2026-08-10**: designed against `burn-remote 0.22.0-pre.1` source. Corrected
  the `RemoteTicket` name both plans carried; confirmed application-owned
  endpoint mounting and an opaque credential; located the existing but
  unreachable `SessionManager::close`; specified the credential as a live-lease
  projection rather than a bearer token; and named the one-lease-per-device
  interim with its cost.
