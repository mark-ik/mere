# Documentation Policy

> **Canonical core v1 (2026-08-24).** Everything from "## Core principles" down
> to the end of §10 is the shared core, copied verbatim into every repository
> under `Code` that keeps a `design_docs/`. It is not owned by any one repo:
> change it in all of them or not at all, so that `diff` between any two copies
> shows only local addenda. Repo-specific rules belong under
> **Local addendum** at the foot of this file, never inside the core.

## Core principles

### 1. Control doc growth

Add to an existing doc unless the material is substantial (>500 words), covers
a distinct topic, and is unrelated to any current document. Keep the total doc
count low. Do not create a file for a one-time analysis.

### 2. Eliminate redundancy

Audit before commits and after substantial changes. Newer documents are
generally more authoritative. If two docs disagree, reconcile them — do not let
drift accumulate. Material shared across several repos lives once, in a named
home, and is cited by path from the others; never copied.

### 3. No legacy friction

When a path changes, optimize for clean fit with the new path. Do not preserve
obsolete parallel systems or migration shims unless they are needed for real
user data. Tests track current semantics only.

### 4. Location and archival

- **Active docs** live directly in `design_docs/`. Flat is fine, and is the
  right default. When one domain accumulates enough material to justify it,
  promote that domain to an area root, `design_docs/<area>_docs/`.
- **Area roots**, once a repo has them, take a consistent set of category
  subdirectories. Use only the ones a given area needs:

  | Category | Holds |
  |---|---|
  | `research/` | briefs, surveys, reports, critiques, design probes |
  | `technical_architecture/` | component definitions, boundaries, interfaces, decisions |
  | `implementation_strategy/` | dated plans, development approaches, roadmaps |
  | `design/` | UI/UX, interaction design, accessibility |
  | `testing/` | test plans, harness docs, manual checklists |

- **Docs live with the repo that owns the subject, at that repo's doc root.**
  Do not scatter `design_docs/` into member crates of a workspace: a doc in a
  member crate is invisible to the canonical index, which is a violation of §6
  rather than a matter of taste.
- **Archive**: `design_docs/archive_docs/<YYYY-MM-DD>/` for retired plans and
  superseded notes. Check for an existing checkpoint folder before creating a
  new one. Move rather than delete; delete only with rationale and
  confirmation.

### 5. Cross-referencing

- Within a repo: relative links.
- Across repos: cite by path (`isometry/design_docs/...`), since relative links
  do not cross repository boundaries reliably and rot silently when the
  neighbour moves or is archived.
- Crates: link to crates.io when referring to a public API
  (`https://crates.io/crates/<name>`).
- When a doc moves, repair the links that pointed at it in the same session.

### 6. DOC_README authority

`design_docs/DOC_README.md` is the sole canonical index. It must contain:

- AI-assistant working principles for this project
- An index of all active docs with one-line descriptions
- Pointers to `DOC_POLICY.md` and `PROJECT_DESCRIPTION.md`

Any doc added, moved, or removed requires a `DOC_README.md` update in the same
session. If any other index disagrees with `DOC_README.md`, `DOC_README.md`
wins.

### 7. PROJECT_DESCRIPTION.md ownership

**design_docs/PROJECT_DESCRIPTION.md** — inside the doc root, not at the
repository root — is reserved for the maintainer. Do not edit it without
explicit instruction. Treat it as authoritative and surface contradictions for
discussion rather than resolving them silently.

The root `README.md` is derived from `PROJECT_DESCRIPTION.md` and the current
authoritative docs. Speculative features without plans appear only in
`PROJECT_DESCRIPTION.md`.

### 8. Plan documents

Work that changes code — not doc-only work — gets a dated plan named
`<YYYY-MM-DD>_<keyword>_plan.md`, in `design_docs/` or, where the repo has area
roots, in `<area>_docs/implementation_strategy/`. Each plan carries:

- A dated **Status** line, kept current: plan, in progress, landed, superseded
  by X.
- **Phases** organised by feature target and validation criteria, each with
  **done-conditions**. Never calendar labels — no "Day 1", no "Week 2" — and
  never time estimates.
- A **Findings** section for facts verified during the work, dated, with code
  references.
- A **Progress** log, dated, appended as phases land.

Code samples in a plan state whether they are illustrative or compile-ready.

