# Persona Wallet — The Universal Carry Layer

**Status (2026-07-04):** storage slice, first host-adoption slice, typed signed-grant
slice, remote-auth grant issuance slice, wrapped private-epoch crypto helper slice,
pairing-transcript helper slice, pairing ticket/code helper slice, first Meerkat
pairing-host slice, delegated-device response/SAS preview slice, enrollment-bundle
slice plus delegatee enrollment-host/bootstrap-preservation slice, and the first
`private.read` host/restore slice plus pairing-expiry/artifact-coherence hardening
slice plus first capability-slot wiring slice plus first delegated-device
revocation slice landed, plus the encrypted-vault design slice and the first
local sealed-record unlock/migration slice plus the startup-unlock setting /
locked-startup flow slice. Companion to the
[persona_transport_unlinkability_plan](2026-06-25_persona_transport_unlinkability_plan.md).
The wallet is "Layer 0", the carry layer everything else references. Most of what sits
under it exists or is named in code; the identity-level and persona-level wallet manifest
stores are now real in `session-runtime`, and Meerkat now seeds/loads them at startup and
points `sync`/`comms` at the shared identity root. `identity/grants/<device_id>.cbor` now
also has a typed signed envelope with canonical CBOR encode/decode, signing, verification,
stable content-hash helpers, a remote-auth issuance helper that updates roster/index
state coherently, XChaCha20-based wrap/unwrap helpers for private-epoch material, and a
deterministic pairing-transcript helper that derives both the wrapping key and short auth
string from a shared pairing secret plus device identities. The wallet layer also now has
a typed pairing ticket/response seam for QR or manual-code transport, and Meerkat now has
an artifact-based omnibar host seam that can mint remote-auth pairing tickets and accept a
filled response artifact into grant issuance. That host seam is still manual/admin, but it
now requests `identity.act` plus `private.read`, loads the delegator's current plaintext
private epoch from a temporary per-persona bridge, wraps it into the grant, and exports an
enrollment artifact the delegatee can install. Meerkat can now also materialize the
delegated device side: it persists a local delegated-device identity bridge, caches the
pairing ticket locally, writes a filled response artifact from a scanned ticket, previews
the shared short auth string before grant issuance, and on install restores the signed
grant, persona wallet manifests, roster enrollment, grant index, and the current plaintext
private epoch against that local delegated-device identity. Remote-auth revocation can now
also mark a delegated device revoked, clear its persona wallet slot grants, block new
enrollment-bundle export, rotate future-write private epochs when that device had
`private.read`, and refresh the remaining pairing-backed delegated grants with new wrapped
epoch material for the rotated head. The identity seed, local delegated-device identity,
owner-side wrapping-key bridge, and temporary persona epoch bridge now all have
sealed-record migration paths under the new vault seam; the remaining gap is the actual
PAKE/QR chrome, transport UI around that shared secret, per-persona encryption-at-rest and
epoch-history usage beyond the current epoch, copy-mode export/import, and non-Windows
startup unlock backends.

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

1. **The secret root (tiny, must move securely).** Persona keys are derived, not stored.
   The v1 convention this doc and the transport companion both assume is:
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
  wants. The root is untouched. This is least-privilege and the only mode with clean
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
   delegation cert plus the wrapped private-epoch material the device needs to read the
   private lane.
4. **Enroll.** Append a signed `DeviceEnrollment {device_pubkey, mode, label, exposure}`
   to the synced `DeviceRoster`, the identity-level device fabric the transport plan uses
   for per-device exposure. Per-persona egress stays on `persona.json`.
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

`LocalOnly` needs one clarification here: in this carry model it means **persona-local,
not single-device-local**. A `LocalOnly` engram may sync to your authorized devices as
ciphertext plus wrapped key material, but it does not widen beyond that persona unless you
explicitly promote it.

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
| **0 Wallet** (storage landed; crypto/pairing unbuilt) | private | seed recovery policy + device roster + capability tokens + persona root refs; bulk synced by reference |
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

## The concrete v1 shape

The current persona plan already gives the per-persona root:
`<data_root>/personas/<persona_id>/persona.json`, with sibling `vault/`,
`settings/`, and `engine-profiles/` directories. The structural correction is that the
wallet is **not only per-persona**. The device fabric, grant set, and seed recovery policy
are identity-level; the root refs and private-lane epochs are persona-level. Keep the
split:

- `identity/wallet.json` is the identity-level carry root: master-seed recovery posture,
  the device roster, the grant index, and the set of known personas.
- `personas/<persona_id>/persona.json` stays the small, human-facing persona manifest:
  display name, durability, default engine-profile posture, and the transport plan's
  persona-level egress binding.
- `personas/<persona_id>/wallet.json` is the persona-scoped carry file: root refs, epoch
  history refs, and persona-scoped capability slots.

```text
<data_root>/
├── identity/
│   ├── wallet.json
│   ├── device-roster.json
│   └── grants/
│       └── <device_id>.cbor
└── personas/
    └── <persona_id>/
        ├── persona.json
        ├── wallet.json
        ├── vault/
        ├── settings/
        └── engine-profiles/
```

`identity/wallet.json` is the identity-level root. It carries only the small things that
must move with the identity as a whole:

```rust
struct IdentityWalletManifest {
    schema_version: u32,
    device_roster_ref: Hash,
    recovery_policy: RecoveryPolicy,
    personas: Vec<PersonaWalletRef>,
    grant_index: Vec<DeviceGrantRef>,
}
```

