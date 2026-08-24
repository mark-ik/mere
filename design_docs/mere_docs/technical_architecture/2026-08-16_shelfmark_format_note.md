# Shelfmark: the scene citation format (v1 note)

Status: ruled format note. Mark ruled the five open questions of the
[scene citation index brief](../research/2026-08-16_scene_citation_index_brief.md)
on 2026-08-16: (1) note now, scoped Mere-level; (2) the citation delta and
A6's sidecar are one record, grounded in the stack; (3) home is incipit,
scoped below, chirograph the fallback if the scoping fails in practice;
(4) checkability is v1, not deferred; (5) the name is **shelfmark**.
**Founded 2026-08-23:** `incipit::ShelfmarkV1` landed at Mere `6cc014c4` after
the two-reading Matrix forced named inputs, per-input generations, reading
parameters, and instance-scoped target deltas. Mer3ly is the first executable
consumer; the gazette Ledger independently replays the shape as the second.
**Proving ground receipt, 2026-08-19:** mer3ly v2 shipped the convergence
(`mer3ly` `4447472`, live on mer3ly.net). The wire is the shelfmark shape:
authority cite with revision cursor, projection cite by registry ids, required
`expects.generation` computed from the same recipe that stamps
`score.generation`, and a delta of named sections with `placement` as
`HeldPlacement` verbatim and `selection` as `chirograph::Selection`. Motion and
backdrop ride as host-local sections under the unhonored rule. Verified live in
a browser: a shared link restores and reports "citation verified against this
authority"; a tampered generation reports "authority has moved"; a foreign
`acme.experimental` section survives the re-share round trip and is counted in
the report. v1 links keep decoding.
Date: 2026-08-16
Scope: v1 envelope fields, the delta rule, checkability, home, and the
convergence map from `mer3ly.graphshell-scene-state/v1`

## The object, compressed

A shelfmark names a projection so a peer can reconstitute it, instead of
shipping the realized scene. It sits above the frozen score contract:

```text
realized scene   heavy; graphshell wire ships it as snapshot + diffs
score            deterministic spec; the 0.0.3 contract
shelfmark        name of (authority, projection, delta); the index
```

The register is deliberate. In a manuscript catalog the incipit identifies
the work and the shelfmark locates the copy on the shelf. Mere's `incipit`
crate already carries the work-identity half (GraphId, SessionId); the
shelfmark is the where-to-find-a-view half of the same vocabulary.

## v1 envelope

The landed envelope has four parts. `inputs` is an ordered map, so each named
authority/reading pair keeps its own checkability. Authority records, reading
parameters, and delta sections are opaque strings interpreted by their owning
adapter or target.

```jsonc
{
  "schema": "mere.shelfmark/1",
  "projection": "matrix",
  "inputs": {
    "rows": {
      "authority": {
        "adapter": "mer3ly.dataset/v1",
        "record": "{...opaque authority locator...}"
      },
      "reading": "neighbors",
      "reading_parameters": "{\"focus\":\"mere\"}",
      "arrangement": null,
      "expects_generation": "1234567890"
    },
    "columns": {
      "authority": {
        "adapter": "mer3ly.dataset/v1",
        "record": "{...second authority locator...}"
      },
      "reading": "changes",
      "reading_parameters": null,
      "arrangement": null,
      "expects_generation": "9876543210"
    }
  },
  "delta": {
    "selection": "{...chirograph::CoordinatedSelection...}",
    "mer3ly.instances": "[{...projected instance address...}]"
  }
}
```

Membership bar, unchanged from the brief: if removing a field makes
reconstitution impossible, it is envelope; if removing it changes fidelity,
it is delta; if the resolver can derive it, it must not be present.

## The delta rule (ruling 2)

The delta is a map of named sections, and **each section is defined by the
target that owns the concern, by reference, never by a parallel
serialization**. Concretely:

- The `placement` section *is* A6's record. Whatever A6 decides, write-back
  or sidecar, there is one serialization of a pin. If A6 chooses write-back,
  the section is a score patch; if sidecar, the section embeds the sidecar
  record. Deciding A6 decides this section.
- The `selection` section is `chirograph::CoordinatedSelection`: named clauses
  carry their producing view, role, and targets beside an explicit resolution
  rule. The pre-coordination `chirograph::Selection` remains the single-view
  record and old links still decode through their old envelope.
- Instance-scoped sections are target-owned. Mer3ly's
  `"mer3ly.instances"` and gazette's `"gazette.instances"` each serialize the
  target's own projected-instance address. Incipit preserves the bytes without
  acquiring either product vocabulary.
