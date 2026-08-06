# G12 admitted endpoint context

**Date:** 2026-08-06
**Result:** Graphshell can hand an already-admitted projection session and
public-key subject to a product endpoint without handing it a vault, a
delegation chain, or a second transport.

## Contract

`SessionAuthority::endpoint_context` yields an
`AdmittedEndpointContext` containing the transcript-derived
`ProjectionSession` and the admitted `[u8; 32]` subject. The context is not
serializable and contains no delegation, expiry record, revocation state, or
carrier. It is only an in-process composition handoff.

The Graphshell session loop remains around the endpoint and continues to
recheck the retained authority before every request. The context therefore
does not let a product re-admit a browser or carry an identity claim to another
process.

`BindAdmittedSession` gives a product a narrow place to map those facts onto
its own endpoint and authorization model. The existing browser host now uses
the same context when it sends the connected session and subject to the
extension.

## Second consumer

Cleromancy's opt-in `graphshell-admission` feature implements the binding. It
sets the ephemeral projection session on its local endpoint, clears resources
and active instances from any preceding session, and maps the subject into the
existing Servitor `Subject`. Cleromancy does not open Personae or issue a
delegation.

## Evidence

```text
cargo test --features graphshell-admission \
  --test a18_admitted_endpoint --offline
```

The A18 integration proof passed. It bound an admitted context, mounted the
endpoint through Graphshell's retained carrier session, submitted the bounded
Cleromancy concurrence form with the context subject's Servitor grant, and
observed the saved Pattern occasion after resnapshot.

The focused Graphshell lifecycle unit test is
`endpoint_context_carries_only_the_admitted_session_and_subject`. It could not
be run directly from the current Mere workspace because unrelated live Genet
patch configuration names a missing `scroll-protocol` workspace dependency.
The A18 feature build compiled Graphshell's native library; the direct unit
receipt remains pending that unrelated workspace repair.

## Stop rule

This does not add a resident endpoint catalog, browser endpoint selection,
cross-process bearer context, or a Mere-to-Cleromancy dependency. The next
gate is a host-owned catalog that selects an endpoint before it starts the
admitted session loop and keeps that loop as the authority owner.
