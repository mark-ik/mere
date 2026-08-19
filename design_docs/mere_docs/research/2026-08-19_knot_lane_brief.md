# The knot lane: clipping, collaboration, and lexical lenses

Status: thinking brief, commissioned by Mark 2026-08-19. Nothing here is
ruled. The clipping section is the most developed and carries one open question
that blocks part of it; the plugin section is triage, not a roadmap.
Date: 2026-08-19
Scope: what knot is as a product lane, what already ships toward it, and what
the clip format has to be. Written against the current tree only.
Sources: the tree, cited by path. See "Provenance" at the foot for why no
recovered graphshell-era document is cited as authority.

## The lane

Knot is a djot editor on a graph substrate with peer-to-peer replication. The
claim worth testing is that a markdown editor combining p2p collaboration, text
clipping, and lexical analysis does not currently exist, despite the number of
markdown editors.

Held loosely, and deliberately not load-bearing below. Anytype is local-first
and peer-adjacent without being markdown-native; Obsidian and Logseq sync
through servers; the Yjs-based editors assume a websocket host. The gap looks
real. It has not been surveyed properly, and no decision in this brief depends
on it being real.

A narrower validation signal is available now: Mark runs naming rounds
continually and keeps an aesthetic word list. The lexical lenses in Part III
are a naming instrument he would use weekly. A tool whose author uses it every
week is worth more than a market read.

## What already ships

Verified in the tree, because most of this lane is further along than it looks.

- **Clipping, v1.** `ports/knot/src/endpoint.rs:1493` implements `insert_clip`.
  Its payload is `InsertKnotClipV1` at `crates/chirograph/src/lib.rs:905`:
  `{base_token, source_url, title, selector: Option<String>, knot_body}`.
  Provenance is written in-band as a fenced `knot.clip.provenance` block of
  JSON. The intent and schema constants are `knot.clip.insert` and
  `knot.clip.insert/v1`.
- **Replication with merge.** `ports/knot/src/sync.rs` is 2299 lines of
  "causal personal and Commons document replication over Stickleback", with
  `KnotDocumentConflict`, `KnotAutomaticTextMerge`, a `resolve_conflict` API
  and a `ConcurrentWriter` error. There is no CRDT crate anywhere in the tree.
  The model is deliberate: it collaborates like git, not like Google Docs.
- **Document provenance.** `inker/src/document.rs:118` defines
  `DocumentProvenance { source_kind, canonical_uri, fetched_at, source_label }`,
  engine-agnostic and already populated for every lane.
- **Content addressing.** `eidetic-core/src/schema.rs:94` defines `Hash` as
  BLAKE3-256 with string form `<fn>:<hex>`. `eidetic-iroh-fetcher` resolves
  `BlobSource::Iroh { ticket }`, parsed as `<node-id-hex>/<blob-hash-hex>`,
  fetching the blob from the named peer.
- **Djot.** jotdown 0.10 in genet's illume, nematic and knot-editor-host.
  Nematic ships sixteen engines including `knot` and `knot-djot`.
- **Phonology.** `crates/intel/mora-cmudict` carries the full CMU dictionary,
  135,010 entries, permissively licensed, in ARPAbet with stress digits
  (`'bout B AW1 T`). The crate currently exposes only `Cmudict` and
  `CmudictError`; there is no analysis layer yet.

## Part I: clipping

### The distinction it turns on

Djot is an authoring format. Clipping needs an observation format.

An authored document is authoritative. The author's intent is the document and
there is nothing behind it to be unfaithful to. A clip is a claim about
something else: this is what that page said, at that moment, as well as I could
represent it. Everything the format needs beyond djot follows from that gap.

### The load-bearing decision: keep the bytes

Retain the artifact the lowering ran over, content-addressed in eidetic. State
this first, because the rest of the design is downstream of it.

Without retained bytes the fidelity ledger below is a set of unverifiable
assertions, the document body fills with raw HTML, and anchors point at a live
URL that moves. With them the ledger is checkable, the body stays clean djot,
and a lossy region can be re-rendered later, including one the lowering failed
to flag.

**Retention is a policy, not a permanent commitment.** The W3C
`TextQuoteSelector` carries prefix and suffix precisely so a quote can be
re-found in a changed document, so it resolves against the live page too.
Dropping the archive degrades anchoring rather than breaking it. What is lost
on release is narrower: ledger verifiability, re-rendering, and a stable target
when the page moves.

So retained bytes are load-bearing *at capture time*, because that is when the
ledger is verified. Verify once, record that it was verified, then release
under policy. The envelope records the transition, archive retained at hash X
versus archive released on date D, and the clip demotes from evidence to claim
and says so. Content addressing helps the storage arithmetic, since repeated
clips of one site share bytes.

### The anchor invariant

**Selectors anchor into whatever artifact the lowering actually ran over.**

