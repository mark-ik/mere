# Projection Receipts Plan

**Date:** 2026-08-23  
**Status:** active; Wave 1 receipts landed 2026-08-23. Mer3ly is the headed
first consumer and gazette Ledger the heterogeneous second; the Gazette port
promotion and this plan landed at `0da3b8ba`. Wave 2 FT6 and FT7 closed
2026-08-25; FT8 is next, and wave 3 remains explicitly gated.
**Scope:** sequence the nine acceptance receipts of the
[projection-scenes direction note](../../2026-08-23_projection_scenes_and_graph_native_platform.md)
§8 into feature targets with named forcing consumers and done-conditions.

**Related:**

- [projection-scenes direction note](../../2026-08-23_projection_scenes_and_graph_native_platform.md) — the receipts this plan schedules
- [projection grammar adoption plan](2026-08-15_projection_grammar_adoption_plan.md) — the gated targets this plan supplies evidence to
- [projection grammar catalog](../research/2026-08-15_projection_grammar_catalog.md) — grammar, promotion rules, boundary map
- [shelfmark format note](../technical_architecture/2026-08-16_shelfmark_format_note.md) — owns the citation envelope FT4's receipt determines
- [Turnstone suite census](../../2026-08-22_turnstone_suite_composition_and_capability_census.md) — gazette port status and the composition seam
- [Scenograph content catalog](../research/2026-08-18_scenograph_content_catalog.md) — scene recipes

## 1. Charter and boundaries

This plan schedules proofs, not scenes. The direction note's nine receipts
demonstrate the graph-native platform shape without requiring the scene
catalog to be built; this plan orders them, names their consumers, and states
their done-conditions. Rulings it does **not** own:

- **Adoption-plan gates.** A receipt landing here supplies evidence to a gated
  target (A2, A5); the gate's own entry rules any status change. This plan
  never closes a gate.
- **The shelfmark envelope.** The
  [format note](../technical_architecture/2026-08-16_shelfmark_format_note.md)
  owns the eventual citation shape. FT4 produces the forcing receipt that
  determines it; the ruling lands there.
- **Categorical scene status.** The direction note and the catalogs own scene
  judgments. Nothing here promotes a portable primitive; every promotion still
  requires the grammar catalog's receipts in full.
- **Wave 3 consumers.** Scheduling the field receipts does not assert their
  consumers exist. Those targets carry explicit gates and no work is opened
  under them until a gate's trigger condition is met.

**Consumer pairing (ruled 2026-08-23).** Mer3ly's graph sandbox is the first
consumer: it already ships shelfmark v1 and single-target
`chirograph::Selection`, so wave 1 extends live wiring rather than founding a
harness. The gazette picker is the heterogeneous second consumer: contacts ×
facets — the Ledger form — over `gaz` in `ports/gazette`, whose surfaces the
census records as unbuilt. The promotion rules require both roles; the pairing
fills them with one shipping prover and one product pull.

## 2. Program shape

Three waves in dependency order. R*n* names receipt *n* of the direction
note's §8.

```text
Wave 1  FT1 (R2 Matrix) -> FT2 (R1 instances) -> FT3 (R8 coordination)
                        -> FT4 (R9 citation)  -> FT5 (second consumer)
Wave 2  FT6 (R7 view-state), FT7 (R5 parity), FT8 (R4 mixed realization)
Wave 3  FT9 (R3 derived marks), FT10 (R6 field distinction)   [gated]
```

Why this order: R1's three appearances need Matrix headings, so FT1's Matrix
precedes the full R1 receipt; FT3's second coordinated view *is* the Matrix
FT1 adds beside the sandbox's existing spatial view; FT4 cites the Matrix FT1
built; FT5 replays the wave against gazette data, which is the heterogeneous
second consumer the promotion rules demand. Wave 2 leans on already-closed
substrate (B1 accessible static realization, B2 probe-drivable projections)
and the Graphshell remote host. Wave 3 waits on a field-data consumer that
does not exist today.

## 3. Wave 1 — Matrix and coordination

