# Platform Boundary and Repository Topology Plan

**Date:** 2026-09-02  
**Status:** **P0-P6 landed 2026-09-02 through 2026-09-05.** Woodshed's P4
exception closed 2026-09-04 with single-source, software, and headed receipts.
P6's public topology and hosted Pages receipt closed 2026-09-05; the follow-up
domain verification, certificate issuance, and HTTPS enforcement are also
complete. P7 remains open behind the documentation-policy D3
historical-disposition gate. Vello
V1-V4 are owned separately by Netrender's durable upstream-ask note.
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

The original `mark-ik/hybrid-scene-append` branch at `c73ba2c3` adds
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

Mark activated the consumer experiment on 2026-09-03. The refreshed
`mark-ik/all-vellos` branch at
`ca3f40ea182216883cd543c7b9deae991268917c` composes the append prototype with
the wgpu-30 upgrade on current upstream. Netrender pins that immutable revision
behind opt-in `vello-cpu`, `vello-hybrid`, and `vello-all` features. Classic
remains the shipping path; CPU and Hybrid share one sparse lowerer from the
same authoritative Netrender `Scene`.

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

#### V3. Admit consumers incrementally on proof

- Keep the Vello choice at the Netrender backend boundary. Cambium and product
  consumers continue to emit one Netrender `Scene`.
- Admit CPU and Hybrid operations through an explicit capability table and
  typed refusals; never silently drop an unsupported scene operation.
- Prove CPU pixels, Hybrid rendering on Netrender's shared wgpu device, and
  Hybrid native scene append before connecting product selection.
- Then wire images, text, registered fragments, and the rasterizer-independent
  corpus. Measure fragment invalidation separately for each backend.

**Done when:** all three are selectable for a named target, preserve the
admitted corpus, and carry measured fragment-level invalidation and
composition receipts appropriate to their native form.

**In progress 2026-09-03:** the three-backend vocabulary, immutable dependency
pin, shared CPU/Hybrid lowerer, typed refusals, CPU pixel receipt, Hybrid append
receipt, and Hybrid GPU readback on Netrender's shared wgpu device exist.
Product selection, images, text, filters, registered-fragment wiring, and
corpus parity remain open.

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
| P2 | [Workbench component plan](../../cambium_docs/implementation_strategy/2026-08-31_workbench_component_plan.md) | W4 landed on genet `main` | `codex/workbench-core-20260831` is already on `main`; the four remaining `codex/workbench-followups-*` and `codex/pelt-*` branches (nine commits) touch none of the four mixed crates, so P1 may split them, but Workbench itself must not move under an open lane |
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

**Status (2026-09-03): met, all four clauses.** Genet's removal landed at
`a93189b1d7c` and mere's landing at `725bbf1a`; the receipts for each clause are
in the Progress entries below. The content ran in the order the second-pass
assessment recommended — Pelt and its companions first, Cambium after — so P3's
consumer half is landed and P3's remaining items (Inker's upper controller,
document-canvas, Nematic, Errand, Fleece, Illume, Tinct, Verso, and Knot's
reconciliation) are what is left of that phase.

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

**Status (2026-09-04): all five clauses met.** Four landed on 2026-09-03; the
fifth, Knot's reconciliation, landed the next day with the extraction plan's
E2 and is recorded at the end of this paragraph. The content moved in three motions — Pelt and its companions,
then Cambium and Workbench, then the engine-management layer — with genet's
removals at `75d3900f82e`, `ce79fd44a4d` and `6d8daca939b` and mere's landings
at `cb3fd887`, `91bf62c9` and `13b64e30`. **One source identity:** every moved
crate is a mere workspace member resolving from a path, and the unpatched
resolve reports 23 genet-derived packages at exactly one revision and no
duplicate package name in the family. **Genet's remaining workspace is
engine-shaped:** its members are the Servo-derived platform crates, the lower
libraries, WPT, and one raw host, ortet. **Pelt and Tabard run from Mere:** the
Pelt article receipt renders `digest=b1d6a62acf85b553` from mere, byte-identical
to genet's last commit that had Pelt. **Moved docs are indexed only here:**
`inker_docs/`, `nematic_docs/` and `verso_docs/` are in mere's `DOC_README.md`
and genet's carries a note saying where they went. **Knot's standalone and
embedded surfaces share one document model, 2026-09-04:** that clause was the
knot-editor extraction plan's E-series work, listed in this plan's own
prerequisites table, and its E2 closed it. `knot-editor` and `knot-document`
are no longer mere members; the standalone process is the knot-editor
repository's `apps/desktop`, and Djinn, Turnstone and that process all resolve
the same `knot-document` from knot-editor
`fcd004b655b595038eba0a7e49f209b8477edadf`, so no graph in the family carries
two of it. `knot-editor-host` stayed, as inker-family integration code, and
moved to `crates/inker/knot-editor-host`.

Fleece did not move: §9.1's census reclassed it independent, so P3's list names
one crate it does not own.

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

**Status (2026-09-05): met.** The Mer3ly
[P6 receipt](../../../../mer3ly/docs/receipts/org-transfer/2026-09-05_p6_platform_topology.md)
records the accepted Pages deployment and the completed Cloudflare DNS,
organization-domain verification, certificate, redirect, and HTTPS checks.

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

### 2026-09-03 — all-three Vello experiment activated

- Mark selected Classic, Hybrid, and CPU as three realizations of one
  Netrender scene contract, rather than replacement architectures.
- The refreshed fork branch combines current upstream, wgpu 30, and the Hybrid
  retained-append prototype. Netrender consumes it only behind opt-in features
  at an immutable revision.
- The first adapter slice deliberately covers geometry, gradients, and layers,
  with CPU pixel and Hybrid GPU readback receipts. Images, text, filters, and
  fragments return typed admission errors until their resource and retention
  contracts are wired.

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

### 2026-09-03 — Vello experiment

- Refreshed and pushed `mark-ik/all-vellos` at immutable commit
  `ca3f40ea182216883cd543c7b9deae991268917c`, combining current upstream,
  wgpu 30, and Hybrid retained append.
- Added Netrender's opt-in all-three vocabulary and its shared CPU/Hybrid
  geometry, gradient, and layer lowerer. Focused CPU pixel, common-subset,
  explicit-refusal, and Hybrid append tests pass.
- Classic remains the shipping path. Resources, fragment registry wiring,
  corpus parity, and product selection are the remaining V3 gates. Upstream
  contact remains unstarted.

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

- 2026-09-03: **ortet founded in genet** (`9b32f3defe7` O0, `f41d5bfa2b2`
  O1; plan `genet/design_docs/2026-09-03_ortet_founding_plan.md`). This
  answers §9.3 ruling 4 the second way: a smaller host takes the raw-host role
  rather than Pelt shedding its lanes. `ports/ortet` is one binary over
  `genet-winit-host`, `genet-render-host`, `genet-documents`' Livery lane,
  `document-session-api`, `genet-host-api` and `netfetcher`; it drives the
  session itself, has no chrome, and its receipts are self-driven (`--frames`,
  `--artifact`, a two-verb `--actions`). The cone witness gained
  `assert_ortet_cone`: 592 packages from ortet over normal edges, none of
  inker, workbench, cambium*, mere-*, nematic, errand, document-canvas, pelt*,
  tabard or knot-editor-host, with a positive control over `pelt-desktop` that
  reports eleven of them. `fleece` is in the cone through `genet-documents`'
  clip lane and is named separately on every run; that is consistent with
  §9.1, which reclassed fleece independent, and the founding plan's first
  draft, which listed it as Mere, is corrected. Receipts: the article fixture
  digests identically across two runs (`0x6377ba8a6bf4dbc9`), the scrolled,
  in-page-link and cross-document runs each move the digest and settle where
  they should. The Pelt manifest assertion stays in the witness until Pelt
  moves (ortet plan O4). Name ruled by Mark 2026-09-03; the crates.io claim
  waits on a publishable dependency set, since the host crates it sits on are
  `publish = false`.

- 2026-09-03: **Pelt and its companions left genet** (genet
  `75d3900f82e`, parent `8c1e324ed4d`). The first half of the consumers-first
  order in the P2 assessment above: four paths removed from genet's workspace,
  129 tracked files, 35,446 deleted lines.

  | path | packages | genet dependents at removal |
  |---|---|---|
  | `ports/pelt` | `pelt`, `pelt-core`, `pelt-desktop` | none outside Pelt itself |
  | `ports/tabard` | `tabard` | **none at all** |
  | `components/inker/knot-editor-host` | `knot-editor-host` | none |
  | `components/mere-document-lanes` | `mere-document-lanes` | `pelt`, `pelt-desktop` only |

  Each claim was checked with `cargo tree -i <crate> --workspace` before
  anything was removed, and one expectation was wrong in the safe direction:
  **`tabard` is not a `pelt-desktop` dependency.** No manifest in genet named
  it; it is a reservation crate with no dependent anywhere, so it left as a
  leaf rather than with Pelt. `mere-surface-api` stayed, as planned:
  `cambium`, `cambium-genet-winit-host` and `knot-editor-host` all reached it,
  and after the removal `cambium` still does.

  **History did not come out on a branch, and the Cambium precedent does not
  transfer.** All four `git subtree split -P <path> -b split/<name>` runs were
  started first, given more than the fifteen-minute budget, and produced
  nothing; they were stopped and no `split/*` ref exists. The reason is
  specific and checkable: `components/cambium` carries a `git subtree add`
  join commit (`6e37c2c41e7`, "Add 'components/cambium/' from commit
  a2a25b78f74"), and `find_existing_splits` uses that join to bound the walk,
  which is why the probe reported 738 commits in 275 s. These four paths were
  never subtree-added — `git log --grep=git-subtree-dir` over genet returns
  exactly three commits, for `components/netfetcher`, `components/cambium` and
  the 2013-era `src/components/script/style/` — so git-subtree walks the whole
  58,242-commit history in shell, twice (once to count `revmax`, once to
  process). Measured rate on this machine, from the split cache's `notree`
  entries: about 250 commits a minute, roughly 3.9 hours per path, with all
  four together no faster. `ports/pelt` reached 6,331 of 58,242 in 25 minutes.

  So the receiving side has three options, and this is **Mark's call**:

  1. **Copy plus pointer**, the fallback the move already names. Cheapest, and
     the history stays legible in genet at `8c1e324ed4d`.
  2. **Run the splits offline**, four hours a path, unattended. Nothing is
     wrong with them; they are only slow.
  3. **`git filter-repo --subdirectory-filter` in a throwaway clone**, then
     fetch the four branches back. Exact, preserves the DAG, authors, dates
     and messages, and takes minutes rather than hours — but `git-filter-repo`
     is not installed on this machine (not in Git for Windows), so it is a new
     tool on Mark's box.

  Whichever is chosen, **`8c1e324ed4d` is the revision to split or copy from**;
  every removed file is intact there.

  **Genet after the removal.** `default-members = ["ports/ortet"]`, and
  `assert_ports_depend_inward` asserts ortet's single manifest at
  `ports/ortet/Cargo.toml` where it asserted Pelt's two (ortet plan O4). The
  cone witness's positive control had to be rebuilt: `pelt-desktop`'s cone
  exercised an exact forbidden name (`inker`) and the `cambium`/`mere-`
  prefixes in one walk, and no remaining member reaches both — `cargo tree -p
  cambium` and `cargo tree -p cambium-genet-winit-host` reach `inker` zero
  times, checked. It is now two controls: `document-canvas` must report
  `inker`, `cambium-genet-winit-host` must report `cambium`.

  Receipts, all in genet: `cargo check --workspace` green, 0 errors and 24
  warnings across eight crates, every one pre-existing and none touched; root
  `cargo build` (default member ortet) green; `cargo check -p ortet` green;
  `cargo test -p ortet` 10 passed; `cargo check -p netfetcher
  --no-default-features` green; `python support/ci/check_dependency_cones.py`
  passes, ortet's cone still 592 packages with none forbidden and `fleece`
  named separately as before. `relicense_headers.py --audit` went 887 -> 843
  owned sources, exactly the 44 sources in the four directories, with "without
  Exhibit A" unmoved at 6, all in other lanes.

  Files changed outside the four directories: genet's root `Cargo.toml` (six
  workspace entries and the `default-members` line), `support/ci/check_depen`
  `dency_cones.py`, `README.md` (the default member and its run examples),
  `ports/ortet/README.md`, `components/netfetcher/README.md`,
  `components/genet-documents/README.md` and
  `design_docs/2026-09-03_ortet_founding_plan.md`. Prose in
  `components/cambium/cambium/src/{frisket,field,tests}.rs` and in genet's
  `docs/` still names `pelt-desktop` and `ports/pelt`; that is history and
  another lane's tree, and it was left alone.

  Not done here: nothing landed in mere. P3's other half — Cambium and
  Workbench, with no genet consumer left — is still ahead, and the crates now
  need a home in mere before their consumers can be repointed.

