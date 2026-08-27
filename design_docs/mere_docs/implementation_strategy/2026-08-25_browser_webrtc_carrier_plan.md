# Browser WebRTC Carrier Plan

**Date:** 2026-08-25
**Status:** in progress. C0-C2 landed 2026-08-26; C3's code landed 2026-08-26,
its forced-relay and reconnect RECEIPTS await a live TURN service (Mark to
supply). C4 is otherwise the next code phase.
**Scope:** Let an ordinary browser join a bounded live Graphshell session whose
native application retains state and authority. Build the carrier and admission
proof before adding public rendezvous infrastructure.

**Related:**

- [browser-to-native WebRTC feasibility](../research/2026-08-25_browser_native_webrtc_carrier_probe.md)
- [Graphshell remote projection host](2026-07-22_graphshell_remote_projection_host_plan.md)
- [Graphshell reference host](2026-07-27_graphshell_reference_host_plan.md)
- [reachability rungs and privacy lanes](2026-08-03_reachability_rungs_and_privacy_lanes_plan.md)
- [net-media plan](2026-05-26_net_media_plan.md)
- [Djinn family resident services](2026-08-22_djinn_family_resident_services_plan.md)
- [auto-update brief](../../2026-07-22_auto-update_brief.md)
- [Luggage current API](../../../crates/system/luggage/README.md)

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

The Iroh custom-transport donor is superseded as a candidate: iroh 1.1 ships
first-party browser support that needs no port and carries no version skew
(Findings, 2026-08-26). That path is the **second lane** of §9, for networks
where WebRTC is blocked. Neither is the browser join path.

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
- Its Luggage release reference is a claim until a separately trusted publisher
  key verifies the exact signed manifest. Neither the host's Personae key nor a
  feed location grants that trust.
- The ordinary first visit still trusts the `mer3ly.net` HTTPS origin to deliver
  the bootstrap verifier honestly. Luggage constrains later manifest and
  artifact substitution; it does not erase initial web-origin trust.
- Joining never installs a release or changes publisher, feed, channel, or
  update policy.
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

**Landed 2026-08-26.** All five conditions met; see Findings and Progress.

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
- `ReleaseRefV1 { manifest_blake3, publisher_key_id }` from Luggage.

The release reference identifies the exact signed manifest bytes and the
publisher-key identity used to look up trust. It carries neither the manifest,
public key, nor feed. The host-signed invite descriptor binds the reference so
signaling cannot substitute a different release claim. Publisher verification
remains a separate loader decision and never contributes to Notochord
admission.

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
  the fragment nor its redemption secret;
- altering the bound release reference is rejected, while an unknown publisher
  remains joinable only through a compatible already-trusted generic client.

**Landed 2026-08-26** except the headed browser-log/referrer/signaling hygiene
check, which needs a real browser and travels with C4's client work; the
structural half (redacted `Debug`, no secret in any log path, `#`-tolerant
fragment parsing) is in. See Findings and Progress.

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
The join route serves a small pinned bootstrap loader and security headers. The
loader resolves the invitation's `ReleaseRefV1`, verifies the detached manifest
signature against its own publisher trust store, selects the browser artifact,
and verifies its digest and signature before importing JavaScript or
instantiating Wasm. A small Worker brokers opaque offer, answer, and ICE
messages and mints short-lived TURN credentials. A Durable Object is
permissible for bounded rendezvous mailboxes and presence; it does not hold
application state or decide admission.

The service stores only operationally necessary, expiring rendezvous material.
Rate, size, and lifetime limits are user-configurable at the native host within
safe protocol ceilings. The page displays verified or unknown publisher,
source revision, release identity, client selection, requested action, and
invite expiry before joining. Exact-release client selection is preferred.
A separately trusted generic Graphshell client may join a signed-compatible
wire version without claiming to be the host's exact release.

**Done when:**