- `recovery_policy` belongs here because copy mode transfers the one master seed that
  derives every persona, not something owned by one persona directory.
- `personas` belongs here because a device delegation may cover a persona set, and that
  set needs one authoritative home.
- `grant_index` belongs here for the same reason: one grant may authorize several
  personas, so it should not be duplicated under each persona.

`personas/<persona_id>/wallet.json` is the persona-scoped carry file:

```rust
struct PersonaWalletManifest {
    schema_version: u32,
    persona_id: PersonaId,
    chain_root: PersonaChainRoot,
    private_epoch_head: KeyEpochId,
    epoch_history_ref: Hash,
    private_roots: PrivateRoots,
    public_roots: PublicRoots,
    capability_slots: Vec<CapabilitySlotRef>,
}
```

- `PrivateRoots` are references into the encrypted lane: the current eidetic root CID,
  any typed subtree roots we decide are first-class, and later a wallet-local cursor for
  faster restore. These are *pointers*, not bulk data.
- `PublicRoots` are references into the cleartext lane: public/moot-portable roots whose
  bytes can sync and verify without a decryption key.
- `private_epoch_head` is the current wrapped-key epoch handle for new writes, not the key
  bytes.
- `epoch_history_ref` is required because restore and revocation rotate across many epochs
  over time. Reading existing private engrams means either carrying readable historical
  wraps or carrying the indirection that can re-wrap them; "current epoch only" is not
  enough.
- `capability_slots` are the persona-scoped signed grants the wallet knows about:
  cluster-path read caps now, later moot-scoped capability bundles.

`identity/device-roster.json` is the synced fabric the transport plan already wants. It
owns membership, exposure, and revocation, not the per-persona wallet:

```rust
struct DeviceRoster {
    schema_version: u32,
    devices: Vec<DeviceRecord>,
    revoked: Vec<DeviceId>,
}

struct DeviceRecord {
    device_id: DeviceId,
    device_pubkey: Ed25519PublicKey,
    label: String,
    mode: DeviceMode,          // Copy | RemoteAuth
    exposure: DeviceExposure,  // HiddenClient | ExposedEgress
    grant_ref: Option<Hash>,
}
```

Each remote-auth device gets one grant file under `identity/grants/`. That grant has two
parts and the current draft only named one of them:

1. **Authorization** — the meadowcap-style delegation cert: what this device may do, for
   how long, and for which persona.
2. **Decryption material** — the current private-lane epoch key, wrapped to that device,
   so it can actually read `LocalOnly` / `TrustedPeersOnly` material without receiving the
   master seed.

That split is the keystone. Without it, "encrypt at rest" and "remote-auth device" fight
each other. The seed stays put; the device gets a signed grant plus the wrapped private
epoch material it needs. Because the grant can span a persona set, the grant's home is the
identity-level wallet root, not one persona's directory.

## Carry modes, clarified by key material

The earlier copy-vs-remote-auth split is right, but it needs the key story stated
directly:

- **Copy mode** receives the 32-byte master seed. The device can derive every persona
  signing key and every future wallet key forever. Revocation means master rotation.
- **Remote-auth mode** never receives the seed. It mints its own device keypair, receives
  a signed delegation cert, and receives only the *current* wrapped private-lane epoch
  material plus whatever scope-specific caps the grant includes.

This yields a clean revocation story:

- Revoke a remote-auth device: append it to `device-roster.json`, stop syncing new private
  epochs to it, rotate the private epoch for future writes, and re-wrap the new epoch for
  the remaining devices. Only the timing of that re-wrap is optional: eager at revocation
  time or lazy on next unlock/write.
- Revoke a copy device: you cannot. Rotate the master root and re-attest.

Past ciphertext already handed to a revoked remote-auth device remains readable if it has
the old epoch. That is not a flaw in the plan; it is the honest "no retroactive recall"
boundary stated elsewhere in this doc.

## The encrypted vault seam (designed 2026-07-04)

The vault replaces the transitional plaintext bridges (`identity/master.seed`,
`identity/local-device.json`, `identity/remote-auth-wrapping-keys.json`,
`personas/<id>/private-epoch-bridge.json`). Design fixed here before the
implementation slice; the shape is Firefox-like (typed metadata outside,
key-protected secrets inside), with an Anytype-like key hierarchy and
browser-like optional OS integration.

### Boundary

- `session-runtime::wallet_store` stays the typed clear-metadata layer:
  manifests, roster, grant refs, paths. Manifests point at sealed records and
  never contain secret bytes.
- The sealed-record store is a separate concern owning secret material, unlock
  policy, and record formats. **Decided (2026-07-04): it lands as a
  sealed-record backend inside `crates/persona/identity`, beside the existing
  slot vault, and `session-runtime` consumes it** (the dependency edge already
  exists: `session-runtime` depends on `identity`). No new crate. The rule
  this placement enforces: **one unlock ladder** — the record backend shares
  identity's `PassphraseEncryptedStorage` substrate (Argon2id +
  ChaCha20-Poly1305), `UnlockTier` vocabulary, and zeroize discipline. Two
  parallel KDF/unlock stacks is the failure mode. One vault root, one unlock
  ceremony, one KDF config; record classes reuse `UnlockTier`.
