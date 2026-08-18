# Device grants and delegation certificates

**Decided 2026-08-11.** Spun out of the
[wallet carry fold-in plan](../implementation_strategy/2026-08-10_wallet_carry_foldin_plan.md)'s
W3 ruling. Related: the
[sited device identity brief](../research/2026-08-10_sited_device_identity_brief.md),
the [castellan OTP plan](../implementation_strategy/2026-08-10_castellan_otp_plan.md),
`personae::delegation`, `session_runtime::wallet_grant`.

## The question

W3 ruled that the device-grant CBOR envelope stays in the session-runtime
adapter, because moving it into `personae` as-is would install a second
delegation model beside the first, inside the one crate that owns delegation.
It left the real question open: should a device grant instead *become* a
`SignedDelegationCertificate`, with a device-scoped `CapabilityScope`,
delegation depth in place of a `no-subdelegation` atom, and the wrapped epoch
material carried alongside?

## What changed since it was asked

Castellan grew its first device-identity consumer. `ReticulumStationMaterial`
derives a station credential from a Persona provider under domain-separated
salts, and a feature-gated `SitedStationGrant` adapter issues a narrow
RemoteAuth envelope for it: `transport.egress` only, no personas, no private
lane, `no-subdelegation`, mandatory hard expiry. Its module doc says outright
that it "deliberately does not decide the later reconciliation with
`SignedDelegationCertificate`".

So the question is no longer hypothetical. There is a first consumer, it is
the most security-sensitive one the stack has (an unattended radio expected to
be stolen), and it is currently built on the envelope under review.

## The two models, as they actually are

| | `SignedDelegationCertificate` | `SignedDeviceGrant` |
|---|---|---|
| issuer | persona master public key | persona master public key, typed `DevicePublicKey` |
| subject | persona master public key | device public key |
| signing key | scope-derived, with a `DerivedKeyAttestation` | the master keypair, directly |
| parent | `Root` or another certificate; chains | none; flat |
| scope | `CapabilityScope` with an `attenuates()` lattice | `Vec<String>` atoms |
| attenuation | `remaining_delegation_depth: u16` | `Vec<String>` atoms |
| time | `issued_at`, `not_before`, `expires_at`, `nonce` | `issued_at`, `expires_at` |
| encoding | hand-rolled, length-prefixed, domain-separated | canonical CBOR, 612-byte pinned fixture |
| revocation | `SignedDelegationRevocation` | a local roster list |
| secret carriage | none | `wrapped_private_epochs` |

## Four findings

### 1. The principal gap is a newtype, not a model difference

W3's table read as persona-to-persona versus device-to-device. Reading the
issuance path instead of the type names, the grant's delegator is
`DevicePublicKey::from(provider.master_public_key())`, signed by
`provider.master_keypair()`. The device grant is already persona-master to
device, which is exactly the direction a delegation certificate models.
`DevicePublicKey` is a newtype over `[u8; 32]` that, since W1, lives in
`personae::carry`, one module away from `delegation`'s bare `[u8; 32]` issuer
and subject fields. There is no principal mismatch to bridge. There are two
spellings of the same thing in one crate.

### 2. The envelope does two jobs, and refresh proves it

`refresh_remote_auth_private_read_grant` appends a `WrappedEpochMaterial` to
`payload.wrapped_private_epochs` and then re-signs the entire grant. The
capability did not change. Only the key carriage did. Because the grant's
content ref is a hash over the whole envelope, every new private epoch also
invalidates the `grant_ref` that wallets track, for a capability statement
that says exactly what it said before.

A capability statement and a key delivery have different lifetimes, different
re-issue conditions, and different audiences. They share an envelope here for
no reason except that the envelope was written once.

### 3. Revocation cannot travel

`revoke_remote_auth_device` pushes a device id onto `roster.revoked` and saves
the roster. There is no signed artifact. That is adequate for a wallet on one
machine, where the roster is the authority, and it is precisely the gap the
sited-device brief could not close: nothing here can tell a mesh that a
station's grant is withdrawn, because a local list is not a statement anyone
else can verify.

`personae::delegation` already has the missing artifact.
`SignedDelegationRevocation` is signed with the same scope-derived key as
issuance and is verifiable by anyone holding the issuer's master public key.
For a stolen radio this is the difference between revoking access and hoping
every peer asks the right wallet first.

### 4. personae wrote down the division of labour, and the wallet crossed it

The delegation module's own doc comment: it owns the identity proof and
attenuation grammar shared by applications, and it explicitly does not own an
application's grant ledger or policy, since applications may keep grants in
memory or fold the same statements through durable stores.

The wallet is exactly such a ledger. It was supposed to fold personae's
statements. It grew its own statements instead. The duplication is not that
two models exist. It is that a ledger authored a statement format.

## Ruling: converge, as a split

A device grant becomes a `SignedDelegationCertificate`, but the envelope is
not moved wholesale. It is cut along the seam finding 2 exposes.

1. **The capability statement becomes a certificate.** `domain` is the device
   authority family, `resource` carries the device id, `path_prefix` is `/`,
   and `actions` takes today's scope atoms verbatim. `no-subdelegation`
   becomes `remaining_delegation_depth: 0`, which is what it always meant.
   `not_before_ms` takes `issued_at_ms` where a grant has no separate start.
2. **Wrapped epoch material leaves the signed envelope.** It becomes its own
   record keyed by the certificate id, so a new epoch never re-signs an
   unchanged capability and never churns a `grant_ref`.
3. **Revocation gains a portable statement.** `SignedDelegationRevocation` is
   the artifact; the roster stays, demoted to what it should always have been,
   a local fold of revocations the wallet has seen.