- a fresh browser with no extension joins through the public URL;
- the URL fragment is absent from HTTP requests and service logs;
- signaling substitution cannot impersonate the signed native host;
- destroying or revoking the invite prevents a later join;
- direct and TURN paths both close the C4 receipt through the public service;
- official release manifests and browser bundles verify before client code
  executes, while an altered bundle is refused before import;
- an unknown fork publisher is never described as Merely-signed and cannot add
  itself to the trust store through the invitation;
- service deletion leaves the native application state intact.

## 9. C6: Luggage release offer and native adoption

Once the public live-session receipt closes, make the release reference useful
beyond display. Luggage owns a Wasm-clean signed-envelope core; its native side
keeps feed polling, verified staging, update policy, and platform apply. The
signed manifest names application id, version, source revision, supported
invitation and Graphshell wire versions, and artifacts by kind and target.

Artifact locators live in a disposable `ReleaseOfferV1`, outside the signed
content manifest. A mirror, native host, or peer may change without changing
the release identity. The source revision inside the manifest is a publisher
claim; a reproducible-build receipt remains separate.

“Install this release” offers the same `ReleaseRefV1` plus the exact platform
artifact selected from its verified manifest. Artifact bytes may arrive from
`assets.mer3ly.net`, the native host over the admitted session, or an Iroh peer.
The source does not affect verification. Existing installations hand a
verified self-sufficient offer to Luggage and Djinn's configured policy. A
fresh browser may verify and download an OS installer, but installation remains
an explicit operating-system action.

Adopting a fork requires two visible changes: its trusted publisher key and its
configured update feed. Joining the fork's live session performs neither.

**Done when:**

- one signed manifest names distinct browser, Windows, Linux, and macOS hashes
  under one release identity and source revision;
- native and browser verifiers derive the same `ReleaseRefV1` from frozen
  vectors;
- corrupted manifests and artifacts fail identically whether bytes came from
  HTTPS, WebRTC, or Iroh;
- changing only offer locators preserves the release reference, while changing
  any signed content fact changes it;
- the host can seed an exact platform artifact by content hash without becoming
  publisher authority;
- an installed app stages and applies the offer through Luggage's
  self-sufficient signed stage, while a browser download produces a verification
  receipt and waits for explicit installation;
- accepting a session leaves publisher trust, feeds, channels, and update
  policy byte-for-byte unchanged.

## 10. C7: the iroh browser relay as a second lane

Direct WebRTC is the join path. This phase exists for the case it cannot serve:
a network that blocks WebRTC outright, where a relayed session beats no session.
It is a **fallback, not a competitor** — it does not start until C4 closes, and
it does not dilute the direct carrier's receipts.

The candidate is **iroh 1.1's own browser support, not the external donor**. It
needs no port, no version bump, and no new manifest entry: `iroh-relay` 1.1.0
declares a `wasm32-unknown-unknown` target block using `ws_stream_wasm`, and that
crate is already in this workspace's lock. A browser `Endpoint` binds with
`presets::N0` and dials by `EndpointId` over an ALPN — the shape
`mere-transport::Transport` already has, so the identity plane is preserved
rather than rebuilt.

Two properties are settled and need no re-litigating (Findings, 2026-08-26):

- it is **relay-only**. A browser tab cannot hole-punch, so this lane always
  carries traffic through a relay. That is the price of the fallback, not a
  defect to engineer away.
- it costs **10.8x the browser payload** of the direct carrier, because iroh
  compiles into the wasm rather than being supplied by the browser. That is why
  it is the second lane and not the first.

What must be proven before adopting it, per lane rather than per crate:

- a headed browser reaches a native host through a relay, and Notochord admits
  the session on facts the relay cannot forge;
- `shared_link` binds iroh's connection identity — there are no DTLS
  fingerprints here, so the C0 transcript survives but `fingerprint.rs` does not;
- reconnect runs fresh admission, exactly as C3 requires of the direct lane;
- the relay operator learns no Graphshell key and no application capability;
- setup and resume latency measured against the direct carrier, and the payload
  cost re-measured optimised (`wasm-opt`, `opt-level="z"`, LTO, `panic="abort"`),
  since both current figures are unoptimised floors.

