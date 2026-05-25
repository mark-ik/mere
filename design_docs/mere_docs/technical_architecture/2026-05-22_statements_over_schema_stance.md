# Statements Over Schema: Mere's Data-Model Stance

**Date**: 2026-05-22
**Status**: Adopted principle. Applied incrementally; each application is its own dated plan.
**Sibling**: the [composition spine](2026-05-21_mere_composition_spine.md) governs *how truth becomes surfaces*. This doc is about *what truth is* at the statement level, one layer beneath it.

---

## The principle

Mere's graph truth is a set of **statements**: `(subject, predicate, object)`. The predicate is an **open identifier** (an IRI) drawn from an unbounded space, with a **recognized core** that carries first-class behavior. Anything unrecognized is still stored faithfully and rendered generically.

This holds uniformly across what the kernel currently treats as three separate things:

- **edges** (node to node relations),
- **node properties** (title, tags, classification),
- **types** (what a node is).

All three are statements about a subject. The only real distinction is whether Mere *recognizes* the predicate and gives it behavior.

## Why we adopted it

We keep reinventing linked-data primitives and finding our version is the rigid one. That is not coincidence: we are building a graph of the web, which is the territory RDF mapped. The friction is always the same shape, a closed schema meeting an open world:

| Mere concept | The linked-data primitive it reinvents |
|---|---|
| Opening the Semantic predicate (in-flight) | open vocabulary + recognized core |
| `AddressClaim` (Primary / Alias) | `owl:sameAs` / identity aliasing |
| `Provenance` / `Imported` edge families, document trust state | reification (statements about statements) |
| `tags` as opaque local strings | shared vocabulary types |
| the "no node property bag" gap | properties are just predicates on the node |

Naming the principle stops us hitting this wall a fifth time, and gives every linked-data plan a parent to point at.

## The two-layer model (the guardrail)

This is the part that keeps Mere from dissolving into a generic triple store, which would discard everything that makes it itself.

- **Open statement substrate, underneath.** Predicates are open IRIs. Anything can be said about any subject. Storage is lossless.
- **Curated behavioral lens, on top.** A small, closed, opinionated vocabulary that Mere recognizes and acts on: `EdgeFamily`, the recognized `SemanticSubKind`s, the relation-family UX, the recognized node properties. Behavior dispatches on this layer, which stays exhaustive and small.

The recurring pattern, stated once: **recognized core gets behavior, open tail gets stored, presentation curates.** An unrecognized predicate is never dropped and never breaks dispatch; it renders generically until something recognizes it.

Mere is not becoming RDF. Mere has an RDF-shaped substrate that it *curates*. The composition spine's small-vocabulary discipline governs the lens; the substrate beneath can be as open as the web is.

## The posture

Two stances follow, both of which a *browser* should hold anyway:

- **Partial and multi-source.** The graph is what Mere has gathered so far, from many sources, not a closed truth. Absence is not falsehood. We take the open-world *posture* (provenance per statement, mergeable views) without open-world *logic* (entailment and reasoning, which is the footgun).
- **Identity as the merge key.** Global, dereferenceable identity is what lets two sources about the same thing merge. Mere's nodes are already URL-addressed, so this is a strength to lean on rather than bolt on. `AddressClaim` is the existing seam.

## Linked data is a projection, not a boundary

The composition spine already says: graph truth, projected into surfaces (tree, cartography, and so on). A JSON-LD document is another projection of the same truth, and ingestion is the inverse. Linked-data interchange is the spine's own logic applied to data exchange instead of display, not a special boundary feature.

## What this does not mean

The anti-scope, stated plainly so the principle is not over-applied:

- The kernel is not rewritten. The model evolves instance by instance, each its own plan.
- Reasoning stays out: OWL, RDFS, entailment.
- Signing stays out: RDF canonicalization and the verifiable-credential stack (deferred with federation).
- The curated vocabulary stays. The lens remains opinionated and small.
- Structure stays. The families exist because behavior needs more than a flat triple gives.

## How it is applied

Incrementally. Each instance is a small, separate plan that points back here.

- **First instance, in-flight:** open the Semantic predicate for linked-data ingest/export. See the [linked-data ingest/export plan](../implementation_strategy/2026-05-22_linked_data_ingest_export_plan.md).
- **Candidate next instances, named but not scheduled:** node properties as recognized predicates on the node-as-subject (closes the property-bag gap); `tags` and classifications as recognized vocabulary types; folding the scattered provenance and trust annotations into one statement-level annotation.

Each lands only when a consumer needs it, under the composition spine's discipline. The principle is the through-line; the plans are the work.
