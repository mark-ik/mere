# License Posture Brief: MPL-2.0 by default, with correct provenance

**Status:** ruling recorded. Mark ruled on 2026-08-22 in conversation, in two
steps: first "copyleft on the platform, consumers license themselves", then,
on noticing that every app becomes a platform when it matures, the simpler
rule this brief records: MPL-2.0 is the default for everything Merely owns.
This brief is the durable record. No manifest has changed yet; the sweep is
the [license sweep plan](mere_docs/implementation_strategy/2026-08-22_license_sweep_plan.md).
**Date:** 2026-08-22.
**Scope:** cross-cutting, every repository Mark owns. Supersedes the founding
license convention of 2026-07-08 (MIT OR Apache-2.0 by default, MPL-2.0 only
for Servo-derived code), which survived only in memory and in the
[repo consolidation plan](mere_docs/implementation_strategy/2026-07-23_repo_consolidation_plan.md)
§4. Absorbs the radio-household ruling of 2026-07-23 and the games-wing
ruling of 2026-07-31, both of which become instances of the default.

---

## 1. The ruling

**Everything Merely owns is MPL-2.0. Third-party code keeps its provenance.
The one exception route is the fork/vendor criterion.**

- The default applies to every repository, without a classification step:
  the platform (mere, genet, netrender, the radio household), the games
  (mesocosm, paredros, isometry), the applications (turnstone, woodshed,
  hocket), and the standalone crates and reservation stubs (wavicle, mora,
  gaz, tulpa, djinn, tabard, and the rest). Inside a repository it applies to
  everything: the named crates, the `mere-*` and `genet-*` component crates,
  ports, probes, apps.
- **Correct provenance.** MPL-2.0 goes only on files Mark wrote. Code that
  arrived under another license keeps that license and its notice:
  `hyper_serde` and `malloc_size_of` (Servo's, MIT OR Apache-2.0),
  `support/patches/*` in genet (taffy, ipc-channel, gpu-allocator, sonic-rs),
  `support/patches/cubecl-*` in mere, `vendor/lora-phy` in retinue, genet's
  `LICENSE_WHATWG_SPECS`. A substantial derivative taken into the household
  is relicensed to MPL-2.0 with the upstream notice retained: tucket keeps
  MeshCore's MIT notice; cambium and meristem (xilem heritage, Apache-2.0,
  substantial own work) go to MPL-2.0 with the Apache notice retained (Mark,
  2026-08-22), replacing the exception genet's README records today. Each
  repository keeps a `LICENSES.md` scope record, the pattern the games wing
  already uses, so the retained permissive texts cannot be mistaken for a
  dual license.
- **No standing exceptions.** The fork/vendor test in §4 remains the only
  route by which a crate could stay MIT OR Apache-2.0, and as of 2026-08-22
  nothing takes it: `illume` and `buckram` were proposed, `errand` and `tinct`
  considered, and Mark declined all four. The rule is the simple one —
  everything Merely owns, with no list to maintain.
- **Edition 2024** for every founding, unchanged from 2026-07-08. isometry is
  on 2021; that is an edition migration, not a license matter, and is not
  part of the sweep.

Mark's rule of thumb before the rule existed: "make it MPL if I gave it one
of my funky lil names." The named crates are the identity-bearing work. The
default formalizes that intuition and stops treating the utilitarian crates
beside them differently.

## 2. Why

- **Classification was the churn.** Every license event since May was a
  boundary move that forced a re-decision: mere's July relicense, illume's
  promotion out to a permissive public repo, woodshed-graph's fold-in,
  paredros-identity's promotion to a permissive library. An app that matures
  becomes something other apps consume (woodshed's graph swatch is already
  the second consumer that gates a promotion), so "platform gets MPL, apps
  choose" would have meant relicensing each app at maturity. A default with no
  class boundary has no such events; only provenance remains to track, and
  provenance is a fact rather than a judgement.
- **MPL locks in other people's forks, never Mark's.** He is the sole
  copyright holder of his own code, so he can dual-license or relicense any
  of it later. The default removes options only from a third party who would
  take a Merely file closed.
- **The Mozilla posture, now everywhere.** Copyleft on what is built, any
  terms for what others build on it. MPL-2.0's scope is the file: §3.3 lets a
  larger work, statically linked included, ship under other terms so long as
  the MPL-covered files' source stays available. It stops closed forks of
  Merely files and does not stop closed products on top of them; the latter
  would need GPL or AGPL, which would cost third-party adoption of exactly the
  crates meant to be adopted (castellan as a Secret Service provider,
  personae, notochord).
