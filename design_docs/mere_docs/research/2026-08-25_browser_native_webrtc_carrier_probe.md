# Browser-to-Native WebRTC Carrier Probe

**Date:** 2026-08-25
**Status:** feasibility research complete; production implementation remains open.
**Question:** Can an ordinary browser join a bounded live session whose native
Merely application retains state and authority?

## Ruling

Yes. A browser can run the real Wasm client locally and reach a native host over
an ordered, reliable WebRTC data channel. The application protocol stays above
that carrier, and Notochord remains the sole admission step before Graphshell
sees application bytes.

The first production route should be a direct WebRTC carrier, not Iroh packets
wrapped in WebRTC. The external Iroh transport is useful prior art for
framing, backpressure, and browser interop. It is not the right authority or
signaling base for this lane today.

```text
mer3ly.net join page
  |- versioned Wasm/Cambium client
  |- opaque offer/answer signaling
  `- short-lived TURN credentials
                 |
          WebRTC DataChannel
                 |
native application instance
  |- signed invitation issuer
  |- Notochord admission
  |- authoritative application state
  `- Graphshell projection and intent host
```

Cloudflare may serve the loader, relay signaling messages, and supply TURN.
Those roles do not give it application authority. The native application
decides what is disclosed and which intents are admitted.

## Live seams

The tree is closer to this shape than the older browser-hosting discussion
suggested.

- The [Graphshell remote projection plan](../implementation_strategy/2026-07-22_graphshell_remote_projection_host_plan.md)
  already keeps carrier, admission, application protocol, and product
  authority separate. Its native G5 path is landed. The browser profile is
  explicitly unclaimed until a headed browser-to-native reconnect exists.
- `ports/graphshell/src/browser_carrier.rs` is a local WebExtensions native
  messaging bridge. It proves browser admission adaptation, but it is not a
  remote ordinary-browser carrier.
- `mere-transport::Transport` is a native bilateral known-peer seam with a
  Tokio byte stream. A browser invitation is an inbound accepted session whose
  peer identity is established by Notochord, so forcing it to implement the
  whole known-peer trait would invent semantics. The reusable entry point is
  an `AcceptedSession<S>` handed into the existing admission path.
- Notochord's handshake core is sans-I/O. `SessionHello`, `SessionReply`,
  transcript verification, policy, and Personae delegation evaluation can run
  in Wasm while the browser driver supplies message I/O.

This yields a small boundary change: split Graphshell's current
`accept_projection_session(&T)` into an accepted-stream admission function plus
the existing transport wrapper. WebRTC can construct honest carrier facts and
call the first function without pretending to be Iroh or changing Graphshell's
protocol.

## Physical probe

