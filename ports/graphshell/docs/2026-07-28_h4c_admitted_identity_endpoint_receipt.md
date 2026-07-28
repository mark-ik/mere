# H4c admitted identity endpoint receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** portable endpoint and admitted-session application path passed; H4 remains in progress

## Product cut

`IdentityEndpoint` adapts the resident Personae authority to Graphshell's
ordinary endpoint vocabulary:

- one discoverable Identity projection;
- Scenograph card instances backed by content-addressed `PortableCardV1`
  resources;
- memory-only caching with purge-on-revocation;
- advertised typed actions dispatched only from the card that disclosed them;
- revision checks before every mutation.

The adapter stays in native Graphshell. `graphshell-protocol` and
`graphshell-client` remain independent of Personae.

## Admission boundary

The carrier-facing constructor receives `SessionAuthority` and uses its
transcript-derived projection session. The browser cannot choose or restate
that identity. `SessionHello` remains the sole admission step below the
application protocol.

The focused receipt drove an `AdmittedSession` through the existing
`serve_admitted_session` loop. A portable `ClientState` opened the session,
mounted the identity scene, fetched every card resource, found a pending
approval action, reconstructed its typed payload from the public request ID,
and approved it. The waiting real Personae SSH adapter returned a signature
which verified against the disclosed public key.

Every request also passed the existing per-request expiry and revocation
checks. The same focused suite retains the separate `SessionHello` admission,
captured-hello refusal, reconnect, expiry, and revocation tests.

## Secret boundary

The pending card now includes its public request UUID so the browser does not
need a hidden native payload channel. It still contains only the payload
digest, public key reference, operation, and proven requester context.

The admitted-session test fetched and decoded every portable card resource.
Neither the private OpenSSH encoding nor a private-key header appeared in the
snapshot or resources.

## Evidence

- Isolated live-source Graphshell suite: 36 tests passed.
- The admitted identity test mounted public cards through the session loop,
  invoked approve-once through the disclosed action, and verified the resulting
  signature.
- `IdentityEndpoint` direct tests reject an action issued against a card that
  did not advertise it.

The suite compiled live source against real path dependencies in an isolated
manifest. This receipt does not yet claim a headed Chromium or Firefox session.

## H4 gates still open

- implement and prove the actual browser-to-device carrier;
- capture the identity scene in the headed browser and repeat an approval
  through that carrier;
- wire the native import picker and passphrase interaction;
- bind the standard SSH-agent endpoint and prove a real login after restart;
- own launch-at-login and crash recovery, then retire the standalone task;
- add carry mutation intents and mixed-scene reopen.

The standard endpoint and live scheduled task remain unchanged.