- 2026-09-03: **the four crates landed in mere** (merge commits
  `39904694`, `e4263609`, `dfd9276f`, `a57e9f62`; wiring `cb3fd887`). The
  receiving half of the consumers-first order. History came in by
  `git fetch <bare> HEAD:import-<name>` plus
  `git merge --allow-unrelated-histories`, from four bare repositories exported
  path-limited from genet `8c1e324ed4d`, so option 3 of the three the removal
  offered — an exact DAG in minutes rather than four hours a path — is what was
  taken. Each landed tree hashes identically to its bare tip
  (`git rev-parse HEAD:<path>` against the bare repo's), and nothing outside
  the four paths moved in any merge: 129 files, exactly genet's count.

  | path | packages | commits | tree |
  |---|---|---|---|
  | `ports/pelt` | `pelt`, `pelt-core`, `pelt-desktop` | 113 | `f76f8a4edee` |
  | `ports/tabard` | `tabard` | 6 | `7dcb8836bbd` |
  | `ports/knot/editor-host` | `knot-editor-host` | 10 | `5b5dd8298f5` |
  | `crates/system/document-lanes` | `mere-document-lanes` | 2 | `156319ea3ae` |

  `ports/knot/editor-host` is nested inside the member `ports/knot`, which
  needed no `exclude` change: cargo auto-excludes a subdirectory carrying its
  own `Cargo.toml` from the parent package, and `exclude` names only
  `ports/knot/desktop`, a different path.

  **One revision, and the pins that had to be invented.** Every genet.git pin
  in the tree moved from `388d89c3a64` to `b78e2b92251`, genet's head after the
  removal. Of the 39 existing lines one, `knot-editor-host`, became a workspace
  path instead; fifteen new ones joined, leaving **53 across five manifests at
  one revision**, witnessed by grep and by two `cargo metadata` runs.
  Pelt's manifests reached genet by relative path
  (`../../../components/...`); those became git pins declared once in the root
  `[workspace.dependencies]`, as mere already does for the rest of the family,
  so a future repoint stays a one-file edit. `knot-editor-host` and
  `mere-document-lanes` went the other way, from git pins to workspace paths.
  The unpatched resolve reports **39 genet-derived packages at
  `b78e2b92251` and nowhere else**, with `cambium 0.3.3` coming from
  `genet.git?rev=b78e2b92251` rather than a path — the proof that the
  machine-local patch table was not loading — and no duplicate package name
  anywhere in the genet family in either resolve.

  **The patch table gained more than it lost.** `knot-editor-host` left it,
  because patching a git source at a path that is also a workspace member is a
  hard lockfile collision; none of the four may ever appear there. Seventeen
  entries were added, and three of them (`workbench`, `scrying-engine`,
  `genet-probe`) were already named by mere's root manifest and simply missing
  — the same missing-entry trap the file's own notes record twice, caught here
  because `genet-probe` resolved once from git and once from a path in the same
  graph. The committed `.cargo/config.toml.example` carries all of it.

  **Two findings the landing exposed, both latent before it.**

  1. `ports/knot` uses `nematic::HtmlFragmentEngine` while mere's workspace pin
     is `default-features = false`. That compiled only because
     `knot-editor-host` resolved from genet.git and pulled nematic's defaults
     in through *genet's* workspace table. As a member it cannot, so
     `ports/knot` now names `features = ["html-fragment"]` where it uses it.
  2. Two `pelt-desktop` tests failed on a missing image, and the `article`
     receipt would have rendered a different frame. Pelt's fixtures reference
     `resources/servo_64.png` and `resources/servo_1024.png` by
     **repository-root-relative** URL, four, five and six `..` segments up, so
     they sit outside the four exported paths and did not travel. Both are
     carried here byte-identical from genet as `resources/`, with a README.
     They are not relocated under `ports/pelt/` because those authored URL
     strings are exactly what `static_viewer.rs` asserts on — the tests
     exercise relative resolution at each depth, so moving the files would mean
     rewriting the strings under test. **Open for Mark:** whether a top-level
     `resources/` of two Servo-derived fixture PNGs is the right shape for
     mere, or whether the fixtures and their assertions should be rewritten.

  **Receipts.** `relicense_headers.py --audit` went 1111 -> 1155 owned
  sources, exactly the 44 the removal took out of genet, with "without
  Exhibit A" and "Exhibit B hits" at 0 before and after.
  `cargo check --workspace` **patched** (the ordinary loop) green, 0 errors,
  192 warnings across twelve crates; the nine crates and 182 warnings of the
  2026-09-03 repoint are unchanged, and every new warning is in a crate that
  landed today (`mere-document-lanes` 7, `pelt-desktop` 1) or in `nematic`,
  whose html-fragment module now compiles (2). No warning was fixed.
  `cargo check --workspace` **unpatched at true head** — run from a
  directory outside the tree so the machine-local patch table does not load,
  witnessed by thirty-five genet crates checked from
  `genet.git?rev=b78e2b92251` and none from the sibling path — is green in
  41.6 s with 0 errors and 190 warnings across eleven crates: the twelve above
  minus `nematic`, whose warnings cargo does not surface when it arrives as a
  git dependency rather than a patched path.
  `cargo check -p pelt --no-default-features --features livery` green;
  `cargo check -p pelt-desktop --features livery` green;
  `cargo check -p knot-editor-host` green; `cargo check -p mere-document-lanes`
  green. `cargo test -p pelt-core` **6 passed**, `cargo test -p pelt-desktop
  --features livery` **64 passed** — genet's numbers exactly, once the fixture
  images were carried; before them it was 62 passed and 2 failed.
  `scripts/check_port_boundaries.py` passes.

  **The headed article receipt**, this step's done-condition, runs from mere:
  `cargo run -p pelt --no-default-features --features livery -j 1 --
  --product-receipt article --artifact <png>` reports
  `assertion=jump-link press/release moved the retained viewport` and
  `digest=b1d6a62acf85b553`, identical across two runs. The frame is non-blank
  by construction — `capture_composition` refuses a blank frame rather than
  writing one — and by inspection: a 960x640 PNG with the Ahem-set route text,
  the separate-border table, and **both** servo icons, the CSS background and
  the `<img>`, which are the two resources the missing-asset finding was about.
  **The same receipt was then run from genet's last commit that still had Pelt
  (`8c1e324ed4d`, in a throwaway worktree) and reports the same digest,
  `b1d6a62acf85b553`, with a byte-identical 50,836-byte PNG.** So the move
  changed nothing a user would see: Pelt renders the article receipt from
  mere exactly as it did from genet. That cross-repository comparison, rather
  than the digest alone, is what the P2 done-condition asks for. It also
  supersedes the reading in the landing report that the digest had moved from
  genet's 2026-08-25 `973595d7fbd90151`; that older figure belongs to an
  earlier fixture state, not to the repository move.

  **Still open.** `ports/graphshell/web` and `ports/knot/desktop` had their ten
  and three pins repointed and remain unproven for the same two pre-existing
  reasons the 2026-09-03 repoint recorded; nothing here changed either. The
  Circuit recipe's `workspace_graph.json` fixture is now one generation behind
  the member list; its test reads the committed snapshot rather than live
  `cargo metadata`, so nothing fails, and regenerating it belongs to the lane
  that owns it. P3's other half — Cambium and Workbench, with no genet consumer
  left — is next, and Pelt's genet pins become workspace paths in that same
  motion.

- 2026-09-03: **the four open items from the Pelt landing are closed**
  (`020c0449`, `f75db463`, `76a4c58c`, `ac3d1024`, this entry). Each was
  recorded above as open or deferred; none needed the P3 work ahead of it.

  **1. The fixture images moved under Pelt, and the strings were rewritten**
  (`76a4c58c`, plus the three file moves in `020c0449`; **Mark's ruling** on
  the question the landing left open). `resources/servo_64.png` and
  `resources/servo_1024.png` are now `ports/pelt/examples/resources/`, beside
  the fixtures that use them, and the top-level `resources/` is gone. The
  landing kept them at the root because the authored URLs are themselves under
  test, and that was right about the constraint and wrong about the
  conclusion: **the depth is what the tests exercise, not the destination.**
  `static_viewer.rs` asserts the p5 fixture's authored URL exactly and its
  *unnormalized* resolved URL, `..` segments and all. So every URL keeps the
  climb it had — four segments from `livery-route/index.html`, five from
  `livery-route/assets/route.css`, six from `p5-resources/final/styles/root.css`
  and from the four assertion strings — and then descends into
  `ports/pelt/examples/resources/`. A one-segment `../resources/` would have
  resolved correctly and silently stopped testing relative resolution at those
  depths. `LICENSES.md`'s retained row is now `ports/pelt/examples/resources`,
  same license, upstream and notice text, paragraph corrected.

  Receipts: `cargo test -p pelt-desktop --features livery` **64 passed; 0
  failed**, genet's number and the landing's. The article product receipt
  reports `assertion=jump-link press/release moved the retained viewport
  digest=b1d6a62acf85b553` with a 50,836-byte PNG — **the same digest and the
  same byte count the landing recorded on both sides of the repository move**,
  so the relocation changed nothing that renders.
  `relicense_headers.py --repo . --audit`: 1155 owned sources, **0 without
  Exhibit A**, 0 Exhibit B hits, and the ledger path resolves to the new
  directory.

  **2. `ports/knot/desktop` builds standalone** (`020c0449`). Both pre-existing
  faults are fixed as the 2026-09-03 repoint described them. It has an empty
  `[workspace]` table, which is what actually keeps it out of the workspace —
  the root `exclude` entry never could, because the path sits inside the member
  `ports/knot`. Given the table it restates the two `[patch.crates-io]` entries
  its graph needs, `taffy` (as `genet-taffy`) and `parley`, at the same genet
  revision the root names and with the root's own reasons; without them
  `genet-livery` resolved published parley 0.10.0 and failed on
  `AlignmentOptions::last_line_alignment` and `StyleProperty::TabSize`, exactly
  as predicted. The root manifest's exclude comment claimed the port "remains
  buildable by manifest path"; that was false and is corrected for both nested
  entries, `ports/graphshell/web` included.

  Compiling it for the first time found a third fault, in code no build had
  ever reached: `focused_text`'s `get` slot closure is an `E0282`, type
  annotations needed. One parameter type. Receipt: `cargo check
  --manifest-path ports/knot/desktop/Cargo.toml` green, `Finished dev profile
  in 25.75s`.

  **3. `ports/graphshell/web` is proven** (`f75db463`). The shared worktree
  `worktrees/genet-head` was clean, and went from `577e2471e97` to
  `b78e2b92251` by detached checkout — the revision every genet.git pin here
  names. That alone was not enough. The port's gitignored `.cargo/config.toml`
  states its own invariant, that its patch table is the **union** of itself and
  the root config's, all pointed at the worktree; sixteen names the root gained
  since had never been added, so the root table supplied them from the genet
  *working copy* and the first build died on `package collision in the
  lockfile: workbench ... are different`. The missing-entry trap in its
  loudest form yet: a hard error rather than a duplicate crate or a silent
  mismatch. Sixteen entries added, and `knot-editor-host` removed — it left
  genet with Pelt and no longer exists under the worktree at all.

  Receipts, both from `ports/graphshell/web`: `cargo check` green in 1m 34s,
  `cargo check --target wasm32-unknown-unknown` green in 1m 15s. Only the
  reasons are machine-independent, so they, and not the absolute paths, went
  into the committed `.cargo/config.toml.example`, which had said nothing about
  the patch table at all.

  **4. The Circuit workspace-graph fixture is current** (`ac3d1024`).
  Regenerated with `scripts/workspace_graph_fixture.py`; **31 lines added, 1
  removed** — two packages (`knot-editor-host`, `tabard`), six edges, and the
  `generated_from` sha. One edge corrects a claim from the removal: **`pelt`
  `-desktop` does name `tabard`**, optionally, behind `tabard-preview`, and
  `cargo metadata --no-deps` reports optional dependencies like any other.
  Genet's "no dependent anywhere" was true of genet's manifests, not of Pelt's.
  Receipt: `cargo test -p distillery` **13 passed, 0 failed**, the landing's
  number.

  **Two things to know about how these receipts were taken.** The genet
  working copy went mid-edit under P3 while this work ran — `components/`
  `cambium` is being removed from it right now — and mere's machine-local
  patch table points there, so the ordinary patched loop failed with `failed
  to load source for dependency cambium` and, once, with a Windows `os error
  267` as a directory vanished under a running `rustc`. The article receipt and
  the distillery test were therefore taken **unpatched, run from a directory
  outside the tree** so the machine-local table does not load and genet
  resolves from git at `b78e2b92251` — the same technique, and the same
  reasoning, as the unpatched witnesses above. That is a stronger receipt, not
  a weaker one: it is the pinned revision rather than a neighbour's working
  copy. Items 2 and 3 were checked before that removal reached them.

  And one mis-bundling, recorded rather than repaired: `git mv` stages
  immediately, so the three file moves of item 1 were already in the index when
  item 2 was committed and rode into `020c0449`. `76a4c58c` carries every
  content change of item 1 and none of the moves. Nothing is lost and nothing
  is pushed; forward commits are safe and history surgery to un-bundle is not
  the trade.

  **Still open after this.** Nothing from the landing's list. P3's other half —
  Cambium and Workbench, with no genet consumer left — is in flight in genet as
  this is written, and Pelt's genet pins become workspace paths in that motion.

- 2026-09-03: **P2 assessment, second pass, after Pelt left.** The cluster
  is now a leaf. `cargo tree -i` over the nine Cambium members (`cambium`,
  `cambium-rootstock`, `cambium-winit`, `cambium-winit-a11y`,
  `cambium-genet-winit-host`, `cambium-genet-web-host`, `cambium-nematic`,
  `meristem`, `sprigging`), `workbench` and `mere-surface-api` reports no
  dependent in genet outside the cluster itself, so the eleven can leave in
  one commit without breaking any member that stays, and invariant 1 holds
  at that commit by construction. History travels the way Pelt's did:
  path-limited `git fast-export` from the last commit that has the code,
  prefixes rewritten in the stream to the landing paths, trees verified
  identical; the July `cambium-history-probe` branch (328 commits) is
  superseded, since its tip predates three Cambium commits and the export
  takes seconds. Landing shape as proposed and ruled: the nine under
  `crates/cambium/`, Workbench at `crates/cambium/workbench`,
  `mere-surface-api` at `crates/system/surface-api`, the scene family
  (`sceno`, `scenomise`, `scenotime`) from `crates/scenograph/` to
  `crates/cambium/scenes/`, and the `scenograph` facade crate deleted: it
  has no consumer left in mere, and its one substantive file, the 528-line
  solver registry behind `sceno::Arrangement::Custom`, moves into
  `scenomise`, which already owns the families and the solve path. The
  published `scenograph 0.0.4` name stays held for the editor. Cambium's own
  documents (`components/cambium/docs/`, with its history and receipts)
  travel inside the exported tree; the two live plans in genet's
  `design_docs/` (`2026-08-31_workbench_component_plan.md`,
  `2026-09-03_host_ui_zoom_plan.md`) and their index entries move to mere's
  `design_docs/` in the same change, per the plan's "docs with their code";
  the older engine-era notes in genet's `docs/` stay where they are and are
  cited by path. On the genet side the witness loses `mere-surface-api` from
  its leaf table and `cambium*` from every positive control, so the ortet
  cone's prefix half of `is_ortet_forbidden` gets a direct unit assertion in
  place of a live crate. Blocker at assessment time: none. The Cambium lane
  that was dirty all day committed its work at 14:56 (two commits, unpushed);
  genet's tree is clean, and those two commits ride to origin with the
  removal.

- 2026-09-03: **Cambium, Workbench and `mere-surface-api` left genet** (genet
  `ce79fd44a4d`, parent `5b509d25507`). The second half of the consumers-first
  order: three paths removed from genet's workspace, 176 tracked files, 52,360
  deleted lines. Eleven packages in one commit.

  | path | packages | genet dependents at removal |
  |---|---|---|
  | `components/cambium` | `cambium`, `cambium-rootstock`, `cambium-winit`, `cambium-winit-a11y`, `cambium-genet-winit-host`, `cambium-genet-web-host`, `cambium-nematic`, `meristem`, `sprigging` | none outside the cluster |
  | `components/workbench` | `workbench` | `cambium`, `mere-surface-api` only |
  | `components/mere-surface-api` | `mere-surface-api` | `cambium` only |

  Every claim was checked with `cargo tree -i <crate> --workspace --prefix
  none` before anything was removed, and `--target all` for the four whose
  edges are target-gated (`cambium-rootstock`, `cambium-winit`,
  `cambium-winit-a11y`, `cambium-genet-web-host`; the web host resolves to
  nothing on the host target). Every reverse dependency of all eleven was
  itself one of the eleven, so the cluster was a true leaf and invariant 1
  holds at this commit by construction. Nothing was wrong in either direction
  this time.

  **History came out in seconds, and the Pelt entry's three options were not
  needed.** `git subtree split` was not attempted: the Pelt entry already
  measured why it cannot work here. The path-limited export it named as the
  exact alternative was used instead — `git fast-export --signed-tags=strip
  --tag-of-filtered-object=drop main -- <path>`, the path prefix rewritten in
  the stream by a filter that touches only `M <mode> <dataref> <path>` and
  `D <path>` command lines (copying `data` blocks verbatim by declared length,
  so no line inside a blob can be mistaken for a command), then
  `git fast-import` into a bare repository:

  | genet path | landing prefix | bare repo | commits | elapsed |
  |---|---|---|---|---|
  | `components/cambium` | `crates/cambium` | `cambium-history.git` | 91 | 1 s |
  | `components/workbench` | `crates/cambium/workbench` | `workbench-history.git` | 1 | 2 s |
  | `components/mere-surface-api` | `crates/system/surface-api` | `surface-api-history.git` | 1 | 1 s |

  Each was verified by tree identity, not by inspection: `git --git-dir=<bare>
  rev-parse refs/heads/main:<landing prefix>` equals `git rev-parse
  main:<genet path>` in genet, for all three — `593ffeb82f1`, `ae9ab404f5f`,
  `e5aef208675`. The commit counts match `git log --oneline -- <path>` in
  genet exactly (91/1/1), and the filter reported 752/2/4 path lines rewritten
  with **zero** unmatched path lines, so nothing was silently left behind.
  The July `cambium-history-probe` branch (which the probe took 275 s to
  produce 738 commits for) is superseded and was not used. The three bare
  repos are the artifact for the mere side.

  A throwaway worktree was meant to pin the export, per the Pelt procedure,
  but `git worktree add --detach` on genet did not finish inside ten minutes —
  the tree is large and the worktree's only job is pinning. It was removed and
  the exports were pinned instead by exporting from `main` while asserting
  `main` still equals `5b509d25507` before and after each run, which is the
  same guarantee for none of the cost. Note for anyone repeating this:
  `git fast-export <raw-sha>` emits the sha *as the ref name*, and
  `fast-import` then produces a repository with no usable ref — pass a ref
  name, not a sha.

  **Docs travelled with their code.** `2026-08-31_workbench_component_plan.md`
  and `2026-09-03_host_ui_zoom_plan.md` were removed from genet's
  `design_docs/` and copied verbatim (md5-checked) to
  `<scratch>/cambium-docs/` for mere; `DOC_README.md` keeps a one-line note
  under its "cambium" heading saying where they went, and its now-empty
  "workspace composition" heading was dropped with the Workbench entry.
  Cambium's own `components/cambium/docs/` travelled inside the exported tree.
  `docs/2026-08-09_cambium_desktop_host_g1_receipt.md` **stays in genet** —
  `genet-livery`'s `src/lib.rs` and `tests/deep_nesting.rs` still cite it —
  but the moving zoom plan cites it too, so a copy is in the scratch folder
  and mere's copy of that plan will need its link repointed at genet.

  **Genet after the removal.** The cone witness loses `mere-surface-api` from
  its host-api leaf table. Its ortet positive control kept the exact-name half
  over `document-canvas` -> `inker`; the prefix half, which the Pelt entry had
  just rebuilt onto `cambium-genet-winit-host`, has no member left to carry it,
  so it is asserted directly on the predicate: `is_ortet_forbidden` must
  forbid `cambium-anything`, `mere-anything` and `pelt-anything` and must
  admit `genet-livery`. That is weaker than a cone walk — it proves the rule,
  not the walk — so it is paired with the live `document-canvas` control
  rather than replacing it, and the code says so. Every name stays in
  `ORTET_FORBIDDEN`: it is a set of names the cone may not reach, not a set of
  members, so it fails loudly if any of them ever returns from mere.

  Receipts, all in genet: `cargo check --workspace` green; `cargo check -p
  ortet` green; `cargo test -p ortet` 10 passed; `cargo run -p ortet -- --url
  ports/ortet/examples/article.html --frames 3` renders to digest
  `0x6377ba8a6bf4dbc9`, unchanged, so the engine's output is untouched by the
  removal; `cargo check -p netfetcher --no-default-features` green;
  `check_dependency_cones.py` passes with ortet's cone still 592 packages,
  none forbidden, and `fleece` named separately as before.

  Two receipt deltas worth recording rather than hiding:

  1. `cargo check --workspace` gained exactly **one** warning, and the
     per-crate `generated N warnings` set is byte-identical to the baseline.
     The new line is `patch vello v0.10.0 ... was not used in the crate
     graph`: `vello` was reached only through `cambium-rootstock`, so the
     patch is now dead. It was **left in place** — its own comment in
     `Cargo.toml` sets its retirement condition ("retires by deletion when a
     wgpu-30 vello (0.11) ships"), which has not been met, and genet already
     carries two other unused-patch entries on the same footing. Deleting it
     is a resolution decision, not a consequence of this move.
  2. `relicense_headers.py --audit` went **845 -> 713** owned sources, exactly
     the 132 owned sources in the three directories (126 carrying Exhibit A
     plus meristem's 6 that did not). "Without Exhibit A" went **7 -> 1**, not
     `0 -> 0`: the audit was already failing before this commit. Six of the
     seven were meristem's and left with it. The seventh,
     `components/genet-livery/src/paint.rs`, arrived in the Cambium lane's own
     `86019eacccc` ("Scale the interface: UI zoom for the Cambium desktop
     host"), is in a crate that **stays** in genet, and was not touched here.
     It is a one-file `--apply` for whoever owns that lane. (The Pelt entry
     recorded 6 "all in other lanes"; those six were the meristem files, and
     the seventh appeared afterwards.)

  Files changed outside the three directories: genet's root `Cargo.toml`
  (eleven workspace members, two `[workspace.dependencies]` entries, and a
  departure note beside Pelt's), `support/ci/check_dependency_cones.py`,
  `README.md` (the component list and the LICENSES paragraph),
  `LICENSES.md` (`meristem` was the only row in the derivatives table; that
  table is explicitly *not* the tool's skip list, so the ledger path count is
  unmoved at 18 and the audit numbers above are comparable),
  `design_docs/DOC_README.md`, and `examples/genet_web_smoke/Cargo.toml`.
  `Cargo.lock` is gitignored in genet, so there is no stale lock to sweep.

  Prose naming `cambium` in `components/genet-clipboard/Cargo.toml`,
  `components/genet-probe/Cargo.toml`, `components/livery/properties.toml`
  (`consumer = "cambium"` — still true, just cross-repo now) and in genet's
  `docs/` was left alone: it is history or a still-accurate statement, not a
  path. `support/name-claims/{cambium,frisket,meristem,sprigging}` are name
  reservations, not workspace members, and are untouched.

  **Two open items for Mark.**

  1. `examples/genet_web_smoke` is deliberately *not* a workspace member, so
     `cargo check --workspace` never covered it, and it path-depended on
     `../../components/cambium/cambium`. It was repointed at
     `../../../mere/crates/cambium/cambium` — the ruled landing path — which
     is correct the moment mere lands the export and dangling until then. If
     the landing path changes, this is the one manifest that must follow.
  2. Four receipt harnesses in `scripts/` — `wayland-frame-receipt.ps1`,
     `windows-maximized-receipt.ps1`, `windows-snap-receipt.ps1`,
     `x11-shadow-receipt.ps1` — drive `cargo run -p cambium-genet-winit-host`
     against scenario files that left with the crate. They cannot work in
     genet any more. They were **not** removed: deleting four files outside
     the named moving set is a call to make deliberately, not a side effect.
     They belong with the host in mere, and copies are in the scratch folder
     for whoever lands it.

  Not done here: nothing landed in mere, and nothing was pushed. The commit
  sits on genet's `main` as the single unpushed commit.

- 2026-09-03: **the Cambium family, Workbench and `mere-surface-api` landed in
  mere** (merge commits `523a5979`, `5cc203bf`, `69ad7907`; scenes `3e4d5098`,
  wiring `91bf62c9`, headers `725bbf1a`). The receiving half of P2, against
  genet `a93189b1d7c`.

  History came in the way Pelt's did: `git fetch <bare> main:import-<name>`
  then `git merge --allow-unrelated-histories`, from the three bare
  repositories the removal exported path-limited with their prefixes already
  rewritten. Each landed tree hashes identically to its bare tip and to
  genet's, so these are the same objects, not a copy that looks alike:

  | path | packages | commits | tree |
  |---|---|---|---|
  | `crates/cambium` | `cambium`, `cambium-rootstock`, `cambium-winit`, `cambium-winit-a11y`, `cambium-genet-winit-host`, `cambium-genet-web-host`, `cambium-nematic`, `meristem`, `sprigging` | 91 | `593ffeb82f1` |
  | `crates/cambium/workbench` | `workbench` | 1 | `ae9ab404f5f` |
  | `crates/system/surface-api` | `mere-surface-api` | 1 | `e5aef208675` |

  `git diff --name-only <before> HEAD` after each merge listed **nothing**
  outside that merge's prefix: 168, 2 and 4 paths. The three landed trees hold
  **174** tracked files, exactly genet's count across the three directories at
  the parent of its removal commit. The import branches were deleted.

  **The scene family moved under the umbrella and the facade dissolved**
  (`3e4d5098`). `sceno`, `scenomise` and `scenotime` went from
  `crates/scenograph/` to `crates/cambium/scenes/`, keeping their names and
  versions; `crates/scenograph/README.md` came with them, corrected — its
  four-crate table is three, and its dual Apache/MIT license section, inherited
  from the standalone repository and already stale (mere has no
  `LICENSE-APACHE` or `LICENSE-MIT`, and every source in the three crates
  carries an MPL-2.0 header), is now MPL-2.0. The `scenograph` facade crate is
  deleted. `git grep` found no consumer of it anywhere in mere, in code or in a
  manifest — only prose in historical docs — and its 528-line solver registry
  moved into `scenomise` as `scenomise::registry`. The published
  `scenograph 0.0.4` name is untouched and stays held for the editor.

  **One thing had to be decided in the registry move, and it is a naming
  collision, not a design change.** The facade exported eight items at its
  root. Seven are re-exported at `scenomise`'s root exactly as the facade
  exported them. The eighth is `solve`, and `scenomise::solve` already exists
  and is consumed: it is the closed-form solve over the eleven named families,
  while the registry's `solve` dispatches an `Arrangement::Custom` id. The
  registry's was first left as `scenomise::registry::solve` (renamed `solve_via` on Mark's ruling the same day), said in
  the crate docs and in the README. **Open for Mark:** whether that is right,
  or whether the registry's should take a distinct root name. Nothing consumes
  either path today, so this is cheap to change now and expensive to guess at
  later. Two `scenomise::` self-references inside the registry became
  `crate::`, which the compiler caught immediately; that is the only code
  change in the whole file.

  **Wiring** (`91bf62c9`). Eleven new members, so 116 workspace packages where
  there were 106 — eleven in, the facade out. Every git pin of one of the
  eleven became a workspace path, in the root manifest and in the two
  standalone ports that name them directly (`ports/graphshell/web` for
  `cambium`, `ports/knot/desktop` for `cambium-genet-winit-host`). Seven patch
  entries left `.cargo/config.toml.example` and both live gitignored twins
  identically — `cambium`, `sprigging`, `workbench`, `mere-surface-api` and
  `cambium-genet-winit-host` from the genet table, `cambium` and `sprigging`
  from the crates-io one — because patching a git or crates.io source at a
  path that is also a workspace member is a hard lockfile collision. The files
  now say so, and name the four P3 crates under the same prohibition.

  The landed manifests reached genet's other components by relative path
  inside genet's tree (`../../genet-livery`, `../../shared/layout-dom` and
  eight more). Those became git pins declared once in the root
  `[workspace.dependencies]`, as mere already does for the rest of the family.
  `genet-render-host` had no root declaration at all — it existed only in the
  patch table, for `ports/graphshell/web` — and gains one; `windows-sys 0.61`
  joins for the winit host's Win32 frame geometry. Every genet.git pin in the
  repository moved from `b78e2b92251` to `a93189b1d7c` in the same motion:
  **56 lines across four manifests at one revision**, zero occurrences of the
  old one, witnessed by grep and by the unpatched resolve below.

  Two fields needed inlining. `workbench` and `mere-surface-api` inherited
  `rust-version` from genet's workspace and mere's does not declare it; it is
  inline at genet's `1.86.0`, unchanged, because the move is not an MSRV
  decision. Genet's workspace `authors` is the Servo Project Developers, which
  those two inherited and now inherit from mere instead; that is a correction,
  not a side effect. Every landed crate's `repository` inherits mere's rather
  than naming genet.

  **`examples/genet_web_smoke` came with the family.** It is not a workspace
  member here, as it was not in genet, so `cargo check --workspace` does not
  cover it; its `cambium` dependency is a plain relative path inside this
  repository again rather than the dangling `../../../mere/...` the removal
  left, and its four genet path deps and two patch entries became git pins at
  `a93189b1d7c` (cargo resolves `genet-taffy` and `sonic-rs` by package name
  anywhere in that repository, so `support/patches/` needs no special
  handling). That closes the first of the removal's two open items.

  **The four receipt harnesses came too**, closing the second:
  `wayland-frame-receipt.ps1`, `windows-maximized-receipt.ps1`,
  `windows-snap-receipt.ps1` and `x11-shadow-receipt.ps1` are in mere's
  `scripts/`, with their scenario paths corrected from `components/cambium/...`
  to `crates/cambium/...` (each `.scn` verified present) and their default
  output directory from `testing\genet\` to `testing\mere\`. Their
  `$repoRoot` / `$codeRoot` derivation needed nothing: `scripts/` sits at the
  repository root in both trees. The same two stale paths inside the host's own
  `examples/smoke.rs` and `smoke.scn` were corrected.

  **Docs.** `2026-08-31_workbench_component_plan.md` and
  `2026-09-03_host_ui_zoom_plan.md` are in
  `design_docs/mere_docs/implementation_strategy/` and indexed in
  `DOC_README.md`. Both are dated implementation plans about application
  composition, which is what `mere_docs/` holds; **open for Mark:** whether the
  Cambium family should instead have an area root of its own, which the policy
  would allow and two documents do not obviously earn. **Ruled 2026-09-03,
  later the same day:** yes — `crates/cambium/docs/` was independently found to
  be exactly the member-crate scatter §4 forbids, so the question was no longer
  "do two documents earn a root" but "does an already-scattered third source
  join them under one." Both plans and every `crates/cambium/docs/` file moved
  by `git mv` into the new `design_docs/cambium_docs/` (`implementation_strategy/`,
  `technical_architecture/`, `testing/`); see `DOC_README.md`'s
  `cambium_docs/` section and `DOC_POLICY.md`'s area-root list. The zoom plan's link to
  `docs/2026-08-09_cambium_desktop_host_g1_receipt.md`, which stays in genet
  because `genet-livery` still cites it, became the cross-repo path citation
  `genet/docs/...`; its `mere/design_docs/...` citation of the configuration
  ownership plan became a relative link, now that the plan is a neighbour.
  Each got a **Home** note saying where it came from and that `components/...`
  paths in its body are genet's. Cambium's own `crates/cambium/docs/`
  travelled inside the exported tree and its links were checked: only
  `local-genet-development.md` was wrong, and badly — it described resolving
  genet from crates.io through a per-crate patch file, a posture two moves out
  of date. It is rewritten around mere's root workspace pin and the two
  standing rules of the patch table. `design_docs/scenograph_docs/` stays where
  it is, describing code that still lives here, with its landing note extended
  to record the second move.

  **Receipts.**

  `cargo check --workspace` **patched** green, 0 errors, **192 warnings across
  twelve crates** — byte for byte the same per-crate set as the Pelt landing's.
  The eleven landed crates and the three scene crates generate **zero**
  warnings on the host target, so the warning delta from this landing is 0.
  What did move is the unused-patch list, 9 lines to 6: `genet-render-host`,
  `boa_engine` and `boa_gc` became live, because `cambium-rootstock` pulls the
  present core and the host chain reaches the script engine. Nothing was fixed
  and nothing was silenced.

  `cargo check --workspace` **unpatched** — run from a directory outside the
  tree so the machine-local patch table does not load — green, 0 errors, **190
  warnings across eleven crates**, the twelve minus `nematic`, exactly as the
  Pelt landing recorded. The witness that the patch table was off is
  `genet-livery v0.0.2 (https://github.com/merely-made/genet.git?rev=a93189b1d7c...)`;
  **zero** lines in the whole log name `C:\Users\mark_\Code\repos\genet`.
  **30 genet-derived packages resolve from `genet.git?rev=a93189b1d7c` and
  nowhere else** — 39 before, minus the nine Cambium packages that are now this
  repository's own. `cambium 0.3.3`, `sprigging`, `meristem`, `workbench` and
  `mere-surface-api` all check from `C:\Users\mark_\Code\repos\mere\...` in
  that same unpatched run, which is the landing's sharpest single fact.

  `cargo test -p cambium-genet-winit-host`: **75 passed, 0 failed** across
  seven binaries — including the two the P2 done-condition names, `decorations`
  (11) and `input_routing` (18), plus `accessibility` (5), `spatial_focus` (5),
  `ui_zoom` (17), `lifecycle` (3) and 16 unit tests. The accessibility and
  input receipts are green from mere.
  `cargo test -p cambium -p cambium-rootstock -p sceno -p scenomise -p scenotime`:
  **339 passed, 0 failed** (182 / 23 / 26 / 78 / 30). Scenomise's 78 include
  the registry's own tests, which travelled with the file.

  `cargo check -p cambium-genet-web-host --target wasm32-unknown-unknown`
  green, 1 warning (an unused import that arrived with the crate).
  `cargo check --target wasm32-unknown-unknown` from `ports/graphshell/web`
  green. `cargo check --manifest-path ports/knot/desktop/Cargo.toml` green,
  resolving `cambium-winit`, `cambium-rootstock`, `cambium-winit-a11y` and
  `cambium-genet-winit-host` from mere and `genet-livery`, `genet-render`,
  `genet-probe` and `buckram` from genet — the boundary in one build.
  `scripts/check_port_boundaries.py` passes.

  **The desktop half of the done-condition**, unchanged output across the
  repository move for the second time: `cargo run -p pelt --no-default-features
  --features livery -j 1 -- --product-receipt article` reports
  `assertion=jump-link press/release moved the retained viewport` and
  `digest=b1d6a62acf85b553`, and its 50,836-byte PNG is **byte-identical** to
  the one genet rendered at `8c1e324ed4d`, its last commit that still had Pelt.
  A retained widget rendering through Genet, from mere, pixel for pixel as
  before.

  **The web half is a real browser, not a type-check.**
  `testing/mere/scripts/run-graphshell-web-scenario.ps1 h3_boot -Build` builds
  the wasm bundle, serves it, and opens headed Chrome; the page drives itself
  through `genet-probe`'s verb loop and POSTs its receipt back. `RESULT ok`,
  two captures (`h3_local.png` 62,515 bytes, `h3_remote.png` 61,021 bytes), no
  page errors, under
  `testing/mere/scenarios/graphshell-web/p2_cambium_h3_boot`. Its receipt
  carries **both** lanes at once: `"layout": "phyllotaxis.default"` over 11
  nodes with a projection editor reading `x=x y=y` into a canvas realization —
  a `sceno` scene — and the chrome around it is a retained Cambium view tree,
  `ports/graphshell/src/web_view.rs` building `cambium::View` / `el` / `text`
  into a `ScriptedDom` that `genet-render` paints. A widget and a scene,
  through Genet, on the web target, live.

  **At least two unlike products consume the Mere source**, and the resolve
  graph says six: `pelt` (the reference browser), `graphshell` (the graph
  portal, native and wasm), `knot-editor` with `knot-document` and
  `knot-desktop` (the document editor), `distillery` (the model works), `djinn`
  and `mere-persona-picker`. `cargo tree -i cambium --workspace --target all`
  names every one of them at a `crates/cambium/...` path.

  `relicense_headers.py --audit` went **1155 -> 1286** owned sources: the 132
  the removal took out of genet, less the facade's `lib.rs`. "Without Exhibit
  A" arrived at **6**, and genet's removal entry expected those six to be
  meristem's. They were not. Meristem's thirty sources were already correct;
  the six were five in `cambium-genet-winit-host` (`src/decorations.rs`,
  `src/harness.rs`, and the `decorations`, `input_routing` and `ui_zoom` test
  files) and one in `cambium-rootstock` (`src/input.rs`). They are Mark's own,
  so they took the plain Exhibit A header rather than the retained-notice form
  the xilem-derived files use (`725bbf1a`). The audit is at **0** without
  Exhibit A and **0** Exhibit B hits, on 1291 owned sources once
  `genet_web_smoke` is counted.

  **P2's "Done when" is met, in all four clauses.** Genet has no Cambium
  workspace member (`a93189b1d7c`, with its own receipts on that commit); a
  retained widget and a `sceno` scene both render through Genet on desktop (the
  Pelt article receipt, byte-identical across the move) and on web (the headed
  Chrome `h3_boot` receipt, which carries both in one page); accessibility and
  input receipts are green from mere (`accessibility` 5, `decorations` 11,
  `input_routing` 18); and six unlike products consume the Mere source, where
  two were asked for.

  Not done here, and not P2's: nothing was pushed. The nine repositories that
  pin the Cambium family from genet.git repoint to mere.git in P4, after the
  receiving head is public and green. The Circuit recipe's
  `workspace_graph.json` fixture is now two generations behind the member list;
  its test reads the committed snapshot rather than live `cargo metadata`, so
  nothing fails, and regenerating it still belongs to the lane that owns it.
  The `C:/Users/mark_/Code/worktrees/genet-head` clean worktree that
  `ports/graphshell/web`'s local patch table points at was moved to
  `a93189b1d7c` for the web receipt; that is a detached checkout of an existing
  worktree and changed nothing in genet.

- 2026-09-03: **the engine-management layer left genet** (genet
  `6d8daca939b`, parent `ee8a3089055`). P3's genet half: nine paths removed
  from genet's workspace, 125 tracked files, 30,320 deleted lines. Ten
  packages in one commit, and three documentation area roots with them.

  | path | packages | genet dependents at removal |
  |---|---|---|
  | `components/inker` | `inker` | `document-canvas`, `nematic`, and the three engine adapters only |
  | `components/inker/document-canvas` | `document-canvas` | none |
  | `components/inker/engines/{scrying,graft,weld}-engine` | `scrying-engine`, `graft-engine`, `weld-engine` | none |
  | `components/verso-tile` | `verso-tile` | **none at all** |
  | `components/nematic` | `nematic` | none |
  | `components/illume` | `illume` | **none at all** |
  | `components/errand` | `errand` | `nematic` only |
  | `components/tinct` | `tinct` | **none at all** |
  | `design_docs/{inker,nematic,verso}_docs` | — | — |

  Every claim was checked with `cargo tree -i <crate> --workspace --prefix
  none --target all` before anything was removed, and then again against every
  manifest in the tree, because `cargo tree` hides an optional dependency
  whose feature is off and §9.2 predicted three of exactly that shape
  (`genet-documents` -> `document-canvas`, `errand`, `nematic`, all optional).
  **Those three edges are already gone**: P1's authority split took them out
  when `genet-documents` was cut back to the Livery and Scripted lanes, and
  the only mention of any of the ten outside the moving directories in any
  manifest is a prose word in `document-session-api`'s `description`. So the
  set was a true leaf and invariant 1 holds at this commit by construction.
  Nothing was wrong in either direction. `genet-render`'s §2 edge into inker
  is likewise already satisfied by P1: it reaches the accessibility
  projection and inspect report types through
  `components/shared/document-session-api`, which stays.

  **History came out the way Cambium's did**, and in the same seconds rather
  than hours: `git fast-export --signed-tags=strip
  --tag-of-filtered-object=drop main -- <path>`, the prefix rewritten in the
  stream by a filter that touches only `M <mode> <dataref> <path>` and
  `D <path>` command lines and copies `data` blocks verbatim by declared
  length, then `git fast-import` into a bare repository. `git subtree split`
  was not attempted; the Pelt entry already measured why it cannot work here.
  A throwaway worktree was again not used — the exports were pinned by
  exporting from `main` while asserting `main == origin/main == ee8a3089055`
  before and after every run, and a ref name was passed rather than a sha, per
  the Cambium entry's note.

  | genet path | landing prefix | bare repo | commits | path lines | elapsed |
  |---|---|---|---|---|---|
  | `components/inker` | `crates/inker/{inker,document-canvas,engines}` | `inker-history.git` | 52 | 356 | 0 s |
  | `components/verso-tile` | `crates/inker/verso-tile` | `verso-tile-history.git` | 7 | 35 | 0 s |
  | `components/nematic` | `crates/nematic/nematic` | `nematic-history.git` | 22 | 127 | 0 s |
  | `components/illume` | `crates/nematic/illume` | `illume-history.git` | 7 | 34 | 0 s |
  | `components/errand` | `crates/system/errand` | `errand-history.git` | 23 | 192 | 0 s |
  | `components/tinct` | `crates/cambium/tinct` | `tinct-history.git` | 11 | 24 | 0 s |
  | `design_docs/inker_docs` | (identical) | `inker-docs-history.git` | 2 | 2 | 0 s |
  | `design_docs/nematic_docs` | (identical) | `nematic-docs-history.git` | 4 | 13 | 0 s |
  | `design_docs/verso_docs` | (identical) | `verso-docs-history.git` | 2 | 4 | 1 s |

  Every commit count matches `git log --oneline -- <path>` in genet exactly,
  and the filter reported **zero** unmatched path lines on all nine runs, so
  nothing was silently left behind. Verification is by tree identity against
  the parent commit, not by inspection:
  `document-canvas` `a45d1ea3d03`, `engines` `b1955c52229`,
  `verso-tile` `b1a62c58fc2`, `nematic` `e07ecc5a5cb`, `illume`
  `94d298f7134`, `errand` `c812b108193`, `tinct` `49430d85e80`,
  `inker_docs` `8b0c6d2c239`, `nematic_docs` `bad24a5107d`,
  `verso_docs` `0c49cfebf78`.

  **The inker export is one export with an ordered rewrite**, because the
  crate's own files and two nested member trees share a prefix:
  `components/inker/document-canvas/` -> `crates/inker/document-canvas/`,
  then `components/inker/engines/` -> `crates/inker/engines/`, then everything
  else under `components/inker/` -> `crates/inker/inker/`. The two nested
  trees are verified by the tree hashes above. `crates/inker/inker` cannot be
  compared that way — genet's `components/inker` tree *contains* the two
  nested trees and the landed one does not — so it is verified at blob level
  instead: genet's 18 `components/inker` entries outside the two subtrees
  match the landed 18 exactly, mode and blob sha, and the bare tip carries 44
  files, genet's count for the whole directory.

  **One wart in the inker history, recorded rather than repaired.**
  `components/inker/knot-editor-host` left with Pelt earlier the same day, so
  it is deleted at the export head but present in 32 historical path lines,
  and the ordered rewrite's catch-all rule lands those under
  `crates/inker/inker/knot-editor-host/`. The tip tree is unaffected — zero
  such paths in it — and mere already holds that crate's own exported history
  at `ports/knot/editor-host`, so this is duplicated ancestry in intermediate
  commits, not a wrong landing. **Open for Mark:** whether to leave it or
  re-export inker with a fourth rule that drops those lines.

  **Genet after the removal.** The cone witness keeps every forbidden name in
  every table — `ORTET_FORBIDDEN`'s seven and `assert_host_api_cone`'s
  `genet-render`/`genet-documents` rows both name crates that have now all
  left — because they are names an edge may never carry again, not a list of
  members. Both subjects of the host-api table still resolve, so no
  adjustment was needed there. What did have to change is the ortet cone's
  **live positive control, which retires with P3**: it was `pelt-desktop`
  until Pelt left, `cambium-genet-winit-host` for the prefix half until the
  Cambium family left, and `document-canvas` -> `inker` for the exact half
  until now. No member remains whose cone reaches any forbidden name, and one
  cannot be invented without reintroducing what the boundary removed. The
  direct predicate assertions stay and are widened to every name in
  `ORTET_FORBIDDEN` rather than the prefixes alone, the `genet-livery`
  negative stays, and the code says in as many words that this proves the
  rule and not the walk. The walk itself is still exercised every run by the
  ortet cone's own must-reach assertion on `genet-documents` and `netrender`.

  **Receipts, all in genet.** `cargo check --workspace` green in 52.9 s, 0
  errors, **22 warnings across seven crates**, against a baseline of 24 across
  eight taken on the parent commit in the same working copy. The delta is
  **-2 and nothing else**: `nematic`'s two warnings left with `nematic`, and
  the per-crate set for the other seven is identical line for line. No warning
  was fixed and none is new. The three `patch ... was not used in the crate
  graph` lines are unchanged at three (`paint_list_api`, `netrender_text`,
  `vello`), so this removal made no patch dead and revived none.
  `cargo check -p ortet` green; `cargo test -p ortet` **10 passed**;
  `cargo run -p ortet -- --url ports/ortet/examples/article.html --frames 3
  --artifact <png>` reports `digest 0x6377ba8a6bf4dbc9` — unchanged from the
  Cambium removal — identical across two runs with byte-identical 87,137-byte
  PNGs, so the engine's output is untouched. `cargo check -p netfetcher
  --no-default-features` green (its `profile package spec num-bigint-dig ...
  did not match any packages` line is pre-existing and was confirmed on the
  parent commit). `cargo check -p genet-documents --features livery` green;
  `cargo check -p genet-wpt` green. `check_dependency_cones.py` passes.

  **The ortet cone went 592 -> 591 packages, and the one that left is `qoi`.**
  Traced rather than assumed: `scrying-engine` depended on `scrying`
  (wgpu-scry), which takes `image` with **default** features; `default` pulls
  `default-formats`, which includes `qoi`. Every other consumer of `image` in
  genet — `arboard` via `genet-clipboard`, and `genet-livery` — names explicit
  format features, so with `scrying-engine` gone nothing enables `image/qoi`
  and it drops out of the whole workspace graph, not just ortet's cone. No
  forbidden name is involved, and `fleece` is still named separately on every
  run as before.

  `relicense_headers.py --repo . --audit` went **708 -> 611 owned sources**,
  exactly the 97 `.rs` files among the 125 removed (the other 28 are 14
  markdown, 10 manifests, and the four `LICENSE-MIT`/`LICENSE-APACHE` files
  that `tinct` and `verso-tile` carry as dual-licensed crates). **"Without
  Exhibit A" is 0 before and 0 after**, and Exhibit B hits 0 both ways — the
  one-file failure the Cambium removal left open in
  `components/genet-livery/src/paint.rs` was closed by its own lane before
  this commit. Ledger paths unmoved at 18.

  **`LICENSES.md` needed no change.** No row in the Retained licenses table
  names a moved path, so the mere side has no row to add on that account.
  The only mentions of the moved crates in the file are in the retained
  2026-08-27 relicensing precedent, which states in its own words that it is
  kept "because it is the precedent the sweep was decided on, not because
  genet still carries the files" — crate names and published versions, not
  paths, so it stays as written. `README.md` needed no change either: it
  names none of the ten. **Note for the mere side:** `tinct` 0.1.2 and
  `verso-tile` 0.1.0 arrive with their own `LICENSE-MIT` and `LICENSE-APACHE`
  files and `MIT OR Apache-2.0` manifests, so mere's manifest license census
  gains two dual-licensed crates.

  Files changed outside the nine removed directories, five in all: genet's
  root `Cargo.toml` (ten member entries with their comment blocks, eight
  `[workspace.dependencies]` entries, and a departure note beside Pelt's and
  Cambium's); `support/ci/check_dependency_cones.py`;
  `design_docs/DOC_README.md` (the three area sections replaced by one
  four-line note, and the required-reading order's "the area root you are
  working in" corrected, since there is no longer one);
  `design_docs/DOC_POLICY.md` (the local addendum's area-root tree drops the
  three roots and gains `archive_docs/`, with the same note); and
  `design_docs/archive_docs/2026-09-02/2026-06-12_knot_evaluation_export_plan.md`,
  whose three relative links into `nematic_docs/` would otherwise dangle and
  are now cross-repo path citations at `mere/design_docs/nematic_docs/...`.
  `Cargo.lock` is gitignored in genet, so there is no stale lock to sweep.

  Prose left alone deliberately: the `nematic.*` engine-id constants in
  `components/shared/document-session-api/src/engine_ids.rs` (an id namespace,
  not a path, and still the vocabulary inker's routing policy is written in);
  doc comments in `genet-documents` and `document-session-api` that name inker
  as the crate whose contract half they are; the archived export plan's table
  row citing `components/inker/src/document/render/export.rs` as a historical
  location; and genet's `docs/`, which this change did not touch. `.github`,
  `scripts` and `support` name none of the ten anywhere.

  **Three things for Mark.**

  1. **The smolweb protocol workspace dependencies are now unconsumed.**
     `nex-protocol`, `spartan-protocol`, `guppy-protocol`, `gopher-protocol`,
     `gemini-protocol`, `scroll-protocol` and `finger-protocol` were errand's
     wire layer and no member names them now. They are **left declared**, with
     a comment saying why: they are outside the named moving set, and deleting
     workspace dependency entries is a resolution decision of its own, the
     same footing on which the Cambium removal left four receipt harnesses in
     place. Cargo does not warn about an unused `[workspace.dependencies]`
     entry, so nothing fails either way. They belong with errand in mere.
  2. **Six of the ten package names are not in `ORTET_FORBIDDEN`.**
     `scrying-engine`, `graft-engine`, `weld-engine`, `illume`, `tinct` and
     `verso-tile` were never on that list, because it came from the ortet
     founding plan's set. They are Mere crates now, so the witness would not
     catch them coming back. Adding six names is a one-line change; it was
     not made here because widening a forbidden table is a policy call, not a
     consequence of this move.
  3. **The inker history's `knot-editor-host` residue**, item recorded above.

  Not done here: nothing landed in mere, and nothing was pushed. The commit
  sits on genet's `main` as the single unpushed commit. The nine bare
  repositories are the artifact for the mere side, under the scratch folder
  as `inker-history.git`, `verso-tile-history.git`, `nematic-history.git`,
  `illume-history.git`, `errand-history.git`, `tinct-history.git`,
  `inker-docs-history.git`, `nematic-docs-history.git` and
  `verso-docs-history.git`; `ee8a3089055` is the revision they were exported
  from and every removed file is intact there.

- 2026-09-03: **the engine-management layer landed in mere** (merge commits
  `75199c1c`, `3e13a02a`, `2e7bfe04`, `4b3fae66`, `390dd649`, `ebb37f55`,
  `5165170f`, `12609e3c`, `1c64dcb7`; wiring `13b64e30`; licenses and docs
  `0712c210`). The receiving half of P3's last motion, against genet
  `115d348dedd`.

  History came in the way Pelt's and Cambium's did: `git fetch <bare>
  main:import-<name>` then `git merge --allow-unrelated-histories`, from nine
  bare repositories the removal exported path-limited with their prefixes
  already rewritten. Each landed tree hashes identically to its bare tip and to
  genet's at the parent of the removal, so these are the same objects rather
  than a copy that looks alike:

  | path | packages | commits | tree |
  |---|---|---|---|
  | `crates/inker/inker` | `inker` | 52 (one export) | `b842e6fb84a` |
  | `crates/inker/document-canvas` | `document-canvas` | (same export) | `a45d1ea3d03` |
  | `crates/inker/engines` | `scrying-engine`, `graft-engine`, `weld-engine` | (same export) | `b1955c52229` |
  | `crates/inker/verso-tile` | `verso-tile` | 7 | `b1a62c58fc2` |
  | `crates/nematic/nematic` | `nematic` | 22 | `e07ecc5a5cb` |
  | `crates/nematic/illume` | `illume` | 7 | `94d298f7134` |
  | `crates/system/errand` | `errand` | 23 | `c812b108193` |
  | `crates/cambium/tinct` | `tinct` | 11 | `49430d85e80` |
  | `design_docs/inker_docs` | — | 2 | `8b0c6d2c239` |
  | `design_docs/nematic_docs` | — | 4 | `bad24a5107d` |
  | `design_docs/verso_docs` | — | 2 | `0c49cfebf78` |

  `git diff --name-only <before> HEAD` after each merge listed **nothing**
  outside that merge's prefix, and each count is exactly the bare repository's
  file count: 44, 9, 26, 10, 23, 6, 1, 4, 2. The eleven landed paths hold
  **125** tracked files, genet's count. The import branches were deleted.

  **The inker history's `knot-editor-host` residue is left as recorded.** The
  crate left with Pelt earlier the same day, so the ordered rewrite's catch-all
  rule put 32 historical path lines under `crates/inker/inker/knot-editor-host/`.
  The tip tree carries **zero** such paths, checked before the merge and after;
  seven intermediate commits do. Mere already holds that crate's own exported
  history at `ports/knot/editor-host`, so this is duplicated ancestry in
  intermediate commits, not a wrong landing, and re-exporting inker with a
  fourth rule to drop those lines remains **open for Mark**.

  **One revision, and nine pins that became paths.** Every genet.git pin in the
  repository moved from `a93189b1d7c` to `115d348dedd`: **47 lines across four
  manifests at one revision**, zero occurrences of the old one, witnessed by
  grep and by the unpatched resolve below. Fifty-six lines went in and 47 came
  out because nine of them became workspace paths (`inker`, `document-canvas`,
  `nematic`, `illume`, `errand`, `verso-tile`, and the three engine adapters);
  `tinct` was the tenth and was a crates.io pin rather than a git one. Ten
  members join, so **126 workspace packages** where there were 116.

  **`tinct` and `tincture` are one package under two keys**, and both had to
  move together: mere pins `tinct = "=0.1.2"` for Tabard and the same package
  as `tincture` for `mere-canvas` and `register-theme`, and the alias exists so
  every `use tincture::` stays unchanged. Both are now the member path. Leaving
  either on crates.io would have put two incompatible `Tinct` types in one
  graph — the duplicate-crate form of the missing-entry trap this file's notes
  already record twice.

  **Errand's wire layer came with it.** The seven smolweb protocol crates
  (`nex-`, `spartan-`, `guppy-`, `gopher-` and `finger-protocol` at `=0.1.1`,
  `gemini-protocol` at `=0.1.7`, `scroll-protocol` at `=0.1.0`, finger with
  `default-features = false, features = ["client"]`) were errand's
  `[workspace.dependencies]` in genet and are mere's now. Genet's removal entry
  left them declared there as a resolution decision of its own; genet then made
  that decision two commits later, in `3af40493b0f`, and dropped them. That
  commit and `115d348dedd` — which widened `ORTET_FORBIDDEN` with the six names
  the removal entry's item 2 flagged — close items 1 and 2 of that entry's
  three, after it was written. Item 3, the inker residue, is the one that
  stands.

  **What actually needed inlining was one field, not three.** The removal's
  hand-off expected `publish`, `rust-version` and `description` to have been
  inherited from genet's workspace and to need inlining. None was: cargo's
  `[workspace.package]` inheritance is opt-in per field, and **zero** of the ten
  manifests wrote `publish.workspace`, `rust-version.workspace` or
  `description.workspace` — checked against genet at `ee8a3089055`, and
  confirmed after the move by `cargo metadata`, which reports the same `publish`
  and a null `rust-version` for all ten as genet did. The three engine adapters
  did inherit `version`, genet's `0.2.0`, and that is inline now because mere's
  workspace version is `0.0.1`. What they also inherited was `repository`,
  genet's `https://github.com/servo/servo`; that now inherits mere's, and the
  seven crates that spelled out `https://github.com/merely-made/genet` with a
  comment explaining that genet's workspace pointed at servo inherit it too.
  Same correction the Cambium landing made for `authors`.

  Two landed manifests reached genet by relative path inside genet's tree —
  `verso-tile`'s `genet-scripted-dom` and `layout-dom-api` (both behind
  `genet-donor`) and `nematic`'s `genet-static-dom` (behind `html-fragment`).
  Those became the root table's git pins, as mere already does for the rest of
  the family, so a future repoint stays a one-file edit.

  **The patch tables lost ten names.** `inker`, `nematic`, `illume`, `errand`,
  `document-canvas`, `verso-tile` and `scrying-engine` left the genet table and
  `tinct` left the crates-io table, in `.cargo/config.toml.example`, the
  gitignored live twin, and `ports/graphshell/web`'s live table — identically,
  checked by diffing the two root tables name for name (they differ only by a
  `radio-hand` entry the live file already carried) and by confirming the web
  table still names every genet package the root names, 23 each.
  `graft-engine` and `weld-engine` were never in any of them. Their genet path
  targets no longer exist either.

  **Receipts.**

  `cargo check --workspace` **patched** green, 0 errors, **192 warnings across
  twelve crates** — byte for byte the same per-crate set as the Pelt and Cambium
  landings. The ten landed crates generate warnings only in `nematic` (2), which
  was already in that twelve because the Cambium landing's patched run compiled
  it from the genet working copy. So the warning delta from this landing is
  **0**. Six `patch ... was not used` lines, the count the Cambium landing left;
  removing eight live patch entries cannot add an unused one.

  `cargo check --workspace` **unpatched** — run from a directory outside the
  tree so the machine-local patch table does not load — green, 0 errors, **192
  warnings across twelve crates**. That is a change from the Cambium landing's
  unpatched 190 across eleven, and the reason is the landing itself: `nematic`
  was the crate whose warnings cargo did not surface when it arrived as a git
  dependency, and it is this repository's own source now, so patched and
  unpatched finally agree. The witness that the patch table was off is that
  **23 genet-derived packages resolve from `genet.git?rev=115d348dedd` and
  nowhere else** — one revision, checked over the whole resolve — with **zero**
  lines naming the sibling genet path, and all ten landed crates plus `cambium`
  resolving from this repository. 30 genet-derived packages before, 23 after:
  seven are this repository's own now. No duplicate package name anywhere in
  the genet or mere family in either resolve.

  `cargo test` on the seven crates that have tests: `inker` **95 passed**,
  `document-canvas` **52**, `errand` **35 passed, 1 ignored** plus a doctest,
  `illume` **32**, `tinct` **14** plus a doctest, `verso-tile` **12**, and
  `nematic` **169 passed as mere consumes it** (`--no-default-features`, which
  is what the workspace pin declares). With nematic's own `html-fragment`
  default on, **167 pass and 1 fails**, and the failure is pre-existing in
  genet rather than a consequence of the move — see below.

  `cargo check -p scrying-engine -p graft-engine -p weld-engine` green, all
  three, on this host. Nothing had to be skipped: `scrying-engine` builds its
  WebView2 producer on Windows through `scrying`, and `graft-engine` and
  `weld-engine` deliberately carry **no** `grafting` or `cef` dependency — each
  defines a host seam instead, precisely so an engine consumer never resolves
  the Servo tree or the CEF distribution — so neither has a platform gate to
  fail. A Linux or macOS host would exercise `scrying`'s WebKitGTK and
  WKWebView paths instead; that is untested here and is P4's target matrix, not
  this step's.

  `cargo test -p cambium --example component_catalog`:
  `catalog_is_the_component_acceptance_surface` passes;
  `committed_receipts_match_the_live_catalog` **fails, for a line-ending reason
  that predates this landing and is not about tinct**. Measured rather than
  guessed: the committed receipt blob is **835 LF and 0 CRLF**; on this machine
  `core.autocrlf = true` checks it out as **835 CRLF and 0 LF**; and the live
  render is a mix, because it emits its own newlines as LF and splices in
  `component_catalog.css`, which the same setting checks out with **824 CRLF**.
  Normalising line endings makes blob and working copy identical. So the
  generator's output can never equal its own checked-out capture here, whatever
  tinct resolves from, and neither the CSS nor the receipts are files this
  landing touched. **Open for the lane that owns the receipts:** an `eol=lf`
  `.gitattributes` rule for the three files, or normalising in the comparison.

  `cargo run -p pelt --no-default-features --features livery -j 1 --
  --product-receipt article`, run unpatched from outside the tree, reports
  `assertion=jump-link press/release moved the retained viewport` and
  `digest=b1d6a62acf85b553` with a **50,836-byte** PNG — the same digest and the
  same byte count as the Pelt landing, the fixture relocation, the Cambium
  landing, and genet's `8c1e324ed4d`. Four repository motions, one
  pixel-identical frame.

  `cargo check --target wasm32-unknown-unknown` from `ports/graphshell/web`
  green in 21.96 s, after moving the shared `worktrees/genet-head` checkout from
  `a93189b1d7c` to `115d348dedd` by detached checkout — the revision every
  genet.git pin here names. That is a checkout of an existing worktree and
  changed nothing in genet. `cargo check -p knot-editor-host` green.
  `scripts/check_port_boundaries.py` passes.

  `relicense_headers.py --repo . --audit` went **1291 -> 1388 owned sources**,
  exactly the 97 `.rs` files among the 125 the removal took out of genet, and
  exactly the rise the removal predicted. **"Without Exhibit A" is 0 before and
  0 after**, and Exhibit B hits 0 both ways, so genet's sweep had already
  covered every header and none was written here. The manifest census gains no
  `MIT OR Apache-2.0` row: all ten landed manifests are already `MPL-2.0`, and
  the five in that column are the pre-existing burn/cubecl patch trees. What is
  true, and is what `LICENSES.md` now records, is that seven of the ten have a
  published version whose grant predates the sweep, and that `tinct` and
  `verso-tile` carry `LICENSE-MIT` and `LICENSE-APACHE` files for it.
  **The sweep plan needs that item**: those grants come up for review at each
  crate's next functional bump, and nothing here decided them.

  **The products consuming the mere source**, from `cargo tree -i` over the ten
  with `--target all`: `pelt` with `pelt-core` and `pelt-desktop` (the reference
  browser), `graphshell` (the graph portal), `knot-editor` with `knot-document`
  and `knot-editor-host` (the document editor), `tabard` (theme authoring),
  `distillery` (the model works), `djinn`, and `mere` itself. Six unlike
  products, the same census the Cambium landing reported.

  **Docs.** The three area roots landed at the same names with their history,
  and are indexed only here: `DOC_README.md` gains a section each, adapted from
  genet's entries at the parent of its removal, and `DOC_POLICY.md`'s local
  addendum lists them again with a note that the round trip is the
  docs-follow-code invariant working in both directions. Seven documents came
  back where eight left; genet archived the knot evaluation/export plan on
  2026-09-02 and it stays in genet's `archive_docs/`, cited by path. Links were
  repaired both ways: inside the roots, source links into genet's `components/`
  now point at `crates/` here, and the two naming documents genet keeps became
  path citations; outside them, **25 citations across sixteen files** still
  named the three roots at `genet/design_docs/...` and would have dangled, so
  they are local again. Prose naming `components/` inside the documents is
  history — including a genet component, `smolweb-views`, that never came here
  — and was left alone.

  **Two findings, both pre-existing and neither repaired here.**

  1. **`nematic`'s `lowers_reader_structure_and_links` is stale by one inker
     change.** It asserts `doc.outgoing_links() == ["/source"]` on a fragment
     that also carries an `<img>`; the live answer includes the image URL. The
     cause is dated: the HTML fragment engine landed **2026-07-27**
     (`4ee2dcb1`), and inker's `collect_block_link_urls` began walking image
     spans on **2026-08-21** (`1f3fc462`, "Add host-driven smolweb inline
     images"), in the same repository, without the assertion following. It is
     not the revision bump — `genet-static-dom` is unchanged between
     `a93189b1d7c` and `115d348dedd` — and it is not the move: the landed tree
     is byte-identical to genet's. It surfaced because `cargo test -p nematic`
     had never run against the html-fragment feature; genet's gates were
     `cargo check --workspace` and ortet's tests. **Open for Mark:** whether
     inker over-collects or the expectation is stale. Mere's own consumption is
     unaffected, and `ports/knot`, which does name `html-fragment`, compiles
     and tests green.
  2. **The catalog receipt's line endings**, recorded above.

  **Still open, and not this step's.** Nothing was pushed; these twelve commits
  sit on mere's `main`. The nine repositories that pin the Cambium family and
  the engine-management layer from genet.git repoint to mere.git in P4, after
  the receiving head is public and green. The Circuit recipe's
  `workspace_graph.json` fixture is now three generations behind the member
  list; its test reads the committed snapshot rather than live `cargo
  metadata`, so nothing fails, and regenerating it still belongs to the lane
  that owns it. Knot's standalone and embedded surfaces still do not share one
  document model; that clause of P3's done-condition is the knot-editor
  extraction plan's work and stays open.

- 2026-09-03: **P3 assessment: Knot's reconciliation.** The facts, all
  measured today. Standalone `knot-editor` (repository, 525k lines with its
  vendored tree, 25 editor sources) and mere's `ports/knot` are byte-identical
  in the editor crate and one file apart in `knot-document`; both last moved
  on 2026-09-01 with the same commit subject. mere also holds `ports/knot/desktop`
  (one file apart from the standalone `apps/desktop`) and, since Pelt's
  landing, `ports/knot/editor-host` (the Cambium host, 420 lines). The
  extraction plan in knot-editor (`design_docs/2026-09-01_knot_editor_repository_extraction_plan.md`)
  rules the shape: Knot owns document, vault, evidence, sync and publishing;
  `knot-desktop` is the standalone process; mere and Turnstone may carry
  integration code only. Its gates are E1, Turnstone and Djinn consuming an
  immutable knot-editor revision, and E2, removing `ports/knot` and
  `ports/knot-document` from mere; its stop rule forbids deleting mere's copies
  before both consumers compile against a pushed revision. **The two PRs that
  carried E1 and E2 are stale.** turnstone #4 is CONFLICTING against its main;
  mere #5 is 337 commits behind, and a `git merge-tree` against today's main
  conflicts in `Cargo.toml`, `ports/knot/Cargo.toml`, `ports/knot/desktop/Cargo.toml`
  and both `knot-document` files, because today's moves rewrote exactly those
  manifests. They cannot be merged as they stand. There is a second reason
  they cannot: knot-editor pins `inker`, `nematic`, `illume`, `knot-editor-host`
  and `genet-scripted-dom` from `genet.git` at `eff0cb6`, and after P3 four
  of those live in mere. So the order is fixed by the stop rule: (1) P4
  repoints knot-editor to one genet revision and one mere revision, with the
  moved crates from mere; (2) E1 is redone as fresh commits, Turnstone and
  Djinn pinning that knot-editor revision; (3) E2 is redone on today's main,
  removing `ports/knot` and `ports/knot-document` and, per the ruled shape,
  `ports/knot/desktop` too (the standalone process belongs to the standalone
  repository, which already carries it as `apps/desktop`); `ports/knot/editor-host`
  is integration code and stays in mere unless Mark rules it Knot's. The two
  open PRs are then closed with a note pointing at the fresh commits, which is
  Mark's action on GitHub. Nothing in this step touches code until (1) lands.

- 2026-09-03: **P4 assessment: repoint and prove consumers.** Census taken
  after P3 landed, over every manifest outside mere and genet that pins
  `genet.git` or `mere.git` (heads at genet `115d348ded`, mere `9e6a74c2e6`):

  | repo | manifests | genet rev pinned | mere rev pinned | crates it still pins from genet that now live in mere |
  |---|---|---|---|---|
  | cleromancy | 2 | `bad78dda19` | `e8d04dd445` | cambium, cambium-genet-winit-host |
  | hocket | 1 | none (patch table) | none | cambium, cambium-genet-winit-host, sprigging, tinct, workbench |
  | isometry | 3 | `86019eaccc` | `1443420b27` | cambium, cambium-genet-winit-host, cambium-winit, sprigging |
  | knot-editor | 5 | `eff0cb6df4` | `1e59c0b7c9` | cambium, cambium-genet-winit-host, illume, inker, knot-editor-host, nematic |
  | mer3ly | 1 | none | `057cb99240` | none |
  | mesocosm | 1 | none | `33f9b6b655` | cambium, sprigging, workbench (by mere-relative wording; verify) |
  | paredros | 1 | none | `33f9b6b655` | none |
  | retinue | 2 | `5ec8274ed2` | `1443420b27` | cambium, cambium-genet-winit-host, sprigging |
  | turnstone | 1 | `eff0cb6df4` | `ca47d6ef63` | cambium, cambium-winit, errand, inker, knot-editor-host, sprigging, weld-engine, workbench |
  | woodshed | 4 | `da8762fd91` | none | cambium, cambium-genet-web-host, cambium-genet-winit-host, cambium-rootstock, sprigging, tinct |

  Every one of these still resolves today, because a git pin names a
  revision and the old revisions still carry the crates; nothing is broken by
  the moves until a consumer advances. What P4 changes is the source of the
  moved crates: `genet.git` at the old rev becomes `mere.git` at a rev after
  `9e6a74c2e6`, and every other genet pin moves to `115d348ded` or later, so
  each consumer resolves one genet and one mere. The order is by dependency:
  knot-editor first, since turnstone and djinn consume it and its own E1/E2
  gates wait on it (see the Knot assessment above); then turnstone; then the
  Cambium consumers that pin no Knot (isometry, retinue, woodshed, hocket,
  cleromancy, mesocosm); mer3ly and paredros need only their mere rev checked.
  Two hazards carried over from §9: turnstone's four `genet_host_api::tile`
  call sites break on any repoint past `abcea38b962` and need the Workbench
  seam, and knot-editor's repository-local cargo checkout of genet must be
  invalidated. Proofs per the plan: each consumer's own gate green, the
  representative headed proofs (Turnstone's browser path, Knot standalone and
  embedded, one Retinue/Cambium surface, the Mesocosm/Paredros scene path),
  and a source-identity audit over the family. This is one bounded change per
  repository, each its own commit and push, in the order above; it starts on
  Mark's word, because every one of these repositories is outside the two the
  plan named and several carry live lanes.

- 2026-09-03: **The Circuit workspace graph is generated, not committed.**
  Mark's ruling closes the staleness clause carried in the notes above. The
  snapshot at `ports/distillery/tests/fixtures/circuit/workspace_graph.json`
  fell a generation behind the member list every time a crate was added — it
  did so twice on 2026-09-03 alone, and by the end sat three generations back
  at `f75db463` while HEAD was `b57d2021`, naming 106 packages against the
  workspace's actual 126. A snapshot of a thing the build already knows is a
  second source of truth that no test can defend, so it is gone.

  The generator's logic was a plain projection of `cargo metadata --no-deps`
  (member names; normal and build edges between members, dev edges dropped so
  the graph stays acyclic; short HEAD as `generated_from`), so it moved into
  the test in Rust rather than staying a script the test shells out to.
  `workspace_graph_fixture_is_a_dag_over_named_packages` in
  `ports/distillery/tests/walk_fixtures.rs` now runs `cargo metadata` itself
  through the `CARGO` the test was launched with, writes the graph under
  `CARGO_TARGET_TMPDIR` (`target/tmp/circuit/workspace_graph.json`), and reads
  back what it wrote. `scripts/workspace_graph_fixture.py` is deleted; the
  fixture is `git rm`ed and `ports/distillery/tests/fixtures/circuit/` is
  gitignored. The `text eol=lf` attribute on `ports/distillery/tests/fixtures/**`
  stays: the two Chronicle boards under it are still authored and committed.

  A derived dataset can fail by being empty as easily as by being stale, and
  the DAG walk is happy to succeed over nothing, so the test now asserts the
  generated graph names `pelt`, `knot-editor-host`, `tabard` and
  `mere-document-lanes` and carries at least one edge. (The ruling said
  `document-lanes`; the workspace member is `mere-document-lanes`, and that is
  what is asserted.) Receipts: `cargo test -p distillery` green twice in a row,
  the generated file byte-identical across the two runs
  (`570f277aa204b6159bc9c8e943090e926a893d22bd498dc50f76beeb3183d5a2`,
  `generated_from` `b57d2021`, 126 packages, 309 edges). Two positive controls
  prove the test is not vacuous: with the cargo binary unreachable it fails
  with "`…metadata…` did not run: program not found" rather than passing, and a
  member name that is not in the workspace fails with "the generated graph does
  not name `…`, so it is stale or empty". Nothing pushed.

- 2026-09-04: **E2 landed — Knot's sources left this repository** (knot-editor
  `design_docs/2026-09-01_knot_editor_repository_extraction_plan.md`). This is
  P3's Knot clause, the one the 2026-09-03 status left open, and it closes it.
  `ports/knot` (the `knot-editor` crate, its docs, examples and tests) and
  `ports/knot-document` are gone; `ports/knot/desktop` went with them because
  the standalone process belongs to the standalone repository, which carries it
  as `apps/desktop` — verified by diff before removal: same two files, the
  knot-editor copy being the one under its CI, with mere's differing only in the
  standalone-workspace scaffolding that dies with it and in three cosmetic test
  and type-annotation details. `ports/knot/editor-host` was **moved**, not
  removed, to `crates/inker/knot-editor-host`: it is inker-family integration
  code rather than Knot product source, and `ports/knot/` should not survive as
  a directory holding only it. 39 owned sources deleted, 5 files moved with
  `git mv`.

  **What replaced them.** `knot-editor` 0.0.3 and `knot-document` 0.0.1 are
  `git = "https://github.com/merely-made/knot-editor.git"` at
  `fcd004b655b595038eba0a7e49f209b8477edadf`, the revision that pins mere
  `d82afa17` and genet `115d348d`. Djinn takes `knot-editor` through the
  workspace table. The `exclude` entry for `ports/knot/desktop` and the nesting
  caveat that named two paths are gone.

  **The patch table this needed, and the one it did not.** Knot pins ~30 Mere
  packages from `mere.git` at `d82afa17` so it builds standalone. Embedded here
  those must not arrive a second time — two sources for one package is a type
  mismatch, not a resolution failure — so the root manifest gains
  `[patch."https://github.com/merely-made/mere.git"]` redirecting every one of
  them to its workspace member. No Knot entry was added to, or needed removing
  from, `.cargo/config.toml`, its committed twin, or graphshell web's pair:
  those tables carry only comments about `knot-editor-host`, whose path they now
  spell correctly. Genet needed nothing: Knot and mere already pin the same
  `115d348d`.

  **Receipts.** `cargo metadata` **unpatched**, run from outside the tree so the
  machine-local patch table does not load: two packages from
  `knot-editor.git?rev=fcd004b6` and no others, no package named `knot-editor`
  or `knot-document` from a path, no workspace member by either name, zero
  packages from `mere.git`, and 23 genet-derived packages at exactly one
  revision. `cargo check --workspace` is green both ways with an identical
  warning set — 192 warnings across the same 12 crates, 0 errors, patched and
  unpatched — so that delta is zero and every warning is pre-existing. Neither
  departed crate is in the list and neither can be now: a git dependency's lints
  are capped. HEAD's own count was not re-measured, so the only warning delta
  claimed here is patched against unpatched. `cargo test -p
  djinn`: 65 lib tests and 5 distillery-lane tests pass, including
  `resident_knot::tests::route_reopens_over_joined_sync_and_live_pairing_updates_all_authority`,
  the live-pairing/joined-sync/route-reopen test the extraction plan names, now
  running against the external package. `cargo check -p knot-editor-host` green
  at the new path. `cargo check --target wasm32-unknown-unknown` from
  `ports/graphshell/web` green. `scripts/check_port_boundaries.py` passes.
  `python scripts/relicense_headers.py --repo . --audit`: 1349 owned of 1703
  tracked, without Exhibit A 0, Exhibit B 0 — down exactly 39 from 1388 of 1742,
  which is every deleted `.rs` file, all 39 of which carried Exhibit A at HEAD.
  `cargo test -p distillery`: 13 pass, and the generated workspace graph is 124
  packages and 280 edges against 126 and 309, the two departed members and the
  29 edges into them. `walk_fixtures.rs` gained the other half of its member
  assertion: the graph must still name `knot-editor-host` and must **not** name
  `knot-editor` or `knot-document`, so a re-added member fails rather than
  passing quietly.

  **Wording.** `README.md` and `design_docs/DOC_README.md` now say Knot Editor is
  an independent repository consumed by Djinn and Turnstone rather than a port
  under `ports/`, citing the extraction plan by path. `ports/djinn/README.md`,
  the Turnstone suite census's Knot section, and the configuration-ownership
  plan's live code list name the new source. Seven Knot plans and briefs kept
  their text and gained a repository note saying the `ports/knot` paths in them
  are the layout their receipts landed against; the lane brief's opening
  sentence, which asserted a live ruling rather than a receipt, was corrected.
  `scripts/cross-repo-smoke.ps1` lost its two `-p knot` steps, which had been
  dead since before this change — the package was `knot-editor`, never `knot`.

  Nothing pushed. Turnstone is green against `fcd004b6` at its own pushed
  `e68e2764e4d`, which is what the extraction plan's stop rule required before
  any of this could be deleted; Djinn is Mere's own consumer and cut over here.

- 2026-09-04: **P4 landed for every consumer this lane could reach.** Family
  revisions: genet `115d348ded`, mere `d82afa17e2`, knot-editor
  `fcd004b655`. Per repository, all pushed: knot-editor `3c2d30d` then
  `fcd004b` (six crates now from mere, `mere-surface-api` for the surface
  types, 112 tests matching its E0 receipt); turnstone `1643b12` then
  `e68e276` (eight crates from mere, both Knot crates from knot-editor.git,
  `genet-documents` split with the http(s) fallback injected through
  `LocalFetcher::with_fallback(RemoteFetcher::shared())`, the four tile call
  sites already on Workbench; 414 pass with one pre-existing failure proven
  on the parent; the two Knot suites 5/5 and 5/5; browser_keep, rung4_content
  and gemini-browse headed receipts ok) which is E1; retinue `eaa9bb9`
  (one manifest, `taffy` renamed to `genet-taffy` and `parley` restated, the
  Cambium surface smoke 12 tests green, firmware check green); hocket
  `076bfa2` (every family pin was `branch = "main"`, now revisions; the
  handoff_circle headed receipt ok; the machine-local table rewritten from a
  vanished Codex worktree); cleromancy `7fddf07` by this lane and then
  `8a91f77` by Mark's other session, which put its fifteen mere pins on
  `branch = "main"` because Cargo keys a git source by URL plus reference
  and isometry, its consumer, tracks main; mesocosm `310513c` (pins, swept
  in by the same session) and `3c39dc4` (the `resident_ground` example
  un-duplicated: the `conatus/resident` fault was a parse error left by two
  header passes, not a missing feature); paredros `b5f2d41` (one `modulus`
  line; the headed S0 room probe renders 64 ticks to a stable hash);
  mer3ly `d52d7f9` (fourteen pins, locks re-resolved from outside the tree,
  `authority validate` clean). mere itself took E2 as `d666e160`: the Knot
  copies removed, djinn consuming knot-editor.git, `knot-editor-host` at
  `crates/inker/knot-editor-host`, and a `[patch."mere.git"]` table so
  Knot's pin of mere resolves to the workspace; turnstone #4 and mere #5
  closed with pointers to the fresh commits.

  **Source-identity audit** (`cargo metadata` per workspace, machine-local
  tables off): knot-editor (both workspaces), turnstone, retinue's app,
  hocket, cleromancy (root and `core/`), isometry, mesocosm, paredros,
  mer3ly's repo-graph and mere each resolve one reference per host with no
  package from two sources. **Two exceptions, both deliberate and outside
  this lane:** isometry (`ce6e69f`, unpushed at audit time) and cleromancy
  track mere by `branch = "main"` rather than an immutable revision, on the
  reasoning in `8a91f77`; the done-condition's "immutable revisions" is met
  by every other consumer and waived for that pair by Mark's session.
  **Not done:** woodshed, whose session was mid-repoint all evening with
  genet pinned at `da8762fd`, a pre-P1 revision that still contains Cambium;
  its next advance meets P1 through P3 at once, and `cargo metadata` there
  fails today on a half-edited `workbench` entry.

  **Open, recorded for their owners:** Knot pins a pre-E2 mere and should
  follow the next family bump; mer3ly's relation manifest does not know the
  knot-editor repository exists and its Genet showcase still attributes Pelt
  to genet (P6, product copy, Mark's); two `paredros-sortie` tests are red on
  main independently of the pins; paredros carries a dead `vello` patch;
  cleromancy takes `ipc-channel` and `gpu-allocator` from crates.io where
  genet vendors both; hocket has two stale worktree registrations.
  **Done-conditions:** supported consumers resolve a single genet and mere
  source, met for all but woodshed; representative headed proofs pass
  (turnstone's browser path, Knot standalone and embedded through djinn and
  turnstone, retinue's and hocket's Cambium surfaces, paredros' scene); the
  source-identity audit reports no duplicate origins. P4 is met with the
  woodshed exception and the branch-tracking pair noted.

- 2026-09-04: **P5 assessment: extractions and consolidations against the §4
  bar.** Facts measured today; each item ends in a recommendation for Mark,
  since P5 is rulings, not moves.

  **Conatus** (`conatus` 4.5k lines, `seiche` 10.3k, `numen` 3.0k,
  `modulus` 0.8k, `nisus` 0.6k; all five published). External consumers
  after P4, all on one mere identity: isometry (conatus), mesocosm (conatus,
  modulus, nisus), paredros (modulus), retinue and mer3ly (seiche). The
  bar's first clause fails: `seiche` depends on `armillary`, mere's
  actor-kernel runtime (a three-line edge), so the family does not build
  without Mere. The
  third clause has no pressure behind it: every consumer is a Merely
  repository pinning `mere.git` by revision, and a separate repository
  would add a host to the pin lattice for no independent release or
  governance. **Recommendation: stays in mere without ceremony.** Trigger
  to reopen: seiche's armillary edge cut, plus a consumer outside the
  family.

  **Eidetic core** (`chartulary` 6.0k, `eidetic-core` as `mere-eidetic`
  6.9k, `muniment` 2.7k, `eidetic-search` 1.0k, three adapters under 300
  each, `hagiograph` 26). The boundary is already proven by consumers:
  chartulary and muniment are pinned from seven repositories (cleromancy,
  hocket, isometry, knot-editor, retinue, turnstone, woodshed). The bar's
  first clause is not met: `mere-eidetic` reaches mere's `identity`, and
  `eidetic-search` reaches `esp` and `import`, and both sit inside the
  family directory beside the portable crates, so the Mere-specific half is
  not "visibly outside the core". **Recommendation: no repository; one
  mere-internal reshuffle** that moves `eidetic-search` (a tantivy index
  over browsing traces, a Mere product concern) out of `crates/eidetic/`
  and records `mere-eidetic`'s identity edge as the line the core stops
  at. After that the family's portable half (chartulary, muniment,
  hagiograph, the fjall/https/iroh adapters) is visibly separable, and the
  extraction question waits for release pressure that does not exist today.

  **Knot duplicate:** resolved by E1 and E2 on 2026-09-03/04; nothing open.

  **Sonance placeholder:** `merely-made/sonance` is already an archive
  pointer (seven files; its README names `mora::sonance` as the canonical
  implementation, commit "retire sonance repository"), but the GitHub
  repository is not archived. **Recommendation: archive it on GitHub** (a
  P6 mutation, Mark's), leaving the pointer README as the durable redirect
  §4 asks for.

  **Anise mirror:** `merely-made/anise` is not a fork in GitHub's sense and
  carries no change of ours: its head `71e973a` is exactly upstream's `0.10.6` release tag in
  `nyx-space/anise` (upstream has since moved to `61b4291`). Its only
  consumer is turquet's optional `verify` feature, which pins the mirror by
  revision and `version = "=0.10.6"`; crates.io carries `anise 0.10.6`.
  **Recommendation: retire the mirror.** turquet takes `anise = "=0.10.6"`
  from crates.io (its manifest already names that version), which removes
  the only reason the mirror exists; the mirror is then archived with a
  README saying it was an unmodified pin of upstream at `71e973a`. If Mark
  wants the supply-chain hedge a mirror gives, the alternative is to keep
  it with exactly that README and nothing else.

  **Shared wgpu release automation:** the three repositories share no
  workflow today. wgpu-scry and wgpu-weld both carry `hardware-headed`,
  `msrv` and `wgpu-matrix` workflows that differ by 258, 137 and 74 lines;
  wgpu-graft carries a different set (`sync-main-release`,
  `registry-only-triplet`, `sync-servo-line`, `hardware-interop`). The
  organization's `.github` repository holds no workflow templates or
  reusable workflows. **Recommendation:** found reusable `workflow_call`
  workflows in `merely-made/.github` for the wgpu matrix and MSRV checks,
  parameterized by crate and feature set, and have the two near-identical
  repositories call them first; graft joins when its release sync fits the
  same shape. This is outward work in a public organization repository and
  starts on Mark's word; §4's merge question stays closed until coordinated
  upgrades dominate.

  **netfetcher, carried from the 2026-09-03 ruling:** identity yes, a
  portable WHATWG Fetch engine; consumers turnstone, mere's document lanes,
  genet-documents and ortet, all Merely; no independent release pressure.
  **Recommendation: stays a genet crate**; the Transport seam is what a
  radio consumer needs and it needs no repository.

  **Mer3ly:** the relation manifest records repositories, not crates, and
  three facts are now stale or missing: no entry for `knot-editor`; the
  Genet showcase attributes Pelt to genet; sonance and anise dispositions
  once ruled. Updating it publishes to the company site and is P6, Mark's.

  **Done-condition:** every separate repository has a stated independent
  surface (genet, mere, knot-editor, netrender, the wgpu trio, and the
  product, radio, protocol and numerical repositories keep theirs; sonance
  and anise are the two folded placeholders, with a durable redirect in
  place for sonance and one proposed for anise); the two candidate
  extractions fail the bar and remain mere crates. P5 is met on the
  rulings above once Mark takes them.

- 2026-09-04: **P5 landed on Mark's rulings** ("1-6: sure", with notes).
  Conatus stays in mere; Mark's note recorded: seiche could be split out of
  the family and stay in mere, and the family could leave mere, and neither
  is worth doing now. Eidetic: the reshuffle landed (`cfe7d2d8`,
  `mere-eidetic-search` to `crates/intel/eidetic-search` beside `esp`, its
  only in-workspace consumer; the eidetic README now states the portable
  core and its one Mere edge, `mere-eidetic`'s identity types behind
  `pack-signing`), and the review Mark asked for is
  `design_docs/eidetic_docs/research/2026-09-04_eidetic_review_brief.md`
  (`5cdb3ff1`): tantivy costs 117 packages against 19 without it and keeps
  the crate off the wasm lane; turnstone re-mints the whole index on every
  query and never opens one from disk, so most of the crate's surface has
  no consumer and page text never enters the index; recommendation, an
  in-tree BM25 behind the unchanged `Hit`/`fuse` seam, with the criteria
  that would keep tantivy stated; one visited page is materialized five
  times with dedup at read time; `chartulary::rdf` duplicates
  `mere-linked-data`; five unsurfaced features and eight questions, the
  first being that the corpus size the judgement turns on is unmeasured.
  Sonance archived on GitHub (its code is `mora::sonance`; deletion of the
  repository and the local checkout are Mark's). Anise mirror archived with
  a description saying it was an unmodified pin of upstream's 0.10.6 tag;
  turquet takes anise from crates.io (`ce1a9ab`, verify feature checks).
  Mer3ly (`4ee5e1e`): knot-editor entered as a repository with four
  relations and a migration record, nine evidence lines corrected for
  Cambium's home, showcases corrected (the Pelt captures stay on Genet's
  card as Genet-era, since the schema ties an image to the repository that
  holds it and mere has no Pelt capture; ortet named in prose), sonance and
  anise rendered archived through the refreshed metadata; `authority
  validate` 19 repositories, 29 relations, 29 migration records. Shared
  wgpu automation: measured first over the last 40 runs per repository, and
  the premise was off: the hosted lanes were already green (scry and weld
  matrix 10/10), and the pointless red was the hardware lanes cancelling
  themselves on the next push (scry's headed lane: 2 useful badges in 9
  runs) and graft's fan-in job, which ran on Ubuntu under a Windows name.
  Landed: `merely-made/.github` gained `wgpu-check.yml` and
  `wgpu-msrv.yml` as `workflow_call` workflows (`c994b3f`), toolchain read
  from the caller's `rust-toolchain.toml`; weld (`b55d8cc`) and scry
  (`392ff05`, which gained a `rust-toolchain.toml` at 1.97.1) call them,
  with MSRV on tags and dispatch only and hardware lanes weekly plus
  dispatch, non-gating; graft (`d391a4c`) moves MSRV to the shared gate,
  names the failing lane in its fan-in, and unhooks its hardware lane; its
  release sync workflows are untouched. Every check that existed is still
  reachable. All pushed, org repository first. Machine-local patch tables
  in mer3ly and woodshed repointed to the moved paths (gitignored).
  **For Mark:** branch-protection required-check names change (shared jobs
  appear as `gate / <os> / <wgpu>`), the hardware jobs must leave the
  required set, and graft's fan-in name is his to rename; scry's clippy and
  fmt are off to keep today's green and are a two-line flip after a
  cleanup; two live bugs the table surfaced are out of scope here, weld's
  `fd_is_closed` assertion in `vulkan_dmabuf.rs:475` and graft's `E0507`
  against bevy 0.19 in `demo-servo-bevy`. **Done-condition met:** every
  separate repository has a stated surface, both folded placeholders are
  archived with a durable pointer, and both candidate extractions remain
  mere crates.

- 2026-09-04: **P4's Woodshed exception closed.** Woodshed now resolves the
  moved Cambium family, Workbench, Tinct, surface API, scenes, identity, and
  storage crates from mere `d82afa17e2cca86da843f07a2d718d2e69eb9f10`, and
  the remaining raw engine, layout, scripted-DOM, probe, Taffy, Parley, and IPC
  crates from genet `115d348deddc344d949754e63beaece47cf49f34`. A
  config-free locked metadata audit reports one origin for every package in
  both families. The repoint also follows the landed Scholia fold by using
  `chartulary::rdf` and moves settings projections from `genet-host-api` to
  `mere-surface-api`.

  Receipts: `cargo test -p woodshed-core --locked -j 1` passes 77 unit tests
  and one integration test; `cargo check -p woodshed-genet --locked -j 1` and
  an isolated build pass; and the headed `p4e_stage_layouts.scn` receipt passes
  all three Snake, Ten, and Circle captures. The host reports accessibility
  installed, projects 286 nodes, and reaches `RESULT ok`. This closes the last
  consumer exception recorded above. The selective change was integrated over
  the newer graph lineage in an isolated worktree and pushed normally as
  Woodshed `c5eaa7d6`; the original dirty checkout and its concurrent docs,
  workflow, README, and example work were left untouched.

- 2026-09-05: **P6 public-topology receipt.** `mark-ik/p2panda` remains a
  personal, diverged upstream line: at refresh it is 8 commits ahead and 199
  behind `p2panda/p2panda:main`; Mere-family consumers continue to pin the
  immutable `mere-p2panda-net-0.7.2` tag, so no branch-following dependency
  was introduced. Graphshell is still archived, but now points at
  `merely-made/mere`; GitHub required a recoverable unarchive/edit/rearchive
  sequence to make that metadata change. Ringdown and Cleromancy now have
  plain README-derived descriptions. The live Sonance and Anise endpoints are
  deleted (404), not archived, so Mer3ly's refreshed public manifest removes
  their stale archive cards. Emblem's separately owned transfer is now live at
  `merely-made/emblem`, with the old slug redirecting. Main protection now
  requires only the exact shared software contexts on wgpu-scry, wgpu-weld,
  and wgpu-graft; headed hardware lanes are deliberately non-required.
  Mer3ly's P6 receipt records the exact contexts and mutation evidence at
  `docs/receipts/org-transfer/2026-09-05_p6_platform_topology.md`.

  Emblem's own metadata correction is pushed as `8c9aebb8`, with 151 focused
  tests green. Woodshed's public relation evidence followed in Mer3ly after its
  P4 push. Mer3ly's hosted Pages run `33948128095` passed the Rust behavior,
  immutable Mere graph, exact artifact, headed browser smoke, receipt upload,
  and deploy gates; the live custom domain served the new artifact after that
  deployment. **P6's topology done-condition is met.** Separate site hardening
  remains outside this repository move: Cloudflare still proxies the custom
  domain, GitHub therefore reports the organization domain unverified and does
  not yet permit HTTPS enforcement.

- 2026-09-06: **Pelt scripted host continuation, first native slice validated.**
  Pelt now creates scripted sessions after the shared GPU device is available,
  supplying a fresh Fetch handler and WebGL factory before authored scripts
  execute. `mere-document-lanes` owns the asynchronous transport adapter and
  shares the existing resource cache, in-memory cookies and concurrency budget.
  Genet retains engine-neutral runtime contracts, document retirement and
  texture placement metadata; Pelt resolves and presents those textures on the
  shared device.

  The canonical scope, limits and frozen native receipt are recorded once at
  `genet/design_docs/2026-09-06_scripted_host_capabilities_plan.md`. The integrated
  adapter's 11 tests and Pelt's five scripted tests pass, including local HTTP
  Fetch, GPU pixel composition and texture retirement. The bounded headed P5
  scripted receipt also passes. The published Genet pin is
  `d424a689518a53e38d4c6f89c5c792bebb439248`; all 28 Genet selectors agree.
  Candidate metadata with the recorded source map preserves the frozen Mere
  lock and confirms one wgpu 30.0.1 identity across Pelt, WebGL and the render
  host. This is a source-mapped candidate receipt; a config-free build from the
  final pins has not been claimed. Strict lint closure and broader WebGL/WPT
  conformance remain open as stated in the canonical receipt. The work is on
  `codex/scripted-host-capabilities-20260906`, preserving concurrent main work.
