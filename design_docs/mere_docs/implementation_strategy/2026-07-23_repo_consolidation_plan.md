# Repo Consolidation Plan

**Date:** 2026-07-23
**Status:** ruled with Mark 2026-07-23; execution started same day. Later
rulings: the bucket repo is named **smolweb**, and the errand spec crates
(spartan/nex/guppy protocols) move into it, with gemini, titan, gopher,
finger, plain, a shared TOFU/client-cert helper, and gemtext listed as
trigger-gated later extractions from errand. **Personae folds** with a
dependency-minimal wall and an explicit re-extraction trigger: the first
external verifier, interop partner, or security-audit engagement pulls it
back into a dedicated repo. Execution order is adjusted from the phase
numbering: C4 runs first among code phases (active radio hardware work wants
the merged workspace), C2 runs last (a live genet livery session holds
uncommitted edits), consumer repoints happen only after each receiving repo
is pushed green, and old repos are archived only at the very end, so no
intermediate state leaves a consumer unresolvable. This plan supersedes the
repository-boundary posture of the
[Graphshell remote projection host plan](2026-07-22_graphshell_remote_projection_host_plan.md)
sections 2, 3, and 9 where they conflict; that plan's protocol design, proof
sequence, and receipts remain in force.

**Companions:** the
[projection-engine prior-art brief](../research/2026-07-21_projection_engine_prior_art_brief.md),
the [projection proofs plan](2026-07-21_projection_proofs_plan.md), the
[Murm peer-runtime and Moot-domain plan](2026-07-12_murm_peer_runtime_and_moot_domain_plan.md)
(promotion rows withdrawn by this plan), and the
[mere/merecat boundary pass plan](2026-07-09_mere_merecat_boundary_pass_plan.md).

## 1. Ruling

Mere is the platform; the family repos that were extracted from it remain its
components wherever they are packaged. The 2026-07-22 Graphshell plan framed
the session stack as a product-neutral commons with Mere as one endpoint among
peers. That framing is withdrawn. Graphshell is Mere's shell and remote port,
the session protocol is Mere's session grammar, and an application that
implements the endpoint is participating in Mere. The genet/pelt shape is the
governing precedent: the platform repo hosts its own contracts, runtime, and
reference shell, and products consume them from it.

The bar for a separate repository: **real coherent utility and identity apart
from mere, genet, merecat, woodshed, hocket, and isometry.** A repo below that
bar folds into mere or genet, or is consumed through them. The dependency
graph should mostly pull mere or genet.

One structural invariant makes the split between the two buckets: nothing
genet consumes may move into mere, or genet acquires a cycle. Genet's family
pulls are netrender, netfetcher, misfin, and wgpu-scry; all stay genet-side or
independent.

## 2. Target shape

```text
products     merecat   isometry   woodshed   hocket
                \         |          |        /   (hocket also -> wavicle, woodshed)
platform     mere ................................ genet
             graph domain, murm/moot,             engine, pelt, cambium,
             graphshell, eidetic, scenograph,     sprigging, meristem,
             conatus, personae, servitor,         netfetcher, tinct
             armillary, vates, sibylla
                \                                  /
components   netrender   wavicle   radio   wgpu interop   smolweb (misfin)
```

Products pull mere and genet; the two platforms pull the components that pass
the bar. Crates.io package names survive every move; only repository homes,
`repository` fields, and git URLs change.

## 3. What moves

### Into mere (one folder per incoming family under `crates/`)

| Incoming repo | Crates | Consumers to repoint |
|---|---|---|
| personae | `personae` | woodshed, hocket, isometry, mere's `identity` alias |
| armillary | `armillary` | mere, merecat, isometry, hocket, vates |
| eidetic | `muniment`, `codicil`, `chartulary`, `scholia` | mere (graph-kernel, session-runtime, murm, moot, mesh, intel, eidetic lane), merecat, isometry, woodshed, hocket, servitor |
| servitor | `servitor` | mere, merecat |
| vates | `vates` | mere |
| sibylla | `sibylla` | mere |
| conatus | `numen`, `quint`, `seiche` | mere (currently crates.io pins; become workspace paths) |
| scenograph | `sceno`, `scenomise`, `scenotime`, `scenograph` | mere, merecat, isometry, graphshell |
| graphshell | `graphshell-protocol`, `graphshell-client`, `graphshell-endpoint`, `graphshell-stdio`, `graphshell` | merecat, isometry |