- **The radio-household argument generalizes.** Meshtastic refused to
  relicense because GPL stops taking without giving back; a permissive
  reimplementation would be a legal route around that choice, and MPL shuts
  the door while staying consumable and GPL-compatible. Nothing about that
  reasoning was specific to radios.
- **Consumers barely notice.** mere's own lock already resolves 22 MPL-2.0
  registry crates (the stylo family, cssparser, selectors, hpke-rs, nucleo,
  option-ext) and 40 git dependencies on genet and netrender; both sibling
  `deny.toml` files allow MPL-2.0; there is no hard-copyleft dependency
  anywhere (the GPL entries are all `OR` permissive). Corporate allowlists
  pass MPL-2.0 for unmodified use. MPL-2.0 carries a patent grant; MIT does
  not.
- **The cost is credibility rather than law:** mere's public repository
  changes license for the second time in five weeks (§3). Hence one rule,
  ruled once, recorded with its reasons, so the change is a decision and not
  another accretion.

## 3. History, so this is not accretion

| Date | Event | Reason recorded |
|---|---|---|
| 2026-05-06 | mere founded MPL-2.0 (`dee147b4`) | inherited meerkat / Servo-fork boilerplate |
| 2026-07-08 | founding convention: MIT OR Apache-2.0 by default, MPL-2.0 only for Servo-derived code | permissive by default; MPL "only where legally required" |
| 2026-07-19 | mere relicensed to MIT OR Apache-2.0 (`a9902e3c`, 361 files) | provenance hygiene only: no Servo copyright remained, so the convention applied. Copyleft versus adoption was not weighed |
| 2026-07-23 | radio household relicensed MIT OR Apache-2.0 to MPL-2.0 (retinue `20fc747`) | reciprocity: a permissive reimplementation would route around Meshtastic's copyleft |
| 2026-07-31 | games wing: game code MPL-2.0, promoted reusable libraries MIT OR Apache-2.0, assets CC BY-SA 4.0 | software license for code, culture license for assets; libraries permissive once their boundary is proven |
| 2026-08-22 | platform rule: platform MPL-2.0, consumers license themselves; games are platform | Mozilla posture; the radio argument generalizes |
| 2026-08-22, same conversation | **this ruling**: MPL-2.0 by default for everything owned, with correct provenance | the platform rule deferred the same outcome to each app's maturity; classification was the churn |

The July move is not contradicted: it answered a provenance question and left
the strategic one open. The games-wing clause about promoted libraries is
reconciled rather than reversed: a promoted library is MPL-2.0 unless it
meets §4's criterion. The games' assets stay CC BY-SA 4.0; that clause was
about content, not code.

## 4. Exceptions: the fork/vendor criterion, currently empty

**Ruled 2026-08-22: there are no exceptions.** Every candidate was considered
and declined. The criterion below is kept as the documented test for any
future request, so a later case is judged against a standing rule rather than
re-argued from scratch, but it admits nothing today.

The test, when one is proposed: a crate stays MIT OR Apache-2.0 only when a
third party would need to **modify or vendor** it rather than merely link it —
reference implementations of external standards meant to be embedded and
adapted, and contracts designed for foreign implementers. "Someone might want
to use it" is not the criterion; MPL-2.0 already permits use.

Considered and declined 2026-08-22:

| Crate | The case for it | crates.io at the ruling |
|---|---|---|
| `illume` (genet) | portable Djot and syntax lexer, deliberately taken permissive in June 2026 as a standalone public library | 0.0.2, MIT OR Apache-2.0, 84 downloads |
| `buckram` (genet) | standards-owned CSS box and fragment models | unpublished |
| `errand` (genet) | the small-web transport (gemini, gopher, finger, spartan, nex, guppy, titan); protocol clients get vendored and adapted | 0.3.4, MIT OR Apache-2.0, 281 downloads — the most-adopted candidate |
| `tinct` (genet) | OKLCH seed-to-palette derivation, a small self-contained algorithm | 0.1.2, MIT OR Apache-2.0, 144 downloads |

MPL-2.0 under the default and never proposed: `sceno` (scene contracts),
`chirograph` (the projection wire contract), `register-renderer-types`
(data-only host vocabulary), and castellan's OTP core and any CXF importer
once either is split into a crate of its own.

