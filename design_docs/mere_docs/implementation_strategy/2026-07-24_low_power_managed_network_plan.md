# Low-Power Radio and Managed Network Plan

**Status (2026-07-24): planned; implementation has not started.**

This round joins two pieces that are useful independently and stronger
together:

1. a Heltec V4 direct-PHY radio which can listen continuously while its
   ESP32-S3 enters Light-sleep; and
2. owner-controlled Mere service access and Reticulum transit, enforced from
   honest incoming-session facts.

The first receipt is a useful low-power radio. The second is one authorized
Murm service crossing that radio. Transit, offers, replication efficiency,
failover, and bonding follow those proofs.

```text
V4 UART -> Light-sleep -> ingress truth -> local policy -> session proof
         -> authorized RF service -> transit classes -> offers/sync -> paths
```

This plan lives with Mere because Mere owns the product policy and service
boundary. The radio, Reticulum, and queue work lands in the Retinue workspace.

## Related work

- Retinue's existing direct-PHY Resource receipt:
  [`2026-07-23_direct_phy_resource_acceptance.md`](https://github.com/mark-ik/retinue/blob/main/design_docs/2026-07-23_direct_phy_resource_acceptance.md)
- Tulle/RNode headed receipt:
  [`2026-07-22_tulle_headed_acceptance.md`](https://github.com/mark-ik/retinue/blob/main/design_docs/2026-07-22_tulle_headed_acceptance.md)
- Retinue reliable-link RF findings:
  [`2026-07-21_first_reliable_link_over_rf.md`](https://github.com/mark-ik/retinue/blob/main/design_docs/2026-07-21_first_reliable_link_over_rf.md)
- Mere's current Reticulum adoption boundary:
  [`MURM_AS_BILATERAL.md`](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md)
- Personae's shared delegation grammar:
  `crates/persona/personae/src/delegation.rs`

## Decisions

### D1. Native USB remains the development transport

The Heltec V4 uses the ESP32-S3 USB Serial/JTAG peripheral. In Light-sleep its
clocks stop, it cannot answer the host, and the host may fail to re-enumerate it
after wake. The existing USB direct-PHY firmware therefore remains the default
development and recovery personality.

The low-power personality uses UART0 on the exposed header:

- GPIO44: UART0 RX
- GPIO43: UART0 TX
- GPIO14: SX1262 DIO1 wake

The UART adapter carries RX, TX, and ground. It does not power the board during
current measurements.

Primary hardware references:

- [ESP32-S3 USB Serial/JTAG sleep behavior](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/usb-serial-jtag-console.html)
- [Heltec WiFi LoRa 32 V4 datasheet](https://resource.heltec.cn/download/WiFi_LoRa_32_V4/datasheet/WiFi_LoRa_32_V4.2.0.pdf)

### D2. Power policy is explicit

The V4 firmware already waits asynchronously for USB input or SX1262 DIO1. Its
runtime idles with `WAITI`; the missing step is genuine Light-sleep.

Light-sleep is permitted only while:

- continuous receive is armed;
- the host-command parser is between commands;
- host output is drained;
- SPI and SX1262 setup are inactive; and
- the pending work consists only of UART RX or DIO1.

The first implementation does not sleep across an Embassy timer deadline. The
ESP32-S3 time source used by the current runtime does not advance in
Light-sleep, so timer-aware sleep needs a separate clock-compensation design.

### D3. Network policy axes remain independent

Discovery, incoming services, transit, replication, compute, path selection,
and capacity commitments are separate owner settings. There is no global
public/private or client/router mode.

A node may:

- publish a service without carrying transit;
- carry transit without exposing owner data;
- accept one community profile while refusing another;
- share storage while refusing compute; or
- use Reticulum reachability without accepting Mere protocols.

### D4. Transport facts are facts, not claims

Mere transport acceptance reports the peer only when the transport
authenticated it.

```rust
pub struct AcceptedSession<S> {
    pub stream: S,
    pub protocol: Alpn,
    pub peer: Option<PeerID>,
    pub ingress: IngressContext,
}
```

`peer: Some` means transport-authenticated. A subject named by application
bytes is not placed there.

p2panda can report `Connection::remote_id()`. Reticulum best-effort acceptance
cannot identify its initiator yet; its session proof supplies the application
identity later.

### D5. Personae remains the authority grammar

`personae::delegation` already supplies:

- signed capability certificates;
- domain, resource, path, and action scopes;
- attenuation;
- validity intervals;
- bounded delegation depth; and
- signed revocation statements.

The new local network-policy crate validates chains against configured roots
and keeps its own revocation ledger. It does not create another membership
token. Gemot remains responsible for Moot membership and constitutional
authority.

### D6. The session proof signs the transport context

The handshake signs a canonical transcript containing:

- wire version;
- network and profile;
- protocol;
- requested action and traffic class;
- nonce;
- subject;
- authenticated transport peer, when present;
- ingress interface and Reticulum link, when present; and
- hashes of the supplied delegation certificates.

For p2panda, the signed subject must equal the transport-authenticated
`PeerID`. For Reticulum, the proof binds the subject to the current link and
nonce.

### D7. Ordinary Reticulum remains ordinary Reticulum

The policy handshake is an application protocol selected by a service owner.
It does not alter ordinary Reticulum routing or require unrelated RNS peers to
understand Mere.

Anonymous Reticulum transit is enforceable by interface, packet type,
destination, hop count, and budgets. Persona policy applies only after a Mere
session proves a subject.

### D8. Bonding requires independent capacity

Logical paths over one SX1262, frequency, or collision domain are not
independent bearers. Retinue's singular path table remains intact until two
independent bearers demonstrate aggregate goodput and graceful lane loss.

## Ownership

- **Tulle** owns PHY configuration, host-radio transport, power state, airtime,
  channel/collision information, turnaround, and radio observations.
- **Retinue** owns Reticulum links, interface ingress, anonymous transit policy,
  packet classes, queue budgets, forwarding, and path use.
- **mere-transport** reports authenticated peer and ingress facts without
  deciding policy.
- **network-policy** evaluates local service policy and runs the bounded
  session handshake.
- **Personae** supplies delegation and revocation proof primitives.
- **Gemot** owns Moot membership and constitutional authority.
- **Murm and other service domains** remain responsible for their application
  authorization beyond session admission.
- **Replication, storage, and compute domains** retain their existing
  authorizers.
- **The host** persists owner settings and presents them for editing.

`network-policy` is a session-policy component, not a master policy engine.

## Current evidence

### Radio

- `firmware/heltec-v4-phy/src/main.rs` selects between host input and
  `lora.rx()` asynchronously.
- DIO1 is already connected to GPIO14 and awaited by `lora-phy`.
- `esp-rtos 0.3.0` uses `wait_for_interrupt()` in its default idle hook and
  exposes `start_with_idle_hook`.
- `esp-hal 1.1.1` exposes Light-sleep, GPIO wake, and UART wake on ESP32-S3.
- The V4 direct-PHY firmware has already exchanged stock-compatible Sennet
  frames.
- Retinue Resource has passed byte-exact 4 KiB publish and fetch over direct
  PHY in both directions.

Bench snapshot on 2026-07-24:

- COM6: ESP32-S3, USB VID/PID `303a:1001`
- COM7: ESP32-S3, USB VID/PID `303a:1001`
- COM10: nRF52840/T114, USB VID/PID `1915:521f`

COM numbers are observations, not identities. Every headed run re-queries the
ports after flashing.

### Ingress and policy

- `retinue::endpoint::InterfaceId` is a `u32` already carried through the
  internal router.
- `retinue::endpoint::Accepted` currently exposes only `stream` and
  `destination`.
- `mere_transport::Transport::accept()` currently returns a naked stream.
- The p2panda queue handler accepts a `Connection` but currently queues only
  its bi-stream.
- Reticulum's Mere accept router currently discards `Accepted.destination` and
  any future interface context when it queues `LinkStream`.
- Retinue routing is currently one `AtomicBool`, toggled by
  `Endpoint::enable_routing()`.
- Personae has certificate and revocation primitives, but no
  application-specific chain evaluator or local revocation ledger.
- `DropExportSelector` and `DropExportBudget` already provide a real
  privacy-aware and byte-bounded replication seam.

## Build order

### V0. Reproducible power baseline

**Repositories:** Retinue only
**Code change:** none

Use COM7 as the V4 target, COM6 as the unchanged V4 control, and COM10 as the
T114 RF peer.

Measure the V4 through one consistent supply path:

1. current firmware in continuous receive;
2. SX1262 standby;
3. UART firmware with Light-sleep disabled;
4. UART firmware with Light-sleep enabled and SX1262 receiving continuously;
5. a representative packet workload.

For each run record:

- supply path and voltage;
- board and firmware revision;
- firmware commit;
- radio profile;
- steady and peak current;
- observation interval; and
- energy over the interval.

Keep raw captures under an ignored receipt directory. Commit only the summarized
receipt.

**Done when:** the existing receive-current claim has a repeatable baseline on
the actual board and supply path.

### V1. UART direct-PHY without sleep

**Repository:** Retinue
**Files:**

- `firmware/heltec-v4-phy/Cargo.toml`
- `firmware/heltec-v4-phy/src/main.rs`
- `crates/tulle/src/direct_phy_serial.rs`

Add mutually exclusive firmware features:

```toml
default = ["host-usb"]
host-usb = []
host-uart-low-power = []
```

Generalize the firmware's host input/output helpers over
`embedded_io_async::{Read, Write}`. The USB build retains its current behavior.
The UART build uses UART0 and the same direct-PHY byte protocol.

UART wake consumes the triggering character and may lose adjacent characters.
Add a configurable host wake sequence:

```rust
pub struct WakeSequence {
    pub preamble: Vec<u8>,
    pub settle: Duration,
}
```

The firmware ignores the selected wake byte only when the command parser is at
a frame boundary. The host sends the preamble, waits `settle`, then sends the
complete command. Sleep remains disabled in V1 so transport correctness is
isolated from power state.

**Tests:**

- direct-PHY duplex tests cover wake framing before status, configure, and TX;
- fragmented host commands still assemble correctly;
- the USB configuration has no wake prefix;
- invalid bytes cannot desynchronize the next valid command; and
- the existing direct-PHY tests remain green.

**Headed proof:** run the existing 4 KiB Resource publish/fetch acceptance
between the UART-controlled V4 and COM10.

**Done when:** changing the host transport does not change radio behavior or the
direct-PHY protocol.

### V2. Guarded Light-sleep

**Repository:** Retinue
**Files:**

- `firmware/heltec-v4-phy/src/main.rs`
- new `firmware/heltec-v4-phy/src/power.rs`
- direct-PHY status/diagnostic framing only if needed for the receipt

Use `esp_rtos::start_with_idle_hook` for the UART personality. The custom idle
hook enters `Rtc::sleep_light` only when a small atomic sleep gate permits it.
Wake sources are DIO1 high level and UART0 activity.

The command loop closes the gate before:

- parsing a partial command;
- configuring the SX1262;
- SPI activity;
- transmitting;
- writing a host event; or
- awaiting a timer.

Expose counters for sleep entries and wake causes. These are diagnostics, not
the long-term Tulle observation API.

**Headed proof:**

1. 100 consecutive UART wake/configure cycles;
2. 1,000 received RF frames with no loss attributable to sleeping;
3. 100 transmissions whose first complete UART command is retained;
4. the existing bidirectional 4 KiB Resource proof;
5. quiet continuous-RX current and representative-workload energy; and
6. the default USB proof on COM6 as a regression control.

**Done when:**

- quiet continuous RX falls by at least 5x on the same supply path;
- whole-board continuous RX is at or below 12 mA, or the receipt identifies the
  remaining consumer;
- RSSI/SNR remain comparable to the awake control;
- no wake cycle corrupts a host command; and
- the default USB firmware remains unchanged.

This is the first useful milestone.

### V3. Preserve Retinue ingress

**Repository:** Retinue
**Files:**

- `crates/retinue/src/endpoint.rs`
- focused endpoint tests

Add `interface: InterfaceId` to `Accepted`. Preserve equivalent ingress
context when reliable and Resource sessions are accepted rather than allowing
those paths to diverge.

The internal router already has `iface` where it constructs each accepted
session. The change is propagation, not inference.

Retain `LinkStream::link_id()` as the link identifier rather than duplicating
it in every accepted structure.

**Tests:**

- a link arriving on interface A reports A;
- the same destination reached through B reports B;
- concurrent accepts do not exchange ingress;
- a malformed or forged packet does not create an accepted session; and
- existing single-interface callers remain source-compatible where practical.

**Done when:** every Retinue accepted-session form preserves the interface and
link on which it arrived.

### V4. Preserve Mere acceptance context

**Repository:** Mere
**Files:**

- `crates/murm/transport/src/transport.rs`
- `crates/murm/transport/src/memory.rs`
- `crates/murm/transport/src/p2panda_transport.rs`
- `crates/murm/transport/src/reticulum_transport.rs`
- transport stubs and tests

Introduce version-independent local context:

```rust
pub struct AcceptedSession<S> {
    pub stream: S,
    pub protocol: Alpn,
    pub peer: Option<PeerID>,
    pub ingress: IngressContext,
}

pub struct IngressContext {
    pub transport: TransportKind,
    pub interface: Option<IngressInterfaceId>,
    pub link: Option<[u8; 16]>,
}

pub struct IngressInterfaceId(u64);
```

`IngressInterfaceId` is an opaque local identifier. The Retinue adapter converts
its `u32` without leaking Retinue types into Memory or p2panda builds.

Backend changes:

- Memory queues its paired peer with the stream.
- p2panda captures `Connection::remote_id()` before queuing.
- Reticulum's accept router queues destination, interface, and link context.
- Reticulum best-effort returns `peer: None`.

Change every `Transport` implementation and test stub in the same commit so
callers cannot silently ignore the new boundary.

**Done when:** all backends preserve protocol and ingress, p2panda reports the
correct authenticated peer, and a wrong-peer assertion fails.

### V5. Minimal local evaluator

**Repository:** Mere
**New crate:** `crates/system/network-policy`

Add the crate to the Mere workspace and workspace dependency table.

Initial versioned, serializable types:

```rust
NetworkId
ProfileRef
LocalNetworkPolicy
SessionRequest
SessionDecision
DenyReason
TrafficClass
HandshakeLimits
```

The first supported action is deliberately narrow:

```text
domain: mere.network
path: /services/murm
action: connect
```

`LocalNetworkPolicy` independently configures discovery, services, and transit.
It remains private. `ProfileRef` identifies a shared vocabulary/profile without
making that profile locally authoritative.

The Personae adapter:

1. verifies every certificate signature;
2. resolves each `DelegationParent`;
3. proves strict attenuation;
4. enforces configured chain depth;
5. checks not-before and expiry against a caller-supplied clock;
6. checks the local revocation ledger; and
7. terminates at a locally accepted root.

Evaluation order:

1. wire version supported;
2. profile accepted locally;
3. required transport identity present;
4. Personae chain valid;
5. requested action covered;
6. local service rule permits it; and
7. capacity currently available.

**Tests:**

- public service with transit disabled;
- transit enabled with the service private;
- member-only service;
- missing identity where one is required;
- expired certificate;
- revoked parent cascading to a child;
- widened child scope;
- excessive delegation depth;
- incompatible profile; and
- capacity refusal after otherwise valid authority.

**Done when:** the policy matrix is deterministic and models service access and
transit independently.

### V6. One authorized service over Memory and p2panda

**Repository:** Mere
**Files:**

- `crates/system/network-policy/src/handshake.rs`
- focused integration tests in `network-policy`
- one Murm acceptance adapter

Before application bytes, exchange a bounded postcard frame:

```rust
SessionHello {
    version,
    network,
    profile,
    requested_action,
    requested_class,
    nonce,
    subject,
    session_signer,
    transcript_signature,
    delegations,
}

SessionReply::Accept {
    session_id,
    class,
    limits,
    profile_revision,
}

SessionReply::Reject {
    reason,
}
```

`session_signer` contains the Personae-derived signing-key attestation used for
this session. The transcript signature binds D6's fields and the certificate
hashes.

`HandshakeLimits` supplies owner-configurable limits within compile-time safety
ceilings:

- encoded hello and reply bytes;
- certificate count;
- certificate bytes;
- delegation depth; and
- handshake deadline.

Expose a wrapper shaped like:

```rust
policy
    .accept(accepted_session, network, action)
    .await
    -> Result<AuthorizedSession<_>, SessionRejection>
```

Run the same matrix over Memory and p2panda. For p2panda, prove that a valid
Personae signature from the wrong transport peer is rejected.

**Done when:** an owner rule admits or rejects one real Murm connection before
Murm receives application bytes.

### V7. The same service over Reticulum and direct PHY

**Repositories:** Mere and Retinue

First run the V6 matrix over Reticulum/TCP. Then connect the Reticulum transport
to Tulle direct PHY and run the accepted and rejected cases over the V2
UART/Light-sleep firmware.

The Reticulum hello supplies the subject unavailable from best-effort transport
acceptance. Its transcript binds:

- the Reticulum link id;
- ingress interface;
- ALPN-derived destination/protocol; and
- fresh nonce.

Proof cases:

1. accepted service session and application round trip;
2. locally denied service;
3. expired delegation;
4. revoked delegation;
5. valid signature replayed on a different link; and
6. valid certificate presented by the wrong session signer.

Record the board ports by USB identity before and after flashing. The committed
receipt names board roles and firmware commits, not assumed COM numbers.

**Done when:** the same authorization semantics pass over Memory, p2panda,
Reticulum/TCP, and one real sleeping direct-PHY RF link.

This is the second useful milestone.

### V8. Retinue transit policy and traffic classes

**Repository:** Retinue
**Files:**

- `crates/retinue/src/endpoint.rs`
- interface configuration and scheduler modules split out when the boundary is
  clear

Replace the internal routing boolean with:

```rust
pub struct RoutingPolicy {
    pub forward_announces: bool,
    pub forward_packets: bool,
    pub allowed_ingress: InterfaceSelector,
    pub allowed_egress: InterfaceSelector,
    pub max_hops: u8,
    pub packet_budget: Budget,
    pub byte_budget: Budget,
    pub queue_weights: QueueWeights,
}
```

Keep `enable_routing()` as shorthand for the present behavior.

Add:

```rust
attach_interface_with_config(InterfaceConfig)
attach_stream_with_config(TcpStream, InterfaceConfig)
```

The outbound path needs an explicit envelope because packet bytes cannot tell
Retinue whether locally originated data is interactive or background:

```rust
OutboundEnvelope {
    packet,
    origin: Local | Transit,
    class: Control | Interactive | Background | Transit,
}
```

Use bounded queues and weighted or deficit round robin. Control traffic remains
bounded and cannot be generated without rate limits.

Export counters for:

- accepted;
- locally originated;
- forwarded;
- policy rejected;
- rate limited;
- queue dropped; and
- bytes and packets by class/interface.

**Done when:** a sustained public-transit test cannot consume the configured
local interactive reservation, and disabling transit leaves local service
traffic unaffected.

### V9. Signed offers

**Repository:** Mere, with a small Retinue discovery adapter

Add a signed, expiring `NodeOffer` only after V5-V8 stabilize the vocabulary:

```rust
NodeOffer {
    accepted_profiles,
    supported_protocols,
    offered_roles,
    capacity_bands,
    transfer_paths,
    expires_at,
}
```

An offer describes current willingness. It does not expose
`LocalNetworkPolicy`.

For Reticulum, append an independently signed offer digest after the existing
peer binding and fetch the full offer through a bounded control protocol. Older
parsers already tolerate trailing app data.

For p2panda, distribute the same object over its address-book/control
connection. Do not create another identity system.

**Done when:** a node can discover an offer, verify it, observe expiration, and
still receive a local denial because the private policy changed.

### V10. Selective replication

**Repository:** Mere

Build on `DropExportSelector` and `DropExportBudget`:

- exchange retained frontiers and content inventories;
- request one missing operation or blob from one peer;
- prefer an available nearby cache;
- skip known inventory on later sessions;
- apply privacy selection before scheduling;
- resume blob chunks by content hash; and
- permit store-and-forward with expiration.

**Done when:** repeating an unchanged sync transfers only the compact inventory
exchange, and partial sync obeys both semantic and byte budgets.

### V11. Tulle feedback, failover, and bonding

**Repository:** Retinue

After V2 establishes the power behavior, add a transport-neutral Tulle
observation surface:

```rust
InterfaceObservation {
    channel,
    collision_group,
    queue_depth,
    estimated_airtime,
    actual_airtime,
    turnaround,
    power_state,
    wake_reason,
    last_failure,
}
```

Retinue consumes observations for path selection and diagnostics without
owning PHY policy.

Implement in this order:

1. `single`, preserving ordinary Reticulum behavior;
2. `failover`, with a complete transfer surviving lane loss; and
3. `bonded`, only for two independently measured bearers.

Bonded acceptance requires:

- byte-exact reassembly under loss and reordering;
- duplicate suppression;
- graceful loss of either lane;
- per-lane airtime budgets obeyed; and
- aggregate goodput approaching the sum of isolated-lane measurements.

**Done when:** the evidence distinguishes useful independent capacity from two
logical paths sharing one bottleneck.

## Verification wall

### Retinue workspace

Run package-specific host checks so cross-target firmware members do not enter
the host build accidentally:

```text
cargo test -p tulle --all-features
cargo test -p retinue --all-features
cargo test -p retinue --lib --no-default-features
cargo test -p sennet --all-features
cargo test -p tucket --all-features
cargo fmt --all -- --check
git diff --check
```

Build the V4 personalities separately with the Espressif toolchain:

```text
cargo +esp build -p tulle-heltec-v4-phy --release \
  --target xtensa-esp32s3-none-elf -Zbuild-std=core

cargo +esp build -p tulle-heltec-v4-phy --release \
  --no-default-features --features host-uart-low-power \
  --target xtensa-esp32s3-none-elf -Zbuild-std=core
```

### Mere workspace

At each Mere slice:

```text
cargo test -p mere-transport
cargo test -p mere-transport --features reticulum
cargo test -p network-policy
cargo test -p personae
cargo test -p murm
cargo fmt --all -- --check
git diff --check
```

The exact new crate package name is fixed when V5 lands and then replaces
`network-policy` in this wall if Cargo naming requires a prefix.

### Evidence ladder

Each claim names its level:

1. API wired
2. deterministic unit test
3. in-memory integration
4. real TCP/p2panda transport
5. real OS serial
6. real RF
7. measured power/current
8. headed owner-policy scenario

Later evidence does not retroactively widen an earlier receipt. A one-hop bench
proof is not a range, recovery, roaming, or multi-hop-discovery proof.

## Non-goals for this round

- Light-sleep while native USB Serial/JTAG remains connected
- Wi-Fi power management
- A universal policy engine for storage, compute, or domain operations
- A new membership or identity token beside Personae and Gemot
- Changing ordinary Reticulum wire behavior
- Inferring an authenticated peer from an unsigned Reticulum claim
- Simultaneous protocol personalities on one SX1262
- Bonding logical lanes over one radio or collision domain
- Treating advertised willingness as an authorization decision
- Hiding measurement conditions behind a single current number

## Progress

### 2026-07-24 — V3 and V4 landed (ingress preserved end to end)

Started on the managed-network half, because V0/V1/V2 need a bench session
(see the blocker below).

**V3 (Retinue, commit `f02f572`).** `Accepted` and `AcceptedResource` gained
`interface`; `ResourceSession` gained an `interface()` accessor (it already
carried the field). The reliable path was the one at risk of diverging, since
it surfaces a bare `LinkStream` with no wrapper to hang ingress on, so `iface`
went onto `LinkStream` itself beside `link_id` — which also gives outbound
streams the interface they were opened over. As the plan predicted this was
propagation, not inference: the router already had `iface` in scope at every
construction site. Four tests: two leaves report different interfaces for one
destination, two links from one leaf share one, a reliable accept reports the
same ingress as a best-effort accept over the same bearer, an outbound stream
reports its own. The forged-packet requirement was already covered by
`a_forged_proof_does_not_strand_link_setup`, and remains so because
`interface` comes from the router's own record, never from packet bytes.

**V4 (Mere, commit `166b68ba`).** `Transport::accept` now returns
`AcceptedSession<S>` (protocol, `Option<PeerID>`, `IngressContext`), with
`IngressContext`/`IngressInterfaceId`/`TransportKind` in a new
`mere-transport::accepted` module. Backends: p2panda captures
`Connection::remote_id()` in the protocol handler while the connection is
still in hand; Memory reports its constructed counterparty; Reticulum reports
`peer: None` honestly and carries interface + link, its accept router no
longer discarding them. All four implementations (including murm's stub) and
every caller changed in one commit. Tests assert the honest cases, including
a wrong-peer assertion and a real TCP-loopback reticulum round trip.

**Two blockers, both needing Mark:**

1. **V0 and the headed proofs in V1/V2 need a bench session.** V0 is current
   measurement across five scenarios on one supply path; V2's done conditions
   are current and RF numbers. These cannot be driven remotely: they need a
   meter, the boards on a known supply, and someone to power-cycle. Everything
   else in the radio half (the V1 firmware feature split, the UART host I/O
   generalization, the wake sequence) is ordinary code that can land ahead of
   the bench, with its headed proof deferred to that session.
2. **V4's reticulum arm needs retinue V3 pushed.** Mere tracks retinue's
   GitHub main; V3 is committed locally but unpushed, so
   `cargo test -p mere-transport --features reticulum` will not build from a
   clean checkout until it lands. Verified here against the sibling checkout
   through the gitignored `.cargo/config.toml` patch (note: the patch was
   silently bypassed until `cargo update -p retinue` forced a re-resolve —
   worth remembering, it produced no "patch not used" warning). The default
   mere build is unaffected, since reticulum is optional and default-off.

Next code slice is **V5** (the `network-policy` crate and its evaluator),
which is pure Rust with no hardware dependency.

## Completion

The round is complete when all of the following are true:

1. the V4 has a measured low-power continuous-RX personality with preserved
   direct-PHY behavior;
2. every incoming Mere transport session carries honest protocol, peer, and
   ingress context;
3. one owner-configured Murm service is admitted or rejected through the same
   evaluator over Memory, p2panda, Reticulum/TCP, and direct PHY;
4. Retinue transit cannot starve locally reserved traffic;
5. offers reveal current capabilities without revealing private policy;
6. repeated unchanged replication sends inventory rather than payload again;
   and
7. failover and bonding claims are limited to the independent-bearer evidence
   actually obtained.
