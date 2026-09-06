# License Sweep Plan

**Date:** 2026-08-22
**Status:** **P0 and P1 landed 2026-08-27; P2 (genet), P4 turnstone and hocket, P5 mora and wavicle, and P7 mesocosm, paredros, netrender, wgpu-graft, wgpu-scry and wgpu-weld landed 2026-09-03; P6 clause and layout landed 2026-09-03**; P3 isometry, P4 woodshed and P7 retinue wait on their lanes' dirty trees; P5 gaz has no repository to sweep (see 2026-09-03); one P2 hazard open in genet (see 2026-09-03). mere is MPL-2.0 by
default with correct provenance — receipt in §6's Progress. Both P0
confirmations were settled 2026-08-22 (header shape C, the notice
`Mark Alan Boykin`, no exceptions); P0's tooling and ledger were built
2026-08-27, and its two open verifications are answered there. Two rulings
were taken during the work: `crates/system/luggage` carries MPL-2.0 with the
Tauri/CrabNebula notices retained, and published crates ship no license text
file (root `LICENSE` only), which struck one P1 done-condition. The remaining
gate for each later phase is unchanged: a clean tree in that repository.
**Scope:** Carry the 2026-08-22 ruling, MPL-2.0 by default with correct
provenance, into every owned repository: manifests, source headers, LICENSE
files, READMEs, and a provenance ledger per repository. No code changes, no
version bumps, no publishes, no edition migrations.

**Related:**

- [license posture brief](../../2026-08-22_license_posture_brief.md) (the ruling)
- [repo consolidation plan](2026-07-23_repo_consolidation_plan.md) §4 (superseded line)
- mere's July relicense `a9902e3c` and its copyright-string follow-up `fc90a3f`
  (the reverse sweep: 361 files, line endings preserved per file)