**FT1. Two-reading Matrix (receipt 2).**
**Status: implemented 2026-08-23.** Mer3ly crosses independently produced
neighborhood and change graphlets from distinct datasets, carries headings and
cells through `ProjectionCaptureV1`, and exposes a semantic table beside the
spatial view. Cells cite relation identity or their contributor set.
Context: the direction note §3 rules Matrix a family over two independently
produced readings; a cell is a projected object with its own instance identity
citing the source relation, value, contributor set, or derivation that
produced it. Scenograph contract pressure is recorded but unforced; this
target proves the shape locally first.
Tasks: produce two independent axis scopes from mere's reading algebra; derive
cells with provenance; realize the Matrix in the mer3ly sandbox beside the
existing spatial view, with semantic row, column, and cell targets; give it an
accessible table realization (B1's machinery is the precedent); round-trip the
scene through Graphshell carriage deterministically. Prove the
graphlet A × graphlet B flavor (cross-scope relations), since that is the one
no single-reading form can fake.
Done when: receipt 2 holds verbatim — two independently produced graphlets
form the axes, cells preserve relation identity or contributor provenance,
the scene round-trips through Graphshell, and an accessible table realization
exists.
Outputs: the sandbox Matrix; a recorded list of what a *portable* Matrix
would require from sceno (contract pressure only — no sceno addition here).

**FT2. Repeated source instances (receipt 1).**
**Status: implemented 2026-08-23.** Spatial, Matrix, Scatter, and Deck
appearances share source identity while retaining distinct view/facet instance
addresses. A Deck dismissal records only that instance and leaves authority
untouched.
Context: the direction note §4 makes multiple projected instances per source a
platform rule. `sceno::Scene` already separates source and instance identity,
so this is host wiring, not scene-structure change.
Tasks: minimal Scatter panel and Deck cards in the sandbox, sufficient for one
entity to appear simultaneously as Scatter point, Matrix heading, and Deck
card; source selection emphasizes every visible instance; hover and keyboard
focus stay instance-local; each instance identifies the facet or derivation it
represents; accessibility groups or cross-references the repeated appearances;
removing one instance leaves source authority intact.
Done when: receipt 1 holds verbatim across the three appearances.
Outputs: the source-to-instance emphasis wiring, reusable by FT5 and FT7.

**FT3. Coordinated-view selection (receipt 8).**
**Status: implemented 2026-08-23.** `chirograph::CoordinatedSelection` owns
named focus/filter/brush clauses and explicit `single` or `crossfilter`
resolution. The sandbox's spatial and Matrix views consume foreign clauses,
and removing the Matrix clause restores the unfiltered spatial reading.
Context: A2's serialization half is landed; its resolution half is
deliberately gated on a genuine two-view ask (`chirograph::Selection` carries
`source` and `targets` and documents the resolution strategy as deliberately
absent). This target builds that ask: the sandbox's spatial view and FT1's
Matrix are two views over one authority.
Tasks: both views contribute selections with the producing view preserved; an
explicit, declared combination rule; deterministic serialization; clause
removal restores the unfiltered reading. The receipt requires one explicit
rule; A2's own validation list (crossfilter honored, brush-in-one-filters-the-
other) governs what the gate itself needs before closing.
Done when: receipt 8 holds verbatim — the combination rule is explicit,
deterministic, serialized, and removable. Evidence is then recorded against
A2, whose entry rules its own status.
Outputs: a candidate clause-and-resolution record shape; its home (chirograph
beside the intent triple, or mere host state) is decided under A2 with this
evidence.

**FT4. Composed citation and collapse compatibility (receipt 9).**
**Status: implemented 2026-08-23.** `incipit::ShelfmarkV1` carries named
inputs, per-input generation checks, reconstitutive reading parameters, and
opaque target-owned deltas. Mer3ly reconstitutes the Matrix and instance state;
the format note records the landed shape and exposure audit.
Context: the shelfmark note's 2026-08-23 section declares the gaps — composed
authorities and instance-scoped authored deltas — and rules that the receipt
determines the envelope rather than the note guessing it.
Tasks: build FT1's Matrix with axes from two distinct sandbox datasets;
round-trip a citation that reconstitutes it, preserving any instance-scoped
authored state and retaining checkability for every input; record the shape
the round-trip actually required in the format note, which owns the ruling.
Separately, audit which reading, arrangement, and registry ids have actually
been emitted in shipping shelfmarks and check them against the collapsed
catalog names; record the finding. Alias or incompatibility machinery is
built only if real wire exposure exists.
Done when: receipt 9 holds verbatim, the format note records the determined
envelope shape, and the exposure audit finding is written down.
Outputs: the envelope proposal in the format note; the audit finding.