For HTML that is the post-script DOM, because that is what was observed. The
raw HTTP response is a different artifact answering a different question: what
the server said, and the only one a third party can independently re-derive.
Store both where practical, but only one is the anchor and the format must name
which.

### The envelope

Four parts. Two are already half-built.

**Provenance** extends `DocumentProvenance` rather than defining a new struct.
It already carries `source_kind`, `canonical_uri`, `fetched_at` and
`source_label` for every lane. It gains HTTP status, content-type, and capture
method.

**Anchor** replaces `selector: Option<String>` with W3C Web Annotation Data
Model selectors: `TextQuoteSelector` with prefix and suffix, `RangeSelector`,
position selectors. Borrowed rather than invented, and interoperable with
everything else that annotates the web.

**Fidelity ledger** records what the lowering could not represent. HTML into
djot is lossy and always will be; the choice is whether the loss is silent or
recorded, which is livery's loud-diagnostics posture applied to content. A clip
that says a table with merged cells was here and could not be represented is
worth more than one that quietly drops it, because the reader knows the shape
of what is missing and tooling can offer to re-fetch. It also turns fidelity
from an aspiration into a measurable property.

Entries point into the retained artifact, not into the body. That placement is
what resolves the format's one real insufficiency, described under the export
constraint below.

**Discovered edges** are captured and labelled observed and unverified. A clip
is a user selection, so its link set is not what another party re-deriving from
the source would produce. It therefore cannot accumulate corroboration under
any scheme that works by independent re-derivation. That is a property of
selections. It binds nothing today, but the labelling costs nothing now and
prevents the graph accumulating edges with a confidence they never earned.

### The two constraints that keep it cheap

**The body stays actual djot.** Not djot-like. Then export is not a conversion,
it is discarding the envelope. Anything djot cannot say goes in a raw block
with a ledger entry, never in new syntax.

**The export constraint is tighter than it first appears.** Export being free
does not merely require adding no syntax djot lacks. It requires using only the
commonmark subset of djot. From jotdown 0.10, djot's containers include `Div`,
`Span`, `Section`, `Insert`, `Delete`, `Mark`, `Subscript`, `Superscript`,
`Math`, `Symbol`, `DescriptionList`, `Caption` and `Attributes`. Commonmark has
none of them, so a djot-to-commonmark lowering is itself lossy, and by this
brief's own principle would need its own ledger.

The natural way to bind a ledger entry to a place in the body would be a djot
attribute or span, and those are exactly what die at that boundary. Putting
ledger locations in the retained artifact instead means the body carries no
markers at all, so nothing has to survive export. This is the strongest single
argument for retained bytes and the reason it is stated as load-bearing.

Note also that djot *has* tables, so a plain table is not a loss. The real
table losses are colspan, rowspan, nesting, and block content in cells.

### Fidelity classes

The ledger catches omission: what the lowering skipped. It is blind to
arrangement: a two-column layout flattened to sequential prose, a sidebar that
was semantically an aside, grouping expressed only in CSS, DOM order overridden
by flex or grid `order`. Every node is handled, nothing is skipped, and the clip
is still unfaithful.

Detecting that needs computed layout, which means the lowering must run after
style resolution. That is not available everywhere and should not be forced to
be.

**Nematic cannot supply it, by charter rather than by cost.** Nematic's
`src/html.rs` is a reader-mode fragment engine whose header states that
scriptable elements, frames, forms, styling authority, event handlers and
active URL schemes never reach the portable document model. Styling authority
is deliberately excluded. Nematic could link a layout engine behind a feature,
exactly as it already gates `html-fragment` on `genet-static-dom` with
`default = ["html-fragment"]`, but it should not, because arrangement is
outside what that engine is for.

So arrangement fidelity comes from the genet.web path and nowhere else. This
yields two fidelity classes, and the envelope's capture-method field already
distinguishes them: full page versus reader extraction versus user selection. A
reader-mode clip reports arrangement as **not applicable**, never as zero,
because zero implies it was checked.

The ledger *schema* stays engine-agnostic so a gemtext clip and an HTML clip
produce the same shape. Gemtext has no arrangement to lose and its ledger is
correctly empty.

### Sharing

A shared clip carries a `BlobSource`, not the archive. The envelope names the
retained bytes by BLAKE3 hash and the recipient fetches from the clipping peer
over iroh, using machinery `eidetic-iroh-fetcher` already implements.

The honest limit: this is peer-dependent. An offline clipper leaves the
recipient holding the claim without the evidence. That is a real property to
state in the design rather than smooth over, and it is the actual answer to the
worry that a shared file loses its evidence.

**This is the open question that blocks the section.** Which surface carries a
shared knot document today is unverified. `crates/murm` and
`crates/moot/moothold` both exist and neither has been checked for this. The
sharing design is guesswork until that is read against the tree.

### The actionable part

`knot.clip.insert/v1` to `/v2`, as field additions on `InsertKnotClipV1`. The
existing five fields stay; `selector` changes from an opaque string capped at
4096 bytes to a structured selector. Everything else is addition, so v1 clips
remain readable.

