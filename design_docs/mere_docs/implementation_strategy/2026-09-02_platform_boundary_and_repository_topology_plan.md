# Platform Boundary and Repository Topology Plan

**Date:** 2026-09-02  
**Status:** reviewed against the code 2026-09-02; P0 and P1 authorized by Mark
the same day and in progress, with the inventory landing as §9 of this plan.
The authority boundary is ruled with Mark, while code moves,
repository changes, and GitHub operations have not started.  
**Authority:** this is the canonical plan for the Genet/Mere boundary and the
follow-on repository-topology review. Mer3ly continues to own the public
repository manifest and transfer receipts.  
**Supersedes:** the [2026-07-23 repo consolidation plan](2026-07-23_repo_consolidation_plan.md)
where it places Cambium and upper application components in Genet, and where it
treats the resulting repository layout as final. Its completed migration and
publication receipts remain historical facts.  
**Companions:** the [projection grammar adoption plan](2026-08-15_projection_grammar_adoption_plan.md),
the [Scenograph absorption plan](2026-08-22_scenograph_absorption_plan.md), and
the [projection/scenes architecture](../../2026-08-23_projection_scenes_and_graph_native_platform.md).
The public-repository execution record lives at
`mer3ly/docs/2026-07-29_live_repos_graph_and_org_migration_plan.md`. The
`vello_hybrid` evidence lives at
`netrender/netrender-notes/2026-08-10_vello_hybrid_upstream_ask.md`.

## 1. Ruling

The stable boundary is semantic rather than visual:

```text
products  ->  Mere  ->  Genet  ->  lower independent libraries
               |         |
               |         +-- web DOM, script, style, layout, paint,
               |             accessibility, web APIs, raw host contracts
               +-- Cambium UI and scenes, application composition,
                   projection policy, session and workspace policy
```

Genet is the web-platform engine. Mere is the application and projection
framework that consumes it. Products normally consume Mere and may reach Genet
directly only for a named low-level facility.

Cambium moves to Mere without losing any capability. It continues to speak
Genet's DOM, host, layout, paint, accessibility, and input contracts through
downward-facing adapters. It becomes the umbrella for two related but distinct
projection lanes:

1. the retained, diffed widget tree, currently expressed through Cambium,
   Meristem, Sprigging, controls, and Workbench; and
2. data-oriented scenes, currently expressed through `sceno`, `scenomise`, and
   `scenotime`.

Those lanes share lifecycle, input, accessibility, styling, commands, and host
integration where that is honest. Their state models stay separate. A widget
tree is retained interaction structure; a scene is a projection of content.

The generic `scenograph` facade dissolves into Cambium as its APIs find their
proper lane. **Scenograph** is then available for the scene/projection editor
product, built with Cambium rather than naming the scene runtime itself.

This ruling does not make Mere a dumping ground. A crate belongs there only
when it expresses application composition, durable product-facing contracts,
projection policy, or a reusable UI surface. Engine-observable web behavior
stays in Genet. A broadly reusable lower library may remain independent.

## 2. Authority map

| Concern | Owner after the pass | Physical direction |
| --- | --- | --- |
| DOM, script, CSS/style, layout, paint, accessibility, web APIs | Genet | stays |
| Raw engine host/profile/resource/navigation contracts | Genet | stays or is split from mixed crates |
| WHATWG-observable Fetch behavior | Genet | stays behind an injected transport/policy seam |
| WPT and a minimal raw engine host | Genet | stays |
| Retained widgets and diffing | Cambium in Mere | moves from Genet |
| Data scenes, arrangements, and scene time | Cambium in Mere | existing Mere crates are absorbed under the umbrella |
| Workspace, surfaces, settings, commands, and application composition | Mere | moves out of mixed Genet crates |
| Engine selection, non-web document lanes, reader extraction, and presentation ports | Mere | moves from Genet |
| Product document authority, sync, publishing, and files | owning product | stays with Knot, Turnstone, and peers |
| Renderer internals and broadly reusable GPU libraries | independent repos when they have their own release surface | stays independent pending explicit review |

### Mixed crates must be split by authority

