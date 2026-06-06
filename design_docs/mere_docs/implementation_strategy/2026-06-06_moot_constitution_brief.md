# Moot Constitution: the governance DNA

**Date**: 2026-06-06
**Status**: Proposal (design probe)
**Scope**: Defines the **constitution** primitive: the per-moot ruleset that says
how capabilities are granted and how reputation gates them, plus the rule for
changing the ruleset itself. Consolidates the scattered "the moot's constitution
declares X" references across the existing briefs into one layer model, names the
constitution's place in the §8.8 capability stack (the amendable owner of the
*policy-authorization* layer), and proposes a minimal first build that mirrors
tessera's event-sourced fold. The governance organ identified as the keystone in
the moot-synthesis discussion: today a moot can *remember and rank* (tessera), and
has a reference gate (`tessera::gate::authorize`), but cannot yet *durably decide*
or amend its own law.
**Grounded in**: a read of the live tree (2026-06-06) confirming no constitution /
amendment / role-bundle / cap-grant primitive exists in code; tessera is the sole
built coordination layer (`Ledger::scores`, `composite_score`) and already includes
a configurable preset authorizer (`tessera::gate::Policy` +
`tessera::gate::authorize`) that the constitution must own rather than duplicate;
meadowcap-shaped structural caps exist only as a proof/probe
(`crates/probes/willow-cluster-cap`).
**Related**:

