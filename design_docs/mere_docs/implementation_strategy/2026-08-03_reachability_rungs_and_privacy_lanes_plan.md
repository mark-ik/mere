# Reachability Rungs and Privacy Lanes

**Date:** 2026-08-03
**Status:** R0 landed, recorded 2026-09-01; R1 landed 2026-08-03 (Graphshell) and 2026-08-06 (Knot), the genuinely remote receipt still open; R2 scoped; R3 scoped, gated on emissary entering the tree. Veilid retired 2026-09-01.
**Depends on:** the S1 findings recorded in the
[reference host plan](2026-07-27_graphshell_reference_host_plan.md) H6 addendum;
the retinue lane in `mere-transport` (feature-gated, trusted-mesh only until
retinue R8 IFAC / R9 ratchets land).

## Why

The 2026-08-03 relay-leg experiment disproved the mechanism it meant to prove:
a device holding only a paired node id and a relay URL cannot reach its
sibling. Registering a relay makes the local endpoint reachable and resolves
nothing about the peer, and the address book is per-process, so every learned
route dies on restart. Of the resolver ladder (mDNS, cached address, relay,
holepunch, radio), exactly one rung existed: pairing worked where multicast
worked, or where a fresh ticket was hand-carried. The Mac, whose mDNS is dead
(errno 65, unsigned binary), had no durable path to its siblings between
restarts at all.

Separately, privacy is wanted as a lane in its own right, common for people
rather than an expert posture. Those two threads meet: the identity-routed
systems that solve discovery without a third party are the privacy systems.

The prior-art survey behind these choices, in one line each:

- **Syncthing** is the shape to copy for R1: device id plus cached addresses
  refreshed on every successful connection, discovery services only for first
  contact and recovery, relays carry bytes and never identity.
- **Tailscale** is the shape deliberately not copied: cleanest UX, but the
  coordination server sees the whole mesh.
- **I2P** and **Tor** are identity-routed: the address is dialable by
  construction (netDB / onion services), no resolver service exists to see
  the metadata. The cost is tunnel latency, so they are control-plane
  material, never the fat byte path.
- **Reticulum** has the same identity-routed property without onion latency,
  at personal-mesh scale, over any medium. Retinue is the owned, RNS
  1.3.x-wire-compatible implementation of it.

The resulting plane split, which every rung below serves:

> iroh is the byte plane. The identity-routed lanes (retinue announces,
> emissary destinations) are the control and rendezvous plane. Radio is the
> constrained plane and the zero-infrastructure rendezvous. Nothing below
> moves blobs off iroh.

## R0. Local-link discovery (the landed rung)

Recorded 2026-09-01, after the fact. The bottom rung of the ladder was built
under the reference host plan's H10 and the device resident consolidation
plan's R5, and no single document said so. This section is the home: the
standards survey §4, H10 and the Djinn plan's F3 point here.

**What is in production.** p2panda's mDNS (`MdnsDiscoveryMode`,
`P2pandaTransportBuilder::mdns`) runs `Active` in Turnstone's Knot publish and
share-reader services, Graphshell's personal sync host, Knot's sync host and
Djinn: LAN-first, an exported ticket as the deliberate off-LAN fallback, no
relay by default. Five product crates enable `p2panda-net`'s `iroh_mdns`
feature. The service on the wire is `_p2pandav1._udp.local`, not `irohv1`;
every early instrument filtered for the wrong string, and `avahi-browse` never
surfaces it, so observe with a raw multicast listener armed before the peer
starts.

**Where the policy lives.** `P2pandaHostPolicy` and `P2pandaOverlayHost` in
`crates/murm/transport/src/p2panda_host.rs` (landed 2026-08-22, `0f0a8006`,
device resident R5) own discovery mode, relays, route seeding, endpoint
reporting and shutdown once; product hosts keep identity, admission, stores and
protocol handlers. Default policy is `Active` mDNS and no relay. A new host
configures discovery through this type, not the builder.

