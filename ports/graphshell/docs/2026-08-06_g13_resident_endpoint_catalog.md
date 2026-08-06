# G13 resident endpoint catalog

**Date:** 2026-08-06
**Result:** a native Graphshell host can select one locally registered product
endpoint after admission without giving that product carrier or identity
authority.

## Contract

`native::endpoint_catalog::ResidentEndpointCatalog` owns local route
registrations. `open` takes a host-derived `AdmittedEndpointContext` and asks
the selected factory to make one endpoint for that exact session and subject.
It does not admit a carrier, reconstruct a delegation, or serialize the
context.

`register` adapts ordinary Graphshell projection, presentation, and intent
endpoints. `register_notifying` also preserves a product's revision notices.
`register_erased` is the narrow custom path for endpoint-specific carrier
behavior such as projection resume. The resulting `ResidentEndpointSession`
is the carrier-facing erased endpoint, so product error types do not become
host route API.

## Second consumer

Cleromancy A19 registers a `cleromancy` route. Its factory receives only the
admitted context and binds its local endpoint through `BindAdmittedSession`.
The retained Graphshell session mounts the resulting catalog endpoint through
the real `LocalCarrier`, submits the already-bounded concurrence action, and
observes the saved Pattern occasion after resnapshot.

## Evidence

```text
cargo test --features graphshell-admission \
  --test a19_resident_endpoint_catalog --offline
```

The focused receipt is intentionally in Cleromancy because it proves a real
product consumer rather than a Graphshell-only adapter. Graphshell's catalog
unit tests separately cover a notifying typed registration and reject duplicate,
invalid, and unknown routes.

The A19 binary passed: it mounted the catalog-selected Cleromancy endpoint,
saved the bounded concurrence, and found the Pattern occasion after a fresh
snapshot.

## Stop rule

G13 does not add browser route selection, a remote endpoint registry, dynamic
plugin loading, a serialized admission bearer, or a Mere dependency on a
product endpoint. A browser route may use this catalog later, but must enter it
only after admission and retain Graphshell's authority loop outside the
selected endpoint.
