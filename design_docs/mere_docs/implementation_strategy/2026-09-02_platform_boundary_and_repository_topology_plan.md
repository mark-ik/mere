# Platform Boundary and Repository Topology Plan

**Date:** 2026-09-02  
**Status:** P0 and P1 landed 2026-09-02 (§9 and the Progress entries); P2 not started.
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
- Alembic folds into Distillery as a core component crate, and Athanor with
  it (ruled and executed 2026-09-02; the suite census §7.4 carries the ruling
  and the argument). Flat under the port as `ports/distillery/{alembic,athanor}`;
  `mere-alembic` unchanged, `mere-athanor` founded.
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
| P2 | [Workbench component plan](../../../../genet/design_docs/2026-08-31_workbench_component_plan.md) | W4 landed on genet `main` | `codex/workbench-core-20260831` is already on `main`; the four remaining `codex/workbench-followups-*` and `codex/pelt-*` branches (nine commits) touch none of the four mixed crates, so P1 may split them, but Workbench itself must not move under an open lane |
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

P0 has no prerequisite. P1 may begin on all four mixed seams: the open
Workbench and Pelt branches touch none of them (checked 2026-09-02). Its
surface half still honours the v1 freeze in the second row.

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
- Extract inker's contract half (session traits, accessibility projection,
  capabilities, page capture, the engine-id namespace) into an engine-owned
  crate in this phase, not P3, so Genet never carries a Mere source while
  the controller moves; give the cone witness an inker cone.

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

## 9. P0 authority inventory (2026-09-02)

Mechanical census from `cargo metadata --no-deps` over genet (99 members) and the
17 mere members that are scene crates or consume Cambium or a scene crate,
joined with every manifest under `Code/repos` outside the two repositories that
names a family crate (source kind in parentheses), and with the docs that name
the crate in their title or head. Classes are the plan's defaults, corrected
where the code disagrees; the split analyses in §9.3 name every mixed seam.

### 9.1 Members by class

**Engine, stays in Genet (70).** The Servo-derived platform crates, the DOM,
style, layout, script, render and host crates, WPT, and the vendored `parley`
patch. None has a consumer the plan moves except through the mixed crates, and
their external consumers pin genet directly:

- `engine-observables-api`: turnstone
- `genet-clipboard`: hocket
- `genet-livery`: mesocosm, turnstone
- `genet-paint-types`: turnstone
- `genet-probe`: cleromancy, hocket, knot-editor, mesocosm, retinue, retinue-wn1-v4-physical, turnstone, woodshed
- `genet-render`: turnstone
- `genet-scripted-dom`: cleromancy, hocket, isometry, knot-editor, mer3ly, mesocosm, retinue, retinue-wn1-v4-physical, turnstone, woodshed
- `genet-static-dom`: turnstone
- `genet-winit-host`: isometry, turnstone
- `layout-dom-api`: cleromancy, hocket, isometry, knot-editor, mer3ly, retinue, retinue-wn1-v4-physical, turnstone, woodshed
- `parley`: hocket, knot-editor, mesocosm, netrender, turnstone, woodshed
- `script-engine-api`: turnstone
- `script-engine-piccolo`: turnstone

**Move to Mere as written (25), with their engine-side consumers inside genet
and their external consumer repositories:**

| member | version | engine-side consumers in genet | external consumer repos |
|---|---|---|---|
| `cambium` | 0.3.3 | - | cleromancy, hocket, isometry, knot-editor, mer3ly, mesocosm, retinue, retinue-wn1-v4-physical, turnstone, woodshed |
| `cambium-genet-web-host` | 0.1.0 (unpublished) | - | woodshed |
| `cambium-genet-winit-host` | 0.1.0 (unpublished) | - | cleromancy, hocket, knot-editor, retinue, retinue-wn1-v4-physical, woodshed |
| `cambium-nematic` | 0.3.1 | - | - |
| `cambium-rootstock` | 0.1.0 (unpublished) | - | woodshed |
| `cambium-winit` | 0.3.0 | - | isometry, turnstone |
| `cambium-winit-a11y` | 0.3.0 (unpublished) | - | - |
| `document-canvas` | 0.1.0 | genet-documents (optional) | - |
| `errand` | 0.3.4 | genet-documents (optional) | turnstone |
| `fleece` | 0.4.0 | genet-scripted, genet-documents | knot-editor, turnstone |
| `graft-engine` | 0.2.0 (unpublished) | - | - |
| `illume` | 0.0.2 | - | knot-editor |
| `knot-editor-host` | 0.1.1 | - | knot-editor, turnstone |
| `meristem` | 0.2.0 | - | - |
| `nematic` | 0.1.1 | genet-documents (optional) | knot-editor |
| `pelt` | 0.2.0 (unpublished) | - | - |
| `pelt-core` | 0.2.0 (unpublished) | - | - |
| `pelt-desktop` | 0.2.0 (unpublished) | - | - |
| `scrying-engine` | 0.2.0 (unpublished) | - | - |
| `sprigging` | 0.2.1 | - | hocket, isometry, mesocosm, retinue, retinue-wn1-v4-physical, turnstone, woodshed |
| `tabard` | 0.0.1 | - | - |
| `tinct` | 0.1.2 | - | hocket, woodshed |
| `verso-tile` | 0.1.0 | - | - |
| `weld-engine` | 0.2.0 (unpublished) | - | turnstone |
| `workbench` | 0.1.0 | genet-host-api | hocket, mesocosm, woodshed |

