# Scenograph Lane Handoffs

**Date**: 2026-08-18
**Status (reconciled 2026-08-19)**: dispatch document, written at Mark's request after a review of
what is deferred, forgotten, or open across the scenograph family and the
threads this session opened. The [expansion brief](2026-08-10_scenograph_expansion_brief.md)
is the map and the [content catalog](2026-08-18_scenograph_content_catalog.md)
is the material; this is the dispatch: verified entry state as of today,
per-lane first acts, and the decision each lane waits on. Every lane keeps
the family standard: a question is answered by the consumer that forces it,
and unforced surface does not ship.

**Related**: the scene contract note
(`design_docs/scenograph_docs/technical_architecture/2026-07-22_scene_contract_note.md`),
[projection_proofs_plan](../implementation_strategy/2026-07-21_projection_proofs_plan.md),
[swatch_primitive_design](../design/2026-06-27_swatch_primitive_design.md),
woodshed's `design_docs/2026-07-11_stage_set_tools_plan.md`.

## Verified entry state (checked 2026-08-18, not inherited from docs)

- `sceno` / `scenomise` / `scenotime` / `scenograph` have moved from the
  published **0.0.3** baseline to the unpublished **0.0.4** development line.
  Score v3 names `Grid` and carries placement holds.
- `sceno::measure` is **gone** from the tree, so the release note's "does
  `measure` earn its place" question is closed by deletion. What remains is
  `footprint`, `geometry`, `scene`, `score`.
- **isometry and turnstone were never actually unresolved.** Both lock
  `sceno` 0.0.3, `cargo update -p sceno` moves zero packages, and both
  compile green: a git dep on branch `main` followed the release unattended,
  so the loose end the release plan recorded had closed itself.
- **woodshed depends on no scenograph crate at all**, so L1 is a founding.
- `mora` is consumed by Knot's landed Rosette scene through `mora-cmudict` and
  `mora.perfect-rhyme`.
- Display names live in **two** tables, `arrangements::registry` and the
  canvas's `CANVAS_LAYOUT_STRATEGIES`, and nothing enforces agreement.

## Rulings (Mark, 2026-08-18)

Listed once here so each lane can point at them rather than re-deriving.

1. **Plotted is ratified**, with the mechanism stated narrowly: each item
   supplies an already-projected planar coordinate through its adapter, and
   one shared frame transform maps those coordinates into scene space while
   preserving their relative geometry. Raw latitude/longitude still require
   adapter-owned map projection. Physics is orthogonal to arrangement, so
   "never force-settled" belongs to Atlas's scene template rather than the
   Plotted definition. The eventual portable implementation replaces or
   generalizes Geographic and Semantic placement; it does not join them as a
   third duplicate, and waits for the forcing consumer before changing the
   portable enum.
2. **The scene register is a sequence, not one of three rival homes.** It
   stays catalog-only until the second real scene ships. At that point a
   Mere-side complete-template surface absorbs the dormant `register-lens`
   preset role rather than creating another authority beside it. Built-in
   templates live there, persona settings own saved variants, and active
   per-view state stays in `ViewIntent`. Scene recipes do not live in
   `arrangements`.
3. **Mora gets a first-party English default outside its core.** `mora`
   continues to accept pronunciations and stays language-neutral, `no_std`,
   and zero-dependency. A companion CMUdict data/provider boundary supplies
   Knot's offline default while keeping custom dictionaries and writer
   overrides possible. Unknown words are reported rather than guessed.

---

## Lane A — Woodshed Stage founding (the release gate)

**Repo**: woodshed. **Brief lane**: L1. **Gate**: the data half is
**founded 2026-08-18**; the rendering half remains.

> **Taken 2026-08-18.** `StageGraphSnapshot` exists at
> `crates/woodshed-core/src/stage_scene.rs`. The lane was adapter-only,
> because the entry state below was wrong in woodshed's favour: `CardId`,
> `Set::graph()`, and a 16-kind relation engine (`relations_between` returns
> *every* reason for a pair) were already landed, so no relation had to be
> derived. What remains of this lane is the **rendering** half — a host that
> draws and hit-tests fanned cells rather than one line per pair — plus
> retiring `related_swatch` onto `scenomise::relax` and the fretboard `Space`
> so picking resolves through `scenotime`. A new boundary was recorded: the
> relations lifted are formula-level and key-agnostic, so keyed relations
> (diatonic in, dominant of, resolves to) are their own slice, and the Stage
> projection is the layer that owns them.

Woodshed takes its first `sceno` dependency and projects the Set as a graph
through the `scene_out` shape: each staged Card occurrence a numbered node,
Set order a typed `Next` edge, and one `RoutedRelation` per *reason*
(diatonic, shared-tone, voice-leading, practiced-after) per the cells-as-edges
ruling, rather than one collapsed line per pair. Expansion state rides the
projection; Card edits land on the one Set.