Adopt it only for a named consumer the direct carrier cannot reach. The direct
WebRTC/Notochord path remains the product path regardless of the result.

## 11. Outside this plan

- Gemini or HTML projection from Knot;
- WebRTC audio/video tracks and media decode;
- multi-host fanout, SFU operation, and cloud-held graph state;
- operating-system code-signing procurement and unattended first installation.

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
- Luggage, rather than `InviteV1`, owns release meaning. The invite carries only
  a signed-manifest hash and publisher-key identity; release trust stays in the
  loader or installed application's publisher store.
- Exact release identity and protocol compatibility are related but distinct.
  Exact artifacts make receipts reproducible; a compatible trusted generic
  client preserves interoperability without pretending to be that release.
- Luggage narrows byte-source authority but cannot secure an ordinary browser
  against a compromised first-load web origin that replaces the verifier
  itself. That bootstrap trust stays explicit.
- **2026-08-26:** WebRTC is structurally the Reticulum case, not a new proof
  shape. `IngressContext::webrtc(shared_link)` fills the same
  `ingress.link` slot Reticulum uses, and the derived
  `SessionFacts::proof_binding()` is equal to
  `transport::initiator_link_binding(&alpn, link)` — asserted in
  `crates/murm/transport/src/notochord.rs`
  (`a_webrtc_binding_pins_the_link_and_names_no_peer`). C2 therefore redeems
  into the existing Personae delegation grammar rather than adding one.
- **2026-08-26:** the carrier vocabulary has exactly one mapping site.
  `carrier_of` in `crates/murm/transport/src/notochord.rs` is the only match on
  `TransportKind` in the workspace; `notochord/src/facts.rs:19` mirrors the
  enum in prose only. Adding a carrier is a one-file change, and the
  exhaustiveness guarantee is real rather than assumed.
- **2026-08-26:** Graphshell's accept path splits cleanly at the accept call.
  `admit_accepted_session` in `ports/graphshell/src/carrier.rs` takes
  `AcceptedSession<S>` where `S: AsyncRead + AsyncWrite + Unpin` — the bounds
  `notochord::admit_session` already requires, with no `Send`/`'static` added,
  which a browser-side stream could not have satisfied. All five existing call
  sites compile unchanged.
- **2026-08-26:** a transcript-link derivation already existed.
  `ports/graphshell/src/browser_carrier.rs` derives a 16-byte link for the same
  Notochord slot over the WebExtensions bridge. `webrtc-carrier` was written
  against SHA-256 and was unified onto browser_carrier's discipline — blake3,
  domain raw at the front, `u64` little-endian length prefixes — so one
  derivation convention holds across carriers. The frozen vector moved from
  `383d1883…5960` (SHA-256, `u32be`) to `4692c88e70470f7e4b7ba46b7fce78b2`
  (blake3, `u64le`, 280-byte transcript).
- **2026-08-26:** the browser payload decides this, and the donor's size was
  never an argument against WebRTC. Three routes, built release for
  `wasm32-unknown-unknown` the same unoptimised way and each verified to contain
  its transport rather than having been dead-code-eliminated:

  | route | raw | gzipped |
  |---|---|---|
  | direct WebRTC (C0 core + `web-sys`) | 540,025 | 116 KB |
  | iroh 1.1 first-party browser client | 5,828,687 | 1.57 MB |
  | donor `iroh-webrtc-transport` example | 7,060,115 | — |

  The browser *is* a WebRTC implementation, so a direct carrier ships only the
  C0 core and bindings; iroh must be compiled into the payload. "Already in the
  dependency graph" is a native saving that inverts in the browser. The donor's
  7 MB indicted putting Iroh in a browser, not WebRTC.
