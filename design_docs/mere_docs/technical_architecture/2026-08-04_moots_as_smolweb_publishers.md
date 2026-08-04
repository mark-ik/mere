# Moots as smolweb publishers — and whether knot is a smolweb format

**Date:** 2026-08-04
**Status:** analysis, answering Mark's questions. Nothing scheduled.
**Companion to:** [carrier independence](../../nematic_docs/technical_architecture/2026-08-04_protocol_carrier_independence.md)
(which handles the client direction) and the
[smolweb home decision](../../nematic_docs/technical_architecture/2026-08-03_smolweb_home_decision.md).

Two questions arrived together and turn out to have one answer, so they are
answered together: *could knot become a smolweb protocol and format?* and
*could `dict://` and a `wiki://` be a default moot that any node can
instantiate?*

## The posture change this implies

Everything scoped so far makes us a **client** of the small web: fetch, parse,
render, honestly. Both questions above ask about the other direction —
**publishing** — and that has not been scoped anywhere.

It is the natural payoff. A moot already holds authored content with real
authority, real merge, and real replication. Nothing about that content is
inherently private to our stack, and the small web is exactly the audience for
"a text document you can read with a fifty-line client".

## Knot as a format: yes, as an inert profile

Knot is a good smolweb document in every respect but two, and both are
disqualifying as it stands.

**Transclusion.** Knot's `include` fence pulls another document into this one.
Smolweb formats are deliberately fetch-once: every gemtext link is an
*explicit user action*, and nothing a document contains causes a further
request on its own. That is not an oversight in gemtext, it is the property
that makes the small web unsurveillable — no automatic fan-out means no
beacons, no amplification, and no third-party fetch a reader did not choose.
A format that transcludes over the network gives all of that back.

**Evaluation.** `lua eval` and `rhai eval` fences make a knot document active.
Inertness is the small web's defining property; TerseNet made "100%
JavaScript free" its headline, and gemtext, micron, gophermaps and nex
listings are all incapable of computing anything. A format with an evaluator
is not a smolweb format, whatever its syntax looks like.

**So the answer is a subset: knot-inert.** The polyglot block structure and the
markup, minus network transclusion, minus evaluation. Local includes that were
bundled before publication are fine, because they cost no request.

That is the same move made twice already and it is a good sign for it:
`gemini-protocol` separates the dependency-free grammar from the active
machinery behind a feature, and the [home
decision](../../nematic_docs/technical_architecture/2026-08-03_smolweb_home_decision.md)
separates spec-accurate implementations from our enrichment. Knot-inert is
that same cut, applied to a format we own.

Publishing that profile as a spec, with a spec-accurate crate, is the
stewardship path the smolweb protocol crates already established. Optional,
and only worth it if we want knot read by people who do not run our software.

## Knot as a protocol: no, and the reason is the useful part

Serving knot needs a **MIME type**, not a protocol. `text/x-knot` over gemini,
spartan, nex, or a Reticulum link works today with no new transport code at
all — that is the carrier-independence argument one level up, where a format is
independent of the protocol just as a protocol is independent of its carrier.
Knot already exports to gemtext and gophermap, so a client that does not know
the type degrades gracefully instead of failing.

A new protocol has to earn itself with a **transaction difference**, and knot
has exactly one candidate: fetching a document together with its includes in
one round trip. But that is the feature the inert profile removes, so it
argues against itself. And if it ever came back, **Kepler** is the closer
starting point than gemini, because Kepler is the only protocol in the family
with cache metadata (`last_cached`, last-updated, expires), which is precisely
what a document assembled from parts needs.

So: knot is a smolweb **format** (in an inert profile) and is not a smolweb
**protocol**.

## Moots as publishers: the dictionary idea generalises

Mark's framing — "an aggregate peer-to-peer dictionary people can add to...
essentially a default moot, a preloaded template that can be instantiated by
any node" — is the right shape, and the reason it works is worth stating
plainly.

**The hard part of a shared dictionary is not the dictionary.** It is who may
add an entry, how competing edits resolve, and how the result reaches everyone.
Those are the three problems Commons and Gemot already solve: authority-filtered
projection, delegation certificates, and replicated merge. A dictionary is
then a *content class* over that machinery, which is a schema, not a system.

**So the division is:**

> **The moot is the write model. A smolweb protocol is a read projection.**

Writes go through the moot, where authority lives. `dict://` serves whatever
the current authority-filtered projection says. A stranger with a fifty-line
dictionary client reads a community's work without being granted any way to
write to it, and without us inventing an authorisation story inside a protocol
that has none.

`dict://` ([RFC 2229](https://datatracker.ietf.org/doc/html/rfc2229), TCP port
2628) suits this unusually well: it is a read protocol with a command loop
(`DEFINE`, `MATCH`, `SHOW DB`), and databases are a first-class concept in it,
so one moot can present as one database and a node hosting several moots can
present several.

**`wiki://` does not need to exist.** A wiki is a set of linked documents, and
gemini already serves those; inventing a scheme buys nothing that
`gemini://…/wiki/` does not already have. The valuable half of the idea is not
the scheme, it is the **template**.

## Moot templates

A template is a foundable moot: its content classes, its authority rules, and
its projection configuration, as an artifact any node can instantiate.

- **Content classes** — chartulary already carries these, and Turnstone's own
  two are registered through the same seams a pack would use, so a template is
  not a privileged construct.
- **Authority rules** — the Gemot constitution and delegation shape,
  preconfigured for the pattern (a dictionary wants low-friction contribution
  with revocable authority; a catalogue may want the opposite).
- **Projection** — which protocols this moot publishes over, and what the
  mapping is.

Dictionary and wiki are then two templates rather than two features, and the
list keeps going: a catalogue, a library, a bestiary, a recipe box, a seed
exchange. The layer is the same each time.

## The three-layer story, stated once

- **knot-inert** is the document.
- **the moot** is who may write it, how it merges, and how it replicates.
- **a smolweb protocol** is how a stranger reads it.

Each layer is already built or already scoped except the projection, which is
the new work and the smallest of the three.

## The gate nobody should skip

**Publishing to strangers is a different posture from replicating among
members**, and it should be an explicit act rather than a setting that drifts
on. A moot that serves the public inherits abuse, resource exhaustion, and
responsibility for what it hosts. The participant gate is the right
vocabulary — publishing is a capability a moot grants, revocably, and the
Steward should show it plainly, because "this community is currently readable
by anyone on the internet" is exactly the kind of fact that must never be
discovered by accident.

## What would need deciding, if this is ever picked up

1. Whether knot-inert is published as a spec or stays internal.
2. Whether serving lives in Turnstone, in a separate daemon, or in a moot
   host — a browser that is also a server is a real change in what the app is.
3. How a projection names itself: a dictionary needs a database name, a
   capsule needs an authority, and both must survive the moot being rehosted.
4. Whether writes ever arrive over a smolweb protocol. Titan, spartan's `=:`
   and scorpion all have upload; the recommendation here is **no**, at least
   initially, because every write path needs authority and none of those
   protocols can express ours.
