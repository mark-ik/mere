# Doc Policy Consolidation Plan — one canonical core, facade-collapsed areas, spec docs repatriated

**Date**: 2026-08-24
**Status**: **D3 complete 2026-09-06; phase D remains open for D2 coverage of
16 documents added after the 281-document judgment snapshot.** A, B and C
landed 2026-08-24. D1, the original D2 judgment pass and depth re-pass, and D4
completed 2026-09-02. D3 closed 2026-09-06 with the repository's self-testing
audit at zero failing findings; intentional unresolved evidence and committed
future targets now carry exact occurrence-level annotations rather than
weakening an authoritative document with a whole-file historical label.
Decisions taken by Mark 2026-08-24 and 2026-09-02 remain recorded in
§Decisions.

**Committed 2026-08-24** in twelve repos: mere (swept by a concurrent session
in `db9c613c`), smolweb, mesocosm, woodshed, and the eight policy-only repos.
**Not committed**: genet and turquet, where sessions were actively writing at
the time — genet's `design_docs/` is therefore still untracked, which is a live
risk recorded below. Two follow-on lanes are recorded and **not** scheduled:
genet's `docs/` migration, and the pre-existing broken links (§Findings).

**Scope**: Three separable pieces, in dependency order.

- **A** — extract a canonical `DOC_POLICY.md` core and distribute it.
- **B** — collapse scattered member-crate `design_docs/` into repo-central area
  roots, and repair the index and structure list they were invisible to.
- **C** — repatriate spec-accurate docs to the smolweb workspace; found the
  receiving structure there.

## Decisions (Mark, 2026-08-24)

1. **Distribution**: copy the core into each repo with a provenance stamp
   naming the canonical source and a date, so drift is detectable by diff.
   *Not* a cross-repo pointer — `Code` is not a git repository, so a root-level
   canonical file would be unversioned, and the one cross-repo pointer this
   workspace already had (mere → `graphshell/design_docs/DOC_POLICY.md`) is
   dead, since graphshell was archived and its local clone deleted.
2. **Facade shape**: repo-central. Everything lands under
   `design_docs/<area>_docs/`; `crates/*/design_docs/` all disappear.
3. **Repatriation**: found `smolweb/design_docs/` and move the spec-accurate
   material there.
4. **C3 / genet**: found `genet/design_docs/` properly — canonical policy plus
   a real index — and move the eight orphaned docs into it. Chosen over the
   three alternatives with the trade-off stated: it leaves genet with **two doc
   homes**, the new `design_docs/` and the existing flat `docs/`. Mark took
   that knowingly. It has one real advantage the migration option lacks:
   founding a new directory does not touch the thirteen files another session
   currently has dirty inside `docs/`.

## Decisions (Mark, 2026-09-02)

Taken after the D1 numbers were on the table, with named options.

5. **Scope**: everything active — claim verification across all 281 docs, not
   only the current set. Older material is expected to yield less per doc.
6. **Fix versus report**: fix the mechanical class as one batch once the
   other session's `DOC_README.md` edit commits; bring judgment calls (dead
   plans, superseded receipts) with evidence before changing anything.
7. **Record**: reopen this plan rather than found a new brief — it already
   holds the standing 2026-08-24 finding, and core §1 prefers the existing
   home.
8. **Code review**: "review mere" also meant the code; that is a separate
   lane with its own scope, not part of D. Non-Fable subagents may be used
   for the audit's fan-out.

Rulings on the D4 report, taken 2026-09-02 with the report in hand:

9. **Dead plans (§2 of the report, sixteen)**: hold. Mark reads the list
   himself before any archiving; nothing in that class moves in D3.
10. **Historical banners (§4, twenty-five)**: yes as a class, but the list
    with proposed banner text is shown first; nothing is bannered until he
    has seen it. Produced after ruling 12's re-pass so the list reflects the
    deeper evidence, since most candidates sit in those four batches.
11. **Status lines contradicted by the document's own Progress log (§6a,
    thirty-four plans)**: mechanical. The header is rewritten to what the
    log already records; lands with the D3 batch.
12. **Depth re-pass**: yes — batches 03–06 (the May–July plans, 88 docs)
    are re-run on Opus, independently of the Sonnet outputs, which are kept
    beside them; the aggregate prefers the deeper pass where both exist.

## Findings

Verified 2026-08-24 against the trees, not against other docs.

### The five slim policies share a core; they diverged into two lineages

