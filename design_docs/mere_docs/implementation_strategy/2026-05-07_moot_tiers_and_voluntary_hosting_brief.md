# Moot Tiers and Voluntary Hosting Brief

**Date**: 2026-05-07
**Status**: Proposal (architectural commitment under review)
**Scope**: Captures today's architectural sharpening on moot semantics, the
four-tier scale, voluntary hosting with reputational stakes, cheesecloth
pinning, ILL-shaped reciprocity at federation tiers, and the
demesne→moothold→demesne lexicon shift (t4 "demesne" since renamed "coalition",
see §1). Updates the
[2026-05-07 event-DAG substrate brief](2026-05-07_event_dag_substrate_brief.md)
§7 and the moot-related lexicon entries.
**Related**:

- [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md) — earlier brief from today; this one supersedes its §7 framing of moot hosting and refines §8.7 (personas) and §8.9 (spam resistance).
- [`2026-05-05_protocol_architecture_plan.md`](2026-05-05_protocol_architecture_plan.md) — original protocol architecture plan; §5 protocol-mod framing is now interpreted through the tier and hosting-model lens captured here.
- [`../../moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md`](../../moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md) — the Pattern A / Pattern B distinction; reframed as routing-only-clients (Pattern A becomes "the moot links to the resource and members open it via a thin client") and outbound bridges (Pattern B; one-way publishing).
- [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md) — current lexicon; needs the coalition / moothold rename pass after this brief lands.

---

## 0. The architectural sharpening this brief captures

Today's protocol-architecture conversation moved through three sharpening
passes:

1. **Drop Cable, unify on BLAKE3, MereEvent DAG as the canonical wire** (the
   substrate brief's §0–§6 — still authoritative).
2. **Multi-protocol moot hosting as bidirectional adapters** (the substrate
   brief's §7 first revision — partially correct).
3. **Moots are graph views that link to and store, not translate** — and
   moot scale is tiered (t1 personal → t2 community → t3 federation →
   t4 coalition), with voluntary hosting and reputational stakes
   throughout. **This brief.**

The third pass changes what `moothold` and `mooting` are *for*. They are
not protocol-translation layers. They are coordination, governance, and
routing layers over a graph view that *links to* and *stores* community
resources without trying to absorb every protocol's behavior.

---

## 1. The lexicon shift: demesne → moothold → (new) demesne (t4 since renamed coalition)

> **Rename note (2026-06-04):** the tier this brief named **demesne** (t4) was
> later renamed to **coalition** for familiarity, because "demesne" is pronounced
> like "domain" and "domain" already names Mere's domain layer. Everywhere outside
> this naming-history section, t4 is **coalition**. The original demesne rationale
> is kept below as the record of the 2026-05-07 decision.

Effective from this brief (with t4's name updated to coalition per the note above):

| Term (was) | Term (now) | Meaning |
|---|---|---|
| moothold | moothold | Tier 3 — *a holding of moots*. A federation of moots. (Was: umbrella term for any moot-related structure.) |
| demesne | moothold | (At t3.) The old umbrella/federation term *demesne* is no longer the federation tier. |
| — | coalition (orig. *demesne*) | Tier 4 — *a sovereign cluster of mootholds*. Organizes member mootholds with optional shared defaults, per-moothold override, and clean fork-out. |
| moot | moot | Tier 2 — a single persistent themed federatable graph-view community. Unchanged. |
| orrery | orrery | Tier 1 — a single user's root graph view. *"Your orrery is your moot."* The personal moot. |

**Why the moothold rename is right:** *household*, *stronghold*, *freehold* are all
**holdings of bounded units**. A federation of moots literally IS a
holding-of-moots. The previous use of *moothold* as an umbrella term lost
the etymological precision; restoring it to the federation tier matches the
Anglo-Saxon sense exactly.

**Why t4 was first named *demesne*, and why it is now *coalition*:** the original
Latin sense of *demesne* ("a lord's directly-controlled lands, distinct from tenant
holdings") carried the sovereignty-over-multiple-holdings connotation t4 needs. But
*demesne* is obscure on the page and sounds like *domain* when spoken, and *domain*
already names Mere's domain layer, so t4 was renamed **coalition** (2026-06-04): a
plain, familiar word that says exactly what t4 is, a coalition of mootholds that
organizes them without collapsing them into one.

**Forkability throughout:** all structures (moot, moothold, coalition) are
forkable. A fork is a first-class operation, not a disaster recovery. A
moot can fork over a governance disagreement; members of the fork commit
their own pins; the original keeps going. Mature federations fork well.

---

## 2. Moots are graph views, not protocol containers

A moot is a **persistent shared graph view** of a community's resources.
It contains:

- **Members + capabilities** — who's in, what scope they have.
- **Pinned content** — engrams, archives, page snapshots, files
  (content-addressed via BLAKE3, replicated via iroh-blobs).
- **Resource pointers** — links to Matrix rooms, IRC channels, Nostr
  feeds, Gemini capsules, websites, anything addressable. Each is a graph
  node with a URL and a protocol tag.
- **Hosting commitments** — signed events naming who's running which
  service, with heartbeats and successor chains.
- **Engram fauna** — durable contributions accumulated over time.
- **Governance** — tessera-weighted decisions about resource use,
  capability assignment, scope changes (not about hosting *burdens*; see
  §4).

**Critical:** the moot does *not* try to absorb foreign protocols into a
unified internal abstraction. A Matrix room linked from a moot stays a
Matrix room; a Gemini capsule stays a Gemini capsule; an IRC channel stays
IRC. The moot's job is to **link to** these, **store** what passed through
them (when it cares to), and **route** members to them.

This means `mooting-*` adapter crates are **thin protocol clients** — they
help members open / consume / publish to foreign resources linked from the
moot — not translators. `mooting-mere` is the canonical: it handles the
moot's *internal* coordination (governance, capability evaluation,
hosting negotiation, member chat) over MereEvents. Other adapters
(`mooting-matrix`, `mooting-irc`, etc.) provide client orchestration for
their respective protocols.

---

## 3. The tier framework

### Tier 1: Personal node (orrery)

Your browser eidetically stores your own graph, annotations,
browsing-derived embeddings, cached pages, and local blobs. You can
publicly address individual nodes (a webpage, a clip, an annotation) or
the entire graph for the public, kith, kin, or specific people, in a
bilateral fashion, and end the hosting whenever you want.

**Your orrery is your moot.** Tier 1 is just a moot-of-one — same
primitives, no governance machinery needed.

### Tier 2: Moot (storage pool, voluntary)

A neighborhood, research group, fandom, union, school, or friend group
runs a moot. One individual bootstraps and pins; others may pin (if they
have the capability) or visit (if they don't).

The moot **dissolves if nobody pins it** — liveness is determined by
whether any member cares enough to maintain a pin.

Within the moot, members make and pin: shared graph spaces, public
annotations, recommended links, indexes, page snapshots, fauna,
cross-moot requests (compute, federation reach, etc.).

Resources can be publicly accessible — a friend group can host a curated
collection of Gemini capsules and share it without doxxing the host.

Governance is human, with tessera-weighted voting for resource
apportionment and capability assignment.

**Practical scale ceiling: 5K–10K active members.** Above that,
fragmentation into a moothold of related moots is healthier than a single
mega-moot. Anchors:

- Dunbar layers (~150 / ~500 / ~1500).
- Mastodon instances (1K–50K typical, strain >10K, admin burnout >25K).
- Discord servers (theoretical 250K, active community feel ~5K–10K).
- Subreddits (function up to ~100K but governance is lossy).

Mere's moots have richer governance (tessera, capabilities, persistent
graph view) so might scale slightly higher, but not by orders of magnitude.

### Tier 3: Moothold (federation of moots, voluntary storage pool)

Trusted hubs (member moots) replicate each other's selected data. Credits
are tracked, but **are not cash and depend on rep**. The model is **library
interlibrary loan (ILL)**:

- **Capacity-based, not cash-based.** Big moots contribute more; small
  moots contribute what they can; everyone benefits from the network.
- **Reciprocity is reputational, not monetary.** Bad-actor moots (lose
  pinned content, fake credits) get sanctioned by losing privileges, not
  fined.
- **Identity is institutional.** A moot's standing in the moothold isn't
  its founder's clout; it's the moot's track record over time.
- **Resists cash exploitation.** You can't deposit money to make the
  network preserve your stuff; you have to be a participating institution
  with actual capacity.

A moothold is a deliberate institution: member moots sign federation
commitments, pledge storage / compute / engram replication shares, and
gain reciprocity credits proportional to contribution.

**Validation:** a moothold's institutional standing is initially validated
by **member moots' resource apportionment** — the act of pledging is the
justification. Later, **vouching from other mootholds** reinforces. The
form is **stake + agreement**: members of a forming moothold pay a real
infrastructure cost (their old laptop running a mooting server, their
bandwidth, their pinned storage allocation) and mutually agree (signed
events) to establish the federation.

### Tier 4: Coalition (coalition of mootholds, organizing default)

A coalition organizes member mootholds. It defines shared defaults that
member mootholds adopt unless they override; per-moothold override is
always possible; **forking out of the coalition is always possible**.

Being in a t4 changes the character of the lower tiers — a moothold
inside a coalition can compose with sibling mootholds more easily, share
defaults (governance templates, capability schemas, archive policies),
and benefit from coalition-wide reciprocity ledgers.

Without a t4 around it, a moothold is fully sovereign; with a t4, it's
sovereign-but-cooperating.

T4 is a *future* tier — t1–t3 are sufficient for personal, communal, and
broad-topical-federation scales. T4 enables civilizational-scale
composability if and when it's needed.

---

## 4. Voluntary hosting + reputational stakes

The most important architectural commitment in this brief: **no hosting
is ever imposed on anyone.** Tessera-weighted voting governs how the
community uses its resources or apportions capabilities. It does NOT
distribute burdens. Burdens are picked up voluntarily.

Honey: you accrue reputation (tessera) by following through on
commitments — contributing storage, pinning durably, hosting reliably,
participating in governance.

Stick: you lose reputation when you commit publicly and fail to follow
through. The cost is *visible-failure-to-follow-through*, not coercion.
Saying no costs nothing.

Mechanics that make the stick real:

- **Public commitments.** Hosting commitments are signed MereEvents in
  the moot's graph view. Verifiable by any member.
- **Heartbeat-shaped commitments.** Long-running commitments lapse unless
  renewed. "I'm still hosting" is an active assertion. Avoids zombie
  commitments where someone disappeared but the graph still says they're
  hosting.
- **Clean handoff protocols.** You can transfer a commitment to a willing
  successor without rep penalty. Walk away cleanly = small or no rep hit;
  ghost = real rep hit.
- **Tessera-gated capability scopes.** What you can pin (or host) is
  capped by structural scope plus policy authorization. Structural caps
  say which cluster/path a member may touch; policy facts say whether
  current tessera, quota, heartbeat, role, and handoff state permit the
  action. Caps prevent rug-pulls (someone with high rep committing to
  host everything, then walking away with no fallback).

---

## 5. Cheesecloth pinning + emergent uptime

For static content-addressed material, **liveness emerges from overlapping
individually-unreliable contributions.** Many casual pinners with partial
uptime → high overall availability without anyone being on the hook 24/7.
Tight weaves fail catastrophically when one thread snaps; cheesecloth
retains the essence even when individual threads break.

This enables **casual hosting** — your laptop becomes your contribution.
You don't have to set up server infrastructure or commit to 24/7 uptime.
Your role is "I pin some bytes from this moot when my laptop is on,"
which is enough.

**Caveat: cheesecloth + live services.** The pattern works perfectly for
static / archived content (any pinner serves any blob). It works *less*
well for live services (Matrix homeserver, IRC daemon, real-time chat)
which need at least one continuous host with graceful failover. So
tiers 2–3 will run **hybrid**:

- Cheesecloth for archives, engrams, snapshots, page caches.
- Leadership commitments for live services (with named successors).

Both shapes are voluntary; they just have different commitment cadences.

**The natural-selection mechanism:** if nobody pins something, it's gone
— from the moot's perspective. Stuff that matters gets pinned by multiple
members. Stuff nobody cares about lapses.

---

## 6. Lapse and revive as a normal life cycle

The web has trained us to think "if it's offline, it's gone forever." For
mere's design, that intuition is wrong:

- **eidetic preserves local copies.** Members who care still have records.
- **Content-addressed pinning means anyone can re-host with the same hash.**
  A moot can be put back up at a different address, at a different time.
- **Schedules are valid availability.** "8 AM-6 PM EST" is fine, as long
  as members know. The moot can publish its schedule (or members cache
  copies for off-hours).
- **Resurrection is normal.** A community lapse for a season and revive a
  year later, with the original engrams intact (because someone preserved
  them locally or via coalition reciprocity).

Whether something is currently online is **not** its total existence.
Communities have rhythms. Mere's design respects them.

---

## 7. Stake + agreement: the universal coordination protocol

At every tier transition (member → moot, moot → moothold, moothold →
coalition), the coordination pattern is the same:

1. **Stake.** Participants pay a real infrastructure cost — bytes pinned,
   storage allocated, server hours pledged, bandwidth committed. *Real*
   stakes, not abstract tokens.
2. **Agreement.** Participants sign a mutual commitment event ("I, alice,
   am pledging X storage / Y bandwidth / Z hours to this structure for
   duration D"). Signed MereEvents in the parent structure's graph.
3. **Instantiation.** Once the agreed stakes are met, the structure is
   instantiated. Content addressed at instantiation goes live; reserved
   empty storage is allocated for future content.
4. **Maintenance.** Heartbeats keep stakes active; lapsed stakes
   automatically retire from the structure.

If stakes lapse and no one steps in, the affected portion of the
structure goes offline. The structure as a whole continues if other
stakes are still met.

**Bootstrap is not the issue.** What is an issue is **reputation for new
participants** — a fresh person with no track record can't take on
high-stakes commitments. This resolves over time via:

- **Time and proven reliability** — small commitments fulfilled build
  tessera.
- **Settled expectations** — the moot adapts its capability scopes as
  members demonstrate reliability.
- **Kith/kin vouching** — established members can vouch for newcomers,
  transferring a fraction of their own tessera to accelerate the
  newcomer's meaningful participation. (Connects to the substrate
  brief's persona-id-chain insight: tessera is owned by the chain root,
  and vouching delegates a slice.)

---

## 8. Per-tier parameters as settings, not hardcodes

Heartbeat cadence, capability decay rate, pinning quotas per tessera
tier, vote thresholds, reciprocity ratios, archive retention rules — all
**per-tier configurable** with sensible defaults. Different communities
have different rhythms; the protocol provides defaults, the community
decides.

This applies at every tier:

- A friend-group moot might use weekly heartbeats and casual quotas.
- A research-group moot might use daily heartbeats and strict quotas
  on archive retention.
- A moothold might use monthly heartbeats for federation pledges.
- A coalition might use quarterly review cadences for member-moothold
  standing.

Defaults ship with the protocol; configuration ships with each
structure's constitution.

---

## 9. Provenance is a social problem (and that's OK)

Strong technical ID for content (BLAKE3 hash = unforgeable identity for
byte-exact content) plus social citation chains (signed claims of
derivation, vouching, network observations of first-seen timestamps) gets
mere most of what's useful for provenance.

The remaining gap (deliberate fabrications, edited derivatives that
strip prior signatures, watermark-stripped media) is a recurring social
challenge in *any* system. It's not specific to mere; it's a property of
the world. Mere's contribution is making **first-seen aggregation across
mootholds** a tessera-aware protocol surface, so a content-aware
federation can corroborate provenance claims meaningfully even when it
can't prove them absolutely.

**Deduplication semantics** for the credit ledger:

- Same content address (BLAKE3 hash) pinned in two moots → both moots'
  pinning credit goes to the original content host's tessera record. iroh-
  blobs already deduplicates the bytes; the credit attribution is the
  protocol-level convention.
- Same content, two different addresses (e.g., re-encoded media) → the
  network attempts to detect and deduplicate via similarity heuristics; if
  detected, credit goes to the **first submitter** the network has on
  record.
- If the network can later learn that content X was actually the original
  of content Y from another moothold's records (provenance inference),
  the credit can retroactively attribute. This is a "real value" feature
  of tessera-aware federations.

---

## 10. What this changes about `moothold` and `mooting`

Smaller, cleaner responsibilities than the protocol-translation framing.

**`moothold` (the crate) owns at all tiers:**

- Graph view CRDT (the persistent shared graph).
- Pin-tracking ledger (signed commitments + heartbeats, per-member).
- Tessera ledger (reputation per chain root, depreciating across persona
  forks per the substrate brief §8.7).
- Capability stack: structural namespace caps
  (meadowcap / meadowcap-shaped), moot policy authorization
  (Biscuit candidate), and the group/key-state integration seam
  (Keyhive eval), per the substrate brief §8.8.
- Foreign-resource node type (URL + protocol tag + optional
  content-addressed archive).
- Reciprocity ledger (at t3 and above; ILL-shaped credits between
  member structures).
- Tier transitions (moot → moothold; moothold → coalition).
- Forking primitives (clean fork at any tier).

**`mooting` (the crate) owns:**

- The `MootProtocol` trait — small surface for moot-internal
  coordination (governance events, capability evaluation, hosting
  negotiation over MereEvents).
- Dispatcher to per-protocol client adapters when members open foreign
  resources linked from the moot graph.

**`mooting-*` sibling adapter crates** are **thin protocol clients**, not
translators. Each provides:

- Open / consume operations: "given a graph node tagged `matrix-room`,
  route the member to the right client."
- Host operations (when a member is committed): "I'm running this
  moot's Matrix homeserver; expose status / failover handoff /
  resource-use telemetry to the moot's governance layer."

**`mooting-mere`** is the canonical: handles internal moot coordination
over MereEvent streams + iroh.

**`mere-bridge-*` crates** remain separate from `mooting-*` adapters:
they're outbound-only publishers to systems that can't host moot
semantics or where mere only wants to *publish*, not *interact*.

---

## 11. What this changes about earlier docs (touch-up list)

This brief introduces a lexicon shift and architectural reframe that
ripples through previously-written docs. Touch-ups required (deferred
to a follow-up pass; not done in this brief):

- **`design_docs/2026-05-04_lexicon_brief.md`** — update *moothold* /
  *coalition* entries to match this brief's tier framework.
- **`design_docs/DOC_README.md`** — update working-principles vocabulary
  line.
- **`design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md`**
  — §7 (multi-protocol moot hosting) needs a "see also: 2026-05-07 moot
  tiers brief, which reframes this section" pointer; §8.7 needs a
  cross-reference to the tier framework's bootstrapping section.
- **`design_docs/mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`**
  — §5 protocol-mod framing reinterpreted through the new tier and
  hosting model.
- **`design_docs/moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md`**
  — Pattern A reframed as "thin client routing"; Pattern B reframed as
  "outbound bridge."
- **Crate READMEs** — `crates/meerkat`, `crates/moot/moothold`,
  `crates/moot/mooting`, `crates/persona/identity`, `crates/murm/transport`,
  `crates/murm/murm`, `crates/murm/murmuring` — touch up *coalition* and
  *moothold* references and the moot-as-graph-view framing.
- **Memory** (`project_naming_state.md`, `memory/MEMORY.md` index) —
  update lexicon entries to match.

---

## 12. Open questions

- **T4 coalition crate split.** Does t4 coalition functionality live as a
  module inside `moothold`, or as a separate `coalition` crate? Probably
  defer until t3 lands; the answer falls out of how complex coalition
  governance turns out to be.
- **Bootstrap-by-vouching mechanics.** What fraction of voucher tessera
  transfers? How long does a vouched-tessera boost last? Per-moot
  configurable, with defaults — but the defaults need design.
- **First-seen aggregation protocol.** How do mootholds publish
  first-seen content claims, and how does the network corroborate?
  Probably a federation-level event type with tessera-weighted
  attestations.
- **Live-service hosting handoff protocol.** When a leadership-style
  hosting commitment lapses and the named successor takes over, what's
  the handoff sequence? Probably involves DNS-style cutover for
  externally-facing services.
- **Fork operation semantics.** When a moot forks, do member tesseras
  carry over to the fork? Both? At what ratio? Probably configurable;
  defaults need design.
- **T2 → T3 graduation criteria.** What signals "this is now a
  moothold"? Member-moot count, resource pledges, governance maturity?
  Probably descriptive, not prescriptive — a structure becomes a
  moothold when its members federate explicitly.

---

## 13. First milestone (extending the substrate brief's milestone)

The substrate brief proposed an 8-step prototype that demonstrates the
event-DAG substrate end-to-end. This brief extends it with tier-specific
checkpoints:

**Tier 1 milestone** (smallest):

1. A user's orrery is instantiated as a one-member moot-of-self.
2. They pin a graph snapshot to their local eidetic.
3. They publish a node publicly (e.g., bilateral share with kith).

**Tier 2 milestone**:

4. Two users form a moot. Both pin the moot's seed graph. Both sign a
   stake + agreement event.
5. One user authors an engram into the moot's fauna; the other pins it.
6. A signed hosting commitment for a live service (a Matrix-room-link
   node) is recorded with a heartbeat.

**Tier 3 milestone**:

7. Two moots federate into a moothold. Each pledges a reciprocity share.
8. Cross-moot pin requests succeed via the reciprocity ledger.

T4 milestone deferred until t3 is solid.

---

## Findings

### 2026-05-07 — moot tiers and voluntary hosting

Today's conversation established three load-bearing commitments that
sharpen the protocol architecture beyond the substrate brief:

1. **Moots are graph views that link to and store**, not protocol
   translators. Foreign protocols stay themselves; the moot links to and
   optionally archives.
2. **Voluntary commitment + reputational stakes** — hosting is never
   imposed; tessera rewards good follow-through and penalizes ghosting,
   never coerces participation.
3. **Tier framework (t1 personal → t2 moot → t3 moothold → t4 coalition)**
   with a clean lexicon shift restoring etymological precision.

Cheesecloth pinning, ILL-shaped reciprocity, lapse-and-revive as a
normal life cycle, and stake+agreement as the universal coordination
protocol all fall out cleanly from these commitments.

The framing also resolves a long-standing tension: how does mere's
federation scale without becoming a Mastodon-style admin-burnout
catastrophe or a crypto-economic plutocracy? Answer: voluntary capacity
contribution + reputational reciprocity + cheesecloth durability +
tier transitions for scale.

---

## Progress

### 2026-05-07

- Brief drafted to consolidate today's architectural conversation on
  moot tiers, voluntary hosting, and the coalition / moothold lexicon
  shift.
- Touch-up list (§11) enumerates the docs that need updating to match;
  follow-up pass deferred.
- First milestone (§13) extends the substrate brief's 8-step prototype
  with tier-specific checkpoints.
- Open questions (§12) flag follow-up design work for bootstrap-by-vouching,
  first-seen aggregation, live-service handoff, fork semantics, and
  graduation criteria.