- **`genet-host-api`:** keep the raw engine host, capability, resource,
  navigation, and profile contracts in Genet. Move workspace, settings,
  surface, tear-out, and Workbench vocabulary to Mere. Genet may depend on the
  raw half; Mere adapts it into application policy.
- **`genet-documents`:** keep Genet's DOM/script/render implementation in
  Genet. Move session-engine routing, retained application sessions, protocol
  selection, fetch integration, and non-web content lanes to Mere.
- **`netfetcher`:** first separate web-observable Fetch semantics from
  transport and host policy. Genet owns behavior visible to web content. Mere
  or the host owns transport choice, trust, credentials, caching, persistence,
  and protocol admission. The current crate stays put until this split can be
  proved without a Genet-to-Mere edge.
- **Diagnostics and probes:** engine telemetry may stay in Genet; Cambium
  projections of that telemetry belong in Mere.
- **`inker`:** its contract half is engine-facing. `genet-render` consumes
  the document accessibility projection types and the inspect report types
  from it, so "Inker's application-facing controller" moving in P3 requires
  the contracts to stay, or to move into an engine-owned crate, before the
  controller goes. P0 names that split.
- **`fleece`:** genet's CI cone witness pins its dependencies to
  `layout_dom_api` and `unicode-segmentation`, and `genet-scripted` exposes
  its page extraction as part of the scripted document's API. By §1's own
  bar that is a lower independent library with an engine consumer, not a
  Mere crate; P0 decides its class rather than this plan pre-assigning it.

## 3. Planned content moves

### Move to Mere

- the whole Cambium family: `cambium`, `cambium-rootstock`, Meristem,
  Sprigging, winit/web/accessibility hosts, and the Genet and Nematic adapters;
- Workbench and other retained application chrome;
- `sceno`, `scenomise`, and `scenotime`, with the generic `scenograph` facade
  absorbed rather than preserved as another layer;
- Inker's application-facing controller, `document-canvas`, its product host
  adapters, and the scry/graft/weld engine adapters;
- Nematic, Errand, Illume, Tinct, and Verso, and Fleece only if P0 does not
  class it as an independent lower library (see §2);
- Pelt and Tabard as Mere applications or ports over Genet; and
- the application-facing portion of `genet-documents` and `genet-host-api`.

Published crate names can survive a repository move. Package identity is not a
reason to preserve the wrong authority boundary.

### Keep in Genet

- web-platform implementation crates and their observable contracts;
- `engine-observables-api` and similarly raw engine-facing APIs;
- engine-owned DOM, scripted-document, resource, layout, render, and platform
  integration needed to host the web; and
- WPT, conformance harnesses, and one small host that proves Genet works
  without Mere.

The P0 inventory decides ambiguous crates by reading their dependencies and
callers. Folder names and July placement decisions are evidence, not authority.

### Product reconciliation

Standalone Knot is the authority for opening, editing, saving, revision,
evidence, vault, peer replication, and publishing. Its reusable retained
document surface may consume Cambium from Mere. Any duplicate Knot authority
still in Mere retires; Mere keeps only general contracts and a thin integration
port.

Mesocosm, Paredros, Isometry, Retinue, Turnstone, Hocket, Woodshed, Knot, and
Mer3ly are downstream proofs of the boundary. Their product models stay local.
Their reusable UI and projection machinery should arrive through Mere.

## 4. Repository topology after the boundary move

A repository, crate, and module answer different questions. Reuse alone does
not earn a repository. A separate repository needs coherent identity,
independent release or governance pressure, and a useful boundary for more
than one consumer.

### Keep separate for now

- **Genet and Mere:** the semantic boundary above is real.
- **Knot Editor:** it is a product and owns document authority.
- **Netrender:** it is a renderer with several consumers and its own backend
  questions.
- **`wgpu-graft`, `wgpu-scry`, and `wgpu-weld`:** retain their distinct public
  library identities and platform CI. First share release, pin, and wgpu-upgrade
  automation. Reconsider a merge only if coordinated upgrades repeatedly
  dominate their independent work.
- **Product, radio, protocol, and numerical repositories:** retain their
  existing product authority unless the P0 graph finds a duplicate owner.

### Candidate later extractions from Mere