- **2026-08-26:** iroh 1.1 has first-party browser support, which the 2026-08-25
  probe never tested — it evaluated the donor instead. `Endpoint`,
  `builder(presets::N0)`, `bind()` and `connect(EndpointId, alpn)` all
  type-check for `wasm32-unknown-unknown` with **no addition to this
  workspace's manifests**: `iroh-relay` 1.1.0 declares a wasm target block
  using `ws_stream_wasm`, already resolved in `Cargo.lock`. It is relay-only —
  a browser cannot hole-punch — and carries no version skew, unlike the donor.
- **2026-08-26:** the dual-target receipt was not reproducible as written.
  `rust-toolchain.toml` declared only `wasm32-wasip2`, so C0's
  `wasm32-unknown-unknown` check passed only on machines that happened to have
  the target installed. The target is now declared.
- **2026-08-26 (C1 stack choice):** measured, not asserted. Against a
  tokio-only baseline, `webrtc` 0.20.3 adds 199 crates and ~329 MB of compiled
  code; `str0m` 0.23.1 adds 76 crates and ~214 MB. str0m selected — also for
  its Sans-IO shape, which matches Notochord's sans-I/O handshake core and the
  deliberately I/O-free carrier core. Features: `default-features = false,
  features = ["rust-crypto"]`, avoiding `aws-lc-sys` (C/asm, needs cmake and a
  C toolchain on every validation target). Native feature delta measured:
  12 -> 114 crates, +314,752,494 bytes of release artifacts. Per the plan's own
  rule this is an evidence result, not a permanent product rule.
- **2026-08-26 (C1 headed receipt, session 2):** a real Chrome and the native
  str0m answerer: ICE `checking -> connected` on a direct host pair
  (7 binding round trips), 200 frames echoed both ways, 3,277,600 bytes each
  way (payload + exactly 200 x 4-byte headers), `malformed_datagrams: 0`,
  clean close ("peer closed the channel"), both DTLS fingerprints captured.
  Fingerprints are read on `Event::Connected` — the certificate actually
  presented, not the SDP's claim (str0m fills `remote_dtls_fingerprint` only
  after verifying the presented certificate). C2 binds the stronger value.
- **2026-08-26 (headed-only defects):** the headed run found four defects that
  compile checks, unit tests, and the loopback suite all passed over:
  `create_offer` treated the `RTCSessionDescriptionInit` *dictionary* that
  `createOffer()` resolves to as a failed prototype cast, reporting every
  valid offer as an error; two browser handlers held a `RefMut` across user
  callbacks and tripped wasm-bindgen's reentrancy guard during ICE gathering;
  `Driver` handed its 0.0.0.0 bind address to `Receive::new`, so str0m
  discarded every inbound datagram as addressed to no local candidate
  (signature: 78 STUN requests sent, zero answered, both sides parked in
  `checking`); and a peer vanishing mid-transfer (browser refresh with ~400 KiB
  buffered) crashed the native process with a tokio worker stack overflow.
  The first two are fixed; the last two are in repair as this is written.
- **2026-08-26 (constraint for C2/C5):** default Chrome emits ONLY
  mDNS-obfuscated `.local` host candidates, and str0m has no mDNS resolver, so
  a candidate-bearing offer requires either an mDNS answer path, an ICE/TURN
  server, or a browser flag no ordinary user will set. str0m also performs no
  peer-reflexive discovery from an empty remote candidate list — an offer
  stripped of candidates hangs both ends in `checking` with no error. The C5
  rendezvous design must treat "at least one resolvable remote candidate" as a
  hard precondition, and the probe now refuses to signal a candidate-free
  offer rather than letting it fail silently.
- **2026-08-26 (C2 redemption scheme):** the redemption secret is an ed25519
  seed. The browser derives the redemption keypair and signs
  `redemption_signing_bytes(challenge, subject)`; the host stores only the
  redemption PUBLIC key plus use-state and expiry, so a stolen host store
  cannot mint redemptions. A refused redemption costs the invitation nothing
  (`a_refused_redemption_costs_the_invitation_nothing`), and the proof binds
  both the challenge transcript and the ephemeral subject, so it neither
  crosses connections nor transfers between subjects.
