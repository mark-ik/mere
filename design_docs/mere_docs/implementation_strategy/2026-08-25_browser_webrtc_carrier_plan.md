# Browser WebRTC Carrier Plan

**Date:** 2026-08-25
**Status:** planned; bounded feasibility research is complete, implementation
has not started.
**Scope:** Let an ordinary browser join a bounded live Graphshell session whose
native application retains state and authority. Build the carrier and admission
proof before adding public rendezvous infrastructure.

**Related:**

- [browser-to-native WebRTC feasibility](../research/2026-08-25_browser_native_webrtc_carrier_probe.md)
- [Graphshell remote projection host](2026-07-22_graphshell_remote_projection_host_plan.md)
- [Graphshell reference host](2026-07-27_graphshell_reference_host_plan.md)
- [reachability rungs and privacy lanes](2026-08-03_reachability_rungs_and_privacy_lanes_plan.md)
- [net-media plan](2026-05-26_net_media_plan.md)

## 1. Ruling

Build a direct browser-to-native WebRTC data-channel carrier. Keep these layers
distinct:

1. `mer3ly.net` serves a versioned client and relays opaque signaling.
2. WebRTC supplies an encrypted, ordered, reliable peer data channel, using
   TURN only when direct connectivity fails.
3. A carrier challenge authenticates the native host and derives Notochord's
   connection-specific `shared_link`.
4. The browser redeems the private capability into a narrow delegation for a
   locally generated ephemeral Personae subject.
5. Notochord admits that subject before the application receives a byte.
6. Graphshell carries projections, diffs, resources, and admitted intents.
7. The native application remains authoritative for state and action.

The Iroh custom-transport donor is a later optional adapter. This plan does not
make it the browser join path.

## 2. Boundaries

```text
Cloudflare loader/signaling/TURN
          |
WebRTC offer/answer and ICE
          |
webrtc-carrier
  |- bounded data-channel frames
  |- host challenge + DTLS fingerprint binding
  `- honest AcceptedSession facts
          |
Notochord SessionHello / SessionReply
          |
Graphshell session protocol
          |
