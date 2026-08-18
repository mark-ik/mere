> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verse_docs/research/2026-03-28_libp2p_nostr_synergy_for_verse.md`
> - Source commit: `f36fc49b`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.
>
> The donor carried a repo-wide MPL-2.0 notice; it is kept verbatim below and
> has not been reviewed against this repository's MIT/Apache posture.

---

<!-- This Source Code Form is subject to the terms of the Mozilla Public
     License, v. 2.0. If a copy of the MPL was not distributed with this
     file, You can obtain one at https://mozilla.org/MPL/2.0/. -->

# libp2p + Nostr Synergy for Verse

**Date**: 2026-03-28
**Status**: Research / Design Exploration
**Purpose**: Analyze how libp2p and Nostr compose for Verse community-scale networking. Identify the unifying architectural pattern, enumerate concrete integration points, and surface gaps in the current design documentation.

**Related**:

- [`../technical_architecture/VERSE_AS_NETWORK.md`](../technical_architecture/VERSE_AS_NETWORK.md) — Verse network position; bilateral/community boundary
- [`../technical_architecture/2026-02-23_verse_tier2_architecture.md`](../technical_architecture/2026-02-23_verse_tier2_architecture.md) — Tier 2 dual-transport, VerseBlob, community primitives, Nostr signaling (§8)
- [`../technical_architecture/2026-03-05_verse_nostr_dvm_integration.md`](../technical_architecture/2026-03-05_verse_nostr_dvm_integration.md) — NIP-90 DVM compute layer, Verse-specific NIP-72 tags
- [`../../graphshell_docs/implementation_strategy/system/2026-03-05_network_architecture.md`](../../graphshell_docs/implementation_strategy/system/2026-03-05_network_architecture.md) — Three-context + two-fabric model; NIP-72/NIP-29 Verse integration (§4)
- [`../../nostr_docs/technical_architecture/nostr_relay_spec.md`](../../nostr_docs/technical_architecture/nostr_relay_spec.md) — Embedded relay Community mode
- [`../technical_architecture/2026-03-05_verse_economic_model.md`](../technical_architecture/2026-03-05_verse_economic_model.md) — Three-track economic model (sats/FIL/reputation)

---

## 1. The Unifying Pattern: Control Plane / Data Plane

The existing Verse architecture already separates Nostr and libp2p responsibilities, but the separation hasn't been given a name. Naming it sharpens every downstream decision.

| Plane | Protocol | What flows | Persistence | Addressing |
|-------|----------|-----------|-------------|------------|
| **Control** | Nostr (WebSocket relays) | Identity, governance, membership, announcements, social signals, discovery metadata, payment receipts | Relay-persisted, globally queryable | `npub` / event ID |
| **Data** | libp2p (QUIC / GossipSub / Bitswap) | VerseBLOBs, index segments, engrams, WARC archives, bulk content | Ephemeral mesh, DHT-addressed | CIDv1 / PeerId |

The rule: **Nostr events reference CIDs; libp2p delivers the content behind those CIDs.** Neither protocol does the other's job.

The DVM integration doc states this directly: *"Nostr is never used for bulk data transfer. It is a signalling and discovery bus only."* This research document treats that statement as the architectural invariant and explores its consequences.

### 1.1 Why This Split Works

- **Nostr relays are optimized for small signed events** (~64 KB max per NIP-01). They provide global discoverability, relay-backed persistence, and a social graph. They are not built for streaming megabytes.
- **libp2p is optimized for content-addressed bulk transfer.** Kademlia DHT, GossipSub, and Bitswap are designed for exactly this. They have no built-in persistence or social layer.
- **Neither protocol needs to be extended to cover the other's role.** The interface is a CID reference in a Nostr event tag — trivially parseable, universally addressable.

### 1.2 Interaction Points

Nostr and libp2p meet at well-defined seams:

1. **Nostr event references a CID** → libp2p retrieves the content.
2. **Nostr NIP-72 defines a community** → libp2p forms the content swarm.
3. **Nostr NIP-29 enforces membership** → libp2p distributes content to authenticated members.
4. **Nostr kind 30078 carries multiaddrs** → libp2p connects using them.
5. **Nostr NIP-90 dispatches compute jobs** → libp2p delivers input/output content.
6. **Nostr NIP-57 zaps settle payment** → libp2p transfer generates the Proof of Access receipt.

Each interaction crosses the plane boundary exactly once. No protocol reaches into the other's domain.

---

## 2. Integration Point Analysis

### 2.1 Community Bootstrap via Nostr

**Problem**: Tier 2 architecture §12.4 flags community bootstrapping as an open question — how do the first 100 users find each other when there is no DHT yet?

**Solution**: Nostr is the bootstrap layer.

The DVM integration doc already specifies Verse-specific tags in the community kind 34550 event:

```json
{
  "kind": 34550,
  "tags": [
    ["d", "<community-id>"],
    ["verse_dht_bootstrap", "<libp2p_multiaddr_1>", "<libp2p_multiaddr_2>"],
    ["verse_community_id", "<hex-community-id>"],
    ["verse_manifest_cid", "<CIDv1 of CommunityManifest blob>"]
  ]
}
```

This gives the community's initial bootstrap addresses. But bootstrap can go further:

**Member self-advertisement**: Each community member publishes a replaceable kind 30078 event advertising their own libp2p multiaddr when they come online. Tag structure:

```json
{
  "kind": 30078,
  "tags": [
    ["d", "verse-peer-<community-id>"],
    ["verse_community_id", "<hex-community-id>"],
    ["libp2p_multiaddr", "<multiaddr>"],
    ["online_since", "<unix-timestamp>"]
  ]
}
```

This creates a **self-healing bootstrap set**: new joiners query Nostr relays for the community's kind 30078 events, extract multiaddrs from recent member announcements, and try them in parallel. No hardcoded rendezvous servers needed. Stale multiaddrs (peer offline) fail quickly and the joiner falls through to the next.

The embedded relay in Community mode (nostr_relay_spec.md §3.3) doubles as a bootstrap cache: it stores these member-multiaddr events locally, so even when public relays churn, community operators maintain a local discovery index.

### 2.2 Nostr Social Graph as libp2p Trust Signal

**Problem**: libp2p's Kademlia DHT is trustless by design — any peer can join. But Verse communities need trust differentiation: the Tier 2 rebroadcast levels (Core → Extended → Public) and the curated governance model require distinguishing trusted peers from anonymous participants.

**Solution**: Feed Nostr-derived trust into libp2p's peer scoring.

GossipSub 1.1 has a built-in peer scoring framework. Application-layer scoring callbacks can influence message propagation priority. The Nostr social graph provides the trust signal:

| Nostr signal | GossipSub scoring effect |
|-------------|------------------------|
| Peer's `npub` is in your NIP-02 follows list | Elevated score — messages propagated preferentially |
| Peer's `npub` is in the community moderator set (kind 34550) | Maximum trust — messages validated and relayed first |
| Peer's `npub` has high reputation in Proof of Access ledger | Positive score bonus proportional to reputation tier |
| Peer's `npub` is in your contacts list as `Blocked` | Score floor — messages deprioritized or dropped |
| Peer's `npub` is unknown (no social graph signal) | Neutral score — standard GossipSub behavior |

This doesn't require modifying GossipSub — it's application-layer scoring fed into libp2p's existing peer scoring API. The Nostr social graph acts as a pre-existing web of trust that the data plane can leverage without having to build its own.

### 2.3 NIP-29 Membership as libp2p Swarm Gate

**Problem**: NIP-29 provides relay-enforced membership for private Verse spaces, but the enforcement only covers Nostr event access on that relay. The libp2p swarm for bulk content distribution has no equivalent membership gate — any peer with the community's GossipSub topic can attempt to join.

**Solution**: Use the NIP-29 relay as a membership attestation issuer for the libp2p swarm.

Flow:

1. Peer authenticates to the NIP-29 relay via NIP-42 AUTH.
2. Relay verifies membership and issues a short-lived **swarm attestation**: a signed Nostr event (custom kind) containing the peer's `npub`, their `PeerId`, and an expiry timestamp.
3. Peer presents this attestation to libp2p peers when connecting to the private community's GossipSub topic.
4. Receiving peers verify the attestation signature against the relay's pubkey (published in the community kind 34550 definition) and check expiry.
5. Peers with a valid attestation are accepted into the swarm. Peers without one are rejected.

```rust
/// Relay-issued attestation for libp2p swarm admission
struct SwarmAttestation {
    /// NIP-29 relay that issued this attestation
    relay_pubkey: NostrPubkey,
    /// The authenticated member
    member_npub: NostrPubkey,
    /// The member's libp2p PeerId (derived from same Ed25519 root)
    member_peer_id: PeerId,
    /// Community this attestation is valid for
    community_id: CommunityId,
    /// Wall-clock expiry (short-lived: 1–24 hours)
    expires_at: SystemTime,
    /// Relay's signature over the above fields
    signature: NostrSignature,
}
```

This gives relay-enforced membership (NIP-29's strength) applied to the libp2p data plane. The relay is the bouncer; libp2p is the venue. Attestation refresh is periodic — the peer re-authenticates to the relay before expiry.

**Trade-off**: This makes the NIP-29 relay a liveness dependency for private swarm access (peers can't get fresh attestations if the relay is down). Mitigation: attestations are valid for hours, not seconds. The swarm continues operating during brief relay outages; only new joins are blocked.

### 2.4 Dual-Rail Publication (Nostr + GossipSub)

**Problem**: Community governance events need both durability (must be retrievable months later) and real-time propagation (active members should see them immediately). Nostr provides durability; GossipSub provides immediacy. Neither alone covers both.

**Solution**: Dual-rail publish — send the event to both channels simultaneously.

| Action | Nostr (durable rail) | GossipSub (real-time rail) |
|--------|---------------------|--------------------------|
| FLora checkpoint approved | kind 4550 approval event (relay-persisted) | Announcement to active swarm members |
| New index epoch published | kind 30078 with CID references | GossipSub notification + Bitswap for segment content |
| Moderation action | kind 9000-9009 (NIP-29) or kind 4550 | GossipSub blacklist propagation |
| Community manifest update | kind 34550 (replaceable event) | GossipSub to inform connected peers immediately |
| Member came online | kind 30078 (replaceable, self-advertisement) | GossipSub peer exchange |

The pattern: **publish to Nostr for permanence, broadcast on GossipSub for immediacy**. Peers that were offline during the GossipSub broadcast catch up from Nostr when they rejoin. Deduplication is trivial — events have unique IDs.

This is not double the bandwidth: the Nostr event is the metadata (< 1 KB), the GossipSub message is the same metadata or a pointer to it. Bulk content is always libp2p-only.

### 2.5 NIP-90 DVMs as the Compute Bridge

The DVM integration doc covers this comprehensively. The key synergy summarized:

- **Job dispatch** (control plane): Nostr kind 5000+ events. Contain CID references to input data, not the data itself.
- **Input/output transfer** (data plane): DVM provider pulls input content from the libp2p swarm, pushes result content back as a VerseBlob.
- **Payment** (control plane): NIP-57 Lightning zaps on the result event.
- **Reputation** (control plane): Proof of Access receipt generated, stored as Nostr event or in the PoA ledger.

Verse communities don't need to build compute infrastructure — they outsource it to the Nostr DVM marketplace while keeping content distribution on libp2p.

### 2.6 The Embedded Relay as Unified Community Service

The nostr_relay_spec Community mode makes a single Graphshell instance a combined Nostr relay + libp2p peer:

**Nostr side** (relay):
- Stores governance events, membership, announcements
- Enforces NIP-29 group membership
- Provides NIP-46 bunker transport for community signing
- Caches member multiaddr events for bootstrap

**libp2p side** (peer):
- Participates in GossipSub for content distribution
- Serves VerseBLOBs via Bitswap
- Routes DHT queries for content discovery

Both share the same Ed25519 identity (via `P2PIdentitySecret`). A community member connects to one endpoint and gets access to the full stack. This makes "run a Verse community" a single toggle in Graphshell's settings, not two separate processes.

---

## 3. What the Existing Docs Get Right

The existing architecture is well-positioned. Specifically:

- **network_architecture.md §4.4** — layer assignment table correctly separates Nostr (community definition, membership, host approval) from libp2p (peer discovery, state replication, blob transfer).
- **verse_tier2_architecture.md §8** — correctly positions Nostr as convenience signaling, not a dependency.
- **verse_nostr_dvm_integration.md §2–3** — comprehensive layer assignment and Verse-specific NIP-72 tag schema.
- **nostr_relay_spec.md §3.3** — Community relay mode covers the Nostr-side infrastructure for community operation.
- **The three-context + two-fabric model** — Nostr as a cross-cutting fabric rather than a competing substrate is the right framing.

---

## 4. Gaps in Current Documentation

### 4.1 No explicit control-plane / data-plane naming

The separation exists in practice but isn't named as an architectural invariant. Design decisions have to re-derive the boundary each time. Adding a named principle ("Nostr is the control plane; libp2p is the data plane; they meet at CID references") to VERSE_AS_NETWORK.md or the network architecture doc would make the boundary self-documenting.

**Scope**: One paragraph addition to an existing doc.

### 4.2 NIP-29 → libp2p swarm gating not specified

The relay-issued `SwarmAttestation` pattern (§2.3 above) closes the private-community story for the data plane. Currently, NIP-29 only gates Nostr event access; the libp2p swarm has no equivalent membership enforcement.

**Scope**: New section in verse_tier2_architecture.md or a standalone spec. Medium complexity — requires defining the attestation event kind, expiry semantics, and the libp2p connection guard.

### 4.3 GossipSub peer scoring from Nostr trust signals not documented

The tier2 doc discusses moderation and curator signatures but doesn't connect them to GossipSub 1.1's native peer scoring API. The mapping (§2.2 above) is straightforward but needs to be specified so implementers know to wire it.

**Scope**: New subsection in verse_tier2_architecture.md §4 (Community Model). Small addition.

### 4.4 Member self-advertisement events not specified

Tier2 §8 covers community-level kind 30078 announcements. Individual members publishing their own replaceable kind 30078 events with their multiaddr (§2.1 above) for self-healing bootstrap isn't specified.

**Scope**: Extension to the existing kind 30078 usage in tier2 §8. Small addition.

### 4.5 Dual-rail publication pattern not formalized

The idea that governance events go to both Nostr and GossipSub simultaneously, with Nostr as the durable fallback, is implied but not stated as a formal pattern. Formalizing it prevents future specs from accidentally making GossipSub the sole distribution channel for durable content.

**Scope**: New subsection or a "publication patterns" section in VERSE_AS_NETWORK.md. Small addition.

---

## 5. Curve Mismatch Accommodation

Nostr uses secp256k1; libp2p/iroh use Ed25519. The network_architecture.md §7 already addresses this with the signed presence-binding assertion. For the synergy patterns above, this means:

- `SwarmAttestation` (§2.3) is signed with the relay's Nostr key (secp256k1). libp2p peers verify it using the relay's Nostr pubkey from the community kind 34550 event.
- GossipSub scoring (§2.2) maps `npub` (secp256k1) → `PeerId` (Ed25519) via the same binding assertion. The mapping is cached locally per community.
- Member self-advertisement (§2.1) events are signed with the member's Nostr key but contain the libp2p `PeerId` as a tag. Peers verify the Nostr signature and accept the `PeerId` claim.

No new curve bridging is needed beyond what the existing binding assertion provides.

---

## 6. Summary

libp2p and Nostr compose naturally for Verse because they occupy non-overlapping roles:

- **Nostr** tells you *what exists, who made it, who approved it, and where to find peers*.
- **libp2p** *moves the actual content between those peers*.

The five integration patterns identified (bootstrap, trust scoring, swarm gating, dual-rail publication, compute bridging) all follow the control-plane / data-plane split without exception. The gaps in current documentation (§4) are additions to existing specs, not architectural changes — the foundation is sound.