**Entry state**: `woodshed-graph` already projects the theory catalog into
the chartulary graph, which is the Set's graph form and therefore the
projection source. The stage/set/tools plan carries a 2026-08-10
gate-is-open banner. No `sceno` dep exists in any woodshed crate.

**First acts**: add the dep to whichever crate owns Set state (per woodshed's
CLAUDE.md layering that is `woodshed-core`, portable state, not
`woodshed-views`); write the Set-to-scene adapter beside it; keep musical
facts and typed relations woodshed-side and let `sceno` own scene identity,
placement facts, and routing. Selection remains Chirograph/ViewIntent state.

**Done when** Stage renders a Set graph through the portable contract with
fanned relations, before woodshed's release.

**Pitfalls**: woodshed's CLAUDE.md forbids adding beyond the active plan's
feature target without surfacing the scope change, and forbids re-adding
direct genet-layout / netrender deps; keep `woodshedding` and
`audio-primitives` pure. This lane also founds the **Loom** scene's dataset
(parallel tracks on a shared axis: Set order beside practice state), so name
it that way when it appears.

## Lane B — Backdrops

**Repos**: mere (contract), isometry and woodshed (the two forcing
consumers). **Brief lane**: L2. **Gate**: opens the moment either consumer
touches environment work.

The missing tier. Nothing behind content can cross the wire today, because
the contract has no place for it, and every host paints its own background ad
hoc. Graphshell remote is the discipline: a backdrop must be scene data, or a
remote viewer sees content floating on nothing.

**Questions the prototype must answer**: backdrop as an item kind versus a
separate table; layering (always behind, or interleaved); whether `Region`
generalizes or a new type is forced; hit transparency (backdrops mostly never
pick, except a VTT map, which does, and that is what `hit` overrides exist
for).

**First acts**: prototype against isometry's map and woodshed's stage floor
at the same time, not one and then the other. Ship whichever shape *both*
force; ship nothing either fails to force.

**Done when** a backdrop crosses the wire and a remote viewer renders content
over it with no source access.

**What it unlocks**: the **Tabletop** and **Atlas** scenes both need this
tier before they are more than an arrangement. There is no Atlas without a
map, which is the observation that produced the scene register.

## Lane C — Rosette for text (mora's first consumer)

