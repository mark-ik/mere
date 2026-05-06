# Documentation Policy

Governs all documentation under `mere/design_docs/`. Mirrors the spirit of the inherited [`graphshell/design_docs/DOC_POLICY.md`](../../graphshell/design_docs/DOC_POLICY.md), simplified for the smaller current state.

## Directory structure

Per-component subdirectory under `design_docs/` for each major architectural area:

```
mere/design_docs/
├── DOC_README.md                  ← canonical index
├── DOC_POLICY.md                  ← this file
├── TERMINOLOGY.md                 ← canonical terms (skeleton)
├── YYYY-MM-DD_<keyword>_brief.md  ← cross-cutting briefs
├── mere_docs/                     ← product-level concerns
├── graphshell_docs/               ← shell layer + host GUI
├── verso_docs/                    ← rendering-surface management
├── inker_docs/                    ← engine controller
├── platen_docs/                   ← composition surface
├── nematic_docs/                  ← smolweb engine
├── murm_docs/                     ← bilateral comms
├── moothold_docs/                 ← community/federation
└── archive_docs/                  ← superseded checkpoints
```

Within each area-root:

```
<area>_docs/
├── implementation_strategy/   ← dated plans, feature targets
├── technical_architecture/    ← canonical architecture decisions
├── research/                  ← briefs, surveys, design probes
├── design/                    ← UI/UX docs (where applicable)
└── testing/                   ← test plans, harness docs
```

## Core principles

### 1. Control documentation growth

Prefer adding to existing docs over creating new ones. Create a new doc only when material is substantial (>500 words), covers multiple sub-topics, and is unrelated to existing docs. Do not create files for one-time analyses.

### 2. Eliminate redundancy

Periodic audits before any major change. Newer documents are generally more authoritative. Move superseded material to `archive_docs/` checkpoint folders rather than editing in place.

### 3. No legacy friction

When a new architecture path is chosen, optimize for clean fit rather than preserving legacy. Replace, don't half-migrate. Keep fallbacks only when they provide architectural safety, not to keep obsolete parallel systems alive.

### 4. Archival

`archive_docs/` contains superseded material in dated checkpoint folders. When archiving, check for an existing checkpoint folder first; create a new one if needed (named by date of last edit of any file in the checkpoint).

### 5. Cross-referencing

- Within `mere/design_docs/`: relative links
- Across to inherited `graphshell/design_docs/`: explicit relative paths up two levels (`../../graphshell/design_docs/<file>`)
- Crates: link to crates.io when referring to public API (`https://crates.io/crates/<name>`)

### 6. Categories

Standard subdirectory categories within an area-root:

1. **research**: briefs, reports, critiques, reviews. General/technical-architecture resources.
2. **technical_architecture**: core component definitions, boundaries, interfaces, integration points. General/implementation-strategy resources.
3. **implementation_strategy**: dated plans, development approaches, feature-gated roadmaps. General/design resources.
4. **design**: UI/UX docs, interaction design, accessibility. General/testing resources.
5. **testing**: automated tests, manual checklists, performance targets.

### 7. DOC_README authority

`DOC_README.md` is the sole canonical index. Any doc add/move/remove must include a same-session `DOC_README.md` update.

### 8. Plan documents

Tasks that change code (not just docs) get a dated plan file:

```
mere/design_docs/<area>_docs/implementation_strategy/YYYY-MM-DD_<keyword>_plan.md
```

Plan structure:
- **<Keyword> Plan**: phases and progress
- **Findings**: research and findings
- **Progress**: session log and test results

Update the plan every two prompts on the project, or every two completed tasks. Move to `archive_docs/` upon completion.

### 9. Implementation feedback loop

Every implementation pass is also a design probe. After each implementation pass, disseminate structural learnings to the relevant plans/docs in the same session. Surface architectural problems explicitly in the plan even if the fix is deferred.

## Inheritance and migration

The inherited [`graphshell/design_docs/`](../../graphshell/design_docs/) remains authoritative for content not yet migrated here. Pull content over incrementally as it becomes relevant to active work in this workspace. Until migrated, the original location is canonical.

When migrating a doc:
1. Place in the appropriate `<area>_docs/<category>/` directory here
2. Update terminology to current Mere-aligned vocabulary (per `TERMINOLOGY.md` / `2026-05-04_lexicon_brief.md`)
3. Add the new file to `DOC_README.md` index
4. Either delete the original (with rationale + confirmation) or leave it with a `> Superseded by [link]` header

## Trademark / brand notes

- **Mere** — product name (humble; "merely a browser!")
- **Strophos** — parent brand layer
- **Crate names**: `mere`, `graphshell`, `verso-tile`, `inker`, `platen`, `nematic`, `murm`, `murmuring`, `moothold`, `mooting` (all reserved on crates.io 2026-05-04)