Published versions keep the grant they shipped with: `illume` 0.0.2, `errand`
0.3.4, and `tinct` 0.1.2 stay MIT OR Apache-2.0 permanently at those versions
and carry MPL-2.0 from their next functional bump. A related inconsistency the
sweep resolves: `nematic` is already published as MPL-2.0 (0.1.0) while its
in-tree manifest says MIT OR Apache-2.0, a leftover from the July convention
sweep.

## 5. Repository table

| Repository | Today | Under the default | Sweep phase |
|---|---|---|---|
| mere | MIT OR Apache-2.0 (2 probe crates and signalman MPL-2.0) | MPL-2.0 | P1 — landed 2026-08-27 |
| genet | MPL-2.0 default; ~25 own and cambium manifests permissive | MPL-2.0 throughout, no exceptions | P2 — landed 2026-09-03 |
| netrender | MPL-2.0, block-form headers | MPL-2.0, shape C | P7 — landed 2026-09-03; Mark ruled the two WebRender-derived files his, with a historical attribution line retained above the header |
| retinue household | MPL-2.0, 383 of 388 files headerless | MPL-2.0, shape C | P7, first — waits on its lane's dirty tree |
| mesocosm, paredros | MPL-2.0, shape A headers | MPL-2.0, shape C | P7 + the promoted-library clause — headers landed 2026-09-03; the clause and the single-`LICENSE` layout pending, with `paredros-identity`'s grant needing a ruling |
| isometry | MIT OR Apache-2.0, edition 2021 | MPL-2.0 (edition separate) | P3 — waits on its lane's dirty tree |
| turnstone, woodshed, hocket | MIT OR Apache-2.0 | MPL-2.0 | P4, each when its tree is clean — turnstone and hocket landed 2026-09-03; woodshed waits on its lane's dirty tree |
| wavicle, mora, gaz, reservation stubs | MIT OR Apache-2.0 | MPL-2.0 | P5 — wavicle and mora landed 2026-09-03; the stubs went with mere and genet; gaz has no repository on GitHub any more (checked 2026-09-03), so there is nothing to sweep and the name stays Mark's by the naming ledger |
| wgpu-graft / -scry / -weld | MPL-2.0 manifests over a largely Apache-2.0 wgpu fork | MPL-2.0 on Mark's files only | P7, after a provenance ledger — 2026-09-03: wgpu-scry and wgpu-weld landed; wgpu-graft's 57 owned sources headered the same day on Mark's ruling, the vendored 86% (GPUI, not wgpu) retained under its own licenses |

## 6. What the sweep touches

Numbers measured 2026-08-22 on each repository's `main`.

**mere.** 120 manifest `license` lines: 73 inherit `[workspace.package]`
(one line flips them), 44 say `MIT OR Apache-2.0` explicitly, 2 already say
`MPL-2.0`, 1 says `Apache-2.0`. 1,043 tracked `.rs` files, of which 442 carry
the July `Copyright 2026 Mark AB (markik)` / SPDX header and about 600 carry
none; all of Mark's receive Exhibit A. 24 directories carry a `LICENSE-MIT` /
`LICENSE-APACHE` pair (two of them third-party patches that keep theirs). 44
READMEs name the permissive license.

**genet.** About 18 own permissive manifests (inker and its engines,
genet-probe, genet-clipboard, genet-livery, fleece, tabard, verso-tile,
buckram, errand, nematic, illume, tinct, the livery import tool), cambium and
meristem with a retained Apache notice, five name-claim stubs, 12 READMEs,
and the root README exception line. Third-party-derived manifests stay.

**isometry** 123 tracked `.rs`, no headers, LICENSE pair, README §License.
**turnstone** 97, none, pair, README. **woodshed** 74, none, pair, README; 47
files in flight on 2026-08-22, so it waits. **hocket** 31, none, pair, README.
**wavicle** 13, 4 headed, pair, README. **mora** (`repos/mora`, published
0.1.0 MIT OR Apache-2.0). **gaz** lived only on GitHub (`merely-made/gaz`);
its code is the subtree in mere. On 2026-09-03 the repository was found gone
from GitHub under both accounts; the name remains claimed in the naming ledger.

