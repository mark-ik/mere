# Derived Faces Plan

**Date:** 2026-08-28
**Status:** plan. Gated on emblem's encoder
(`repos/emblem/design_docs/2026-08-28_encoder_plan.md`, E1–E3 at minimum;
E4 for the LOD arm).
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
appearance). Golden byte-vector fixtures pin the mapping: a committed table of
(address → expected `.ivg` bytes) that CI compares byte-for-byte.

## 6. Phases

### D1. `pictograph` v1: hash → face bytes

Seeded parameter extraction, a first motif grammar (symmetry, density, a small
family of forms), palette-indexed fills only, two LOD arms, emblem encoding.

Done when:
- golden fixtures pass: the committed (address → bytes) table reproduces
  byte-exactly, twice in-process;
- decoding a derived face against two different palettes yields different
  resolved paints from identical bytes (theming proof);
- decoding under two host LOD values yields the two arms (LOD proof);
- a fuzz pass over random addresses produces decodable faces with no panics.

### D2. The vello bridge

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

## 9. Progress

- 2026-08-28: plan written; scope ruled with Mark (encoder + derivation,
  editing deferred; doc split — encoder plan in emblem, this plan here).
- 2026-08-28: generator named **pictograph** (Mark's pick) and claimed as a
  0.0.1 stub on crates.io the same day; workspace wiring committed
  (`42731a59`). The plan's `face-derive` working name is retired.