- **Conatus:** extract when its portable contracts and tests build without
  Mere, its Mesocosm/Paredros/Isometry/Retinue consumers use one source
  identity, and the new repository has an independent release and CI surface.
- **Eidetic core:** separate the portable muniment/codicil/chartulary/scholia/
  tulpa core only after Mere-specific adapters are visibly outside that core
  and at least two consumers prove the boundary.

Cambium stays in Mere through this migration. A standalone Cambium repository
becomes worth considering only when its release cadence, governance, or
third-party consumer surface is genuinely independent of Mere. Graphshell,
Murm, Mesh, Moot, Dramatis, and Personae remain Mere concerns under their
existing extraction triggers.

### Consolidate or clarify

- Keep `knot-editor` and retire duplicate Knot authority from Mere.
- Treat `sonance` as a dormant name reservation while implementation remains
  in Mora: archive the placeholder or fold its notice into Mora's docs.
- `anise` is currently an organization fork used by an exact Turquet pin. Pin
  upstream directly and archive the fork, or document it explicitly as a
  controlled dependency mirror.
- Turquet's divergence from `astro-rust` warrants a later decision about
  leaving GitHub's fork network; provenance must remain explicit either way.

## 5. GitHub ownership pass

The July fork rule remains sound: a thin patch carrier, mirror, or evaluation
checkout stays personal. A fork enters `merely-made` only when it is integral
to the family and its maintained divergence is a meaningful project surface.

Apply that rule as follows:

- transfer first-party `mark-ik/emblem` to `merely-made` after the ordinary
  dependency, secret, redirect, and publication gates;
- make a fresh ruling on `mark-ik/p2panda`. Its branch has grown from the July
  review's 1-ahead/3-behind patch into an 8-ahead/0-behind line carrying
  per-instance sync protocol IDs, a store-agnostic address book, store release
  on shutdown, and external ALPN handlers across several Merely consumers;
- either transfer the archived Graphshell donor as an archive or update its
  tombstone to point from `mark-ik/graphshell` to `merely-made/mere`;
- keep ordinary upstream and patch-carrier forks personal, including the
  current Arboard, Boa, Iroh address lookup, Swarm Discovery, and Vello lines,
  unless a later review shows the same threshold crossing; and
- fill the missing Ringdown and Cleromancy descriptions and update Smolweb's
  description to match its current protocol family.

Any namespace change repins mutable Cargo branches to an immutable revision or
release tag. GitHub redirects are checked but are not treated as the canonical
dependency address. External transfers, archival, discussions, and pull
requests require a separate explicit instruction from Mark.

## 6. The `vello_hybrid` experiment

This is protected evidence, not disposable CI residue.

The `mark-ik/hybrid-scene-append` branch at `c73ba2c3` adds
`Scene::append_scene(other, Option<(u16, u16)>)` to `vello_hybrid`. The 569-line
prototype covers same-viewport composition and tile-granular translation. Its
tests compare the complete retained recording byte for byte, including paints,
layers, clip ranges, repeated appends, rejection atomicity, and translated
recording. The local test and Clippy gates passed, followed by all 20 upstream
CI checks through `mark-ik/vello-ci#1`.

The result also found real API constraints: translation is tile-granular,
sentinel strips need special handling, canonical node batching matters,
by-reference append needs a cloneable/shared encoded paint representation, and
translated filter layers remain deliberately rejected.

The branch proves feasibility. It does not yet justify a shipping dependency:
Netrender has no current `vello_hybrid` consumer, and its fragment-retention
seam can permit backends to retain different native forms.

### Vello gates

#### V0. Preserve the receipt

- Record the branch head, prototype diff, test output, and 20-check CI result
  in Netrender's durable note.
- Keep the branch, fork, and `vello-ci#1` reachable.

**Done when:** the experiment can be reconstructed without relying on a Codex
thread or mutable GitHub UI state.

**Met 2026-09-02:** the Netrender note carries the disposition guard, the
`c73ba2c3` head, the prototype shape, and the all-20-checks `vello-ci#1`
receipt.

#### V1. Review the upstream ask

- Mark reviews the existing draft and chooses between an upstream Discussion
  and holding the work locally.
- If the old conversation thread is found, link it from the durable note; the
  note remains the authority.

