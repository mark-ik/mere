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
2. **Not until the record's plaintext identifiers are blinded.** As written it
   would trade a persona-separation guarantee for an availability convenience.
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

## What this does not decide

- **The blinding construction.** Ruling 2 names the requirement, not the scheme.
- **Retention policy.** Ruling 4. Until it is answered, epoch carriage stays
  `LocalOnly`, which is also eidetic's default and today's behaviour, so nothing
  ships in an undecided state.
- **What "trusted peer" means when trust is withdrawn.** Shared with the sited
  device brief's revocation-propagation question; a peer that stops being
  trusted still holds what it already replicated.

## Consequence for today's code

None immediately, and deliberately. `WrappedEpochRecord` is a wallet file at
`identity/grants/epochs/<certificate-id>.cbor` and stays one. This doc exists so
that the first person to reach for replication finds the two preconditions
already stated rather than discovering the persona leak in review.