product endpoint and native authority
```

The following rules hold through every phase:

- The WebRTC carrier reports no authenticated initiator. Notochord establishes
  the ephemeral or persistent Personae subject.
- Signaling data never becomes a carrier fact merely because a Worker relayed
  it.
- `InviteV1` grants only a named service action, has a bounded expiry, and does
  not alter executable trust.
- A refused stream never reaches Graphshell.
- Browser and native share wire types and test vectors, not one runtime.
- The current bilateral `Transport` trait remains intact until a second
  listener-only carrier proves a shared acceptor trait useful.
- Application state, graph authority, and intent policy stay in the native
  product endpoint.

## 3. C0: accepted-stream seam and carrier core

Create one Wasm-clean `webrtc-carrier` crate under the Murm transport family.
Its default core contains only bounded frame types, invitation identifiers,
the link-challenge transcript, fingerprint canonicalization, and shared-link
derivation. Native and browser runtimes are feature-selected adapters.

In `mere-transport`:

- add `TransportKind::WebRtc` and an `IngressContext::webrtc(shared_link)`
  constructor;
- map it honestly to `notochord::CarrierKind::Other` until a distinct public
  Notochord variant has a policy consumer;
- keep `peer: None` on accepted browser sessions.

In Graphshell, extract an accepted-stream function from
`accept_projection_session`. It receives `AcceptedSession<S>`, maps its facts
through the existing audited adapter, runs Notochord, checks the served action,
and returns `AdmittedSession<S>`. The existing `Transport` wrapper calls it
after `transport.accept`. The WebRTC listener calls it directly.

**Done when:**

- the core builds for native and `wasm32-unknown-unknown`;
- role-tagged fingerprint and shared-link vectors match on both targets;
- existing Memory, P2panda, Reticulum, and Noise mappings remain exhaustive;
- existing Graphshell carrier tests pass through the extracted function;
- a WebRTC acceptance fixture reaches the same function with `peer: None` and
  a nonempty shared link.

## 4. C1: direct data channel

Add a native answerer and browser initiator around the shared core. Use an
ordered, reliable data channel. Define explicit maximum frame and queued-byte
limits. The browser driver must stop sending above its high-water mark and
resume only after `bufferedAmount` crosses a configurable low-water mark. The
native stream adapter must propagate close, cancellation, and write failure.

Use local copy/paste or a tiny loopback signaling fixture for this phase. Public
infrastructure is deliberately absent so transport behaviour can be inspected
without mixing it with service deployment.

Record the exact native WebRTC dependency, feature graph, native binary delta,
browser Wasm delta, and why that version was selected. Compare current
`webrtc-rs` with `str0m` at this gate. A version choice is an evidence result,
not a permanent product rule.

**Done when:**

- a headed ordinary browser opens, exchanges bounded ping frames, and closes
  cleanly against a native process;
- oversize frames are rejected before allocation or deserialization;
- a sustained transfer observes the configured high and low water marks;
- browser refresh and native cancellation terminate both tasks without a hot
  loop;
- the receipt records direct ICE candidate selection and measured artifact
  deltas.

## 5. C2: invitation and Notochord admission

Define `InviteV1` as a versioned, bounded fragment payload containing:

- public rendezvous id;
- random redemption secret;
- expected native host public key;
- network and profile references;
- permitted service action;
- expiry and one configurable session-use ceiling;
- signed application release identity for display, not executable adoption.

The browser imports the invitation into memory and clears the fragment from
visible history before loading any optional resource, then generates an
ephemeral Personae subject locally. The host challenge binds the protocol,
data-channel label, invite id, fresh client and server nonces, and role-tagged
SHA-256 DTLS fingerprints. The host signs that transcript. Both sides derive
Notochord's 16-byte `shared_link` from it.

After verifying the host, the browser proves possession of the redemption
secret, bound to the challenge transcript and its ephemeral public subject.
The host keeps only the redemption verifier and use state, then issues the
subject a short-lived delegation for the permitted action. It never learns the
browser's private subject key. The browser uses the returned delegation and the
existing Personae/Notochord sans-I/O path to issue `SessionHello`. The host
supplies `SessionFacts` from its accepted channel and runs the current
Graphshell admission function. It writes a refusal or accepted reply before
Graphshell opens.

**Done when:**

- a valid invite admits the browser-generated ephemeral subject and exact
  Graphshell action without disclosing the subject's private key to the host;
- a one-use redemption cannot mint a second delegation;
- altered action, expired certificate, revoked delegation, wrong host key,
  substituted SDP fingerprint, and oversized invite all fail closed;
- a captured hello replayed on a second WebRTC connection fails because the
  host challenge and shared link differ;
- a refused session cannot send `SessionOpen` successfully;
- browser logs, server logs, referrers, and signaling records contain neither
  the fragment nor its redemption secret.

## 6. C3: forced relay and reconnect

Add configurable STUN/TURN servers and credential expiry. First run against a
controlled TURN service where direct candidates can be disabled. Then run the
same receipt against Cloudflare TURN using credentials minted server-side from
the long-term key. The long-term key never reaches the browser.

Exercise ICE restart before short-lived TURN credentials expire. Treat a new
DTLS connection as a new carrier link and run the host challenge plus Notochord
admission again. Graphshell may resume from its last acknowledged revision only
after that fresh admission.

**Done when:**

- the selected candidate pair is demonstrably relay-only in the forced case;
- the TURN service carries encrypted WebRTC packets but never receives a
  Graphshell key or application capability;
- credential expiry causes refresh or a clean failure, not an infinite retry;
- direct loss followed by relay recovery runs fresh admission and resumes by
  diff when the retained revision is valid, otherwise by snapshot;
- the same test matrix passes with direct connectivity restored.

**Stop line:** do not begin product or public-service wiring if C0 through C3
cannot demonstrate bounded backpressure, connection-bound admission, and a
forced relay. Revisit the native WebRTC stack or move the runtime adapter to
`str0m` while preserving the core and receipts.

## 7. C4: real Graphshell session

Replace the ping protocol with the current Graphshell web client and native
projection host. The browser requests one disclosed score, receives a snapshot,
applies a diff, disconnects, and resumes. It invokes one permitted intent and
receives one policy refusal for a second intent.

The browser rendering surface should use the shared Cambium web host. This
phase does not duplicate Graphshell's protocol or create an iframe-only app
model. The same client component must be usable as the full application and as
an embed.

**Done when:**

- a headed browser renders a native-owned projection through the real
  Graphshell protocol;
- snapshot, diff, reconnect, admitted intent, and refused intent are present in
  one machine-readable receipt;
- the native state revision changes only for the admitted intent;
- keyboard and accessibility-tree checks pass in the embedded and full-page
  surfaces;
- the browser carrier profile in the remote projection plan can be marked
  physically proven.

## 8. C5: public rendezvous

Deploy `https://mer3ly.net/join/<rendezvous>#<private-capability>` only after C4.
The join route serves a pinned client manifest and security headers. A small
Worker brokers opaque offer, answer, and ICE messages and mints short-lived
TURN credentials. A Durable Object is permissible for bounded rendezvous
mailboxes and presence; it does not hold application state or decide admission.