**Mixed, split by authority (4):** `genet-documents`, `genet-host-api`, `inker`, `netfetcher`. Their seams are §9.3.

**Class corrections from the census.** `fleece` (0.4.0) is consumed inside
genet by `genet-scripted` and `genet-documents`, both engine or mixed, and
externally by knot-editor and turnstone; with its CI-witnessed cone of
`layout_dom_api` and `unicode-segmentation` it is classed **independent** rather
than move: it may stay in genet as a lower library or leave for its own
repository, but it does not go to Mere. `tinct` (0.1.2) and `illume` (0.0.2)
have no engine-side consumer and move as written. `verso-tile` (0.1.0) and
`cambium-nematic` (0.3.1) have no consumer anywhere in the census and move
with their families.

**Mere scene and UI members (17):** `chirograph`, `distillery`, `djinn`, `gazette`, `graphshell`, `graphshell-client`, `graphshell-local`, `graphshell-stdio`, `knot-document`, `knot-editor`, `mere-canvas`, `mere-cartography`, `mere-persona-picker`, `sceno`, `scenograph`, `scenomise`, `scenotime`. Every one is `framework`; `scenograph`
(0.0.4) has no consumer in the census, consistent with §1 dissolving the
facade; `sceno` is consumed by thirteen manifests across nine repositories.

### 9.2 Edges that would point from Genet to Mere

Computed over the same graph. After the moves as originally written, and
before any split:

- `genet-documents` (mixed) -> `document-canvas (optional)`, `errand (optional)`, `fleece`, `nematic (optional)`
- `genet-host-api` (mixed) -> `workbench`
- `genet-scripted` (engine) -> `fleece`

`genet-render` and `genet-scripted` are engine crates the plan keeps; their
edges are why `inker` is mixed and `fleece` is reclassed. No other kept member
reaches a moving one. Genet reaches no Mere source today, in manifests, in the
resolved graph, or through the machine-local patch file; the new witness in
`support/ci/check_dependency_cones.py` holds that line from here on.

### 9.3 Mixed seams

Each mixed crate was analysed read-only against the plan's §2 sentence for
it: item inventory, every use of a moving dependency, every consumer across
genet, mere and the twelve product repositories, the proposed split, and the
seam that lets the Genet half name no Mere type. The positive controls run
afterwards are stated where they were run.

#### `genet-host-api` (676 lines, four files at the crate root)

The cut is clean and needs no seam trait. `lib.rs` and `navigation.rs` are
the raw half: `ResourceFetcher` and `ResourceResponse` (the injected fetch
seam, 110 keep-side uses across genet-scripted, genet-documents and
genet-document-resources), `resolve_href`, and the `EngineProfile` /
`ShellEngine` / `ShellEngineCapabilities` / `DeferredShellEngine` cluster.
`settings.rs` (238) and `surface.rs` (143) are the application half in their
entirety: settings scope, movement, mutability and security are Mere and
product vocabulary, and the surface descriptor is frozen at v1 under a Mere
plan that `surface.rs:12-17` names as its authority. Nothing in either half
names a type from the other; the only coupling is two `pub mod` lines and a
seven-name re-export. Delete those and the crate falls into a ~290-line Genet
remainder with zero in-workspace dependencies and a ~380-line Mere crate that
depends on `workbench` and on nothing from Genet. The crate's single
`workbench` edge is `settings.rs:12`, so the split severs it without waiting
for Workbench to move.

Every in-genet caller of the application half is itself scheduled to move
(cambium's setting row, surface and catalog; Pelt's appearance and workspace
viewer), so after P2 and P3 the application half has no keep-side consumer.
External consumers of it are turnstone (seven files), woodshed, hocket,
knot-editor and mere's `knot-document` and `distillery` ports: eighteen import
sites, no logic changes. None of the open `codex/*` branches touches this
crate.