Update the plan every two prompts on the project, or every two completed tasks.
Re-read it before resuming work rather than working from memory of it. On
completion, extract any deferred or still-open points into a new or existing
plan *before* moving it to `archive_docs/<date>/`.

### 9. Implementation feedback loop

Every implementation pass is also a design probe. After each pass, disseminate
structural learnings to the relevant plans and docs in the same session.
Surface architectural problems explicitly in the plan even when the fix is
deferred.

### 10. Workflow rule for AI assistants

Read `DOC_README.md` first, then this policy, before starting work. Any durable
working principle learned during a session is promoted into `DOC_README.md`'s
working-principles section in that same session.

## Local addendum — Mere

Mere is the largest doc tree in the workspace and the only one with area roots,
so §4's promotion rule is fully exercised here.

### Active-tree citation audit

Preserve an obsolete link or known-root path only when it is evidence rather
than navigation. Mark that exact occurrence on the same line with visible
`*(historical citation)*` text followed by
`<!-- doc-audit: historical-link -->` for a Markdown link or
`<!-- doc-audit: historical-path -->` for a code-font path.

A deliberately not-yet-existing target uses visible `*(planned target)*` text
with `<!-- doc-audit: planned-link -->` or
`<!-- doc-audit: planned-path -->`. Use this only when the document commits to
creating that exact target. These markers do not make the whole document or
section historical or planned. Mere's active-tree audit fails detached,
malformed, wrong-kind, and newly resolving annotations.

### Active-tree judgment coverage

Every active Markdown document must have a D2 judgment record. The durable
ledger lives under `support/doc-audit/d2/`: the original snapshot aggregate is
retained unchanged, later documents receive parser-shaped supplemental batch
records, and archived records remain as history. Run
`python scripts/mere_doc_judgment_audit.py` whenever an active document is
added, moved, or archived; the command must report full current-path coverage.

### Area roots

Corrected 2026-08-24. The previous list named `platen_docs/`, which has never
existed, and omitted `eidetic_docs/`, which does.

```
mere/design_docs/
├── DOC_README.md                  ← canonical index (§6)
├── DOC_POLICY.md                  ← this file
├── TERMINOLOGY.md                 ← canonical terms
├── YYYY-MM-DD_<keyword>_brief.md  ← cross-cutting briefs
├── mere_docs/                     ← product-level concerns
├── armillary_docs/                ← the armillary crate
├── cambium_docs/                  ← the Cambium desktop-host/scene family
│                                     (Meristem, Cambium, Sprigging, Workbench)
├── dramatis_docs/                 ← identity and contacts (personae, gaz)
├── eidetic_docs/                  ← the memory stack (Eidetic codicils,
│                                     muniment journals, chartulary + RDF)
├── inker_docs/                    ← the engine controller (inker,
│                                     document-canvas, the engine adapters)
├── intel_docs/                    ← embedding and inference (esp)
├── moothold_docs/                 ← community / federation
├── murm_docs/                     ← bilateral comms
├── nematic_docs/                  ← the smolweb engine and knot composition
│                                     (nematic, illume, errand)
├── scenograph_docs/               ← the scene contract
├── verso_docs/                    ← rendering surfaces and the engine flip
│                                     (verso-tile)
└── archive_docs/                  ← superseded checkpoints
```

**`cambium_docs/` founded 2026-09-03, ruled by Mark**, the day Cambium itself landed in this repository (platform boundary plan, P2). It collects two things that had scattered: the two live plans (`workbench_component_plan`, `host_ui_zoom_plan`) that moved with the code into `mere_docs/` as a two-document lodger, and `crates/cambium/docs/` *(historical citation)* <!-- doc-audit: historical-path -->, which was the member-crate scatter §4 forbids — the same defect the 2026-08-24 collapse fixed for the other nine `crates/*/design_docs/` directories.

Every root above corresponds to code that lives in this repository. That is an
invariant now, not a coincidence — see below.

**Three roots left on 2026-08-24 and came back on 2026-09-03.**
`inker_docs/`, `nematic_docs/` and `verso_docs/` described
`components/{inker,nematic,verso-tile}`, which had lived in genet since the
adoption; the docs were orphaned here and went to `genet/design_docs/` with
their subject. On 2026-09-03 the engine-management layer itself moved to this
repository under the platform boundary plan's P3, so the three roots travelled
back with it, at the same names and with their history — seven documents, one
fewer than the eight that left, because genet archived the Knot
evaluation/export plan on 2026-09-02 and it stays in genet's `archive_docs/`.
They are indexed only here now; genet's index carries a four-line note saying
where they went. Two further documents — the smolweb home decision and the
carrier-independence analysis — went to `smolweb/design_docs/` in the
2026-08-24 pass, being spec-level rather than implementation-level, and did not
travel back.

