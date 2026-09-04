# Licenses in this repository

**This repository: MPL-2.0.** Every file Mark wrote carries Exhibit A and the
SPDX tag `MPL-2.0`, per the
[license posture brief](design_docs/2026-08-22_license_posture_brief.md) of
2026-08-22. The full text is in [`LICENSE`](LICENSE).

This file is the provenance ledger. It is the authority for what the relicense
tool skips: `scripts/relicense_headers.py` reads the backtick-quoted paths
below and never touches them. Provenance comes before license — a file gets
Exhibit A only if Mark wrote it.

## Retained licenses

Third-party code keeps its own license and its own notices. Nothing here is
relicensed, and nothing here receives a Merely copyright line.

| Path | License | Upstream | Notice files |
|---|---|---|---|
| `support/patches/cubecl-runtime` | MIT OR Apache-2.0 | [tracel-ai/cubecl](https://github.com/tracel-ai/cubecl) | `LICENSE-MIT`, `LICENSE-APACHE` in-tree |
| `support/patches/cubecl-wgpu` | MIT OR Apache-2.0 | [tracel-ai/cubecl](https://github.com/tracel-ai/cubecl) | `LICENSE-MIT`, `LICENSE-APACHE` in-tree |
| `support/patches/cubek-reduce` | MIT OR Apache-2.0 | [tracel-ai/cubecl](https://github.com/tracel-ai/cubecl) | `LICENSE-MIT`, `LICENSE-APACHE` in-tree |
| `support/patches/burn-cubecl` | MIT OR Apache-2.0 | [tracel-ai/burn](https://github.com/tracel-ai/burn) | upstream's |
| `support/patches/burn-remote` | MIT OR Apache-2.0 | [tracel-ai/burn](https://github.com/tracel-ai/burn) | upstream's |
| `ports/pelt/examples/resources` | MPL-2.0 | [servo/servo](https://github.com/servo/servo) (`resources/`), by way of genet | Servo's |

`ports/pelt/examples/resources` holds two Servo logo images that Pelt's
example documents reference by URLs that climb to the repository root and
descend again. They came with Pelt from genet on 2026-09-03, sat at the
repository root until later the same day, and are Servo's work under the same
MPL-2.0, so they keep Servo's copyright and take no Merely line; genet's own
ledger carries the identical row. The directory's `README.md` is Mark's and
carries the house header.

384 tracked files. These are vendored patch trees consumed through
`[patch]`; they are upstream's work carrying upstream's terms.

## Derivatives carrying MPL-2.0 with an upstream notice retained

These are **not** skipped. Each file receives Exhibit A and Mark's copyright
line, and every upstream copyright line above it is kept verbatim. Apply with
`--retain-notice`, which preserves foreign copyright lines while replacing
Mark's own.

| Path | Upstream | Notices kept |
|---|---|---|
| `crates/system/luggage` | [cargo-packager-updater](https://github.com/crabnebula-dev/cargo-packager), MIT OR Apache-2.0 | `Copyright 2019-2023 Tauri Programme within The Commons Conservancy`; `Copyright 2023-2023 CrabNebula Ltd.` |

Ruled 2026-08-27, on the brief's substantial-derivative precedent: tucket keeps
MeshCore's MIT notice, and cambium and meristem go MPL-2.0 with the Apache
notice retained. Both MIT and Apache-2.0 permit relicensing a derivative so
long as the notice travels with it. luggage is published (0.1.0, MIT OR
Apache-2.0); that version keeps its grant permanently and MPL-2.0 ships at its
next functional bump, per the sweep plan's invariant 8.

**This section is deliberately not the skip list.** The tool reads only the
`## Retained licenses` table above. Adding a path here documents a disposition;
it does not exempt the path from receiving a header.

## Published grants that arrived with a crate

A published version keeps the grant it shipped with, permanently; MPL-2.0
reaches it at its next functional bump, per the sweep plan's invariant 8. That
rule already governs `crates/system/luggage` above. It governs the
engine-management layer too, which arrived from genet on 2026-09-03 under the
[platform boundary plan](design_docs/mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md)'s
P3, after genet's own 2026-08-27 ruling had already put every source in these
crates under Exhibit A and every manifest on `MPL-2.0`.

| Crate | Path | Published version's grant | In-tree notice files |
|---|---|---|---|
| `inker` | `crates/inker/inker` | 0.1.1, MIT OR Apache-2.0 | — |
| `document-canvas` | `crates/inker/document-canvas` | 0.1.0 | — |
| `verso-tile` | `crates/inker/verso-tile` | 0.1.0, MIT OR Apache-2.0 | `LICENSE-MIT`, `LICENSE-APACHE` |
| `nematic` | `crates/nematic/nematic` | 0.1.1 | — |
| `illume` | `crates/nematic/illume` | 0.0.2 | — |
| `errand` | `crates/system/errand` | 0.3.4, MIT OR Apache-2.0 | — |
| `tinct` | `crates/cambium/tinct` | 0.1.2, MIT OR Apache-2.0 | `LICENSE-MIT`, `LICENSE-APACHE` |

Each arrived from genet 2026-09-03 under its own published grant; relicensing
is the sweep plan's call at the next bump. Nothing here was relicensed on the
move, and the four `LICENSE-MIT` / `LICENSE-APACHE` files that `verso-tile` and
`tinct` carry are **not** to be deleted: they are the notice text for versions
already on crates.io. This is the same disposition genet's own ledger records,
which names `sprigging` 0.2.1, `illume` 0.0.2, `errand` 0.3.4, `tinct` 0.1.2
and `inker` 0.1.1 explicitly and keeps the ruling as the precedent the sweep was
decided on.

The three engine adapters — `scrying-engine`, `graft-engine` and `weld-engine`
— are `publish = false` and have no published version, so nothing is held open
for them.

This section, like the one above it, is **not** the skip list. Every source in
these crates carries Exhibit A already; the audit reports 0 without it.

## Exceptions under the fork/vendor criterion

**None.** The brief's §4 test — a crate stays MIT OR Apache-2.0 only when a
third party would need to *modify or vendor* it rather than merely link it —
admits nothing in this repository. `illume`, `buckram`, `errand` and `tinct`
were each proposed and declined on 2026-08-22.

If one is ever granted, its manifest says `MIT OR Apache-2.0` explicitly with a
comment naming the brief, and it is listed here. `illume`, `errand` and `tinct`
now live in this repository; the decline stands, and what they do carry is the
published-grant disposition in the section above, which is a different thing.

## How to add a file from elsewhere

1. Do not delete or rewrite the upstream copyright or license notice, ever.
2. Add its path to **Retained licenses** above with its license, upstream URL,
   and where its notice text lives. The tool then skips it automatically.
3. If it is a substantial derivative rather than a verbatim import, the brief's
   rule is MPL-2.0 on the derivative *with the upstream notice retained* —
   record it here with that disposition so the distinction is not lost.
4. Never add `license-file` to an owned manifest; the field is for retained
   third-party crates only.
5. Re-run `python scripts/relicense_headers.py --audit` and confirm the owned
   source count moved by exactly what you expected.

## A note on the discovery grep

The sweep plan's invariant 1 lists `Copyright (c)` among its discovery
patterns. That form is not universal: `crates/system/luggage` writes bare
`Copyright 2019-2023 <holder>` with no parenthesised `(c)`, and was missed by
the plan's own pattern set on 2026-08-27. Discovery should grep for
`Copyright` unqualified, then read the hits.
