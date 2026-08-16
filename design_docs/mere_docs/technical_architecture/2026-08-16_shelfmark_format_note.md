# Shelfmark: the scene citation format (v1 note)

Status: ruled format note. Mark ruled the five open questions of the
[scene citation index brief](../research/2026-08-16_scene_citation_index_brief.md)
on 2026-08-16: (1) note now, scoped Mere-level; (2) the citation delta and
A6's sidecar are one record, grounded in the stack; (3) home is incipit,
scoped below, chirograph the fallback if the scoping fails in practice;
(4) checkability is v1, not deferred; (5) the name is **shelfmark**.
Founding the format in code waits for the second shipping consumer; this note
exists so mer3ly's v2 converges instead of diverging.
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

Five parts. Field sketch is **illustrative, not implementation-ready**; the
Rust shapes land with the founding, gated on the second shipping consumer.

```jsonc
{
  "schema": "mere.shelfmark/1",
  "authority": {
    // reconstitutive: a re-derivation recipe the resolver can fetch
    "dataset": "mer3ly/public-repos",
    "cursor": { "source": "...", "commit": "...", "committed_at": "..." }
    // referential mode instead carries a storage key into a saved score;
    // one envelope, two authority forms (DOI versus URL)
  },
  "projection": {
    "reading": "...",              // cartography registry id
    "arrangement": "graph_layout:radial"  // arrangements registry id
  },
  "expects": {
    "generation": 1234567890        // required in v1; see checkability
  },
  "delta": {
    "placement": { /* A6's record, by reference; see the delta rule */ },
    "selection": { /* reserved; defined by A2 when it lands */ },
    "backdrop":  { /* reserved; defined by C3 when it lands */ }
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
- `selection` and `backdrop` are reserved names. A2 and C3 define them when
  their gates open. Until then a shelfmark may carry them only as host-local
  content under the unhonored-section rule below.

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

`expects.generation` is required in v1. Because `score.generation` derives
from the authority's SHA-256, the resolver reconstitutes, compares
generations, and reports match or mismatch. A mismatch is a report, not a
failure: the authority may legitimately have moved, and the cursor tells the
resolver what the citing party saw. Nothing blocks this today; the field
exists on every score mer3ly ships.

## Home: incipit, scoped (ruling 3)

Evidence for the fit:

- incipit is 127 lines, depends on serde and uuid only, and a shelfmark with
  plain types (strings, f32 pairs, u64) adds no dependency;
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

## Convergence map for mer3ly v2

| `mer3ly.graphshell-scene-state/v1` | shelfmark v1 |
| --- | --- |
| `schema` | `schema` (becomes `mere.shelfmark/1` at adoption) |
| `dataset` | `authority.dataset` |
| `source { source, commit, committed_at }` | `authority.cursor` |
| `reading` | `projection.reading` |
| `arrangement` | `projection.arrangement` |
| `pins`, `motion` | `delta.placement`, once A6 defines the record |
| `selection` | `delta.selection`, reserved until A2 |
| `backdrop { kind, collidable }` | `delta.backdrop`, reserved until C3 |
| `physics` | host-local section under the unhonored rule; no target owns it |
| (absent) | `expects.generation`, new and required |

The carrier stays the site's choice: URL hash base64url on the web, a
chirograph frame on the wire, a muniment record at rest.

## Founding gate and non-goals

The format founds in code when the second consumer ships: graphshell's
saved-score reference (chirograph `ProjectionRequest` currently inlines a
full `Score`) or turnstone's share-a-curated-view, whichever lands first.
Until then this note is the authority and mer3ly v2 is its proving ground.

Non-goals, restated: not a serialized presentation, not an intent (D1
stands), not a second score format, and no new crate; the shelfmark is a
module inside an existing home.
