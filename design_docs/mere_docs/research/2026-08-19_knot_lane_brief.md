# Knot: product floor and the next three cuts

Status: position revised and cuts implemented 2026-08-20.

Knot is a standalone-capable Mere port. Turnstone can host it, but Turnstone is
not what makes it a product. The useful floor is already ambitious enough:

> a Djot editor on a graph substrate with local-first, git-like peer
> replication

That is a credible Obsidian or Anytype alternative without a hosted sync
dependency or an arbitrary storage allowance. Clipping and lexical lenses make
it more distinctive. They do not change what the authored document is.

## The format decision

The authored format is Djot. Knot may keep its domain terminology, protocols,
graph concepts, and `knot:` addresses without inventing a file format.

The current `.knot` codec is Djot with a YAML front matter convention. That
does not earn a distinct format. `.knot` and `text/vnd.knot` remain accepted as
compatibility spellings in the present code, while `.djot` is a first-class
authoring route. They should not be presented as a promise that Knot owns a
new markup language.

A real Knot format becomes worth considering only if the portable object must
carry something Djot cannot carry cleanly. The likely trigger is a future
single-object interchange story containing authored Djot plus referenced
artifacts, manifests, retention state, or other enriched graph material. Call
that cut 4. It is not required for the product floor or for cuts 1 through 3.

One useful piece has been taken from that future work now: a portable content
reference. A captured artifact is named `urn:blake3:<digest>`. The name is
independent of the local store and can survive copying the authored Djot. It
identifies bytes; it does not encode a location or turn the Djot document into
a package.

Availability beside p2p replication is now a separate resident path. Murm's
blob store retains the bytes under that digest. Stickleback and p2panda
replicate the ordinary encrypted Djot document containing the reference. The
receiving Knot then asks an authorized peer for the named blob over the same
authenticated transport and verifies the byte count and BLAKE3 digest before
exposing it. A named local tag keeps the fetched blob available after restart.

The ownership split is concrete:

- Personae pairing supplies the symmetric peer set for a personal vault;
- a communal space materializes document writers and evidence readers from
  Gemot capability paths;
- Knot decides what a reference means and when verified bytes may be shown;
- Murm carries and persists opaque bytes;
- p2panda and Stickleback replicate the signed causal document operations.

This is useful p2panda synergy, but not a reason to invent `.knot`. The document
and the artifact have distinct replication and retention behavior while still
sharing stable content identity and authenticated peer identity.

The resident composition shares one blob-store handle between authoring and
sync in a single process. The existing `knot_endpoint` and
`knot_sync_host` executables still have separate lifetimes, and the redb-backed
blob store must have one process owner. Joining those executable paths requires
either one combined resident or an explicit local retain/fetch IPC. Pointing
both processes at the same directory would only disguise the ownership bug.

## 1. Evidence-bearing clipping

The distinction remains useful: authored Djot is a statement, while a clip is
an observation about another artifact. The observation needs enough evidence
to be checked without turning the Djot body into an archive container.

`knot.clip.insert/v2` now carries:

- structured text-quote, text-position, or DOM-range selectors, each naming
  whether it addresses the source response or an observed representation;
- one or two bounded artifacts with their media type and canonical URI;
- fidelity findings tied to the relevant selector;
- discovered links explicitly labelled as observed edges;
- the lowered Djot body.

Knot only advertises v2 when the host injects an evidence store. Otherwise it
continues to advertise and accept v1. A v2 insertion retains the exact artifact
bytes in the transport blob store under their BLAKE3 digest, then writes only
their portable references and the observation metadata into the authored Djot.
The evidence root is a host setting, including its byte limit, and Turnstone
rejects a directory inside the served document tree so retained blobs cannot be
mistaken for authored files.

The proven vertical path is deliberately narrower than the ideal one. Genet's
static session retains the exact response bytes it parsed. Turnstone lowers the
session's semantic clip and selector into the v2 contract. Knot retains those
bytes and authors the portable reference. Livery and scripted sessions do not
yet claim an artifact they cannot honestly supply, and Turnstone rejects their
use against a v2-only target.

The current fidelity result is also explicit about its limit. Source response
retention proves what the parser consumed. It does not prove post-script DOM or
visual arrangement. Turnstone records arrangement as unchecked rather than
calling the clip lossless. A DOM-range selector is retained only when an
observed representation exists; with raw source alone, Turnstone demotes it to
an exact source quote when possible and otherwise records it as unanchored.
Retaining an observed DOM or layout artifact is a separate consumer-driven
extension of the same two-role contract.

Done means:

- exact bytes cross Genet, Turnstone, and Knot without being embedded in Djot;
- the authored provenance contains a carrier-independent content identity;
- a paired peer can replicate the Djot first and fetch the artifact separately;
- fetched bytes remain usable offline and fail closed on size or digest mismatch;
- selectors say which artifact they address;
- unavailable evidence storage cannot be silently advertised;
- v1 remains readable and usable.

## 2. Structure-aware replication merge

Knot's collaboration model remains git-like. Peers exchange causal document
versions; they do not share live cursors or require a session server.

Concurrent Djot edits now try a source-preserving structural merge before the
existing line merge. The parser divides the document into heading-scoped
blocks, using structure to identify paragraphs and other block content while
retaining the exact original source slices. Changes to different stable blocks
can merge even when their changed lines are adjacent. Attributes, spacing, and
other authored spelling survive because the merge chooses source slices rather
than reserializing a parsed document.

The first cut is intentionally conservative. It requires the same structural
keys in the same order in base, local, and remote versions. Insertions,
deletions, moves, and competing edits to a heading identity fall back to the
existing line merge and then to the existing conflict record if ambiguity
remains. A structural parser is useful here only when it reduces false
conflicts without inventing authorship intent.

Done means:

- edits to separate stable Djot blocks merge automatically;
- unchanged source remains byte-for-byte authored source;
- ambiguous structure changes continue through the established conflict path;
- transport, causal history, and conflict resolution semantics do not change.

## 3. Lexical lenses

Lexical analysis remains derived graph data. It does not add Djot syntax or
require a Knot file format.

Rosette now projects three configurable CMUdict-backed lenses:

- perfect rhyme, matched from the final stressed vowel onward;
- slant rhyme, matched by partial ARPAbet vowel and consonant agreement;
- meter, choosing the closest common stress-foot pattern for each line and
  reporting fit, overrun, regularity, and resolved-token coverage.

The existing pronunciation coverage report stays visible, so unknown words do
not quietly become confident analysis. Hosts can enable or disable perfect
rhyme, slant rhyme, and meter independently. All three are projections over
the authored text and can be discarded and rebuilt.

WordNet remains a good conceptual fit, but it is not part of this cut. There is
not yet a selected provider, packaged data policy, or real consumer that would
justify freezing a lexical graph vocabulary. Adding an abstract WordNet seam
now would make the architecture look finished without proving the product
interaction.

Done means:

- rhyme and meter are useful from the CMUdict data already in the tree;
- results report their own coverage and confidence limits;
- lens choice is host-configurable;
- authored Djot and replication semantics remain untouched.

## What remains deferred

Cut 4 starts only when a concrete portable consumer needs more than a Djot file
plus content references. Peer fetch and offline blob retention no longer wait
for that cut. Its questions are single-object bundling, export/import manifests,
evidence retention transitions, and whether a portable enriched object is
actually better than a document beside a content-addressed store.

That work may eventually justify `.knot`. Cuts 1 through 3 do not.
