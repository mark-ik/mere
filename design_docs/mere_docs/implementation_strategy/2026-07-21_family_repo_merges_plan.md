# Family Repo Merges Plan (eidetic + conatus)

**Status:** P1-P3 and the `mere-eidetic` publish follow-on landed; donor GitHub archiving and later published-metadata refreshes remain pending.

2026-07-21. Ecosystem repo reorganization: group satellite single-crate repos
into family repos by concern, where the family is a true lockstep stack.
Decided with Mark in session; both merges executed same day.

## Plan

- **P1. conatus** (physics family): merge repos numen + quint + seiche into
  `mark-ik/conatus`, one workspace, histories preserved. **DONE.**
- **P2. eidetic** (durable-memory family): merge repos muniment + codicil +
  chartulary + scholia into `mark-ik/eidetic`, same shape. **DONE.**
- **P3. Consumer repoint**: mere, turnstone, hocket, isometry, woodshed,
  servitor onto the family repos (manifest URLs, path deps, local `.cargo`
  patches, lockfiles). **DONE** (this session; per-consumer detail below).
- **P4. Follow-ons**:
  - **DONE 2026-07-22 — mere-eidetic rename + publish.** mere's
    `crates/eidetic/*` lane renamed to **mere-eidetic** (Mark's ruling: the
    eidetic name belongs to the family; the mere lane takes the prefix). Done
    the low-churn way so it did NOT ripple through the workspace: the published
    package names became `mere-eidetic` + `mere-eidetic-{fjall,https-fetcher,
    iroh-fetcher,search}`, but each crate keeps its old `[lib] name`
    (`eidetic`, `eidetic_fjall`, …) and mere's `[workspace.dependencies]` keep
    their `eidetic*` keys via `package = "mere-eidetic*"` — so all `use
    eidetic::` sites, consumer manifests, the `image-store-check` probe, and
    the `crates/eidetic/` directory are untouched. turnstone's two direct git
    deps + its gitignored `.cargo` patch keys updated the same way (patch keys
    are package names, so they became `mere-eidetic*`). **`mere-eidetic` 0.0.1
    published to crates.io** (mirrors the old `eidetic` 0.0.1). The bare
    **`eidetic` 0.0.1 stays as an orphaned reservation** — free for a future
    family facade. Companions were never published, so their rename just
    reserves the `mere-eidetic-*` names by manifest (not yet published).
    Verified: `mere-eidetic` lib 85 tests green, consumers + turnstone build.
  - GitHub-archive the seven donor repos (numen, quint, seiche, muniment,
    codicil, chartulary, scholia) with tombstone READMEs pointing at the
    family repos; delete the local checkouts after. Left for Mark.
  - Next `cargo publish` of each crate picks up the new `repository` field
    (update each crate's `Cargo.toml` repository URL at that time; not
    changed preemptively so the published metadata keeps matching the live
    published versions).
  - crates.io `eidetic` (0.0.1, Mark's reservation) still carries the
    mere-lane description; reword at next publish.

## Findings

- **Bucket analysis** (the reorg's scope ruling): of 31 repos, only two
  satellite groups are true lockstep families. Everything else keeps its
  posture: engines (mere, genet, cambium, netrender) as-is; apps (turnstone,
  isometry, hocket, woodshed) as-is; deliberate standalones (wgpu-graft,
  wgpu-weld, wgpu-scry, misfin, wavicle, personae, armillary, netfetcher,
  vates, sibylla, servitor) as-is. **retinue + tulle + tucket + sennet
  stay separate repos: declined by Mark for legal reasons** (provenance
  boundary; MeshCore source-readable vs Meshtastic clean-room must remain
  independently auditable).
- **Names**: family repos take fresh names rather than the top crate's name.
  **eidetic** for the memory family (Mark: the name was always meant for the
  memory/recording/consolidating role). **conatus** for the physics family
  (cause-side word: the instantaneous striving that integration turns into
  motion, Leibniz; covers inertia/perseverance via Spinoza and goal-free
  motion like a seiche's oscillation). **telotaxis** stays banked for a
  future goal-seeking/steering layer; it names stimulus-steered goal-directed
  behavior only.
- All seven crate names plus bare `eidetic` were already Mark's on crates.io,
  so no claims were needed; `conatus` and `telotaxis` verified free 2026-07-21
  (conatus repo name used; neither crate name claimed yet).
- Merge mechanics: `git subtree add --prefix=crates/<name> <local-repo> main`
  preserved each crate's full history in the family repo. Intra-family deps
  converted to `path` + `version` (publishable). Member lockfiles were never
  tracked; each family has one workspace lock.
- Cargo resolves git deps by crate name inside a workspace repo, so consumer
  git URLs simply swap to the family URL; `branch`, `version`, and `features`
  fields survive unchanged.

## Progress

- conatus: founded, 3 subtree merges, path-linked, **98 tests green**
  (13 numen + 35 quint + 50 seiche), pushed to `mark-ik/conatus` (public).
- eidetic: founded, 4 subtree merges, path-linked, **70 tests green**,
  pushed to `mark-ik/eidetic` (public).
- mere: 14 git-dep lines across 10 manifests repointed to `eidetic.git`;
  `.cargo/config.toml` patches moved to family paths plus a new
  `[patch."…/eidetic.git"]` section; lock refreshed (zero references to old
  URLs); `mere-kernel` lib **273 tests green** on both family paths.
- Remaining consumers (turnstone, hocket, isometry, woodshed, servitor):
  repointed this session; see the commit trail in each repo.