Notes:

- The incoming eidetic four join `crates/eidetic/` beside the existing
  adapter lane (`eidetic-core`, `eidetic-fjall`, fetchers, search). This
  settles the mere-eidetic naming: primitives and adapters live in one folder,
  and the orphaned bare `eidetic` crates.io name is documented where it is
  reserved.
- `crates/persona/identity` (the dead duplicate) is deleted in the same change
  that lands personae; the workspace `identity` alias becomes a workspace path.
- The graphshell five keep their crate names and land under
  `crates/graphshell/`; the facade is Mere's reference client, pelt-shaped.
- Scenograph is below the bar today because every consumer is mere-side. If
  outside consumers materialize later, extraction is available then; the P3/P4
  receipts attach to crate contents and survive the move.

### Into genet

| Incoming repo | Crates | Consumers to repoint |
|---|---|---|
| cambium | `cambium`, `cambium-nematic`, `cambium-winit`, `meristem`, `sprigging` | merecat; genet's `genet_web_smoke` example path dep becomes a workspace reference |
| netfetcher | `netfetcher` | genet (genet-documents, genet-wpt become workspace refs), mere (`crates/system/fetch`) |

tinct already lives in genet (`components/tinct`); woodshed and hocket repoint
from the `mark-ik/tinct` mirror to genet.git and the mirror repo retires with
a tombstone.

### New smolweb bucket

New repo `smolweb`. `misfin` moves in (MIT, held in stewardship; the transfer
offer to the protocol author stays visible in the repo README). Candidates to
join: `spartan-protocol`, `nex-protocol`, `guppy-protocol`, currently inside
genet's errand component; **decision gate: Mark rules whether they move now or
stay in genet.** Errand itself stays in genet as the client integration and
keeps consuming `misfin` from crates.io, unchanged.

### Radio family

Execute the already-ruled merge: `tulle`, `sennet`, `tucket` fold into the
`retinue` workspace, preserving crate names, histories, provenance files, and
the license split (crates MIT/Apache, firmware images GPLv3, kept in separate
folders exactly as today). Mere's `path = "../retinue"` becomes a git pin to
the merged workspace.

### Stays separate, passing the bar

| Repo | Why it passes |
|---|---|
| netrender | Public render engine, servo-upstream lineage, upstream of both platforms. Fold target if ever ruled otherwise is genet, never mere. |
| wavicle | First pure-Rust WavPack codec; zero family coupling. |
| radio workspace | Own hardware, firmware, FCC, and business surface. |
| wgpu-graft, wgpu-scry, wgpu-weld | Standalone public interop libs, one-way deps, consumed via pins. Optional later: one wgpu workspace. |
| smolweb | Public protocol family with stewardship obligations. |
| merely-made.github | Brand site; no Rust edges. |

### Withdrawn

The murm/moot promotion to `repos/murm` and `repos/moot` is withdrawn. They
stay in mere; isometry keeps pulling them from mere.git. The audio-primitives
standalone promotion is deferred: hocket repoints to woodshed.git, and a
standalone repo waits for a third consumer.

## 4. Invariants and walls

1. **No cycle:** netrender, netfetcher, misfin, wgpu-* never enter mere.
2. **The walls travel.** The graphshell crates keep their G0-G4 discipline
   inside mere as CI checks scoped to those crates plus the scenograph four:
   no kernel or product dependency from the portable crates, wasm32 check for
   `graphshell-protocol`/`-client`, warning-denying clippy. Pelt-core is the
   precedent that contracts stay honest inside a platform repo only when the
   checks are mechanical.
3. **History preserved.** Every absorption is a subtree/merge that keeps the
   incoming repo's history, the technique already used for the eidetic and
   conatus family merges and graphshell PR #308. Absorbed GitHub repos are
   archived with a tombstone README naming the new home. crates.io publishes
   continue from the new home; `repository` fields update at next publish.
4. **Licenses unchanged per crate.** Incoming mere-side crates are MIT/Apache
   already; MPL stays genet-side (Servo-derived only), per the founding
   convention.
5. **One absorption per change.** Each move lands as one commit pair: subtree
   plus workspace membership in the receiving repo, consumer repoints in the
   same session. Targeted `cargo check -p` per touched consumer, not broad
   workspace checks, while sibling work is in flight. No concurrent test runs.

