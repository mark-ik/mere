# Frontier sync over Reticulum carriage

**Date**: 2026-09-01
**Status**: Research brief with rulings. Five decisions taken with Mark on
2026-09-01 (see Rulings). No code proposed beyond the spike in §7; no plan
exists yet. Findings come from three read-only surveys run the same day
(p2panda fork internals, the mere/retinue carrier seam, and delay-tolerant
sync prior art); every code claim below was verified against the tree on
that date.
**Question**: what would p2panda sync over a Reticulum link actually look
like, given a ~500-byte MTU, hundreds of bits to a few kilobits per second,
long variable latency, links that drop, and a store-and-forward layer where
two peers may never be online together.
**Related**:

- [`2026-06-21_offgrid_lora_transports_brief.md`](2026-06-21_offgrid_lora_transports_brief.md)
  named the ceiling this brief works under: "a lean pairwise RBSR tuned for
  tiny frames, which is a Reticulum-native sync projection nobody has built."
  The finding here is that RBSR is the wrong tool and the frontier vector
  p2panda already carries is the right one.
- [`../implementation_strategy/2026-07-24_low_power_managed_network_plan.md`](../implementation_strategy/2026-07-24_low_power_managed_network_plan.md)
  V10 (selective replication) is the unstarted item this brief scopes.
- [`../implementation_strategy/2026-07-12_deletion_retention_and_native_drop_plan.md`](../implementation_strategy/2026-07-12_deletion_retention_and_native_drop_plan.md)
  decision 6, "the drop is a carrier, not a second sync engine," and its seam
  table row "no delay-tolerant application bundle," both bind here.
- `retinue/design_docs/2026-08-25_compact_signed_feed_and_local_control_plan.md`
  owns the MC family (compact carriage of p2panda operations) and the MC0
  control case the spike in §7 reuses. Its warning against a second
  replication authority beneath stickleback is honoured by ruling 3.
- Code: [`crates/stickleback/src/joined_space.rs`](../../../crates/stickleback/src/joined_space.rs)
  (the iroh-bound sync driver), [`crates/murm/transport/src/reticulum_transport.rs`](../../../crates/murm/transport/src/reticulum_transport.rs)
  (the carrier), `crates/p2panda` under `Code/crates` (the fork surveyed).

---

## 1. Summary

Everything below and above the sync protocol is already carrier-neutral and
in the tree. The sync protocol is the one missing piece, and it is small:
p2panda's own LogSync message vocabulary driven statelessly, one request and
one response per exchange, over retinue's existing request lane, with a
compact log id and one added field for pruning. That is "the same wire
vocabulary with a different driver," not a new protocol. Three session shapes
(live link, eager one-way push, courier drop) share it.

## 2. What exists, verified 2026-09-01

| Layer | State | Evidence |
|---|---|---|
| Carrier | `ReticulumTransport` maps `connect`/`accept` onto retinue links as `AsyncRead + AsyncWrite`; announces stay under the 255-byte LoRa frame (regression test); `link_mtu`, reliable window and RTT are tunable. Feature-gated, default off, no consumer in `ports/`. No reconnect, no outbox. | `reticulum_transport.rs`, `tests.rs`; direct-PHY probe sets MTU 255 / window 1 / RTT 2 s |
| Bulk transfer | Retinue Resources: windowed, bz2 when it shrinks, 464-byte parts, per-part map hashes, 64-byte completion proof. `Endpoint::send_payload_with_config` already chooses one link packet or a Resource by size. A request/response lane (`request`, `respond_auto`) exists and mere does not use it. | `retinue/crates/retinue/src/{resource,endpoint}.rs` |
| Operation format | p2panda-core header is definite-length CBOR, ~104 bytes (genesis, no body) to ~230 bytes (facade extensions). No size cap anywhere; body and header ship in one frame; blobs crate is a stub in 0.7.1. | `p2panda-core/src/operation/header.rs`, `validation.rs` |
| Ingest and ordering | p2panda-stream has no networking dependency; its causal orderer keeps the pending queue in the store, so late and out-of-order arrival is the designed case. Stickleback's processor adds retained-frontier continuity and prune-aware backlink checks. | `p2panda-stream/src/lib.rs`, `crates/stickleback/src/processor.rs` |
| Bundle | Native drop: manifest, self-delimiting records, staged resume by `DropId`, `DropReceipt` as the one anti-retransmission primitive. Limits are desktop-shaped (64 MiB protected, 16 MiB per record); `CausalLimits` carries a source comment saying it is "not a radio budget." | `crates/stickleback/src/{drop,drop_io,receipt,causal}.rs` |
| Courier store | Outrider propagation node: opaque `(destination, ciphertext)` by transient id, proof-of-work admission, capacity eviction, age expiry, two-stage offer / want-handled / serve fetch. Never decrypts. Defaults: 240 bytes per message, one per fetch. | `retinue/crates/outrider/src/propagation.rs` |
| Reconciliation | None a non-iroh carrier can use. `JoinedSpace::join` takes `p2panda_net::{Endpoint, Gossip}` and builds LogSync over iroh QUIC plus an iroh-gossip overlay. The direct session trigger upstream is `#[cfg(test)]`. The only bilateral operation lane, murm's session lane, is a one-way push with no frontier, want-list, ack or resume. | `joined_space.rs:133-172`; `p2panda-net/src/sync/handle.rs:76-86`; `crates/murm/murm/src/session_lane.rs` |

