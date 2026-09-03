# Sited Device Identity: Castellan, Signalman, and the Map

**Date:** 2026-08-10
**Kind:** research brief (design probe; nothing here is scheduled)
**Anchors:** [credential port + gazette brief](2026-08-10_credential_port_gazette_brief.md),
[castellan OTP plan](../implementation_strategy/2026-08-10_castellan_otp_plan.md),
[projection proofs plan](../implementation_strategy/2026-07-21_projection_proofs_plan.md) (P5),
retinue's `2026-08-09_signalman_cambium_desktop_scope.md`.

**Execution note, 2026-08-11:** the station-identity origin boundary below is
implemented, deliberately short of commissioning and grant distribution.
`postilion::StationConfig` now accepts a caller-supplied typed Reticulum
identity and has no identity path, loader, or minting fallback. Castellan
derives separate X25519 and Ed25519 material from domain-separated Personae
children, and the new private `ports/signalman` adapter is the first consumer
that turns it into a Retinue identity. Retinue itself does not depend on Mere,
Castellan, or Personae.

**Execution note, 2026-08-11, follow-on:** Castellan now wraps the current
signed `DeviceGrantPayload` in a narrow `SitedStationGrant`: the Reticulum
Ed25519 half must match the derived credential and the unlocked host Persona;
the payload has exactly `transport.egress` and `no-subdelegation`, no personas
or private epochs, and a mandatory expiry. Signalman only builds its public
`StationConfig` after host-side signature, roster, revocation, key-binding, and
strict expiry checks (`now >= expiry` refuses). A revoked id is refused before
the generic issuer can write a replacement grant. Distribution or refresh to a
remote station, device-side and mesh-wide revocation enforcement, the host-side
placement record, and P5's geographic adapter remain open seams.

**Execution note, 2026-08-11, live lease:** Signalman's private port now owns
the running `postilion::Station`, rather than exposing an unchecked
`StationConfig`. It reloads the host grant before each public station action
and drops the station at its accepted deadline. A renewal must carry a strictly
later expiry *and* be accepted by the live lease before the old deadline; a
late replacement cannot resurrect it. Local host revocation closes and drops
the station immediately. Distributing a renewal or stop signal to a remote
headless station remains carrier work, not an implied property of the local
wallet store.

**Execution note, 2026-08-12, control carrier:** Signalman's port now defines
a bounded control body over ordinary authenticated LXMF delivery: a
Persona-attested, device-specific control key signs grant/renewal and revoke
frames; the station's derived Reticulum key signs acknowledgements. The
receiver admits the exact same grant again after a lost acknowledgement, but
requires a different grant to extend the accepted deadline before that deadline
passes. Its snapshot is deliberately non-secret state, to be persisted with the
device's delegated identity by the eventual unattended station host. Postilion
now offers generic binary LXMF carriage but owns none of those controls. No
current board firmware interprets the control body yet, so this is not a
physical delivery receipt.

**Execution note, 2026-08-12, sealed station runtime:** Signalman now offers
`SitedStationHead`, the unattended host runtime under its private port. It
seals the delegated Reticulum identity and control-receiver snapshot through a
caller-selected `SealedRecordStorage` record, writes an accepted control
transition before returning its signed acknowledgement, and restores the same
station address and permanent expiry/revocation closure after restart. Its
watchdog observes durable receiver transitions, so revocation wakes and drops
the running station rather than waiting for the former deadline. Choosing a
board-backed storage root, dispatching received controls over a live radio,
and proving a physical stop remain separate carrier/deployment receipts.

**Execution note, 2026-09-02, first owner:** board first-owner control is not a
sited-station credential. Signalman derives its private controller signer from
Castellan's separate controller domain and a separate persisted controller
scope, read-only from the existing authority root. It cannot reuse a station
device id or identity; the explicit claim command refuses a missing wallet or
scope and creates neither. This is host integration only, not a board claim or
commissioning receipt.

The origin (Mark, 2026-08-10): castellan should manage derived, ephemeral
device identities, so a sited radio holds nothing worth stealing. Radios "will
probably be sited not under the ambit of a host and stolen at some point."
Then castellan into signalman, and later into graphshell, for arranging
GPS-equipped or self-placed nodes accurately on a local map.

Both halves check out. Two findings reshape them.

---

## Finding 1: signalman's identity model is the thing to replace

Verified in retinue, not assumed. `postilion::StationConfig` (`crates/postilion/src/lib.rs`)
says it outright at line 108: **"Where the operator's private identity lives.
The file is the account."** `load_identity()` reads a 64-byte seed from
`station.id` (or `park-<name>.id`), or mints one with `getrandom::fill` and
writes it beside itself. There are no personae in retinue at all; the only
`Personality` is a board firmware mode (`Phy | Retinue | RNode | MeshCore |
Sennet`), unrelated.

The code already knows the shape of the problem. Refusing a wrong-sized
identity file, it says: *"this is a private key and replacing it silently
would mint a new station under a new address."* That is the brittleness a
derived model removes. A file that IS the account cannot be rotated, cannot be
scoped, and cannot be revoked without the address changing.

