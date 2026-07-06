# Comms Gating, Ticket Sharing, and Key-Addressed Misfin

**Status (2026-07-06):** planning (with Mark). Decisions taken; code deferred until the
in-flight one-state migration lands (it holds `shell_new.rs` / `command_drain.rs`, both
of which this plan touches). Companion to the
[persona_transport_unlinkability_plan](2026-06-25_persona_transport_unlinkability_plan.md)
(this plan builds its Mode 1 host surface) and the
[persona_wallet_carry_layer_plan](2026-06-25_persona_wallet_carry_layer_plan.md)
(capability-gated admission is the eventual invite-revocation story).

## What this changes

Today `spawn_comms` activates everything at app launch: mailbox open, misfin receive
server on TCP `0.0.0.0:1958`, p2panda transport bind, demo-cabal subscribe, ticket
logged. After this plan, comms is **off by default** and the user chooses, per persona,
when each stage comes up:

1. **Start comms (local).** Mailbox + adapters + pane data. No sockets.
2. **Go online (network).** p2panda bind + misfin receive server, for one or more
   chosen personas. The join ticket is shown automatically on success.

The locked-startup work already built the lifecycle this needs: the comms actor holds
`backends: Option<..>` and its `UnlockNow` / `LockNow` arms are activate-on-demand and
teardown (`comms_host.rs`). This plan generalizes that unlock-keyed gate into a
user-and-persona-keyed one on the same seam.

## Decisions (Mark, 2026-07-06)

- **Default off for comms.** Local comms and network comms are *separately* gated;
  neither runs until the user acts. Auto-start becomes a per-persona setting,
  default false.
- **Ticket auto-shown on go-online, with a regenerate affordance** (see the ticket
  findings below for what "regenerate" honestly means).
- **Several personas online simultaneously** is the target shape: transports keyed by
  persona in one comms actor, started and stopped independently.

## Delivery-mode and bridge design space (2026-07-06)

One native mail object (sealed, content-addressed, persona-keyed); delivery mode is a
rendezvous choice over substrates Mere already has, and **which modes are worth
building is a function of the carrier's cost model**, not a free menu.

Native modes, carrier-independent:

- **Direct** — end-to-end link, no intermediary. misfin-over-iroh (G5); today's LAN.
- **Stored** — an async holder keeps ciphertext until the reader syncs. The G6
  voluntary-hosting store node.

Modes that already exist as substrate or are near-free, worth naming:

- **Gossip-drop** — ride a shared overlay (cabal / moot topic); the object arrives
  without a dedicated link. This already exists: a murm cabal post *is* this. It makes
  a 1:1 message and a cabal post the same object at different cardinality + privacy, so
  comms and the moot lane converge rather than being two systems.
- **Out-of-band** — QR / file / USB / paper; liveness requirement zero. The pairing
  ceremony already moves sealed bytes by QR; mail reuses that path. The off-grid mode.
- **1-to-many (cardinality axis, orthogonal to the above)** — envelope-wrap: seal once,
  wrap the content key per recipient, the same machinery the wallet uses for per-device
  epoch wraps. This is group mail without MLS epoch rekeys.

Carrier-specific / parked:

- **Opportunistic** (single unacked packet, no link) — a *constrained-carrier*
  optimization (LoRa / packet radio), near-pointless over iroh where link setup is
  cheap. Belongs to the retinue lane, not the native set.
- **Dead-drop at a derived rendezvous** (shared secret + time window → drop location) —
  a metadata-unlinkability variant of stored; parked until a threat model asks for it.

Bridge encodings sort by **bridge vs gateway**: a bridge re-encodes key-to-key at the
edge, in-app, and only works where the foreign net is key-addressed (a persona maps onto
a foreign identity); a gateway holds a hosted account in an account-addressed foreign
net, which is heavier, more centralized, and optional infrastructure rather than an app
feature.

- **Bridges (key-addressed, in-app):** misfin (smolweb, live, stewarded); LXMF via
  retinue (Reticulum world, gated on retinue); **Nostr** (npub = pubkey, NIP-17
  gift-wrapped DMs give metadata privacy — ruled out as a *native* lane for its public
  relays, but clean and high-reach in the *bridge* role; the best next bridge after
  misfin).
- **Gateways (account-addressed, hosted infra, not app-native):** email/SMTP (infinite
  reach, wrong ethos, deliverability pain, one-way outbound courtesy at most); Matrix
  (real E2EE + users, homeserver-heavy); ActivityPub (a *posting* bridge for the moot
  lane, not a mail bridge — its DMs are barely private).
