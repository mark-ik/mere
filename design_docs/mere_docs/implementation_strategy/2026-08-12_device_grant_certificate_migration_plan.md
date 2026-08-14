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
- **M3** — retiring legacy grants. DONE, but not as "re-issue on unlock":
  see the finding below, which the posture forces.
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

### M2: certificates for device grants — personae half done 2026-08-12

- [x] `carry/grant.rs`: one constructor for both device modes.
      `issue_self_grant` (subject is the master, the `Copy` case) and
      `issue_remote_auth_grant` (subject is the holder's device key) over a
      shared `issue_device_grant`. Parent is the persona's own root, depth is
      0, expiry is mandatory.
- [x] `device_grant_nonce` binds the subject as well as the device and the
      clock. The derivation lifted from `self_grant` used only device and
      clock, so two holders granted the same device inside one millisecond
      would have collided on a certificate id. Pinned by a test.
- [x] `ssh_ca::self_grant` now delegates rather than carrying its own copy of
      the construction, per generalize-don't-duplicate. Verified against its
      seven tests, which drive it through the mint path.
### M2 part two: the records — done 2026-08-12

- [x] `carry::issue_persona_device_grant`: issuance under one persona's own
      authority, signed by a provider seeded from the persona's derived
      keypair, anchored at that persona's `PersonaChainRoot`.
- [x] `wallet_grant/certificate.rs`: `WrappedEpochRecord` keyed by
      `DelegationId`, certificate and record storage, and
      `check_epoch_carriage`.
- [x] Eleven tests across the two crates. The load-bearing one is
      `appending_an_epoch_leaves_the_certificate_untouched`: it asserts the
      certificate id **and** its content ref are unchanged after epoch
      material lands, which is the exact operation the old envelope could not
      perform without re-signing.
### M2 part three: the grant set — done 2026-08-12

- [x] `carry::is_persona_scoped_action` / `partition_actions`: the action
      partition. `identity.act` and `private.read` are a persona's authority to
      delegate; `transport.egress` and the `ssh.*` set are the device's own.
      Unknown actions default to device-scoped, the narrower reading.
- [x] `carry::DeviceGrantSet` and `issue_device_grant_set`: one device
      certificate issued by the master plus one per persona issued by that
      persona's chain root, with the partition deciding which actions land
      where. Refuses persona-scoped actions when no persona is named, rather
      than silently returning a narrower grant than asked for.
- [x] `PersonaId` gained `PartialOrd`/`Ord` so the set can be a `BTreeMap`.
      Additive, and a deterministic order is what a persona-keyed map wants
      anyway.
- [x] Twelve tests. Suites green at personae 97, session-runtime 253, plus a
      non-test `cargo check` and the `ssh,agent` lanes.
### M2 part four: the storage seam — done 2026-08-12

The four flows all read and write grants through one pair of calls, so they
cannot switch one at a time without leaving the tree reading one format and
writing another. This phase builds the replacement pair so the switch becomes
mechanical.

- [x] `device_scope_certificate_path`: the master-issued half is
      `<device>/device.cert`, named rather than persona-keyed because it
      belongs to no persona. A station has only this file.
- [x] `save_device_grant_set` / `load_device_grant_set`. Loading reads the
      device's directory rather than taking a persona list, so a caller that
      has forgotten which personas granted a device still recovers the set.
- [x] `device_grant_set_ref`: the roster and wallet index each hold one ref
      per device, and a grant is now several certificates, so the ref covers
      the set. Hashing member certificate ids suffices because a certificate
      id already commits to its own contents.
- [x] Four more tests, session-runtime 257 and personae 97, plus a non-test
      `cargo check`.
### M2 part five: the flows switch and the envelope dies — DONE 2026-08-12

**M2 is complete.** `envelope.rs` is deleted and `DeviceGrantPayload`,
`SignedDeviceGrant`, and `DeviceGrantSignature` are gone from the tree. No
legacy decoder was kept, per the posture.

- [x] `issue` builds and stores a `DeviceGrantSet`, with epoch material
      written per persona certificate.
- [x] `enroll` reads and installs the set; `RemoteAuthEnrollmentBundle` gained
      an `epochs` field, so the delegatee receives key material beside the
      certificates rather than inside them.
- [x] `refresh` **writes only the epoch record**, and the payoff is now a
      test rather than a claim (below).
- [x] `revoke` asks each persona certificate whether it carried
      `private.read`, instead of consulting one flag for the whole grant.
- [x] `validate` checks every member of the set: each must address this device
      and name this holder, and the effective expiry is the earliest in the
      set.
- [x] Consumers re-based: castellan, signalman, and graphshell's identity
      projection.
- [x] Green: session-runtime 250, personae 97, castellan 31, knot 81,
      signalman 16.

#### The one test that changed meaning

`revoke_remote_auth_device_refreshes_remaining_private_read_grants` asserted
`assert_ne!` on the grant ref across a refresh. It now asserts `assert_eq!`.

That inversion is the whole migration in one line. Rotating a private epoch
replaces key material a device holds; it does not change what the device is
permitted to do. The old envelope could not express that difference, so every
rotation re-signed the capability statement and pushed a new ref through the
roster, the wallet index, and every persona capability slot. The test was
faithfully recording the churn as if it were correct.

#### Two behaviours deliberately tightened

- **Expiry is mandatory.** `grant_lifetime_ms` refuses a spec with no
  `expires_at_ms`, where the old envelope allowed `None`. The delegation
  grammar still permits an unbounded certificate, so this is a narrowing at
  the wallet rather than a limitation: a device grant that never expires is
  what the sited-radio case must not be able to mint by omission.
- **`no-subdelegation` is enforced.** It was a string atom no code read.
  Castellan's station policy now checks `remaining_delegation_depth == 0`,
  and the grammar refuses to extend a chain past it.

### M3: retiring legacy grants — DONE 2026-08-12

**The migration cannot be automatic, and the posture is why.** M3 was written
into the plan as "re-issue on unlock," which assumed the wallet could re-mint
each grant from what it already knew. It cannot. What a grant *permitted* lived
in the signed payload, and "no legacy decoder" means refusing to read the
payload, so the scopes are exactly the thing that does not survive. Re-minting
from a default would either widen a device's authority or narrow it in silence,
and both are worse than asking.

That is not an argument against the posture, which remains right: a dual-read
window would have carried the old codec forward indefinitely for grants that
are cheap to re-issue while no radio is sited. It is the price of the choice,
and it should have been named when the choice was made rather than discovered
in M3.

What survives outside the payload turns the restatement into a confirmation
rather than an act of memory, and `survey_legacy_grants` recovers it without
opening a single grant byte:

- the roster keeps each device's label, mode, exposure, and public key;
- each persona wallet keeps a capability slot named for the device, so **the
  persona set is fully recoverable**.

Only the scopes must be restated, and `reissue_legacy_grant` refuses an empty
scope list rather than defaulting.

- [x] `survey_legacy_grants`, `reissue_legacy_grant`, `retire_legacy_grant`.
- [x] `retire_legacy_grant` refuses while a device is stranded, so the last
      record that a device ever had authority cannot be deleted before that
      authority is restored.
- [x] Private-lane material is not recoverable either: it was wrapped to a
      pairing key inside the payload. A device that needs it is re-paired, not
      re-issued. Recorded in the code at the point that would otherwise look
      like an omission.
- [x] `legacy_grant_hint` makes the failure attributable. A stranded device
      used to report only "certificates missing", which reads like corruption;
      enrol and revoke now say the device predates the format and point at the
      survey.
- [x] Seven tests. Green: session-runtime 257, personae 97, castellan 31,
      signalman 16.

#### Noted, not fixed

`ports/graphshell/src/bin/persona_switch_receipt.rs` does not compile, from a
borrow of a temporary. Pre-existing and not this work: the commit that added
it says so in its subject, "Add the persona-switch receipt, not yet compiled".
The `gemot` / `p2panda_auth` trait errors in the workspace check are likewise
someone else's in-flight work.

The remaining surface is smaller than it looked. Counting references to
`DeviceGrantPayload` and `sample_payload` across `wallet_grant`: eight are in
`envelope.rs`, which the switch deletes outright, and the rest are one or two
call sites each in `issue`, `enroll`, `revoke`, `types`, and `test_support`.
The flows are long, but they touch the grant itself in very few places.

### The empty-persona case forced the action partition

The reconciliation doc and this plan both assumed a device grant became "one
certificate per persona." Castellan's sited-station grant is the
counterexample, and it is the real shipping case: `transport.egress`, no
personas at all. A purely per-persona split issues it **nothing**.

So a grant is a *set*, and which certificate an action lands on depends on
whose authority covers it. Carrying traffic outward is the device's own
authority and needs no persona behind it. Acting as a persona is that
persona's authority to delegate and cannot exist without one. The partition
falls out of that question rather than being imposed.

This is the third time contact with the code has corrected the ruling's cost
line, after "the principal gap is a newtype" and "per-persona certificates
need per-persona issuers." The pattern is consistent: the delegation grammar
keeps being more specific than the envelope it replaces, and each specificity
is a decision the envelope had been leaving implicit.

### The persona split needs per-persona issuers, which is bigger than costed

The reconciliation doc costed `personas: Vec<PersonaId>` at "one certificate
per persona ... more records." Building it showed the records are the small
part.

A certificate has one issuer, and `SignedDelegationCertificate::issue`
requires the provider's master public key to *equal* that issuer. Per-persona
certificates therefore need per-persona **signing identities**, not just per
persona rows. They exist: `persona_wallet_salt` and `derive_persona_chain_root`
were already there, and the name says what they were for. So issuance seeds a
provider from the persona's derived keypair and anchors the certificate at
`DelegationParent::Root(persona_chain_root)`.

The consequence worth flagging is on the verifying side. The wallet's trusted
root set stops being one entry and becomes **one `TrustedRoot` per persona**,
`authority` and `issuer` both that persona's chain root. Anything evaluating a
device grant has to know which personas it trusts, rather than knowing one
master key. That is the correct shape, and it is what buys the independence
the ruling wanted, but it is an architectural change rather than a bookkeeping
one and it lands in M5 when consumers re-base.

Two shapes were considered and rejected before this one. Encoding personas as
scope *actions* would make attenuation work for free but conflates what a
device may do with whom it may do it for, and revoking one persona would still
mean re-issuing the whole certificate, losing the only benefit. A
master → persona → device **chain** reads better but does not fit:
`attenuates` requires the child's `resource` to equal the parent's, so the
persona link would have to be minted per device, which is not what a persona
link means.

### The ssh feature never compiled on Windows (M2)

`cargo test -p personae --features ssh` failed to build on this workspace's
primary platform, because `tests/ssh_ca_live.rs` carried `#![cfg(feature =
"ssh")]` but no `#![cfg(unix)]` while using `os::unix::fs::PermissionsExt`
and `Permissions::from_mode`. The target drives a real `sshd` and chmods its
key files, so it is inherently Unix-only; it was simply missing the guard that
says so.

Found only because the dedup above forced a build under the feature. Worth
noting as a pattern rather than a one-off: the `ssh` feature is not default,
so the whole module and its 7 tests are invisible to a plain `cargo test -p
personae`, which is why the edit had to be verified with `--features ssh`
before it could be believed.