- retinue `20fc747` (single `LICENSE`, dual files removed, `deny.toml` allowing MPL)
- mesocosm `2026-07-30_games_wing_founding.md` §8 (the `LICENSES.md` scope-record pattern)
- [MPL-2.0 text and Exhibit A](https://www.mozilla.org/MPL/2.0/)

## 1. Ruling

Everything Merely owns is MPL-2.0. Third-party code keeps its provenance. The
one exception route is the fork/vendor criterion. See the brief; this plan
does not re-argue it.

## 2. Invariants

1. **Provenance before license.** A file receives Exhibit A only if Mark
   wrote it. Every path that keeps another license is listed in the
   repository's `LICENSES.md` with its license, upstream, and notice file.
   Discovery is mechanical: grep for `Copyright (c)`, `Licensed under`,
   `Permission is hereby granted`, `Apache License`, and existing SPDX lines
   naming anything but MPL-2.0, then read the hits.
2. **Exhibit A, never Exhibit B.** The string "Incompatible With Secondary
   Licenses" must not appear in any owned source file.
3. **Header shape, confirmed 2026-08-22** (shape C of the four in the tree):

   ```rust
   // Copyright 2026 Mark Alan Boykin
   // This Source Code Form is subject to the terms of the Mozilla Public
   // License, v. 2.0. If a copy of the MPL was not distributed with this
   // file, You can obtain one at https://mozilla.org/MPL/2.0/.
   // SPDX-License-Identifier: MPL-2.0
   ```

   The notice replaces `Mark AB (markik)` wherever it appears (489 files across
   the lattice). Exhibit A attachment is what makes a file Covered Software
   (§1.4), so the header is the operative act and the manifest field is only
   registry metadata. The copyright line itself is optional under Exhibit A
   ("You may add additional accurate notices of copyright ownership") and is
   attribution by choice; Merely LLC is deliberately not named, per the
   brief's entity note.

   Servo-derived files in genet keep Servo's bare Exhibit A; no Merely
   copyright line is added to code Mark did not write. The header goes above
   `//!` inner docs and below a shebang. Each file's line endings are
   preserved (mere is mixed CRLF/LF; the July sweep preserved per file and so
   does this one). Non-Rust sources (`.py`, `.ps1`, `.wgsl`, `.js`) get the
   same notice in their comment syntax when they are Mark's.
4. **One `LICENSE` per repository** holding the MPL-2.0 text; `LICENSE-MIT`
   and `LICENSE-APACHE` go, at the root and per crate. Third-party patch and
   vendor directories keep their own files. mere's text is restored from
   history: `git show a9902e3c^:LICENSE-MPL`.
5. **Manifests** say `license = "MPL-2.0"` or `license.workspace = true`.
   Never `license-file` for owned crates. There are no exceptions as of
   2026-08-22, so no owned manifest keeps a permissive string; if one is ever
   granted under the brief's §4 test, it says `MIT OR Apache-2.0` explicitly
   with a comment naming the brief.
6. **No functional change.** Headers are comments and the `license` field
   does not affect a build, so the verification is a sample `cargo check -p`
   per repository plus `cargo package --list` on one member crate to confirm
   Cargo copies the workspace-root `LICENSE` into a package that has none
   (verify this at P0 rather than assume it).
7. **One repository per commit series. Never sweep a dirty tree.** On
   2026-08-22 woodshed has 47 files in flight and mere has three foreign
   modified files plus the `genet-layout` resolve breakage; each waits.
8. **No republish.** `repository` fields untouched. A crate carries the new
   license to crates.io at its next functional bump.
9. **READMEs** replace their license line with `MPL-2.0 (see LICENSE)`;
   exceptions and retained paths state theirs and point at `LICENSES.md`.
10. `.gitattributes`, `.cargo/config.toml`, `deny.toml` allowlists, and the
    weave wiring are untouched.

## 3. Phases

### P0. Tooling, ledger template, confirmations

Tasks:

- write `scripts/relicense_headers.py` in mere (Python is on the machine and
  `scripts/` already holds `check_port_boundaries.py`): modes `--dry-run`
  (list the files that would change and why), `--apply`, `--audit` (counts
  per repository: manifests by license, tracked sources without Exhibit A,
  Exhibit B hits, LICENSE files present); a `--skip-from LICENSES.md` list;
  per-file line-ending preservation; idempotent on rerun; a `--bare` flag for
  Servo-derived files;
- write the `LICENSES.md` template: "This repository: MPL-2.0", "Retained
  licenses" (path, license, upstream, notice file), "Exceptions under the
  fork/vendor criterion", "How to add a file from elsewhere";
- verify `cargo package --list -p <member>` in a workspace whose root holds
  `LICENSE` and whose member does not, and record the answer;
- check whether `crates/probes` are tracked (the July memory says they were
  gitignored then; the tree lists their manifests now);
- (done 2026-08-22) Mark confirmed the header shape and ruled out exceptions.

Done when:

- `--dry-run` over mere lists exactly the tracked sources minus ledger paths;
- `--apply` followed by `--apply` again produces an empty diff;
- one CRLF file and one LF file round-trip unchanged except for the header;
- `--audit` prints the §6 table columns for mere, genet, isometry, turnstone,
  woodshed, hocket, wavicle, mora;
- `--dry-run` shows the new notice replacing `Mark AB (markik)` on a sample of
  the 489 files that carry it, and adding it where no header exists.

### P1. mere

Preconditions: clean tree; the `genet-layout` patch entry retired by its lane
so `cargo metadata` resolves.

Tasks:

- `[workspace.package] license = "MPL-2.0"` (73 crates follow);
- 44 explicit `MIT OR Apache-2.0` lines and `nexus-lbvh-probe`'s
  `Apache-2.0` to `MPL-2.0` (or `license.workspace = true` where the crate
  already inherits everything else); the 2 existing MPL-2.0 lines stay;
- `LICENSES.md` naming `support/patches/cubecl-runtime` and
  `support/patches/cubecl-wgpu` (MIT OR Apache-2.0, tracel-ai/cubecl) and
  anything invariant 1's grep finds;
- root `LICENSE` restored from `a9902e3c^:LICENSE-MPL`; root and 22 owned
  per-crate `LICENSE-MIT`/`LICENSE-APACHE` pairs removed;
- headers on every owned tracked source (1,043 `.rs` on 2026-08-22: 442 with
  the July header replaced, about 600 bare ones given one);
- root README §License and 44 crate READMEs;
- commit subject in the July style: `Relicense mere to MPL-2.0 by default`,
  body pointing at the brief.

Done when:

- `git grep -l 'MIT OR Apache-2.0' -- '*.toml'` returns only ledger paths and
  brief-named exceptions;
- `git grep -L 'Mozilla Public' -- '*.rs'` returns only ledger paths;
- `git grep -l 'Incompatible With Secondary'` returns nothing;
- `ls LICENSE*` at the root prints `LICENSE` alone;
- ~~`cargo package --list -p personae` includes `LICENSE`~~ — **struck
  2026-08-27.** P0 verified against retinue, which already has the target
  layout, that Cargo does *not* copy a workspace-root `LICENSE` into a member
  package: retinue's root carries one, `linkboy` carries none, and
  `cargo package --list -p linkboy` includes no license text. This
  done-condition and invariant 4 could not both hold. Mark ruled for
  invariant 4: **one `LICENSE` per repository, and published crates ship the
  SPDX field plus Exhibit A in every source, with no license text file** —
  which is what retinue's published crates already do. Licit, since §1.4
  makes Exhibit A in the file the operative act;
- `cargo check -p castellan -p personae -p notochord -p sceno` is green;
- `--audit` for mere reports zero unheaded owned sources.

### P2. genet

Tasks:

- the ~18 owned permissive manifests (inker, document-canvas, the three
  engines, knot-editor-host, genet-probe, genet-clipboard, genet-livery,
  fleece, tabard, verso-tile, buckram, errand, nematic, illume, tinct, the
  livery import tool) to `MPL-2.0` — all of them, since no exception was
  granted; `nematic` merely returns to what crates.io already publishes;
- cambium and meristem to `MPL-2.0`; their existing `LICENSE` and
  `LICENSES.md` under `components/cambium/` *(historical citation)* <!-- doc-audit: historical-path --> reworked so the xilem Apache-2.0
  attribution is retained as a notice, not presented as the crate license;
- the name-claim stubs `cambium`, `frisket`, `genet-livery`, `livery`,
  `meristem` to `MPL-2.0`; `genet-taffy` (a taffy fork, MIT) stays and is
  ledgered;
- `LICENSES.md` at the root naming `hyper_serde`, `malloc_size_of`,
  `support/patches/{taffy,ipc-channel,gpu-allocator,sonic-rs}`,
  `LICENSE_WHATWG_SPECS`, `genet-taffy`, and the Servo heritage itself;
- headers: owned files get the Merely header; Servo-derived files without a
  header get bare Exhibit A (`--bare`); the 484 already headed are untouched;
- root README license paragraph rewritten: MPL-2.0 throughout, the exception
  crates named, the ledger linked; 12 crate READMEs;
- `cargo deny check licenses` still green (MPL-2.0 is already allowed).

Done when: the P1 greps hold for genet with its ledger; `cargo deny check
licenses` green; `cargo check -p genet-probe -p inker` green; `--audit` zero.

### P3. isometry

Tasks: workspace line, the 2 explicit lines, single `LICENSE`, README §License
(line 49), headers on 123 owned sources, `LICENSES.md`. Edition stays 2021;
the migration is its own task, not this plan's.

Done when: P1 greps hold; `cargo check` on the workspace default members
green; `--audit` zero.

### P4. Applications, each when its tree is clean

turnstone (97 sources, README line 76), woodshed (74, line 54; 47 files in
flight on 2026-08-22), hocket (31, line 54). Same task list as P3. woodshed's
`woodshed-web` is already MPL-2.0 and stays.

Done when: per repository, P1 greps hold, a sample `cargo check` is green,
`--audit` zero.

### P5. Standalones

wavicle (13 sources, 4 already headed), mora (`repos/mora`, published 0.1.0
MIT OR Apache-2.0; the new license ships at its next functional bump), and
gaz (GitHub only, `merely-made/gaz`; either relicense in one commit there or
archive it with a tombstone README pointing at mere's subtree, Mark's call
when reached). Reservation stubs inside mere and genet are covered by P1 and
P2.

### P6. Documents

- mesocosm and paredros founding docs: the promoted-library clause now reads
  "MPL-2.0 unless the fork/vendor criterion"; assets stay CC BY-SA 4.0;
- genet root README (done in P2), mere README (P1);
- the posture brief's §5 table marked per phase as each lands;
- this plan archived under `archive_docs/` when P1-P6 are done.
- **Added 2026-09-03, from the platform boundary moves:** seven published
  crates arrived in mere from genet under their own published grant (inker,
  document-canvas, nematic, errand, illume, tinct, verso-tile; `tinct` and
  `verso-tile` still carry `LICENSE-MIT`/`LICENSE-APACHE`). Their manifests
  already read MPL-2.0 from genet's sweep; the published versions keep the
  old grant permanently and the new one ships at each crate's next functional
  bump (the brief's no-republish rule). mere's `LICENSES.md` records them.
  Open: whether the two notice-file pairs are removed at that bump.

  **2026-09-03, superseding ruling.** Mark ruled the promoted-library clause
  retired outright rather than reworded to the fork/vendor criterion this
  section originally planned: `paredros-identity` becomes MPL-2.0 like the
  rest of the wing, and the 2026-07-31 "promoted reusable libraries MIT OR
  Apache-2.0" clause is retired. Recorded, with this date, in both
  repositories' `LICENSES.md` (their "Reusable libraries" sections), not in
  the founding docs themselves, which stay as the historical record of the
  original clause. Assets stay CC BY-SA 4.0 as planned; both `LICENSES.md`
  files also now note that content Mark expects later (body templates, a
  defined core of data types plus an extensible frontier where new types are
  made and core ones combined) follows the same asset grant.

### P7. Normalize the already-MPL repositories

Ruled 2026-08-22. Shape C is the house header, and none of the repositories
that were already MPL-2.0 matches it: mesocosm (118 of 122 files) and paredros
(46 of 57) use shape A, netrender (89 of 104) uses the block form, and retinue
carries no per-file notice at all (383 of 388 bare). Every one of them is
licit — Exhibit A permits the LICENSE-file location — but none carries a
copyright line, and retinue's files are not individually marked as Covered
Software, which §1.4 makes the operative fact.

Order, by what each gains:

1. **retinue household** (retinue, tulle, sennet, tucket): the largest gain,
   since these files carry no notice at all. `vendor/lora-phy` is excluded and
   tucket's MeshCore `NOTICE` is untouched.
2. **netrender**: block form to shape C, 104 files.
3. **mesocosm, paredros**: shape A already; this only adds the copyright line.
   Their `LICENSES.md` and the CC BY-SA 4.0 asset scope stay as they are.
4. **wgpu-graft / -scry / -weld**: **provenance pass before any header
   change.** wgpu-graft is largely a wgpu fork — 25 Apache-2.0 manifests
   against 12 MPL-2.0, and only 3 of 411 files carry an MPL header — so most
   of that tree is upstream's and keeps its own license and notices. Only
   Mark's own files take the header, and the ledger has to be written before
   the tool runs. If the ledger turns out to be most of the repository, say so
   and leave it.

Tasks per repository: run the P0 tool in `--apply` with that repository's
ledger; confirm its single `LICENSE` and its `LICENSES.md`; no manifest
changes, since they already say MPL-2.0.

Done when, per repository: `git grep -L 'Mozilla Public' -- '*.rs'` returns
only ledger paths; every owned file carries the copyright line; a sample
`cargo check` is green; `--audit` reports zero unheaded owned sources.

## 4. Verification receipt

The `--audit` table, one row per repository, recorded in §6 at the end of
each phase: manifests by license; owned sources without Exhibit A (must be
0); Exhibit B hits (must be 0); LICENSE files at root (must be `LICENSE`
alone); ledger paths count; sample `cargo check` result; `cargo deny` result
where a `deny.toml` exists.

## 5. Stop line

No version bumps. No publishes. No edition migrations. No code edits beyond
comment headers. No sweeping a tree with uncommitted work. No changes to
`.cargo/config.toml`, `deny.toml`, or `.gitattributes`. No changes inside
third-party patch or vendor directories beyond listing them in the ledger.
No CLA, DCO, or contributor-agreement files: MPL-2.0's own grant is the
contribution term. The §3.2 executable-form obligation is real but out of
scope here: a packaged build must tell recipients how to obtain the Source
Code Form "by reasonable means in a timely manner, at a charge no more than
the cost of distribution", which is one line in an About panel or installer
and belongs to the luggage/velopack packaging lane. Public repositories
already satisfy it for source distribution. In the already-MPL repositories
(netrender, the radio household, mesocosm, paredros, the wgpu trio) P7 changes
headers only — never a manifest, a `LICENSE`, or a published version — and
never before that repository's ledger is written.

## 6. Progress

- **2026-08-22:** plan written from the posture brief's survey.
- **2026-08-22, confirmations:** Mark ruled header shape C, the notice
  `Mark Alan Boykin` (replacing `Mark AB (markik)` on 489 files), and no
  exceptions — `illume`, `buckram`, `errand`, and `tinct` all declined.
  Invariants 3 and 5 updated; P0's confirmation task closed. P0's tooling work
  has not started.
- **2026-08-22, P7 ruled:** the already-MPL repositories are normalized to
  shape C, retinue first. Added as P7 with a provenance caveat for the wgpu
  trio, whose Apache-2.0 majority is upstream wgpu rather than Mark's.
- **2026-08-22, P0 held:** not commissioned. mere's tree carries 85 files from
  a concurrent scenograph absorption (`crates/canvas/arrangements` *(historical citation)* <!-- doc-audit: historical-path --> deleted,
  `scenomise/families/` added, plus a Turnstone census and a muniment OPFS
  probe), so invariant 7 blocks P1 regardless. The §6 counts are as-measured
  on 2026-08-22 and will drift when that lane lands; re-run `--audit` before
  P1 rather than trusting them.
- **2026-08-27, P0 built and largely done.** `scripts/relicense_headers.py`
  written and verified against every P0 done-condition: `--dry-run` lists 1040
  owned sources against 363 ledger-skipped; `--apply` twice produces an empty
  diff; a CRLF file (`crates/armillary/src/lib.rs`) and an LF file
  (`scripts/check_port_boundaries.py`) each round-trip with their endings
  intact; a shebang keeps its first line; a July-headed file has
  `Mark AB (markik)` + the permissive SPDX replaced by shape C with its `//!`
  docs preserved. `LICENSES.md` written at the root, and the tool reads its
  table rows' first column as the skip list. `--audit` prints the §4 receipt
  for mere, isometry, turnstone, woodshed, hocket, wavicle and mora; genet's
  run exceeds two minutes and needs a longer budget or an index-based check.
  Test edits were reverted; only the two new files remain.
- **2026-08-27, five P0 findings, three of which change P1.**
  1. **Invariant 1's discovery pattern is wrong.** It greps `Copyright (c)`,
     but `crates/system/luggage` writes bare `Copyright 2019-2023 <holder>`
     with no parenthesised `(c)`, so the plan's own mechanical discovery finds
     nothing there. Running P1 as written would have stripped
     `Copyright 2019-2023 Tauri Programme within The Commons Conservancy` and
     `Copyright 2023-2023 CrabNebula Ltd.` and relicensed their code to
     MPL-2.0 under Mark's notice. Grep `Copyright` unqualified, then read.
  2. **`crates/system/luggage` is a fork of Tauri/CrabNebula's
     `cargo-packager-updater`**, published 0.1.0, named in neither the brief
     nor this plan. Eight files carry upstream copyright beside Mark's.
     Ledgered as pending; the brief's substantial-derivative rule (tucket,
     cambium, meristem) would make it MPL-2.0 with the upstream notice
     retained, but that is Mark's call, not an inference.
  3. **`support/patches` holds five third-party trees, not the two listed:**
     `cubecl-runtime`, `cubecl-wgpu`, `cubek-reduce`, `burn-cubecl`,
     `burn-remote` (tracel-ai, MIT OR Apache-2.0), 384 tracked files.
  4. **Cargo does not copy a workspace-root `LICENSE` into a member package.**
     Verified on retinue, which already has the target layout: root `LICENSE`
     present, `linkboy` carries none, and `cargo package --list -p linkboy`
     includes no license text at all. So invariant 4 ("one `LICENSE` per
     repository; the per-crate files go") and P1's done-condition
     ("`cargo package --list -p personae` includes `LICENSE`") cannot both
     hold. Retinue's published crates ship today with the SPDX field and no
     license text, which is licit — Exhibit A in each source is the operative
     act under §1.4 — but it is a posture to choose deliberately, not to
     inherit by deleting files. **Open for Mark.**
  5. **`crates/probes` are tracked** (9 files); the July memory saying they
     were gitignored is stale. P0's fourth task is closed.
- **2026-08-27, P1's preconditions:** the `genet-layout` patch entry no longer
  blocks — `cargo metadata` over mere resolves cleanly and repeatedly. The tree
  was cleaned by commit `83615038`. Both gates in the Status line are now
  clear; P1 waits only on findings 2 and 4 above.
- **2026-08-27, baseline `--audit` before P1** (compare after): mere 125
  manifests (77 workspace, 42 `MIT OR Apache-2.0`, 4 MPL-2.0, 2 none), 1040
  owned sources, 0 with Exhibit A, 0 Exhibit B. isometry 129 owned; turnstone
  121; woodshed 78; hocket 31; wavicle 14; mora 7. Every one of them carries a
  `LICENSE-APACHE`/`LICENSE-MIT` pair and zero Exhibit B hits.
- **2026-08-27, P1 LANDED.** mere is MPL-2.0 by default with correct
  provenance. What changed: `[workspace.package] license = "MPL-2.0"`; 37
  permissive manifest lines rewritten (23 to `license.workspace = true` where
  the crate is a workspace member, 14 to explicit `MPL-2.0` where it is not);
  the root `LICENSE` restored from `a9902e3c^:LICENSE-MPL` (350 lines) and 46
  owned `LICENSE-MIT`/`LICENSE-APACHE` files removed, with support/patches'
  8 kept; shape-C headers written to **1050 owned sources**; 39 markdown files
  moved to `MPL-2.0 (see LICENSE)`, and luggage given a License section it
  never had.
  **Receipt (`--audit`, after):** manifests `{MIT OR Apache-2.0: 5,
  MPL-2.0: 18, workspace: 100, (none): 2}` — the 5 permissive are
  support/patches' own; owned sources 1050, **0 without Exhibit A**,
  **0 Exhibit B hits**; root prints `LICENSE` alone beside `LICENSES.md`;
  5 ledger paths. `cargo check -p castellan -p personae -p sceno` (pulling
  notochord) green in 5m08s, so invariant 6 holds — the headers are comments
  and changed no behaviour.
  **luggage** took MPL-2.0 with the upstream notice retained, per Mark's
  2026-08-27 ruling on the brief's substantial-derivative precedent: all 7
  derived files keep `Copyright 2019-2023 Tauri Programme within The Commons
  Conservancy` and `Copyright 2023-2023 CrabNebula Ltd.` verbatim above Mark's
  line, verified one occurrence per file with no duplication. `staging.rs` and
  `bin/luggage-manifest.rs` are Mark's own and carry no upstream notice, which
  the tool classified correctly per file.
  **Two greps still report matches, both correct:** `MIT OR Apache-2.0` in
  `crates/system/luggage/Cargo.toml` is a comment naming the upstream's
  license, and the Exhibit B string appears in this plan and the posture brief
  because both quote the rule. Neither is a violation, and P1's done-condition
  wording should say so.
  **Tooling note:** `--retain-notice` was added during P1 for exactly the
  luggage case; without it the header stripper would have deleted the upstream
  copyright lines. The Exhibit B literal in the tool is built by concatenation
  so the tool never matches itself.
- **2026-08-27, remaining:** P2 genet (its `--audit` needs a longer budget than
  two minutes; consider an index-based check), P3 isometry, P4 the
  applications, P5 standalones, P6 documents, P7 the already-MPL repositories.
  Baseline counts for P3-P5 were taken 2026-08-27 and are in the P0 entry.
  **No crate was republished for the license change**, per invariant 8.
- **2026-09-03, P2 LANDED** (genet `957926e4e8a`). genet is MPL-2.0 by default with
  correct provenance. What changed: 23 owned manifests to `MPL-2.0` (the ~18
  permissive ones the plan lists plus sprigging, meristem, document-session-api
  and the cambium, frisket, genet-livery, livery and meristem name claims);
  23 owned per-crate license texts removed, leaving the single root `LICENSE`
  beside `LICENSE_WHATWG_SPECS` (Servo's retained notice file, ledgered);
  meristem keeps its Apache-2.0 text as the Xilem notice file; hyper_serde and
  malloc_size_of keep their pairs as retained Servo-lineage crates; a root
  `LICENSES.md` with 18 ledger paths (the five vendored patches, the two
  Servo-lineage crates, the WPT, Blink, Dromaeo and jQuery corpora, Servo's
  test pages and resources, mozdebug, the WHATWG text) and a Servo-heritage
  section; ten READMEs. Headers: 384 files, 269 added and 115 replaced (the
  76 July headers and 39 meristem files keeping the Xilem Authors' line above
  the new header, via `--retain-notice`); second run changes 0.
  **Receipt (`--audit`, after):** 881 owned sources of 66,421 tracked, **7
  without Exhibit A** (held out, below), **0 Exhibit B hits**; manifests
  `{MPL-2.0: 50, workspace: 66}` plus 8 permissive lines all in retained
  trees. `cargo deny check licenses` green; `cargo check -p genet-probe -p
  inker` green; line endings preserved per file (a CRLF file gained exactly
  the six CR lines of its header).
  **Tool change for this repository:** `relicense_headers.py` gained a guard
  that leaves any file whose leading lines already carry Exhibit A untouched,
  whatever comment shape carries it. Its header detector saw only line
  comments and would otherwise have stacked a second header on the 497
  Servo-derived files that carry Exhibit A as a `/* ... */` block. No
  `--bare` run was needed: every bare owned file was Mark's.
  **Invariant 7 was breached and recovered.** genet's status was not checked
  before `--apply`, and two other lanes had ten files in flight (a Cambium
  pointer-button lane, a Livery paint lane with scratch tests). Seven of those
  had received a header; it was removed again so their working copies carry
  only their lanes' edits, and none of the ten is in the commit. They take
  their headers when those lanes land, by rerunning the idempotent tool; the
  7 in the receipt are exactly them. Check `git status` before every
  `--apply`; the tool should refuse a dirty tree itself, which is a P3 item.
  **Two greps report matches that are correct:** `MIT OR Apache-2.0` in
  `components/inker/Cargo.toml` *(historical citation)* <!-- doc-audit: historical-path --> is a comment naming the published 0.1.1's
  grant, and the Exhibit B string appears in `LICENSE` and in Servo's
  about:license resource page, both quoting the license text.
- **2026-09-03, P5 LANDED: mora** (`741e18f`). 7 headers added, 0 replaced; 1 manifest to MPL-2.0; single `LICENSE`; no third-party content (the ARPABET table and the CMUdict line splitter are Mark's own, no dictionary data embedded; recorded in the ledger). Audit 7 owned, 0 unheaded, 0 Exhibit B. `cargo check` green. Published 0.1.0 keeps its grant.
- **2026-09-03, P4 LANDED: hocket** (`9a0fe17`). 31 headers added; workspace license to MPL-2.0 (four members inherit); single `LICENSE`; no third-party code in tree (the codec is wavicle, the DSP is woodshed's audio-primitives, the graph is Firewheel, all dependencies). Audit 31 owned, 0 unheaded, 0 Exhibit B. `cargo check` could not run: the gitignored `.cargo/config.toml` points at a deleted Codex worktree, proven pre-existing in a throwaway worktree at HEAD; left alone per the stop line.
- **2026-09-03, P4 LANDED: turnstone** (`ed46e0c`). 122 headers added (116 Rust, 6 PowerShell scenario servers); 1 manifest; single `LICENSE`; no third-party code (fixtures are Mark's, dependencies external). The plan's baseline of 121 was one short. Audit 122 owned, 0 unheaded, 0 Exhibit B. `cargo check --offline` green. Noted for the tool: `.scn`, `.lua` and `.knot` are not in its extension map.
- **2026-09-03, P5 LANDED: wavicle** (`14be097`). 14 headers added (the plan's "4 already headed" was stale: none carried a header); 1 manifest; single `LICENSE`; ledger records nine `src` modules as ports of dbry/WavPack 5.9.0 (BSD-3-Clause) carrying MPL-2.0 with David Bryant's notice retained in their docs, and nothing verbatim to skip. Audit 14 owned, 0 unheaded, 0 Exhibit B. `cargo check --all-features --all-targets` and 17 tests green. Published 0.1.0 keeps its grant. Open for P6: the founding plan still states the July convention in three lines; append a dated note rather than rewrite.
- **2026-09-03, P7 LANDED: mesocosm** (`a1b3a4c, 20151dc`). 259 sources to shape C, 298 lines added and none removed (251 shape-A headers gained the copyright line, 8 unheaded files a full header), plus one file the rebase onto two concurrent commits brought in. No third-party code; `LICENSES.md` unchanged and all four license texts kept, since the ledger names `LICENSE-MPL-2.0`, `LICENSE-MIT` and `LICENSE-APACHE` for a live dual-licensed library scope and P7 forbids touching a LICENSE file: the single-`LICENSE` rename waits for P6 with the promoted-library clause. Audit 259 owned, 0 unheaded, 0 Exhibit B. `cargo check --workspace --all-targets` green. `tracer.wgsl`'s header is valid comment syntax but no run compiles WGSL under `cargo check`.
- **2026-09-03, P7 LANDED: paredros** (`43e14a3, 323f89c`). 65 sources to shape C, 110 lines added and none removed; no third-party code. A `## Retained licenses` table was added to the existing ledger (its own commit, since the tool reads the ledger from the tree before `--apply`) for two of Mark's own scopes held under other grants by recorded rulings: `crates/paredros-identity` *(historical citation)* <!-- doc-audit: historical-path --> (MIT OR Apache-2.0 by the 2026-08-10 R4 extraction review) and `assets` (CC BY-SA 4.0). **Open for Mark:** the identity crate was skipped, not relicensed, because P7 changes headers only and putting MPL headers on files whose manifest says MIT OR Apache-2.0 would relicense it by inference; the brief admits no exceptions, so its grant needs a ruling, and until then paredros keeps four root license texts. Audit 65 owned, 0 unheaded, 0 Exhibit B. `cargo check --workspace` green.
- **2026-09-03, P7 item 4 LANDED: wgpu-graft, ledger only** (`d459c9e`). The plan's premise was wrong: wgpu-graft is not a wgpu fork, its vendored majority is Zed's GPUI (`patches/glass-gpui`, 398 Apache-2.0 files with IBM Plex and Lilex under OFL 1.1 and three Microsoft shims) beside taffy 0.9 (MIT), serde_fmt and yeslogic-fontconfig-sys. Retained fraction 460 of 582 tracked files, 361 of 418 sources (86%), so per the plan's own rule the ledger was committed alone and no header changed. Two derivative dispositions recorded, not acted on: `grafting` and `servo-wgpu-interop-adapter` carry the Slint example's SixtyFPS notice through `NOTICE`, and the three demo `keyutils.rs` are Servo-adapted with an `Original: Mozilla Public License 2.0` provenance line that a later pass must not mistake for Exhibit A. `patches/freetype-sys-compat` is Mark's six-line shim with a stray `MIT` manifest line, recorded. **Open for Mark:** whether to header the 57 owned sources anyway.
- **2026-09-03, P7 item 4 LANDED: wgpu-scry** (`547d089`). 108 headers added (104 Rust, one PowerShell, and `scrying/src/lib.rs` by hand); nothing retained; one derivative row, `scrying/src/native_frame` shaped after the Slint example's rendering context with the notice kept in `NOTICE`. Audit 108 owned, 0 unheaded, 0 Exhibit B. `cargo check -p scrying` green.
- **2026-09-03, P7 item 4 LANDED: wgpu-weld** (`d1f5e83`). 48 headers added (`welding/src/lib.rs` by hand); nothing retained and no derivatives; CEF and grafting are dependencies with no source in tree. Audit 48 owned, 0 unheaded, 0 Exhibit B. `cargo check -p welding` green. Tool finding from these two: a Rust inner attribute `#![...]` on the first line was mistaken for a shebang and the header landed below it; fixed in the tool (`aec0d3e8`).
- **2026-09-03, P7 LANDED: netrender** (`f8c3485`). 108 sources to shape C: 106 of Mark's with the copyright line, 2 WebRender-derived (`renderer/mod.rs`, `renderer/init.rs`) bare, decided by `git log --follow` (with its rename false positives caught), `git blame -w` and the rename commit together; the diff is exactly the header arithmetic (+554/−276, no code line). Ledger: a Servo-heritage section for the two files plus the carried-verbatim `.taskcluster.yml` and `servo-tidy.toml`, and one retained row, `netrender/res`. Audit 108 owned, 0 unheaded, 0 Exhibit B. `cargo check -p paint_list_api -p netrender_device` green. Procedure note: the guard blocked the second of two per-class runs, so the agent ran one whole-repo pass and removed the copyright line from the two derived files by hand, proven byte-identical to the tool's `--bare` output; the guard now scopes itself to `--only`. **Open for Mark:** `netrender/res/Proggy.ttf` and `area-lut.tga` came through the fork with no notice, nothing references them since the vello rasterizer plan, and their terms are unverified: confirm or delete; and whether the two thin derived files should count as his own.
- **2026-09-03, P7 LANDED: netrender, rulings** (`5e8b2b9`). Mark ruled on the two open items. The two WebRender-derived renderer files are his: nothing substantive of Mozilla's survives, so they take the house header with the copyright line and keep one historical attribution line above it (`// Copyright the WebRender authors (Mozilla): derived from webrender/src/renderer.rs under MPL-2.0.`), listed under the ledger's Derivatives; a `--renormalize` pass must carry `--retain-notice` to keep it, proven by dry runs (plain 0, renormalize with retain 0, renormalize alone 2). `netrender/res/Proggy.ttf` and `area-lut.tga` are cruft and were deleted; the Retained table is now empty. Audit 108 owned, 0 unheaded. Mark also asked for a cruft pass over genet and netrender, which is running separately.
- **2026-09-03, P7 item 4 LANDED: wgpu-graft, headers** (`2df70d6`). Ruled: the 57 owned sources take shape C; their derivation from the Slint servo embedding example stays recorded through `NOTICE` and the ledger's derivative rows. 57 headered (53 Rust and WGSL, 4 scripts), the retained table's four paths proven to cover all 460 vendored files, the `freetype-sys-compat` manifest's stray `MIT` corrected. Audit 57 owned, 0 unheaded, 0 Exhibit B; `cargo check -p grafting` green. Tool finding: `already_covered` matched "Mozilla Public" alone, so the three Servo-adapted `keyutils.rs` with an `// Original: Mozilla Public License 2.0` provenance line were skipped as if headed; the mark is now Exhibit A's opening words.
- **2026-09-03, P5 LANDED: gaz** (`none`). The brief's premise no longer holds: `merely-made/gaz` does not exist on GitHub, and no repository containing `gaz` exists under `merely-made` or `mark-ik` (checked with `gh repo view` and `gh repo list` on 2026-09-03). Mark ruled the crate relicensed rather than archived and wants the name kept; the name is his by the naming ledger, and the code is mere's `crates/dramatis/gaz` (Mark, 2026-09-03): 11 files under the workspace's MPL-2.0 with shape C headers since mere's P1, audit clean. Nothing to sweep. If the repository is to exist again, that is a founding, and per the naming ledger a claim is made with a real publish.
- **2026-09-03, P2 hazard LANDED: genet** (`957926e`). **The genet relicense committed half of an active lane's change.** `components/cambium/cambium/src/lib.rs` *(historical citation)* <!-- doc-audit: historical-path --> was swept in carrying the lane's new `pub use pointer::{.., PointerButton, ..}` while `pointer.rs`, which defines the enum, stayed in the working tree with the other held-out files. On a clean checkout genet head therefore fails to compile `cambium` (`E0432` unresolved import), found by mere's repoint against head on 2026-09-03; the lane's own checkout builds because the definition is present there. The hold-out list was taken from `git status` once, before the tool ran, and the sweep's diff was then reviewed as headers only; a file the lane touched between the two moments, or whose lane edit sat beside a header change, was not caught. Rule for the next sweep: after `--apply`, diff every swept file against `HEAD` with the header lines filtered out and require the remainder empty, per file, before committing; a non-empty remainder is the lane's and the file is held out. Repair is Mark's call: the lane commits its `pointer.rs` half, or authorizes committing it.