- Small versioned sealed records, not one blob: `identity/vault/*.cbor` and
  `personas/<id>/vault/*.cbor`, each AEAD-sealed (`xchacha20poly1305-v1`, as
  `wallet_grant` already does) with typed AAD naming the record kind, owner,
  and schema version. Epoch history is append-shaped from the start:
  `personas/<id>/vault/epochs/<epoch_id>.cbor` per epoch, matching
  `epoch_history_ref` and keeping revocation-driven rotation cheap. The
  single-file whole-rewrite backend stays fine for credential slots; it is not
  the shape for rotating history.

### Two ladders, never entangled

- **Identity ladder (unchanged):** persona signing keys derive from the master
  seed via `BLAKE3("persona" || persona_id)`, and only from the seed. A fresh
  device holding the seed re-derives identical personas. Nothing in the vault
  participates in identity derivation.
- **Encryption ladder (the vault):** passphrase (Argon2id) unlocks a
  **per-device vault root key**; the vault root wraps the locally stored seed
  and local-only device records; seed-derived subkeys seal everything that
  syncs. The vault root never syncs and never derives identity.

### The sync invariant

Anything syncable is sealed under keys derived from the master seed, never
under the vault root. Otherwise fresh-device restore is circular (the vault
root needed to read the record only exists on the old device). Restore order
on a fresh device: obtain the seed (recovery phrase or copy-mode pairing),
re-derive wrap keys, read the synced sealed records, then mint a new local
vault root from the new device's passphrase. Record classes:

- **Syncable identity secrets** (seed-derived wrap): recovery wraps,
  remote-auth wrapping keys the owner devices need for refresh/revoke.
- **Syncable persona secrets** (seed-derived wrap): private epoch history,
  future content-key history.
- **Local-only device secrets** (vault-root wrap, never sync): this install's
  delegated-device private key, cached pairing ticket, convenience-unlock
  state. Syncing a delegated-device key would collapse remote-auth's
  least-privilege story.

Two distinct artifacts, distinct lifetimes: the **recovery phrase** encodes
the master seed (BIP39-shaped, loss means master rotation); the **passphrase**
unlocks one device's vault (loss means re-enrolling that device).

### Unlock policy is the first implementation decision

Meerkat's bootstrap currently reads `identity/master.seed` at startup to bring
up sync/comms. Once sealed, every launch needs one of:

1. **OS-keychain silent unlock** (DPAPI / Keychain / libsecret wraps the vault
   root): convenience layer, never the root of trust; on Linux without a
   secure store, fall back to prompting, never to plaintext.
2. **Passphrase prompt in chrome**: does not exist yet; needs a locked-mode
   startup state.
3. **Locked degraded mode**: host runs, sync/comms/private-lane come up on
   unlock.

Exposed as a setting (auto-unlock via OS store / prompt at launch / stay
locked), not a hardcoded pick. Choosing (1) as the shipped default makes the
OS wrap effectively mandatory on day one; say that in the setting's own copy.
First implementation slice landed 2026-07-04: `StartupUnlockMode` now exists
in `persona/identity`, `session-runtime` currently defaults to `AutoOs`, and
Windows DPAPI is the first implemented `AutoOs` backend. `Prompt` and
`Locked` are now surfaced in Meerkat's `pelt/wallet` settings page as persisted
startup policies. A follow-on slice now adds an explicit `Unlock now with OS store`
action for the current launch; full passphrase prompt chrome is still pending.

### Retention policy for the wrapping-key bridge

The revocation-refresh slice retains pairing-derived wrapping keys so
still-authorized grants can be re-signed with rotated epoch material. Retained
wrapping keys are standing secrets: a machine compromised while unlocked can
use them to mint enrollments. The vault must carry an explicit retention
policy for them (indefinite / time-boxed / re-derive per ceremony) as a
first-class field, not inherit the bridge's implicit indefinite retention.
Default leans time-boxed; the tradeoff is silent grant refresh vs standing
mint capability.

### API sketch (typed and boring, illustrative signatures only)

`unlock_identity_vault(..)`, `load_master_seed()`,
`store_local_device_identity(..)`, `store_remote_auth_wrapping_key(..)`,
`load_current_epoch(persona_id)`, `rotate_epoch(persona_id)`,
`export_copy_bundle(..)`. Grant validation keeps riding the existing
`wallet_grant` helpers (`private.read` requires wrapped epoch material).

### Non-goals

- No whole-wallet blob encryption: it breaks grant refresh, persona-scoped
  export, and diffable recovery.
- Local compromise of an unlocked machine stays a real attack surface
  (Anytype states the same honestly); the vault narrows at-rest exposure, it
  does not solve endpoint compromise.
- Once the vault lands, the host stops reading plaintext `master.seed` and
  `private-epoch-bridge.json`; the bridges become migration shims only, then
  get deleted.

## Exists vs gap

**Built, shipped, or already source-grounded:** persona identity (master Ed25519 +
BLAKE3 derivation); content-addressed engrams with `PrivacyClass` (cleartext today); the
persona vault with `PassphraseEncryptedStorage` (Argon2id + ChaCha20-Poly1305) for
credentials; the tessera ledger + gate; the moot roster + flora; the transport (iroh,
per-persona NodeId optional); the new `session-runtime::wallet_store` module (identity
wallet, device roster, persona wallet, grant paths, and tests); and the chosen lane
engines `p2panda-auth` (cleartext gating) and `p2panda-encryption` (the private lane),
both evaluated in the substrate spike.

