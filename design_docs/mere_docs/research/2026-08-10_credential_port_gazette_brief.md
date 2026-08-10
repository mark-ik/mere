# The Dramatis Tier's Outward Surfaces: Credential Port + Gazette

**Date:** 2026-08-10
**Kind:** research brief (design probe; nothing here is scheduled)
**Anchors:** [dramatis tier plan](../implementation_strategy/2026-08-10_dramatis_tier_plan.md) (D4),
[contact identity model brief](2026-06-15_contact_identity_model_brief.md),
the 2026-07-22 vault/agent plan, the participant gate + packs plan.

The dramatis tier holds the cast list: personae (me), gaz (them, kept),
gazette (them, found). This brief maps its two growth fronts, which point in
opposite directions and share one spine. The credential port carries authority
*outward*: proving, signing, releasing. The gazette carries discovery
*inward*: resolving, reading. Both are persona-scoped, both compose gaz, and
both put the sensitive half behind the participant gate.

The origin observation (Mark, 2026-08-10, after weeks of using the personae
ssh-agent for real work including remote builds on the iMac): the agent
pattern generalizes to passwords, 2FA codes, keys, tokens, and a wallet, there
are standards to build against, and no equivalent exists with this shape.

---

## Part I: the credential port

### What already exists

- `personae`: vault, sealed records, DPAPI/OS-store unlock ladder, delegation
  grammar, signing, `ssh_slot`, and the ssh-agent (live on Mark's Windows box
  since 2026-07-22; the laptop SSH key is vault-only).
- The wallet carry layer sits in `session-runtime::{wallet_store,
  wallet_grant}` with the epoch sealer; the fold-in to personae is ruled and
  deferred (dramatis plan D4).
- `servitor::Gate`: the petition/approval primitive the authority half rides.

### Architecture: one port, two halves

The port lives in `mere/ports/`, sibling to graphshell and knot. The split is
the load-bearing decision:

- **Embeddable half.** Vault browse, credential status, TOTP tiles, picker
  integration. Any host app composes these views. They render *about*
  secrets and never contain them.
- **Authority half.** Credential release, signing, decryption approvals. Lives
  with the resident (mere/Graphshell per the 2026-07-22 ruling), receives
  requests as gate petitions, and answers over an agent-style channel.

This generalizes what the ssh-agent already proves: apps talk to a pipe, apps
never see the key. A host that embeds the browse half and is later compromised
can lie about *labels*, not exfiltrate secrets; the approval prompt renders on
the resident's surface, not the requesting host's. That is also the anti-
spoofing answer: any app may embed the vault view, only the resident asks for
consent.

### Standards inventory (rough cost order)

| Standard | What it buys | Notes |
|---|---|---|
| TOTP RFC 6238 / HOTP RFC 4226 | 2FA codes | Test vectors in the RFC appendices. Import via `otpauth://` URIs (the de-facto key-uri format). Small, self-contained, daily value. Steam's variant is nonstandard; decide explicitly whether to carry it. |
| ssh-agent protocol | SSH keys | Shipped (ssh-agent-lib 0.6). The template for every other agent surface. |
| CXF / CXP (FIDO Alliance Credential Exchange) | Import from 1Password, Bitwarden, Apple, Google | The 2024+ portability standard; drafts public, adopters shipping. An early Rust implementation would be notable and is the migration path for real users. |
| `org.freedesktop.secrets` (Secret Service) | Serve every libsecret app on Linux | The one OS surface a third party can *be* rather than read: personae as the D-Bus secrets backend makes existing apps consumers without knowing it. |
| KDBX 4 | KeePass-world import | Format documented; `keepass` crates exist as references. |
| BIP-39 | Seed-phrase carry | Adjacent to persona seed carry regardless of the wallet. |
| WebAuthn / CTAP2 passkey provider | Passkeys | The heavyweight item, and platform-entangled: Windows has a plugin-authenticator API (23H2+), macOS routes third parties through AutoFill provider extensions (entitlement territory), Linux has no blessed provider seam (virtual hidraw device or browser-level integration). Treat as its own project with per-platform plans. |

Explicitly *readable but not servable*: Windows Credential Manager and macOS
Keychain have no third-party backend seam; they are import sources only.

A browser bridge (native-messaging host, the KeePassXC pattern) is the other
high-leverage surface: it gives autofill without a full extension platform.
Bench it behind the OS surfaces.

### Prior art (read for technique, not adopt)

- **IOTA Stronghold**: Rust secrets engine; runtime isolation, guarded memory,
  non-exportable secret references. The techniques matter even where the
  runtime is not adopted.
- **keyring-rs**: the OS-keystore abstraction crate; overlaps the unlock
  ladder personae already has.
- **KeePassXC**: single-vault UX, browser native-messaging bridge, autofill.
- **Bitwarden**: open clients, CXF participant, the server-shaped contrast.
- **zeroize / secrecy** crates: secret-hygiene discipline for every buffer the
  port touches.

