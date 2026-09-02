# Terminology and crate folds plan

**Status (2026-09-02):** Landed on `main`. The stack was rebased from its
`9a53c77a` base onto `origin/main` `4d68c465` on 2026-09-02, reconciled with
the Djinn Distillery lane that arrived upstream in between, and every recorded
gate was rerun green before the push. The seven external consumer commits
follow it in their own repositories.

## Scope and rulings

This plan executes the 2026-08-31 vocabulary and ownership pass without
preserving empty crate boundaries:

- Eidetic's immutable typed `Engram` becomes `Codicil`.
- The former generic `codicil::Codicil<T>` append-only log becomes the plain
  `muniment::Journal<T>` storage primitive.
- Scholia's RDF projection folds into `chartulary::rdf`.
- Sonance's implementation remains `mora::sonance`; the standalone repository
  becomes an archive pointer.
- Quint's expression, projection, and lowering machinery belongs to `numen`;
  tensor force laws belong to `seiche`; resident GPU state belongs to
  `conatus::resident`. The `quint` and `quint-shaders` packages leave the
  workspace.
- Tessera becomes Standing. The community-protocol details live in the
  companion FLORA and Tulpa plan.
- The old memorial meaning of the standalone `tulpa` reservation moves to
  `hagiograph`; Tulpa itself now lives inside Gemot.

Historical documents may retain the words they originally used when a dated
supersession note makes their status explicit. Active APIs, manifests, package
descriptions, and UI copy use the current vocabulary.

## Phase 1: Codicil and Journal boundary

Move the generic journal implementation beneath Muniment without changing its
Serde shape. Re-export `Journal`, `Seq`, `LogId`, `Provenance`, and
`CausalError` from Muniment. Remove the old `codicil` package and migrate all
owned consumers.

Rename Eidetic's immutable envelope and module to `Codicil`. Keep a deprecated
source alias for old readers. Preserve content-derived schema and
cryptographic identities where changing descriptive bytes would strand stored
data. Where a wire schema names Engram structurally, introduce a current
Codicil schema and an explicit legacy reader rather than silently changing the
old schema's meaning.

Done conditions:

- Current source compiles against `eidetic::Codicil` and
  `muniment::Journal<T>`.
- The TrainingCorpus v2 writer emits `*_source_codicils`; its reader accepts
  v1 `*_source_engrams`.
- Graphshell accepts both `graphshell.graph-codicil/v2` and the legacy
  graph-engram tag.
- Pandect preserves old sealed payload context bytes and reads the former
  `consolidated_engrams` field.
- Turnstone, Retinue, Isometry, and Mesocosm have focused consumer commits.

## Phase 2: ownership folds

Land the Scholia and Quint moves as history-preserving file moves where useful,
then delete their obsolete package manifests. Keep consumer-facing behavior at
the new owner paths. Sonance's standalone repository must explain that the
live implementation is `mora::sonance` and must not publish another package.

Done conditions:

- `chartulary::rdf` passes the former Scholia projection tests.
- Numen's default and Rhai field paths compile; Seiche owns tensor force laws;
  Conatus exposes resident GPU support behind its feature.
- Canvas, Isometry, and Mesocosm name the new owners.
- Workspace manifests contain no active `scholia`, `quint`,
  `quint-shaders`, or standalone `codicil` package dependency.

## Phase 3: integrated receipt

Run focused format, test, check, and Clippy gates with isolated target
directories and one Cargo job. Audit active source separately from historical
docs and compatibility readers. Record any root-workspace failure at the
actual dependency boundary rather than treating a resolver failure as a code
receipt.

Done conditions:

- Focused owner crates and disposable provider workspaces pass their tests.
- Every compatibility reader has a fixture or regression test.
- `rg` finds old words in active code only where a deprecated API, legacy
  schema, database fallback, or cryptographic context intentionally preserves
  them.
- The integration branch is clean and its commit list identifies each external
  consumer commit needed after the Mere provider lands.

## Findings

- **2026-08-31:** `Codicil` already named two different concepts. Giving the
  ordinary append-only structure the plain name `Journal` removes the collision
  and places persistence mechanics under Muniment.
