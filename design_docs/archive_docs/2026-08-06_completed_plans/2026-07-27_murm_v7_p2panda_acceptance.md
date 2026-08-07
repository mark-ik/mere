# Murm V7 p2panda Acceptance

**Status: passed on 2026-07-27.**

This receipt closes the p2panda carrier arm of V7 in the
[low-power managed-network plan](../../mere_docs/implementation_strategy/2026-07-24_low_power_managed_network_plan.md).
It exercises Murm's real session listener over p2panda-net's authenticated Iroh
endpoint rather than supplying carrier facts from a fixture.

## Construction

`murm::serve_accepted_session` now consumes
`mere_transport::AcceptedSession` directly. It converts the protocol,
transport-authenticated peer, and ingress context through
`AcceptedSession::into_session`, then runs Notochord admission before reading a
Murm post frame.

The listener therefore has one construction site for carrier facts. A subject
or peer decoded from the session hello cannot replace what the transport
observed.

## Receipt

The focused command was:

```powershell
cargo test -p murm --features session-lane --test session_lane `
  --target-dir C:\t\mere-reticulum-radio-target --offline -- --nocapture
```

Result:

```text
running 6 tests
test p2panda::authenticated_member_reaches_murm_over_real_p2panda ... ok
test p2panda::wrong_authenticated_peer_is_refused_before_murm ... ok
test result: ok. 6 passed; 0 failed
```

The accepted case proves:

- the p2panda endpoint authenticates the same Ed25519 subject that signs the
  Notochord hello;
- the accepted session reports `CarrierKind::P2panda` and retains that peer;
- one signed Murm post crosses only after admission; and
- the conversation holds exactly that post.

The refusal case uses a different p2panda endpoint while presenting an
otherwise valid member-signed hello and member grant. Both ends report
`SubjectNotTransportPeer`, and the conversation remains empty.

Wider verification:

- `cargo test -p murm --features session-lane`: 57 unit tests and 6
  session-lane tests passed;
- `cargo test -p mere-transport --features notochord`: 40 unit tests and the
  Memory/Notochord integration test passed; and
- the standalone direct-PHY probe still compiles against the same
  `serve_accepted_session` listener.

Warning-denying Clippy did not reach a clean repository result. Dependency
linting stops on an existing `stickleback/src/synced_space.rs` documentation
continuation, and `--no-deps` stops on an existing needless question mark in
`murm/src/cabal.rs`. Neither file belongs to this slice.

## Evidence boundary

This is a real p2panda-net/Iroh QUIC connection between two endpoints on one
Windows machine, explicitly bootstrapped with their endpoint addresses. It
does not exercise mDNS, relays, NAT traversal, or a second physical host.

It is not RF or power evidence. The headed direct-PHY receipt covers the radio
carrier. V0/V2 still own Light-sleep, DIO1 wake, current, and energy
measurements.
