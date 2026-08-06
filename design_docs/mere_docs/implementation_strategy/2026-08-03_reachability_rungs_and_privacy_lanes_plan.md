# Reachability Rungs and Privacy Lanes

**Date:** 2026-08-03
**Status:** R1 in progress; R2 scoped; R3 scoped, gated on emissary entering the tree.
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

Still open on this lane, and deliberately not done here: nothing yet *writes*
the hint back. Graphshell refreshes from `remote_info` while a peer is
connected; Knot's resident has no equivalent refresh loop, so hints currently
arrive only if something else records them. That is the remaining half, and it
is a resident-loop change rather than a schema one.

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

## Ordering

R1 now: it is small, it is the common case, and no tunnel or mesh should sit
on the path between two machines on the same desk. R2 after the H6 addendum's
S2/S3 or beside them, since the transfer lane is the first real consumer of
durable remote reachability. R3's rendezvous receipt whenever emissary enters
the tree as a dependency; it does not block R1 or R2.