**Gap to build:**

1. **The wallet store** — an identity-level root (`identity/wallet.json`,
   `device-roster.json`, `grants/<device_id>`) plus per-persona `wallet.json` files under
   `personas/<persona_id>/`, with typed load/save APIs. **Storage v1a plus first host
   adoption landed 2026-07-02** in `session-runtime::wallet_store` and Meerkat startup:
   identity seed/path helpers, deterministic persona chain-root derivation, startup/session-load
   bootstrap of wallet + roster + persona wallet, and `sync`/`comms` now sharing the
   identity root instead of separate ad hoc seed files. **First encrypted-vault host
   adoption landed 2026-07-04**: `identity/master.seed` and
   `identity/local-device.json` now auto-migrate into sealed local records when a local
   vault root is available, `identity/remote-auth-wrapping-keys.json` now also
   self-migrates into a seed-derived sealed record, the temporary
   `personas/<id>/private-epoch-bridge.json` now migrates into a sealed record using the
   seed-derived store on copy roots and the local vault-root store on delegated installs,
   and `session-runtime` now has a typed startup unlock mode seam with a Windows-first
   `AutoOs` backend. Still open: exposing unlock mode as a setting/chrome flow and
   non-Windows `AutoOs` backends.
2. **Per-persona encryption-at-rest for eidetic** — private-lane payloads sealed under a
   persona-owned private epoch history, with per-device wrapped copies for remote-auth
   devices. Today privacy is a metadata tag, encryption is wire-only. **The eidetic-core
   seal seam landed 2026-07-08** (`eidetic::seal`): a `PayloadSealer` boundary, seal-on-
   write / unseal-on-read helpers keyed on `PrivacyClass`, and a `resolve_sealed_blob`
   that unseals before the content-hash check. Still open: the host `PayloadSealer` impl
   over the wallet epoch keys (`session-runtime`) and the meerkat wiring at the store-open
   / ingest sites (both held by the one-state migration), plus migrating existing cleartext
   private blobs.
3. **The capability-token layer** — typed signed device-grant storage, remote-auth grant
   issuance, wrapped private-epoch crypto helpers, pairing-transcript derivation, and a
   typed pairing ticket/response seam landed 2026-07-02 in `session-runtime::wallet_grant`
   (canonical CBOR envelope, signing, verification, content-hash refs, wallet/roster
   persistence over `identity/grants/<device_id>.cbor`, XChaCha20 wrap/unwrap helpers for
   the remote-auth private lane, deterministic wrapping-key + SAS derivation from a shared
   pairing secret plus device identities, and CBOR pairing-ticket transport plus manual
   code formatting/parsing). Still open: the actual meadowcap-shaped delegation vocabulary,
   cluster-path read caps, and later Biscuit re-expression when the constitution's
   authorize seam graduates.
4. **The pairing ceremony** — PAKE + SAS + roster enrollment + the copy/remote-auth fork,
   plus emitting the wrapped-key half of the remote-auth grant. A first Meerkat host seam
   now exists for ticket artifacts, response ingestion, `private.read` grant wrapping, and
   current-epoch restore on the delegatee side, but it is still a manual/admin path and the
   actual PAKE/QR chrome is still open.
5. **Cross-device persona restoration** — wallet export/import: seed recovery, root-CID
   restore, private-epoch restore, standing re-fold. A first remote-auth enrollment
   bundle now restores signed grant, persona wallet manifests, and the current wrapped
   private epoch onto the delegatee side, but copy-mode export/import, epoch-history
   restore, and standing re-fold are still open.

## Sequencing

1. **Identity root + persona wallet store.** Keep `persona.json` small; add an
   identity-level wallet root for the device fabric and grants, plus per-persona
   `wallet.json` files for root refs and epoch history. Thread typed repositories through
   the host the same way `ManifestStore` and persona settings already work.
   Done when: the identity root can persist and reload device membership, grant refs, and
   recovery posture once, while each persona root persists its own refs without duplicating
   the same `DeviceRecord` or grant across persona directories.
2. **Pairing ceremony**, remote-auth default. PAKE + SAS, then either seed copy or
   meadowcap delegation plus wrapped private epoch, with enrollment in `DeviceRoster`.
   Done when: a newly paired remote-auth device can restore a persona root, read existing
   private engrams, and be revoked without rotating the master seed.
3. **Per-persona encryption-at-rest** for the private lane (`p2panda-encryption`
   envelope posture). `LocalOnly` / `TrustedPeersOnly` engrams are sealed on disk under the
   current private epoch; the wallet stores wrapped epoch history per authorized device.
   Done when: private engrams at rest are unreadable without either the seed or a valid
   remote-auth wrapped-key bundle, and restore can still read pre-rotation content.
4. **The public lane wired through replication.** Honor `PrivacyClass` at the
   `p2panda-auth` gate (who syncs cleartext) for promoted `MootScoped` / `PublicPortable`
   engrams; promotion is the decrypt-and-recommit step with explicit provenance.
   Done when: publishing a private engram to a moot produces a new cleartext object in the
   public lane, rather than silently widening the original object's readership.