The external [`iroh-webrtc-transport`](https://github.com/SuddenlyHazel/iroh-webrtc-transport)
was built from commit `fdb511d` in a temporary checkout. Its browser example
was compiled in release mode for `wasm32-unknown-unknown`, bound with the
installed `wasm-bindgen` CLI, and served locally. The only source adjustment
was moving its exact `wasm-bindgen` pin from 0.2.118 to the installed 0.2.126;
the transport logic was unchanged.

A headed Chromium session dialled the native example and returned `pong` over
the WebRTC data channel. This is a real browser-Wasm to native WebRTC receipt,
not a native-to-native inference. The generated example Wasm was 7,060,115
bytes before Graphshell or Cambium, which makes it a feasibility donor rather
than a payload baseline.

The probe also found a live donor defect: the browser latency benchmark opens
`iroh-webrtc-transport/benchmark/1`, while the native example listens on
`iroh-webrtc-transport/worker-benchmark/1`. Ping works; the advertised latency
benchmark waits forever. That mismatch is further reason to borrow the
mechanics selectively.

The donor is an experimental alpha pinned to Iroh 0.98.2 and `webrtc` 0.17.1.
Mere uses Iroh 1.0.3. It uses Iroh bootstrap and relay paths for signaling,
does not configure TURN, and transports encrypted Iroh/noq packets inside the
data channel. Porting it would therefore couple browser join to the native Iroh
addressing plane before the simpler carrier is proven.

A separate external-consumer probe compiled local `notochord` and `personae`
for `wasm32-unknown-unknown`. The consumer had to enable JavaScript randomness
for both `getrandom` 0.3 and 0.4 and the `uuid` `js` feature. With those explicit
consumer features, `SessionHello` issue, encode, decode, and proof verification
compiled successfully. A direct workspace `--locked` check did not reach
compilation because the existing lock records `knot-editor-host` 0.1.0 while
its selected Git source now presents 0.1.1. That is baseline resolver drift,
not a WebRTC or Notochord failure.

## Invitation identity

An ordinary browser does not begin with a persistent Personae profile. The
private invitation can still enter the existing authority grammar rather than
adding a bearer bypass.

`InviteV1` carries the rendezvous reference, a random redemption secret, the
expected host key, permitted action, expiry, and application release identity.
It is encoded in the URL fragment. The browser removes the fragment from
visible history, generates its own ephemeral Personae subject locally, and
proves possession of the redemption secret over the host-authenticated data
channel. The native host then issues that public subject a short-lived
delegation for exactly `mere.graphshell / /services/projection / connect`.
The browser uses its locally held key and the returned delegation in the normal
Notochord handshake.

This is better than putting a host-generated subject seed in the fragment. The
host can authorize the guest but cannot impersonate the admitted guest
principal. Possession of the fragment is the right to redeem the bounded
delegation. It does not authorize a native executable, alter the user's
persistent Personae identity, or grant a different service action. Installed
native peers continue to use their normal persistent identity and grants.

The join page needs a strict first-party CSP and must avoid third-party script,
analytics, and referrer-producing subresources before the fragment is cleared.
The rendezvous service receives only the public invite reference and opaque
signaling envelopes.

## Binding admission to the WebRTC connection

WebRTC authenticates the DTLS connection described by the exchanged
fingerprints, but it does not establish a Personae subject. Notochord must see
`authenticated_initiator: None` and establish the subject itself.

Leaving `shared_link` empty would let a captured signed hello be replayed on a
second WebRTC connection. The carrier therefore needs a small pre-admission
link challenge:

1. The browser contributes a fresh client nonce after the data channel opens.
2. The host contributes a fresh server nonce and signs a role-tagged transcript
   containing both nonces, the protocol and channel label, the invite id, and
   the browser and host SHA-256 DTLS fingerprints from the negotiated SDP.
3. The browser checks the signature against the host key in `InviteV1` and
   verifies that both fingerprints match its local and remote descriptions.
4. Both sides derive the same 16-byte `shared_link` from that transcript.
5. The browser binds its locally generated subject to the transcript with a
   proof of the redemption secret; the host returns the narrow delegation.
6. The browser issues its Notochord hello against that binding. The host builds
   `SessionFacts` from the accepted channel and the independently derived link.

The fresh host nonce blocks replay onto a later channel. Fingerprint binding
blocks a signaling intermediary from terminating two separate DTLS sessions
and relaying the challenge. Application messages remain withheld until the
Notochord reply accepts the delegated subject.

The link transcript and fingerprint parser need fixed test vectors and a
focused security review before public deployment. A DTLS exporter would be a
simpler primitive, but browsers do not expose one to ordinary application
JavaScript.

## Stack choice

Three implementations remain worth distinguishing.

| Route | Value | Problem for the first Mere slice |
|---|---|---|
| Browser `web-sys` plus native `webrtc-rs` | Directly models the desired browser/native session; TURN configuration is explicit | Native dependency is still moving and must be pinned by a receipt |
| `str0m` | Sans-I/O and unusually compatible with Mere's explicit host/runtime boundaries | Socket, ICE, and TURN integration become Mere's responsibility immediately |
| Iroh custom WebRTC transport | Reuses Iroh streams and demonstrates browser/native interop | Experimental, version-skewed, large in the donor build, Iroh-signaled, and lacks TURN |

The first implementation should use the browser's WebRTC API and a pinned
native WebRTC implementation behind a narrow carrier module. `str0m` remains
the fallback if the native stack prevents reliable cancellation, TURN, or
backpressure. The Iroh custom transport becomes an optional later adapter if a
real native-peer composition benefit survives measurement.

## Open physical gates

The feasibility probe proves one direct ping. It does not close the browser
carrier profile. That claim requires:

- direct and forced-TURN candidate receipts;
- bounded framing and `bufferedAmount` backpressure under load;
- host-authenticated DTLS fingerprint binding;
- accepted, expired, revoked, and replayed invitation cases;
- signaling substitution and second-connection replay rejection;
- ICE restart and Graphshell snapshot-or-diff resume;
- one disclosed Graphshell projection and one admitted intent in a headed
  ordinary browser;
- a receipt showing the signaling service cannot authorize a session or read
  Graphshell application payloads.

Artifact seeding, native release adoption, media tracks, and Gemini publishing
remain separate work. They should not enter this carrier until the live-session
boundary closes.

## Sources

- [W3C WebRTC](https://www.w3.org/TR/webrtc/)
- [Cloudflare TURN](https://developers.cloudflare.com/realtime/turn/)
- [Cloudflare TURN credential generation](https://developers.cloudflare.com/realtime/turn/generate-credentials/)
- [`webrtc-rs`](https://github.com/webrtc-rs/webrtc)
- [`str0m`](https://github.com/algesten/str0m)
- [Iroh WebRTC transport discussion](https://github.com/n0-computer/iroh/discussions/4024)
- [`iroh-webrtc-transport`](https://github.com/SuddenlyHazel/iroh-webrtc-transport)
