# personae — Founding

**Date:** 2026-07-08
**Status:** founded. The identity core is ported verbatim from mere's
`persona/identity`; the crate builds and tests standalone (32 tests + doctest).
mere keeps its in-tree `persona/identity` until a deliberate re-base (the
chartulary doctrine: fresh/lifted core, the app re-bases last).

## What personae is

The identity **and carry** layer for the Merely ecosystem (mere, isometry,
strophe, woodshed). A person has *personae*, plural — a work face, a research
face, a burner — so the crate is the register of those faces and the root of
trust they derive from. It is the trust-plane spine, companion to the
[chartulary](https://github.com/mark-ik/chartulary) data-plane substrate; see
mere's `2026-07-08_signet_trust_plane_plan.md` for the two-plane model.

## personae subsumes signet

The trust-plane plan proposed two crates: `personae` (identity) and `signet`
(the wallet carry layer). Collapsed 2026-07-08: **one crate.** `signet` was
taken on crates.io and needed the `signaculum` publish-alias workaround;
`personae` is free, and the wallet plan's own framing — "a persona is just the
carry layer's root-of-trust instance" — means the faces and how they carry
belong together. So personae holds both: the keys now, the carry layer as it
lifts out of mere. `signet` / `signaculum` is dropped.

## Ported now (v0.1, the identity core)

From `persona/identity`, verbatim: the master Ed25519 keypair + deterministic
per-protocol derivation (`IdentityProvider`, `BLAKE3-keyed(seed, salt)`); the
`IdentityVault` profile/slot store; `PassphraseEncryptedStorage` (Argon2id +
ChaCha20-Poly1305); the sealed-record store; the passphrase-wrapped vault root
and the `StartupUnlockMode` OS-store unlock (Windows DPAPI today); the reusable
`SealedIdentityProvider` for app-selected sealed records;
`DerivedKeyAttestation` for master-authorized protocol keys; and `PersonaId`.

## Roadmap

1. **Carry layer.** ~~Lift mere's `session-runtime::wallet_store` +
   `wallet_grant` ... into personae.~~ **Done 2026-08-10, and narrower than
   this line promised** (mere's `2026-08-10_wallet_carry_foldin_plan.md`).
   `personae::carry` took the *model*: the identity and persona wallet
   manifests, the device roster, private-epoch records, and derivation. Three
   pieces deliberately stayed in mere, each for a reason found by doing it:

   - **The store adapter.** Path layout, device-settings policy, sealed-record
     wiring, and bootstrap are sequenced filesystem effects, not portable model.
   - **The `WalletEpochSealer`.** It implements eidetic's `PayloadSealer`, so it
     *is* the seal seam of roadmap item 2; personae owns the epoch key, not the
     joint.
   - **The capability grants.** personae already has `delegation` (certificates,
     scopes, attenuation, revocation). Device grants are the same concept
     serialized a second way, so moving them in would install two delegation
     models side by side. Reconciling them is its own design question.

   Content refs crossed the boundary as `carry::CarryRef`, byte-identical to the
   `eidetic::Hash` string form the manifests already stored, so no disk format
   changed and personae stays free of an eidetic dependency.
2. **The two seams** (from the trust-plane plan): the seal seam over
   [muniment](https://github.com/mark-ik/muniment) (personae owns the epoch key
   the sealer uses) and the sync gate over codicil (moot/tessera/kith admission).
3. **mere re-base (last).** mere consumes personae and retires in-tree
   `persona/identity`; deliberately deferred so the app re-bases once, at the end.

## Conventions

MIT OR Apache-2.0, edition 2024 — the ecosystem default (Mark, 2026-07-08):
permissive by default for original code. MPL-2.0 is reserved for genuinely
Servo-derived crates (serval, netrender, netfetcher); the earlier promoted
crates that took MPL-2.0 did so by accretion from mere's file headers, not by
decision, and are candidates for a relicensing sweep. The MPL headers the
ported files carried were stripped here.