**Two forks pinned by branch** (root `Cargo.toml`, the "LAN peer discovery
(H10)" block):

- `iroh-mdns-address-lookup`, `mere` branch: upstream
  n0-computer/iroh-address-lookups PR #7, per-interface multicast sockets kept
  in sync through netwatch. The real fix. 0.4.0 joins only the default-route
  interface, so a multi-homed Windows host whose WSL/Hyper-V adapter holds the
  multicast route never hears LAN mDNS. Drop the patch when a release contains
  it.
- `swarm-discovery`, `mere` branch: rebuilds a socket after three consecutive
  send failures. A dead theory (H10 final, `dfe95f3e`). The macOS stall is
  policy, not sockets: an unsigned binary with no responsible GUI app is denied
  local-network egress on Sequoia and never becomes grantable. A resident Mere
  peer on macOS must ship as a signed app carrying the local-network usage
  declaration. The pin is harmless and fixes nothing; drop it at the next
  revisit.

**The client-side race**, fixed in `g5_peer` and inherited by every host:
p2panda builds the endpoint and its mDNS actor lazily on the first dial and
reads the address book synchronously, so a ticketless dial straight after
start loses on any real link and wins on loopback. Force endpoint construction
before dialing and retry to a deadline. `network_carrier.rs` records that the
retry belongs to the caller that knows its deadline, never to the carrier.

**What the rung does not do.** It resolves an address for a peer id you
already hold; `mere-transport` still exposes no way to enumerate what
discovery found. Service browsing and advertisement (DNS-SD, RFC 6763:
`_ipp._tcp`, `_http._tcp`, arbitrary services) exist nowhere in the tree. That
is planned work with owners rather than a gap in this rung: Murm implements
browsing and advertisement and Djinn supplies the registry of disclosure-safe
records ([Djinn family resident services plan](2026-08-22_djinn_family_resident_services_plan.md), F3);
H10 in the [reference host plan](2026-07-27_graphshell_reference_host_plan.md)
scopes the consumer, where a printer is a node, a TXT record is a claim and
never a fact, and browsing never implies admission. The transport-level
device-list accessor is the seam both need first.

**Done when** (met for peer discovery): ticketless connect across two physical
machines, proven Fedora-to-Windows and Windows-to-Fedora under H10 and
Q-PC-to-Windows for Knot's K2. Open: macOS as a discoverable peer waits on a
signed bundle, and cross-LAN discovery stays convenience coverage by G5's own
terms.

## R1. Cached dial hints (the workhorse rung)

`PairedDevice` gains two optional fields, one schema migration for both since
the H6 addendum already specs the second:

- `last_endpoint`: the peer's last known endpoint ticket (a relay-tagged
  `EndpointAddr`, serialized). Seeded into the transport at open as a HINT:
  a parse or dial failure is logged and skipped, never fatal, because a hint
  from last week must degrade where an argument the owner just typed should
  fail loudly. Refreshed from `remote_info` while the peer is `connected`
  (the field the peer directory grew on 2026-08-03), written back through the
  atomic settings save only on change.
- `pairing_id`: minted at pair time, retired at unpair, re-pair mints fresh.
  Recorded now so the migration happens once; the transfer grant binds to it
  under the H6 addendum's S2/S3, not here.

The `node_id` field's "deliberately not a ticket" doctrine stands unchanged:
identity is still the node id, and the hint is disposable. A stale hint costs
a failed dial candidate, not a wrong belief.

**Done when:** the Friday experiment reruns green: both ends restarted, no
`--sync-peer`, mDNS dead on the fetching side, and the fetch succeeds through
the cached hint alone.

**Met 2026-08-03, same day.** Seed session at 21:20 (first contact, ticket
carried on purpose: the rung is reconnect AFTER first contact); hint verified
in the Mac's settings; O-PC restarted to a fresh process and a fresh ticket;
receipt run at 21:24 with no ticket and mDNS dead fetched `60a7b29c…` from
`9b662f09…` 0.3 seconds after bind. Two details worth their lines: the
availability record had replicated into the Mac's durable store during the
seed session, so the holder resolved locally and the whole wait design never
engaged; and five seconds after the fetch, the refresh loop replaced the
cached hint with O-PC's post-restart address unprompted, which is the
self-healing property observed rather than asserted. One precision for later:
this LAN's direct IPs are stable across restarts, so the receipt does not
isolate WHICH component of the hint (direct address or relay) carried the
dial; the relay component alone is what an off-LAN reconnect would lean on,
and that isolation belongs to the first genuinely remote receipt.

Also confirmed in passing on O-PC: the new host cached the ThinkPad's hint
within seconds of starting, with no operator action. The rung is on for every
paired device, not just the receipt path.

Recorded during the audit, deliberate rather than accidental: existing
pairing records never receive a `pairing_id` (pair() is idempotent and will
not re-mint; S2 should mint at grant creation for legacy records); the
resident host is now a second writer of the settings file, with a
milliseconds-wide lost-update window against concurrent CLI pair/unpair,
tolerable at the 5s cadence, generation counter if it ever bites; and Knot's
lane has the same bare-node-id gap R1 just closed for Graphshell, untouched.

**Knot's half closed 2026-08-06.** `KnotSyncSettings.paired_writers` was a
flat `Vec<String>` of hex keys, so it carried identity and no route at all.
It is now `Vec<PairedWriter>` with an optional `last_endpoint`, seeded into
the transport at open through `add_peer_ticket` exactly as Graphshell's
`device_sync` does, with a parse or dial failure logged and skipped rather
than fatal.