**Done when:** the intended ask, prototype shape, and open filter/ownership
questions are concise enough for upstream review.

#### V2. Refresh and discuss upstream, if authorized

- Rebase or otherwise remove the empty CI-nudge commit without losing a
  recovery ref.
- Re-run the relevant upstream matrix.
- Open a Discussion before offering a pull request, unless upstream guidance
  clearly prefers a PR.

**Done when:** upstream has a stable link to the evidence and records a
direction. This phase is dormant until Mark authorizes the external action.

#### V3. Admit a consumer only on proof

- Build Netrender's backend/fragment-retention seam independently of a standing
  fork.
- Add `vello_hybrid` only when a headed WebGL2 or downlevel target needs it.
- Compare the existing rasterizer-independent corpus and fragment invalidation
  behavior across both backends.

**Done when:** the second backend serves a named target, preserves the corpus,
and has measured fragment-level invalidation and composition receipts.

#### V4. Retire or maintain deliberately

- Archive `vello-ci` only after upstream disposition and after its receipts are
  preserved elsewhere.
- If upstream accepts the capability, delete the patch when a released version
  is usable.
- If upstream declines and a proven consumer still needs it, review the fork
  under the same integral-and-substantial rule as P2panda. Otherwise retire the
  branch and mirror.

**Done when:** there is one explicit disposition and every live dependency uses
an immutable source.

## 7. Implementation phases

### Prerequisites and sequencing

Surveyed 2026-09-02 across every open plan since July that names two or more
of the crates this plan moves (59 of them). Most only consume those crates
and are unaffected. These restructure the seams themselves and must reach the
stated point before the phase that touches their seam:

| Before | Plan | Must reach | Why |
| --- | --- | --- | --- |
| P1 | [Workbench component plan](../../../../genet/design_docs/2026-08-31_workbench_component_plan.md) | W4 landed on genet `main` | W1-W3 and the Pelt receipts live on five unmerged `codex/workbench-*` and `codex/pelt-*` branches; splitting `genet-host-api` from Workbench on `main` would fork them |
| P1 | [Knot shared surface and port contribution plan](2026-08-24_knot_shared_surface_and_port_contribution_plan.md) | F0, or an explicit hold on the v1-frozen surface contract | the surface contract in `genet-host-api` is what P1 moves to Mere; it was frozen at v1 under this plan (Genet `001448d55`) |
| P2 | [License sweep plan](2026-08-22_license_sweep_plan.md) | genet's P2-P7 headers and ledger, or a deliberate move-then-sweep ruling | invariant 5; each moved crate otherwise re-enters the sweep |
| P2 | [Browser WebRTC carrier plan](2026-08-25_browser_webrtc_carrier_plan.md) | a quiet window, not completion | the lane commits to mere hourly and `git subtree` refuses a dirty tree (July finding) |
| P3 | [Pelt host reconstruction plan](../../../../genet/docs/2026-08-22_pelt_host_reconstruction_execution_plan.md) | its stated immediate next lane closed | Pelt and Tabard move in P3 |
| P3 | [Knot editor repository extraction plan](../../../../knot-editor/design_docs/2026-09-01_knot_editor_repository_extraction_plan.md) | E1 and E2 consumer PRs merged | P3 reconciles Knot around its standalone authority |
| P4 | Turnstone [browser surface](../../../../turnstone/design_docs/2026-08-25_browser_surface_implementation_plan.md) and [page capture](../../../../turnstone/design_docs/2026-08-28_page_capture_plan.md) plans | their clean-source compile gates | both pin genet at exact revisions; a repoint before their gates close cannot be receipted |
| P7 | [Doc policy consolidation plan](2026-08-24_doc_policy_consolidation_plan.md) | phase D3 | docs travel with code onto a settled tree |

Not prerequisites, checked: the wgpu 30 unification plan completed
2026-08-16; the projection grammar plan's open stages wait on external
events and do not touch the seams; the settings projection plan is complete
through C6, though its code pointer to `genet-host-api/tile.rs` is stale
since the Workbench plan removed that module, which P0 records.

P0 has no prerequisite. P1 may begin on the two mixed crates' seams that the
Workbench branches do not touch, but its split of the surface and Workbench
vocabulary waits on the first two rows.