Two rulings and two hazards. **Ruling 1:** §2's mixed-crate bullet keeps
"profile contracts" in Genet while the authority map moves "engine selection"
to Mere; `EngineProfile` is both. Recommendation: keep it, since it names two
Genet engine identities and its only consumer is Pelt; the routing policy
lives in Pelt's `selected_engine`, not in the enum. **Ruling 2:**
`ShellEngineCapabilities` has no caller in the fourteen-repository census;
decide whether it is a reservation or dead before either half inherits it.
**Hazard 1:** turnstone and woodshed pin a revision where `tile.rs` still
existed, and turnstone has four call sites on `genet_host_api::tile::*`
(`settings_provider.rs:13`, `settings_pane.rs:13`, `app/pane_arms.rs:511-534`)
that break on any repoint past `abcea38b962`, independent of this plan.
**Hazard 2:** the existing cone witness asserts Pelt's manifests are at
`ports/pelt`; when Pelt moves in P3 that assertion breaks and Genet's "one
small raw host" role is unfilled, `pelt-core` being disqualified by its
`workbench` and `inker` dependencies.
Two late additions: knot-editor carries a repository-local cargo checkout of
genet at its pinned revision, which still has the old `tile` alias, and P4's
repoint must invalidate it; and the `frisket` name-claim's published
description still names the removed `genet-host-api` tile contract, to be
corrected when Workbench moves.

#### `genet-documents` (6,802 lines, 12 files)

Its feature table already draws the boundary. `livery`, `scripted`,
`livery-scripted` and `scripted-nova` pull only Genet crates; `netfetch`,
`reader` and `smolweb` pull netfetcher, document-canvas, errand and nematic,
all optional and already gated. The stays half is the Livery and Scripted
session engines (`engines/livery.rs`, 1,746; `engines/scripted.rs`, 251), the
clip and content-report walk (`engines/clip.rs`), the `data:`/`file://`
branches of `LocalFetcher`, and the `href` re-export, about 4,000 lines with
tests. The moves half is `reader.rs` (836), `smolweb.rs` (1,038),
`engines/smolweb.rs`, `net_fetch.rs` (the shared tokio runtime, the
netfetcher host, errand fetch and the TOFU trust store), `ResourceFetchPolicy`
and `ConfiguredLocalFetcher`, about 2,500 lines with tests; a Mere crate on
the order of `mere-document-lanes`. `net_fetch.rs:10-12` already states the
rule the crate violates: engine components stay byte-consuming and never link
transport.

After that move the stays half names two moving dependencies. `inker`, only
through `session_engine`, `a11y`, `capabilities` and one routing id constant:
the contract half, which `genet-render` already depends on for the same
types. And `fleece`, at `engines/clip.rs:108` (`extract_main_text`), which
stands or falls with `genet-scripted`'s public `extract()`; it is the second
witness for classing Fleece independent. One leak remains in the stays half,
`fetch.rs:101` sniffing schemes through `errand`.

Three seams, all types Genet already owns. Session dispatch is
`inker::SessionEngine` and `DocumentSession`: both halves implement them and
Mere's host registers both in one `SessionRegistry`, so Genet names only the
trait. Routing: the engine id constants are the first fifty lines of
`routing.rs` (`ENGINE_GENET_LIVERY` and its siblings); keep those names with
the contract half and move the route policy, rules and decisions that fill
the rest of that file, which is the "routing moves, semantics stay" cut.
(`routing/ids.rs` holds the host-graph context identities, `NodeKey` and
`RouteViewId`, deliberately kernel-free; they stay with the contract.)
Fetch: `genet_host_api::
ResourceFetcher` is already the seam; restructure `LocalFetcher` so the
remote schemes are injected as a fallback fetcher rather than `#[cfg]`
branches, which deletes the errand leak and the `netfetch`/`smolweb` features
from Genet's manifest without waiting for the netfetcher split.

Consumers: exactly two. Pelt's desktop port takes both halves (its default
features are `livery`, `reader`, `netfetch`), and turnstone by git revision
takes one Genet item and four Mere-side ones, which is the right shape for a
product. Mere does not consume the crate; genet-wpt does not either, so the
split cannot regress WPT. Decisions for Mark: whether Pelt, as the host that
proves Genet without Mere, sheds `reader` and `smolweb` or accepts a Mere
dependency; whether `ResourceFetchPolicy` (redirect, concurrency, body and
timeout caps, used at four Pelt sites) is reclassed as a `genet-host-api`
contract. Caveat for receipts: turnstone's `.cargo/config.toml` redirects the
whole genet family to a Codex worktree, so a turnstone build today proves
that worktree, not its pinned revision. Unverified positive control: `cargo
check --no-default-features --features scripted` on the stays half.

