# Scope / Graphlet Model Reconciliation (decisions + rulings)

**Date**: 2026-06-27
**Status**: Decision record, no code. This does **not** re-derive the scope/swatch
model: that already exists in [gloss = the Navigator](2026-06-07_gloss_navigator_design.md)
(Mark, through 2026-06-23). After three review turns the build and the docs had
drifted from that model and from each other; this records the cross-cutting
**rulings + decisions** and points each home.

**The model already exists (canonical, do not duplicate).**
[gloss = the Navigator](2026-06-07_gloss_navigator_design.md) holds it: one
Navigator (single surface, configurable scope + form factor, **never split**, §1);
scope is a containment zoom **node ⊂ graphlet ⊂ graph** (§2b); the **swatch** is
the portable, embeddable primitive configured by `(scope, layout, lens, mode,
filters)` with a view/edit toggle and a variant library (§2a/§2b); graphlets are
latent rule-defined views, including the **chronological** chain (§3).

**The docs this reconciles.**

- [gloss = the Navigator](2026-06-07_gloss_navigator_design.md) — the **model**.
- [graphlet derivation from selection](2026-06-13_graphlet_derivation_from_selection.md)
  — the **selection UX** (reveal → classify → project → frontier → crystallize).
- [graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md)
  — the **build** (branch + Linked graphlets, kernel derivation, drift, selectors).
- [relational browse plan](../implementation_strategy/2026-06-23_relational_browse_graphlet_plan.md)
  — a **consumer** (link materializer + crawl).

---

## 1. The build instanced where the model says scope

gloss_navigator_design's founding rule is "one surface, never split." The wiring
build opened a **new window per graphlet** (it reused the tear-out branch-window
path), which is splitting into instances. **Ruling:** scope is a first-class,
navigable property of the *one* Navigator (arrows step the scope, an X reverts to
the whole graph); window / swatch / strip are form factors; a torn-out window is a
deliberate gesture, not the default way to view a graphlet. The graphlet *data*
already supports this; only the presentation took the shortcut. (Correction for the
wiring plan; not a teardown.)

## 2. Where latent graphlets live — RESOLVED

gloss_navigator_design §8 left this open (gloss-owned store vs forme `GraphletRef`
vs a cartography spec; "lean: forme"). The wiring plan **settled it: forme
`GraphletRef`** in a per-session `SessionGraphlets` index over kernel uuids
(decision B; `GraphTree` closed by the derivation finding). This closes the gloss
§8 open question.

## 3. Terminology ruling

- **graphlet** = a named, persistable **scope** (forme `GraphletRef`). Branched =
  frozen roster; Linked = derived (`kind` + `selectors` → drift-tracking).
- **A graphlet selects; it does not make nodes.** Relational-browse adds nodes,
  graphlets scope them, the graph keeps one truth.
- The relational-browse plan uses "graphlet" colloquially (its title only); it
  becomes literal once a browse mints a Linked scope (ruling 7).
- **frontier**: the *crawl* frontier (a fetch queue) and the graphlet **Frontier**
  *kind* (a scope's one-hop boundary, the candidate ghosts) are different layers
  naming the same boundary; the crawl's "real negatives in context" *are* a
  Frontier scope.

## 4. Derivation preview = the swatch primitive, not an orrery mutation

The [2026-06-13 UX](2026-06-13_graphlet_derivation_from_selection.md) mutates the
orrery in place (dim, reveal latent edges, strip, frontier ghosts). **Ruling:**
route the rich preview through the existing **swatch primitive** (gloss §2a/§2b),
scoped to the `Selection`. Non-destructive, with **kind-preview** (hover a kind
chip and the swatch lays the selection out *as* that kind) and **edges / scene /
theme as a swatch preset** (the WHAT projection + HOW form-factor bundle). Keep a
**light** in-canvas anchor (highlight the selection, fade its inter-edges, no full
dim) for in-context position + frontier ghosts. So the preview is a *consumer* of
the swatch primitive §2a already calls for, not a bespoke canvas mode.

## 5. Traversal always threads the selection

Default projection is all-families (2026-06-13) including traversal; gloss §3
already lists the chronological graphlet. **Ruling:** because browse order threads
the selection, **Corridor is the always-available fallback shape** and a pure
"Loose set" is rare (only across nodes never navigated between). The classifier
treats the traversal corridor as the floor.

## 6. Classifier is the gap; forward derivation is built

"Pattern-match to shapes we recognise" is the **shape classifier** (selection →
ranked kinds), the 2026-06-13 doc's named "one real gap," still unbuilt. The wiring
plan built the **inverse**: forward derivation (`kind` + seed + `selectors` →
members, drift-tracked, two layers). Classification (selection → kind) feeds
derivation (kind → members).

## 7. Relational-browse mints a Linked graphlet (open decision)

The merge that makes relational-browse a real graphlet consumer: a browse adds the
nodes **and** mints a Linked Ego/Component scope over them, so the neighborhood is
a navigable, drift-tracking scope, re-readable by relation family, with the
**Frontier kind = the candidate set** the plan already prizes as "real negatives in
context." **Mark's call**, recorded not assumed.

---

## Open decisions

- Make scope first-class + navigable on the one Navigator (ruling 1)? Recommend
  yes.
- Relational-browse mints a graphlet (ruling 7)? Mark's call.
- Keep the light in-canvas anchor alongside the swatch (ruling 4)? Recommend yes.
- **Classifier ranking strength** (carried from the 2026-06-13 open question):
  traversal has counts, semantic has decay, arrangement has durability; a unified
  strength number likely wants to be a setting, not a constant.

## What changes when a build is greenlit (not a teardown)

Lift the per-surface scope flags (`Orrery::scope`, `gloss_scope_selection`) into
one `Scope`; shared arrows/X nav chrome; the derivation preview via the swatch
primitive; build the shape classifier; demote window-per-graphlet to a gesture. The
graphlet data (`GraphletRef` / `SessionGraphlets`) and the orrery scope mechanism
stay.
