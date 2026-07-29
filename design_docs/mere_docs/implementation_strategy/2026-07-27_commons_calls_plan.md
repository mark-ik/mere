# Commons Calls Plan

**Date:** 2026-07-27  
**Status:** A0 complete and promoted at `crates/moot/commons` 2026-07-28;
A1-A6 not started. Turnstone is
the fixed first consumer for A1, gated on its render-free shared-place port.
**Depends on:** the
[Commons profile](../design/2026-07-27_commons_profile_v1.md), the
[Notochord session spine](./2026-07-26_notochord_session_policy_spine_plan.md),
the real `mere-transport` carriers, and Turnstone's
[peer-web reframe](../../../../turnstone/design_docs/2026-07-28_turnstone_peer_web_reframe.md).

## Product contract

A call is an admitted live session inside one Commons. The Commons supplies
membership, identity, invitation context, and settings. Notochord admits each
live service session. A call service owns signaling, media, presence, and
carrier policy after admission.

Turnstone is the first product consumer. It owns call actions, device choices,
visible consent, interruption settings, and the live call surface inside a
shared place. The retained call grammar remains reusable Mere-side software;
Graphshell may project call state but is not the product host.

The first product is two-person audio:

- invite, ring, accept, decline, leave, and end;
- mute and push-to-talk;
- current participant and connection state;
- user-configurable interruption policy;
- automatic carrier choice with an honest degraded state.

Group calls, video, screen sharing, telephony gateways, and recording are later
products. The first slice must not create abstractions for them.

## Boundaries

| Concern | Owner | Rule |
|---|---|---|
| Commons membership and retained invitations | Commons profile and Stickleback | Signed immutable facts, encrypted under the Commons data profile. |
| Live-session admission | Notochord | `SessionHello` proves the caller and action before call bytes are read. |
| Carrier truth | `mere-transport` and Retinue | `AcceptedSession` supplies only observed peer, protocol, interface, and link facts. |
| Call state and media control | Call service | Ephemeral state with sequence numbers, expiry, and explicit terminal states. |
| Audio capture and playback | Turnstone first; product host contract | Device permission, selection, gain, and local mute remain user controls. |
| Retained chat and clips | Commons chat | A recording or voice note is a message attachment, not live media history. |

Presence is soft state, not authority. “Online”, “ringing”, and carrier quality
expire unless refreshed. They never grant admission and they are not retained
as durable member facts.

Call media uses fresh per-call keys. Commons data epochs authorize and address
the invitation, but they do not become a shared media key handed to every
member. Joining, leaving, revocation, and reconnect each have explicit key
transitions.

## State model

The user-visible state machine is:

```text
idle
  -> inviting -> ringing -> connected -> ended
                    |          |
                    v          v
                 declined   reconnecting
                                |
                                v
                             connected
```

`declined`, `missed`, `cancelled`, `failed`, and `ended` are distinct terminal
reasons. A peer can only end its own participation; the call owner may end the
whole call if the Commons capability grants that action.

Every control frame carries:

- one random call id;
- the Commons space id;
- a monotonically increasing sender sequence;
- the admitted participant identity;
- an expiry or terminal marker;
- the media offer or selected media parameters when relevant.

Duplicate and stale control frames are ignored. A terminal frame dominates
later non-terminal frames from the same participant. Wall-clock order is never
used to settle concurrent state.

## Durable and live signaling

An invitation is a retained, encrypted Commons fact so an offline member can
receive it later. Accepting an old invitation does not resurrect a call: the
fact carries an expiry, and the live service must still admit both peers.

Once a session is admitted, ringing, acceptance, mute state, media negotiation,
quality reports, and leave are live control frames. A sparse terminal receipt
may be retained for missed-call history when the user enables call history. It
contains participants, start/end times, and terminal reason, never media keys
or packet telemetry.

Initial profile names:

- profile `commons.call.v1`;
- ALPN `mere/commons-call/v1`;
- service path `/services/commons-call`;
- Notochord domain `mere.commons.call` and admission action `connect`;
- call-grammar capabilities `invite`, `join`, and `end`, evaluated after
  admission;
- traffic class `Interactive`.

These names stay in the call consumer. Notochord remains vocabulary-neutral.

## Carrier policy

The carrier policy is user-configurable per device and Commons. Its inputs are
carrier facts plus measured loss, latency, jitter, and available bitrate. It
does not infer quality from transport names alone.

The first policy is:

1. use an admitted IP bearer for live audio;
2. reconnect on another admitted IP path when the active path fails;
3. use Reticulum or direct PHY for invitation, presence, terminal state, and
   recorded voice notes when their byte budget permits;
4. expose “messages only” or “voice notes only” when no live-audio bearer is
   available.

The direct-PHY receipt proves canonical Commons operations cross the radio. It
does not prove live audio. At the current LoRa profile, latency and throughput
rule out an ordinary duplex call. Any later low-bitrate radio-voice experiment
is a separate push-to-talk profile with its own intelligibility and airtime
receipts.

Carrier changes never change call identity or participant authority. Each new
stream runs Notochord admission and proves possession of the current call
resume secret before it can replace the old bearer.

## Interruption policy

The host evaluates interruption locally before it rings:

