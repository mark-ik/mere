# Persona Transport Unlinkability — Plan + Research Brief

**Status (2026-06-25):** design. Mode 1 + the relay-diversity half-measure are the
near-term buildable plan; the own-device-cluster family is the core of the privacy
story and is mostly a design gap, not an implementation gap; Nym and the metadata
ceiling are a named research track. No code yet.

This doc owns the persona-transport privacy model: how a persona's network presence is
(or is not) linkable to you and to your other personas, what modes we offer, and how a
persona's transport choice couples to its tessera standing. It sits between the identity
crate (`crates/persona/identity`), the transport (`crates/murm/transport`), and the
standing layer (`crates/moot/moothold/tessera`).

## The axis: anonymity is the opposite of reputation

The frame that ties everything together: **reputation is the cost of continuity;
anonymity is the cost of starting over.** Standing accrues at a persona chain root
([`persona_chain.rs`](../../../crates/moot/moothold/src/tessera/persona_chain.rs)); a
face that carries standing is, by carrying it, more linkable. A fully unlinkable face is
a fresh zero-standing chain. So a persona's transport mode and its standing tier are one
slider, not two:

| Persona | Standing | Transport profile |
| --- | --- | --- |
| **Main** | master root, full undepreciated standing | master endpoint, public node, fully linkable (it has to be, to carry reputation) |
| **Forked** | child chain, depreciated standing | relay diversity or a shared cluster node — less anon than a burner, more private than main |
| **Burner** | fresh chain, zero standing (below the gate's posting threshold until earned or vouched) | most-unlinkable transport (own-cluster, friend-relay, or the Nym ceiling) |

The gate already enforces the bottom row: a fresh chain sits below `posting_threshold`
([`gate.rs`](../../../crates/moot/moothold/src/tessera/gate.rs)), so a burner's transport
privacy is a free cost. Picking a persona therefore picks a face, a reputation, and a
transport profile at once. The product surface is one gesture, not a settings maze: a
**burner** button mints a fresh chain on the most-private transport; a **main** is the
master endpoint with full standing.

## The four leaks and the adversaries

"Transport-unlinkable" is not one switch. There are four ways two personas get
correlated, and the honest way to talk about each is to name the adversary it defeats.

**Leaks:** (1) NodeId, (2) discovery/directory, (3) IP address, (4) timing/behavior.

**Adversaries:** a moot member, the discovery directory, a relay operator, a direct
counterparty, your home ISP / IP-to-subscriber map, a global passive observer.

The leaks cost wildly different amounts to close, which is why this is tiered.

## What we have today (grounded)

- **Per-persona NodeId is a caller choice, not a wall.** `P2pandaTransport::bind` takes a
  keypair argument; passing `derive_keypair(persona_id)` instead of the master makes the
  NodeId that persona's key. Crypto is free; the binding to master is convention.
- **Discovery is optional.** mDNS and random-walk are both opt-in builder calls
  ([`p2panda_transport.rs`](../../../crates/murm/transport/src/p2panda_transport.rs)); a
  quiet persona turns them off and connects via **iroh-tickets** (an out-of-band
  `NodeAddr` invite). Discovery-free is supported today.
- **Relays are automatic NAT fallback, not deliberate routing.** iroh-relay rides under
  p2panda-net and kicks in only when holepunch fails; iroh 0.98 does not expose
  deliberate "relay through this chosen node" as a public API.
- **Multi-device sync is proven, multi-device routing is not.** Mesh M1 (2026-06-12) does
  two-machine job round-trips over LogSync. But the mesh is cross-device *coordination*,
  not single-persona *egress routing*. A persona is a context boundary, not a network
  boundary: there is no "this persona egresses through device X."
- **The seam is a design gap.** `PersonaId` is a bare UUID; there is no
  `PersonaManifest.home_device`, no `PersonaStore` to persist it, and no
  session-startup resolution of a persona's transport. Adding it is design work, not a
  fight with iroh.

## Mode 1 + the half-measure (the near-term plan)

The cheap, real win. Defeats the directory and (with diversity) the relay operator;
leaves your IP visible to a direct counterparty.

- Per-persona **distinct NodeId** (`bind` with a derived endpoint key).
- **Discovery off** for non-main personas; ticket-only invites.
- **Half-measure:** a different relay (or a small rotation) per persona, so no single
  relay operator links your faces by IP.

**Done:** two personas on one device present two unlinkable NodeIds, share no discovery
record, and use different relays, so a directory watcher and moot members cannot
correlate them. Explicitly does **not** defeat a counterparty who sees your IP across two
faces. That is the next tier.

**Real work:** threading "which persona's endpoint does this moot ride" through the
moot/sync layer (today one transport per process). **Cost:** N endpoints means N sockets,
N holepunch states, more battery. The crypto is nothing.

## The own-device-cluster family (the core of the privacy story)

Routing through your own always-on devices is a genuinely different point in the design
space than Tor, and it is the right fit for Mere: **no third-party trust, QUIC stays
native, and it falls out of machinery we are already building (the device mesh +
voluntary hosting).** Three composable pieces:

- **(a) Self-hosted iroh relay.** Run iroh's own relay binary on your VPS or home box and
  point your endpoints at your relay URL (p2panda-net allows custom relay URLs). This
  removes stranger relays from the trust set. Native QUIC, lowest cost (one extra hop
  only on fallback). Caveat: iroh prefers direct connections, so the relay only engages
  on holepunch failure; to *always* hide your IP behind your relay you force relay-only
  (disable direct addrs) and pay the perf tax.
- **(b) Home-egress.** Route a persona's outward dialing through your always-on home node,
  so counterparties see your **home IP regardless of where you physically are** (cafe,
  hotel, mobile tether). This is location-decoupling, the purest own-cluster move.
- **(c) WireGuard / headscale glue.** The connective tissue: a roaming device tunnels to
  the home/VPS exit and QUIC rides inside transparently. The key insight is that this
  **re-origins at the network layer, below iroh, so it does not need iroh to expose
  deliberate relay-through-node.** It works today.

**What it buys:** your current network is hidden from peers, zero third-party trust (your
hardware), native QUIC, and a bonus of availability (a persona that lives on an always-on
node stays reachable when your laptop sleeps).

**What it does not buy (the honest boundary):** your **home ISP still maps the home
node's IP to your subscriber account**, so your household is linkable to anyone with
ISP-level visibility or a subpoena to your ISP. This is location-unlinkability, not
identity-anonymity. And routing *all* personas through one home node shares that one IP,
so they become linkable to each other behind it — use a different egress device per
persona, or accept that those faces are co-located behind home.

### Three build paths, in order of friction

1. **Network-layer (WireGuard) — simplest, buildable now.** The home node runs the iroh
   endpoint (or is the WireGuard exit); the roaming device tunnels in. No iroh relay
   protocol required. This is the pragmatic Mode 2.
2. **Mesh-job relay — middle.** Model "relay through device D" as a mesh M2 compute job:
   a relay request is posted, D's worker claims it and opens the forwarded stream. Keeps
   relay logic in the app/actor layer, costs one extra gossip round, and reuses the mesh
   M1 substrate. Buildable once mesh M2's resource-routing lands.
3. **iroh-relay-protocol — deepest, deferred.** A deliberate relay-negotiation protocol on
   the peer (iroh 0.98 does not expose this). Only worth it if in-protocol relay beats
   the WireGuard layer, which it probably does not for v1.

The shared missing seam for all three is persona-owned transport config:
`PersonaManifest.home_device: Option<PeerID>`, a `PersonaStore`, session-startup
resolution, and a `connect`-site check. The research scoped this at roughly 60% feasible
in four weeks on today's stack, the bulk of it being the persona-to-transport plumbing
rather than anything in iroh.

## Scale-up: friend / social relay routing

Widen the cluster to a small set of trusted friends' always-on nodes. A counterparty sees
a friend's IP, not yours, and with several friends you get a small rotating anonymity set
("which of N households originated this?"). The trust is **social** (your friends), not
commercial (a VPN) or stranger (Tor/Nym) — a different and for many a more acceptable
model, and it maps directly onto Mere's coalition/moothold federation tiers: the same
trusted-peer set that federates data can serve as egress relays. The cost is that a
friend's node sees your traffic metadata, and availability/coordination across the circle.

## The metadata-resistance ceiling: Nym (research-only)

The single option that defeats a **global passive timing adversary** (Sphinx fixed-size
packets, per-hop delay and reordering, cover traffic). But it breaks the own-cluster
intuition on every axis: the anonymity comes from a stranger operator set (third-party
trust we are trying to avoid), and it fights QUIC (true mixnet mode is store-and-forward
Sphinx, not a transparent UDP path, so it forces relay-/proxy-only and breaks
holepunching). Keep it as an optional high-sensitivity egress for a burner persona,
layered after your own egress, reached for only when the threat model genuinely includes
a global timing adversary. Not a default, not the own-cluster story.

**Skipped:** I2P (transport-model mismatch for native QUIC, and the anonymity set is
strangers' routers, not your cluster) and Yggdrasil (wrong axis: it gives stable
location-independent addressing, not IP-unlinkability, and adds no anonymity over
WireGuard).

## Voluntary-hosting tie

Cluster-routing is the minimal voluntary-hosting case, and it unifies the tier framework:
tier 1 is your own device as an egress node; tier 2 clusters a moot via member
volunteers; tier 3 federates moots via reciprocity-ledger infrastructure; tier 4
organizes mootholds via shared relays. Transport *privacy* is a per-persona choice;
transport *accountability* (a public node, cluster-routing for others) is a voluntary
commitment that accrues reputation, gated by a signed, heartbeated `HostingCommitment` and
the tessera reward ladder. So the same always-on node that gives you private egress is the
first rung of becoming infrastructure.

## Adversary x mode matrix

| Mode | Defeats | Leaves open |
| --- | --- | --- |
| **1: distinct NodeId + discovery off + tickets** | directory, (with diversity) relay operator | direct counterparty's IP, home ISP, global, public-post content |
| **own-cluster (home-egress, WireGuard)** | counterparty's *current-location* tracking, stranger relay operator, current local-network observer | **home ISP → household**, global passive, node-uptime-as-identifier |
| **friend/social relay** | link to *your* household (egress is a friend), small anonymity set | global passive, a friend's node sees metadata, all-N-friends observer |
| **Nym mixnet** | global timing observer, relay operator, ISP | content unless E2E, third-party trust, QUIC compatibility |

## Sequencing

1. **Now:** Mode 1 + the relay-diversity half-measure. Cheap, defeats casual correlation.
2. **Next:** own-cluster v1 via WireGuard (path 1) — the pragmatic Mode 2. Add
   `PersonaManifest.home_device`, a `PersonaStore`, and the `connect`-site resolution.
   Hides current location, zero third-party trust.
3. **Later:** mesh-job relay (path 2) once mesh M2 lands; friend/social relay as the
   social scale-up of own-cluster.
4. **Research:** Nym as an optional high-sensitivity egress (the global-timing tier);
   the iroh-relay-protocol path only if in-protocol relay proves worth it.

## Done-conditions and open questions

**Done (Mode 1):** two personas, two unlinkable NodeIds, no shared discovery record,
different relays; directory and members cannot correlate. Not defeating a same-IP
counterparty.

**Done (own-cluster v1):** a roaming device's persona egresses through the home node;
peers see the home IP, not the current network; zero third-party trust; the
home-ISP/household linkability boundary stated plainly in the UI so no one mistakes
location-privacy for anonymity.

**Open:**
- Per-persona egress device, so personas do not all share one home IP.
- Relay-only-forced vs direct-preferred (always-hide-IP vs fast-when-direct).
- Friend-relay coordination and availability; anonymity-set sizing.
- Whether the global-timing tier (Nym) is ever in scope, or "unlinkable to the directory,
  members, and a counterparty's current-location read" is the real product ceiling.

## Findings (research, 2026-06-25)

Grounded by a three-way research sweep (codebase grounding, a Tor-alternatives survey, an
adversary x mode matrix). Key cross-checks worth keeping:

- The own-cluster trio (self-hosted iroh relay + home-egress + WireGuard glue) is the
  canonical own-cluster move, needs zero third-party trust, keeps QUIC native, and is
  buildable today. It is the recommended start for IP-level privacy.
- WireGuard re-origins below iroh, which sidesteps iroh 0.98's lack of a deliberate
  relay-through-node API. That is why path 1 is the pragmatic Mode 2.
- Cluster-routing hides current location from peers but **not** your household from your
  home ISP. One research agent overstated this as "defeats ISP"; corrected here.
- Nym is the only option touching global timing correlation, but it is a trust-model and
  QUIC-model mismatch for the own-cluster intuition. High-sensitivity optional mode only.
- The seam is a design gap (no persona-owned transport config), not an iroh limitation.
