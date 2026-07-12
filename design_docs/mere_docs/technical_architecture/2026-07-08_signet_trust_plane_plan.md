# The Trust Plane — personae and the Persona / P2P / Social Promotion

**Date:** 2026-07-08
**Status:** planning; the spine is founded. The companion to the
[generic graph substrate plan](2026-07-08_generic_graph_substrate_plan.md): that
plan promotes mere's **data plane**, this one promotes its **trust plane**. Home
is mere's design_docs because mere is the donor and first consumer; the crates
become standalone repos (the muniment / codicil / chartulary pattern), not mere
crates.

**Naming collapse (2026-07-08):** the spine is **personae**, one crate for
identity *and* carry. An earlier draft split it as `personae` (identity) +
`signet` (carry); `signet` was taken on crates.io and `personae` is free and
truer, so the two folded into one. personae 0.1.0 is published (MIT/Apache,
edition 2024) with the identity core promoted from `persona/identity`; the carry
layer folds in per §6. Read any lingering "signet" below as personae.

## 1. The frame: two planes, not one stack

mere holds two orthogonal substrates that were only ever entangled because mere
was the first app to need both:

- **Data plane (the charter family):** muniment (bytes) → codicil (edit log) →
  chartulary (the `Graph<N, E>`) → stemma (lineage) → scholia (RDF). *What things
  are and how they relate.* Owned by the graph substrate plan.
- **Trust plane (this plan):** **personae** (root of trust + carry spine) → murm /
  retinue / misfin (wire + comms) → moot / tessera / kith (standing + membership).
  *Who you are, who may read, who it syncs to.*

The planes are orthogonal. muniment "moves bytes and does not model what they
mean" (its own words); chartulary is graph shape with no notion of a reader.
Privacy, identity, and sharing are the trust plane's concern, and they bond to
the data plane at two seams (§4), not by living inside it. isometry, strophe, and
woodshed each need the data plane; the ones that are multiplayer or personal
(isometry sessions, strophe shares) need the trust plane too.

## 2. The spine: personae

**personae is the persona wallet carry layer, promoted.** The wallet plan already
declared it ecosystem-shaped: "the wallet is not persona-specific. It is the
carry layer for the whole system, and a persona is just its root-of-trust
instance." It sat in mere only because mere was the first consumer.

personae is to the trust plane what codicil is to the data plane: the spine every
other layer references. It carries the small, must-move-together things and never
the bulk:

- the master **seed** (identity root; carried out of band, re-derives everything),
- the **device roster** (the fabric of your devices, their exposure, revocation),
- **capability grants** (device delegation certs, cluster/kith read caps),
- **epoch history** (the private-lane key epochs, for encrypt-at-rest + rotation),
- **root references** (which graphs / engrams / logs a persona can recover).

Today this is split across `session-runtime::wallet_store` (manifests, roster,
grants) and `persona/identity` (vault, sealed records, the passphrase root + the
DPAPI startup-unlock backend landed 2026-07-06/08). Promotion unifies the carry
surface into one crate. Everything under it (identity primitives) and beside it
(comms, standing) references the spine.

**Name.** personae — the plural of *persona*, the register of a person's faces.
This spine subsumes what an earlier draft split off as `signet` (the carry
layer): `signet` is taken on crates.io and would have needed a `signaculum`
publish-alias, while `personae` is free, and the wallet plan's own framing (a
persona *is* the carry layer's root-of-trust instance) means the faces and their
carry belong in one crate. Founded + published 2026-07-08 (`personae` 0.1.0,
MIT/Apache, edition 2024) with the identity core; the carry layer folds in per §6.

## 3. The trust-plane stack

```text
personae    identity + carry in one crate: master Ed25519 + BLAKE3 derivation +
            vault + sealed records + passphrase/OS-store unlock (the identity core,
            published), plus seed carry, device roster, grants, epoch history, root
            refs (the carry layer, folding in). The register of your faces and how
            they travel. Root of trust.
murm        bilateral comms: cabals (murmuring) + misfin mail
retinue     the Reticulum transport lane (own endpoint impl, when it lands)
transport   the p2panda / iroh wire under murm
moot        community graph + fauna + hosting commitments
tessera     earned standing (the trust token); an input to authz
kith        capability sharing: kith/kin grants, revocation, namespace caps
```

Consumers: isometry (portable player identity + P2P session membership), strophe
(shared-session identity + capability grants), woodshed (optional sync identity),
mere (the full persona/comms/moot experience, at re-base).

## 4. Where the two planes touch

Exactly two seams, and both already have working prototypes rather than sketches.

### 4a. The seal seam, over muniment

muniment refuses to know what bytes mean, so at-rest privacy must be a layer
above it. That layer is the `PayloadSealer` seam landed for wallet gap #2
(`eidetic::seal`, commit `de0dd88`): it decides sealed-vs-cleartext by
`PrivacyClass`, sealing `LocalOnly` / `TrustedPeersOnly` under a persona epoch
key and passing the public lane through cleartext (so moots keep dedup / pin /
verify). In the promoted world this seam is the thin interface between muniment
(data plane) and personae (trust plane): **muniment stores bytes, personae owns the
epoch key, the sealer is the joint.** muniment stays usable by apps with no
personas at all; the sealer is the opt-in privacy layer identity-having apps add.

