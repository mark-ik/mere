# The Family Composition Thesis: every application its own datalake, one identity across, composable views

**Date:** 2026-08-12
**Kind:** research brief digesting a chat chain (Mark's framing prompt + assistant
response, 2026-08-12); analysis, terminology alignment, and system-shape prior
art added here. Nothing scheduled.
**Anchors:** [application prospects brief](2026-07-24_application_prospects_brief.md)
(the three-seam composition thesis this elevates),
[Graphshell remote projection host plan](mere_docs/implementation_strategy/2026-07-22_graphshell_remote_projection_host_plan.md)
and [reference host plan](mere_docs/implementation_strategy/2026-07-27_graphshell_reference_host_plan.md)
(the rulings this extends),
[credential port + gazette brief](mere_docs/research/2026-08-10_credential_port_gazette_brief.md)
(the castellan split this generalizes),
[persona model brief](mere_docs/research/2026-05-14_persona_model_brief.md),
[participant gate + packs plan](mere_docs/implementation_strategy/2026-07-17_participant_gate_packs_plan.md).

## 1. The origin framing (Mark, 2026-08-12, verbatim)

> i just look at it as, you can access your google drive in a bunch of apps.
> but what if every app is its own repository, its own datalake which can be
> associated with a cross application identity you create and manage, through
> a p2p substrate, network, and system. and then graphshell lets you look at
> and manage 'em, which is a capability that can be composed into other apps
> (like turnstone or signalman). because the ports are embeddable reference
> models of capabilities native to the mere stack.

The Drive analogy is the inversion that positions the family: Drive-like reach
(your things visible from many applications, from many devices) without the
universal repository, the single account namespace, or the landlord cloud. The
projection brief's Graphshell line already said half of this — "like a cloud
service, except the app is the backend, wherever it runs" — for *reachability*.
The chain extends it to the *data model*: there is no central store to reach.
Each application is the store. What crosses applications is identity,
association, and views — never custody.

The response's concise formulation, kept as the family's four-noun shorthand:

> **Personal datalakes, cross-application identity, peer-to-peer association,
> and composable views.**

## 2. The thesis in canonical vocabulary

The chain's nouns land on existing terms; restated so the corpus stays one
vocabulary:

- **"Every app is its own repository/datalake"** → every application owns its
  **source truth** (the 2026-07-22 ruling: "Turnstone, Woodshed, Isometry,
  Hocket, and a radio-management application remain the authorities over
  their native data"). The spatial unit of that truth is **a mere** — whose
  lexicon entry already banks *datalake* as the closest borrowed term. The
  prospects brief's "thin hosts wiring the three seams" and the chain's "rich
  repositories" are the same claim from opposite ends: thin over the
  substrate, rich in domain authority.
- **"Cross-application identity you create and manage"** → **personae** and
  the dramatis tier. The chain's SSO contrast (a Google account gives apps a
  common principal under Google's authority; this gives independently held
  stores recognition under the participant's authority) is the persona model's
  standing claim. Its "prove authorized relationships without learning
  unrelated identity material" is the **emblem** grammar: graded proofs made
  to be shown, where your emblem is what someone else's gaz keeps.
- **"P2p substrate, network, and system"** → stickleback replicated spaces
  under murm/moot domains, retinue's mesh, iroh/p2panda beneath, with the
  standing rule that the substrate never infers authority from transport
  access. The chain's "hosted infrastructure as a convenience without turning
  it into the owner" is the voluntary-hosting posture.
- **"Graphshell lets you look at and manage 'em"** → the ruled port: discover
  granted projection sessions, realize scenes, return typed intents through
  the participant gate.
- **"Projections"** → granted scores and scenes (scenograph); **"intents"** →
  petitions from denizens holding grants. The chain's insistence that a
  projection is purposeful (per context, audience, capability) rather than a
  generic serialization is the score/admission machinery described at product
  altitude.

## 3. What the chain adds to the corpus

Four genuinely new items, one narrative device:

**The port law (the castellan split, generalized).** "The ports are embeddable
reference models of capabilities native to the mere stack." Castellan already
ruled this shape for credentials: an embeddable half any host composes, an
authority half that stays with the resident, custody without ownership. The
chain promotes that from a castellan decision to the *definition of a port*:
the stack owns the capability; the port is its first-party, coherent,
embeddable embodiment; and another application composes the relevant subset
without cloning the reference application or inheriting its authority. The
port answers "what does a complete first-party expression look like"; the
capability answers "what can a host embed without violating authority
boundaries." This prevents the standard platform failure where a capability is
technically reusable but practically trapped inside its first application.

**Graphshell as capability, not only port.** The 07-22/07-27 rulings made
Graphshell a client and reference host — the composition surface *over* the
family. The chain adds the reverse embedding: Turnstone or signalman composing
a Graphshell-powered pane (a Woodshed practice artifact beside a research
page; the field manual beside a radio) while remaining themselves. The
in-process precedent exists — a **swatch** is exactly a scoped embedded
projection — so the new claim is that the same composition works at the
session boundary: a host embeds scene realization + intent return the way it
would embed a swatch, holding a lens, never truth. No new machinery is implied
beyond the existing planes; what is new is naming it a first-class arrangement
rather than a degenerate case.

**The anti-shell test.** If Graphshell's capability only works with Graphshell
as the outermost application, the family shipped a shell with plugins. If the
same capability composes into a second host that keeps its own identity,
workflow, and authority, it is a platform capability. This is the product-tier
form of the discipline the engine work already follows ("the framing pays rent
only when a pipeline end has more than one occupant" — proof 4). Worth
adopting as a named gate when Graphshell embedding is ever scheduled: the
receipt is a *second host*, not a second view.

**The narrowing gradient.** Possession ⊇ disclosure ⊇ synchronization ⊇
projection: what an application holds, what a grant admits, what a replicated
space carries, what a score selects — each strictly narrower, each owned by an
existing mechanism (vault/journal; grant/admission boundary; stickleback space
scope; score). Stated this compactly it is testable: any place a projection
can exceed its sync scope, or a sync exceed its disclosure, is a bug class
with a name.

**The release narrative (device, not schedule).** Woodshed proves an
application is worth using alone before any composition ships; Personae +
Castellan prove identity crosses applications through both headless and
graphical ports; Graphshell proves discovery and composition; a second host
embedding the capability passes the anti-shell test; Knot proves the
application model isn't secretly one product's shape. This is a *story
ordering*, not the build order — the corpus already interleaves (Knot's port
is furthest along; castellan's OTP core landed first) — but it is the right
order for what each release must *demonstrate*, and it explains why Woodshed
standing alone is a feature of the plan rather than a delay.

## 4. Corrections and cautions (terminology alignment)

- The chain's diagram labels a "Castellan repository … identity state,
  identities, grants, proofs." Castellan owns no repository: identity truth
  is personae's (dramatis tier), and castellan is the keeper — custody
  without ownership, browse-and-consent surfaces over material it does not
  hold. The port law survives the correction; the box label does not.
- **"Repository" should not be minted as vocabulary.** The corpus already
  uses *repository* for git repositories, with a standing rule that
  repository boundaries are packaging, never authority. The chain's sense is
  covered by **source truth** (the authority) and **a mere / datalake** (the
  dataspace). Three senses on one noun would be a lexicon regression.
- Signalman is retinue's application (`apps/signalman-desktop`, founded
  [2026-08-06](https://github.com/merely-made/retinue/blob/main/design_docs/2026-08-06_signalman_founding.md)),
  whose founding already states this brief's authority split from its side:
  graphshell surfaces meres rather than owning radios. Its postilion base
  (station logic with no UI) is what would make an embedded cross-application
  pane a face among faces rather than a bolt-on.
- The chain describes application repositories as "heterogeneous… no giant
  universal schema." True and already doctrine (schema at the engram
  boundary; no shared core, the doctrine is the unit of reuse) — recorded
  here so the thesis is not read as a universal-schema proposal by a fresh
  reader.

## 5. System-shape prior art

The adjacent altitudes are already surveyed: protocol adoption in the
[p2p landscape brief](mere_docs/research/2026-05-31_murm_p2p_landscape_brief.md),
the projection engine in the
[prior-art brief](mere_docs/research/2026-07-21_projection_engine_prior_art_brief.md),
feature borrows in the
[borrowed ideas brief](mere_docs/research/2026-06-25_borrowed_ideas_brief.md).
Not yet surveyed is the *decomposition* altitude the chain speaks at: who owns
the data, and how applications compose over it. One lesson each.

| System | Decomposition | Lesson for this thesis |
|---|---|---|
| **Solid** (pods) | One user-owned pod; apps are views requesting access | The inverse split. A universal personal store pushes cross-app schema agreement onto every app pair, and the app ecosystem never came. Mere's per-app authority avoids the shared-schema tax; the projection contract, not a shared store, is the interop surface. |
| **AT Protocol** (PDS repos) | One signed repo per *user*; apps are lexicon-namespaced collections inside it; portable DID identity | The nearest modern analog, mirrored: atproto = one datalake per person, apps as namespaces; mere = one datalake per app, personae across. Lexicons are its projection-contract analog. Validates identity-crosses-apps; hosting stays server-shaped. |
| **Sandstorm** (grains + Powerbox) | Per-instance app data ownership; user-mediated capability picker brokers all cross-app access | Per-app ownership plus consent-brokered composition worked; packaging a whole SPA per grain made composition heavy. The Powerbox is the ancestor of "only the resident asks for consent" (castellan's anti-spoofing rule). Keep the embeddable unit a *scene*, not an app. |
| **Plan 9** | Every program a file server; one tiny protocol (9P); per-process namespace composition | The cleanest ancestor of "every app exposes projections over a common contract." Composition was free because the contract was minimal and uniform. Sceno scenes + typed intents are the family's 9P; contract growth is the thing to resist. |
| **Holochain** (hApps) | Agent-centric: each app its own p2p network, per-agent source chains, app-defined validation | Literally "every app is its own repository through a p2p substrate" — and per-app sovereignty siloed, because no family-level composition surface emerged. Graphshell-as-capability is the piece whose absence kept that ecosystem fragmented. |
| **OpenDoc / OLE** | Embeddable component parts inside host documents | The embeddable-capability dream, dead of contract breadth, resource weight, and business model. Survival trait the family already holds: one narrow scene contract instead of arbitrary embedded runtimes. |
| **remoteStorage / unhosted** | Per-user storage server; per-app scoped directories; app-agnostic protocol | Bring-your-own-storage failed on adoption friction: the storage had to exist before any app was useful. The release narrative's Woodshed-first rule is the counter: each application must be worth using before the substrate is asked for. |

On novelty, the same verdict as the projection survey: every element has
precedent; no surveyed system combines per-application authority,
participant-held cross-application identity, p2p association without a
landlord, and an embeddable composition capability. The combination, not any
row, is the claim — and Holochain's row is the warning about shipping the
first three without the fourth.

## 6. Sidequests surfaced (each needs its own pass before code)

1. **Name the embedded-Graphshell seam.** A swatch at the session boundary:
   does it need a term and a plane-level contract of its own, or is it
   strictly a client-library packaging question? (Cheap to answer; do it when
   a second host first wants a pane.)
2. **Adopt the anti-shell test as a G-series gate.** A "G-embed" receipt —
   the capability composed into Turnstone or signalman-desktop — belongs in
   the Graphshell plan queue the day embedding is scheduled, with the second
   host, not Graphshell, naming the done condition.
3. **The gradient as invariants.** Possession ⊇ disclosure ⊇ sync ⊇
   projection could become admission-boundary assertions (a score cannot
   select what the space does not carry; a space cannot carry what the grant
   does not admit). Worth a testing-category pass in graphshell_docs when the
   planes next move.
4. **Per-persona continuity vs correlation.** The chain's "continuity without
   homogenization" (same persona across apps by intent, different personae
   where separation matters, no app learning unrelated identity material) is
   personae doctrine; what lacks a written home is the *cross-application
   correlation* threat model — which compositions let a host join two
   personae it was shown separately. Dramatis-tier research brief candidate.

## What this brief deliberately does not do

No schedules, no crate founding, no name claims, no new machinery. It records
the chain, aligns its vocabulary, extracts the port law / capability-embedding
/ anti-shell / gradient deltas, and files the decomposition-altitude prior
art beside its three sibling surveys. The sidequests convert into dated plans
only when their consumers arrive.