## 5. Phases

### C0. Record the ruling in the docs

- Amend the Graphshell plan: status note pointing here; section 2's
  "must not depend on Mere" becomes a crate-linkage rule (portable crates do
  not link the kernel) rather than a repo rule; section 3's repo layout
  superseded; section 9 rows updated (murm/moot withdrawn, audio-primitives
  deferred, boundary map reread against the bar).
- Amend the prior-art brief with one dated note: the projection
  compiler/runtime destination is realized as the scenograph family inside
  mere; Mere names the platform, not one product among peers.
- Amend DOC_POLICY's inheritance paragraph: new Graphshell documentation
  belongs in mere's design_docs again once the crates land.
- DOC_README updated in the same session as each amendment.

**Done when:** no active doc contradicts the target shape in section 2.

### C1. Pin hygiene with non-moving targets

- woodshed and hocket: tinct repointed to genet.git.
- hocket: audio-primitives to woodshed.git, wavicle to wavicle.git or
  crates.io.

**Done when:** woodshed and hocket build from clean clones with those pins,
and no committed sibling path targets a repo that is staying put.

### C2. Genet absorption

- Subtree cambium (five crates) and netfetcher into genet; workspace
  membership; `genet_web_smoke`'s cambium path dep becomes a workspace
  reference, removing the example-only cycle.
- Repoint merecat (cambium, sprigging) and mere (`crates/system/fetch`,
  netfetcher) to genet.git.
- Archive the cambium and netfetcher GitHub repos with tombstones.

**Done when:** a fresh genet clone builds the absorbed crates, merecat and
mere check green on the new pins, and the old repos are read-only.

### C3. Mere absorption, bottom-up

Ordered so mere is green after every sub-step. Coordinate with in-flight
sibling sessions before C3b; the eidetic step touches every product.

- **C3a personae + armillary.** Delete `crates/persona/identity`; the
  `identity` alias becomes a workspace path. Repoint woodshed, hocket,
  isometry.
- **C3b eidetic four** into `crates/eidetic/`. Repoint merecat, isometry,
  woodshed, hocket, servitor (interim, until C3c), and mere's own git pins
  become paths.
- **C3c servitor, vates, sibylla, conatus.** All their family deps are now
  workspace-internal. Mere's crates.io pins for numen/quint/seiche become
  workspace paths.
- **C3d scenograph four** into `crates/scenograph/`. Repoint merecat and
  isometry.
- **C3e graphshell five** into `crates/graphshell/`, with the section 4 CI
  walls landing in the same change. Repoint merecat and isometry. Re-archive
  mark-ik/graphshell with a tombstone; its donor lineage and the 2026-07-22
  portable workspace remain in its read-only history, and the same history
  arrives in mere via the subtree.

**Done when:** all nine incoming families build as mere workspace members
from a fresh clone, every consumer is repointed and green, the G1-G4
receipt tests pass from their new home, and the absorbed repos are
archived.

### C4. Radio merge

Per the standing ruling: tulle, sennet, tucket into the retinue workspace;
provenance and license split preserved; mere's transport pin becomes git.

**Done when:** one radio workspace builds all crates and firmware targets,
and mere clean-clones without the path dep.

### C5. Smolweb bucket

Found `repos/smolweb`; subtree misfin in; stewardship note in the README;
resolve the errand-protocols decision gate with Mark before or during this
phase.

**Done when:** misfin builds and publishes from smolweb, genet errand is
unchanged, and the old misfin repo is tombstoned.

### C6. Verify and disseminate

- Tree-wide gate: no committed cross-repo `path =` dependency remains.
- Clean-clone builds: mere, genet, merecat, woodshed, hocket, isometry.
- Regenerate the repo dependency graph and compare against section 2.
- Update the memory index and any plan that names an old home.
- Archive this plan per DOC_POLICY on completion.

**Done when:** the regenerated graph matches the target shape and the six
primary repos build from clean clones.

## Findings

- Manifest sweep of all 28 repos, 2026-07-23: mere is the apex consumer with
  13 family deps and in-degree 2 (merecat deep, isometry murm/moot lane);
  every product consumes genet, netrender, and eidetic; mere and graphshell
  have identical dependents (merecat, isometry).
- Committed sibling paths found in hocket (woodshed, eidetic, armillary,
  personae, wavicle), woodshed (chartulary, scholia, personae), and mere
  (retinue, personae via `identity`), beyond the known radio-family paths.
  All are eliminated by C1 through C4.
