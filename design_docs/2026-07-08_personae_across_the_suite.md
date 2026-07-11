# personae Across the Suite — the identity spine of the Merely apps

**Date:** 2026-07-08
**Status:** vision / architecture brief. Sits above two mere-side docs that hold
the mechanics: the trust-plane plan
(`mere/design_docs/mere_docs/technical_architecture/2026-07-08_signet_trust_plane_plan.md`,
the two seams and the S0–S5 promotion) and the wallet carry layer plan
(`.../implementation_strategy/2026-06-25_persona_wallet_carry_layer_plan.md`,
pairing + encrypt-at-rest detail). Those are mere-flavored; this is the whole
suite: mere, isometry, strophe, woodshed.

## Thesis: the wallet is a server-less account

Every app suite has an account system. The Merely suite's is a **P2P wallet.**
One personae identity, a master Ed25519 seed and the faces derived from it, signs
into all four apps with no login server, offline-first, carried between your own
devices by the wallet's device roster. personae is not persona support built four
times; it is the identity spine of the suite, built once. The pairing ceremony (a
QR plus a short-auth-string comparison) is "sign in on a new device," and it never
contacts a company.

The consequence worth stating plainly: the apps become a **family** rather than
four programs that share crates. The same person moves through all of them under
one carried identity.

## One substrate, N meanings

The crate is identical; each app gives persona its own meaning, the way chartulary
gives one graph N node meanings.

| App | A persona is... | Dominant posture |
|---|---|---|
| mere | a privacy face (work / research / burner) | many faces, unlinkable |
| isometry | a table role (DM face, player face, guest) | portable player identity |
| strophe | a collaborator (named project vs anonymous jam) | shared-session identity |
| woodshed | just you (one guitarist) | one face, many devices |

The main-vs-burner slider is the same everywhere: reputation is the cost of
continuity, anonymity is the cost of starting over. Each app only names the ends.

## Capabilities: the one collaboration primitive

The sharpest reuse. Three collaboration features across three apps are one
mechanism:

- **strophe "pass the mic"**: a grant handing write/record authority.
- **isometry "the DM lets you roll on this table / reveal this map"**: a grant
  scoped to a table resource.
- **mere "join this moot"**: a grant admitting you to a shared space.

They are one thing: a delegable, attenuable, expiring, offline-verifiable grant,
the kith machinery in the wallet. So personae is not only carrying identity, it is
the shared **collaboration engine.** Build the grant system once and every
multiplayer app inherits its sharing model instead of inventing one. Whoever
builds strophe's mic-passing has built isometry's DM grants and mere's moot
admission.

## The two poles the one wallet serves

- **Many personas, unlinkable** (mere): distinct faces, distinct NodeIds,
  deliberately uncorrelated. The privacy story.
- **One persona, many devices** (woodshed): sync your practice history and custom
  exercises across laptop and phone, sealed at rest, no faces at all. The
  continuity story.

Same device roster and carry machinery, opposite posture. isometry and strophe
live in between. A wallet that nails both ends covers the suite; one that only
tells mere's multi-face story leaves woodshed and half the others unserved.

## What every app gets for free

**Encrypt-at-rest.** The `PayloadSealer` seam (landed in mere as wallet gap #2,
promoting to sit over muniment) seals private data under the persona epoch by
`PrivacyClass`, with no per-app crypto. woodshed practice logs, isometry character
sheets, strophe private takes, mere private engrams: all sealed the same way.
Adopt personae plus muniment and at-rest privacy comes with them.

**Standing.** tessera is a fold over a signed event log, so it re-folds on any
device and is portable without a server. Not just moots: woodshed practice
streaks, isometry table reputation, strophe collaboration history are all
standing. Reputation travels with the wallet.

## The cross-app frontier

Because all four apps ride personae (trust plane) and chartulary (data plane),
data can reference across app boundaries under one identity: a woodshed practice
node citing a mere document, a strophe session linking an isometry campaign, each
sealed by the same wallet, each admitted by the same grant machinery. This is the
payoff of the two-plane split and the one thing no single-app design reaches.
Speculative today, but the direction the substrate makes cheap.

## What this means per app

- **mere** is the reference consumer: it already has the persona model, the moots,
  the engrams. It re-bases onto personae last (the wallet carry layer plan's arc);
  its multi-face and moot work is the proving ground for the shared grant and seal
  seams.
- **isometry** is the highest-value *new* consumer: P2P sessions already need
  identity (who is at the table), and DM-grants-to-players is the grant engine's
  first non-mere use. Portable player identity across devices is the concrete win,
  behind isometry's own keystones.
- **strophe** wants the grant engine specifically: "pass the mic" is a capability,
  and shared-session collaboration is grant admission over a codicil log. Its
  layered-track model maps onto per-persona authorship.
- **woodshed** wants the *simplest* slice: one persona, device carry,
  encrypt-at-rest, stemma-backed practice lineage. The cleanest place to prove the
  one-persona-many-devices pole with zero multi-face complexity, and it already
  consumes chartulary (woodshed-graph).

## Open questions

1. **Shared vs per-app persona sets.** Is your "woodshed self" a distinct persona
   from your "mere research face," or a derived face of one root? The wallet holds
   a registry; apps choose which faces they surface. Lean: one root, app-scoped
   derived faces, so cross-app-you is possible but never forced.
2. **Grant vocabulary.** meadowcap now, Biscuit later (the wallet plan's open
   question); it must stay app-neutral enough for mic-passing, DM-grants, and moot
   admission to share it.
3. **Discovery across apps.** One identity reachable in four apps: does presence
   unify or stay per-app? Privacy says per-app by default, opt-in to unify.
4. **The suite's front door.** If the wallet is the account, where does a user
   first mint their personae, and how does app #2 adopt the identity app #1 made?
   A shared onboarding, or each app bootstraps and later pairs.

## Provenance

Grounded in the mere trust-plane plan + wallet carry layer plan, the app memories
(isometry P2P VTT, strophe passing-the-mic cap rule + layered model, woodshed
practice toolkit), and the personae founding (this repo). Written 2026-07-08 as
the suite-level "why" above the mere-flavored mechanics.
