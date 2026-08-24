# Moots as smolweb publishers — and whether knot is a smolweb format

**Date:** 2026-08-04
**Status:** analysis, answering Mark's questions. Nothing scheduled.
**Companion to:** carrier independence (`smolweb/design_docs/research/2026-08-04_protocol_carrier_independence.md`)
(which handles the client direction) and the
smolweb home decision (`smolweb/design_docs/technical_architecture/2026-08-03_smolweb_home_decision.md`).

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
machinery behind a feature, and the home
decision (`smolweb/design_docs/technical_architecture/2026-08-03_smolweb_home_decision.md`)
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

## Assembly on activation: the transclusion objection, resolved (Mark, 2026-08-04)

The objection above was aimed slightly wrong, and Mark's reframing fixes it.
Gemtext's rule is not *never fetch another document*. It is **never fetch
without being asked**. A link is a link, not a booby trap. What makes
transclusion hostile is not that a part arrives, it is that it arrives
unasked.

So `include` does not have to be removed. It has to be **deferred**: rendered
as an affordance rather than as content, fetched when the reader activates it,
exactly like following a link. A knot document becomes a **manifest of parts**
you assemble by activating blocks, and the assembled result is a document built
like lego rather than a page that phoned six servers before you finished
reading the title.

That inverts the earlier conclusion in a good way: knot-inert keeps
transclusion and loses only the *automatic* part of it.

**Three borrowings make the assembly model real,** two of them from protocols
Mark surfaced:

- **Caching, from Kepler.** Kepler is the only protocol in the family with a
  cache model (`last_cached` in the request; content length, last-updated and
  expires in the response). An activated part should be stored, so assembly is
  paid for once and re-assembly is local. Without this, "activate to expand"
  costs a fetch every time and the model is a nuisance.
- **Version control, from Demarkus.** A part referenced by revision makes an
  assembly **reproducible**: the same manifest yields the same document, and a
  changed part is a visible change rather than a silent one. Codicil's
  append-only log and chartulary's content addressing already provide the
  substrate.
- **Tessera**, for parts that are not public. A gated part is served against a
  trust receipt rather than an ACL, which keeps it in character: tessera says
  *this reader is vouched for*, not *this reader has permission to read row 7*.

**The constraint that keeps it honest, and it is not optional:** the base
document must stand alone. A fifty-line client that knows nothing of
activation, caching or tessera must see a complete, readable document, with
unassembled parts appearing as links it can follow or ignore. Parts *enrich*;
they never carry the meaning. The moment a knot document is unreadable without
assembly, it has stopped being a smolweb format and become an application.

**One line that must not be crossed.** Assembly-on-activation is about
*fetching*, not *evaluating*. Activating a fetch is a reader asking for a
document, which is ordinary. Activating an evaluator on content someone else
wrote is running a stranger's program, which no amount of clicking makes safe.
Evaluation stays as it already is: available for content you authored or
trust, inert for content you received.

## Self-serve: projection is not the same as template

Mark's question — should a gemtext capsule be a moot template, or is that only
for formats we own? — has a clean answer, and getting it wrong would import
exactly the heaviness the small web exists to reject.

**A capsule is one author.** A directory of files, served. It has no
coordination problem, so it needs no coordination machinery: no membership, no
delegation certificates, no merge. Putting a moot under a personal capsule
would be paying Gemot's whole cost to solve problems that do not exist there.

So the two concepts separate:

| | What it is | Who needs it |
|---|---|---|
| **Projection** | how content is served: a gemtext capsule, a dict database, a gophermap | **any node**, moot or not, including a solo user with none |
| **Template** | a preconfigured governance shape: content classes, authority rules, projection config | only where there is coordination to govern |

> **Single-author publishing needs no moot. A moot earns its place at the
> second writer.**

"Serve my knot notes as a gemtext capsule" must work with no moot anywhere in
the picture. "Instantiate the dictionary template" is a moot, because a
dictionary many people add to is a governance problem wearing a content
schema. Mark's dictionary is the second case and a personal capsule is the
first, and they should not be made to share a mechanism.

This also rhymes with a decision already taken: under the Knot-in-graphshell
plan's Option A, personal documents replicate while shared documents project.
Same shape. The solo case stays lighter.

### And formats we own versus formats we do not

The home rule (`smolweb/design_docs/technical_architecture/2026-08-03_smolweb_home_decision.md`)
applies to publishing exactly as it does to parsing:

- **Formats we own** (knot): we may innovate — assembly, caching, revisions —
  provided the inert base degrades gracefully for everyone else.
- **Formats we do not** (gemtext, gophermaps, dict databases): we project into
  them **faithfully and add nothing**. A capsule we serve should be
  indistinguishable from a capsule anyone else serves. No extensions, no
  private conventions, no markers announcing which software produced it.

That second rule is what keeps self-serve honest. The small web's whole
proposition is that the format is small enough that anyone can implement it,
and a publisher who quietly extends the format they publish takes that back.

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