- conatus reaches mere as crates.io pins today (`numen 0.1`, `quint 0.0.2`,
  `seiche 0.0.3`); it has no consumer outside mere.
- The `mark-ik/tinct` mirror duplicates `genet/components/tinct`.
- `crates/persona/identity` is a dead duplicate; the live alias already
  targets personae by path.
- genet's `genet_web_smoke` example reaches cambium by sibling path,
  a dev-only cycle that C2 removes.
- The 2026-07-22 Graphshell plan's product-dependency walls are receipts
  about crate contents, not repo homes; they relocate intact as CI checks.

## Progress

### 2026-07-23

- Ruled with Mark: the separate-repo bar, the mere and genet buckets, the
  smolweb bucket for misfin, tinct through genet, and the withdrawal of the
  neutral-commons framing. Plan written.
- **C0 landed** (mere `73d8b1b1`): supersession notes on the Graphshell plan,
  the prior-art brief, and DOC_POLICY, plus the DOC_README entry.
- **C4 landed** (retinue `e071d08`). tulle, sennet, and tucket are subtree
  members of the retinue workspace with history preserved:
  `crates/{retinue,tulle,phy-profile,sennet,tucket}`, `firmware/` as
  non-default members, `vendor/lora-phy` keeping its own MIT/Apache licensing.
  Sibling `../tulle` path deps resolve unchanged under `crates/`. Gates:
  fmt, clippy `-D warnings`, 251 tests, retinue and sennet/tucket sans-io
  builds, and `tulle-t114-phy` for `thumbv7em-none-eabihf`. mere repointed to
  a branch pin (`9430c441`), `mere-transport --features reticulum` green.
  Findings worth keeping:
  - The family was **relicensed MIT/Apache to MPL-2.0** the same day, with a
    MeshCore NOTICE in tucket and `vendor/lora-phy` untouched. This
    supersedes section 4's "licenses unchanged per crate" for the radio
    bucket; `deny.toml` already allows MPL-2.0. The founding MIT/Apache
    convention now has a deliberate exception, recorded here rather than
    treated as drift.
  - Deleting the incoming sub-lockfiles lost pins the firmware needed:
    `fixed` re-resolved to 1.31.0, which requires rustc 1.93 against a 1.92
    toolchain. Recovered by pinning 1.30.0 from tulle's old lock. **Every
    later absorption must diff the incoming lockfile before discarding it.**
  - tulle, sennet, and tucket had no CI; joining retinue's gated workspace
    surfaced eight pre-existing lints, now fixed or allowed with reasons.
    Absorbing an ungated repo into a gated one is a code-change event, not a
    move. Expect the same in C2 and C3.
  - Windows held a lock on the freshly written firmware images, so
    `git mv` of the directory failed; moving each subdirectory worked.
- **C1 landed** (woodshed `aa8026d` on the `redesign` branch, hocket
  `7fe70e1`): tinct now comes from genet.git in both, and hocket's
  audio-primitives and wavicle sibling paths became git pins.
  `cargo check -p hocket-engine` green. woodshed's active branch is
  `redesign`, not `main` — hocket's woodshed pin tracks `main` deliberately.
- **C2 landed** (genet `ccb0b5d91df`, pushed). cambium and netfetcher are
  genet components:
  `components/cambium/{cambium,cambium-nematic,cambium-winit,meristem,sprigging}`
  and `components/netfetcher`, histories preserved. Checks green for all six
  plus `genet-documents --features netfetch`; the standalone
  `genet_web_smoke` example resolves. Consumers repointed: merecat's cambium
  and sprigging, mere's netfetcher, both to genet.git. Findings:
  - **The absorption paid for itself immediately.** cambium carried three
    `[patch.crates-io]` entries (stylo_taffy, gpu-allocator, taffy) that
    existed *only* because path patches do not transit to git consumers.
    Path deps inside one workspace made all three unnecessary. The
    `genet_web_smoke` sibling path also became in-repo, closing the dev-only
    cycle between the two repos.
  - **Workspace inheritance is the real hazard in an absorption, not paths.**
    genet's `[workspace.package]` is Servo-shaped (version 0.2.0,
    `repository = servo/servo`, `publish = false`); cambium's crates are
    independently published at 0.1.0-0.3.0 under three different licenses.
    Inheriting silently would have mislabelled and unpublishable them. They
    now spell out version, rust-version, and repository, which is what
    genet's other adopted components (tinct, errand) already do. **C3 must
    check the same for every incoming crate against mere's workspace.**
  - Another session was live in genet throughout; its livery work was
    committed forward in two batches (`9d366ffefd5`, plus a length-value
    pass) before each subtree add, since `git subtree` refuses a dirty tree.
    One `cargo check` failure on untouched code proved transient on retry,
    as the workflow memory predicts.
