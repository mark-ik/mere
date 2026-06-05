# IRC Mod Plan

**Date**: 2026-05-05
**Status**: Draft / branching from the cross-cutting protocol architecture plan. **Reframed by the 2026-05-07 [moot-tiers brief](../../mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md):** Pattern A is now "thin-client routing — the moot links to an IRC channel and members open it via a `mooting-irc` adapter that doesn't translate IRC into moot semantics," and Pattern B is "outbound bridge in a separate `mere-bridge-irc` crate, not a `mooting-*` adapter, since IRC's wire format can't fully express tessera or engrams." The plan's T1 / T2 / T3 cost tiers also reinterpret through the new tier framework (orrery → moot → moothold → coalition). Re-read the moot-tiers brief before extending this plan.

**Scope**: Concrete plan for IRC as the **first T1 protocol mod** in Moothold (per
[`../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) §5.5).
Wraps the maintained [`irc`](https://crates.io/crates/irc) crate; lands as
both Pattern A (user-orrery-pinned) and Pattern B (moot-relayed; puppet mode
only) per the plan's pinning patterns.

**Related**:

- [Protocol architecture plan](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) §5.0–§5.5 — split rule, pinning patterns, cost tiers
- [Inherited IRC positioning](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/2026-04-09_irc_public_comms_lane_positioning.md) — Graphshell-side public-lane framing (still relevant after the rename)
- [Inherited COMMS_AS_APPLETS](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md) — applet surface family that consumes the mod
- [`MURM_AS_BILATERAL.md`](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md) — Murm's boundaries (this mod sits in moothold, NOT murm; the protocol architecture plan §5.0 corrects MURM_AS_BILATERAL's earlier framing — IRC `/msg` lives with the network/channels in Moothold, not partitioned)

---

## 1. Why IRC first

Per [protocol architecture plan §5.5](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md), IRC has the right combination for a Phase 3 leadoff:

1. **T1 cost tier.** Wraps the maintained [`irc`](https://crates.io/crates/irc) crate. Weeks-of-work integration, not months-of-spec-implementation.
2. **Stress-tests Pattern B end-to-end.** IRC needs a long-lived TCP connection — exactly the workload where moot-host-as-relay matters. Visitor doesn't have to keep the IRC socket alive; the moot does.
3. **Shape-simple wire protocol.** Line-oriented; no E2EE crypto state to model in the vault. Slot is `IdentitySlot::Direct` — SASL credentials are bytes the vault stores.
4. **No federation propagation problem.** Unlike Matrix, IRC has no federation; the puppet-vs-portal mode question that complicates Matrix collapses here. A visitor's "ghost" is just another nick on the moot's connection.
5. **User explicitly named it** as the Phase 3 first cut.

The mod also serves as the architectural template for the other T1 protocol mods (Nostr, Matrix wrap of `matrix-rust-sdk`, ATproto wrap of `atrium-rs`) that follow.

---

## 2. Crate layout

The mod ships as **`mere-mod-irc`** under `crates/moothold/mere-mod-irc/`:

```text
crates/moothold/
├── moothold/                  # supercrate (existing)
├── mooting/                   # protocol-mod-core (existing)
└── mere-mod-irc/              # new — IRC protocol mod
    ├── Cargo.toml
    └── src/
        ├── lib.rs             # public surface + PrimitiveMootProtocol impl
        ├── client.rs          # connection lifecycle, command write/read
        ├── relay.rs           # Pattern B moot-host relay; puppet attribution
        ├── identity.rs        # SASL credential plumbing → vault slot
        └── tests/             # integration tests against a local ircd
```

Naming convention: `mere-mod-<protocol>` for protocol mods. Reserves the namespace and lets `mere-mod-matrix`, `mere-mod-nostr`, etc. follow the same shape.

Add to crates.io? Reserve when the crate name is needed for `cargo publish`; not before. Pre-publish scaffolding stays workspace-local.

---

## 3. Identity vault slot

IRC slot lands as a `Direct` variant with `kind = "irc"`:

> *Illustrative — signature-only, not implementation-ready. The
> declarative shape lives in [`mere-identity::vault::IdentitySlot`](https://docs.rs/mere-identity/0.0.1/mere_identity/vault/enum.IdentitySlot.html).*

```rust
// Constructed by mere-mod-irc; stored opaquely by the vault.
let slot = IdentitySlot::Direct {
    kind: "irc".into(),
    payload: serialize_irc_creds(IrcCredentials {
        network: "tilde.chat".into(),
        nick: "mark".into(),
        sasl_username: Some("mark".into()),
        sasl_password: Some(secret),
        tls: true,
    }),
    lineage: CredentialLineage::ExternallyIssued, // NickServ password
    unlock_tier: UnlockTier::ShortTtl { idle_seconds: 900 },
};
let key = ProtocolKey::new("irc", Some("tilde.chat".into()));
vault.add_slot(key, slot)?;
```

Notes:

- **Multiple networks**: one slot per network, distinguished by `ProtocolKey::instance` (`"tilde.chat"`, `"libera.chat"`, etc.).
- **Lineage = ExternallyIssued**: NickServ passwords rotate / can be revoked by the network. Not recoverable from the master.
- **Unlock tier**: `ShortTtl` default — IRC creds are sensitive (network operator can read all your channel activity if compromised). User-overridable to `Session` for convenience or `PerUse` for hardened setups.
- **Serialization**: bincode or postcard inside the `payload: SecretBytes`. The mod owns the encoding; the vault stores opaque bytes.

---

## 4. Pattern A — user-pinned IRC node in the orrery

When the user installs `mere-mod-irc` and pins an IRC channel into their personal orrery:

1. The orrery node is bound to a `(network, channel)` tuple.
2. On open, the mod reads the user's IRC slot from the vault for that network.
3. The mod establishes a TLS TCP connection to the IRC server using the `irc` crate.
4. SASL auth via the slot's credentials.
5. JOIN `#channel`.
6. Stream messages into a per-channel buffer; expose as a `Stream<Item = ChatMessage>` for the comms applet UI.
7. On user input, PRIVMSG to the channel.

### 4.1 Connection lifecycle

| Event | Action |
|-------|--------|
| First open | Open connection, SASL, JOIN; populate buffer from this point forward |
| Background (user navigates away) | Keep connection alive; continue buffering |
| User closes node | Send PART, close connection (or leave if other channels on same network are open) |
| Network glitch | Reconnect with exponential backoff; deduplicate against last-seen line per channel |
| Reload Mere | Re-establish from slot; replay any locally-cached recent buffer |

Per protocol architecture plan §3.3 (Element-style sub-instances), per-profile sub-instances live concurrently — switching profiles in the UI doesn't kill IRC connections of the previous profile, just hides their nodes.

### 4.2 IRCv3 capabilities

Phase 3 first cut targets **just the floor**:

- `sasl` — auth
- `server-time` — accurate timestamps, not server-now
- `away-notify` — neighbor presence
- `multi-prefix` — accurate user mode display
- `extended-join` — account info on join
- `message-tags` — for typing indicators et al (future)

Out of scope for the first cut (per inherited [`2026-04-09_irc_public_comms_lane_positioning.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/2026-04-09_irc_public_comms_lane_positioning.md) §1):

- DCC / file transfer
- bouncer protocols (ZNC etc.) — visitor uses Mere as bouncer-equivalent, not in addition to one
- chathistory / batches with replay (post-2.0)
- operator-only commands

---

## 5. Pattern B — moot-relayed IRC channel (puppet mode)

When a moot operator places an IRC channel as a primitive moot node:

### 5.1 Mounting

1. Moot operator goes through `mere-mod-irc`'s "mount in moot" flow.
2. Picks a moot-bot identity slot (a separate vault slot — could be the same person's IRC nick or a moot-dedicated bot).
3. Picks the network + channel.
4. The moot host opens the IRC connection on the moot-bot's identity, JOINs the channel, and runs the relay.

### 5.2 Visitor flow

Per protocol architecture plan §5.1.1, Phase 3 ships **puppet mode only**:

- Visitor enters the moot's graph and sees the IRC node.
- Their Mere opens a stream over `mere/moothold/primitive-node/v1` to the moot host.
- Moot host streams the channel buffer to the visitor (live messages + recent backfill).
- When the visitor types in the IRC node UI, their bytes go to the moot host, which forwards as `PRIVMSG #channel :<visitor's display name> <message>` from the moot-bot's nick.

The "via moot" prefix is critical: messages sent by visitors appear in the IRC channel *attributed to the moot-bot*, not to a per-visitor nick. This is the matterbridge-style semantics.

### 5.3 What's lost in puppet mode

Documented limitations the moot UI surfaces explicitly:

- IRC sees **one nick** (the bot) regardless of how many visitors are watching.
- IRC ops can't kick / ban a specific visitor — they can only kick the bot, which removes everyone.
- Visitor's "join" / "leave" events do not propagate to IRC; only the bot's join is visible to the channel.
- Direct messages (`/msg <visitor>`) from IRC users to a visitor: **not supported in v0**. Some kind of moot-side mailbox or webhook may be added in a follow-up.

Worth-noting non-loss: IRC's lack of federation means there's no "this MXID is now permanently on `bridge.example`" propagation problem like Matrix has. The puppet's nick is the moot-bot's choice, scoped to the moot's IRC connection.

### 5.4 Scaling cliff

Per protocol architecture plan §7 — the single-moot-host relay is Phase 3 scope. When a moot's IRC channel has more visitors than the host can carry, the named candidates (kith/kin volunteer relays, hosted fallback, time bank) need real prototypes. **Not Phase 3.**

---

## 6. ALPN registration

The IRC mod does **not** register a separate Mere ALPN — it uses the existing
`mere/moothold/primitive-node/v1` ALPN (per protocol architecture plan §2.3) for moot-host → visitor streaming. The mod-supplied protocol negotiation (which moot node is being viewed, what backfill window, etc.) rides on top of that as a sub-protocol agreed by mod-author convention.

Production-side IRC traffic (TCP to the IRC server) does **not** ride iroh — it's plain TLS-over-TCP using the `irc` crate's transport.

---

## 7. Phase boundaries

### Phase 3.0 — Mod scaffold + Pattern A

- `mere-mod-irc` crate scaffolded, depends on `irc`, `mere-identity`, `mere-transport`, `mooting`.
- `IrcClient` + lifecycle (connect, SASL, JOIN, stream, send).
- Vault slot integration (read on open, write on save).
- Tests: connect to a local ircd test fixture; round-trip a PRIVMSG.

### Phase 3.1 — Pattern B puppet relay

- `IrcMootRelay` runs on the moot host; serves visitors via the primitive-node ALPN.
- Visitor-side: render messages, accept user input, route through the relay.
- Tests: puppet-mode round-trip — moot bot is in IRC, visitor in moot sends a message, IRC channel sees the bot post it.

### Phase 3.2 — Comms applet UI

- Surface in graphshell-side comms applet (per `COMMS_AS_APPLETS`).
- Compose, scrollback, channel list, network list.
- Out of scope here — driven by graphshell-side work; this plan exposes the mod's API surface.

### Future (post-Phase-3) — research probes

- Portal-mode (per-visitor nick on bridge): requires running an IRC server. Substantial scope; not warranted before user demand.
- ChatHistory replay on reconnect.
- IRC-side DM bridge (moot-mailbox approach).
- Bouncer interop — `mere-mod-irc` as "the bouncer", reachable from external IRC clients. Niche.

---

## 8. Open questions

1. **Moot-bot identity slot ownership.** Is the bot's vault slot owned by the moot operator's profile, or does Moothold have its own per-moot identity-vault concept? *Pending Moothold's own identity model — see protocol architecture plan §7 still-open #2.*
2. **Per-visitor display-name policy.** When visitor "alice" types, the bot says `<alice via mere-moot> hello`. What if alice changes her display name mid-session? Refresh per-message vs cache?
3. **Channel topic / mode changes**. Should puppet-mode visitors be able to *suggest* topic changes (sent as a PRIVMSG by bot for the operator to action), or read-only?
4. **Buffer retention**. How much backfill does the moot host keep for late visitors? Per-channel, with a TTL? *Persistent storage shape is shared with Cable's persistent-cabal-store decision; lift that pattern.*
5. **Network-level config (autojoins, autorelogin, server password)**. Lives in the slot, or in moot config separate from creds?

---

## 9. Acceptance standard

Phase 3.0 + 3.1 are acceptable when:

1. A user can install `mere-mod-irc`, register an IRC slot in the vault, and pin an IRC channel as an orrery node; messages flow both directions.
2. A moot operator can mount an IRC channel as a primitive moot node; visitors in the moot can read and send to the IRC channel; messages are bot-attributed in IRC.
3. The mod gracefully reconnects across network glitches without duplicating displayed messages or losing user input mid-flight.
4. Removing the mod cleanly disconnects all running IRC connections (orrery + moot).
5. IRC slot bytes are stored in the vault using the established `IdentitySlot::Direct` shape; load/save round-trips through both `InMemoryStorage` and `PassphraseEncryptedStorage`.

---

## Findings

(Populate during execution.)

## Progress

### 2026-05-05

- Plan drafted. Branches from the cross-cutting protocol architecture plan §5.5 (IRC as Phase 3 T1 first cut).
- No code yet; this plan is the design step. Code work begins with Phase 3.0 once the plan is signed off.
