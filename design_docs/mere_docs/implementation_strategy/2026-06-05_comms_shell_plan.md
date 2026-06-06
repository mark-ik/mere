# Comms Shell Plan

**Date**: 2026-06-05
**Status**: Draft (for review). The realization of Mere's **comms surface** in
meerkat: a docked peripheral pane that surfaces unified communications (misfin
mail + murm cabals, with room for mooting protocols), rendered through the same
domain → host pattern as the chrome. It closes the [modular integration
plan](2026-06-02_modular_integration_plan.md)'s **gap #7** ("murm/moot
unsurfaced") and its **S5** (comms surface), and is the on-screen form of the
inherited `COMMS_AS_APPLETS` family the [protocol architecture
plan](2026-05-05_protocol_architecture_plan.md) names.
**Grounded in**: a read of the actual crates this session (misfin, errand,
nematic, murm, the chrome domain) against the 2026-06-05 tree.

---

## The one idea

Comms is a **contingent projection like gloss / apparatus / settings**: a docked
peripheral pane, never a root. It follows the chrome pattern exactly. A
host-neutral **comms domain** crate (sibling to `chrome`) holds the
protocol-agnostic model (conversations, messages, identities, compose) over a
`ProtocolAdapter` seam; meerkat renders it as a docked pane; message bodies ride
the `nematic` engines the content card already uses. The backends (misfin, murm,
later mooting) are adapters behind the seam, so the shell is general from day one
and lights up each protocol as its backend matures.

---

## Decisions (resolved with Mark, 2026-06-05)

1. **Scope: the general comms shell**, not misfin-only. The pane is the
   comms-applet shell from the start (misfin + murm cabals + room for mooting
   protocols like Matrix / Nostr). Live adapters arrive as backends mature.
2. **misfin transport moves into errand (send lane).** errand gains an async
   misfin **send** lane reusing its `tls` module + the `titan_upload` write shape
   + client certs; the standalone sync misfin transport retires.
3. **misfin receive = run a server**, with an **external-server option**. Since
   errand is a *client* fetch transport, the server is **not** errand's job: the
   misfin crate keeps identity + types and gains the server; errand owns only the
   client send lane. A user can self-host the server or point at an external one.