**Repos**: [`knot-editor`](https://github.com/merely-made/knot-editor), mere,
mora. **Gate**: open, and this
lane carries the session's one piece of unforced surface.

`mora` 0.1.0 is published and consumed by nothing. The founding convention
says a crate earns its next publish when it *functions and is wired into at
least one consumer*, so this lane is what makes 0.1.0 honest rather than a
name reservation with code attached.

The scene: a document's interior projected as a wheel. Lines and stanzas are
stations on the meter's own cycle; chords across the disc are recurrence —
rhyme, assonance, alliteration, repeated lemmas, semantic kinship — with
chord *span* carrying meaning (an end-rhyme spans a stanza; alliteration is
the flurry of short adjacent arcs). A tight villanelle and a loose free-verse
poem look different before you read a word.

**Entry state**: `mora` supplies the sound half (syllables, weight, meter,
sonance) and needs pronunciations supplied to it; `esp` owns the semantic
half already; the scene contract needs nothing new, because `SourceRef` is
opaque, so a knot adapter can emit document interiors as projected items.

**First acts**: the lexicon ruling above opens a read-only C0 proof. Write a
pure knot adapter with an injected pronunciation lookup, emit stanzas and
lines as items, and derive one live sound relation family before endpoint
selection or semantic chords. Exercise the same adapter against a poem and a
song lyric or speech. The existing reveal-then-crystallize path only reveals
relations already present in graph truth and crystallizes a graphlet; it does
not promote analyzer suggestions into annotation. C0 therefore writes no
truth. A later writer-kept-rhyme slice must design that annotation intent
explicitly rather than claiming the graphlet path already supplies it.

**C0 receipt:** both datasets produce deterministic serializable scenes,
each has line items, at least one perfect-rhyme chord is derived live through
Mora, unknown-token coverage is explicit, and no scenograph contract changes.
The full lane is done when Knot can select and render that Rosette and the
second dataset still reads true. That second application is the receipt that
Rosette is a scene rather than a one-off view.

**C0 landed 2026-08-18.** `mora-cmudict` is the first-party offline provider;
Knot's pure `rosette` adapter emits line and stanza items, derives
`mora.perfect-rhyme` chords, reports unresolved tokens with source byte spans,
and serializes deterministically for poem and lyric fixtures. The adapter does
not write graph truth and no scenograph crate changed. Knot scene selection
and headed rendering were the remaining full-lane boundary.

**Lane C completed 2026-08-18.** Every eligible UTF-8 Knot text document now
advertises a stable document-scoped Rosette session beside Knot's ordinary
projection. Graphshell mounts poem and lyric sessions simultaneously through
the serializing local carrier, resolves their line/stanza presentations, and
renders both through its headed HTML scene path with live rhyme chords. Source
size and geometry are host-configurable; source removal replaces the mounted
Rosette with an empty current snapshot and drops its resources rather than
leaving stale text visible. Scenograph remains unchanged. Writer-kept analysis
still waits on an explicit annotation intent and is not smuggled into this
read-only scene lane.

**Adjacent, deliberately out of scope**: a morpheme and etymology analyzer is
a *different unit* (morpheme, not mora) and a sibling lane, not scope creep
into this one. `kenning` is banked free for it. One licensing note recorded
now because it will bite later: Wiktionary is CC BY-SA and cannot be embedded
in an MIT/Apache crate casually, where CMUdict is permissive.

## Lane D — The scene register

**Repo**: mere (docs first, then wherever recipes land). **Gate**: the
second-dataset Rosette proof is landed; a code register may now be founded by
the next consumer that needs to select a named complete recipe.

Deck 2 established a third naming register beside surfaces and arrangements,
on the ruling that **levers co-vary**: Mosaic needs tiles that size
themselves to close their gaps, and Atlas needs a map beneath it, so the
intent lives in a combination no single lever holds. Mosaic and Atlas were
released from the arrangement register to this one.

**Ruled**: the content catalog is a collection of scene recipes composed from
the projection grammar catalog. A future Mere-side complete-recipe surface
may absorb `register-lens`'s dormant preset role; persona settings own saved
variants, and `ViewIntent` owns the active instance. That surface depends on
grammar/compiler types. `sceno` never imports it or knows scene names. Plotted
is the common direct-coordinate direction, with physics left to the scene
recipe.

**First acts**: let the next consumer that selects Rosette beside another
named scene found the smallest recipe surface. A scene recipe must carry
**every** lever it depends on rather than a
placement id plus defaults, or it arrives at a second dataset as a scatter of
dots.

## Lane E — Arrangement plumbing cleanup

**Repos**: mere, turnstone. **Gate**: open, small, and best done before a
third table appears.

Two owed follow-ups closed this session: the picker derives its labels from
the registry, and view intent persists as its own sidecar. Two remain.

1. **The two-tables hazard.** `arrangements::registry` carries the
   arrangement register proper; `CANVAS_LAYOUT_STRATEGIES` carries the picker
   catalog, whose ids distinguish dispatchable variants (`kanban.default`
   versus `kanban.community`) that the registry does not. The Columns/Fractal
   rename had to be applied in both by hand. Unify them, or derive one from
   the other; the picker correctly reads the canvas table today.
2. **A checkmarked layout picker surface.** Palette-only today, so the active
   arrangement is invisible until the palette is open.

**Pitfall**: every rename in this area is display-only. The `graph_layout:*`
ids are persistence keys, and turnstone's `view-intent.json` now stores one,
pinned by a test. Renaming an id moves stored sessions.

## Lane F — Held deliberately (do not start)

Recorded so a later session recognizes these as *decided to wait* rather than
forgotten.

- **L3 common arrangement citation.** Mer3ly met the first-device gate. The
  shelfmark format remains held until a second shipping consumer asks for the
  same arrangement record.
- **L4 projection captures as documents.** Entrance gate: the first consumer that wants
  yesterday's projection back. Cheap when it opens, because the encoding
  exists; the work is identity and retention, which eidetic owns.
- **L5 motion.** Entrance gate: the first continuous re-projection a consumer
  ships. **Chronicle**'s scrub transport is the likeliest first consumer and
  deck 5's **Flow** (pulses along a routed polyline) the cheapest receipt.
  The host owns the clock, per the `mere-mesh-host` `Clock` seam discipline.
- **Contract questions held unforced**: a members-free `SpaceId`-only
  `Region` (only `cartography::scene_out` emits regions at all); relation
  endpoints attaching to regions (which would owe a relaxation rule as well
  as a stroke); and whether action intents belong in the contract *ever*, now
  that Graphshell shipped a protocol-side answer. Answer that last one before
  a second intent vocabulary exists that has to agree with the first.
- **Swatch primitive open questions** (§11 of its design): build-order
  sequencing, template conditions vocabulary, per-cell fan geometry,
  edit-layer reach, classifier ranking strength, field-as-scope. These bind
  when the swatch build is greenlit, not before.
