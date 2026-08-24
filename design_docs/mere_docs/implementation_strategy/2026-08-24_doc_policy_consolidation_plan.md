# Doc Policy Consolidation Plan — one canonical core, facade-collapsed areas, spec docs repatriated

**Date**: 2026-08-24
**Status**: **complete 2026-08-24 — A, B and C all landed.** Decisions taken by
Mark 2026-08-24 (four questions, recorded in §Decisions). Phases A and B are
mere-local; phase C touched smolweb and genet.

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
that no longer live in mere. `crates/inker`, `crates/nematic` and `crates/verso`
are all absent; the code is now `genet/components/{inker,nematic,verso-tile}`.

### 18 of 20 member-crate docs are invisible to the canonical index

Nine `crates/*/design_docs/` directories hold 20 docs. `DOC_README.md` — which
policy §7 declares the sole canonical index — indexes only two of them
(muniment's). This is a standing violation of the repo's own rule, and it is
the actual argument for phase B; ease of access is secondary.

| Directory | Docs | Facade member? |
|---|---|---|
| `crates/armillary/design_docs` | 1 | no — standalone crate |
| `crates/dramatis/gaz/design_docs` | 1 | yes |
| `crates/dramatis/personae/design_docs` | 3 | yes |
| `crates/eidetic/chartulary/design_docs` | 3 | yes |
| `crates/eidetic/codicil/design_docs` | 1 | yes |
| `crates/eidetic/muniment/design_docs` | 2 | yes |
| `crates/eidetic/scholia/design_docs` | 1 | yes |
| `crates/intel/esp/design_docs` | 6 | yes (2 subdirs) |
| `crates/scenograph/design_docs` | 2 | no — already facade-level |

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
  keep it at `design_docs/PROJECT_DESCRIPTION.md`. Core §7 now names the path
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
  genet the code is `components/inker/...`, and nematic is a *top-level*
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
