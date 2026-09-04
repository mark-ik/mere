# Derived Faces Plan

**Date:** 2026-08-28
**Status:** **D1 published as `pictograph` 0.1.0, D2 landed as unpublished
0.2.0 on 2026-09-01, D3 landed on 2026-09-02, and D4 default-scale
legibility landed on 2026-09-03.** Editing remains deferred.
The original gate was emblem's encoder
(`repos/emblem/design_docs/2026-08-28_encoder_plan.md`), whose E1–E4 all landed
and shipped as emblem 0.2.0 on 2026-08-29.
**Scope:** procedural node faces for content that has no favicon: derive a
compact vector face from the node's content address, deterministic across
peers, themed at render time by the seed palette, with level-of-detail
carried inside the face itself. Editing is explicitly deferred; its shape is
sketched in §7 so v1's decisions do not foreclose it.

## 1. The gap

Before D3, Canvas's `Face` defaulted to `Favicon`
(`crates/canvas/canvas/src/types.rs:127`): a standard tile wears the fetched
favicon, with the bare state color until one arrives. Every node that is not a
webpage — files, contacts, moots, personae, radio peers, documents — has no
favicon source and stayed bare forever. The face override machinery
(`set_node_face` / `clear_node_face`, default-plus-override with
clear-to-revert, exercised in `src/tests/node_face.rs`) already has exactly
the semantics a derived face needs. The D3 gap was the arm and the thing it
showed.

## 2. The model: derive by default, store only the exception

- A **derived face** is computed from the node's content address:
  hash → seeded parameters → motif grammar → declarative geometry
  (paths + palette-indexed fills) → `emblem::encode` → `.ivg` bytes.
  Deterministic, so **every peer derives the same face for the same content
  without shipping an asset**; a face that travels is a ~hundreds-of-bytes
  blob. Derived faces need no storage: recompute on demand, cache by
  (address, derivation version).
- **Theming happens at decode time, not derivation time.** `emblem::execute`
  takes the palette as an argument; the derivation emits palette *indices*,
  and the render path supplies the palette derived from the current theme seed
  (tinct, wired here as the `tincture` alias). One blob, every theme. This is
  the reason to use palette-indexed fills exclusively in the generator — a
  literal-color face is a bug by construction in v1.
- **LOD rides inside the face.** The generator emits two arms via the
  encoder's level-of-detail branch: a bold simple glyph far out, the full
  figure near. The canvas zooms constantly; the face simplifies itself with no
  render-path involvement.
- A **user-edited face is a stored override** — the one arm that persists.
  Deferred (§7), but the storage model is fixed now: an override is an `.ivg`
  blob that wins over derivation, content-addressed like anything else, and
  `clear_node_face` reverts to derivation. This is the existing override
  contract with bytes attached.

## 3. Prior art

Identicon lineage for the hash side (GitHub identicons, jdenticon, blockies,
DiceBear); Chernoff faces and the scientific-visualization glyph tradition for
rule-driven faces (data as visual features — respecting Chernoff's known
weakness, perceptually uneven feature salience, when mapping rules to
features). The combination — deterministic derivation, a compact themeable
interchange format, user-editable overrides — appears to be novel; no found
prior system re-themes identicons at decode time or lets a user edit one into
a keepsake.

## 4. Where the pieces live

- **emblem stays format-pure**: decoder + encoder, no generator, no rules.
- **The generator is `pictograph`** (`crates/canvas/pictograph`), named by
  Mark 2026-08-28 and claimed the same day as a 0.0.1 reservation on
  crates.io (MPL-2.0 — the first crate founded after the license sweep).
  The word carries the mechanism twice: a pictorial sign bearing meaning by
  convention, and the statistical sense — data encoded as a picture, which is
  what rule-driven faces are. It joins the `-graph` vein beside `chirograph`.
  Verified free on both API and sparse index; near-name `pictogram` is taken
  by a compile-time SVG icon resolver (same vertical), flagged and judged
  survivable. Depends on registry emblem 0.2.0 and emits palette indices in
  tinct's slot vocabulary.
