# Contact and remote-identity model brief

**Date**: 2026-06-15
**Status**: Research brief. Settles the *remote* side of identity (contacts: how a
peer is named, resolved, and trusted across protocols) that the
[comms shell plan](../implementation_strategy/2026-06-05_comms_shell_plan.md)
deferred and the [persona model brief](2026-05-14_persona_model_brief.md) left out
of scope. Pairs with the persona brief: that one is *me*, this one is *them*.
**Scope**: the kith/kin contact record; whether a contact is rooted on a handle or
a key; which resolvers Mere needs; and the addressing / anti-spam / ephemeral
stances that fell out of comparing misfin against SHARP. No code proposed; this
frames decisions for a later phase.
**Related**:

- [persona model brief](2026-05-14_persona_model_brief.md) — the *me* side. §7:
  "persona ≈ DIDs this persona controls."
- [murm p2p landscape brief](2026-05-31_murm_p2p_landscape_brief.md) — §2/§6 already
  name WebFinger + NIP-05 + back-claim proof + the previous-handle chain.
- [comms shell plan](../implementation_strategy/2026-06-05_comms_shell_plan.md) —
  open point "the identity model (one persona, many protocol handles), settle in
  P5"; decision 3 (misfin receive = run a server).
- [daemon split research brief](2026-05-14_daemon_split_research_brief.md) — the
  listener-process question (gpui-era; needs a genet/xilem re-read).
- [capability gate catalogue brief](2026-05-14_capability_gate_catalogue_brief.md) —
  contacts are persona-scoped, so they sit under the gate chain.
- [lexicon brief](../../2026-05-04_lexicon_brief.md) — *kith / kin* contact tiers.
- Code: [`comms/src/model.rs`](../../../crates/shell/comms/src/model.rs),
  [`murm/gazette`](../../../crates/murm/gazette/src/lib.rs),
  [`murm/misfin`](../../../crates/murm/misfin/src/lib.rs),
  [`persona/identity`](../../../crates/persona/identity/src/lib.rs).

---

## 1. The gap this fills

Two docs touch identity; neither owns the remote party.