**FT5. Second consumer: the gazette Ledger (receipts 1, 2, 8 replayed).**
**Status: implemented 2026-08-23.** Gazette's embeddable Ledger reads contacts
× selected facets, repeats one contact across picker/row/detail instances,
crossfilters with a recipient picker, emits a semantic table, and cites contact
and facet authorities separately without adding Gazette fields to the shared
records.
Context: promotion discipline requires a heterogeneous second consumer. The
census records gazette promoted to `ports/gazette` with its picker surfaces
unbuilt; the Ledger form (entities × selected facets) is the picker's natural
reading — contacts × handles, trust state, freshness — over `gaz`.
Tasks: a contact-picker surface realizing the Ledger Matrix with the same
contract and no product-specific fields in the reusable parts; the same
repeated-instance behavior (one contact in a results list, a Ledger row, and
a detail instance); coordinated selection where the picker composes with a
host view. Scope boundary: this target proves the projection contract against
gazette's data; the census's §7.3 boundary proof (the picker consumed by
Knot, Moot, and Signalman) stays the census's own done-condition.
Done when: the same recipe reads true against gazette data with no product
fields leaking into the shared contract, and the FT1/FT2 behaviors replay.
Outputs: the first real product surface on `ports/gazette`.

## 4. Wave 2 — parity and view-state

**FT6. View-state authority (receipt 7).**
**Status: complete 2026-08-24.** Mer3ly now cites its spatial reading as a named
input beside the Matrix axes and carries scope, filters, selected facets,
arrangement, placement, backdrop, camera, and repeated-instance visibility in
target-owned `ShelfmarkV1` delta sections. Unknown delta sections survive
re-share. Changing those deltas leaves every named input's authority record,
reading, parameters, arrangement, and expected generation unchanged. Pandect
now preserves mirror-only pane intent. Turnstone's view-only `Fit` receipt and
its real Knot domain-edit path provide the paired authority proof; the default
Turnstone endpoint remains deliberately unable to edit graph truth.
Context: mere owns saved scene recipes and active `ViewIntent` separate from
graph truth; the sandbox already restores view state through shelfmark v1.
Tasks: enumerate the view-state inventory — scope, filters, selected facets,
arrangement constraints, backdrop, camera; save and restore each without any
entering graph truth; prove an authorized source edit travels through
application intent routing rather than the view-state path.
Done when: receipt 7 holds verbatim.

**FT7. Local, remote, and frozen parity (receipt 5) — CLOSED 2026-08-25.**
Context: B1 (accessible static realization, including the long-form table) is
closed; the Graphshell remote projection host carries the remote leg;
`FrozenScene` names the frozen leg.
Receipt: FT1's Matrix runs through `LocalCarrier`, crosses an admitted
Graphshell session over `MemoryTransport` using Notochord/personae policy to a
source-free viewer, and freezes into a navigable semantic table. The exact
`InstanceId -> SourceRef` mapping, including repeated source instances, is
preserved in all three realizations. The Mere platform seam is at
`302bbe72d7597b7573e14199ce926bd3b03eea7f`; the Mer3ly consumer is
`4c42847272489c41a20dc515884e83f3b413059a`.
Done when: receipt 5 holds verbatim. **Met.**

**FT8. Mixed realization (receipt 4).**
Context: genet-side — one focus and pointer-capture model across DOM content
and custom leaves (web-platform host contract, meristem component contract),
Sprigging or GPU marks, AccessKit and genet-probe addressability (B2's
precedent). This target is also the natural forcing site for Cambium's
scene-hosting and keyboard-navigable Matrix components: the reusable
component work lands here, forced by a genet-hosted consumer, rather than
speculatively in wave 1 where the sandbox realization is web-side.
Tasks: one genet-hosted scene combining DOM controls, Sprigging or GPU marks,
accessible semantic targets, and automation addressing under a single focus
and action model.
Forcing consumer: candidate only — a Turnstone pane or a Knot surface; named
at execution. The census's pane-contribution seam is its own lane and is not
assumed here.
Done when: receipt 4 holds verbatim.

## 5. Wave 3 — field receipts (gated)

No work is opened under these targets until a gate's trigger condition is
met; scheduling them here asserts order, not readiness.

**FT9. Derived-mark integrity (receipt 3).**
A Distribution or Contour scene emits derived marks that remain selectable,
name their values and contributors, and never masquerade as source nodes.
Gate: a named statistical or field consumer.

**FT10. Field distinction (receipt 6).**
One sample dataset produces Territory and Contour scenes whose categorical
and scalar meanings remain distinct through rendering, picking, legends, and
accessible output.
Gate: a field-data consumer. Trigger candidates on record: host-side radio and
mesh placement facts (the sited-device brief's fact surface — the node does
not know where it is, the host knows where it put it), and a simulator or
radio model for Current.

## Findings

- `sceno::Scene` already carries separate source and instance identity; the
  §4 platform rule is host wiring, not new scene structure.
- `chirograph::CoordinatedSelection` now carries the first forced combination
  rule: crossfilter excludes the consuming view's own clause and intersects
  foreign clauses. Union and intersection remain absent until distinct
  behavior forces them.
- B1 and B2 are closed and reusable substrate: accessible static realization
  and probe-drivable projections serve FT1, FT7, and FT8 without new
  machinery.
