# License Sweep Plan

**Date:** 2026-08-22
**Status:** planned; P0 not started. Both P0 confirmations are settled
(2026-08-22): header shape C and the copyright notice `Mark Alan Boykin`, with
no exceptions to the default. The remaining gate is per-repository: a clean
tree, and for mere the retired `genet-layout` patch entry.
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
- `cargo package --list -p personae` includes `LICENSE`;
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
  `LICENSES.md` under `components/cambium/` reworked so the xilem Apache-2.0
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
  a concurrent scenograph absorption (`crates/canvas/arrangements` deleted,
  `scenomise/families/` added, plus a Turnstone census and a muniment OPFS
  probe), so invariant 7 blocks P1 regardless. The §6 counts are as-measured
  on 2026-08-22 and will drift when that lane lands; re-run `--audit` before
  P1 rather than trusting them.