## Part II: collaboration

### What is missing

`ports/knot/src/sync.rs:1256` merges `title` and `media_type` as scalars, then
hands the body to `merge_text_lines` operating over
`LineEdit { start, end, replacement }`. That is line-level three-way merge, the
same granularity git uses. Two people editing different paragraphs inside one
section can still collide on adjacent lines.

### The move

Merge djot at its own structure: `Section`, `Heading`, `Div`, `Paragraph`,
which jotdown already parses. Two people editing different sections of one
document then never conflict at all.

This is the technique weave uses on code, pointed at prose. It is not weave as
a dependency and not weave as a plugin, for three reasons:

1. weave is a git merge driver, invoked by `git merge` on a working tree. Knot
   replicates over stickleback and has no git in that path.
2. weave has no prose entity model to lend. `.gitattributes` does map
   `*.md merge=weave`, but `skip_sesame()` at `weave-core/src/merge.rs:3734`
   lists `.md` and `.markdown` among extensions that *skip* the entity engine,
   grouped with `.json`, `.txt` and `.svg`. Prose already falls back today.
3. Therefore the reuse runs backwards. If block-granular djot merge lands as
   its own crate, knot's sync and weave's driver can both consume it, and weave
   gains markdown entity merge it does not currently have.

The same holds for sem. Its questions transfer well to prose (what changed in
this section, who wrote this paragraph) but it queries git history, and knot's
history is stickleback. That would be a sibling implementation over knot's
causal log, not a dependency.

### Why the git model rather than live cursors

Live concurrent rich-text editing is the hardest unsolved problem in this
space, which is why Peritext remains research. The tree has already committed
to the other model, and it is the better product bet: the pain it solves is
current and widespread, since file-level sync tools conflict destructively on
notes today.

## Part III: lexical lenses

### The principle

These are not features, they are lenses over one graph. Clipping brings outside
text in with provenance. WordNet brings a lexical graph in. mora layers
phonology over words already present. Weaving is how two people's edits
reconcile. They compose because they are all the same substrate.

The discipline that makes clipping cheap has an exact analogue here: **every
lens emits envelope-shaped data, never new djot syntax**. The export path stays
free no matter how many land.

### WordNet: the strongest candidate

Permissively licensed, carrying definitions, synsets, and hypernym, hyponym and
meronym relations.

The reason it fits is not that it is a dictionary. It is that **WordNet is a
graph**, and knot is a graph editor on a graph substrate. It arrives as a
lexical graph navigable in the same model as the user's notes, which also
delivers semantic analysis from the same artifact rather than as a second
feature.

Unverified and not load-bearing: the iOS Advanced English Dictionary is
believed to be WordNet-backed. The case for WordNet does not rest on that.

### Prosody, and rhyme

The data layer exists. `mora-cmudict` is ARPAbet with stress digits, so one
dataset already in the tree yields three lenses:

- **Meter** from the stress digits (0 unstressed, 1 primary, 2 secondary).
- **Perfect rhyme** by matching phonemes from the last stressed vowel onward.
- **Slant rhyme** by partial phonetic match: assonance on vowels, consonance on
  consonants, with a distance metric over the ARPAbet inventory.

Rhyme is the cheapest useful thing on this list, and slant rhyme costs only a
similarity function over the same index. Nothing here needs a new data source.

### Parked, with the reason

**Etymology and word-parts.** Both are synergistic with prosody, and for
poetry, songs, or reading the holistic sense of one's own prose they matter as
much as rhyme does. They are parked on licensing, not on value. Etymonline is
proprietary. Wiktionary and the Etymological Wordnet are CC BY-SA, and
share-alike over bundled data in a shipped product is a commitment rather than
a footnote. CELEX is expensively licensed.

The route that avoids the question is to build rather than bundle: a
morphological decomposer over affixes and roots, seeded from permissive
sources. How deep such a decomposer can get is the open question, and it is a
research task rather than an implementation one. Worth scoping separately.

**Grammar analysis.** Same family of value, parked for effort rather than
licensing. LanguageTool is LGPL Java and awkward to embed, and Rust-native
grammar checking is thin.

## What this brief does not settle

1. Which surface carries a shared knot document. Blocks the sharing design.
2. Whether the arrangement ledger is in the first cut or deferred, given it
   requires the genet.web path rather than nematic.
3. How deep a home-built morphological decomposer can go, and whether that
   depth justifies the work.
4. Whether block-granular djot merge is a knot-internal module or a crate that
   weave can also consume. The second is more valuable and more work.

## Provenance

No recovered graphshell-era document is cited as authority here. Verse, VGCP,
VDIP, verso and middlenet are retired or were never implemented, and earlier
drafts of this reasoning leaned on VGCP's edge model as though it were binding.
That reasoning has been restated on its own merits under "discovered edges"
above. Recovered material remains good mining ground and bad authority.

Written with AI assistance (Claude).