5. **Capabilities as first-class wallet slots.** Meadowcap grants now, Biscuit later,
   unified under `capability_slots`.
   Done when: device grants, cluster-path read grants, and future moot-scoped grants all
   ride the same wallet slot surface even if their internal token format later changes.
   V1 scope: `TrustedPeersOnly` wrapping only needs to reach your own device roster.
   Cross-persona / other-human peer wrapping is a later contact-identity lane.
6. **Recovery + rotation.** Recovery phrase / device handover for the seed, export/import
   of the wallet manifest, standing re-fold, private-epoch rotation after revocation, and
   master rotation only for copy-device loss.
   Done when: a user can recover onto a fresh device, revoke a delegated device, and
   understand from the product surface which action rotated an epoch versus which action
   rotated the root.

## Decisions for v1

- **FROST custody is a later hardening tier, not a v1 blocker.** v1 needs the wallet store,
  remote-auth grants, and private-epoch rotation first. A threshold-held master root is a
  good follow-on once the ordinary copy-vs-remote-auth path exists and the user can already
  recover and revoke cleanly.
- **Decided (2026-06-25): encrypt at rest.** `LocalOnly` / `TrustedPeersOnly` engrams are
  sealed on disk under the persona vault key (private by default; the wallet leans keyring),
  and promotion to `MootScoped` / `PublicPortable` re-encodes into the cleartext-gated lane
  so the moot can dedup / pin / verify. `PrivacyClass` is therefore the storage regime, and
  "publish to a moot" is a decrypt-and-recommit step.
- **`TrustedPeersOnly` stays in the encrypted lane.** It is not "cleartext gated to a short
  allowlist"; it is private material whose keys are handed to named readers. For v1, that
  reader set is your own authorized device roster; wrapping to external peers is a later
  contact-identity lane. Otherwise the private-vs-public dial becomes mushy.
- **Salted private encryption is the v1 default.** Dedup is the public lane's advantage.
  Convergent encryption is optional and later, only for explicitly high-entropy schemas
  where the confirmation attack is acceptable.
- The wallet-sync bootstrap: the wallet is itself data that syncs over the personal mesh,
  but syncing it needs the identity, so the seed is the one thing transported out of band
  (recovery phrase vs device-pairing handover).

## Remaining open questions

- Whether `wallet.json` should carry only root refs, with every rotating detail in sibling
  files, or whether a few tiny hot fields (`private_epoch`, last-restore cursor) belong in
  the root manifest for faster restore.
- Whether remote-auth grants should carry one wrapped private epoch per device directly or
  a small indirection through a re-wrapable key slot record. The first shape is simpler; the
  second may make bulk rotation cheaper.
- How much of the wallet export is one bundle versus a few explicit artifacts
  (recovery phrase, `wallet.json`, device roster, grant set) the user can inspect and move
  separately.

## Progress

- **2026-06-25** — initial design/research pass: carry-by-reference model, copy vs
  remote-auth ceremony, cleartext public lane vs encrypted private lane, and the
  encrypt-at-rest decision.
- **2026-07-02** — reconciled the structural contradiction between identity-level device
  fabric and per-persona wallet placement: device roster + grants + recovery policy moved
  to an identity-level wallet root; per-persona wallet files now hold only persona-scoped
  refs and epoch history. Also clarified `LocalOnly`, aligned the persona-derivation
  convention, scoped `TrustedPeersOnly` v1 to own devices, and made epoch-history support
  an explicit restore requirement.
- **2026-07-02** — implemented the first code slice in
  `crates/system/session-runtime/src/wallet_store.rs`: typed `IdentityWalletManifest`,
  `DeviceRoster`, and `PersonaWalletManifest` shapes; path helpers; atomic load/save for
  identity wallet, persona wallet, roster, and opaque grant blobs; plus 8 focused tests.
  Verified with `cargo test -p session-runtime wallet_store -- --nocapture`.
- **2026-07-02** — landed the next host-adoption slice in Meerkat: a shared
  `identity/master.seed` bridge under the wallet root, deterministic
  `BLAKE3("persona" || persona_id)` persona-chain derivation, startup/session-load wallet
  bootstrap, `active_persona` correction on session load, and `sync`/`comms` now reading the
  shared identity root instead of separate `node_identity.seed` / `comms_identity.seed`
  sidecars. Verified with `cargo check -p meerkat`.
- **2026-07-02** — landed the typed grant slice in
  `crates/system/session-runtime/src/wallet_grant.rs`: a signed
  `identity/grants/<device_id>.cbor` envelope, canonical CBOR encode/decode helpers,
  delegator-signing and verification helpers, stable content-hash refs, and load/save
  helpers/tests over the existing grant path. This narrows the live gap to pairing,
  wrapped-key emission, and the broader capability language above the storage seam.
  Verified with `cargo test -p session-runtime wallet_grant -- --nocapture`.
- **2026-07-02** — landed the remote-auth grant issuance slice in the same module:
  issuing a device grant from the shared wallet root now persists the signed grant, updates
  the enrolled `DeviceRoster` entry, records `grant_index` on the identity wallet, and
  keeps `device_roster_ref` coherent instead of leaving signed grant files detached from
  wallet state. Validation currently enforces "known persona wallet only" plus wrapped-epoch
  persona-set consistency. Pairing UX and wrapped-key production still layer on top.
  Verified with `cargo test -p session-runtime wallet_grant -- --nocapture`.