- **C5 landed** (smolweb founded and pushed; genet `2a7c34c7b98`). The
  workspace holds misfin (history preserved via subtree) plus
  spartan-protocol, nex-protocol, and guppy-protocol; 47 tests pass. genet
  keeps errand and nematic and pins the wire layer from crates.io at `=0.1.1`,
  matching how it already consumed misfin. Finding: the three crates were
  copied rather than subtree-split — `git subtree split` over genet's history
  did not finish in ten minutes, so their 2026-07-10 to 2026-07-23 history
  stays in genet under `components/errand/protocols/`, recorded in the
  smolweb README. **A split out of a large repo is not a viable history move;
  plan for copy-plus-pointer.**
- **C3 landed** (mere `6a37de6b`, pushed). All nine families are mere
  workspace members with history preserved; the dead
  `crates/persona/identity` duplicate is deleted. `cargo check --workspace`
  is green except `document-host`, which is red on servitor's in-flight
  capability model from another session's unpushed commit — a break that
  predates the move, since mere already built against the local servitor
  through its patch file. Products repointed to mere.git: hocket, woodshed,
  and isometry committed and green (`isometry-genet`, which consumes
  scenograph and graphshell, builds clean); merecat verified last. Findings:
  - **The gitignored `.cargo/config.toml` patch files are the sharp edge of
    an absorption, and they are invisible to `git status`.** In mere, a
    redirect pointing at a path that is now a workspace member is a hard
    lockfile collision (`package collision in the lockfile`), not a warning,
    so every absorbed entry had to be deleted. In the four products the same
    entries had to be *redirected* to the new in-mere paths instead. Five
    files, none of them tracked in mere's case. Any future move must treat
    them as part of the change.
  - **Absorbing half a version-unified family splits it.** `graph-kernel`
    pulled numen from crates.io while quint used the workspace copy, so two
    numen versions landed in one graph and `mere-canvas` failed with
    "expected `quint::FieldId`, found `kernel::graph::FieldId`". The old
    crates.io patch had been hiding this. graph-kernel and moothold now take
    the workspace deps.
  - mere's `[workspace.package]` needed an `authors` key so the scenograph
    crates could keep inheriting; unlike genet's Servo-shaped defaults, the
    rest of mere's values already matched what the incoming families
    declared.
  - woodshed tracks its `.cargo/config.toml` (unlike mere and the others),
    so its machine-local absolute paths are committed. Pre-existing, noted
    rather than changed.
- **Toolchain bumped** (retinue `ac2fe43`). The machine's default rustup
  toolchain was **1.92.0**, five releases behind current stable; it is now
  **stable 1.97.1**. This closes the C4 firmware failure properly, and the
  post-mortem corrects what that entry said:
  - retinue **gitignores `Cargo.lock` while tulle tracked its own**, so the
    merge silently dropped the firmware's pinned resolution. The `fixed`
    1.30.0 downgrade recorded in C4 was therefore never committed — it was a
    local lockfile edit only, and a fresh clone would still have failed. The
    real fault was two-sided: an ancient default toolchain and a lost
    lockfile. Both are now closed (default bumped; `Cargo.lock` tracked, with
    the reason written into `.gitignore`).
  - **An absorption inherits the receiving repo's ignore rules, and that can
    silently drop the incoming repo's guarantees.** Diff `.gitignore` as well
    as manifests. Generalizes the C4 lockfile finding.
  - The six primary repos plus cambium pin 1.96.0 (wgpu-graft 1.95.0) via
    `rust-toolchain.toml` and are unaffected by the default. retinue and
    smolweb deliberately carry no pin and follow the default; both are green
    on 1.97.1 (retinue: fmt, clippy `-D warnings`, 251 tests, thumbv7em
    firmware; smolweb: 47 tests). The family pins are now one release behind
    stable — bumping them is a separate, deliberate pass.