### P0. Produce the authority inventory

- Record every Genet workspace member, its direct reverse consumers, published
  identity, feature-sensitive edges, owning docs, and proposed class:
  `engine`, `framework`, `product`, `independent`, or `mixed`.
- Capture the current lock/source identities and dirty-state boundaries before
  moving files.
- Turn each `mixed` entry into a named split rather than assigning it wholesale.

**Done when:** every Genet member and every Mere scene/UI member has one owner,
all mixed seams have a proposed API, and the inventory exposes every edge that
would point from Genet to Mere.

### P1. Establish the lower contracts

- Split `genet-host-api`, `genet-documents`, and `netfetcher` at the authority
  seams in §2 while their code remains in place.
- Preserve Genet's raw host and WPT path.
- Add dependency-direction checks that reject a Mere source in Genet's graph,
  as a new witness in the existing `support/ci/check_dependency_cones.py`
  rather than a new mechanism.

**Done when:** Genet builds and exercises web-visible behavior without Mere;
Mere can inject application policy through the new contracts; WPT still reaches
the exact Fetch and document semantics it is meant to test.

### P2. Move Cambium and scenes

- Move the Cambium family into Mere with path-preserving history where
  practical.
- Place the scene family under the Cambium umbrella while keeping its model
  independent from the widget tree.
- Absorb the generic `scenograph` facade and reserve the name for the editor.
- Move accompanying docs in the same changes as their code.

**Done when:** Genet has no Cambium workspace members, a retained widget and a
`sceno` scene both render through Genet on supported desktop and web targets,
accessibility and input receipts remain green, and at least two unlike products
consume the Mere source.

### P3. Move upper components and ports

- Move Workbench, Inker's upper controller and adapters, document-canvas,
  Nematic, Errand, Fleece, Illume, Tinct, Verso, Pelt, and Tabard according to
  the P0 inventory.
- Leave engine implementation pieces discovered inside a mixed crate in Genet.
- Reconcile Knot around its standalone authority.

**Done when:** every moved crate has one source identity, Genet's remaining
workspace is engine/lower-library shaped, Pelt and Tabard run from Mere, Knot's
standalone and embedded surfaces share one document model, and moved docs are
indexed only in their owning repository.

### P4. Repoint and prove consumers

- Repoint the live consumer census, 25 manifests across 13 repositories at
  review time (cleromancy, hocket, isometry, knot-editor, mer3ly, mesocosm,
  netrender, paredros, both retinue trees, turnstone, woodshed), in
  dependency order using immutable
  revisions.
- Exercise Genet's raw host/WPT gate, Turnstone's browser path, Knot's
  standalone and embedded paths, one Retinue/Cambium surface, and the
  Mesocosm/Paredros scene path.
- Check Cargo source identity across the family after every receiving head is
  public and green.

**Done when:** supported consumers resolve a single Genet and Mere source,
representative headed proofs pass, and the source-identity audit reports no
duplicate package origins.

### P5. Review repository extractions and consolidations

- Apply the repository bar in §4 to Conatus and Eidetic core only after P4.
- Resolve the Knot duplicate, Sonance placeholder, Anise mirror, and shared
  wgpu release automation.
- Update the Mer3ly relation manifest as rulings land.

**Done when:** every separate repository has a stated independent surface and
every folded placeholder has a durable redirect or explanation. Candidate
extractions that fail the bar remain Mere crates without ceremony.

### P6. Execute authorized GitHub changes

- Refresh the live organization/personal inventory and fork ahead/behind
  evidence.
- Perform only the transfers, archival, metadata edits, and p2panda ruling that
  Mark explicitly authorizes.
- Record redirects, branch protection, Actions, release, Pages, secret,
  dependency, and post-transfer build receipts in Mer3ly.

**Done when:** GitHub, Cargo, Mer3ly's public graph, repository descriptions,
and local remotes agree, and each external mutation has a recoverable receipt.

### P7. Disseminate and archive

- Update surviving architecture docs and archive superseded plans only after
  their open tails have a current owner.
- Move Inker, Nematic, Verso, and other component docs with their code.
- Record the final Vello disposition separately from the platform move.
- Add a supersession note to the [Scenograph absorption plan](2026-08-22_scenograph_absorption_plan.md),
  which is complete and rehomes identity in the `scenograph` facade crate
  that §1 dissolves.