- **2026-07-02** — landed the wrapped private-epoch crypto helper slice in the same module:
  `wrap_private_epoch_material(...)` / `unwrap_private_epoch_material(...)` now seal remote-auth
  epoch secrets under a caller-supplied 32-byte wrapping key using
  `xchacha20poly1305-v1`, with persona+epoch AAD binding and grant validation that
  requires wrapped epoch material whenever a device grant carries `private.read`.
  This moves the live gap from "how do we represent wrapped key material?" to "how does
  pairing derive and transport the wrapping key?" Verified with
  `cargo test -p session-runtime wallet_grant -- --nocapture`.
- **2026-07-02** — landed the pairing-transcript helper slice in the same module:
  `derive_remote_auth_pairing_material(...)` now derives both the wrapping key and a
  6-digit short auth string from a shared pairing secret plus the delegator pubkey,
  delegatee pubkey, and device id, and
  `issue_remote_auth_device_grant_from_pairing(...)` now wraps plaintext private epochs
  and emits the signed remote-auth grant in one call. This moves the live gap from "how
  does pairing represent its output?" to "where do PAKE / QR / SAS confirmation actually
  run in the host?" Verified with `cargo test -p session-runtime wallet_grant -- --nocapture`.
- **2026-07-02** — landed the pairing ticket/code helper slice in the same module:
  `mint_remote_auth_pairing_ticket(...)`, CBOR encode/decode for QR transport,
  `format_remote_auth_pairing_code(...)` / `parse_remote_auth_pairing_code(...)` for manual
  entry, and `issue_remote_auth_device_grant_from_ticket(...)` bridging the ticket +
  delegatee response into the pairing-backed issuance path. This moves the live gap from
  "what payload crosses the host pairing boundary?" to "where do scanning, code entry,
  and PAKE/SAS confirmation live in Meerkat?" Verified with
  `cargo test -p session-runtime wallet_grant -- --nocapture`.
- **2026-07-02** — landed the first Meerkat pairing-host slice: the omnibar command shell
  now records `pair_remote_auth()` and `accept_remote_auth_pairing(ticket, response)`,
  Meerkat writes pairing artifacts under `<mere_root>/pairing/`, and the host can ingest a
  filled response artifact into remote-auth grant issuance plus roster/grant persistence.
  This is intentionally the narrow host seam: at that slice it only minted
  `identity.act`-only remote-auth tickets and still blocked `private.read`.
  Verified with focused `meerkat` + `session-runtime` tests.
- **2026-07-02** — landed the next pairing-host slice: `session-runtime::wallet_store`
  now has a typed local delegated-device identity bridge, pairing offer artifacts carry the
  delegator pubkey in their JSON summary, and Meerkat can now
  `respond_remote_auth_pairing(ticket)` on the delegatee side plus
  `preview_remote_auth_pairing(ticket, response)` on the delegator side before
  `accept_remote_auth_pairing(...)`. This closes the host gap around response generation and
  SAS derivation for `identity.act` grants, leaving the actual PAKE/QR chrome and
  `private.read` epoch handoff as the next live seams. Verified in `session-runtime`; current
  full-workspace `meerkat` verification is blocked by an unrelated dirty-tree `kernel`
  compile error in `graph/apply.rs`.
- **2026-07-02** — landed the enrollment-bundle slice in
  `crates/system/session-runtime/src/wallet_grant.rs`: typed
  `RemoteAuthEnrollmentBundle` CBOR encode/decode helpers, delegator-side
  `build_remote_auth_enrollment_bundle(...)`, and delegatee-side
  `install_remote_auth_enrollment_bundle(...)` now bridge signed grant + persona wallet
  manifests into restored wallet state keyed to the local delegated-device identity.
  This narrowed the carry gap from "how does the delegatee get enough state to restore the
  granted personas?" to the remaining startup/load policy split and the still-unbuilt
  private-epoch restore lane.
- **2026-07-02** — landed the next Meerkat host slice over that bundle seam:
  `accept_remote_auth_pairing(...)` now exports a concrete enrollment artifact under
  `<mere_root>/pairing/`, `install_remote_auth_enrollment(bundle)` now installs it on the
  delegatee side, and startup/session-load now call
  `session_runtime::bootstrap_wallet_state(...)` so a pending or enrolled delegated device
  is preserved instead of being clobbered by copy-mode `identity/master.seed` bootstrap.
  This closes the immediate host gap around "accept there, restore here" for
  `identity.act` grants, leaving the actual PAKE/QR chrome, `private.read` epoch handoff,
  and the temporary plaintext seed/device-identity bridges as the live seams. Current
  package/workspace verification is blocked by an unrelated dirty-tree `graph-kernel`
  compile error in `graph/capture.rs` (`PersistedField` / `PersistedCoupling` missing
  `PartialEq`).
- **2026-07-02** — landed the first `private.read` host/restore slice: Meerkat pairing
  now loads the delegator's current plaintext private epoch from the temporary per-persona
  bridge, requests `private.read` in `pair_remote_auth()`, wraps that epoch into the
  signed remote-auth grant, caches the pairing ticket on the delegatee side, and uses the