Promotion shape: the `PayloadSealer` trait travels to sit over muniment's
`BlobStore`; personae provides the concrete sealer from its epoch history. gap #2
was never wallet plumbing — it was this joint, built early.

### 4b. The sync gate, over codicil

A chartulary graph's edits are a codicil log; sharing a graph is syncing that log
to other personas. The moot / tessera / kith layer is what admits the bytes:
membership plus a capability grant decides who a codicil log replicates to, over
murm. So **codicil edits ride murm, gated by the trust plane.** murm + the moot
gate is the prototype; the promotion wires the gate to codicil's replication seam
rather than mere's bespoke sync.

The two seams are the whole contract between the planes. Nothing else crosses:
the data plane never learns about keys, the trust plane never learns graph shape.

## 5. Promotion map (mere crate → trust-plane crate)

- `persona/identity` → **personae** (founded + published 2026-07-08; the identity
  core promoted near-verbatim). `persona` on crates.io is a Mozilla inside-joke
  reservation; `personae`, the plural, is free and truer — the register of a
  user's faces.
- `session-runtime::wallet_store` + `wallet_grant` + identity's vault/epoch parts
  → **personae** too — the carry layer folds into the same crate, the consolidation
  that made a separate `signet` unnecessary.
- `murm/*` (transport, murmuring, murm, misfin) → **murm** family + **retinue**
  (retinue already scaffolded; own Reticulum impl, trigger-gated).
- `moot/moothold` (tessera, roster, fauna, constitution, reciprocity) → **moot /
  tessera**; **kith** (the capability-sharing slice, already named in
  `2026-06-30_kith_capability_sharing_plan.md`).
- `eidetic::seal` → the **seal interface** promoted to the muniment/personae joint
  (§4a); eidetic's semantic memory otherwise re-bases onto muniment like mere's
  graph re-bases onto chartulary.

## 6. Phases

Done-conditions, not durations. S0–S2 do not block on mere's re-base.

- **S0: personae skeleton.** The standalone repo: the carry manifests
  (identity-level + per-persona), device roster, grant vocabulary, epoch-history
  types, typed load/save over a muniment-style backend seam. **Done when** a
  persona's carry state persists and reloads on the seam with no mere dependency.
- **S1: the seal joint.** Promote `PayloadSealer` to sit over muniment's
  `BlobStore`; personae supplies the concrete epoch sealer. **Done when** a private
  blob written through muniment is unreadable at rest without a personae epoch, and
  a public blob stays cleartext and dedups.
- **S2: identity core — DONE 2026-07-08.** `persona/identity` lifted verbatim to
  the standalone **personae** crate (0.1.0, published); builds + 32 tests outside
  mere. This is the founding the carry-layer work (S0/S1) lands on top of.
- **S3: first non-mere consumer.** One sibling app carries a persona on personae.
  Candidate: isometry (its P2P sessions want portable player identity; behind its
  own keystones). **Done when** a non-mere app pairs a second device and shares
  capability-gated state through the trust plane.
- **S4: the sync gate.** Wire the moot/tessera/kith admission to codicil's
  replication seam. **Done when** a chartulary graph shared between two personas
  replicates its codicil log only to admitted members.
- **S5: mere re-base.** mere consumes personae + identity + the promoted comms/moot
  crates; the in-tree wallet_store/wallet_grant retire. **Done when** meerkat runs
  on the trust plane with no behavior change. Long tail, deliberately last.

## 7. Open questions

1. **Umbrella name.** The data plane is colloquially "chart" (chartulary). The
   trust plane has a named spine (personae) and named layers (tessera, kith,
   retinue, murm) but no single colloquial umbrella. Force one, or let personae
   stand for the spine and the plane stay unnamed?
2. **Carry layer in-crate, or a thin split?** personae holds the identity core
   now; does the carry layer (roster, grants, epochs) live in the same crate or a
   thin sub-crate over it? Bias: one crate until a consumer needs identity without
   carry (e.g. a stateless signer).
3. **Seal-interface home.** Does `PayloadSealer` live in muniment (as an optional
   trait consumers implement) or in a thin `personae`-side crate that depends on
   muniment? Bias: trait in muniment, impl in personae (muniment stays the seam).
4. **Grant format promotion.** mere's meadowcap-shaped grants vs a Biscuit
   re-expression (wallet plan's open question) — settle before personae's grant
   vocabulary locks, or version it.
5. **Epoch-history storage.** personae's epoch history is muniment slots vs its own
   sealed records — resolve against muniment's `SlotStore` shape at S1.

## 8. Provenance

Grounded in 2026-07-08 reads of the
[persona wallet carry layer plan](../implementation_strategy/2026-06-25_persona_wallet_carry_layer_plan.md),
the [generic graph substrate plan](2026-07-08_generic_graph_substrate_plan.md),
muniment's README + founding proposal (repos/muniment), the chartulary / sibylla
/ vates repos, and the `eidetic::seal` seam built for wallet gap #2 (mere commit
`de0dd88`). Naming resolved with Mark 2026-07-08: one crate, **personae**, for
identity + carry (the earlier `signet` split collapsed in); personae 0.1.0
published the same day (MIT/Apache, edition 2024).