- **2026-08-26 (C2 signing discipline):** no bare master key signs anything —
  both host signatures (invite descriptor, challenge transcript) use one
  derived key under `mere.graphshell/webrtc-host-signing/v1` with a
  `DerivedKeyAttestation` traveling alongside, the same shape
  `SignedDelegationCertificate` and `SessionHello` already use. Message-level
  separation lives in three frozen wire domains:
  `mere.webrtc-carrier/invite-descriptor/v1`,
  `mere.webrtc-carrier/host-challenge-signature/v1`,
  `mere.webrtc-carrier/redemption-proof/v1`.
- **2026-08-26 (C2 authority boundaries):** `mint_delegation` takes the trust
  root as an explicit host-side parameter — an invitation carrying its own
  root would choose which authority admits it. The mint reads the HOST's copy
  of the invite (a client-presented copy could choose its own scope), the
  minted scope is the invitation's exact action triple at delegation depth 0,
  and the grant's expiry is clamped to `min(now + ttl, invite expiry)` — the
  owner's bound on the offer beats the host's bound on one session.
- **2026-08-26 (C2 matrix):** 26 fail-closed rows in
  `ports/graphshell/src/webrtc_door.rs`, each asserting the exact
  `DenyReason`/`ChainFault`/`RedemptionRefusal` variant with positive controls
  first, plus one integration receipt over the real str0m loopback using
  certificate-actually-presented fingerprints
  (`a_real_webrtc_channel_admits_an_invited_browser`). Cost worth knowing: the
  loopback test's dev-dependency feature pulls str0m (~76 crates) into every
  graphshell test build.
- **2026-08-26 (C3 relay is browser-side):** str0m accepts a remote relay
  candidate unconditionally (`add_remote_candidate` never inspects kind,
  is-0.11.0/agent.rs:729), so forcing relay is entirely the browser's
  `iceTransportPolicy: "relay"`. The native side needs no relay config. TURN
  credentials are minted server-side by the coturn REST convention
  (`username = "<expiry>:<name>"`, `credential = base64(HMAC-SHA1(secret,
  username))`) at the answerer's `/turn-credentials` endpoint; the long-term
  secret never reaches the browser. The HMAC is verified against RFC 2202 and
  a coturn interop vector, so a live TURN server will accept a minted
  credential because the bytes already match the reference.
- **2026-08-26 (C3 reconnect = re-admission, not restart):** two distinct
  events. An ICE RESTART (browser `create_restart_offer`, str0m detects the new
  ufrag/pwd in `accept_offer` and restarts transparently, change/sdp.rs:715)
  keeps the DTLS connection, so it is the SAME carrier link and needs no
  re-admission. A NEW DTLS connection is a new link and runs the full host
  challenge + Notochord admission again — the plan's rule. The browser retains
  its C2-minted delegation across a drop (only a page refresh loses it), so
  reconnect reuses that delegation in a fresh hello; the invite stays one-use
  and the C2 TTL clamp bounds the reconnect window. No re-redemption path is
  needed.
- **2026-08-26 (C3 no-Failed-state constraint):** is-0.11.0's
  `IceConnectionState` has no `Failed` or `Closed` variant by design —
  `Disconnected` can self-heal if new candidates arrive (agent.rs:185). So the
  "credential expiry causes refresh or a clean failure, not an infinite retry"
  done-condition cannot key off an ICE state; it needs a bounded timer. Any
  reconnect trigger must be timer-bounded rather than state-watched.