cached ticket to recover the pairing-derived wrapping key during
`install_remote_auth_enrollment(bundle)` so the delegatee restores the current plaintext
epoch bridge alongside the signed grant and persona wallet manifests. This closes the
immediate "grant can carry wrapped epochs but install cannot use them" gap, leaving the
actual PAKE/QR chrome, encrypted-at-rest integration, epoch-history restore beyond the
current head, and replacement of the temporary plaintext bridges as the live seams.
- **2026-07-02** — landed the next pairing hardening slice: runtime grant issuance now
  rejects expired pairing tickets, enrollment install now rejects expired signed grants,
  delegatee response preparation / preview / acceptance now reject expired tickets on the
  host side, and Meerkat now requires the response artifact's `ticket_id` to match the
  ticket being previewed or accepted instead of silently accepting cross-ticket mixups.
  This does not add the actual PAKE/QR chrome yet, but it closes the integrity gap around
  stale or mismatched artifact reuse in the manual pairing seam.
- **2026-07-03** — landed the first capability-slot wiring slice: remote-auth device grants
  now also populate a stable per-persona wallet slot
  (`capability_slots += "device-grant:<device_id>" -> grant_ref`) on both the delegator
  side during issuance and the delegatee side during enrollment install, including
  multi-persona grants. This closes the gap where `identity/grant_index` tracked the grant
  globally but the persona wallet surface still had no first-class slot entry for the same
  capability. Meadowcap vocabulary, cluster-path read caps, and later Biscuit
  re-expression remain open above that slot surface.
- **2026-07-03** — landed the first delegated-device revocation slice:
  `session-runtime::revoke_remote_auth_device(...)` now verifies the signed grant, rejects
  copy-mode devices, appends the delegated device to `device-roster.json`'s revocation set,
  clears the active `device-grant:<device_id>` persona wallet slots, rotates the current
  private epoch head for granted personas when the revoked grant had `private.read`, and
  blocks new enrollment-bundle export for revoked devices. Meerkat's manual/admin omnibar
  seam now also records `revoke_remote_auth_device("<device-id>")` and routes it through
  that runtime path.
- **2026-07-03** — landed the next carry-layer refresh slice:
  pairing-backed remote-auth grant issuance now retains a temporary identity-level
  wrapping-key bridge, and delegated-device revocation now uses that bridge to re-sign the
  still-authorized remote-auth grants with fresh wrapped epoch material for the rotated
  `private_epoch_head`, updating roster grant refs, identity `grant_index`, and persona
  `capability_slots` coherently. This closes the immediate "rotate locally but strand the
  remaining delegated devices on the old epoch" gap. Focused runtime tests now also prove
  that the revoked device is blocked at enrollment export while a still-authorized device
  can install the refreshed enrollment bundle and recover the rotated current epoch.
  The encrypted vault replacement for that wrapping-key bridge is still open.
- **2026-07-03** — landed the next Meerkat admin-host slice:
  the omnibar seam can now also `remote_auth_devices()` to list the known delegated devices
  with their UUID, label, active-vs-revoked state, exposure, and whether the current grant
  carries `private.read`. This does not replace the eventual roster UI, but it closes the
  raw-discoverability gap around `revoke_remote_auth_device("<uuid>")` in the manual/admin
  host path.
- **2026-07-03** — landed the next Meerkat admin-host slice:
  the omnibar seam can now also `export_remote_auth_enrollment("<device-id>")` to emit a
  fresh enrollment bundle for an already-enrolled delegated device. Pairing-backed
  `private.read` grants now retain their original pairing `ticket_id` in the temporary
  wrapping-key bridge so that refreshed enrollment bundles still carry the right restore
  metadata for the delegatee side. This keeps the manual/admin recovery path viable after
  revocation-triggered epoch rotation, without pretending the encrypted-vault replacement
  for that bridge is done.
- **2026-07-04** — designed the encrypted vault seam (new section above), from an agent
  design take reviewed against the code. Key fixes over the raw take: one unlock ladder
  (decided with Mark same day: a sealed-record backend inside `persona/identity`, consumed
  by `session-runtime`; no new crate), identity ladder vs encryption ladder kept strictly separate (persona
  keys derive from the seed only; vault subkeys only seal), the sync invariant (per-device
  vault root never syncs; syncable records sealed under seed-derived keys, which is what
  makes fresh-device restore non-circular), startup unlock policy named as the first
  implementation decision and exposed as a setting, and an explicit retention policy for
  the pairing wrapping-key bridge. Record format: per-record CBOR AEAD
  (`xchacha20poly1305-v1` + typed AAD), append-shaped epoch history. This was the design
  checkpoint before the code slice below landed later the same day.
- **2026-07-04** — landed the first encrypted-vault implementation slice:
  `crates/persona/identity/src/sealed_record_storage.rs` now provides a typed sealed-record
  backend, `crates/persona/identity/src/startup_unlock.rs` now declares
  `StartupUnlockMode::{AutoOs, Prompt, Locked}` and implements a Windows DPAPI-backed
  local vault root for `AutoOs`, and `session-runtime::wallet_store` now uses that local
  secret store to auto-migrate `identity/master.seed` and `identity/local-device.json`
  into sealed records on read/write. The owner-side
  `identity/remote-auth-wrapping-keys.json` bridge now also migrates into the seed-derived
  sealed-record path. Verified with focused `identity` tests plus `cargo check -p
  session-runtime --lib` and `cargo check -p meerkat --bin meerkat`. Prompt/locked chrome,
  non-Windows `AutoOs`, and the per-persona epoch bridge remain open.
