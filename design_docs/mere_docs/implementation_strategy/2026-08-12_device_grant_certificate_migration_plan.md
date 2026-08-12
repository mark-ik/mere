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

- **M1** — the device scope convention in `personae::carry`, additive.
- **M2** — the certificate and the separated epoch record; issue and enroll
  write them.
- **M3** — re-issue on unlock; the old envelope reader is deleted, not kept.
- **M4** — revocation as a signed statement; the roster demoted to a fold.
- **M5** — consumers re-based; castellan reaches verification through
  personae rather than session-runtime.

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