- The mer3ly sandbox realization is web-side; Cambium components are
  genet-side. Wave 1 proofs ride the sandbox; Cambium's Matrix and
  scene-hosting components wait for FT8's genet-hosted consumer.
- Gazette: the resolver was promoted to `ports/gazette` on 2026-08-23; picker
  and feed surfaces are unbuilt; `gaz` is the contact-store home.
- The collapse audit found no emitted collapsed-name registry id. Mer3ly's
  wires use `graph`, `changes`, `activity`, `neighbors`, `matrix`, and
  `graph_layout:*`; `Neighborhood` is only a label for
  `graph_layout:radial`. No alias machinery is required.
- FT6 needed one additional named `spatial` input, not a new envelope: cited
  projection inputs say which authority and reading to reconstitute; target-owned
  delta sections say what the visitor did to that view.
- A headed FT6 pass exposed a stale browser consumer that still admitted Score
  v3 after the native contract and M7 had moved to v4. The browser now checks
  the same version, and M7 pins the agreement.

## Progress

- 2026-08-23: plan founded. Scoping ruled with Mark: Matrix and coordination
  first; mer3ly first consumer, gazette second; all nine receipts scheduled
  with wave 3 gated. No code opened.
- 2026-08-23: Wave 1 native contracts landed in Mere: coordinated clauses at
  `7aa64240`, `ShelfmarkV1` at `41ff2aba`, and reconstitutive reading parameters
  at `6cc014c4`. Chirograph's 27 tests and Incipit's five tests pass.
- 2026-08-23: mer3ly's first consumer landed at `5410512` and passes 28 focused
  repo-graph tests. Its rebuilt Wasm exposes the Matrix, composed-shelfmark,
  and resolver entrypoints; the M5 site suite passes. Advancing to Score v4
  exposed and corrected the showcase consumer's pinned version-3 assertion;
  all six M7 tests then pass against the landed Mere revision.
- 2026-08-23: Gazette replays receipts 1, 2, and 8 in the Ledger; all seven
  Gazette tests pass. Live attachment to the `gaz` store remains product wiring,
  outside FT5's projection-contract proof.
- 2026-08-23: headed mer3ly receipt passed. The live Matrix is 8 × 12 with five
  relation cells and semantic row/column headers. Spatial and Matrix clauses
  crossfilter; removing the Matrix clause restores all 18 spatial actors. A
  composed `mere.shelfmark/1` reload verifies two input generations, restores
  both clauses, and honors one dismissed Deck instance with no console warning.
  This closes A2 under the adoption plan's own validation rule.
- 2026-08-24: FT6 closed. Mer3ly's 28 repo-graph tests, four M5 tests, and six
  M7 tests pass; the checked-in Wasm was rebuilt. The full browser smoke passes
  against 18 repositories and 25 graph edges after saving and reopening scope,
  crossfilter clauses, selected Deck facet, grid arrangement, free motion,
  props backdrop, pinned Ashland placement, camera `80,0,1.20`, and a dismissed
  Ashland Deck instance. An in-app inspection found seven distinct camera
  controls and no console warnings or errors. Pandect's mirror-only intent
  round-trip passes. Turnstone's projected `Fit` accepts curation while refusing
  graph change, and its real Knot consumer saves, rejects stale authority, and
  reopens; those two Turnstone receipts used the existing post-source test
  binary because the live checkout's local patch redirects prevent a fresh
  locked build.
- 2026-08-25: FT7 closed. The FT1 Matrix preserves its exact
  `InstanceId -> SourceRef` mapping, including repeated source instances,
  through `LocalCarrier`, an admitted Graphshell `MemoryTransport` session
  using Notochord/personae policy to a source-free viewer, and the
  `FrozenScene` semantic-table realization. The Mere platform implementation
  is `302bbe72d7597b7573e14199ce926bd3b03eea7f`; the Mer3ly consumer is
  `4c42847272489c41a20dc515884e83f3b413059a`. FT8 is next;
  A3 stage two and A5 remain gated by their own consumer rules.
  Verification: all 29 `mer3ly-repo-graph` tests pass against Mere's pushed
  revision, and the generated `/repos/` page exposes 116 instance-mapped
  Matrix controls. Repeated `Mere` and `Turnstone` sources keep distinct
  instance IDs; selecting the shared `Mere` cell coordinates the graph node;
  the headed browser records no warnings or errors. The four-test M5 site
  contract passes with the active firmware-data lane. On clean origin, its
  asset-only test passes under `--locked`; the other three tests stop at the
  pre-existing retained-firmware digest mismatch before exercising FT7.
