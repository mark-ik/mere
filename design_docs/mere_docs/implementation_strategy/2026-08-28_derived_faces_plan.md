# Derived Faces Plan

**Date:** 2026-08-28
**Status:** **D1 published as `pictograph` 0.1.0 and D2 landed as unpublished
0.2.0 on 2026-09-01**; D3 (canvas arm) remains open. Was gated on emblem's encoder
(`repos/emblem/design_docs/2026-08-28_encoder_plan.md`), whose E1–E4 all landed and
shipped as emblem 0.2.0 on 2026-08-29.
**Scope:** procedural node faces for content that has no favicon: derive a
compact vector face from the node's content address, deterministic across
peers, themed at render time by the seed palette, with level-of-detail
carried inside the face itself. Editing is explicitly deferred; its shape is
sketched in §7 so v1's decisions do not foreclose it.

## 1. The gap

Canvas's `Face` defaults to `Favicon`
(`crates/canvas/canvas/src/types.rs:127`): a standard tile wears the fetched
favicon, with the bare state color until one arrives. Every node that is not a
webpage — files, contacts, moots, personae, radio peers, documents — has no
favicon source and stays bare forever. The face override machinery
(`set_node_face` / `clear_node_face`, default-plus-override with
clear-to-revert, exercised in `src/tests/node_face.rs`) already has exactly
the semantics a derived face needs; what is missing is the arm and the thing
it shows.

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
  survivable. Depends on emblem (git/path dep) and the kurbo-shaped middle;
  emits palette indices in tinct's slot vocabulary.
- **The sink bridge** (emblem `Sink` → vello scene, via netrender's vello) is
  a small module inside `pictograph` in v1; it moves out only when a second
  consumer forces the wall (module-first doctrine).
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

**Open with Mark before D3 lands:** whether `Derived` becomes the *default*
for favicon-less nodes or an opt-in override — default-behavior change versus
conservative rollout. Both defensible; not decided here.

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

0.2.0 is not published. D3 is still gated on the derived-by-default versus
opt-in decision recorded above.
