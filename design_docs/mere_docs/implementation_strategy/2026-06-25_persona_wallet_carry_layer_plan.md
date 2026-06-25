# Persona Wallet — The Universal Carry Layer

**Status (2026-06-25):** design. Companion to the
[persona_transport_unlinkability_plan](2026-06-25_persona_transport_unlinkability_plan.md).
The wallet is "Layer 0", the carry layer everything else references. Most of what sits
under it exists or is named in code; the wallet primitive itself, per-persona
encryption-at-rest, and the capability-token layer are the gap. No wallet code yet.

This doc answers three questions that turned out to be one: how do you *carry* a persona
across devices, is that mechanism the same for engrams and history, and is data private
(encrypted) or public (cleartext)? The answer: there is one carry model, and a single
public-vs-private dial that the eidetic schema already names.

## The frame: one model, carried by reference

Everything in Mere is a signed, content-addressed object: an engram is a BLAKE3-hashed
payload in a signed envelope; graph state is signed mutation ops; standing is a fold over
signed tessera ops; a persona is a root identity that authored some of them. "Carrying"
any of them is the same operation: **carry the reference plus the key, let the bytes
sync.** So the wallet is not persona-specific. It is the carry layer for the whole system,
and a persona is just its root-of-trust instance.

You carry three things with very different weights, and the trick is to carry only the
smallest:

1. **The secret root (tiny, must move securely).** Persona keys are derived, not stored:
   `derive_keypair(BLAKE3('persona' || persona_id))` over the master seed. You carry the
   32-byte seed and re-derive. This is the wallet's core.
2. **The standing (derived, re-folded).** Tessera standing is a fold over the persona's
   signed event log. You do not move reputation, you sync the ops and re-fold.
3. **The data (large, content-addressed, synced by reference).** Graphs, engrams,
   history: each is a CID plus a signed envelope. You carry their root CIDs and the keys
   to read them; the bytes fetch on demand over iroh-blobs / LogSync.

So the wallet holds **keys, capabilities, and root references, never the bulk data.** The
failure mode to avoid is stuffing data into the wallet and making it heavy. Keep it to the
root of trust plus pointers and "carry" stays cheap.

## Carrying a persona: two pairing modes, one ceremony

The identity crate's README already names the delegation mechanism (**meadowcap**:
`sign(capability + delegatee.pubkey, delegator.privkey)`), so this sits above the existing
Ed25519 primitives with no new crypto.

- **Remote-auth (the default).** The new device mints its **own** keypair and a
  wallet-holding device signs a delegation cert over
  `{device_pubkey, scope, expiry, attenuation, persona set}`. The device never sees the
  seed, signs its own actions, acts fully offline within its grant, and cannot exceed it
  (no sibling-persona keys, no acting past expiry). A lost device is a single append to a
  signed, append-only **revocation set on the `DeviceRoster`** the transport plan already
  syncs. The root is untouched. This is least-privilege and the only mode with clean
  leaf-level revocation.
- **Copy (an explicit "fully me" choice).** Transfer the 32-byte seed; the device
  reconstructs the identical provider and is indistinguishably the user (same master
  pubkey / NodeId, re-derives every persona offline forever). The cost the UI must state
  plainly: a seed copy **cannot be cleanly revoked**, so losing it means rotating the
  master root (mint `root_v2`, sign a "supersedes `root_v1`" link into the persona-chain
  key-history, re-attest to contacts and moots). Reserve it for the inner trust ring.

**The ceremony (one flow, mode chosen at the end):**

1. **Channel.** The old device shows a QR / short code; the new device scans. Establish an
   ephemeral encrypted channel with a PAKE (SPAKE2-shaped) keyed by that code, confirmed
   by a short auth string both users compare (the FIDO caBLE proximity UX). This is the
   human trust anchor, identical for both modes.
2. **Device keypair.** The new device always mints its own keypair into the OS keychain
   (optionally hardware/FIDO-bound for a high-assurance tier), so it is addressable in the
   roster even if it also receives the seed.
3. **Mode choice, in plain words.** "Make this a full device (becomes fully me, works
   offline forever, can't be remotely revoked)" sends the seed over the channel.
   "Give this device limited access (revocable, expiring, scoped)" signs and returns a
   delegation cert.
4. **Enroll.** Append a signed `DeviceEnrollment {device_pubkey, mode, label, exposure}`
   to the synced `DeviceRoster`, the same structure that carries per-persona egress.
5. **Revoke.** Remote-auth: append the device pubkey to the roster's revocation set.
   Copy: trigger master rotation. The UI says which at pairing time, which is the whole
   argument for the remote-auth default.