- **Aligned but not worth it:** Briar / Cwtch (kin, Tor-coupled, tiny); SimpleX (no
  persistent identity, fights personas); Meshtastic (separate radio protocol, overlaps
  the population retinue + LXMF already reaches).

Ranking: misfin (have) → Nostr (key-native, big reach) → LXMF (gated on retinue).
Gateways are weighed only as optional hosted infra, never as app-native lanes.

## Findings (verified against code, 2026-07-06)

**Tickets are addressing hints, not capabilities.** `transport.ticket()` is
`EndpointTicket::from(endpoint_addr)`: the NodeId plus current dialable addresses
(`p2panda_transport.rs:401`). Consequences:

- Auto-reveal on go-online is right; the bind is what produces the ticket.
- "Regenerate" splits into two honest verbs. **Refresh ticket** re-reads
  `endpoint_addr()` (new addresses, same NodeId); an old ticket holder can still reach
  you. **Rotate transport identity** rebinds with a newly derived endpoint key (new
  NodeId); old tickets go dead, but so do existing peers' address books. True
  revocable *invites* are not a transport feature at all; they are the sync-admission
  gate (p2panda-auth / wallet capability slots), where the ticket gets a peer dialed
  but the gate decides whether bytes flow. The pane should offer refresh now, rotation
  as an explicit destructive action, and leave invite revocation to the wallet lane.

**Permissions per technology** (the disaggregation question):

- **Mailbox:** a local redb file. No permissions beyond the filesystem.
- **p2panda bind:** a UDP socket on an ephemeral port. No elevation. Holepunching
  means inbound rides outbound-established mappings, and the iroh relay covers the
  rest, so connectivity survives even a denied firewall prompt (at some direct-path
  cost). Windows may still show its consent prompt on first bind.
- **misfin receive server:** the one real inbound listener (TCP `0.0.0.0:1958`). On
  Windows, binding it triggers the Defender Firewall consent dialog automatically;
  that dialog *is* the in-app "request allowlist" mechanism, and gating server start
  behind a user button means the prompt appears in response to the user's own gesture.
  Programmatic rule-adding (`INetFwRule` / `netsh advfirewall`) needs elevation and is
  an installer concern, not runtime. On Fedora (firewalld default-deny) the app cannot
  self-add without polkit; detect the blocked state and instruct
  (`firewall-cmd --add-port=1958/tcp`). Mint (ufw typically off) and macOS (app
  firewall prompts) are easier. v1 handling: bind at gesture time, surface bind
  failure as pane status (real feedback, not a spinner), document the Fedora step.

**Misfin identity is already the cert, not the IP.** The server validates senders by
TOFU cert fingerprint; the `me@<lan-ip>` address is only routing. The send cert is
already persona-derived (`build_misfin_sender` derives from
`provider.derive_keypair(identity_salt(address))`), and `MisfinServerConfig` serves a
`Vec<ServedMailbox>`, so multiple addresses per identity is the existing model. The
host comment already names the extension: "reaching strangers privately is the
key-anchored-over-iroh path" (`comms_host.rs`). Misfin now lives at
`mark-ik/misfin` (git dep), so protocol-side additions land there.

**Key-addressed mail candidates.** Minimal: **misfin-over-iroh**, a `mere/misfin/v1`
ALPN on the existing transport; dial by persona-derived NodeId, speak the same gemmail
exchange over the QUIC stream. Iroh's handshake already gives mutual Ed25519
authentication, so the inner TLS can be dropped (map sender identity from the QUIC
peer) or kept for protocol fidelity; deriving the misfin cert and the endpoint key
from the same persona salt keeps fingerprints aligned. Offline tier: **Reticulum
LXMF** is the canonical key-addressed store-and-forward mail (destination = key hash,
propagation nodes hold mail for offline recipients), and `reticulum_transport.rs` is
already an active probe with announce-based address books. Rejected for this slot:
Nostr (relay-public by default, wrong privacy posture), SimpleX (no persistent
identity, mismatch with personas).

## Phases

- **G1 — manual-start gate.** Actor spawns idle; `CommsCommand::StartLocal` /
  `StartNetwork` / `StopNetwork` (bodies generalize the `UnlockNow` / `LockNow`
  arms); pane buttons through the existing `CommsIntent` path; per-persona
  `comms_autostart` setting, default false.
  Done when: a fresh launch opens no comms socket and touches no mailbox until the
  user acts, and the pane states (off / local / online) are visibly distinct.
