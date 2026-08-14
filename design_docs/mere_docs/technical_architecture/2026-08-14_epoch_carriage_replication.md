# Epoch carriage and replication

**Decided 2026-08-14, with Mark.** Follows the
[device grants and delegation certificates](2026-08-11_device_grant_delegation_reconciliation.md)
ruling, whose open question this replaces. Related: the
[migration plan](../implementation_strategy/2026-08-12_device_grant_certificate_migration_plan.md)
(M2 part two founded the record), `eidetic::seal`, `session_runtime::engram_seal`.

## The question, restated correctly

The reconciliation doc asked whether the wrapped-epoch record should be "an
eidetic artifact or a wallet file". That was a false alternative and is
corrected there: the wallet is already an eidetic consumer, since
`engram_seal::WalletEpochSealer` implements eidetic's `PayloadSealer`. Eidetic
is a store; the wallet is a keyring and a ledger.

The real question, put by Mark: **should epoch carriage replicate?** If the key
material is encrypted anyway, why not let it travel, so a device that loses its
record can recover without re-pairing?

The instinct is right and the benefit is real. M3 established that re-pairing is
the expensive operation for exactly the devices that most need this. But
"encrypted anyway" turns out to be true of one field and false of the record.

## Ruling

1. **Epoch carriage may replicate, at `TrustedPeersOnly`.** Never at
   `MootScoped` or `PublicPortable`.
2. ~~Not until the record's plaintext identifiers are blinded.~~ **Done
   2026-08-14**, construction below.
3. **Eidetic's sealer is not the mechanism.** The record must stay
   self-protecting, and that must be stated at the call site rather than
   inherited by accident.
4. **Retention is unresolved and gates shipping**, not design.

## Why

### Only one of four fields is ciphertext

`WrappedEpochMaterial` carries `persona_id`, `epoch_id`, `wrap_format` in
plaintext, and `wrapped_key` encrypted. A replica therefore discloses which
personas exist, which devices serve them, and how often epochs rotate.

For a stack whose premise is persona separation, that is the leak that matters:
two personas meant to be unlinkable become linkable by anyone holding replicas
of both. On a radio mesh, where traffic analysis is already cheap, it hands
over the persona-to-device association graph.

This is fixable rather than fatal. The identifiers exist to *route* material to
the right persona and epoch on the receiving side, and the receiver already
holds the pairing key. A blinded index (the identifier under a MAC keyed by the
pairing secret) lets the holder match what an observer cannot correlate. That
work is the precondition in ruling 2.

### The class that fits cannot use its own mechanism

`eidetic::seal::is_private_lane` counts `LocalOnly` and `TrustedPeersOnly` as
the private lane, whose payloads are sealed. The epoch record **cannot be
sealed by that sealer**: `WalletEpochSealer` derives its payload key from the
epoch secret, and the epoch secret is what this record delivers. Sealing it
would require the key it carries.

Checked rather than assumed, and the resolution is a footgun worth recording.
The seal path is:

```rust
match sealer {
    Some(sealer) if is_private_lane(manifest.privacy) => { /* seal */ }
    _ => Ok(cleartext.to_vec()),
}
```

A record written at `TrustedPeersOnly` **with no sealer is stored cleartext**,
and nothing complains. That is what makes ruling 1 mechanically possible, and
it is also a silent degradation from a class that reads as a guarantee. Epoch
carriage should therefore be written through a call site that says out loud
that it is self-protecting and deliberately unsealed, so nobody later reads
`TrustedPeersOnly` on this record as evidence that eidetic sealed it.

### Retention is the risk that does not have a schema fix

Revocation and replication do not compose, and revocation exists here precisely
because devices are expected to be stolen.

`wrapped_key` is encrypted to a per-device pairing key, so a stolen radio opens
only records wrapped to it. That bounds the blast radius without closing it.
Revoking rotates the epoch head, which protects the future; every historical
record that device could open stays openable. Locally that is remediable, since
rotation can delete the superseded record. Once replicated it is not: other
hosts hold copies, `purge_deleted` operates on the local store, and
content-addressed history is built not to forget.

So replication converts "revoke, rotate, purge" from a remediation into "revoke,
rotate, and hope". That is harvest-now-decrypt-later against a device the threat
model already assumes will be lost.

Two candidate answers, neither adopted here: bounded retention, where carriage
records expire on their own rather than persisting as history; or re-wrap on
rotation, where a superseded record is replaced rather than accumulated. Both
need the trusted-peer set to be revocable, which is the same shape as retinue's
open revocation-propagation question.

**Scoped 2026-08-14:** a retention design combining both candidates, and
decoupling them from the revocation-propagation question, is proposed in
[epoch carriage retention: leased slots](2026-08-14_epoch_carriage_retention_leases.md).

## The blinding, as built

`WrappedEpochMaterial` now carries a `BlindedEpochIndex` in place of
`persona_id` and `epoch_id`:

```rust
index = keyed_hash(derive_key(BLIND_INDEX_CONTEXT, wrapping_key),
                   wrapped_epoch_aad(persona, epoch))
```

The blinding key is derived from the device's own pairing-derived wrapping key
under its own context, so it cannot be confused with the key-wrapping use of the
same secret. The message is `wrapped_epoch_aad`, which was already the canonical
encoding of the pair, so the index and the AEAD's associated data now commit to
exactly the same bytes.

Two devices holding material for the same persona therefore produce different
indices, because each has a different pairing key. That is the property the
whole exercise was for, and it is asserted directly.

Nothing was weakened to get it. The ciphertext was always bound to (persona,
epoch) through its AAD, so the plaintext fields were only ever a lookup
convenience. `unwrap_private_epoch_material` now takes the pair the caller
expects, which means the pair is checked twice: once against the index, for a
legible `WrongEntry` rather than an opaque AEAD failure, and once by the AAD,
for the guarantee.

### What it cost, and what it taught

Three call sites had been reading identifiers off the record, and each one
became clearer for having to state what it actually wanted:

- **Enrollment** resolves rather than reads. The installing device already knows
  its personas and their epoch heads from the bundle's own manifests, and it
  holds the wrapping key, so it computes each candidate's index and matches. An
  entry matching no candidate is simply not for this device and is skipped,
  which is the correct behaviour for a record that may one day replicate.
- **Refresh** replaces instead of filtering. The record is keyed by *one
  persona's* certificate, so by construction every entry in it belongs to that
  persona and there is nothing to keep. Blinded entries never need to be read in
  order to be superseded.
- **Revocation** asks presence, not identity. It used to compare the stored
  epoch id against the persona's head, which after blinding would have required
  the device's wrapping key — and the direct issue path never retains one. That
  exposed a real coupling rather than a test artifact. Presence is the better
  question anyway: over-rotating on a revocation costs a re-wrap, while
  under-rotating leaves a withdrawn device holding a live epoch. Idempotence now
  comes from **deleting the record on revoke**, which a withdrawn device should
  not keep regardless.

## What this does not decide

- **Retention policy.** Ruling 4. Until it is answered, epoch carriage stays
  `LocalOnly`, which is also eidetic's default and today's behaviour, so nothing
  ships in an undecided state.
- **What "trusted peer" means when trust is withdrawn.** Shared with the sited
  device brief's revocation-propagation question; a peer that stops being
  trusted still holds what it already replicated.

## Consequence for today's code

`WrappedEpochRecord` is still a wallet file at
`identity/grants/epochs/<certificate-id>.cbor`, and still `LocalOnly`, because
retention remains unanswered. What changed is that it no longer discloses
anything if it does travel: precondition one is met, and the record now carries
no identifier an observer can correlate.