**Done when:** each active doc describes the live owner, both canonical indexes
agree, historical receipts remain reachable, and this plan contains only
genuinely open work before archival.

## 8. Invariants

1. Genet resolves without Mere.
2. Mere may depend downward on Genet and adapts raw engine contracts into host
   and product policy.
3. Cambium's widget and scene lanes share facilities without collapsing their
   state models.
4. Product authority remains with the product; projections and UI surfaces do
   not silently acquire persistence or sync authority.
5. Published names, license provenance, and path history are preserved unless
   a deliberate release decision says otherwise.
6. Docs travel with code and each fact has one canonical home.
7. Consumer repoints follow a green, public receiving revision.
8. Repository and GitHub mutations are separate from crate relocation and need
   their own receipts.

## Findings

### 2026-09-02

- Genet's live workspace still contains the Cambium family, Workbench, Inker,
  document-canvas, Nematic, Fleece, `genet-documents`, `genet-host-api`,
  Netfetcher, Pelt, Tabard, and Verso. Mere already owns the scene family and
  consumes Cambium from a pinned Genet revision. This is the concrete cycle-free
  seam the plan must reverse.
- `genet-host-api` directly depends on Workbench, and `genet-documents` combines
  Genet content implementation with Inker sessions, Fleece, Nematic, Errand,
  and Netfetcher. Whole-crate moves would preserve the present category error.
- Knot Editor's live README names one standalone product authority plus a
  reusable Cambium surface, correcting the earlier idea that the repository
  might merely duplicate a Mere port.
- The July maintained-fork receipt's significance test remains useful.
  P2panda's later semantic commits require a fresh decision; direct Cargo use
  by itself remains insufficient.
- The live public inventory reviewed for this plan contained 26
  `merely-made` repositories and 25 `mark-ik` repositories. Among the latter,
  Emblem and the archived Graphshell donor are the first-party stragglers;
  `vello-ci` is a temporary evidence carrier; most others are forks.
- The Vello append prototype and all-20-check CI receipt exist even though an
  obvious recent or archived Codex thread was not found. The Netrender note is
  therefore the durable recovery point.
- Mere is heavily dirty with unrelated concurrent work and is ahead/behind its
  remote. Genet and Mer3ly were clean at planning time. This documentation pass
  touches only the new plan and narrow index/supersession pointers.
- Review 2026-09-02, computed over all 99 genet workspace members from
  `cargo metadata`: 70 keep, 26 move, 3 mixed. Four edges would point from
  Genet to Mere after the moves as written. Two are the named mixed crates
  (`genet-documents` to document-canvas, errand, fleece, inker, nematic;
  `genet-host-api` to workbench). Two were unnamed: `genet-render` to
  `inker` (accessibility projection and inspect contract types) and
  `genet-scripted` to `fleece` (page extraction on the scripted document).
  Every other moving crate has no keep-side consumer inside genet.
- Fleece's dependency cone is already witnessed in genet CI as
  `layout_dom_api` plus `unicode-segmentation` and nothing else.
- The Workbench component plan's W1-W3 and the Pelt receipts are on five
  `codex/*` branches (`workbench-core-20260831`,
  `workbench-followups-integration-20260902`, `pelt-scrying-tearout`,
  `pelt-secondary-accesskit`, `pelt-surface-producer`), not on `main`.
- V0 of the Vello gates was already met by the Netrender note.

## Progress

### 2026-09-02

- Recorded the Genet/Mere semantic boundary and the Cambium two-lane model.
- Reconciled the boundary with repository ownership, current GitHub stragglers,
  Knot's standalone authority, and the July fork policy.
- Added explicit preservation, upstream-review, consumer-admission, and
  retirement gates for the `vello_hybrid` experiment.
- Code relocation, consumer repoints, repository transfers, archival, and
  upstream contact remain unstarted.
- Reviewed 2026-09-02 against the code. Corrections landed in §2, §3, P1, P4,
  P7 and V0; the prerequisites table and the edge findings were added. Mark
  authorized P0 and P1 the same day, with the inventory as a section of this
  plan.