The service stores only operationally necessary, expiring rendezvous material.
Rate, size, and lifetime limits are user-configurable at the native host within
safe protocol ceilings. The page displays host publisher, source revision,
release identity, requested action, and invite expiry before joining.

**Done when:**

- a fresh browser with no extension joins through the public URL;
- the URL fragment is absent from HTTP requests and service logs;
- signaling substitution cannot impersonate the signed native host;
- destroying or revoking the invite prevents a later join;
- direct and TURN paths both close the C4 receipt through the public service;
- service deletion leaves the native application state intact.

## 9. C6: optional Iroh adapter

After the direct carrier is stable, port the external Iroh custom transport only
if a concrete consumer needs Iroh stream compatibility inside the browser. Pin
it to the workspace's current Iroh line, add explicit TURN or retain the direct
carrier as fallback, and compare:

- incremental Wasm and native size;
- connection setup and resume latency;
- browser memory and queued-byte behaviour;
- relay dependencies and failure modes;
- whether it removes application code rather than merely relocating it.

Adopt it only if those receipts beat the direct carrier for a named consumer.
The direct WebRTC/Notochord path remains valid regardless of that result.

## 10. Outside this plan

- signed native artifact seeding and installation;
- repository/update-feed adoption;
- Gemini or HTML projection from Knot;
- WebRTC audio/video tracks and media decode;
- multi-host fanout, SFU operation, and cloud-held graph state.

Each becomes separate work after the live browser session closes its authority,
relay, and reconnect receipts.

## Findings

- A headed browser-to-native data-channel ping is physically proven with an
  external donor.
- Local Notochord and Personae compile for Wasm when JavaScript randomness
  features are explicit.
- The donor's 7,060,115-byte example Wasm, Iroh version skew, absent TURN, and
  benchmark ALPN mismatch rule out adopting it unchanged.
- The current `Transport` trait is wider than an inbound browser invitation.
  Reusing `AcceptedSession` preserves honest carrier facts without weakening
  the trait.
- A URL capability can enter the existing Personae delegation grammar by
  redeeming into a delegation for a browser-generated ephemeral subject.
- WebRTC needs an explicit host-signed fingerprint challenge to supply
  Notochord's connection-specific link binding.

## Progress

- **2026-08-25:** live seams and active plans reconciled; external donor built;
  headed browser/native ping passed; donor benchmark mismatch identified;
  Notochord/Personae external Wasm compile passed; architecture and gates
  recorded. Production code remains untouched.