All five (`hocket`, `isometry`, `mesocosm`, `paredros`, `woodshed`) carry an
identical section skeleton — same headings, same order. Word-level agreement:

| Pair | Shared |
|---|---|
| hocket ↔ woodshed | 94% |
| mesocosm ↔ paredros | 93% |
| isometry ↔ mesocosm | 82% |
| hocket ↔ mesocosm | 36% |
| isometry ↔ woodshed | 45% |

Line-level diffs overstate this badly: most differing lines are re-wrapping at
a different column width, not different rules. The real split is two lineages,
**each carrying rules the other lacks**:

- **hocket/woodshed**: §8 *Workflow Rule for AI Assistants*; the
  no-"Day 1 / Week 2" rule in §7.
- **mesocosm/paredros/isometry**: a much stronger §7 — a dated **Status** line
  kept current, phases with done-conditions, a **Findings** section carrying
  code references, a dated **Progress** log, code samples marked illustrative
  vs compile-ready, and extraction of deferred points before archiving.

`mere` has none of those five. So the canonical core is a **union of both
lineages plus mere's nine principles**, not a copy of any existing file.

### mere's policy is stale about mere

- Its structure list names `platen_docs/`, which does not exist.
- It omits `eidetic_docs/`, which does.
- It links `../../graphshell/design_docs/DOC_POLICY.md`, which the same
  document elsewhere records as deleted locally.

### Three area roots are orphaned

`inker_docs/` (1 doc), `nematic_docs/` (7), `verso_docs/` (2) describe crates
that no longer live in mere. `crates/inker`, `crates/nematic` and `crates/verso` *(historical citation)* <!-- doc-audit: historical-path -->
are all absent; the code is now `genet/components/{inker,nematic,verso-tile}`.

### 18 of 20 member-crate docs are invisible to the canonical index

