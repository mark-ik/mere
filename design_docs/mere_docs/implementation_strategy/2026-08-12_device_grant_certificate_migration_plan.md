# Device grant certificate migration plan

Executes the
[device grants and delegation certificates](../technical_architecture/2026-08-11_device_grant_delegation_reconciliation.md)
ruling. Consumers: `session_runtime::wallet_grant`, `ports/castellan`,
`ports/signalman`, graphshell's identity projection.

## Posture, decided by Mark 2026-08-12

**Re-issue now, no legacy decoder.** Every device grant that exists today is
on a machine Mark can reach: signalman was founded 2026-08-11 and no station
is sited. The migration is cheapest now and gets strictly more expensive the
first time an unattended radio holds a grant.

**This collapses the sequence.** The reconciliation doc staged the work as
"epochs out of the payload" then "payload to certificate," which only earned
its separation if a legacy decoder had to be maintained across both. With no
dual-read window there is no value in an intermediate on-disk format, so the
old envelope is replaced once rather than twice.

## Phases

Revised 2026-08-12 after surveying what landed on top of M1. Two phases
shrank because the pieces already exist; see the findings below.

- **M1** — the device scope convention in `personae::carry`, additive. DONE.
- **M2** — RemoteAuth device grants issued as certificates, plus the
  separated epoch record. Verification delegates to `notochord`.
- **M3** — re-issue on unlock; the old envelope reader is deleted, not kept.
- **M4** — the roster's `revoked` list demoted to a projection of
  `notochord::RevocationLedger`. Smaller than planned: the ledger, the fold,
  and the signed statement all exist already.
- **M5** — consumers re-based; castellan reaches verification through
  personae and notochord rather than session-runtime.

## Findings

### Two consumers of the delegation model already exist (2026-08-12)

Surveyed before writing M2, and both change it.

**`personae::ssh_ca::self_grant` already builds a device grant as a
certificate.** `DelegationParent::Root(master)`, issuer and subject both the
master, scope from `device_capability_scope`, depth 0, mandatory expiry. It is
the **Copy**-mode shape: the device *is* the persona, so subject is the master
key. M2's RemoteAuth shape differs in exactly one field, the subject, which is
the delegate device's public key. That correspondence is worth keeping visible:
`DeviceMode::Copy` and `DeviceMode::RemoteAuth` are self-grant and
delegated-grant, and nothing else about the construction changes. The shared
construction moves to `carry` and `self_grant` becomes a wrapper, rather than
M2 writing a second near-copy.

**`notochord` is the chain evaluator, and it is complete.**
`crates/system/notochord/src/chain.rs` supplies `TrustedRoot`,
`RevocationLedger` with `fold(&SignedDelegationRevocation)` and
`revokes(&DelegationCertificate)`, and `validate_chain`, which checks
signatures, signer attestation, root anchoring, link integrity, strict
attenuation, revocation of any member, validity windows, and the leaf subject.
M2 must not hand-roll verification; it calls this.

The layering this reveals is the one the reconciliation doc argued for, and it
already exists in the dependency graph: **personae owns the grammar, notochord
evaluates it, session-runtime keeps the ledger.** `session-runtime` already
depends on `notochord`, so no new edge is needed.

The consequence for M4 is that "the roster demotes to a local fold" is mostly
wiring rather than construction. The fold exists, the statement type exists,
and the ledger is already the thing chain validation consults.

## Findings

### The domain question, decided (M1)

The reconciliation doc left "device family or persona family" open. Decided as
the **device authority family**, on the mechanics rather than on taste:
`CapabilityScope::attenuates` requires `self.domain == parent.domain` and
`self.resource == parent.resource`. A device-authority domain means a device
grant can never be mistaken for a narrowing of a persona's non-device
capability, because the domain comparison fails first. Putting every persona
capability in one domain would make that separation rest on `resource`
alone, which is the weaker guard.

### What the atom survey established (step 1, 2026-08-12)

Recorded in the reconciliation doc. Load-bearing here: `attenuations` is
enforced nowhere, so `remaining_delegation_depth: 0` is a behaviour gain
rather than a rename; and `scopes` is enforced in exactly one rule, that
`private.read` requires epoch material, which is the statement/carriage
coupling M2 dissolves into a ledger invariant.

### `path_covers` treats `"/"` as a leaf (M1)

Found while choosing the scope path, and pinned by a test rather than left as
a comment. `path_covers(prefix, path)` extends a prefix only when the
remainder begins with `/`, so `path_covers("/", "/anything")` is **false**
while `path_covers("/device", "/device/anything")` is true. A root prefix of
`"/"` therefore covers itself and nothing beneath it, which is the opposite of
what the value suggests to a reader.

This does not bite device grants, which have no path dimension: the action set
carries the whole capability and every caller passes the same path. It would
bite the first person to nest under it. Giving device capabilities a real
path dimension means choosing a different prefix, not deepening this one.

Not changed in personae. Every existing delegation consumer already lives with
this behaviour, and a fix would be a silent semantic change to a published
grammar. Recorded here and asserted in `carry::scope`'s tests.

## Progress

### M1: the device scope convention — DONE 2026-08-12

- [x] `carry/scope.rs`: `DEVICE_AUTHORITY_DOMAIN` (`mere.device`), the three
      action constants, `DEVICE_SCOPE_PATH`, and `device_capability_scope`,
      beside `DevicePublicKey` and `DeviceId` which already live in carry.
      Additive; nothing else changed.
- [x] Five tests, all green (personae lib 79): the scope is well-formed under
      the grammar's own rules (proven via `attenuates` against itself, which
      runs `is_well_formed` on both sides); dropping an action attenuates and
      the reverse does not; a different device never attenuates; a foreign
      domain never attenuates; and the `"/"`-is-a-leaf finding is pinned.