The migration is backward-compatible by construction: a hand-written
`Deserialize` accepts both the old bare-string form and the new struct form,
so an existing settings file keeps loading and upgrades the next time it is
saved. Refusing the old form would have silently unpaired every device on
upgrade, which is exactly the failure mode this plan's own doctrine warns
about, so there is a test pinning it.

Three rules the API enforces rather than documents: a hint for an unpaired
writer is **ignored**, because a route must never create an admission;
re-pairing does **not** discard a learned route; and unpairing **does** drop
it, since an unpaired device's route is not ours to keep. `remember_endpoint`
returns whether anything changed, so the caller only pays a settings write on
change, matching R1's refresh discipline.

**Knot's refresh loop closed the same day.** The schema half above could store
a hint but nothing wrote one, so a Knot device could only ever use a route some
other component had recorded. `KnotSyncHost` now exposes the same two
primitives Graphshell's host does (`known_peers`, `peer_ticket`), and
`refresh_dial_hints` runs on the resident's existing 5-second pairing poll
rather than adding a second timer.

It follows Graphshell's disciplines rather than reinventing them: connected
peers only; a write only when the ticket actually differs (`peer_ticket` sorts
addresses before serialising, so an unchanged address set is byte-identical);
and a reload-modify-save through the atomic path, because `--pair-writer` is a
second writer to the same file and can land between the loop's read and its
write.

Two notes on where the code sits. The loop was written in the binary first and
moved into the library, because a `bin` target's contents are unreachable from
tests; the policy is library code and only the argument parsing is not.
And the `reachable` versus `connected` filter is its own function with its own
test, since that is the distinction that cost the Graphshell lane hours: an
address the endpoint holds for a peer it is *not* talking to may be exactly the
stale route a working hint would replace, so writing it back trades good
information for bad.

Still open, unchanged by this: a genuinely remote receipt. The LAN's direct IPs
are stable across restarts, so no receipt yet isolates whether the direct
address or the relay component carried a dial.

## R2. Announce-carried dial hints (the sovereign discovery rung)

The retinue announce already binds authenticated app data (peer id plus
master-key signature, see `reticulum_transport/announce.rs`). Extend that app
data to carry the relay-tagged iroh `EndpointAddr`. Three properties fall out:

- **Self-refreshing:** announces re-propagate on `announce_interval`, so the
  hint refreshes itself. The mesh IS the address cache; there is no separate
  invalidation problem.
- **Media-agnostic:** the same announce works over a TCP transport node and
  over RNode RF. Two devices with radios have zero-infrastructure discovery;
  remote devices need one always-on transport node, owned, identity-routed.
- **No new trust surface:** the hint rides inside the binding the announce
  already signs, so a forged address claim fails existing verification.

Open design point, to settle before building: a serialized relay-tagged
`EndpointAddr` is roughly 150-200 bytes, fine on TCP and tight on LoRa
airtime. Either RF announces carry a minimal marker with the full hint
fetched over the link, or the full hint everywhere with the airtime cost
accepted. Decide against measured announce sizes, not guessed ones.

**Done when:** a device with no cached hint and dead mDNS learns a sibling's
iroh address from an announce over a TCP transport node and completes a blob
fetch through it.

## R3. Privacy lanes

**Emissary (in-network plane, first).** An embeddable Rust I2P router
(`emissary-core`, MIT, NTCP2 + SSU2, SAMv3 + I2CP, HTTP/SOCKS proxies) is
forked and local. Three consumers in order:

1. **The netDB rendezvous receipt**, emissary's proving ground: publish the
   iroh dial hint at a persona-derived destination; a sibling with mDNS dead,
   no ticket, and no cached hint resolves it through the netDB and fetches.
   The Friday experiment again, with a netDB where the nothing was. No n0, no
   DNS, no server to keep alive.
2. **The turnstone `i2p://` web lane.** The H3 fixture addresses stop being
   decorative; minimal wiring is scheme-routing through the embedded proxies,
   real wiring is a SAM streaming lane in the fetch stack.
3. **Retinue over I2P.** Upstream RNS ships an I2P interface; wire
   compatibility means an emissary-backed interface makes the mesh's WAN
   links private AND interoperable with existing Reticulum-over-I2P transport
   nodes. Trunk and branches, as ruled.

Operational facts to carry, not discover twice: an embedded router needs a
reseed on first run (`emissary-util` ships one); embedding makes the resident
host an I2P participant, so transit policy, ports, and battery are owner
decisions, not inherited defaults; parity with Java/C++ I2P is a program, not
a milestone, and should live as its own checklist against the I2P proposal
specs once the first consumer is live. Real traffic is the pressure vessel.