### The differentiator

Existing managers partition credentials into folders inside one identity and
sync through an account. Personae partitions by **cryptographic face**:
separate derivation roots, separate reveal surfaces, carried by seed rather
than cloud account, P2P-synced. Nothing in the survey combines persona
separation, serverless carry, agent-first architecture, and
library-substrate packaging. That combination, not any single feature, is the
product claim.

### Open questions for the port's own plan

1. **Derived vs stored.** SSH keys re-derive from the seed; TOTP secrets and
   passwords are foreign material that must be *stored and synced*. Sync of
   sealed records rides the codicil/sync-gate seam, which makes the wallet
   fold-in (D4) a prerequisite, not a sibling.
2. Phishing binding: passkeys carry origin binding, TOTP does not; whether the
   browse half shows origin-match warnings is a product decision.
3. Rate/abuse posture on the authority half: per-persona release policies,
   "ask every time" vs session grants (wallet_grant already models this).
4. The name. Tiring-house is floated (where actors don their costumes);
   needs a challenge round.

---

## Part II: the gazette, from resolver to reading room

### The name now works three ways

A *gazetteer* is an index (the 2026-07-08 sense). To be *gazetted* is to be
officially announced and thereby resolvable (the recovered 2026-08-10 sense).
And a *gazette* is the paper you read (the sense this section grows into).
The rename that looked like a concession to crates.io availability is actually
the roadmap.

### Tiering without an auth protocol

The trust-graded reveal ("strangers get my linktree, friends get my
capsules") does not need authenticated resolution. **The persona split is the
tiering mechanism**: each persona has its own handle, you share different
handles with different tiers, and each handle resolves anonymously to exactly
what that persona reveals. WebFinger stays dumb; the gradient falls out of
which handle the other party holds. personae already manages the faces; gaz
already records, per contact, which of *their* handles you hold and what it
resolved to. Revelation accretes into the gaz record with no new machinery.

Honest limit: an unshared handle is a bearer capability guarded by obscurity.
Fine for socials and capsules; wrong for anything sensitive. Genuinely gated
reveal (prove-who-you-are-then-see) is murm/misfin/moot territory and stays
out of gazette.

### The feed pipeline: four existing owners, no new subsystem

gazette **discovers**, nematic **parses**, fetch/eidetic **store**, a product
surface **composes**. Gazette's only growth is one more typed endpoint class
(`feed`) in its JRD/link classification. Everything downstream exists:
nematic already owns RSS/Atom and gemtext; fetch and eidetic already own
retrieval and retention; trail is the natural display substrate.

What the open world actually offers, verified sense of each:

| Source | Open read path | Caveat |
|---|---|---|
| Blogs / podcasts | RSS / Atom / JSON Feed, `rel=alternate` discovery | None. Polling suffices at friend scale; WebSub exists if push ever matters. |
| Gemini | gemfeed convention (Atom companion or index-page format) | None; nematic territory. |
| Fediverse | AP outbox is spec-public (plain GET); Mastodon also serves per-account RSS (`user.rss`), the cheapest read of all | "Authorized fetch" instances require HTTP-Signature-signed GETs. A signed-GET client is small and still needs no inbox; decide whether tier 1 carries it or skips those instances. |
| atproto | Public records via XRPC (`listRecords` on the PDS; AppView feeds currently anonymous) | AppView anonymity is policy, not spec; the PDS path is the durable one. |
| nostr | Open relay reads by pubkey; NIP-05 resolution already on gazette's roadmap | Relay hygiene/dedup is the work. |

Explicitly out: private grants, cross-service posting or interaction, any
delivery/inbox implementation. That is the AP/atproto-scale work Mark named,
and if it ever lands it lands in moot/murm, not here.

### Open questions for the gazette's own plan

1. **Async first.** gazette's blocking reqwest predates any consumer; the
   async port is the entry ticket to all of this.
2. **Who polls.** The resident is the natural poller (it is already the
   always-on party); graphshell clients then read composed state.
3. **Reading identity leaks.** Fetching a friend's feed reveals your interest
   to their host. Per-persona fetch routing (which persona's network face
   does the polling) should be a first-class setting, not an afterthought.
4. **Normalization.** Feed items want one internal shape; engram is the
   candidate, trail the candidate surface. Decide before the parser count
   grows.
5. Whether the "friend gazette" is a surface of trail, a mere-domain crate, or
   a view the graphshell port composes. Product call, not architecture.

---

## What this brief deliberately does not do

No schedules, no crate founding, no name claims. The two fronts convert into
dated plans when picked up, in this order of readiness: wallet fold-in (D4,
prerequisite for stored-credential sync), then the credential port's TOTP +
Secret Service slice, then gazette async + the feed endpoint class. The
WebAuthn provider and anything resembling federation stay research until a
consumer demands them.