4. **Pane = docked frame.** The first peripheral/docked pane, establishing the
   pattern gloss / apparatus / settings reuse (gap #6 / S4-adjacent).
5. **Pull murm Phase 2B forward** (`Cabal::send` / `subscribe` / `history`) so the
   shell opens with real two-way cabal conversations, not just misfin mail.

---

## Findings (grounded in the crates, 2026-06-05)

1. **The architecture mirrors chrome, in five layers.** Transport (errand send +
   misfin server) → identity (persona vault) → **comms domain** (new, host-neutral)
   → host pane (meerkat, docked) → rendering (nematic). `chrome` lives at
   `crates/graphshell/shell/domain/chrome`, so the comms domain is its sibling
   `crates/graphshell/shell/domain/comms` (relocates with the domain layer if §7
   cleanup moves it).
2. **Backend readiness is partial — this drove the decisions.**
   - *misfin* (`crates/murm/misfin`) is **send + identity only**: `send_message`,
     `identity_status`. There is **no receive / inbox / server** (hence decision 3).
     It is synchronous (std TCP + blocking rustls), self-contained.
   - *murm* (`crates/murm/murm`) is foundation: `SyncedCabal` (the LogSync lane)
     exists, but `Cabal::send` / `subscribe` / `history` is **Phase 2B, unbuilt**
     (hence decision 5). murm's own docs name the comms panel as a *separate*
     consumer — this pane.
   - *mooting* (`crates/moot/mooting`) is a **stub**; Matrix / Nostr / etc. are
     later adapters.
3. **errand fits misfin send, not receive.** errand is async, scheme-routed
   `fetch(url) -> Response`, with a `tls` module, a `CertRequired` status, and a
   `titan_upload` write companion. misfin **send** (a client-cert TLS write to a
   recipient host:1958) is the same shape as titan upload. misfin **receive**
   (serving a mailbox) is a server, a different shape errand should not take on.
4. **nematic already renders the body.** [`MisfinEngine`](../../../crates/inker/engines/nematic/src/misfin.rs)
   parses a gemmail body as gemtext → `EngineDocument` (`message/x-misfin`); the
   *envelope* (sender / recipient / timestamp / cert trust) is the host's job, and
   trust stays `Unknown` until the host verifies the cert. So the pane feeds the
   body to nematic and supplies the envelope + trust override.
5. **Identity belongs in the persona vault.** The protocol-arch plan reserves a
   `Misfin { keypair, handle }` vault slot; today misfin owns a standalone cert
   store. Tying the misfin cert to the persona vault makes the shell's "me" a
   persona.

---

## Phases (done-conditions, not dates)

Each phase is independently green; P1 / P2 and P4 are largely parallel, P3 gates
the inbox, P5 depends on the adapters, P6 is the visible payoff.

- **P1 — errand misfin send lane.** Add an async misfin **send** to errand
  (reusing `tls` + the `titan_upload` write shape + client certs); retire misfin's
  sync TLS transport. *Done:* errand sends a gemmail to a host and returns the
  outcome; the misfin crate's send delegates to (or is replaced by) errand's lane.
- **P2 — misfin identity in the persona vault.** Wire the `Misfin { keypair,
  handle }` slot: derive / store the misfin client cert in the identity vault, so
  send + server present a persona identity. *Done:* the shell's misfin identity is
  persona-derived, not a standalone cert file.
- **P3 — misfin server (receive).** A misfin server in the misfin crate accepts on
  `:1958`, verifies sender certs (TOFU), and lands gemmail in a local mailbox
  store; a config points at an external server instead. *Done:* a gemmail sent
  (P1) lands in the recipient's inbox, self-hosted or via an external server.
- **P4 — murm Phase 2B (pulled forward).** Fill `Cabal::send` / `subscribe` /
  `history` over the Cable protocol on the existing `SyncedCabal` substrate.
  *Done:* a cabal has a real send / history / live-subscribe API for the shell.
- **P5 — the comms domain crate.** `crates/graphshell/shell/domain/comms`: a
  host-neutral `Conversation` / `Message` / `Identity` model + compose state + a
  `ProtocolAdapter` trait, with a **misfin adapter** (mailbox → conversations,
  gemmail → messages, identity) and a **murm adapter** (cabals → conversations,
  posts → messages). Headless-tested, no host dependency. *Done:* one unified
  comms model populated from both backends.
- **P6 — the meerkat comms pane (docked).** Establish the docked peripheral-pane
  slot in the host (the gloss / apparatus / settings pattern), and render the comms
  domain into it: identity, a conversation list, a reader (envelope + nematic body),
  and a compose / send view. *Done:* open the comms pane, see your identity and
  conversations (misfin mail + murm cabals), read + compose + send.
- **Later:** mooting protocol adapters (Matrix / Nostr / …) as those backends land;
  external-server polish; gloss / apparatus / settings reusing the P6 docked-pane
  slot.

**First shippable, demoable milestone:** P1 + P2 + P3 give send + receive misfin
end to end (provable headless with two local identities / a local server); P5 + P6
make it a live shell (compose, send, read, conversation list) with murm cabals
(P4) alongside.

---

## Open design points

- **Where the docked-pane mechanism lives.** Full `frame::FrameLayout` (the tiled
  S4 machinery) vs a simpler docked-strip slot first. The general shell does not
  need BSP tiling; a single docked side panel (resizable, toggleable) is enough for
  P6 and for gloss / apparatus / settings. Lean: a minimal docked-pane slot now,
  `FrameLayout` when the workbench (S4) actually needs splits.
- **The misfin server's home.** In the misfin crate (a `server` module beside the
  client) keeps it cohesive; a separate `misfin-server` crate isolates the
  accept-loop / mailbox-store dependencies. Lean: a module in the misfin crate
  until the deps argue for a split (the 600-LOC ceiling may force the split).
- **The comms-domain model shape.** How a `Conversation` is keyed across protocols
  (misfin mailbox thread vs murm cabal vs a future Matrix room), and the identity
  model (one persona, many protocol handles). Settle in P5.
- **External misfin server config.** How a user points the shell at an external
  server (a setting, a per-identity host field). P3 detail.
- **Async move correctness.** Moving misfin send sync → async (into errand) must
  preserve cert generation + TOFU known-hosts behaviour; port the misfin
  transport tests alongside.

---

## Risks and hard parts

- **Six-phase, multi-crate, sibling-repo arc.** errand (sibling repo) + misfin +
  murm + a new domain crate + meerkat + nematic. Sequence so each phase lands
  green; do not let the pane (P6) get ahead of the adapters (P5) or the backends
  (P3 / P4).
- **murm Phase 2B is real protocol work**, not glue: the Cable `send` / `history` /
  `subscribe` API over the per-author-log substrate. Budget it as its own slice.
- **The docked-pane pattern is new host architecture.** gloss / apparatus /
  settings inherit it, so get the slot's shape (placement, resize, toggle, input
  routing, separate-roots discipline) right the first time.
- **misfin server is a network listener** inside the app: bind lifecycle, cert
  verification, mailbox persistence, and the never-fatal-to-the-shell discipline
  (a server failure disables receive, not the shell), mirroring the sync subsystem.

---

## Progress

- **2026-06-05** — Plan drafted (no code). Read the crates (misfin = send +
  identity only, no receive; murm = foundation, send/history is Phase 2B; mooting =
  stub; errand = async client fetch with `tls` + `titan_upload`; nematic
  `MisfinEngine` renders bodies, envelope is the host's job; chrome domain at
  `crates/graphshell/shell/domain/chrome`). Resolved four decisions with Mark
  (general comms shell; misfin send into errand; receive = a misfin server with an
  external-server option; docked frame pane; pull murm 2B forward). Sliced the work
  P1 (errand send) → P2 (vault identity) → P3 (misfin server) → P4 (murm 2B) → P5
  (comms domain crate) → P6 (docked meerkat pane), with mooting adapters later.
  Next: Mark's steer on the first phase to build.