**Noise (the in-stream plane, landed 2026-08-06).** The other two lanes hide
*where* a connection goes. This one hides *who is speaking and what they say*
from everything the connection passes through, including infrastructure we
run, and it does so without leaving the byte plane.

The framing this started with was wrong and is worth recording as wrong: the
lane was built as `NoiseTransport`, a TCP carrier `impl Transport`, sitting
*beside* iroh and retinue as a third way to move bytes. That contradicts this
document's own plane split. Noise is not a transport; it is a handshake
framework with no opinion about what carries it. It composes **over** iroh
rather than competing with it.

So the module is now `noise`, the `impl Transport` is gone, and the surface is
`handshake` / `secure_initiator` / `secure_responder`, all generic over
`AsyncRead + AsyncWrite + Unpin`. The TCP half survives as `NoiseListener`,
explicitly the standalone deployment for links with no carrier under them, and
deliberately not a `Transport`: a `Transport` dials by `PeerID` because it owns
discovery, and this owns none. The old `impl` had a `connect` that always
failed, which was the tell.

What the composition buys, beyond confidentiality through relays:

> **Layered identity.** iroh's endpoint key answers *where do packets go* — it
> is routing machinery, visible to relays, and long-lived because
> reachability depends on its stability. The Noise identity answers *who is
> speaking*, and it is a parameter, not a derivation of the carrier's key.

That distinction did not exist in the tree before this. Every lane bound with
the persona master keypair, so there was one key doing both jobs. Passing
`derive_child` or `generate` to the handshake now yields an identity a peer can
verify without learning which node, at which address, it reached. The receipt
is `noise_over_an_iroh_stream_layers_a_second_identity`, over a live iroh
connection: the carrier reports the carrier identity, the session layer reports
a different one, and both are proven rather than claimed.

Not done, and not needed yet: Noise over iroh *datagrams* (the low-latency
composition), and pre-shared-key or `Noise_KK`/`Noise_IK` patterns for
capability-scoped sessions. `XX` is the right default while both ends are ours.

**Arti (clearnet plane, later).** The Tor Project's own Rust rewrite:
library-first, client and onion-service support solid, relay mode incomplete.
Consumer posture is correct here, the opposite of emissary: Tor's network is
already the common privacy infrastructure for ordinary web browsing, and the
Rust I2P field is empty where the arti field is institutionally owned. The
concrete future consumer is a "browse privately" toggle in turnstone's web
lane. Nothing depends on it; evaluate when that toggle is wanted.

## Not in scope

- **n0 DNS discovery.** Remains the shortcut for WAN-now; superseded as the
  preferred path by R1+R2+R3, which reach the same capability without the
  third-party visibility. Decide only if remote is needed before they land.
- **A self-hosted iroh relay.** Still useful for the byte plane behind hard
  NAT; unchanged by this note, separate decision.
- **Grant semantics on `pairing_id`.** The H6 addendum's S2/S3 own them; R1
  only mints the field so the schema moves once.
- **The emissary parity checklist.** Its own document, once the rendezvous
  receipt exists to anchor it.
- **Veilid.** The [event DAG brief](2026-05-07_event_dag_substrate_brief.md)
  §6 once made it an optional per-moot privacy transport. Retired 2026-09-01:
  it replaces transport rather than sync, so the plane split above leaves it
  no role that emissary and retinue do not already fill.

## Ordering

R1 now: it is small, it is the common case, and no tunnel or mesh should sit
on the path between two machines on the same desk. R2 after the H6 addendum's
S2/S3 or beside them, since the transfer lane is the first real consumer of
durable remote reachability. R3's rendezvous receipt whenever emissary enters
the tree as a dependency; it does not block R1 or R2.

## Findings

### 2026-09-01 — the ladder's bottom rung was built without a home

- R0 recorded from code: `P2pandaOverlayHost` (2026-08-22), the two
  branch-pinned forks and their verdicts in the root `Cargo.toml`, active mDNS
  in five product surfaces. The standards survey §4 (2026-08-24) claimed no
  local-link discovery story existed anywhere in the stack: true for DNS-SD
  service browsing, false for peer discovery. Corrected there.
- R1's status line said "in progress" while its own body recorded the receipt
  of 2026-08-03 and Knot's half on 2026-08-06. Corrected. The genuinely remote
  receipt, which isolates whether the direct address or the relay component
  carried a dial, remains open.
- Veilid, named in the event DAG brief §6 as a per-moot privacy transport,
  retired in favour of R3's lanes; cross-referenced under Not in scope.

## Progress

### 2026-09-01

- R0 section added; status line corrected; Findings and Progress sections
  added per DOC_POLICY §8; Veilid entry added to Not in scope. Survey §4,
  reference host H10 and Djinn F3 now point here.