#### `netfetcher` (5,221 lines, 26 files)

The authority split is already the crate's design; the graph does not yet
prove it. About 2,430 non-test lines are web-observable Fetch semantics and
stay: `request`, `response`, `cors` (the most WPT-load-bearing file),
`referrer`, `sri`, `data_url`, `decode`, the HSTS and Alt-Svc parsers, the
RFC 9111 freshness rules in `cache`, and all of `fetch/` but the send. About
1,300 lines are transport, trust, credentials and persistence: `client`
(the process-global hyper/rustls client and the public one-way
`accept_invalid_certs` downgrade), `h3_client`, `websocket` (not Fetch at
all, dead public API), the in-memory cookie, cache, HSTS, Alt-Svc and
preflight stores, and the send in `fetch/transport.rs`. `FetchContext`
already carries six host seams as `dyn` objects (cookies, cache, CSP, HSTS,
preflight, Alt-Svc) and Mere already injects through them from
`crates/system/fetch`; the direction of every existing edge is Mere to
netfetcher.

Two defects block a mechanical proof: `fetch/preflight.rs:57` calls the
shared client directly instead of the send, and there is no transport seam,
so the transport is reached through a process-global that cannot be excluded
from a build. Proposed seam, in place: a `Transport` trait taking the
existing `WireRequest` shape and returning the existing `RawResponse`
(`fetch/transport.rs:22`, already the transport-agnostic normal form), held on
`FetchContext` beside the six stores and defaulted in `permissive()` so WPT,
genet-documents and a raw host run unchanged; route the preflight through it;
hoist the 8 MiB cache cap off the crate onto the context; feature-gate the
hyper, h3 and websocket lanes so a `default-features = false` build is the
proof that the semantics half links no transport. No file moves.

Consumers: genet-documents (narrow: `permissive`, `Request::get`, `fetch`,
body drain, behind its `netfetch` feature), genet-wpt (the whole enum surface
plus `accept_invalid_certs` at `main.rs:871`, which needs a replacement in the
same change), and Mere's `mere-fetch` by git revision (storage-wide: it is the
only consumer of the cookie jar's `all_records`/`load_records`). Risks:
`decode` pins tokio into the semantics half through `async-compression`'s
tokio readers, acceptable but not a "no runtime" half; the first async seam on
a context whose seams are all synchronous is a design decision with more than
one answer; `data_url` may duplicate the `data-url` crate genet-documents
already pulls.

#### `inker` (8,410 lines, 20 files; published, MIT/Apache by its own line)

