# FLORA, Tulpa, and Standing plan

**Status (2026-08-31):** Complete on `codex/0831-integration`. Gemot owns the
social protocol, Distillery owns exact tensor execution, and the integrated
signed, replicated, restart-durable receipt is green.

## Rulings and ownership

**Standing** is community-scoped reputation through commitment follow-through.
Signed facts are authoritative; scores and eligibility are deterministic
projections. Standing may explicitly determine who is eligible to endorse a
proposal, but it does not silently create authority or multiply a vote.

**Tulpa** is a Moot's community-recognized, revisioned collective artifact and
persistent identity. Gemot owns its proposals, frozen electorate,
endorsements, adoption, revocation, rollback, and retained facts. This takes
the role earlier sketched as *egregore*. The former memorial Tulpa reservation
is now Hagiograph.

**FLORA** means federated LoRA. The protocol is the FLoRA stacking construction
described by Wang et al. in [*FLoRA: Federated Fine-Tuning Large Language
Models with Heterogeneous Low-Rank Adaptations*](https://arxiv.org/abs/2409.05976):
participant A factors concatenate vertically, B factors concatenate
horizontally, and participant scaling is applied exactly once to B. Ranks may
differ; the global rank is their exact sum. A signed round declares an explicit
rank budget, and exceeding it is an error. Compression is a separate,
explicitly versioned transform, never an implicit part of aggregation.

Gemot carries signed social facts and exact artifact references. Distillery and
ESP execute tensors. Mesh owns jobs, leases, and checkpoints. Eidetic and
Muniment store artifacts. Raw corpora and tensors do not enter Gemot's public
lanes. Adapter artifacts can still leak training information, so audience and
release policy stay explicit community decisions.

## Phase 1: Standing migration

Rename the Gemot domain, wire lane, public types, service calls, Mien, Moothold,
and Gaz consumers. Preserve real stored data by opening `tessera.redb` when
`standing.redb` is absent and accepting serialized `tessera_operations` as the
legacy spelling.

Done conditions:

- New operations use `gemot/standing/v1` and current source names Standing.
- Existing Tessera stores and native drops decode without rewriting facts.
- The deprecated source aliases are visibly compatibility-only.
- Standing gates Tulpa eligibility only through an explicit constitution rule.

## Phase 2: Tulpa social fold

Add Tulpa as a Gemot module and independent replicated lane. A proposal embeds
a `mooting::RecognitionContext`, freezing the electorate and threshold at the
time of proposal. Endorsements are unique by signer. Deterministic tie-breaking
must make the same fact set fold identically on every peer. Revocation and
rollback change effective adoption while proposals, endorsements, and prior
versions remain inspectable.

Done conditions:

- Adoption, duplicate endorsement, outsider endorsement, membership churn,
  revocation, and rollback have deterministic tests.
- A Tulpa version names an exact artifact digest and byte count.
- Two peers converge on a Tulpa proposal over the dedicated LogSync lane.

## Phase 3: exact FLORA aggregation

Gemot validates the round's social and compatibility contract and retains exact
references to each A factor, B factor, receipt, and candidate. Distillery loads
the referenced SafeTensors, validates matching target modules and dimensions,
applies the declared coefficient to B only, stacks A and B on their required
axes in deterministic participant order, and writes one exact output artifact.

Done conditions:

- Heterogeneous ranks produce `global_rank = sum(participant ranks)`.
- Output deltas equal the weighted sum of participant deltas.
- Reordering input receipts cannot change output bytes.
- Missing tensors, incompatible shapes or dtypes, invalid coefficients, and
  exceeded rank budgets fail loudly.
- Gemot and Distillery agree on the same scaling and artifact contract.

## Phase 4: integrated receipt

Exercise a signed FLORA round through Gemot, aggregate its out-of-band factor
artifacts in Distillery, publish the resulting candidate reference, and adopt
that exact candidate as a Tulpa under a frozen electorate. Reopen the durable
stores and prove the same Standing facts, FLORA round, Tulpa facts, and effective
version project after restart.

Done conditions:

- The receipt covers authoring, replication, aggregation, candidate
  publication, adoption, restart, and deterministic replay.
- It distinguishes social validation from measured tensor execution.
- The final report names any environment blocker before compilation and does
  not call an unentered gate green.

## Findings

- **2026-08-31:** the older lower-case *flora* corpus definition was a
  capitalization-driven misunderstanding. The public records collection is the
  Moot's fauna; FLORA is only federated LoRA.
- **2026-08-31:** a frozen `RecognitionContext` is necessary. Recomputing an old
  proposal against current membership lets later joins or departures rewrite a
  historical decision.
- **2026-08-31:** multiplying endorsement weight by Standing would turn a
  reputation projection into undeclared governance authority. The implemented
  rule is an optional eligibility floor with one eligible signer contributing
  one endorsement.
- **2026-08-31:** Gemot can validate ranks, references, participants, and
  release facts without carrying tensor or corpus bytes. This keeps governance
  independent of the tensor runtime and avoids publishing training material by
  protocol accident.

## Progress

- **2026-08-31:** implemented Standing, its legacy store/drop readers, Tulpa,
  FLORA receipts, seven-lane Moot integration, and two-peer lane coverage in
  `244b66be` (source lane `7fddd485`).
- **2026-08-31:** implemented the Hagiograph reservation move and canonical
  terminology amendments on the integration branch.
- **2026-08-31:** implemented exact heterogeneous-rank FLoRA stacking in
  Distillery at `b806acf0`. Contributions sort by governed contribution id;
  their exact rational round weight and `alpha / rank` coefficient apply once
  to B, while A is concatenated vertically and B horizontally.
- **2026-08-31:** bound the social and tensor layers in `0738b709`. The receipt
  signs and replicates Standing, FLORA, and Tulpa facts, resolves exact factor
  references, aggregates reversed arrivals to stable bytes, publishes and
  adopts the candidate, records follow-through, reopens three redb stores, and
  proves the same projections after restart.
- **2026-08-31:** measured 1/1 integrated receipt, 11/11 Distillery library
  tests, and 122/122 Gemot library tests green. Package-scoped strict Clippy is
  recorded in the companion receipt. Upstream `77b3c3a2` corrected the stale
  `genet-taffy` requirement to the pinned revision's `0.14.0`; after merging it,
  the integrated receipt also passed directly from the real integration
  checkout.
- **2026-09-02:** the stack was rebased onto `origin/main` `f2924f08` and
  landed on `main`. The integrated receipt passed again (1/1), with Distillery
  12/12 and Gemot 122/122; the rerun table lives in the
  [receipt](../../mere_docs/testing/2026-08-31_flora_tulpa_standing_receipt.md).
- **2026-09-03:** a FLoRA round is trainer-version homogeneous by the
  stacker's own checks, and that is now proven twice over: the
  [autodiff trainer plan](../../mere_docs/implementation_strategy/2026-09-02_autodiff_lora_trainer_plan.md)
  landed `esp-trainer-v1` adapters, a round over two real v1 adapters stacks
  to identical bytes in either arrival order, and a v0/v1 mix is refused by
  name on `adapter_format_version` first and on the config bytes'
  `peft_version` second. The exact stacking mathematics did not change.
