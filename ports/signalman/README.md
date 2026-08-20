# Signalman

Signalman is Mere's private Retinue-management port. It obtains the operator
identity from Castellan/Personae, derives a station-scoped Reticulum identity,
binds its Ed25519 half to a narrow, expiring RemoteAuth grant, and hands the
typed identity to Retinue's host library. The grant permits only
`transport.egress`, has no persona or private-read authority, and fails closed
at expiry. Retinue itself owns no Mere, Personae, or Castellan dependency.

Grant, renewal, revocation, and acknowledgement travel as bounded signed
control bodies inside normal authenticated LXMF messages. The Persona signs
each control frame through a device-specific attested child; the station signs
the acknowledgement with its derived Reticulum identity. A receiver accepts an
exact retransmission after a lost acknowledgement, but accepts a replacement
only before its prior deadline and only when the expiry grows. The receiver's
snapshot is non-secret state to persist beside the device's delegated identity.
`SitedStationHead` seals both through a caller-provided `SealedRecordStorage`,
persists an accepted transition before returning its acknowledgement, and
checks the receiver before every public radio operation. The process that
selects a board's secure-storage root remains deployment-owned. The
`mere-signalman-provision` command creates that sealed record without exposing
the derived private identity:

```text
mere-signalman-provision \
  --authority-root PATH \
  --station-root PATH \
  --record RELATIVE_PATH \
  --label NAME \
  --expires-hours HOURS
```

The command requires an explicit finite grant duration, installs the signed
grant before the radio opens, verifies the station-signed acknowledgement, and
refuses to replace an existing record. It prints only public receipt fields.
The physical radio receipt remains deployment work.

This integration package is intentionally a separate workspace and pins the
Retinue source revision consumed by downstream Signalman builds. It is not a
publishable dependency story.
The portable boundary is `postilion::StationConfig`, which accepts a supplied
`PrivateIdentity`; this port is its first real Persona-backed consumer.

Linkboy remains below this port. Its package policy, immutable plans,
execution, recovery, and rescue CLI do not depend on Signalman or Mere.