**Rules carried over, with 2026-08-22's confirmations.** Exhibit A in source
files, never Exhibit B ("Incompatible With Secondary Licenses"), so §3.3 keeps
everything GPL-compatible and the GPLv3 firmware images keep working. One
`LICENSE` file per repository; the dual files go. **Header shape: a copyright
line, Exhibit A, and an SPDX tag**, in `//` line comments — shape C of the
four already in the tree; the sweep plan's invariant 3 carries the exact form.
**Copyright holder: `Mark Alan Boykin`**, replacing the `Mark AB (markik)`
string that sits on 489 files across the lattice. MPL requires no copyright
notice at all (Exhibit A: "You may add additional accurate notices of
copyright ownership"), so the line is attribution by choice. No license-only
republish: a crate carries the new license to crates.io at its next functional
bump.

**Entity.** Merely LLC (Kentucky, registered 2026-07-19, sole member) exists,
but copyright vests in the human author at fixation and reaches a company only
by written assignment; a notice is evidence and attribution, not title. The
notice therefore names Mark personally. If the LLC should hold the IP — which
matters once district or ARC money is in play — that is an assignment to
execute, not a string to change in a header, and §3.4 permits correcting the
notice afterwards. Worth a lawyer's time rather than a decision taken here.

**crates.io today.** 47 of mere's crates are published: 40 under
MIT OR Apache-2.0, whose published versions keep that grant permanently, and
7 never republished since the July move (gemot, graphlets, mere-proofs,
moothold, mooting, uxtree, workbench) still under MPL-2.0, which this ruling
makes correct again. `knot` is taken on crates.io by an unrelated crate
(raultov, 1.6.2); mere's `knot` package cannot publish under that name.

**Consent.** None required. Mark is the sole human author across every
repository; bot and assistant commits carry no competing claim; the gaz
subtree is his. MIT and Apache-2.0 both permit relicensing a derivative
provided the notice is retained.

## 7. Follow-ups this ruling creates

1. The [license sweep plan](mere_docs/implementation_strategy/2026-08-22_license_sweep_plan.md),
   written 2026-08-22: tooling and provenance ledger first, then mere, genet,
   isometry, the applications as their trees come clean, the standalones.
2. The `founding-license-edition-convention` memory in `Code/membackup/`
   rewritten to this rule (Mark authorized, done 2026-08-22).
3. Reconcile the promoted-library clause in the mesocosm and paredros
   founding docs, and genet's README exception line, in their own
   repositories, inside the sweep's P2 and docs phases.
4. The consolidation plan's §4 line ("MPL stays genet-side") points here
   (done 2026-08-22).

## 8. Side findings from the survey

- mere's workspace does not resolve on this machine: genet `55c05d11`
  ("Retire Stylo and the incumbent layout cone", 2026-08-21 01:56) deleted
  `components/genet-layout/Cargo.toml`, while mere's `.cargo/config.toml`
  still patches `genet-layout` to that path. Any `cargo metadata` over mere
  fails until that entry is retired. Reported, not touched; another lane owns
  it.
- The dependency license mix was therefore measured from `Cargo.lock` against
  the local registry cache: 1,387 registry packages, all resolved, none under
  a hard copyleft.

## 9. Progress

- **2026-08-22:** Mark opened with "I should license mere under MPL-2.0".
  Survey of manifests, headers, crates.io, the lock, sibling repositories,
  and the recorded history. First ruling: platform copyleft, consumer
  freedom, games are platform, fork/vendor exceptions, cambium to MPL with
  notice. Mark then asked whether that merely deferred the same outcome to
  each app's maturity, and ruled MPL-2.0 by default with correct provenance.
  Brief rewritten to the default; sweep plan written; memory rewritten. No
  manifest changed.
- **2026-08-22, the three confirmations.** Mark asked to see the header
  options, the copyright question, the obligations, and the exceptions list.
  Four header shapes exist in his own repos (mesocosm/paredros `//` + SPDX;
  netrender/genet-Servo block comment; the July copyright + SPDX form;
  retinue's 383-of-388 headerless state, licit under Exhibit A's LICENSE-file
  clause). He ruled: **shape C** (copyright + Exhibit A + SPDX),
  **`Mark Alan Boykin`** as the notice, and **no exceptions** — `illume`,
  `buckram`, `errand`, and `tinct` all declined. The survey that produced the
  question also surfaced Merely LLC and the §3.2 executable-form obligation
  (a packaged build must tell recipients how to get source; that belongs to
  the luggage/velopack packaging lane, not this sweep).