`label` and `exposure` already live in the roster record rather than the
signed payload, which is the same seam drawn correctly the first time.

### This needs no change to personae

Checked against the code rather than assumed. `CapabilityScope::is_well_formed`
wants a non-empty domain, resource, path prefix, and action set, all of which
a device grant supplies. `DelegationCertificate::is_well_formed` wants
`issued_at_ms <= not_before_ms`, satisfiable by equality.
`DelegationParent::Root` accepts a wallet or persona root. The delegation
grammar absorbs device grants as written.

## What it costs

- **A wire migration.** The 612-byte fixture is pinned deliberately so a codec
  swap has to argue for itself, and real grants exist on Mark's machine. This
  is the argument, but the migration still has to be built.
- **One certificate per persona.** `personas: Vec<PersonaId>` becomes N
  certificates, since a certificate has one issuer. That is more records, and
  it is also the better shape: each persona independently issues and
  independently revokes, so withdrawing one persona's device access stops
  touching the others.
- **The station adapter re-targets.** Small, because it is policy-only: it
  builds a `RemoteAuthGrantSpec` and nothing else.
- **Scope atoms need reading once.** Done as step 1 below, and cheaper than
  expected: three real atoms, all straight actions.

## Posture, settled 2026-08-12

**Migration posture: re-issue now, no legacy decoder** (Mark). A device grant
is re-issuable by whoever holds the master seed, so for reachable devices the
cheap answer is to re-mint on next unlock and never write the old format
again. Sited radios cut the other way, since re-enrollment is exactly the
expensive operation for something remote and unattended, which is what made a
dual-read window worth offering.

What settled it is that the expensive case does not exist yet: signalman was
founded 2026-08-11 and no station is sited, so every grant holder today is a
machine Mark can reach. The window is open now and closes the first time a
radio goes up.

**This collapses the sequence below.** An intermediate on-disk format only
earns its keep if a dual-read window has to span it, so the old envelope is
replaced once rather than twice.

**The `domain` question is also decided**, in the
[migration plan](../../archive_docs/2026-08-18_completed_plans/2026-08-12_device_grant_certificate_migration_plan.md):
the device authority family, on mechanics rather than taste, because
`attenuates` compares domain first.

**Corrected 2026-08-13 (Mark).** This doc previously left open "whether the
wrapped-epoch record is an eidetic artifact or stays a wallet file". That is a
false alternative, and Mark caught it: the wallet is *already* an eidetic
consumer. `engram_seal::WalletEpochSealer` implements eidetic's `PayloadSealer`,
and its own header states the relation exactly — the store holds bytes, the
wallet owns the epoch key, and the sealer binds them. Eidetic is a store;
the wallet is a keyring and a ledger. A record is not in one *instead of* the
other.

Restated, the question also mostly answers itself. The wrapped-epoch record
cannot be a **sealed** eidetic payload, on bootstrap grounds: eidetic's sealer
derives its payload key from the epoch secret, and the epoch secret is what
this record delivers. Sealing it would require the key it carries.

What remains is narrower and worth asking properly: should epoch carriage
replicate at all? **Answered 2026-08-14** in
[epoch carriage and replication](2026-08-14_epoch_carriage_replication.md): it
may, at `TrustedPeersOnly`, but not until the record's plaintext identifiers
are blinded, and retention gates shipping. It stays `LocalOnly` until then.

## Sequence, when it runs

**Correction, 2026-08-12.** The first draft called step 2 separable and said
it paid for itself immediately. It is separable from the *certificate
conversion*, not from the *migration*: `wrapped_private_epochs` is a signed
field, so removing it changes the payload bytes and invalidates the signature
on every grant already on disk. Step 2 needs the same read window as step 3.
Nothing below ships ahead of the posture decision.

1. ~~Map the scope atoms to actions.~~ **Done 2026-08-12**, see below.
2. Wrapped epochs out of the payload into their own record, still on the
   current envelope. Independent of the certificate conversion, and it stops
   refresh from re-signing an unchanged capability.
3. Capability statement to `SignedDelegationCertificate`, behind the agreed
   read window.
4. Signed revocation, roster demoted to a fold.
5. Station adapter re-targets; castellan reaches grant verification through
   personae rather than through session-runtime.

### Step 1: what the atom survey found

Three scope atoms are real and in use: `identity.act`, `private.read`, and
`transport.egress`. All three map straight to `CapabilityScope.actions`.
`sync-membership-only` is not real. It appears once in the workspace, as an
example inside a doc comment, and was never implemented, so there is nothing
to reconcile and the comment should go with the field.

**`attenuations` is enforced nowhere.** Every occurrence across mere is a
write, a test assertion, or a display string in graphshell's identity
projection. The one apparent check is castellan's, and it asserts that a
grant castellan itself just issued carries the atom it just put there. So
`no-subdelegation` has never prevented a subdelegation. Mapping it to
`remaining_delegation_depth: 0` is not a rename, it is the first time the
constraint becomes real, because `attenuates()` refuses a parent whose depth
is zero.

**`scopes` is enforced in exactly two places, and both are one rule**: a spec
carrying `private.read` must also carry the epoch material.
`identity.act` and `transport.egress` are declarative and checked by nobody
outside castellan's own station policy. That single enforced rule is exactly
the coupling step 2 dissolves, a scope atom in the capability statement
constraining the key carriage because the two share an envelope. After the
split it becomes a ledger invariant, that a `private.read` certificate has a
corresponding epoch record, which is where it belonged.