- **The sink bridge** (emblem `Sink` → vello scene, via netrender's vello) is
  a small module inside `pictograph` in v1; it moves out only when a second
  consumer forces the wall (module-first doctrine).
- **Canvas keeps its own portable sink** because its public frame boundary is
  `netrender::Scene`, not a raw vello scene. It lowers emblem's generated flat
  fills into `PaintCmd::DrawPath`; netrender applies non-zero winding. The D2
  bridge remains the exact adapter for direct vello consumers.
- **Canvas grows the `Derived` face arm**; rule-driven variation (by node
  kind, state, degree) is parameters into the same generator, upstream of it.

## 5. Determinism and versioning

Same address → same bytes, across machines and peers — this inherits emblem's
byte-determinism requirement and adds one of its own: the generator carries a
`DERIVATION_VERSION`; changing the grammar or parameter mapping bumps it, and
a bump is a deliberate, dated decision (every face in every session changes
appearance). Digest-pinned fixtures pin the mapping: a committed table of
(address, byte length, FNV-1a digest, filled-cell count) values that CI compares
against newly derived faces.

## 6. Phases

### D1. `pictograph` 0.1.0: hash → face bytes

Seeded parameter extraction, a first motif grammar (symmetry, density, a small
family of forms), palette-indexed fills only, two LOD arms, emblem encoding.

Done when:
- digest-pinned fixtures pass: committed address, byte-length, digest, and
  filled-cell values reproduce exactly, with repeat derivation checked
  separately in-process;
- decoding a derived face against two different palettes yields different
  resolved paints from identical bytes (theming proof);
- decoding under two host LOD values yields the two arms (LOD proof);
- a fuzz pass over random addresses produces decodable faces with no panics.

### D2. The vello bridge (`pictograph` 0.2.0)

`Sink` implementation building a vello scene fragment; non-zero winding rule
honored (the one rule emblem's docs call out — vello must not default this
fill to even-odd).

Done when the spec's action/info icon and a sample of derived faces render
via netrender's vello path in a headless harness, and a face re-renders with
new colors after a palette swap without re-derivation.

### D3. The canvas arm

`Face::Derived` wired: shown for nodes with no favicon source, participating
in the existing default/override/clear contract unchanged.

Done when:
- the `node_face` test family extends to the new arm and passes;
- a non-web node in the real app wears a derived face, themed by the live
  seed palette, verified through the headed harness
  (`Code/testing/`, genet-probe self-drive) with a screenshot;
- `clear_node_face` and `set_node_face` behave identically to today for the
  existing arms (no regression in the override contract).

**Decision, Mark, 2026-09-02:** `Derived` is the content-sensitive default for
favicon-less nodes. An explicit `Favicon`, `Derived`, `Sprite`, or `Bare`
override wins. Clearing the override re-evaluates the node: it returns to
`Favicon` when a favicon source exists and `Derived` otherwise.

### D4. Default-scale legibility

The first D3 headed receipt exposed a real product failure in the small LOD
arm: reducing the detailed cells to their bounding rectangle made unrelated
addresses read as the same coloured block at Canvas's normal 25.92px face
height. D4 replaces that rectangle with a 3x3 majority reduction of the 5x5
figure. Each coarse cell is 20 graphic units, or 8.1 physical pixels at the
normal face height, so the mark remains bold while retaining address-specific
geometry.

Done when:
- the fixed 68-address corpus is decoded and rasterized at `Host.height =
  25.92` with one monochrome palette, so colour cannot disguise geometric
  collisions;
- the old arm's measured 7 masks / largest multiplicity 57 improves to at
  least 30 masks / largest multiplicity at most 8, with the bound enforced by
  a test that reads the actual decoded low-detail arm;
- the grammar change deliberately bumps `DERIVATION_VERSION` and replaces the
  digest-pinned fixtures;
- the standalone Graphshell wasm package builds from committed pins and the
  headed default/detail scenario passes against the new bytes.

## 7. Deferred: editing (recorded so v1 does not foreclose it)

- **Tier 1 — parameter editing:** palette-slot recolor, generator family and
  seed nudging, symmetry/density knobs; re-derive and store the result as an
  override blob. No path editor. This is most of the user value.
- **Tier 2 — vector editing** on the declarative middle, only if a surface
  demands it.
- The constraint that shapes both, found in emblem 2026-08-28: decode resolves
  palette indices to concrete colors before the sink sees them, so
  decode → edit → re-encode silently loses themeability. **Editors operate on
  the pre-encode source (parameters or the declarative middle), never on
  decoded bytes.** v1 keeps this door open by making the parameter set — not
  the bytes — the canonical description of a derived face.

## 8. Findings

- 2026-08-28: `emblem::execute` takes `&Palette` at decode time
  (`repos/emblem/src/vm.rs:74`); `Paint` reaches the sink resolved
  (`repos/emblem/src/sink.rs:99`). Theming = re-decode; editing must stay
  upstream of encoding.
- 2026-08-28: tinct is wired into mere as the `tincture` alias
  (root `Cargo.toml`, dependency table), so the seed-palette loop needs no
  new dependency.
- 2026-08-28: the apps are essentially unbranded — turnstone, woodshed and
  signalman ship no icon; hocket ships one `icon.png`; the only `.ico`/`.icns`
  in the lattice are Servo's, still in `repos/genet/resources/` from the
  fork. App icons are raster packaging assets and are *not* this plan's
  scope, but the branding gap is recorded here so it is not lost.
- 2026-09-01: review found that rotate-180 sampled the whole middle row, so two
  mirrored cells were drawn again and overwrote their first values. The output
  remained symmetric, but the claim that the stream sampled only an independent
  region was false. The generator now samples one row-major representative per
  symmetry orbit: 15 cells for mirror-X, 9 for mirror-both, and 13 for
  rotate-180. `DERIVATION_VERSION` deliberately moved from 1 to 2. The
  mirror-both test now proves horizontal and vertical symmetry independently,
  and fixture wording now says digest-pinned rather than byte-exact.
- 2026-09-01: netrender 0.1.2 consumes the published `netrender-vello` 0.10.0
  package, while Mere's otherwise-unused root `vello` pin names a distinct
  package. D2 targets `netrender-vello` directly so its `Scene` has the identity
  netrender's compositor expects. The dependency remains behind a `vello`
  feature so byte-only derivation keeps its D1 dependency surface.
- 2026-09-01: emblem hands sinks premultiplied RGBA while peniko accepts
  straight-alpha colours and performs premultiplied gradient interpolation.
  The bridge therefore unpremultiplies each resolved colour at that boundary.
  IconVG linear gradients carry a one-row effective matrix, so D2 constructs an
  invertible brush basis whose first coordinate is that linear function;
  radial gradients use the inverse effective matrix directly.

## 9. Progress

- 2026-08-28: plan written; scope ruled with Mark (encoder + derivation,
  editing deferred; doc split — encoder plan in emblem, this plan here).
- 2026-08-28: generator named **pictograph** (Mark's pick) and claimed as a
  0.0.1 stub on crates.io the same day; workspace wiring committed
  (`42731a59`). The plan's `face-derive` working name is retired.
- 2026-09-01: the pre-publication review correction moved the mapping to
  derivation v2, replaced the four fixture receipts, added the exact independent
  region counts as a regression test, and brought the focused suite to 14 tests.
- 2026-09-01: `pictograph` 0.1.0 published on crates.io from corrected Mere
  commit `1e59c0b7c930b78046fd62896a4b5fa8e6e9f120`.
- 2026-09-01: D2 added the feature-gated vello bridge as unpublished
  `pictograph` 0.2.0. The feature suite passes 21 unit tests plus two live
  headless pixel tests through netrender's wgpu-30 vello package.
- 2026-09-02: Mark chose derived-by-default for favicon-less nodes. D3 added
  the `Face::Derived` arm, address/version byte cache, live seed-derived
  palette, portable Canvas sink, web representation option, and headed
  default/detail receipts.

### D1 receipt (2026-08-29)

`pictograph` 0.1.0 derives a face from a content address, landed in Mere commit
`d42fd1fbc81ba1ae6a3548c4e5f4e5759e7fa64f`. 13 tests green, clippy clean (the
only warnings under `-p pictograph` are mere's pre-existing unused-patch
notices). Its four pinned fixtures measure **91 to 179 bytes**; that receipt did
not measure the whole address corpus.

**The grammar, v1:** a 5x5 grid over the default ViewBox, cell side 12 from
origin -30 — chosen so every coordinate is an integer in `[-30, 30]` and
therefore encodes in **one byte**. Three symmetries (mirror-X, mirror-both,
rotate-180), two forms (block, diamond), three densities, and two distinct
palette entries per face. Only the independent region is drawn from the
stream; symmetry supplies the rest, which is what makes a face read as
designed rather than noisy. Each cell is a single parallelogram op, and each
colour group is one path with one fill.

**Pre-publication correction, derivation v2 (2026-09-01):** rotate-180 now
samples exactly one cell from each of its 13 symmetry orbits rather than 15
cells with two center-row overwrites. The version bump was intentional because
the parameter mapping changed. The 68-address deterministic corpus spans **34
to 211 bytes**; the four pinned fixtures span **51 to 187 bytes**. The suite is
now 14 tests. Focused test, all-target Clippy with warnings denied, rustdoc with
warnings denied, and package verification all pass.

**The theming mechanism turned out cleaner than the plan assumed.** The plan
expected register writes carrying palette indices. In fact IconVG pre-loads
registers from the caller's palette, and at the starting `SEL` of 56 slot
`8 + k` lands on entry `k` — so a face names palette entries with **no
register ops in the file at all**. Fills are palette-indexed by construction;
a literal colour is not merely discouraged here, it is unreachable.

**Determinism:** FNV-1a over the address seeds a SplitMix64 stream, both
exactly specified, with integer arithmetic throughout and no hash-map
iteration. `DERIVATION_VERSION` is folded into the seed.

Done-conditions met:
- **digest-pinned fixtures pass** — committed (address, length, digest,
  filled-cell) literals for four addresses reproduce exactly, while a separate
  test proves repeat derivation is stable in-process;
- **theming proved** — one face's identical bytes decoded against two palettes
  give two colours, and a sweep asserts that *every* fill across the corpus at
  both LOD heights resolves to a palette entry, never a literal;
- **LOD proved** — the small arm is exactly one shape and one fill for every
  address, the arms differ at the threshold, and most faces genuinely simplify;
- **fuzz** — 300 random addresses decode without panic at four heights.

**A note on the fixture test, which was wrong first.** The initial version
recomputed both sides of the comparison, so it would have passed no matter how
the grammar changed — the same non-discriminating shape as the emblem test
that let the SEL bug through. It now carries committed literals, and its doc
comment says that a failure means bumping `DERIVATION_VERSION` deliberately,
never quietly updating the table. A companion test asserts the grammar does
not collapse: all three symmetries, both forms, and at least six distinct
primary colours must occur across the corpus.

**Published 2026-09-01.** 0.1.0 is the first implementation release.

### D2 receipt (2026-09-01)

Unpublished `pictograph` 0.2.0 adds an optional `vello` feature. `VelloSink`
maps emblem's move, line, quadratic, cubic, close, and fill calls into a kurbo
`BezPath` and the exact `netrender-vello` `Scene` type. Every fill explicitly
uses non-zero winding. `decode` returns a `Graphic` carrying both the clipped
scene fragment and its IconVG ViewBox; callers append it to their frame scene
with a placement transform.

The paint boundary is complete for emblem's current vocabulary: flat fills;
linear and radial gradients; Pad, Reflect, Repeat, and None spread. None uses a
gradient-domain clip because peniko has no transparent-outside extend mode.
Resolved premultiplied RGBA is converted to peniko's straight-alpha input while
gradient interpolation stays premultiplied. A radial transform vello cannot
represent returns a typed error rather than producing invalid scene data.

Done-conditions met:
- **real icons reach pixels** — the specification's 36-byte action/info icon
  and a derived face render through `netrender-vello` on a live headless wgpu
  adapter;
- **theme swap reaches pixels** — one derived byte vector is decoded against
  all-red and all-blue palettes, producing visibly red and blue buffers without
  re-derivation;
- **winding is proved at the raster boundary** — two same-direction nested
  contours fill the centre pixel, the result that distinguishes non-zero from
  even-odd;
- **gradient direction reaches pixels** — known linear and radial effective
  matrices put their start and end colours on the expected sides and radii;
- **structural coverage** — ViewBox clipping, every path segment, all gradient
  spreads, radial inversion, palette brush changes, premultiplied colour
  conversion, and the explicit singular-gradient error are covered;
- **gates** — 21 unit tests and two headless integration tests pass with the
  `vello` feature; all-target Clippy and rustdoc pass with warnings denied. The
  extracted `.crate` repeats all 23 tests using registry dependencies only.

0.2.0 is not published. Canvas uses the portable sink described above; D2
remains available to consumers whose boundary is the exact vello scene type.

### D3 receipt (2026-09-02)

Mere commit `be007322687505a43d8004e5f50816cffd79c8c3` adds
`Face::Derived` and makes it the effective default only while a node lacks a
favicon source. Explicit overrides still round-trip as string codes in the
cartography sidecar. `clear_node_face` removes the override and re-evaluates
the current content-sensitive default; clearing a sprite does the same when
the active override was `Sprite`.

The byte cache is keyed by `(pictograph::DERIVATION_VERSION, canonical
address)`. `DerivedFacePalette` is separate live Canvas state built from
`tincture::Seeds`; a palette swap re-decodes the cached bytes rather than
re-deriving or storing a second face. The Canvas-private emblem sink maps move,
line, quadratic, cubic, close, and flat fill operations into portable
paint-list paths. Its host height is the face's actual raster height, so zoom
selects the LOD arm embedded in the IconVG file.

Done-conditions met:
- **default and override contract**: the `node_face` family proves source-less
  `Derived`, sourced `Favicon`, all four explicit arms, clear-to-re-evaluate,
  sprite clearing, and cartography round-trips;
- **live theming**: one cached byte vector renders red, then blue after a live
  palette replacement while the cache remains byte-identical;
- **portable rendering**: generated paths enter the ordinary
  `netrender::Scene` as `SceneShape`s, while favicon images keep their prior
  image path;
- **gates**: all 184 `mere-canvas` library tests pass; all-target Clippy exits
  successfully with only the existing workspace warnings; the standalone
  Graphshell wasm target checks successfully against the committed pins;
- **headed receipt**: the page-side genet-probe scenario reports `RESULT ok`
  over 16 steps and 24 rendered frames, with `product.face = "derived"`, zero
  scenario errors, and two 1280×720 captures. The first records the default
  1.0× LOD; three zoom commands reach 1.521× and expose the detailed 5×5
  address motifs. Artifacts live under
  `Code/testing/mere/scenarios/graphshell-web/d3_derived_faces_detail_be007322/`
  (`scenario.done`, `result.json`, `d3_default.png`, `d3_detail.png`).
  The exact script is committed at
  `ports/graphshell/web/scenarios/d3_derived_faces_detail.scn`.

The headed composition used the committed page-scenario lane at `dc9dd8ca`
plus the D3 commit in a disposable worktree. That lane calls Genet text APIs
newer than its committed `eff0cb6d` manifest pin, so the receipt supplied the
clean Genet `fcb6c8fb` packages through a disposable Cargo patch while keeping
netrender at Mere's pinned `6f1a4fe7`. The production D3 commit and shared
checkout contain none of that compatibility patch.

The 2026-09-03 platform-boundary landing subsequently moved the remaining
Genet family to the exact `115d348deddc344d949754e63beaece47cf49f34`
revision across the root and standalone Graphshell manifests. D4's clean
standalone build uses that committed pin directly; the D3 compatibility patch
is historical and remains absent.

### D4 receipt (2026-09-03)

Mere commit `81c86f29` changes only pictograph's low-detail grammar and its
documentation. `DERIVATION_VERSION` deliberately moves from 2 to 3, so peers
cannot silently disagree about either arm; the four digest-pinned fixtures now
record v3 lengths, digests, and filled-cell counts. The 68-address byte corpus
spans 34--275 bytes, below the existing 512-byte cap.

The fixed corpus at Canvas's ordinary 25.92px face height now yields 30
monochrome decoded and raster masks, with largest multiplicity 8. The D3
bounding-box arm yielded 7 masks, with 57 of 68 addresses sharing one shape.
The committed test reconstructs the decoded 3x3 geometry after IconVG's LOD
selection and enforces the new 30 / 8 bounds; a separate headless vello receipt
renders the same corpus with an all-black palette and Area antialiasing.

Done-conditions met:
- **generator gates:** all 15 pictograph library tests pass; strict library
  Clippy and formatting pass;
- **consumer gate:** standalone `graphshell-web` builds for
  `wasm32-unknown-unknown` from the committed Genet `115d348d` and netrender
  `6f1a4fe7` pins, without a Cargo patch;
- **raster receipt:**
  `Code/testing/mere/derived-faces/d4_81c86f29/` contains the 768x864,
  68-face monochrome contact sheet and its per-address JSON summary;
- **headed receipt:** the existing 16-step scenario reports `RESULT ok`, 24
  frames, two captures, `product.face = "derived"`, and zero scenario errors.
  At 1.0x the visible faces retain bars, rings, crosses, corners, and forks;
  the 1.521x capture still selects the detailed 5x5 arm. Artifacts live under
  `Code/testing/mere/scenarios/graphshell-web/d4_small_lod_81c86f29/`.