- **2026-08-31:** several schema references are hashes of payload bytes whose
  descriptions contain the old word *engram*. Rewording those bytes would be a
  data migration, so v1 bytes remain stable and current APIs call them
  codicils.
- **2026-08-31:** the first integrated run could not enter compilation because
  `genet-taffy = =0.13.1` did not match the real `0.14.0` package at the pinned
  Genet revision. Upstream `77b3c3a2` corrected the exact pin to `=0.14.0` and
  was merged into the integration branch. The disposable-workspace runs remain
  useful focused evidence, but are no longer required to bypass this resolver
  failure.
- **2026-08-31:** external consumers pin older Mere revisions. Their Journal
  source migrations can land as ordered commits, but their lockfiles cannot be
  made current until a Mere revision containing `muniment::Journal` exists.

- **2026-09-02:** the 31 upstream commits between the branch base and
  `origin/main` overlapped the stack on eight files, with no source-level
  conflict: two doc-index entries and Distillery's `[features]` block were
  the only merge conflicts, and the upstream `trainer-gpu` feature now sits
  beside the branch's `flora` feature. The one real reconciliation was
  upstream's Djinn trainer receipts, written against `TrainingCorpus` while
  it still spelled its partitions `*_source_engrams`; they compiled only
  after the field rename in `ports/djinn/tests/`.
- **2026-09-02:** the Djinn receipts on windows-msvc link only with
  `mere-canvas` built non-incrementally, the rust-lang/rust#86049 shape the
  [projection grammar plan](2026-08-15_projection_grammar_adoption_plan.md)
  already records; the raw invocation fails with 83 unresolved externals
  even from a cleaned target. Strict Clippy over Djinn's library also carries
  four pre-existing lints in `personal_sync.rs` and `resident_knot.rs` that
  this stack neither introduced nor touched.

## Progress

- **2026-08-31:** implemented the Eidetic Codicil rename, TrainingCorpus v2
  reader/writer, Graphshell legacy import, Pandect persistence aliases, and the
  Muniment Journal move in `962333d1`.
- **2026-08-31:** folded Scholia into Chartulary in `0f160ff0` and Quint into
  Numen, Seiche, and Conatus in `d5d5f9b9`.
- **2026-08-31:** migrated Turnstone (`b079e3f`), Retinue (`3c6ff79`), Isometry
  (`0b17fe9`), and Mesocosm (`cff9b71`) on isolated consumer branches.
- **2026-08-31:** Muniment's 37 tests and Eidetic's 95 default-feature tests
  pass in the disposable provider workspace. Chartulary's 59 tests, Woodshed's
  14 tests, Mora's 37 tests, and the focused owner checks and Clippy gates also
  pass.
- **2026-08-31:** active manifests contain none of the removed `codicil`,
  `scholia`, `quint`, or `quint-shaders` packages. Remaining Engram, Tessera,
  and Scholia spellings are explicit legacy readers, stable schema or crypto
  bytes, deprecated aliases, historical receipts, or fold notes. Sonance is a
  current Mora module.
- **2026-08-31:** the integrated FLORA, Tulpa, and Standing receipt is green.
  After merging `77b3c3a2`, the same receipt entered Cargo and passed directly
  from the integration checkout. `cargo tree -i genet-taffy@0.14.0` resolves
  the Buckram/Livery/Mere consumer chain at the exact pinned Genet revision;
  `cargo check -p mere-canvas -j 1` compiles that chain successfully.
- **2026-09-02:** rebased the nine commits onto `origin/main` `4d68c465`, added
  the Djinn field reconciliation, and reran every gate from the integration
  checkout: integrated receipt 1/1, Distillery library 12/12, Gemot 122/122,
  Muniment 37, Eidetic 95, Chartulary 59, Djinn CPU trainer 2/2, Djinn lane
  4/4, Djinn GPU trainer 1/1, strict package Clippy on Distillery (both the
  recorded `flora` gate and `flora,trainer-gpu` over all tests) and Gemot,
  and the Mere Canvas dependency-chain check. Details in the
  [receipt](../testing/2026-08-31_flora_tulpa_standing_receipt.md). Pushed to
  `main` the same day.