- [`2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](2026-05-07_moot_tiers_and_voluntary_hosting_brief.md)
  — moot state already lists "Members + capabilities" and "Governance"
  (tessera-weighted decisions about capability assignment / scope changes); t1
  orrery is "a moot-of-one, no governance machinery needed"; forking over a
  governance disagreement is first-class.
- [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md)
  — every `MereEvent` carries a `capabilities` field; §8.8 corrects the capability
  layer into a **stack** (structural namespace caps Meadowcap-shaped → moothold
  *policy authorization* facts + preset authorizer today / Biscuit candidate later
  → live group/key state, now p2panda-encryption as the leading fit). This brief
  defines the durable, amendable owner of that middle layer.
- [`2026-05-10_graph_cluster_namespaces_brief.md`](2026-05-10_graph_cluster_namespaces_brief.md)
  — "each space's **constitution** declares an algorithm + parameters + RNG seed"
  for canonical clustering; "switching algorithms requires a **constitutional
  amendment**." The first concrete clause anyone wrote for the constitution.
- [`../research/2026-05-14_capability_gate_catalogue_brief.md`](../research/2026-05-14_capability_gate_catalogue_brief.md)
  — the **local / host** policy chain (action → session → persona → app,
  first-match wins). Distinct from this brief (see §6): that gate enforces *one
  user's* policy on *their* device; the constitution is *a community's* shared
  policy over a shared log.
- [`../research/2026-05-14_persona_model_brief.md`](../research/2026-05-14_persona_model_brief.md)
  — personas are the actors a constitution authorizes; the vault holds their keys.
- [`../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md`](../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md)
  — the reputation layer the constitution consumes, the existing preset authorizer
  the constitution should wrap / own, and the event-sourced fold (`from_events` /
  `apply` / `scores`) this brief proposes to mirror.
- [`../research/2026-06-04_resource_coordination_brief.md`](../research/2026-06-04_resource_coordination_brief.md)
  and [`../research/2026-05-10_geist_models_brief.md`](../research/2026-05-10_geist_models_brief.md)
  — two of the briefs that defer load-bearing rules ("the moot's constitution
  declares...") to a primitive that did not exist. This is that primitive, with the
  caveat that fast-changing operational knobs remain governed configs/modules, not
  constitutional law by default.

---

## 0. Why this brief exists

"Constitution" felt like fog because **three different things were wearing the
one word**, and the instinct was to start with the wrong one (the crunchy crypto
of capabilities). The references are already scattered across four briefs but were
never consolidated, and no constitutional amendment primitive exists in code. The
closest existing code is tessera's reference policy gate; that is a seed to own, not
a substitute for a shared amendable law. This brief draws the layer boundaries,
names the thesis (the amendment rule is the DNA), places the constitution in the
existing §8.8 stack, and proposes a first build small enough to land beside tessera.

The motivating observation from the moot synthesis: every recent brief bottoms out
in the same sentence. Geist training consent, trainer roles, adapter adoption,
resource offer policy, tessera thresholds, base-model choice, the clustering
algorithm, the fork rule. All say *"the moot's constitution declares X."* The
constitution is the single highest-leverage unbuilt thing, because building it
turns a half-dozen deferrals into "evaluate against the constitution."

---

## 1. The three layers (the disambiguation)

| Layer | Plain reading | Status in tree |
| --- | --- | --- |
| **Capability** (meadowcap-shaped) | Authority you were *granted*: a delegable, attenuable, offline-verifiable token, "key K may write scope S" (scope = `(SpaceId, grantee, cluster-path)`), carried in each event's `capabilities` field. | Probe / proof only (`willow-cluster-cap`). |
| **Reputation** (tessera) | Standing you *earned*: a log-derived score per `(Scope, persona)`. **Not** authority. An *input*. | Built (`Ledger`, `scores`, `composite_score`), with a non-amendable preset gate (`tessera::gate`). |
| **Constitution** | The law that says how grants get minted and how standing gates them, **plus the rule for changing the law**. | Referenced in briefs; amendment log / durable policy owner built nowhere. |

The line that unlocks it: **meadowcap and tessera are instruments; the
constitution is the score that tells them when to play.** A capability is a key
you hold. Reputation is a number observed about you. The constitution is the rule
that says "to do X you need *this* key and/or *this* much standing and/or *this*
quorum, and here is how that rule is amended."

Conflating them is what stalls the start. Capabilities are *authorization
mechanism*. Reputation is *an input to a rule*. The constitution is *the rule plus
its amendment procedure*. Build the rule layer first (§5), because the other two
are either an input it reads (tessera) or a mechanism it invokes (caps).

---

## 2. The amendment rule is the DNA

The deepest clause of a constitution is not its permissions list. It is **its own
amendment procedure.** A moot is precisely a body that can change its own rules by
a defined process (the *gemōt*, the assembly that decides). That self-amendment is
what makes a moot a polity rather than a chat room.

Consequence: the constitution is not a static config blob. It is a small
**append-only amendment log** (`genesis → amend → amend`), each entry signed and
content-addressed, chained by prev-hash. The "current rules" are the *fold* of that
log. This is the same shape as tessera (§7), which is why the first build is cheap.

---

## 3. One primitive, every tier

The amendment rule is also what unifies the tiers. orrery / moot / moothold /
coalition are not different governance machines. They are **one constitution
primitive with different amendment clauses:**

- **orrery (moot-of-one):** amendment rule = `FounderSigned`. The tiers brief
  already says t1 needs "no governance machinery"; that degenerate clause *is* the
  no-machinery case.
- **moot:** amendment rule = a tessera-weighted quorum of members.
- **moothold:** amendment rule = a quorum of *member moots*.
- **coalition:** amendment rule = a quorum of *member mootholds*.

Same type, different amendment clause, nested. This is the recursive-cell shape
(the moot as the fundamental unit, each tier a moot whose members are moots) made
concrete in one field. It is also why the primitive lives in `moothold` (§7): the
holder is the recursive container.

---

## 4. Where it sits in the §8.8 capability stack

The substrate brief §8.8 already corrected the capability layer into a stack. The
constitution is the amendable owner of the middle layer:

```text
  UCAN                         interop envelope only (cross-system), not the brain
  ─────────────────────────────────────────────────────────────────────────────
  Structural namespace caps    meadowcap-shaped: who may write which cluster-path
   (meadowcap)                 (probe: willow-cluster-cap)            ── below ──
  Policy authorization         THE CONSTITUTION: which caps get minted, gated by
   (preset authorizer today,   tessera / quorum; how the rules change  ◀── here
    Biscuit candidate later)
  Live group / key state       p2panda-encryption fit: membership →    ── beside ─
   (engine behind Mere trait)  keys, key rotation, encrypted sync
```

Three reads fall out of this placement:

1. **Constitution depends on caps less than caps depend on it.** The graph-cluster
   brief established that cluster-path caps only verify if "each space's
   constitution declares the clustering algorithm + seed." So the constitution must
   declare the rule before a meadowcap-over-clusters can even be checked.
   Dependency order is constitution → caps, not the reverse. (Another reason not to
   start with meadowcap.)
2. **Biscuit is a candidate *expression mechanism*, not the constitution itself.**
   The constitution is the policy owner; Biscuit (or a simpler internal evaluator)
   is one way to encode and check it. The current `tessera::gate::Policy` presets
   are the accessible seed of that layer; start native (§6), and evaluate Biscuit
   when the rule language outgrows it.
3. **tessera is the input this layer reads**, while p2panda-encryption is the
   current group/key-state engine candidate it consults indirectly for "who can
   receive keys right now." Neither *is* the constitution.

---

## 5. The authorization seam

There is exactly one place all three layers meet, and it is a single function:

```rust
// Illustrative signature only (not implementation-ready).
fn authorize_governed(
    action: &GovernedAction,   // amend, grant-cap, adopt-adapter, set-policy, ...
    actor: PersonaId,
    constitution: &Constitution,   // supplies the RULE
    facts: &TesseraFacts,          // an INPUT (earned standing / membership facts)
    held_caps: &[CapabilityRef],   // an INPUT (granted authority)
    now_ms: u64,
) -> AuthorizationDecision
```

The constitution supplies the *rule*; tessera and caps are *inputs*. In v0 the rule
body is just "is `actor` the founder." The seam never changes again; only the rule
language behind it grows. Every future governance action and every capability grant
routes through this one chokepoint, which is what keeps the policy auditable and
keeps "what can this persona do here" answerable in one call.

Implementation note: moothold already exports `tessera::gate::authorize(policy,
cap_covers, facts, now)`. The constitution should wrap, own, or eventually move
that authorizer. Do **not** ship a second sibling policy gate with the same job.

---

## 6. The amendment-rule ladder

The rule language grows in rungs, each a new `AmendmentRule` variant, with no rework
of the seam:

```rust
// Illustrative only.
enum AmendmentRule {
    FounderSigned,                              // v0: orrery / bootstrap
    TesseraThreshold { scope: Scope, min: i64 },// v1: a member with earned standing
    Quorum { n: u16, weight: QuorumWeight },    // v2: the real moot
    CapGated { propose: CapabilityRef,          // v3: only governance-cap holders
               ratify: Box<AmendmentRule> },
}
```

- **v0 `FounderSigned`** unblocks the keystone with zero governance UX. It is also
  the correct, complete rule for a moot-of-one.
- **v1 `TesseraThreshold`** plugs tessera in as an input.
- **v2 `Quorum { weight: ByTessera }`** is the deliberative moot.
- **v3 `CapGated`** plugs meadowcap in last, when the probe graduates.

Distinguish the *governed action's* authorization rule from the *amendment* rule:
both are expressed in the same language, but a constitution may set (say) a low bar
to grant a "reader" cap and a high bar to amend the constitution itself.

---

## 7. Constitution as an event-sourced fold (the cheap build)

tessera already proves the exact pattern this needs: a per-moot append-only log,
folded into current state (`from_events` / `apply` / `scores`). The constitution is
the same machine with a different fold:

- `ConstitutionEvent::{ Genesis, Amended }`, each a signed op chained by prev-hash,
  authored exactly like the tessera ops `sync.rs` already writes.
- `Constitution::from_events(..)` folds the amendment log into the current ruleset;
  `apply` advances it; an amendment is accepted only if it satisfies the *prior*
  constitution's amendment rule (the self-amendment check).
- It **rides the same LogSync substrate and moot topic shape**, not the existing
  tessera store unchanged. Today `SyncedMoot` / `TesseraStore` are specialized to
  `Operation<TesseraExt>` and `TesseraEvent`; constitution ops need either a generic
  moothold operation store/session or a sibling `ConstitutionStore` following the
  tessera pattern. No new transport is implied.

Home: **`moothold`**, beside `tessera`. moothold is the governance / coordination
layer per the tiers brief and the recursive holder per §3. `mooting` is the
protocol-face (which social backend a moot speaks); the constitution is moot DNA,
not protocol plumbing, so it does not belong there.

---

## 8. What the constitution declares (the consolidated catalogue)

These are the rules currently deferred to "the constitution" across the briefs.
The constitution does not *implement* them; it *declares* the policy and the
thresholds, which the relevant subsystem then reads:

- **Amendment rule** (§2, the keystone clause).
- **Roles** as named cap bundles, granted under an authorization rule
  (trainer / evaluator / host / moderator named by the geist + tiers briefs).
- **Capability-grant policy**: who may mint / delegate which caps (§4).
- **Canonical clustering algorithm + params + seed** (graph-cluster brief, the
  first written clause).
- **tessera config + thresholds**: the `TesseraConfig` in force and the standing
  bars that gate actions.
- **Geist policy**: training-consent default (forbidden / allowed / ask), trainer
  selection, adapter adoption + revocation gates, base-model choice.
- **Resource policy**: offer/ask defaults, verification-tier bars, durability
  params (resource-coordination brief's "configurable params").
- **Fork rule**: the conditions and procedure for a constitutional fork (§9).

Not every knob should be constitutional law. Split the catalogue in two:

- **Constitutional clauses**: amendment rule, cap-grant authority, role-bundle
  definitions, fork rule, who may adopt / update governed policy modules.
- **Governed configs**: scheduler parameters, resource strategy scripts, verifier
  thresholds, model defaults, training cadence. These are controlled *by* the
  constitution, but can live as ordinary signed config/module events with their own
  lower amendment bars.

Each subsystem stays the implementer and reads its clause through the constitutional
authorization seam or through a direct query for the current governed config.

---

## 9. Forking is a constitutional operation

The tiers brief makes forking first-class ("a moot can fork over a governance
disagreement; the fork commits its own pins; the original keeps going"). In this
model a fork is clean: a new constitution whose `Genesis` cites the parent
constitution hash and the divergence point, carried forward by whoever signs the
new genesis. That `Genesis` must bind the new `MootId`, founder / initial governance
key, parent constitution (if any), divergence point, and initial rules hash; otherwise
joiners cannot distinguish a fork from a squat. Membership, flora pins, and tessera
history are inherited by reference; the amendment rule is what the forkers chose to
change. Fork is not disaster recovery; it is the amendment rule's escape valve when
amendment fails.

---

## 10. Relationship to the local capability gate

The [capability-gate catalogue](../research/2026-05-14_capability_gate_catalogue_brief.md)
already defines a four-layer policy chain (action → session → persona → app). That
is **local**: it enforces one user's policy on their own device, with no network and
no shared state. The constitution is **shared**: a community's policy over a
replicated log, converged across peers. They compose cleanly and must not be merged:

- The **local gate** answers "will *my* device let *me* do this here" (privacy,
  consent, isolation). It is the outermost ring of the resource-coordination brief's
  trust graduation (own devices = scheduling + permissions, no economy).
- The **constitution** answers "does *this community's* law permit this actor to do
  this to *the shared moot*." It is the inner social rule.

A governed action passes if it satisfies both: the local gate (this device's
policy) and the constitution (the moot's policy). orrery collapses the two (you are
the only member and the only device), which is why a moot-of-one needs neither
quorum nor a remote gate.

---

## 11. First slice (done-conditions, not dates)

In `moothold`, beside `tessera`:

1. `Constitution`, `ConstitutionEvent::{Genesis, Amended}`, and
   `AmendmentRule::FounderSigned`, folded by a `from_events` / `apply` pair
   mirroring `Ledger`.
2. `Genesis` binds `MootId`, founder / initial governance key, parent constitution
   (optional), divergence point (optional), and initial rules hash.
3. `authorize_governed(action, actor, ..)` with the v0 founder-only body, wrapping
   or owning the existing `tessera::gate` authorizer rather than duplicating it.
4. A `ConstitutionStore` or generalized moothold store/session that reuses the same
   p2panda LogSync transport + moot topic shape; no new transport, but not the
   tessera-specific `SyncedMoot` unchanged.
5. Tests mirroring the tessera suite: founder amends, non-founder rejected; the
   fold is deterministic; an amendment that fails the prior rule is rejected; two
   peers converge on the same current constitution.

**Done-when**: a moot has a readable, signed, amendable ruleset whose v0 rule is
"founder decides," reachable through one authorization chokepoint that every future
governance action and capability grant will route through.

Sequencing note: this is upstream of the comms shell's mooting adapters and of
S5.3's moot surface (a constitution is what a moot surface would *display* and
*amend*). It does not block the cross-machine sync validation, which is orthogonal.

---

## 12. Open questions

- **Native enum vs Biscuit for the rule language.** Start native (§6); the Biscuit
  evaluation (§8.8) triggers when the rule language needs delegation/attenuation
  the enum cannot express.
- **Membership source of truth.** The authorization seam needs "who is a member
  now." Does that live in the constitution (a roster clause), in tessera (anyone with
  standing > 0), or in the group/key-state engine (§4)? Likely constitution for
  admission / roles, tessera for *standing*, and p2panda-encryption for "which
  admitted members receive key material right now." Settle when v2 quorum lands.
- **Quorum over a partition.** A tessera-weighted quorum computed during a network
  split can disagree across peers. The amendment fold must be deterministic on the
  *converged* log; in-flight amendments are proposals until the log settles (the
  same eventually-correct posture tessera and the geist adapter cadence already
  take).
- **Amendment of the amendment rule.** Tightening vs loosening the amendment rule
  should arguably carry different bars (loosening one's own constraints is the
  classic capture vector). A `meta_amendment` bar, or simply the strictest current
  rule, decided at v2.
- **MootId derivation / discovery UX.** v0 must bind `Genesis` to `MootId` (§11);
  still open is whether `MootId` is directly derived from genesis bytes, points to
  a self-describing genesis record, or is introduced through an invite/ticket flow
  that carries the accepted genesis hash.
- **Cross-tier composition.** When a persona acts in moot M under moothold H, do
  both constitutions gate the action, and in what order? Probably innermost-first
  (M then H), mirroring §10's local-then-shared composition.

---

## 13. What this brief does not decide

- The concrete `GovernedAction` enum (grows with consumers, like `MereEvent`).
- Biscuit vs native rule encoding (§12).
- The membership source of truth (§12).
- The quorum weighting function beyond "by tessera" (linear / sqrt / capped: a
  policy choice, configurable per the configurability rule).
- The moot surface UX (S5.3 territory).
- meadowcap / p2panda-encryption production wiring timing (their own evaluations).

---

## Findings

(Captured during the 2026-06-06 drafting session.)

- Three layers wore one word. Capability = granted authority (meadowcap-shaped,
  probe/proof-only); reputation = earned standing (tessera, built); constitution =
  the durable, amendable owner of the rule binding them. The fog was starting with
  capabilities, and a second fog was treating tessera's existing preset gate as if
  it were already shared constitutional law.
- The amendment rule is the DNA: a constitution is an append-only amendment log
  folded into current rules, and the tiers are one primitive with different amendment
  clauses (orrery `FounderSigned` → moot quorum → moothold quorum-of-moots).
- The constitution is the amendable owner of the §8.8 stack's
  *policy-authorization* layer. Dependency runs constitution → caps (cluster-path
  caps need the constitution's clustering rule to verify), so meadowcap is the wrong
  place to start.
- It builds cheaply because tessera already proves the event-sourced fold + LogSync
  convergence shape; the constitution is the same pattern with a different fold, in
  the same crate. The store/session may need to generalize because current
  `SyncedMoot` is tessera-specific.
- One authorization seam is where constitution (rule) meets tessera + caps (inputs);
  the seam is stable, only the rule language grows. It should wrap/own
  `tessera::gate::authorize`, not compete with it.
- The local capability gate (one device, one user) and the constitution (one
  community, shared log) compose; a governed action satisfies both. orrery collapses
  them.
- The constitution resolves the standing deferrals: roles, clustering rule, tessera
  thresholds, geist consent/adoption, resource policy, fork rule are all clauses it
  declares while the subsystems stay the implementers.

## Pitfalls

- **Do not start with meadowcap.** It is the heaviest dependency, the probe is a
  probe for a reason, and caps depend on the constitution, not the reverse.
- **Do not merge the local gate and the constitution.** Local = my device's policy;
  constitution = the community's. They are different rings; merging them loses both
  the privacy boundary and the convergence guarantee.
- **Keep the rule body behind the authorization seam.** Subsystems ask the shared
  authorizer; they never branch on tessera scores or cap chains themselves, or the
  policy stops being auditable in one place.
- **Do not duplicate tessera's gate.** The existing preset authorizer is the seed /
  reference evaluator. Constitution owns its durable selection and amendment, rather
  than growing a second policy engine beside it.
- **Do not constitutionalize every operational knob.** Scheduler scripts, verifier
  thresholds, model defaults, and resource strategies are governed configs/modules
  unless the moot deliberately raises them to constitutional clauses.
- **Amendments fold deterministically on the converged log only.** Treat in-flight
  amendments as proposals; do not act on a quorum computed mid-partition.
- **Do not claim `SyncedMoot` is reusable unchanged.** Reuse p2panda LogSync and the
  moot-topic shape; generalize the store/session or build a sibling constitution
  store.
- **`mooting` is not the home.** The constitution is moot DNA, not protocol
  plumbing; it lives in `moothold` beside tessera.

## Progress

### 2026-06-06

- Brief drafted from the moot-synthesis conversation. Grounded against the live tree
  (no constitution/amendment primitive in code; tessera is the built facts layer and
  already has a preset reference authorizer; meadowcap-shaped caps are probe/proof
  only) and the four briefs that already reference "constitution." Drew the
  three-layer model, named the amendment rule as the DNA and the tier-unifier, placed
  the constitution as the amendable owner of the §8.8 policy-authorization layer,
  defined the authorization seam and the amendment-rule ladder, and proposed a v0
  first slice that mirrors tessera's fold while reusing the LogSync substrate rather
  than assuming `SyncedMoot` itself accepts constitution ops unchanged. DOC_README
  index updated. Next: Mark's steer on building the v0 fold beside tessera, or
  refining the layer model further first.
