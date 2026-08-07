# G14 browser catalog route

**Date:** 2026-08-06
**Result:** a native browser session can serve one host-configured resident
endpoint after ordinary browser admission.

## Contract

`ResidentEndpointRoute` names a locally registered catalog id and a non-zero
notice polling interval. It is construction-time host configuration. A browser
cannot supply or change it.

`serve_catalog_native_messages` performs the ordinary native-message
challenge, browser admission, and retained-authority setup. It then derives
the `AdmittedEndpointContext`, opens the selected catalog entry with that
context, and only then sends `Connected` and starts the existing
revocation-aware notice loop.

The selected endpoint receives the same narrow session and public-key subject
as the local A19 route. The browser host retains the carrier, delegation,
revocation ledger, and native identity plane. The default
`serve_identity_native_messages` path remains the direct identity endpoint.

## Evidence

```text
cargo test -p graphshell --lib \
  browser_route_opens_a_catalog_endpoint_only_after_admission --offline
```

The focused browser-native test passed. It completes the real challenge and
admission exchange over the native-message carrier, records the context seen
by the catalog factory, checks it against the `Connected` session and subject,
opens the selected endpoint, then closes the admitted carrier normally.

`host_route_rejects_an_ambiguous_id_or_busy_notice_loop` also passed. It
rejects whitespace route ids and a zero interval that would turn a quiet
session into a busy loop.

## Ownership

The caller owns the catalog instance and route value for the browser session.
Graphshell owns selection timing and all carrier authority. A product owns only
the endpoint instance it returns from the registration factory.

## Stop rule

G14 does not alter the installed device broker's default identity route, store
a route in settings, expose a browser-side route picker, share one mutable
catalog across simultaneous browser sessions, discover remote endpoint ids, or
give a catalog product access to Personae. Those are separate configuration,
host-lifetime, and product-policy decisions.