The contract half severs with no edge to cut. `session_engine` (1,267 lines:
the `SessionEngine` and `DocumentSession` traits, `ContentReport`,
`OutlineEntry`, `DocumentClip`, the input vocabulary), `a11y` (458, the
projection `genet-render` produces), `capabilities` (95) and `page_capture`
(131) depend on nothing else in the crate; `session_engine` never names
`document`, `engine` or `routing`, and the registry keys on a string engine
id. That is 1,951 lines, or 2,945 with the whole routing module. Everything
else is controller: the portable document model and its exporters,
evaluators and transclusion (2,800 lines whose own docs say "policy is the
caller's"), the request/response `Engine` and its registry, `surface_engine`
(1,305 lines, the WebSurface and user-agent-policy contract the three GPU
adapters implement), `statements` (its counterpart is Mere's linked-data),
and `sniff` (335 lines, no caller anywhere).

Consumers decide it. After the plan's moves, the only Genet-resident
consumers are `genet-render` and the Livery and Scripted lanes of
`genet-documents`, and between them they use exactly `a11y`, `capabilities`,
`session_engine` (with `SessionRegistry` in tests) and one routing constant,
`ENGINE_GENET_LIVERY`. Every one of Mere's nine consuming crates sits
entirely in the controller half; none names the contracts. Turnstone
straddles both (twenty files; `shell/weld.rs` alone imports thirty-four
`surface_engine` types). `routing.rs` is two authorities in one file: the
engine-id namespace and rung ladder in its first 180 lines are Genet facts a
kept crate returns from `engine_id()`, the route policy, rules and decisions
below them are Mere's; no kept crate uses the policy half.

Proposed shape: extract the contract half into a new Genet crate beside
`engine-observables-api` and `layout-dom-api` under `components/shared`
(name to be claimed: `document-session-api`, `genet-document-api` or
`inker-contracts`); `inker` keeps its name, version line and license, moves
to Mere with the controller, and re-exports the contract crate wholesale so
its flat facade survives. Then turnstone, Pelt, nematic, document-canvas,
the adapters, knot-editor and all nine Mere crates change only a manifest
source line, and only `genet-render` (two files) and `genet-documents`
(seven) repoint their imports, which they must, being the edges severed.
Mere's pin on inker becomes a workspace path and gains a genet pin on the
contract crate; turnstone takes both.

Sequencing correction: the plan puts the Inker move in P3 and contract
establishment in P1. If inker leaves before the contract crate exists,
Genet's graph carries a Mere source for the duration. The extraction is a
P1 item, and the cone witness, which has no inker cone today, should gain
one. Two hazards: `genet-render` reaches inker by a relative path, not the
workspace table, so the move fails loudly there; and four inker revisions
are already live across the family (mere and knot-editor at `eff0cb6d`,
turnstone and woodshed at `da8762fd`, hocket and isometry on `branch = main`
at different heads, isometry still resolving 0.1.0, cleromancy through a
machine-local path patch recorded in its committed lock), so P4's
source-identity audit wants a baseline before the split, not only after.
Docs: `genet/design_docs/inker_docs` holds one file, the engine-picker plan,
which is controller material already citing four Mere documents; it travels,
leaving a page-capture-contract note behind, and genet's `DOC_POLICY.md`
claim that its three area roots mirror `components/{inker,nematic,verso-tile}`
goes false when they move.

#### Rulings the inventory needed from Mark, taken 2026-09-02

All nine were ruled as recommended. Each item below states the recommendation
that is now the ruling.

1. **Fleece's class.** Independent lower library, witnessed by two engine
   consumers and its CI cone; it does not go to Mere. Where it lives (genet
   or its own repository) is a §4 question, not a P1 one.
2. **`EngineProfile`.** Keep in Genet with `ShellEngine` and the deferred
   engine; it names two Genet engine identities, and the routing policy
   lives in Pelt. This resolves §2's contradiction between "profile
   contracts stay" and "engine selection moves": the enum stays, the
   selection moves.
3. **`ShellEngineCapabilities`.** No caller anywhere in the census; delete,
   or state what it reserves.
4. **Pelt's side of the line.** Its desktop port enables `reader` and
   `netfetch` by default and carries the smolweb glue, so today it cannot be
   the host that proves Genet without Mere. Either Pelt sheds those lanes
   before P3 and keeps the raw-host role, or a smaller host takes the role
   and the cone witness's Pelt assertion moves with Pelt.
5. **`ResourceFetchPolicy`.** Redirect, concurrency, body and timeout caps
   are host contracts even though their implementation is transport;
   reclass it as `genet-host-api` vocabulary so Pelt's four uses stay
   Genet-side, or move it and accept a Pelt-to-Mere edge.
6. **Netfetcher's transport seam.** Its six existing seams are synchronous;
   the transport one cannot be. Trait with a boxed future, or `async fn` in
   trait: a design decision with more than one defensible answer, to be
   taken in the netfetcher change itself.
7. **Inker's contract crate.** Recommended: the contracts leave for a new
   engine-owned crate under `components/shared` and `inker` keeps its name
   on the controller, re-exporting them; the reverse fixes nine files and
   churns sixty. The new crate's name is a naming-ledger claim.
8. **Inker's routing file.** Split it at line 180 (engine ids and rung
   ladder stay, route policy moves) or keep the whole 994-line module in
   Genet as a legal downward read for Mere. Splitting is honest to
   authority; keeping avoids forking a file.
9. **`SessionRegistry`.** A registry by the plan's letter, but
   genet-documents' kept tests spawn through it; recommended to stay beside
   the traits it dispatches. `sniff_content_type` has no caller and is a
   retirement candidate rather than a move.

### 9.4 Source identities at review time

Mere consumes eight genet crates (inker, document-canvas, nematic, fleece,
knot-editor-host, illume, cambium, scrying-engine) from one immutable revision,
`eff0cb6df4834ecce9ac552a055c1c459befa7c3`. External consumers reach the moving
crates by three source kinds; a repoint in P4 must replace each:

- **git branch**: hocket, isometry, mesocosm
- **git rev**: cleromancy, knot-editor, retinue, retinue-wn1-v4-physical, turnstone, woodshed
- **registry**: hocket, isometry, mer3ly, mesocosm, woodshed

Registry pins on `cambium`, `sprigging`, `workbench`, `tinct` and the
`genet-*` crates survive a repository move unchanged until the next publish,
when `repository` fields update; git-revision pins repoint; git-branch pins
(hocket, isometry, mesocosm) are mutable and become revisions under §5.

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
- The Workbench component plan's core branch is on genet `main`; four
  follow-up branches (`workbench-followups-integration-20260902`,
  `pelt-scrying-tearout`, `pelt-secondary-accesskit`, `pelt-surface-producer`,
  nine commits) remain open and touch none of the four mixed crates.
- V0 of the Vello gates was already met by the Netrender note.

### 2026-09-03 — P2 assessment: the Cambium move cannot go first

Written before any file moves, from the P0 census and the tree as it stands
after P1 and the license sweep.

**The cluster.** Cambium is not a leaf in genet. Two members the plan moves
in P3 depend on it today: `knot-editor-host` on `cambium`, and `pelt-desktop`
on `cambium` and `cambium-genet-winit-host` behind its `livery` feature. And
`workbench`, which the plan lists among the P3 moves, is depended on by
`cambium`, `mere-surface-api`, `pelt-core` and `pelt-desktop`. So the set
{Cambium family, Workbench, mere-surface-api, mere-document-lanes, Pelt,
knot-editor-host} is one connected cluster inside genet, and every edge in it
points toward Cambium and Workbench.

**Why P2 as written breaks invariant 1.** If the Cambium family and Workbench
leave genet while Pelt and knot-editor-host stay, those two must reach Cambium
from mere's repository, and the resolved graph then carries a Mere source.
The witness landed in P1 fails on the first `cargo metadata`. There is no
feature flag that removes Pelt's dependence: `livery` is its shell.

**Three orders, one that holds.**

1. *P2 then P3 as written* — fails the witness between the two phases, as
   above.
2. *The whole cluster in one motion* — Cambium, Workbench, the two
   Mere-bound contract crates, Pelt and knot-editor-host move together. Every
   step holds invariant 1, but it is one change across nine Cambium crates,
   two ports, a host, and 167 tracked Cambium files plus Pelt's, with the
   P2 and P3 done-conditions to prove at once.
3. *Consumers first, then Cambium* — Pelt and knot-editor-host move to mere
   first (P3's items), consuming Cambium from genet by revision as mere's
   five Cambium consumers already do; that edge points downward and the
   witness is silent. Then the Cambium family and Workbench move with no
   genet consumer left, and Pelt's pins become workspace paths in the same
   commit. Each step is a bounded change with its own receipts, and invariant
   1 holds at every commit.

Recommendation: 3. It reorders the phases' *content* without changing what
each proves: the P2 done-conditions (no Cambium members in genet, a widget and
a scene rendering through genet on desktop and web, receipts green, two unlike
products on the Mere source) are proven when Cambium lands, after Pelt has
already been running from mere against genet's Cambium.

**What the raw-host ruling becomes under 3.** Once Pelt leaves, genet's only
host is genet-wpt, which is headless. The plan's "one small host that proves
Genet works without Mere" then has to be founded: a minimal headed port over
`genet-winit-host`, `genet-render-host` and a Livery session, with no
Workbench and no Cambium. The cone witness's assertion that Pelt's manifests
live at `ports/pelt` retires with Pelt. That founding is P3 work and its own
small plan; ruling 4's question is answered by it, not by trimming Pelt.

**Landing shape in mere, to be ruled.** mere's convention is one family per
`crates/<family>/` directory. Proposed: `crates/cambium/` for the nine Cambium
crates; the scene family moves from `crates/scenograph/` to
`crates/cambium/scenes/` under the umbrella, keeping crate names; Workbench to
`crates/cambium/workbench`; `mere-surface-api` and `mere-document-lanes` to
`crates/system/` beside `fetch`, since they are host contracts and content
lanes rather than widgets; Pelt to `ports/pelt`, knot-editor-host to
`crates/cambium/knot-editor-host` or beside Knot's port. The `scenograph`
facade (36 lines of re-exports plus a 528-line solver registry) has one
in-mere consumer, the `webrtc-join` probe, and that consumer already takes
`sceno` directly; the registry moves into `scenomise` or a `cambium-scenes`
umbrella crate, the facade crate is deleted, and the published name is held
for the editor product per the naming ledger.

**History.** `git subtree split -P components/cambium` over genet's history
was tried with a ten-minute budget and finished in 275 seconds, walking 738
commits and producing a 328-commit branch (`cambium-history-probe`, local to
genet) whose root is the 2026-06-16 xilem_core vendoring from the Serval
era and whose tip is today's relicense. So the July finding, that a split
out of a large repository is not viable, does not hold for this path, and
the move can preserve history as the plan asks: `git subtree add` of that
branch into mere at the chosen prefix, with the tree's own `docs/history`
and receipts travelling in it. Copy plus pointer remains the fallback if the
add itself misbehaves.

**Consumers.** Nine repositories pin the Cambium family from genet.git today
(cleromancy, hocket, isometry, knot-editor, mer3ly, mesocosm, retinue,
turnstone, woodshed; 35 manifest lines). They repoint to mere.git in P4 after
the receiving head is public and green, and mere's own five consumers of
`cambium` and `workbench` become workspace paths when the crates land.

**Prerequisites now met or not.** The license sweep's genet phase landed
(`957926e4e8a`); mere was clean at assessment time, the quiet window is a
matter of picking the hour; the Workbench W4 receipts are on genet main.

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
- P0 landed 2026-09-02 as §9: 99 genet members and 17 mere members classed,
  the four Genet-to-Mere edges named, every mixed seam given a proposed API
  by a per-crate analysis, and consumer source identities captured. The
  Mere-source witness landed in genet `cbe7d383584`, with its positive
  control, as P1's first deliverable.
- Mark ruled all nine §9.3 items as recommended, 2026-09-02, and named
  netfetcher first. **netfetcher seam landed** in genet `1fae097752c`: a
  `Transport` seam on `FetchContext` (boxed future, object-safe like the
  other seams), the preflight routed through it, the cache body cap hoisted
  onto the context, and the hyper, h3 and websocket lanes behind default-on
  features. The `default-features = false` build is the proof and CI runs
  it, with a cone witness that the semantics-only tree reaches none of
  eleven transport crates and the default tree does. Three seam tests, the
  sixty-eight existing tests unchanged, WPT and genet-documents building
  with `netfetch`. WPT `fetch/api/basic` and `fetch/api/abort` in
  spawn-server mode are identical by name before and after (81/119 and
  19/35 subtests), the baseline built from a detached worktree at the
  pre-change head. While this landed the Workbench component plan's W4
  receipts closed on genet `main` (`66bf5c0b551`), which satisfies the P2
  prerequisite row. Next in P1: `genet-host-api`, then the inker contract
  extraction, then `genet-documents`.
- Names ruled by Mark 2026-09-02, both checked free on the crates.io sparse
  index and both plain hyphenated infrastructure per the naming ledger's
  tier rule: **`mere-surface-api`** for the application half of
  genet-host-api, **`document-session-api`** for inker's contract crate.
  Claims follow with a real publish when each crate is ready.
- **genet-host-api split landed** in genet `57bcc38fdae`: settings and surface
  moved with history into `components/mere-surface-api` (depends on
  workbench alone); the raw half keeps the name at 0.2.0 with no
  in-workspace dependency; six in-genet import sites repointed and cambium
  no longer depends on genet-host-api; the cone witness holds both cones.
  External consumers are untouched until P4, when eighteen import sites
  across turnstone, woodshed, hocket, knot-editor and Mere's knot-document
  and distillery ports move to `mere_surface_api`. Next: the inker
  contract extraction, then `genet-documents`.
- **inker contract extraction landed** in genet `fb16345a204`:
  `components/shared/document-session-api` holds a11y, capabilities,
  page_capture, session_engine (registry included, ruling 9) and the
  engine-id namespace with the rung ladder cut from routing.rs (ruling 8),
  depending on serde alone under inker's founding MIT/Apache license;
  inker keeps its name on the controller and re-exports the contracts
  module for module, so no consumer path changed. genet-render drops inker
  for the contract crate; genet-documents' Livery and Scripted lanes take
  it while its reader and smolweb lanes keep inker. The cone witness now
  holds the contract crate a leaf and forbids genet-render an inker edge.
  Next and last in P1: `genet-documents`.
- **genet-documents split landed** in genet `76a47850946`, and with it **P1 is
  complete**. The reader and smolweb lanes, their session engine and the
  remote fetch bridge moved with history into `components/mere-document-lanes`
  (Mark's name, checked free on the sparse index; reader unconditional,
  `smolweb` and `netfetch` its features). genet-documents keeps the Livery
  and Scripted lanes and links no controller, content lane or transport,
  which the cone witness enforces. The fetch seam was restructured rather
  than moved: `LocalFetcher` serves `data:`, `file://` and bare paths and
  takes a remote fetcher by injection; `RemoteFetcher` in the new crate
  covers http(s) and the smolweb schemes; `ResourceFetchPolicy` is
  genet-host-api vocabulary (ruling 5). Pelt composes the two and takes the
  new crate under its lane features, which answers ruling 4 for P1: Pelt
  depends on the Mere-bound crate for those lanes, and whether it sheds
  them or a smaller host takes the raw-host role is settled in P3.
  P1's done-conditions: Genet reaches no Mere source (witnessed), Mere
  injects policy through the seams (`FetchContext::transport`,
  `ResourceFetcher`, the session traits in `document-session-api`), and WPT
  reaches the same Fetch semantics (fetch/api/basic and abort identical by
  name) and never consumed genet-documents. Four consumer repoints wait on
  P4: turnstone (`genet_documents` reader and smolweb items, `genet_host_api`
  settings and surface, `::tile`), woodshed, hocket and knot-editor.
- 2026-09-03: the license sweep's P2 landed on genet (`957926e4e8a`), which
  satisfies this plan's P2 prerequisite row for the license sweep; the
  quiet-window row is a matter of timing, not completion.

### 2026-09-03

- **mere repointed to genet head after P1** (mere `487e18a478c`). Every
  genet.git pin moved from `eff0cb6df48`, the revision §9.4 recorded as mere's
  single source identity, to `388d89c3a64`; `ports/knot/desktop` was two
  revisions further behind at `da8762fd910` and joins the rest. Thirty-nine pin
  lines across three manifests at one revision, witnessed by grep and by
  `cargo metadata` resolving 31 genet-derived packages there and nowhere else.
- Three of the four split crates reach mere; only one changed code.
  **`genet-host-api`** is 0.2.0 with the raw engine half alone, and mere names
  nothing in it, so the workspace pin becomes `mere-surface-api` 0.1.0 and
  `distillery` and `knot-document` swap crate and import for the six surface
  types across three files, with no logic change. **`inker`**'s re-export is
  module for module, so no import path moved and `document-session-api` needs
  no pin of its own: it arrives transitively at the same revision. **`netfetcher`**
  needs no source change, because `crates/system/fetch` builds its context only
  through `FetchContext::permissive()`, so `transport` and `cache_max_body_bytes`
  default in and the default-on transport features keep the wire.
  **`genet-documents`** turns out not to be a mere consumer at all, so
  `mere-document-lanes` is not pinned; mere's own smolweb and gemini lanes are
  `mere-fetch`'s, over errand. This corrects §9.4's count in one direction and
  §9.3's expectations in another: mere's exposure to P1 was two crates, not four.
- The new crate exposed the machine-local patch table's missing-entry trap in
  its duplicate-crate form. Patched `cambium` reaches `mere-surface-api` by
  workspace path while `knot-document` reaches it by git, and two copies of one
  crate is an `E0308` on `SurfaceDescriptor`, not a resolution failure. The
  entry is added to the committed `.cargo/config.toml.example` with that reason,
  beside the `genet-render-host` note that records the trap's first form.
- Genet's head did not compile when this repoint began. The relicense sweep
  `957926e4e8a` added `PointerButton` to cambium's `lib.rs` re-export without
  the `pointer.rs` half that defines it, which existed only in a working tree
  and on no branch: a sweep commit that captured half of an active lane. Genet
  closed it as `388d89c3a64` and this repoint targets that, so invariant 7 is
  satisfied by a receiving revision that is green rather than merely public.
- Receipts. `cargo check --workspace` **unpatched at head** — run from outside
  the tree so the machine-local patch table does not load, witnessed by
  `cambium v0.3.3` resolving from `genet.git?rev=388d89c3` rather than a path —
  is green in 7m 58s with 0 errors and 182 warnings across nine crates, all
  pre-existing. The ordinary patched run is green in 37s with the same set.
  `cargo test -p knot-document` passes 9, `cargo test -p distillery` passes 13,
  `walk_fixtures` among them being a file this change edited. No warning was
  fixed and none is new.
- Two consumers stay unproven and open. **`ports/graphshell/web`** was not
  built: its own `.cargo/config.toml` patches genet to the shared worktree
  `worktrees/genet-head`, which sits at `577e2471e97`, thirty-seven commits
  behind head and older than all four P1 commits, so a build there would mix a
  pre-P1 cambium with head-pinned workbench, taffy and parley. Refreshing that
  worktree belongs to the lane that owns it, not to this change; its ten pins
  are repointed and its proof is deferred. **`ports/knot/desktop`** cannot be
  built standalone at all, for two pre-existing faults this repoint found and
  deliberately did not fix: it is nested inside the member `ports/knot` yet
  carries no empty `[workspace]` table, so the root's `exclude` entry cannot
  reach it and even `--manifest-path` refuses, contradicting the exclude
  comment's claim that it stays buildable that way; and given such a table it
  then inherits none of the root's `[patch.crates-io]` restatements, so
  `genet-livery` resolves published parley 0.10.0 and fails on
  `AlignmentOptions::last_line_alignment` and `StyleProperty::TabSize` —
  exactly the trap the root manifest's own comment predicts. Its three pins are
  repointed; making it buildable wants both fixes and is P4 work.