- The persona brief settles *me*: one-human-many-personas, and a persona is a
  key-bag rather than a single key ("a persona's vault ≈ DIDs this persona
  controls", §7). It says nothing about peers.
- The comms plan deferred the cross-protocol identity model to P5 and only
  half-landed it. P5 built the per-protocol leaf
  `Identity { protocol, address, display_name }` in
  [`model.rs`](../../../crates/shell/comms/src/model.rs), with no contact rollup and
  no stable middle. The label is "display_name or the raw address", so today two
  addresses for one human are two unrelated rows.

So the *them* side is unbuilt and undecided. This brief decides it.

## 2. The split that governs everything: what an identity is rooted in

Every person-addressing protocol roots identity in one of two things, and that root
decides whether WebFinger can resolve it.

- **Host-rooted** (`name@host`, trust via DNS + TLS): misfin (`mailbox@host`,
  cert-bound), email, XMPP, Matrix, gemini/gopher. WebFinger resolves this whole
  family natively, and [`gazette`](../../../crates/murm/gazette/src/lib.rs)
  already fans one `acct:user@host` into misfin + gemini + gopher + ActivityPub +
  http endpoints.
- **Key-rooted** (the pubkey *is* the identity, host optional): murm (author keys),
  Nostr (npub), AT Protocol / DIDs, the contact-code messengers. WebFinger cannot
  resolve these alone, because there is no host to finger. The two serious
  key-rooted systems resolve through the *same primitives* WebFinger uses: Nostr
  NIP-05 (`/.well-known/nostr.json?name=alice` returns a pubkey) and AT Protocol
  (DNS-TXT or `/.well-known/` returns a DID, whose document lists endpoints).

## 3. Decision: contacts are key-rooted; handle and endpoints hang off the key

Adopt the AT Protocol three-layer split and root the contact on the stable middle:

- a **mutable handle** (`alice@host`, can change)
- a **stable key or DID** (never changes), where the contact is rooted
- **current endpoints** (where she is reachable now), each with its own trust state

A **contact** (a *kith* / *kin* record) reads: your petname, pointing at a stable
key, pointing at a set of resolved endpoints. Reasons:

1. **The corpus already presumes it.** Back-claim proof and the previous-handle
   chain (`mere:previous-handle`) named in the landscape brief only make sense if a
   stable key anchors the handles. This brief makes the implicit root explicit.
2. **It survives host moves.** Rooting on the handle (WebFinger alone, handle to
   endpoints with nothing stable between) breaks the moment a peer changes misfin
   hosts. Rooting on the key does not.
3. **It mirrors the persona side.** The self is already a key-bag; the contact is
   the same shape seen from outside.

WebFinger keeps its job: **one endpoint resolver hanging off the contact**, for the
host-rooted endpoints. It is not the identity.

## 4. Resolvers and trust are separate concerns

- **Resolution (where).** Host-rooted is WebFinger (built). Key-rooted is a
  NIP-05-shaped resolver that returns a key. Note: `gazette` is currently
  "de-nostr'd" (host-rooted only per the landscape brief), so the key-rooted
  resolver was removed and must return, either as a sibling resolver or a typed
  WebFinger `rel`. Hostless keys (a murm invite, an iroh ticket, a QR code) resolve
  out-of-band, through no directory at all.
- **Trust (who).** A resolver says where to reach someone; it never says the binding
  is real. Trust is a separate layer: misfin TOFU fingerprint, murm/nostr key
  signature, DID self-authentication, or the corpus's back-claim proof. The contact
  holds **per-endpoint trust state**, so "verified vs unverified" is per address, not
  per person.

## 5. The contact record (proposed, above the comms `Identity` leaf)

```rust
// Local-only. Never serialized onto any wire. The remote mirror of a persona.
struct Contact {
    petname: String,                        // your name for them (kith/kin)
    keys: Vec<VerifiedKey>,                 // the stable middle; usually one, may rotate
    handles: Vec<Handle>,                   // acct:user@host, did:…, npub… — attested to a key
    endpoints: Vec<(Identity, TrustState)>, // comms::Identity + per-endpoint trust
    tier: ContactTier,                      // kith vs kin (lexicon)
}
```

The join the comms model lacks: a conversation's participants are protocol-level
`Identity`s; a `Contact` is the local rollup that says "these N identities are all
Bob". **Contacts are persona-scoped** (`<data_root>/personas/<persona_id>/contacts/`),
because a throwaway persona must not share the work persona's address book; this also
slots contacts under the
[capability gate chain](2026-05-14_capability_gate_catalogue_brief.md).

## 6. File transfer needs no contact model of its own

File transfer is content- or capability-addressed, not person-addressed. Mere
already has it: [`eidetic-iroh-fetcher`](../../../crates/eidetic/eidetic-iroh-fetcher/)
and [`murm/transport`](../../../crates/murm/transport/src/blobs.rs) move blobs over
iroh tickets (content hash plus node address); the wider field is the same shape
(Magic Wormhole's PAKE code, IPFS CIDs, Willow/Earthstar capabilities). You address
the blob or a one-time channel, not the person. Persona and contact enter only for
**authorization** (who may fetch) and **verification** (is this the file they meant),
both riding the same key the messaging side already holds. File transfer *references*
the contact; it does not duplicate it.

## 7. Stances from the SHARP comparison

[SHARP](https://github.com/outpoot/twoblade) (Self-Hosted Address Routing Protocol,
`user#domain`) prompted three protocol stances:

- **Addressing: keep `@`, do not adopt `#`.** misfin parses `@` out of the cert DN
  and the request line; WebFinger and the whole federation use `@`. The `#` is
  SHARP's "not SMTP" branding; Mere already speaks the real protocols, so consistency
  beats the glyph. Where Mere owns the addressing (murm), the real axis is
  **petname-over-key**, not the separator.
- **Hashcash: design the knob now, gate the wire.** The
  [misfin spec](https://github.com/JCLemme/misfin/blob/master/specification.gmi)
  reserves status **code 64 for a Hashcash-style anti-spam measure** (currently
  undefined). So proof-of-work is a socket misfin left open. Build the configurable
  cost-to-contact knob now; gate the misfin *wire* behavior until the community
  defines 64; for murm (ours) define cold-cabal-invite PoW today. This is the
  bilateral complement to the federation-side "persona id-chain + tessera
  depreciation" spam resistance the landscape brief names.
- **Ephemeral: murm-native real, misfin local-only.** misfin has no expiry in-spec,
  so a TTL on the misfin wire is non-compliant and unenforceable (a non-Mere client
  keeps the gemtext). Give murm real burn/TTL (all murm clients honor it, with the
  usual "not a guarantee against a malicious client" caveat); give misfin a *local*
  retention policy, labeled as convenience, not a promise.

## 8. The daemon question (mostly already decided)

To *receive* misfin, something must listen on `:1958` with your mailbox cert at your
host, whenever mail might arrive. That is inherent to misfin (and to SHARP's
SRV-record self-hosting), not a Mere limitation. The comms plan already decided it
(decision 3: "misfin receive = run a server, with an external-server option"), and
P3b built the server behind a `server` feature. The clean part: the misfin adapter
reads a `MailboxStore`, not a socket, so the listener splits into a headless
**`misfind`** daemon (always-on, owns the store) with the GUI just reading the store.
Three configurable shapes: in-process listener, separate daemon, or a rented host.
The general daemon-vs-clients process question belongs to the
[daemon split research brief](2026-05-14_daemon_split_research_brief.md) (gpui-era; it
needs a genet/xilem re-read). **murm is the no-daemon escape hatch**: p2p over iroh,
key-rooted, NAT-friendly, no always-on listener. Mere can offer "needs a server" and
"needs no server" side by side because both sit behind one comms pane.

## 9. What this commits to / open

**Commits:**

- Contacts are key-rooted: petname, then a stable key, then endpoints (a kith/kin
  record).
- WebFinger is one endpoint resolver, not the identity.
- The key-rooted (NIP-05-shaped) resolver must return; `gazette` is
  host-rooted-only today.
- Resolution and trust are separate layers; trust state is per-endpoint.
- Contacts are persona-scoped.
- File transfer references the contact, never duplicates it.
- `@` not `#`; hashcash knob now with the misfin wire gated on code 64 and murm PoW
  ours; ephemeral murm-native plus misfin-local.

**Open:**

- Exact `VerifiedKey` / `Handle` / `TrustState` shapes.
- Whether the key resolver is a WebFinger `rel` or a sibling crate.
- Raw keys vs DIDs (`did:web` / `did:plc`) for the stable middle.
- Contact-import UX (WebFinger paste, QR, ticket) and how back-claim proof verifies a
  key-to-handle binding.
- kith vs kin tiering: is the tier trust-derived (verified becomes kin) or user-set?