**Strongest receipt.** The archived Commons direct-PHY RF receipt of
2026-07-27: one 1,177-byte signed, group-encrypted Commons operation crossed
two LoRa boards (V4 and T114, SF8, BW 250 kHz) through a retinue Resource at
MTU 255, byte-exact in 9.9 s, then decoded, signature-verified, decrypted with
the stickleback keyring and matched to the expected event. It is a carriage
receipt, not a sync receipt, and says so.

**Export is not sync.** `export_topic_operations_selected` takes no peer
state: it resolves every log under a topic, asks the domain selector for
omit / header / full per operation, sorts by priority and fills a byte budget.
Mesh's `catch_up` selector filters against the node's own checkpoint, never
a peer's. Nothing in the tree accepts a remote frontier.

## 3. Why the frontier vector, not set reconciliation

For per-author, sequence-numbered, hash-linked logs, "author A: retained from
150, head 200" is an exact reconciliation summary in constant space per
author, one message, zero round trips. Willow, iroh-docs and negentropy exist
to reconcile unordered sets with no per-author linearity, and pay logarithmic
sequential round trips for it. On this carrier the round trip is the scarce
resource, not the byte. Adopting RBSR here would pay the general-case price
for a special case the data already has. The one place RBSR earns its keep is
agreeing the set of authors, which is exactly and only what tinySSB's GOset
uses it for, and ruling 2 removes even that need.

p2panda's `Have` message already is a frontier vector
(`BTreeMap<VerifyingKey, BTreeMap<LogId, SeqNum>>`). Its problems are size
and driver, not shape:

- ~70 bytes per (author, log) pair with 32-byte log ids: seven authors fill a
  500-byte packet, a hundred authors is ~7 KB, sent by both peers every
  session even when nothing changed. `LogId` is a blanket impl over any
  ordered serde type, so a compact id costs no code change in p2panda.
- Four lock-step round trips before the first operation moves (topic
  handshake, `Have`, `PreSync`). `PreSync` carries two `u32` counts for
  telemetry only.
- Session state lives on the async stack with no session id, checkpoint or
  resume; stream closure is `UnexpectedStreamClosure`, and the retry re-runs
  the whole session from `Have`. Over a link that is expected to come and go
  this is a livelock generator.
- The only production session trigger is an iroh-gossip neighbour event,
  joined unconditionally per topic subscription.

What the wire type lacks and the internal type has: p2panda's `compare`
produces `(Option<from>, Option<until>)` ranges per log. A retained floor on
the wire is the one field this brief adds, and it is the answer to the
pruning question none of the surveyed designs solves ("up to 200" cannot say
"deleted 1 to 150").

## 4. Prior art, and what each contributes

| Design | Takes | Leaves |
|---|---|---|
| tinySSB (Tschudin; verifier already at `retinue/crates/tinyssb-core`) | The pattern: a per-feed frontier as the whole reconciliation; DMX precomputation so "what I want next" is a 7-byte header, no session. The only surveyed design built for this carrier. | 120-byte frames, 255-feed GOset ceiling, no pruning, a different feed format. Retinue's TP0 scopes exact interop separately. |
| Briar BSP / BTP | The mode taxonomy: interactive (2 RTT), batch (1 RTT), eager (resend rather than await ack); the per-peer send-state table with a `max latency` field; removable media and two-week latency as design assumptions. | 32 bytes per message id, linear, because it syncs an unordered graph. |
| LXMF propagation (outrider) | The courier store: opaque, proof-of-work admitted, expiring, node cannot read content. The only surveyed design with a real pruning story. | The fetch is a flat offer list, 32 bytes per id, with no way to express a frontier. |
| Willow Drop Format | The honest courier: an encrypted self-contained bundle with no reconciliation, zero round trips. Selection is left undefined; a frontier is what makes it non-guesswork. | Its live protocol (WGPS) assumes a reliable ordered stream and mutual capability handshake. |
| negentropy | The only RBSR with a first-class small-MTU answer (frame-size limit, at the cost of extra rounds and duplicate reports). Reach for it if a genuinely unordered set ever needs reconciling (blobs, a keyed store). | Not for logs. |
| SSB EBT | Request skipping: send only feeds changed since the last exchange. | JSON vector clock at ~60 bytes per feed. |
| DTN BPv7 | Vocabulary: lifetime-bounded bundles, endpoint ids. | Custody transfer was removed from the v7 core, so the courier semantics are ours to supply. |

## 5. The shape

**Wire vocabulary.** p2panda's `LogSyncMessage` variants `Have`, `Operation`,
`Done`, postcard-encoded with the existing 4-byte frame prefix from
`p2panda-net/src/codec.rs`. `PreSync` is not sent. Two changes to the
message content, neither touching the operation format:

1. A compact `LogId` (ruling 2: roster index where a scope exists, full key
   otherwise).
2. A retained floor beside the head, so a `Have` entry is a pair.

**Author compaction from the roster.** A scope is a moot with a governed
membership roster (see `moothold_docs`), so both peers already hold an
ordered author set and changes to it are governed operations. The frontier
for a moot is an offset plus a varint pair per member in roster order. A
hundred members is a few hundred bytes, one packet at MTU 500. Peers without
a shared roster fall back to the full key.

**Three session shapes.**

- *Live link.* One `Have` as a request over retinue's request lane; the
  response is the operations past it, byte-budgeted, delivered by
  `send_payload_with_config` as one packet or a Resource. Each exchange is
  independent: a dropped link loses one response, not a session. No
  handshake beyond retinue's own link setup (297 bytes).
- *Eager push.* The sender keeps a per-peer send-state table (Briar's shape
  over frontiers) and pushes everything past the last acknowledged frontier
  unprompted. The acknowledgment is a frontier. Murm's `push_posts` is this
  minus the frontier and the ack.
- *Courier.* A frontier-selected native drop with radio limits, encrypted,
  stored at an outrider propagation node under the moot's destination and
  fetched by transient id exactly as LXMF messages are today. The node never
  reads it; `DropReceipt` prevents re-import. This fills the 2026-07-12
  plan's empty row.

**One ingest path.** All three feed p2panda-stream through stickleback's
processor, the same path gossip, LogSync, local authoring and drop import
already use. The lane is a projection of the store, never a second
authority.

**Not a port of p2panda-net.** `p2panda-core`, `p2panda-stream`,
`p2panda-sync`'s `Protocol` trait and `p2panda-net`'s codec are used
unchanged. The iroh-specific part of the sync path is one 139-line session
driver plus the gossip trigger and a `TransportAddress` enum with one
variant; none of it is needed here. The fork's `accept_raw` commit is a
QUIC-composition seam and does not bear on this.

## 6. Rulings (Mark, 2026-09-01)

1. **Wire shape: p2panda's own LogSync messages, statelessly driven.** A
   fresh protocol was rejected as unnecessary. The vocabulary is kept; the
   driver, the preamble round trips and the gossip trigger are replaced.
2. **Author compaction: roster index where a scope exists, full key as
   fallback.** No separate set-agreement protocol.
3. **Home: stickleback**, as a second sync lane beside `JoinedSpace`.
   `murm/transport` supplies the carrier unchanged; gemot supplies the
   roster ordering through the authority view Commons already consumes.
   Neither murm nor gemot owns the protocol.
4. **Operation carriage is a spike, not a ruling.** Whether operations ride
   as unchanged p2panda bytes or behind a shadow header is decided by
   measurement (§7), per retinue's MC0.
5. **This brief lives in mere**, as the cross-repo synthesis; retinue's
   compact-feed plan keeps the radio-native feed half.

## 7. The spike

Not a plan; the plan is written after these numbers exist. Done when, in a
dirty-tree-refusing test:

- Two root-signed p2panda operations (sequence zero, sequence one with a
  backlink) round-trip through a retinue Link loopback and a Resource
  loopback with identical header bytes, body bytes and operation ids. This
  is retinue's MC0 control case, run from mere's side.
- One Commons chat operation is measured whole and header-only, and the
  count is expressed as link packets at MTU 255 (175 plaintext bytes per
  packet) and MTU 500 (367). The 2026-07-27 receipt's 1,177 bytes is the
  baseline.
- A single `Have` and `Operation` exchange runs statelessly over retinue's
  request lane between two in-process endpoints, with a compact `LogId`, and
  a second exchange with nothing changed transfers only the `Have`.

Real RF airtime is a second slice needing the two boards attached and is
held until the loopback numbers exist.

## 8. Open questions

- **Pruning under a frontier.** The retained-floor field states the fact;
  what a receiver does when the sender's floor is above its own head (a gap
  it can never fill) is stickleback's epoch-retention question, unchanged by
  this brief.
- **Courier selection without a destination.** LXMF's store works because
  each message has one destination. A log operation belongs to a moot, and
  the moot's destination is what the courier drop is addressed to. Whether
  a per-moot destination leaks membership metadata to the propagation node
  is a dramatis-side question.
- **Identity on the composed carrier.** Reticulum authenticates no
  initiator; identity arrives via a Notochord session proof bound to the
  link. Retinue's AT1 finding that requiring carrier identity to equal the
  inner Noise peer "structurally rejects Reticulum plus Noise" stands and is
  not addressed here.
- **Radio budgets.** `DropLimits`, `CausalLimits` and the Commons v1
  admission limits (64 parents, 1 MiB payload, 16 MiB Knot documents) have
  no radio profile. `MeshDropProfile::Radio` only reorders priorities.
- **Upstream.** p2panda's TODO about exposing `initiate_session` and its
  own README roadmap item for delay-tolerant delivery over LoRa and BLE
  suggest the stateless driver may be contributable. Not pursued yet.