- **2026-08-26 (C3 driver placement, window default = Mark's call):**
  `DriverPlacement::DedicatedThread` moves the driver off tokio's shared
  runtime onto a std::thread with an explicit 8 MiB stack, measured to survive
  str0m's full 128 KiB SCTP window (a 2 MiB tokio worker overflows it). The
  dedicated SCTP window default was set to **64 KiB** (Mark, 2026-08-26):
  below str0m's own 128 KiB ceiling, so this crate's `window_holds` guard stays
  the primary defense and the stack is margin, not the sole line. Correction to
  an earlier claim: the 65x throughput penalty is a LATENCY effect (one round
  trip per frame) and does NOT reproduce on loopback — there the raised window
  is ~2.6x SLOWER; the LAN figure is inherited, unverified here, and only the
  machine-independent mechanism (`window_holds` 410-524 -> 0) is asserted.
- **2026-08-26 (C3 application hazard, unowned):** a naive read-then-write
  echo loop wedges a session on BOTH driver placements — parked in
  `send_frame` the end stops reading, its queue fills, SCTP retransmits, the
  session dies mid-transfer. The loopback tests split reader/writer to avoid
  it; a real application (C4) must not couple the directions head-to-head.
- **2026-08-26 (str0m upstream defect, mitigated):** the refresh crash was
  str0m's own recursion — `Rtc::do_poll_output` (str0m 0.23.1, lib.rs:1719 and
  1734) self-calls once per queued SCTP packet in its `SctpEvent::Transmit`
  arm, so on teardown the stack depth equals the outstanding SCTP window in
  packets; str0m's own 128 KiB `MAX_BUFFERED_ACROSS_STREAMS` overflows a 2 MiB
  tokio worker stack. Proven by dose-response (halving the window halves the
  required stack), not by reading. Mitigated here by
  `CarrierConfig::sctp_window_bytes` (default 16 KiB): `try_flush` declines to
  hand SCTP a frame past that mark, the same contract as SCTP declining a
  write, observable via `CarrierStats::window_holds`. Two consequences worth
  keeping visible: (1) the default caps throughput at ~16 KiB per RTT, fine on
  LAN, wrong for long links — it is a config field with the measured stack
  table in its doc comment, and C3's relay work should revisit the default;
  (2) this is an upstream-filing candidate against str0m — the recursion, the
  repro shape (vanished peer with a full send buffer), and the dose-response
  table are all in `tests/native_loopback.rs` and the driver comments.
- **2026-08-26 (frame ceiling):** `MAX_FRAME_BYTES` was 4 + 65,536 = 65,540 —
  over a 64 KiB SCTP `max-message-size` peer by exactly the header. Browsers
  advertise 262,144 (confirmed in the live SDP), so C1 passed on luck. The
  payload ceiling is being corrected to `65,536 - FRAME_HEADER_BYTES` so the
  whole frame fits a default peer; the frozen shared-link vector binds the
  transcript, not frame sizes, and does not move.

## Progress

- **2026-08-25:** live seams and active plans reconciled; external donor built;
  headed browser/native ping passed; donor benchmark mismatch identified;
  Notochord/Personae external Wasm compile passed; architecture and gates
  recorded. Production code remains untouched.