That round trip is the invariant above working, not failing: an area root
follows its subject, in both directions, in the same session as the code.

**The rule this leaves behind:** when code moves out of this repository, its
docs move with it in the same session. An area root describing code that lives
elsewhere is the failure this pass cleaned up, and core §4's "docs live with
the repo that owns the subject" is the general form of it. Track the work in
the [doc policy consolidation plan](mere_docs/implementation_strategy/2026-08-24_doc_policy_consolidation_plan.md).

**Member-crate scatter was collapsed 2026-08-24.** Nine `crates/*/design_docs/`
directories held 20 documents, 18 of which `DOC_README.md` did not index. They
now live under the area roots above. Core §4 forbids reintroducing them.

### PROJECT_DESCRIPTION.md

Mere has no **design_docs/PROJECT_DESCRIPTION.md**. Core §7's derivation rule is
therefore inert here rather than violated; founding one is open work.

### Inheritance and migration (graphshell donor)

The donor graphshell repo was **GitHub-archived on 2026-05-27** (read-only at <https://github.com/mark-ik/graphshell>; local clone deleted). Its design docs are no longer a local sibling. Before archiving, all 633 donor docs were swept into two curated indexes that are now the entry points for any remaining pull: the [full docs harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md) (what to pull, where it lives in the donor, which mere domain wants it) and the [concept brief](mere_docs/research/2026-05-17_graphshell_harvest_brief.md). Treat those indexes as canonical; fetch detail from the GitHub archive when a slice needs it.

The Graphshell name was reclaimed on 2026-07-22 for the family-wide remote
projection host, and on 2026-07-23 the
[repo consolidation plan](mere_docs/implementation_strategy/2026-07-23_repo_consolidation_plan.md)
ruled Graphshell as Mere's shell and remote port, homed in this repository.
Graphshell documentation belongs here under `mere_docs` (a dedicated
`graphshell_docs/` area-root may be founded when volume warrants). The
archived donor and the current Graphshell must remain explicitly distinct in
links and prose.

When pulling a donor doc's content into a mere doc:

1. Place it in the appropriate `<area>_docs/<category>/` directory here
2. Update terminology to current Mere-aligned vocabulary (per `TERMINOLOGY.md` / `2026-05-04_lexicon_brief.md`)
3. Add the new file to `DOC_README.md` index
4. Cite the donor source by its GitHub-archive path (the original is read-only; it cannot be edited or deleted)

### Trademark and brand notes

- **Mere** — product name (humble; "merely a browser!")
- **Merely** — parent brand layer (adopted 2026-07-09, was Strophos; confirmed 2026-07-10 after a challenge round, see the lexicon brief's naming history). GitHub org **merely-made**, registered 2026-07-10 (bare `merely` taken).
- **Crate names**: `mere`, `graphshell`, `verso-tile`, `inker`, `platen`, `nematic`, `murm`, `murmuring`, `moothold`, `mooting` (all reserved on crates.io 2026-05-04); `eidetic` (2026-05-07), `illume` (2026-06-27), `armillary` (2026-07-03) published at 0.0.x from the workspace; `errand` 0.1.0 published 2026-07-04 from its standalone repo (0.1.1/0.1.2 same day: guppy, then spartan/nex/misfin-send, delegated to the spec crates). Three smolweb protocol crates published 2026-07-04 from standalone repos, misfin-shaped, MIT: `spartan-protocol`, `nex-protocol`, `guppy-protocol` (bare names taken by unrelated projects; qualified per ecosystem convention). `misfin` promoted 2026-07-03 to the standalone [mark-ik/misfin](https://github.com/mark-ik/misfin) repo (spec-complete; name held in stewardship, transfers to the protocol author on request; 0.0.2/0.0.3 published 2026-07-04, MIT; 0.0.3 adds selectable TLS providers). `servitor` reserved 2026-07-17 (0.0.1 placeholder now lives at `crates/servitor`, MIT/Apache; the resident-helper unit per the participant gate + packs plan). `forme` was lost to an unrelated claimant (2026-02) — needs a new name if ever published.