- **G2 — local vs network split.** Split `activate_comms` into a socketless local
  half (mailbox, adapters, read/compose) and a network half (transport bind + misfin
  server), gated independently. Surface misfin bind failure as pane status; detect
  the firewalld-blocked case on Linux and show the instruction.
  Done when: "Start comms" alone binds nothing (verified by port inspection), and a
  denied firewall prompt yields an honest pane message rather than silence.
- **G3 — ticket reveal and regeneration.** Auto-show the ticket on go-online;
  "refresh ticket" re-reads `endpoint_addr`; "rotate transport identity" rebinds on a
  fresh derived key behind an explicit confirmation naming the cost.
  Done when: both verbs work live between two installs, and rotation demonstrably
  invalidates a previously shared ticket.
- **G4 — per-persona transports.** Key `CommsBackends` by persona; bind each
  transport with `derive_keypair(BLAKE3("persona" || persona_id))`; per-persona
  tickets; several personas online at once; misfin serves per-persona mailboxes from
  one listener.
  Done when: two personas online together present two unlinkable NodeIds and two
  tickets, each independently stoppable, and stopping one leaves the other connected.
- **G5 — key-addressed misfin lane.** `mere/misfin/v1` ALPN over the p2panda
  transport; a key-anchored address form alongside the compliant `user@host` lane;
  the LAN lane stays the trusted-local default.
  Done when: an install with no reachable misfin port receives mail from a peer over
  the iroh dial path, and the compliant LAN path still interoperates unchanged.
- **G6 — store-and-forward mail as voluntary hosting.** Research done up front
  (2026-07-06, see the
  [LXMF research brief](../research/2026-07-06_lxmf_key_addressed_mail_research.md));
  decision: LXMF-the-blueprint over iroh, not LXMF-the-protocol. Sealed
  content-addressed mail objects wrapped to recipient persona keys, held by
  store-and-forward nodes and synced on reconnect; the store node is the mail
  instance of voluntary hosting, with ingestion gated by tessera standing and
  capability rather than proof-of-work stamps (stamps stay in reserve for
  open-world ingestion). Protocol-level LXMF interop is an optional later bridge
  on the reticulum feature lane; if that lane gets real investment, the
  prerequisite is **retinue**, Mere's own endpoint-scoped Reticulum
  implementation (decided + named 2026-07-06 — the protocol is public domain,
  upstream stewardship is in flux post-founder-departure, and neither Beechat
  0.1.0 nor FreeTAK's daemon-shaped EPL stack fits a long-term library embed;
  see the transport plan's Direction section).
  Sequenced after G5; gated on the moot hosting substrate.
  Done when: a persona receives mail sent while its devices were offline, via a
  store node it does not own, without the sender or store node reading the body.

Open questions: whether the graph-sync actor (`sync.rs`) adopts the same default-off
gates (lean yes, separate slice); where rotation history lives (persona wallet is the
natural home); whether the demo cabal survives G1 or becomes a join-by-ticket flow;
**one native mail object, two delivery modes** — G5 (direct) currently implies gemmail
over iroh while G6 (stored) implies engram-shaped sealed objects, and these should be
one format with direct and stored delivery, with misfin (smolweb) and LXMF-via-retinue
(Reticulum world) as edge bridges rather than a second and third native format.

## Progress

- **2026-07-06** — plan drafted from the comms-gating discussion; decisions recorded
  (default off, both gates user-held, ticket auto-shown + regenerable, several
  personas at once). Code deferred until the one-state migration lands in
  `shell_new.rs` / `command_drain.rs`.
- **2026-07-06** — G6 research pulled up front and completed (web-grounded; see the
  research brief). Headline finding: a real Rust LXMF now exists (FreeTAK `lxmf`
  0.6.0, 2026-06-30, EPL-2.0, own `reticulum-rs` stack), while the Beechat
  `reticulum` crate the probe pins has sat at 0.1.0 since 2025-10. Decision:
  blueprint over protocol — G6 rewritten as "store-and-forward mail as voluntary
  hosting" (tessera-gated ingestion instead of PoW stamps; content-derived message
  ids and transient-id-at-rest borrowed from LXMF's design). G5 unchanged.