- allowed callers or Commons roles;
- quiet hours and focus mode;
- ring, silent notification, or automatic decline;
- whether a second call may interrupt an active call;
- whether expensive or metered carriers may be used;
- selected microphone, speaker, and push-to-talk preference.

Defaults belong in settings, not the protocol. Remote priority is a request
that local policy may ignore. Accepting a call always requires a local gesture
unless the user explicitly configures an auto-answer rule for that caller and
device.

## Security and privacy stops

- The live protocol carries no self-asserted principal. It reads the principal
  from `AdmittedSession`.
- A valid Commons grant for chat, projection, or another service cannot join a
  call.
- Revocation and delegation expiry are rechecked during long calls. Failure
  ends that participant cleanly.
- Media keys are forward-secret and never written into a Commons operation,
  checkpoint, crash report, or quality log.
- Microphone capture starts only after local permission and visible acceptance.
- Recording is a separate, visible consent state. The first product does not
  implement it.
- Quality reports are bounded, coarse, and short-lived so they do not become a
  location or behavior history.

## Build order and receipts

### A0. Grammar and state fold

**Seams:**

- `crates/moot/commons/src/call.rs`
- `crates/moot/commons/src/lib.rs`

Define retained invite/terminal facts and the sans-I/O live control grammar.
Property-test duplicate, reordered, expired, and concurrent control frames.

**Done:** two peers fold every permutation to the same visible state; stale
frames cannot reopen an ended call.

**Receipt:** `commons-spine::call` now owns versioned retained invitation and
terminal facts plus an expiring, sans-I/O control grammar for ring, accept,
decline, cancel, leave, end, failure, reconnect/resume, mute, push-to-talk,
and audio offer/selection. Per-sender sequence settles stale settings. Exact
duplicates are harmless, equivocation at one sender sequence fails closed,
and terminal controls remain dominant over later non-terminal frames.
Concurrent terminal reasons use an explicit semantic rank plus participant
identity, never wall-clock order.

Ten A0 tests cover all 24 permutations of the receipt conversation, a second
peer's reversed and duplicate arrival, property-generated duplicates,
reordering, expiry, and concurrent terminals, retained-terminal dominance,
equivocation, and CBOR round trips. The complete 36-test Commons spine suite
and all-target Clippy with warnings denied pass. A0 selects no audio device or
codec dependency; that remains A2's measured probe.

### A1. Admitted control session

**Seams:**

- `crates/murm/transport/src/accepted.rs`
- `crates/system/notochord/src/io.rs`
- `turnstone/src/call.rs`, beside Turnstone's place port

A1 starts only after Turnstone can open a shared place and supply its admitted
session context. It must not found a second generic call host merely to keep
the proof inside Mere.

Accept `mere/commons-call/v1`, convert `AcceptedSession` through the existing
audited adapter, then call `notochord::admit_session` before decoding call
control.

**Done:** Turnstone's render-free call port completes invite, accept, leave,
and end over Memory and p2panda carriers; foreign-service grants are refused;
denial emits no call frame.

### A2. Loopback audio

Choose the audio-device and codec dependencies only after a small duplex probe
reports capture format, playback format, end-to-end latency, jitter behavior,
and clean device loss. Keep codec frames outside Commons operations.

**Done:** two local processes exchange intelligible mono audio; mute stops
capture; device removal ends or recovers without a stuck microphone; media
bytes are encrypted with fresh call keys.

### A3. Real IP call

Run the A2 media protocol over the admitted p2panda/Iroh bearer. Add bounded
jitter buffering, packet loss accounting, reconnect, and resume-secret proof.

**Done:** two machines complete a call, survive one forced connection loss,
and reject a replayed resume attempt. The final quality receipt reports
measured loss, latency, jitter, reconnect count, and selected codec.

### A4. Presence and interruption

Add expiring presence, local ring policy, focus/quiet-hours integration, device
selection, and visible degraded states.

**Done:** an offline or silenced peer is represented truthfully; presence
expiry cannot change membership; every automatic decline names the local
policy reason without disclosing private settings to the caller.

### A5. Radio complement

Carry retained invites, terminal facts, and bounded voice-note attachments
over Reticulum and direct PHY. Keep live audio disabled on carriers that fail
the configured bitrate and latency floor.

**Done:** the connected T114 and Heltec V4 exchange an invite and voice note
with byte-identical Commons verification; the UI says “voice note” rather than
claiming a live call.

### A6. Group-call decision

Only after A0-A5, compare mesh forwarding, a selected forwarder, and an
external relay using measured two-person media costs and the actual membership
model.

**Done:** a written choice names participant limit, trust model, failure mode,
bandwidth cost, and the proof required before group calls enter the profile.

## Stop rules

- Do not add call types to Stickleback, Chartulary, Notochord, or
  `mere-transport`; they carry domain-neutral facts and bytes.
- Do not call retained invitation sync “presence”.
- Do not claim radio voice from a successful message transfer.
- Do not begin group media before a two-machine A3 receipt exists.
- Do not retain media or fine-grained quality history as a side effect of
  diagnostics.
- Do not start A1 before Turnstone's shared-place port can supply the product
  context and admitted session.
- Do not make Graphshell or a Mere probe the first call product.