- **2026-08-26: C0 landed.** New Wasm-clean `crates/murm/webrtc-carrier`
  (bounded frames, `InviteId`, role-tagged DTLS fingerprints, the link-challenge
  transcript, `shared_link` derivation); `TransportKind::WebRtc` +
  `IngressContext::webrtc` mapped honestly to `CarrierKind::Other`;
  `admit_accepted_session` extracted in Graphshell with every call site
  unchanged; and an acceptance fixture
  (`a_webrtc_session_is_admitted_with_no_authenticated_peer`) that derives its
  link through the core rather than pasting a constant, so a transcript change
  fails admission too. Verified: `mere-transport` 56 tests with the `notochord`
  feature on (the default feature set does not compile that module — a green
  run without it proves nothing), `webrtc-carrier` 29 on native and a clean
  `--all-targets` check for `wasm32-unknown-unknown`, `graphshell` 110.
  Open, deliberately: the SDP fingerprint parser accepts uppercase hex only
  (RFC 8122's grammar, and what Chrome and Firefox emit) — correct per spec,
  but it makes a nonconforming stack fail loudly rather than interoperate, and
  that posture should be revisited when C1 meets a real browser.
- **2026-08-26: Luggage integration ruled.** `InviteV1` now carries a
  `ReleaseRefV1`, C5 verifies the exact browser bundle before execution, and C6
  turns that same signed release into an explicit native adoption offer. The
  prerequisite portable release envelope and self-sufficient stage remain open
  in the Djinn resident plan; no release or trust code landed in this slice.
- **2026-08-26: C1 substantially landed.** str0m answerer (Sans-IO driver:
  single-mutation turn loop, dual-source backpressure gauge, fingerprint
  capture on `Event::Connected`) and web-sys browser initiator (event-driven
  backpressure off `bufferedamountlow`, one-shot terminal-error funnel) behind
  `native`/`browser` features, both off by default; the Wasm-clean default
  core and its frozen vector are untouched. Water marks observed 14 up /
  14 down over 400 loopback frames with str0m's own `low_water_events: 39`
  confirming the engine sees the threshold. Headed receipt: session 2 green
  end-to-end (200/200 frames, byte-exact framing, fingerprints captured,
  clean close); the refresh condition initially failed by crashing the answerer
  (stack overflow) — both that and the 0.0.0.0 `local_addr` defect were fixed
  with negative-control regression tests, and the refresh re-run passed: the
  answerer survived a peer vanishing with ~525 KB buffered, printed an honest
  receipt (`ended: "echo write failed: the data channel is closed"`, 0 dropped,
  0 malformed), and served a fresh session on the same process. Full receipt:
  `Code/testing/mere/webrtc_ping_receipt.md` (RESULT ok, 7 runs, 5 defects).
  Open items carried out of C1: the `sctp_window_bytes` default (16 KiB) costs
  ~65x on a LAN echo workload because the window is smaller than one maximum
  frame — revisit at C3; wildcard bind with a MULTI-address advertise list
  still mislabels inbound datagrams as `advertise[0]` (single-declared-address
  assumption is documented but the probe violates it) — either the probe
  advertises one address or the carrier learns per-packet destinations; and
  the str0m `do_poll_output` recursion is an upstream-filing candidate.
  Harness: `crates/probes/webrtc-ping` (standalone, `wasm-bindgen` pinned
  =0.2.126 to match the installed CLI — an unpinned range resolves 0.2.127
  and fails binding generation after a clean compile).
- **2026-08-26: C2 landed.** Core: `InviteV1` (bounded canonical encoding,
  strict hand-rolled base64url fragment codec, seed-redacting `Debug`, three
  frozen signing domains) in `webrtc-carrier` — 44 unit tests, no new
  dependencies, frozen vector untouched. Door: `webrtc_door.rs` beside the
  native-messaging precedent — invite issue/redeem/mint on the derived-key
  pattern, sans-I/O admission through the audited N1 facts adapter, pure
  client half a wasm build can reuse. Verified independently: graphshell 136
  lib tests (26 new matrix rows), the real-WebRTC loopback receipt, carrier
  58, C0 wasm receipt, and `cargo check --workspace --all-targets`, all exit
  0. Deferred to C4: the headed fragment-hygiene row.
- **2026-08-26: C3 code landed; receipts gated on live TURN.** Carrier:
  `DriverPlacement::DedicatedThread` (8 MiB stack, 64 KiB window default) with
  13 loopback tests across both placements; browser `IceServer`+credentials,
  `IceTransportPolicy::Relay`, `create_restart_offer`; the shared `lib.rs`
  re-export edited once against a settled file. Probe: `/turn-credentials`
  coturn-REST minting (HMAC verified against RFC 2202 + a coturn vector, and
  the running endpoint cross-checked against an independent implementation),
  forced-relay toggle, reconnect button, and a revisioned diff-vs-snapshot
  resume stand-in above the carrier — 17 probe tests. Verified independently:
  carrier 44+13+8+1 native, browser+default wasm clean, probe green.
  AWAITING a live TURN URL+secret from Mark for the three receipts that need
  a real relay: relay-only candidate pair, TURN-carries-encrypted-packets-but-
  no-Graphshell-key, and credential-expiry clean give-up; plus the headed
  reconnect resume run. The plan's stop line (no product wiring until a forced
  relay is demonstrated) holds until those pass.