This is the carry-vs-home fork one notch sharper: a copy device *is* the persona (and is
linkable to your main by design, per the transport plan); a delegated device is a scoped,
revocable hand into it. UCAN and Biscuit are richer encodings of the same cert; adopt the
meadowcap cert internally now, re-express device grants as Biscuit when the capability
gate graduates to it, and keep FROST (master custody) and FIDO (hardware device keys) as
optional tiers, not the everyday answer.

## The privacy dial is `PrivacyClass`, and it has three regimes

The public-vs-private choice is **per object, per persona**, and the eidetic schema
already names it: every engram carries a `PrivacyClass` of `LocalOnly` (default),
`TrustedPeersOnly`, `MootScoped`, or `PublicPortable`, an explicit promotion, never
automatic. Today engrams are stored **cleartext**, content-addressed by BLAKE3, with the
class as a metadata tag rather than an encryption boundary, so everything is in the
cleartext lane until the encrypted lane is built.

The decision rule: **is the readership the membership, and are the hosts trusted-enough?**

- **Public → moot (cleartext, gated at the membership layer).** A moot engram is
  read-by-design, so per-object encryption guards nothing while costing four things
  cleartext gives free: dedup (same payload, one CID; Athanor already dedups by
  `content_hash`), pinning-by-others (a community node pins your public engram precisely
  because it can read it), public verifiability (a signed cleartext engram self-checks
  with no key exchange), and a vanishing membership cost. Access control lives at the
  **replication layer**: `p2panda-auth` (Pull/Read/Write/Manage over a signed-op DAG) plus
  meadowcap cluster-path read caps plus the tessera gate decide who LogSync *sends bytes
  to*. The bytes stay cheap and opaque-free.
- **Private → persona (encrypted under the persona vault key).** Encrypt when a party may
  hold the bytes but must not read them: a leaked replica, an untrusted host you still
  want pinning your private data, a future member who should not see history. The engine
  is chosen: `p2panda-encryption` (MLS/DCGKA-derived), Data Encryption (symmetric group
  key, XChaCha20-Poly1305, post-compromise security via rotation) for at-rest groups,
  Message Encryption (per-message ratchet) for the cabal chat lane. Minimal shape is
  envelope encryption: a random content key seals the payload, wrapped per authorized
  persona. The honest costs: lost dedup (unless convergent encryption, whose
  confirmation-of-a-file attack rules it out for low-entropy data), lost pin-by-strangers,
  lost public verifiability, and **no retroactive recall** (rotation governs only future
  content).
- **Capabilities ride on top of either pole.** This is the cleanest part: a capability is
  *delegable, attenuable, expiring, offline-verifiable* regardless of the dial. For
  private data the capability **wraps the decryption key** (handing it conveys read access,
  an expiry yields time-boxed reading of ciphertext the bearer may already hold). For
  public data the capability **is the membership/invite token** (no key, just
  replication-admission the gate checks before LogSync sends). Same machinery, differing
  only in what it releases: a key, or sync-membership. That is why keyring and
  capability-granter are complementary, not alternatives. They are the two things a
  capability hands out.

**The decisive asymmetry for groups:** a membership change in the cleartext lane is one
signed event in the access-control DAG (start or stop sending bytes), while group
encryption pays an MLS epoch rekey on *every* join/leave, which an offline-first p2p
setting makes worse (concurrent and offline membership changes must be conflict-resolved
before the group agrees on the next key). So membership-gated cleartext is the right call
exactly where naive intuition reaches for E2E: a moot with fluid membership and
trusted-enough hosts. Reserve group encryption for a private moot whose hosts are
explicitly untrusted.

**Mixed sensitivity nests.** A public graph engram can reference a private leaf engram by
content hash, so the public graph stays pin-and-share-able while the sealed leaf stays
persona-encrypted. The dial is per node, not per session.

## The layer map

| Layer | Private or public | What |
| --- | --- | --- |
| **0 Wallet** (unbuilt) | private | seed + capability tokens + persona roster + root CIDs; bulk synced by reference |
| 1 Persona / vault | private | master Ed25519 + per-persona derivation; the identity boundary |
| 2 Eidetic | private by default | immutable BLAKE3-hashed engrams + `PrivacyClass`; the data |
| 3 Tessera | public within a moot | earned standing; an input to authz, gated by `gate.rs` |
| 4 Moot | public/shared to members | the community graph + flora + hosting commitments |
| 5 Constitution | public | amendable law; who may grant caps, thresholds, fork rules |
| 6 Transport | mixed | per-persona NodeId / discovery / egress (the transport plan) |
| 7 Moothold | shared | federation: reciprocity ledger, cross-moot pinning |