- **2026-07-04** — landed the next encrypted-vault migration slice:
  `session-runtime::wallet_store` now also seals the temporary
  `personas/<id>/private-epoch-bridge.json` record. Copy-seeded roots use the
  seed-derived sealed-record store; delegated installs without the master seed fall back to
  the local vault-root store so the currently granted private epoch still rests encrypted at
  rest on the delegatee side. Legacy plaintext epoch-bridge JSON now auto-migrates on
  read when a sealing backend is available. Verified with `cargo check -p session-runtime
  --lib`, `cargo check -p meerkat --bin meerkat`, and focused `wallet_store` tests.
- **2026-07-04** — landed the first startup-unlock flow slice:
  `settings.json` now persists `StartupUnlockMode`, Meerkat now exposes that as the
  `pelt/wallet` settings page (`auto_os` / `prompt` / `locked`), wallet bootstrap now
  classifies sealed-but-unavailable local secrets as `Locked` instead of pretending they are
  absent, and `sync` / `comms` now stay offline rather than silently minting an ephemeral
  identity when the copy-root seed is sealed but locked. Full unlock-after-launch chrome
  still remains open.
- **2026-07-04** — landed the next unlock flow slice:
  `session-runtime` now carries a session-scoped explicit unlock override, so a locked
  launch can open device-local sealed records with the OS store without rewriting the
  persisted startup policy. Meerkat's `pelt/wallet` page now shows current locked/unlocked
  state plus an `Unlock now with OS store` action, and the long-lived `sync` / `comms`
  actors now retry backend setup on `UnlockNow` instead of dying permanently at boot.
- **2026-07-04** — landed the relock follow-on:
  the session-scoped override can now be cleared again without touching the persisted startup
  mode, Meerkat's wallet page now shows `Lock now` after a prompt/locked launch was explicitly
  unlocked, and the sync/comms actors now publish an offline transition when that relock fires
  so the toolbar chip and comms pane do not keep stale success state. Still open:
  passphrase-entry chrome for `Prompt` and delegated device follow-through beyond the
  seed-backed lanes.
- **2026-07-06** — landed the passphrase-wrapped vault-root backend in
  `crates/persona/identity/src/passphrase_root.rs`: an Argon2id-KEK + ChaCha20-Poly1305
  seal over the same 32-byte vault root the DPAPI `AutoOs` wrapper produces, reusing
  `passphrase_storage::derive_kek` (one unlock ladder, one KDF config). Public API is
  `wrap`/`unwrap_vault_root`, `save`/`load_passphrase_root`, `change_passphrase`,
  `passphrase_root_exists`; the root is an explicit input so enrollment re-wraps an
  existing root rather than minting a new one (the dual-wrapper model: OS store and
  passphrase over one root, so a device can carry both). 9 tests including a round-trip
  proving a passphrase-unlocked root opens a `SealedRecordStorage` and seals
  `identity/master.seed`. This is the cross-platform root of trust (DPAPI is Windows-only)
  and the backend the dead `Prompt` / `Locked` modes needed. Deliberately isolated to
  `persona/identity` (untouched by the in-flight one-state migration). Still open and
  deferred behind that migration: the passphrase-entry chrome, and the `session-runtime`
  wiring that routes `StartupUnlockMode::Prompt` through `load_passphrase_root` plus the
  enrollment path that re-wraps an existing `AutoOs` root under a new passphrase (both
  touch meerkat UI files the migration currently holds). Verified `cargo test -p identity`
  green + clippy clean.
- **2026-07-08** — landed the eidetic-core encrypt-at-rest seam (gap #2's foundation) in
  `crates/eidetic/eidetic-core/src/seal.rs`: a `PayloadSealer` trait (seal/unseal, the host
  owns the key so the private-memory core stays crypto-free and wasm-friendly, mirroring the
  `BlobFetcher` split), `seal_payload_for_store` (seals `LocalOnly` / `TrustedPeersOnly`,
  passes `MootScoped` / `PublicPortable` through cleartext for the public lane's dedup/pin/
  verify), and `resolve_sealed_blob` (unseals a marked blob before the content-hash check;
  a sealed manifest with no sealer is a hard error, and `resolve_blob` now rejects sealed
  manifests loudly instead of a confusing hash mismatch). Kept purely additive: the seal
  marker (`SealedBlobRef { epoch, format }`) rides the existing `schema_metadata` under a
  reserved key rather than a new `BlobManifest` field, because a field fans out to
  construction sites in migration-held `ingest.rs`; access is encapsulated so promoting it
  to a typed field later is one spot. `SealEpochId` is eidetic's own 16-byte epoch id
  (maps to the wallet's `KeyEpochId` without depending upward). 10 tests incl. private
  round-trip, public-stays-cleartext, pre-rotation read from epoch history, tamper/wrong-key
  rejection, and back-compat (unmarked = cleartext). chacha20poly1305 is a dev-dep only (the
  reference test sealer); the production impl is the host's. Still open and deferred behind
  the one-state migration: the host `PayloadSealer` over the wallet epochs, the meerkat
  store-open/ingest wiring, and migrating existing cleartext private blobs. Verified
  `cargo test -p eidetic` (83) green + new code clippy-clean.

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
- The wallet (Layer 0) was the one genuinely unbuilt carry primitive when this plan was
  drafted; the storage seam, host adoption bridge, signed-grant seam, wrapped-key crypto
  helper seam, pairing-transcript seam, and pairing ticket/code seam have now landed,
  leaving the actual PAKE exchange, encrypted-at-rest integration, and pairing-host flow
  as the live gaps.