- `backdrop` remains a reserved name. C3 defines it when L2 opens. Until then a
  shelfmark may carry it only as host-local content under the unhonored-section
  rule below.

Sections mirror the stack's own extension valve: the scene extends through
named channels, the shelfmark extends through named sections. Mer3ly's
untyped `"fold"` channel is the cautionary precedent for what happens
without named, owned sections.

**Unhonored sections are reported, never dropped.** A resolver that does not
recognize a section preserves it through round trips and reports it as
unhonored. This is the WebCoLa lesson applied to the index: silent
best-effort is the warned failure mode, at the citation layer as much as at
the solver.

## Checkability (ruling 4)

`inputs.<name>.expects_generation` is required in v1. A resolver reconstitutes
each named input, compares its generation independently, and reports which
authority moved. The Matrix receipt checks both axes; the Ledger receipt checks
its contact and facet authorities. A mismatch refuses reconstitution while
reporting expected and found generations rather than silently accepting drift.

## Home: incipit, scoped (ruling 3)

Evidence for the fit:

- incipit keeps the envelope in plain serde types and adds no scene or product
  dependency;
- its stated posture, "the names by which a work is found," is the
  shelfmark's posture exactly, and the codicological register pairs;
- consumers that need the envelope (mer3ly, graphshell hosts, turnstone) can
  take an incipit dependency freely, since it carries no graph truth.

Two caveats, recorded so the fallback trigger is legible:

- incipit's doc thesis is "two ids, and nothing else"; housing the shelfmark
  widens that to the identity vocabulary its Cargo description already
  claims. Acceptable, but it is a real widening.
- the `placement` section must not pull sceno types into incipit. The
  envelope holds sections without interpreting them (opaque at the envelope
  layer, typed by their owning targets). If in practice this indirection
  fights the one-record rule, that is the "if wrong" trigger, and the
  shelfmark moves to chirograph, where protocol nouns already live and
  `ProjectionRequest` is the named integration point.

## Historical convergence map for mer3ly v2

This table records the singular proving-ground shape, not the landed plural
input envelope. Mer3ly keeps decoding it for compatibility.

| `mer3ly.graphshell-scene-state/v1` | pre-founding shelfmark sketch |
| --- | --- |
| `schema` | `schema` (becomes `mere.shelfmark/1` at adoption) |
| `dataset` | `authority.dataset` |
| `source { source, commit, committed_at }` | `authority.cursor` |
| `reading` | `projection.reading` |
| `arrangement` | `projection.arrangement` |
| `pins`, `motion` | `delta.placement`, once A6 defines the record |
| `selection` | `delta.selection`, now `chirograph::Selection` (A2) |
| `backdrop { kind, collidable }` | `delta.backdrop`, reserved until C3 |
| `physics` | host-local section under the unhonored rule; no target owns it |
| (absent) | `expects.generation`, new and required |

The carrier stays the site's choice: URL hash base64url on the web, a
chirograph frame on the wire, a muniment record at rest.

## Composition receipt (2026-08-23)

The later [projection-scenes direction note](../../2026-08-23_projection_scenes_and_graph_native_platform.md)
defines Matrix over two independently produced readings and makes repeated
projected instances a platform rule. The Wave 1 receipt answered the singular
sketch's two open questions:

- a composed projection cites a named ordered map of authority/reading inputs,
  each with its own expected generation;
- reconstitutive reading parameters travel with their input; and
- instance-local authored state addresses the projected instance in a
  target-owned delta section rather than weakening source identity.

Mer3ly round-trips a two-reading Matrix across distinct sandbox datasets,
restores coordinated selection and a dismissed Deck instance, and verifies
both input generations. Gazette replays the same envelope for contacts ×
facets without leaking Gazette fields into `ShelfmarkV1`.

Scene-catalog collapse creates a related registry rule. If a reading,
arrangement, or other registry id has actually been emitted in a shelfmark, a
later catalog judgment must not silently retarget it. The resolver either
preserves the cited meaning through a versioned alias or reports the identifier
as incompatible or unhonored while preserving it through a round trip. A name
appearing only in a design catalog is not evidence of wire exposure and does
not require an alias.

## Compatibility audit and non-goals

The emitted mer3ly v1/v2 wires use reading ids `graph`, `changes`, `activity`,
`neighbors`, and `matrix`, plus `graph_layout:*` arrangement ids. The catalog
names collapsed by the 2026-08-23 direction note were never emitted as
registry ids. `Neighborhood` is a UI label for `graph_layout:radial`, not a
wire id. No alias or incompatibility machinery is required by current exposure.

Non-goals, restated: not a serialized presentation, not an intent (D1
stands), not a second score format, and no new crate; the shelfmark is a
module inside an existing home.