Nine `crates/*/design_docs/` directories hold 20 docs. `DOC_README.md` — which
policy §7 declares the sole canonical index — indexes only two of them
(muniment's). This is a standing violation of the repo's own rule, and it is
the actual argument for phase B; ease of access is secondary.

| Directory | Docs | Facade member? |
|---|---|---|
| `crates/armillary/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 1 | no — standalone crate |
| `crates/dramatis/gaz/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 1 | yes |
| `crates/dramatis/personae/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 3 | yes |
| `crates/eidetic/chartulary/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 3 | yes |
| `crates/eidetic/codicil/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 1 | yes |
| `crates/eidetic/muniment/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 2 | yes |
| `crates/eidetic/scholia/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 1 | yes |
| `crates/intel/esp/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 6 | yes (2 subdirs) |
| `crates/scenograph/design_docs` *(historical citation)* <!-- doc-audit: historical-path --> | 2 | no — already facade-level |

`eidetic` is double-homed: a central `eidetic_docs/` (3 docs) *and* four member
crates (7 docs). Decision 2 resolves this toward the central root.

### The smolweb split rule was largely executed, contrary to first reading

`2026-08-03_smolweb_home_decision.md` is not an unexecuted decision. Its own
progress log records `gopher-protocol`, `finger-protocol`, `gemini-protocol`
and `scroll-protocol` published, with errand 0.2.0 consuming them and falling
from 2786 to 1612 lines. The smolweb workspace now holds 13 crates.

What remains open there is narrow: the nex de-duplication, and rewiring
nematic's scroll engine off the gemtext fallback.

### Receiving ground

- `smolweb`: clean tree, 13 crates, **no `design_docs/`**. Founding it is new
  construction, no conflict.
- `genet`: 38 dirty files; a flat `docs/` of 163 markdown files with **no**
  `DOC_README.md` and no policy. Not `design_docs/`.
- `mere`: 43 dirty files, most recent 07:56 on 2026-08-24. **None of them
  intersect the 20 docs phase B moves**, so B is safe to run; but the tree is
  live and must not be swept.

### Inheritance set

23 real repos under `repos/` (`testing` is not one). Six carry a policy; seven
more have `design_docs/` but no policy — `cleromancy` (33 docs), `retinue` (92),
`turnstone` (28), `wgpu-scry` (22), `wgpu-weld` (2), `turquet` (1),
`wavicle` (1). None of those seven has a `DOC_README.md` or a
`PROJECT_DESCRIPTION.md`. The remaining ten have no `design_docs/` at all and
are out of scope until they grow one.

## Phases

### A. Canonical core

- **A1**. Write the canonical core: mere's nine principles, plus the
  hocket/woodshed §8 and no-calendar-labels rule, plus the
  mesocosm/paredros §7 plan discipline, plus §6 PROJECT_DESCRIPTION ownership.
- **A2**. Update the six existing policies to core + local addendum. mere's
  repo-specific half (area-root list, graphshell donor history, trademark and
  crate reservations) becomes an addendum below the core, not a deletion.
- **A3**. Add a policy to the seven repos that have docs but none.

**Done when**: every `DOC_POLICY.md` in the workspace shares a byte-identical
core section, each carries a provenance stamp, and `diff` between any two shows
only addendum content. **Met**: fifteen files, core `54f3a6ed…` in all of them,
the `## Local addendum` boundary at line 124 in every one.

### B. Facade collapse (mere-local)

- **B1**. `git mv` all nine `crates/*/design_docs/` trees into
  `design_docs/<area>_docs/<category>/`, founding `armillary_docs/`,
  `dramatis_docs/`, `intel_docs/` and `scenograph_docs/`; eidetic's four
  members merge into the existing `eidetic_docs/`.
- **B2**. Repair every relative link broken by the move.
- **B3**. Index all 20 docs in `DOC_README.md`.
- **B4**. Correct the policy's structure list: drop `platen_docs/`, add the
  roots that exist.

**Done when**: `find crates -type d -name design_docs` returns nothing; every
moved doc appears in `DOC_README.md`; no relative link in a moved doc resolves
to a missing file.

### C. Repatriation

- **C1**. Found `smolweb/design_docs/` with the canonical policy and a
  `DOC_README.md`.
- **C2**. Move the two spec-level docs — `2026-08-03_smolweb_home_decision.md`
  and `2026-08-04_protocol_carrier_independence.md` — to smolweb, repairing
  their cross-repo links.
- **C3**. Found `genet/design_docs/` with the canonical policy and a real
  index, then move the eight orphaned docs — the five implementation-side ones
  in `nematic_docs/`, plus `inker_docs/` and `verso_docs/` — into it, keeping
  their area-root structure. Repair their code links onto genet's
  `components/` layout, and convert mere's inbound references to path
  citations.

**Done when**: C1, C2 and C3 land with links resolving in every direction;
`mere/design_docs/` contains no area root describing code that lives elsewhere;
and genet's two-doc-home split is stated explicitly in its policy rather than
left to be discovered. **All met 2026-08-24.**

### D. Active-tree audit (reopened 2026-09-02)

The 2026-08-24 broken-link finding was recorded and not scheduled. Mark asked
for the audit on 2026-09-02; this phase is it, scoped to the active tree (281
docs at `bcb222ce`; `archive_docs/` excluded by design).

- **D1**. Mechanical pass over every active doc, read from HEAD so another
  session's in-flight edits are neither audited nor disturbed: index coverage
  in both directions, relative-link resolution, plan Status extraction and
  classification, commit-hash resolution against every local repo, code-font
  path existence. Method and blind spots in §Finding (2026-09-02).
- **D2**. Judgment pass over all 281 docs: each gets a disposition (current,
  historical-marked, historical-unmarked, superseded, dead), a status-line
  verdict, and a per-claim list checked against the pinned tree — paths,
  symbols, versions, hashes, cross-doc claims, internal contradictions. Run
  as fourteen batched verifications against a detached worktree pinned at
  `bcb222ce`, with protocol and manifests kept outside the tree; results
  aggregated here.
- **D3**. Mechanical fixes as one batch once `DOC_README.md` is no longer
  dirty under another session: index the orphans; convert links into the
  Claude memory directory to prose or remove them; add Status lines where
  absent; archive plans D2 confirms complete, extracting open points per
  core §8; add the rename-key banner to, or retire paths in, docs D2 classes
  historical-unmarked; convert active-tree cross-repo relative links to path
  citations per core §5.
- **D4**. Judgment report: dead and stale-open plans, contradicted receipts,
  docs to mark historical or superseded — each with evidence, none changed
  until ruled.

**Done when**: D1 numbers are recorded with method and blind spots; every
active doc has a D2 block in the aggregated record; the D1 checker re-run
after D3 reports zero orphans, ghosts, memory-directory links, Status-less
plans, unmarked broken relative links, unmarked missing known-root paths, and
invalid or stale citation annotations. `archive_docs/` remains excluded. Any
unresolved active-tree reference is informational only and has the exact
same-line historical-citation or planned-target marker defined by Mere's local
addendum. D4 is delivered to
Mark with each item's evidence.

## Progress

- **2026-08-24**: Plan written. Assessment verified against the trees; three
  claims from the initial reading corrected (a shared core does exist; the
  smolweb rule was largely executed; two of the nine doc directories are not
  facade-member scatter). Orphaned area roots and the 18-of-20 indexing gap
  found during verification, not predicted.

- **2026-08-24 (A landed)**: Canonical core v1 written as a union of both
  lineages plus mere's principles — ten sections. Distributed to **fifteen**
  repositories: the six that had a policy, the seven that had docs but none,
  and the two founded during phase C, smolweb and genet. Verified
  byte-identical: `sha256` of the core region is `54f3a6ed…` in all fifteen,
  and `diff` between any two shows only addendum text.

  One correction made during the pass, worth keeping because the ambiguity
  caused a real error: the old policies never said **where**
  `PROJECT_DESCRIPTION.md` lives. The initial survey looked at repo roots,
  found none, and concluded no repo had one — wrong, since all five slim repos
  keep it at **design_docs/PROJECT_DESCRIPTION.md**. Core §7 now names the path
  explicitly.

- **2026-08-24 (B landed)**: All twenty member-crate docs moved into area
  roots via `git mv`; `crates/*/design_docs` no longer exists. Four new area
  roots founded (`armillary_docs`, `dramatis_docs`, `intel_docs`,
  `scenograph_docs`); eidetic's double-homing resolved into the central root.
  All twenty indexed in `DOC_README.md`, which is now link-clean. The policy's
  structure list corrected, and its dead link to the archived graphshell policy
  removed.

  **Link repair was larger than the moved files.** Checking links *from* the
  moved docs was not sufficient — twelve references *to* them, scattered across
  briefs and plans elsewhere in the repo, still named `crates/*/design_docs/`
  paths. Two link forms defeated a line-based pass and needed hand repair
  because the link text wrapped across a newline. Four broken links found in
  the process predated this work: one target had been archived on 2026-08-18
  without its referrer being updated, which is the §5 repair rule being missed
  at the time.

- **2026-08-24 (C1, C2 landed)**: `smolweb/design_docs/` founded — policy,
  index, and two category roots. The two spec-level docs repatriated:
  `2026-08-03_smolweb_home_decision.md` to `technical_architecture/` and
  `2026-08-04_protocol_carrier_independence.md` to `research/`. Every
  cross-repo reference in both directions converted from a relative link to a
  path citation per core §5 — five inbound references from mere's tree
  included. Both repos' doc trees are link-clean for these documents.

- **2026-08-24 (C3 landed)**: `genet/design_docs/` founded — canonical policy,
  a real index, and the three area roots `inker_docs/`, `nematic_docs/`,
  `verso_docs/` carried over intact. All eight documents moved and indexed.

  **Code links were repaired, not just doc links.** The moved docs pointed at
  mere's old `crates/inker/...` layout, which no longer exists anywhere; in
  genet the code was `components/inker/...` *(historical citation)* <!-- doc-audit: historical-path -->, and nematic is a *top-level*
  component rather than sitting under `inker/engines/`. Eleven of twelve code
  paths were verified present before rewriting; the twelfth was found at its
  real location rather than guessed.

  **mere's inbound references**: eight link targets across seventeen files,
  including three inside `archive_docs/`, converted to cross-repo path
  citations. Six further `verso_docs/` references were left alone — they point
  at the *donor graphshell's* verso area, not mere's, and were already broken
  before this work.

  **mere's policy now states the rule this leaves behind**: when code leaves
  the repository, its docs go with it in the same session. An area root
  describing code that lives elsewhere is the failure this pass cleaned up.

- **2026-09-02 (D1 landed, D2 opened)**: Mark asked for a review of mere and
  an audit of its docs. Mechanical pass run against `bcb222ce` (numbers and
  blind spots in §Finding 2026-09-02); four scope decisions taken with the
  numbers on the table (§Decisions). A detached worktree pinned at `bcb222ce`
  was created outside the tree so the judgment pass audits a stable snapshot
  and cannot touch the live tree, where another session held eight docs
  dirty. Fourteen batch manifests generated, each carrying its docs'
  mechanical pre-findings; verification protocol written; batches dispatched
  in waves of four, current-state docs first (root briefs, technical
  architecture, July–August plans). The `DOC_README.md` index line for this
  plan still says complete; it is corrected in D3 with the rest of the index
  work, because that file is dirty under the other session.

- **2026-09-02 (D2 landed, D4 delivered)**: fourteen batches, 281 of 281
  docs with a record, every batch at full coverage. 2,556 claims checked:
  1,986 hold, 481 stale (19%), 90 unverifiable; 79 contradictions.
  Dispositions: 134 current, 69 historical-marked, 60 historical-unmarked,
  16 dead, 2 superseded. Recommended actions: fix-refs 67 docs,
  update-status 56, mark-historical 23, archive 16 outright plus those
  recommended after a status update, escalate 7. **One event explains most
  stale paths**: the meerkat deletion, `c5f01064` on 2026-07-18 (198 files,
  63,680 deletions), which roughly thirty plans still describe as landed,
  several with Progress entries dated after it; then the renames
  `session-runtime`→`pandect`, `persona`→`dramatis`,
  `orrery`→`conatus`/`canvas`, `moothold`→`gemot`, `node-lineage`→`stemma`,
  `mere-orrery`→`glossary`, and the device host into `ports/djinn`. **The
  dominant defect class** is a status line contradicted by the same
  document's Progress log — about two dozen plans whose header froze while
  the log moved on; the document is its own authority there, so that class
  is proposed as D3-mechanical. Method: the four Opus batches verified 14–23
  claims per doc, the ten Sonnet batches 3–7, so the May–July plans (batches
  03–06, 88 docs) had the shallower pass; a deeper re-pass there is an
  option, not scheduled. The D4 report lives beside the aggregate in the
  scratchpad and was handed to Mark the same day. D3 still waits on
  `DOC_README.md`, dirty under the other session, whose in-flight edits also
  touch six flagged docs (standards survey, event-DAG brief, reachability
  rungs, graphshell reference host, search surface wiring, djinn resident
  services), so those findings may already be addressed in the working tree.

- **2026-09-02 (D2 depth re-pass landed, decision 12)**: batches 03–06
  re-run on Opus, independently of the Sonnet files, which are kept beside
  them. The aggregate now carries the deeper pass: 4,346 claims checked (was
  2,556), 3,091 hold, 1,144 stale (26%, was 19%), 113 unverifiable; 141
  contradictions (was 79). Dispositions: 127 current, 76
  historical-unmarked, 61 historical-marked, 17 dead. On the same 88 docs,
  Sonnet had checked 330 claims and found 118 stale; Opus checked 2,140 and
  found 786, and changed 29 of 88 dispositions — among them three
  `dead`→`historical-unmarked`, six new `dead`, and several
  `historical-marked`→`historical-unmarked` where the existing 2026-06-09
  audit note was itself found stale (it maps `mere-identity`→
  `persona/identity`, since renamed again to `dramatis/personae`). **Method
  finding to carry forward: on identical inputs and the same protocol,
  Sonnet under-checked by roughly six and a half times and reached a
  different disposition on one doc in three; use Opus for verification
  batches, Sonnet for triage.** House banner form confirmed from the tree
  (a dated audit note as a blockquote under the title, layered rather than
  replaced). The D4 report and the banner list (ruling 10, 31 docs)
  regenerated from the deeper aggregate and re-delivered; the dead list
  (ruling 9) changed membership and is re-delivered with it.

- **2026-09-02 (scoping for rulings 9–10 and the escalations)**: Mark asked
  for the dead list, the banner list and the escalations to be scoped rather
  than read raw. **Dead**: seventeen cases written with evidence, open points
  and a recommendation; extraction target confirmed as the archived-plan-tails
  plan, whose own header names it the single backlog for every pass; fifteen
  archive, one retire, one reclassified live — the kith plan, whose gate
  cleared 2026-08-09 and which `crates/mesh/mesh/src/lease.rs:22` still
  names as owing. **Banners**: thirty-one sorted — twenty banner with per-doc
  text in the tree's own dated-audit-note form (second layer where a
  2026-06-09 note exists, never replacing it), eight archive instead as
  completed records, one two-line repair, two held. **Escalations**: an Opus
  evidence pass over twelve questions across mere, turnstone and genet found
  nothing built under another name — five never built, four partial, one
  superseded, one built-then-deleted. Nine of the twelve resolve through two
  demolitions no plan records: `c5f01064` and genet `55c05d11759` (Stylo and
  `genet-layout` retired 2026-08-21, which also took the parallel-cascade
  thesis's mechanism with it). Both left producers without consumers:
  `GraphTableStats`, `PersonaSettings.menu_actions`/`command_usage`,
  `ToastSpec`, `BarnesHutRepulsion`. Seven secondary escalations settled by
  lookup, including one code breach — `ports/distillery/probe/session-fixture/
  Cargo.toml:21` pins `sha2 = "0.10"` against the ruled 0.11 row — and one
  product gap: turnstone has no `register-theme` dependency, so the T1–T5
  theme-mode machinery has no consumer. The standards-survey index
  discrepancy is the other session's uncommitted working copy, not a lost
  commit. Three scoping documents and the evidence file delivered.

- **2026-09-02 (ruling 9 executed: all seventeen archived)**: Mark ruled
  archive all, overriding the scoping's one NOT-DEAD (kith) and one RETIRE
  (the layout probe) — both archived with their re-scope notes carried.
  `git mv` of the seventeen into `archive_docs/2026-09-02_retired_plans/`, a
  new checkpoint suffix because these are retired, not completed. Per core §5,
  73 inbound links repointed across 35 files in the same pass, six of them in
  older archive folders that also pointed at these plans. Per core §6, the
  seventeen `DOC_README.md` entries repointed and annotated "Archived
  2026-09-02" — that file was still dirty under the other session, so the
  edits were read-modify-write on disk and compose with theirs; noted as a
  risk, not a collision. Open points extracted to the archived-plan-tails
  plan's new 2026-09-02 section, the tree's own precedent for tails. Not
  committed. The proposed Sonnet/Opus method memory was declined ("won't age
  well") and not written.

- **2026-09-04 (D3 checker restored and live tree remeasured):**
  `scripts/mere_doc_audit.py` now makes the D3 mechanical gate reproducible in
  the repository. Its self-test plants a defect in each reported category and
  then proves a clean fixture reaches zero. Against today's active tree it
  initially reported 297 documents, 18 index orphans, one private-memory link,
  31 plans without a Status line, 372 broken relative links, and 726 missing
  known-root path citations. The bounded pass brought the first three defect
  classes to zero, repaired three confirmed current links, and hardened the
  checker against angle-bracket placeholders, bare-memory prose, ambiguous
  bare roots, glob syntax, and versioned protocol identifiers while continuing
  to report those exclusions. The current tree reports 298 active documents,
  347 broken relative links, and 544 concrete missing known-root paths. The
  remaining clean-doc link set did not yield an unambiguous live retarget in a
  319-finding review; it is historical-disposition work, not a bulk rewrite.
  D3 therefore remains open behind banners and archival judgment.

- **2026-09-05 (plan-review checker correction):** the restored checker's
  `STATUS_LINE` expression rejected existing dated labels such as
  `**Status (2026-09-04):**`. The review found 17 such headers among its 31
  reported Status-less plans; those were syntax false positives, not absent
  status records. The checker now accepts plain, bold, dated, and reconciled
  labels, and its self-test includes those real forms plus prose/non-header
  negative controls. The self-test and Python compilation pass. Concurrent D3
  work has also normalized headers and repaired index coverage, so subsequent
  totals must not be attributed to the parser correction alone. The separate
  missing-path and relative-link counts still require judgment for historical
  paths, placeholders, glob examples, and symbol-qualified code citations.

- **2026-09-05 (D3 safe disposition slice):** a live-tree review grouped the
  176 source documents with findings into 36 already historical or superseded,
  nine unequivocally historical but unmarked, seven current documents with
  bounded repair candidates, and 124 ambiguous documents requiring manual
  review. The nine historical documents now carry explicit banners, and six
  exact live references were repaired across the terminology, Cambium catalog,
  and Woodshed/Scenograph documents. The checker reports 298 active documents,
  zero orphans, zero ghosts, zero private-memory links, zero Status-less plans,
  347 broken relative links, and 538 concrete missing known-root paths. D3 stays
  open because the remaining findings have not all been shown to sit in
  historical documents; ambiguous donor history, planned seams, symbol-qualified
  citations, and cross-repository references were deliberately left unchanged.

- **2026-09-05 (D3 three-shard reference review):** Luna/Terra reviewers
  inspected every source document still reported after the safe disposition
  slice. Confirmed archive targets and live moved owners were repaired; guessed
  successors, removed components, planned seams, and symbol-only references were
  left alone. Commit `e620e8f6` was re-audited from an isolated worktree beside
  the sibling repositories: 297 active documents, 296 indexed documents, zero
  orphans, zero ghosts, zero private-memory links, zero Status-less plans, 322
  broken relative links, and 517 concrete missing known-root paths across 154
  source documents. The remaining findings are dominated by historical donor
  paths and ambiguous ownership. D3 stays open until each remaining source is
  either explicitly historical or its current citations are proven.

- **2026-09-06 (D3 closed):** a clean beside-siblings worktree reran the
  complete active tree through the hardened checker. Exact archive/live-target
  repairs landed first. Mere's policy addendum and checker then adopted visible,
  occurrence-scoped `historical-*` and `planned-*` annotations, rejecting
  detached, malformed, wrong-kind, and newly resolving markers; planted
  controls cover those failure modes, Markdown code labels, inline literals,
  nested-list links, and longer closing fences. Final receipt: 297 active docs,
  296 indexed plus the index itself, and zero orphans, ghosts, private-memory
  links, Status-less plans, broken relative links, missing known-root paths,
  invalid annotations, or stale annotations. Informational inventory: 267
  historical links, 474 historical paths, 2 planned links, and 15 planned
  paths; 112 ambiguous-root examples, 37 glob/pattern examples, and 30
  versioned protocol identifiers remain explicitly excluded. The self-test,
  Python compilation, and `--fail-on-findings` gate pass. This closes D3 and
  the platform-boundary plan's P7 dependency. It does not close this plan's
  overall phase-D condition: D2's aggregate covers the 281-document 2026-09-02
  snapshot, while the active tree now has 297 documents. The 16 additions need
  D2 judgment blocks before phase D can be called complete.

### Finding: genet now has two doc homes

Recorded because it is a known cost of decision 4, not an oversight.
`genet/design_docs/` (8 docs, governed, indexed) sits beside `genet/docs/`
(~163 docs, ungoverned, unindexed). The boundary is **date and governance, not
subject matter**, and it is stated plainly in genet's policy addendum so the
next person does not have to infer it. New docs go to `design_docs/`.

Merging them is the obvious end state and was **deliberately not attempted**:
163 files plus **49 references to `genet/docs/` from 32 files outside genet**,
each needing repair under core §5 — and thirteen files inside `docs/` are
currently dirty from another session. Doing it badly is worse than the split.

### Finding: 485 broken link targets already in mere's doc tree

Surfaced by the link checker written for B2, and **not caused by this work** —
each was verified present in `HEAD` before any move.

**Corrected upward 2026-08-24 by an independent audit.** This section first
reported 364, measured over a narrower file set than it claimed. The real
figure across all 386 markdown files under `mere/design_docs/` is **485
distinct broken targets across 806 occurrences**, out of 2809 local targets —
431 occurrences in `archive_docs/`, 345 in `mere_docs/`. The correction does
not change the conclusion, but the original number was quoted as if it covered
the whole tree and it did not. The breakdown below is from the original,
narrower pass and is indicative rather than complete:

| Cause | Count |
|---|---|
| cross-repo relative links (`../../…`) | 128 |
| the archived graphshell repo | 32 |
| the deleted meerkat crate | 27 |
| links into Claude memory files | 9 |
| genet paths | 6 |
| miscellaneous | ~160 |

The 128 cross-repo relative links are precisely what core §5 now forbids, and
the graphshell and meerkat entries are what §5's rot warning describes. This is
a real cleanup lane and it is **not scheduled here** — it is recorded so the
next person does not rediscover it. Much of it sits in `archive_docs/`, where
rot is arguably acceptable, so any sweep should scope active docs first.

### Finding: an independent audit found four defects this pass missed

Run 2026-08-24 after A, B and C had landed. Worth recording both what it found
and why the original checks did not.

**The cross-repo citations were invisible to the link checker.** Core §5 says
cross-repo references are path citations rather than links, so the rewrite pass
turned them into inline code spans. A code span is not a markdown link, so the
`](path)` checker that verified every tree could not see them — it reported
clean while five citations pointed at nothing. Four of those named the exact
`mere/design_docs/nematic_docs/` paths that phase C then deleted: C2 wrote them
before C3 moved those documents to genet, and this side was never re-swept.
Fixed in smolweb; one remains in genet, which is an active tree.

**The committed index contradicted itself.** `DOC_README.md` carried both
"C3 open … genet has no `design_docs/` to receive them" and "moved to genet
2026-08-24 … all eight now live in `genet/design_docs/`". The first was written
before the decision and never revised after executing it.

**Two counts were wrong**: fourteen repos where there are fifteen, and the
broken-link figure above.

**A rule worth keeping:** a checker that reports zero is only evidence if it
would have reported non-zero. The audit found its own first checker silently
dead — a heredoc had mangled its character classes — and thereafter gated every
negative behind planted defects it had to catch first. The link checker this
plan relied on had a real blind spot and reported clean anyway.

### Open risk: genet's design_docs is untracked while mere's deletion is committed

`db9c613c` permanently removed the eight documents from mere. Their only copy
now lives in genet's **working tree**, untracked — `git clean` there would
destroy them, recoverable only from `db9c613c^`. smolweb was in the same state
and is now committed; genet is not, because a session is actively working in
that tree and its `design_docs/` also holds that session's in-flight fleece
work. Committing it is theirs to do, not ours. Flagged rather than fixed.

### Finding (2026-09-02): the active tree, measured

D1, verified against `bcb222ce` with the eight docs then dirty under another
session read from HEAD and one untracked brief skipped. 281 active docs, 135
of them plans.

- **Index (§6)**: 17 orphans, 0 ghosts. The orphans include a root brief the
  projection grammar catalog names as owning its scene judgments
  (`2026-08-23_projection_scenes_and_graph_native_platform.md`), the
  graphshell reference host plan, six `mere_docs/testing/` receipts, and a
  plan filed under `technical_architecture/`
  (`2026-08-14_tactile_tier_plan.md`).
- **Plans (§8)**: 39 self-describe as complete or landed yet sit in the
  active tree — roughly twenty are fully done, the rest partial. 13 have no
  Status line at all. Of 37 that read as open, about fifteen were last dated
  May–June 2026, including one for the deleted meerkat crate.
- **Links (§5)**: 293 broken relative links in the active tree. The
  2026-08-24 figure of 806 occurrences said the bulk sat in `archive_docs/`;
  a third does not. 19 links across six docs point into the Claude memory
  directory (`lexicon_brief` 4, `protocol_architecture_plan` 8,
  `auto-update_brief` 2, `identity-vault-ssh-agent_plan` 2,
  `memory_tiers_brief` 1, `MURM_AS_BILATERAL` 2). 99 cross-repo relative
  links, 44 of them in the graphshell harvest brief.
- **Code-font paths**: 495 cited, roughly 260 absent at HEAD after
  correcting 13 `components/` paths the checker had tested against the wrong
  root (they resolve in genet). The absent set is dominated by the deleted
  meerkat (20 citations), pre-rename `crates/persona` *(historical citation)* <!-- doc-audit: historical-path --> (12) and
  `crates/orrery` *(historical citation)* <!-- doc-audit: historical-path --> (9), and the old `session-runtime`. Only 8 of the 138
  citing docs carry the rename-key banner that makes such references
  deliberate; 130 do not. Live breaks worth naming: `ports/retinue-agent/*`
  (six citations from the reference host plan),
  `ports/graphshell/src/bin/graphshell_device_host.rs` *(historical citation)* <!-- doc-audit: historical-path --> (replaced by djinn,
  per the resident consolidation plan), `ports/pelt`,
  `ports/knot/src/editor.rs` *(historical citation)* <!-- doc-audit: historical-path -->, and five genet paths under the removed
  `genet-layout`.
- **Commit receipts**: 361 hashes cited; 348 resolve in some local repo. Of
  the 13 that do not, three are model revisions rather than commits; the ten
  real ones cluster in the illume text-lexer plan, plus `03661ce` cited by
  `DOC_README.md` itself.

**Method and blind spots**, per the rule recorded above that a zero is only
evidence if the checker would have reported non-zero. Every axis reported
non-zero except ghosts; the ghost check shares the resolver that found the
293, so its zero is credible. Known blind spots: only inline markdown links —
bracketed text followed by a parenthesised target — are seen (no
reference-style links, no bare URLs; and a literal example of that syntax in
prose reads as a link, which this very sentence tripped on 2026-09-02); the code-font path check
requires a known leading segment (`crates/`, `ports/`, **apps/**,
`components/`, `src/`, `design_docs/`, `repos/`, `tests/`, `examples/`), so
paths under other roots are unchecked, and bare `src/`, `tests/`,
`examples/` paths are ambiguous without crate context and over-count; the
hash check accepts a hash resolving in *any* local repo, so a hash
attributed to the wrong repo passes; plan Status classification is by regex,
and its 45-doc "unclear" bucket was not hand-sorted in D1. The checker lives
outside the tree (`mere_doc_audit.py`, scratchpad) and is rerun to close D3.