**What castellan offers instead is already built**, in `session-runtime`'s
grant machinery:

| Need | Existing mechanism |
|---|---|
| Not the master key on the device | `DeviceMode::RemoteAuth` (vs `Copy`, which *is* the master seed and whose revocation means master rotation) |
| Narrow authority | `DeviceGrantPayload.scopes` / `.attenuations` string atoms |
| Ephemerality | `DeviceGrantPayload.expires_at_ms` |
| Revocation without master rotation | `revoke_remote_auth_device` clears capability slots and rotates future write epochs |
| Survivors keep working | `refresh_remote_auth_private_read_grants` re-issues for remaining devices |
| Derived rather than stored-from-master | `personae::derive_keypair(salt)` |

A sited radio should therefore be **RemoteAuth, never Copy**, with a grant
carrying `transport.egress` and neither `identity.act` nor `private.read`.

### The limit, stated plainly

An unattended device that must operate autonomously needs *some* secret at
rest. There is no zero. What the model buys is that the secret is delegated
rather than master, narrow, expiring, and cheap to revoke. Below that line it
is hardware: ESP32-S3 flash encryption and secure boot, nRF52840 APPROTECT.
That is board territory, not Rust, and it is where the residual risk lives.

The soft spot to watch is `LocalDeviceIdentity.device_seed`. It is sealed
through `SealedRecordStorage`, but sealing needs a key, and an auto-unlock
root on a headless box is only as strong as the hardware holding it. On a
laptop the OS store is a real boundary. On a pole-mounted radio it is not.

---

## Finding 2: retinue already forbids what the map half wants to store

`validation/security/flash-policy.toml` lists `"latitude"` and `"longitude"`
under `[forbidden] name_patterns`, in a group named "Site inventory and
contacts", scanned against `radio-hand`'s settings and store and both
firmware stores. Retinue's own policy says: **a radio does not persist its
own coordinates.**

That is not an obstacle to Mark's map idea. It is the same instinct arriving
first, and it resolves the design cleanly:

- **The node does not know where it is.** Nothing to read off a stolen board,
  which is exactly the point, since a site inventory is more sensitive than
  any one radio.
- **The host knows where it put it.** Placement is a commissioning fact,
  authored once by whoever sited the node, whether read from a GPS at
  commissioning time or dropped by hand on a map. It belongs in the operator's
  own store, persona-scoped, alongside the device grant that authorized the
  radio in the first place.
- **A GPS-equipped node can still report position live** without persisting
  it, if a deployment wants that. Live telemetry and durable site inventory
  are different things, and only the second is forbidden.

There is one decoded-but-unused precedent: `tucket::AdvertData.location_e6:
Option<(i32, i32)>`, a MeshCore ADVERT field ported for wire interop. It is
consumed nowhere in retinue, and `AdvertData::chat()` sets it `None`.

---

## The map half, and what it unblocks

Mere's projection proofs plan deferred exactly this, and said why (2026-07-22):

> "P5 adds `scenomise/fixtures/coastal_map.json` ... There is no
> Retinue/Tulle/Sennet location-fact API to adapt yet, so none was invented."

That deferral is still accurate: retinue has no location-fact surface today.
So the chain is legible end to end. **Placement facts, held host-side and
persona-scoped, are the radio fact surface P5 has been waiting for.** Feed
them to `sceno` as geographic source facts and the existing fixture-driven
path realizes them; the plain arrangement name **Atlas** is already reserved
for that geographic arrangement. Graphshell then arranges nodes on a local map
because it is the family's remote lens over exactly that scene, not because it
grew a mapping feature.

Nothing new is needed in the projection stack. What is missing is the fact
source, and this brief says where it should live.

---

## Shape, if this is taken up

Three seams, in dependency order:

1. **Castellan issues, signalman consumes.** Replace `postilion::load_identity`'s
   file-is-the-account with a delegated device grant: derived key, narrow
   scope, expiry, revocable. Signalman's operator identity becomes a persona;
   each radio becomes a RemoteAuth device under it.
2. **Commissioning writes a placement fact.** Persona-scoped, host-side, beside
   the grant. Not on the node, per retinue's flash policy.
3. **Placement facts become sceno geographic facts.** The adapter P5 named and
   declined to invent.

Deliberately unresolved here: whether a device grant should become a
`SignedDelegationCertificate` (the question spun out of the wallet carry
fold-in's W3). That reconciliation sits underneath seam 1, and this brief does
not prejudge it.

## Open questions

1. Retinue is a separate repo with its own workspace and MPL license, and
   signalman consuming castellan means a mere dependency in the radio family.
   Does that cross a boundary Mark wants kept? The trust plane is meant to be
   shared, but the direction of the dependency is worth ruling on explicitly.
2. Grant refresh needs a channel. A radio reachable only over LoRa has a
   very small one, so expiry windows have to be sized to the worst link, not
   the best. What happens when a grant expires and the radio cannot be
   reached?
3. Does a stolen radio's revocation need to propagate *over the mesh* to
   nodes that never see the host? That is a distribution problem, not a
   crypto one, and it may want the moot/tessera admission machinery rather
   than anything in castellan.
