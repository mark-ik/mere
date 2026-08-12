# Signalman

Signalman is Mere's private Retinue-management port. It obtains the operator
identity from Castellan/Personae, derives a station-scoped Reticulum identity,
binds its Ed25519 half to a narrow, expiring RemoteAuth grant, and hands the
typed identity to Retinue's host library. The grant permits only
`transport.egress`, has no persona or private-read authority, and fails closed
at expiry. Retinue itself owns no Mere, Personae, or Castellan dependency.

This integration package is intentionally a separate workspace while it uses
the neighboring Retinue checkout. It is not a publishable dependency story.
The portable boundary is `postilion::StationConfig`, which accepts a supplied
`PrivateIdentity`; this port is its first real Persona-backed consumer.

Linkboy remains below this port. Its package policy, immutable plans,
execution, recovery, and rescue CLI do not depend on Signalman or Mere.
