# Scene citations: the index above the projection contract

Status: thinking brief, commissioned by Mark; **all five questions ruled
2026-08-16**, recorded in the
[shelfmark format note](../technical_architecture/2026-08-16_shelfmark_format_note.md),
which is now the authority. This brief stays as the reasoning record.
Date: 2026-08-16
Scope: what mer3ly's scene-state wire is when read as an index, whether that
reading deserves a Mere-level format, and what it changes about A6
Sources: [mer3ly stack consumer survey](2026-08-16_mer3ly_stack_consumer_survey.md),
[projection grammar adoption plan](../implementation_strategy/2026-08-15_projection_grammar_adoption_plan.md)
(Open rulings), [graphshell remote projection host plan](../implementation_strategy/2026-07-22_graphshell_remote_projection_host_plan.md)
§4.2, `crates/chirograph/src/lib.rs`, `repos/mer3ly/assets/graph-sandbox.js`

## The object

A scene citation is a small versioned record that names a projection so a
peer can reconstitute it, instead of shipping the realized scene. Mer3ly's
`mer3ly.graphshell-scene-state/v1` is the first one in production: ten fields,
base64url in a URL hash, restoring dataset, revision cursor, reading,
arrangement, and an authored delta on a second device.

The reading that makes this an index rather than a transport is a three-layer
stack:

```text
realized scene   heavy; the graphshell wire ships it as snapshot + diffs
score            deterministic spec; the frozen 0.0.3 contract
citation         name of (authority, projection, delta); the index
```

Book terms: the work, the edition, the citation. The citation layer exists
because the score is a pure function of its inputs. `score.generation` is
derived from the authority's SHA-256, so naming the authority and the
projection choices is sufficient to rebuild the scene, and the wire stays
small enough to live in a URL fragment.

## Anatomy

Four parts, generalized from mer3ly's v1 and the remote plan's stated need:

1. **Authority cite.** Which dataset, at which revision. Mer3ly carries
   `{ source, commit, committed_at }`. An authority hash may ride along for
   verification but is derivable by the resolver.
2. **Projection cite.** Reading id and arrangement id, resolved against the
   cartography and arrangements registries. Names, never configurations.
3. **Authored delta.** What a person did that the authority does not know:
   pins, selection, motion class, backdrop choice. This is the only part that
   is not derivable, and it is exactly the contested territory of the
   catalog's promotion rules.
4. **Envelope.** Schema id and version, plus carrier encoding. URL hash for
   the web, a chirograph frame on the wire, a muniment record at rest. The
   citation is carrier-agnostic the same way the remote plan's handshake is:
   a QR code, a web link, and a Retinue address all lead to the same object.

## The property worth protecting

Citations are checkable. Because generation is a function of the authority
hash, a citation can carry the generation it expects, and a resolver can
prove it reconstituted the same scene the citing party saw. A bookmark hopes;
a citation verifies. This is the determinism receipt crossing a device
boundary for free, and it is the single strongest argument that the index
belongs to Mere rather than to one site: the property only holds where the
solver is deterministic, which is a contract guarantee, not a site behavior.

## Two resolution modes, one shape

- **Reconstitutive** (mer3ly today): the resolver holds or fetches the
  authority and re-solves. The citation carries names and a delta only.
- **Referential** (remote plan §4.2, unbuilt): "a versioned score or a
  reference to a saved score." The resolver holds a saved score behind
  muniment; the citation is a key into storage.

DOI versus URL. One envelope covers both if the authority cite may be either
a re-derivation recipe or a storage key. Chirograph's `ProjectionRequest`
currently inlines a full `Score`, so the referential mode is a named gap in
the protocol, not a new invention of this brief.

## What a citation must not become

- **Not a serialized presentation.** The remote plan already rejected uxtree
  as wire contract; the citation sits two layers above that temptation.
- **Not an intent.** Freeze ruling D1 stands. A citation is a noun. Opening
  one is the host's act, carried by chirograph's existing verbs.
- **Not a second score format.** The bar for each field: if removing it makes
  reconstitution impossible, it belongs; if removing it merely changes
  fidelity, it is delta; if the resolver can derive it, it must not be there.

## What this changes about A6

Less than it first appears, and in a useful direction. The citation schema
barely depends on A6; its **fidelity** does. Today a cited pin reconstitutes
through site-local JavaScript because the score cannot express it, so the
citation is lossy at the portable boundary. A6 moves fields downward, from
delta-the-host-interprets to score-the-solver-honors. The citation format
survives that migration unchanged; only the size of its unexpressible
remainder shrinks. So the index framing does not compete with A6, it
motivates it: A6 is what makes citations honest.

The sidecar question inside A6 also gets sharper. If the seam is a sidecar
the score references, the sidecar and the citation delta are close to the
same record, which argues for deciding them together.

## The embedding horizon

Mark's stated ambition: embed full versions of all the apps in the site. Under
that reading the citation is the navigation currency. An index entry becomes
(app, citation), a deep link into an embedded app is a citation with a host
prefix, and the site's repository page becomes an index in the book sense: a
curated list of citations into the family's projections. The IoT beacon
concept (knot HTML projection as beacon container) is the same shape over a
different carrier.

There is also a rhyme worth naming without merging: gazette resolves who
(handles to contacts), a scene citation resolves what-view (names to
projections). Two resolution indices, same posture, different domains. They
should stay separate crates-or-modules; the rhyme is architectural, not an
implementation invitation.

## Home and gate

Module first, per standing policy. The type is small: four parts, serde, one
validation function, no dependency on the graph itself. Candidate homes:

- **chirograph** (protocol nouns live there; `ProjectionRequest` is the named
  integration point for the referential mode);
- **incipit** (the workspace identity vocabulary; a citation is arguably an
  identity for a view, and incipit already ships GraphId and SessionId with
  no graph dependency);
- a module inside the graphshell session grammar.

Promotion gate, honestly applied: one authority-grade consumer ships today
(mer3ly), and a second is named on paper (graphshell's saved-score
reference). Paper is not shipping. The catalog's own discipline says found
the Mere-level format when the second consumer actually lands, and until
then, write the format note so mer3ly's v2 converges instead of diverging.
Turnstone's share-a-curated-view is the most natural third.

## Naming

Not decided here; candidates for Mark's ear, register-checked against the
document family (chartulary, muniment, codicil, scholia, chirograph,
titulus). `incipit` is taken in-tree.

- **siglum**: the short conventional symbol textual critics use to cite a
  manuscript witness in an apparatus. Nearly literal: a compact key that
  names a witness so others can check the reading. Plural sigla.
- **shelfmark**: the library's location code for one manuscript. Plainer,
  leans referential (a key into a collection) over reconstitutive.
- **colophon**: the note identifying a text's production. Weaker fit; it
  describes provenance, not address.
- Plain lane: `SceneCite` / scene citation as the type name inside an
  existing crate, deferring the soulful name until the format is real.

## Questions for Mark

1. Is the index a Mere-level format now, or a format note that mer3ly v2
   follows until graphshell's saved-score reference lands? The brief leans
   note-now, format-at-second-consumer, per the promotion discipline.
2. Should A6 decide its sidecar question and the citation delta as one
   record? They are close to the same shape, and deciding them separately
   risks two serializations of pins.
3. Home when it does land: chirograph, incipit, or session grammar?
4. Does the checkability property (citation carries expected generation,
   resolver verifies) belong in v1 of the format, or is it a later receipt?
5. Any pull toward siglum or shelfmark, or hold the soulful name?