Private (persona, eidetic) sits inside; public (moot, constitution, moothold) is the
shared outside; the wallet is the small carrier that lets a persona and its data move
between your devices.

## Exists vs gap

**Built or named in code:** persona identity (master Ed25519 + BLAKE3 derivation);
content-addressed engrams with `PrivacyClass` (cleartext today); the persona vault with
`PassphraseEncryptedStorage` (Argon2id + ChaCha20-Poly1305) for credentials; the tessera
ledger + gate; the moot roster + flora; the transport (iroh, per-persona NodeId optional);
the chosen lane engines `p2panda-auth` (cleartext gating) and `p2panda-encryption` (the
private lane), both evaluated in the substrate spike.

**Gap to build:**

1. **The wallet primitive** — seed + persona roster + engram root-CID manifest +
   capability-token slots + transport-profile bindings, with sync-on-demand for bulk data.
   Today each layer bootstraps independently; there is no single "this is my wallet" entry
   point. The closest existing structure is `personas/<persona_id>/persona.json`.
2. **Per-persona encryption-at-rest for eidetic** — decide whether non-`LocalOnly` engrams
   are sealed on disk or only on the wire, and wrap content keys per persona. Today
   privacy is a metadata tag, encryption is wire-only.
3. **The capability-token layer** — meadowcap certs now (device delegation + cluster-path
   read caps), re-expressed as Biscuit when the constitution's authorize seam graduates.
4. **The pairing ceremony** — PAKE + SAS + roster enrollment + the copy/remote-auth fork.
5. **Cross-device persona restoration** — wallet export/import: seed recovery, engram
   re-fetch by CID, standing re-fold.

## Sequencing

1. **Wallet primitive + manifest** (extend `persona.json` into the carry layer: seed
   handle, persona roster, root CIDs, capability slots, transport bindings).
2. **Pairing ceremony**, remote-auth default (meadowcap cert) + copy as explicit choice,
   revocation over the `DeviceRoster`.
3. **Per-persona encryption-at-rest** for the private lane (`p2panda-encryption` envelope):
   `LocalOnly` / `TrustedPeersOnly` engrams sealed on disk under the persona vault key.
   Decided posture (2026-06-25), so first-class, not deferred.
4. **The public lane wired through replication:** honor `PrivacyClass` at the
   `p2panda-auth` gate (who syncs cleartext) for promoted `MootScoped` / `PublicPortable`
   engrams; promotion is the decrypt-and-recommit step.
5. **Capabilities** as meadowcap caps, graduating to Biscuit with the constitution.

## Open questions

- Where does the seed live by default: which devices are copy (full) vs remote-auth
  (delegated), and is the master under FROST custody (quorum to rotate / enroll)?
- **Decided (2026-06-25): encrypt at rest.** `LocalOnly` / `TrustedPeersOnly` engrams are
  sealed on disk under the persona vault key (private by default; the wallet leans keyring),
  and promotion to `MootScoped` / `PublicPortable` re-encodes into the cleartext-gated lane
  so the moot can dedup / pin / verify. `PrivacyClass` is therefore the storage regime, and
  "publish to a moot" is a decrypt-and-recommit step. (Open underneath: convergent vs salted
  for the private lane; whether `TrustedPeersOnly` is encrypted-with-handed-key or
  cleartext-gated-to-those-peers.)
- Convergent encryption only for high-entropy private payloads (dedup) vs salted (privacy
  against confirmation), decided per schema.
- The wallet-sync bootstrap: the wallet is itself data that syncs over the personal mesh,
  but syncing it needs the identity, so the seed is the one thing transported out of band
  (recovery phrase vs device-pairing handover).

## Findings (research, 2026-06-25)

Grounded by a three-way sweep (pairing/delegation models, the private-vs-public data
argument, codebase grounding). Cross-checks worth keeping:

- The dial is the existing `PrivacyClass` enum; engrams are cleartext today (privacy is a
  metadata tag, not yet an encryption boundary).
- meadowcap is already named in the identity README, so remote-auth delegation is the
  home-grown fit; UCAN/Biscuit are richer encodings of the same cert.
- Cleartext wins for read-by-design data on four counts (dedup, pin-by-others, public
  verifiability, vanishing membership cost); the membership change being one signed event
  vs an MLS epoch rekey is the decisive asymmetry for groups.
- Both lanes share the no-retroactive-recall limit; encryption buys forward-looking
  revocation, not recall.
- Capabilities are orthogonal to the dial: they wrap a key (private) or are an invite
  token (public), one mechanism either way.
- The wallet (Layer 0) is the one genuinely unbuilt carry primitive; the layers beneath it
  exist or are named.
