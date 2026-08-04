# Smolweb Home Decision — spec-accurate to smolweb, enrichment to genet/cambium

**Date**: 2026-08-03
**Status**: decided (Mark, 2026-08-03). Records the split rule; extraction work
is not scheduled here.

## The rule

Spec-accurate implementations of smolweb protocols and their document grammars
belong in the [smolweb workspace](https://github.com/mark-ik/smolweb) as
general-use crates. The implementation-specific parts stay with us: what we do
to enrich the smolweb browsing experience, how we implement and render the
parsed content, and anything non-spec lives in genet or moves to the cambium
view layer.

This extends the movement already made on 2026-07-23, when `misfin`,
`spartan-protocol`, `nex-protocol`, and `guppy-protocol` moved to the smolweb
workspace and genet's errand went back to consuming them from crates.io. The
reasoning recorded then ("their names belong to the protocols' communities, not
to the engine that speaks them") is the same rule, stated for wire crates; this
decision applies it to everything spec-shaped still inside genet/components.

## Current state (verified 2026-08-03)

- **smolweb workspace**: misfin, spartan-protocol, nex-protocol,
  guppy-protocol. Wire layer only, per its README.
- **genet/components/errand**: the scheme-routed client integration. Delegates
  spartan/nex/guppy/misfin to the smolweb crates, but still carries in-tree
  spec implementations of gemini (gemini.rs, tls.rs, tofu.rs, titan.rs),
  gopher, and finger, plus the `errand::parse` grammars (gemtext, gopher
  menus, nex listings, RSS/Atom feed).
- **genet/components/nematic**: the AST-to-Block lowerings (capture and card
  lane). Implementation-specific by design.
- **genet/components/cambium/cambium-nematic**: the native smolweb views
  (gemtext/gopher/feed views, theming, per-site palettes). This is the
  "cambium-views" home.
- **genet/components/smolweb-views**: an empty husk; its contents became
  cambium-nematic. Delete it.

## What the rule sorts where

Extraction candidates (spec-accurate, general-use):

- Gemini transport with TLS/TOFU and titan upload, as a gemini protocol crate.
- Gopher and finger transports.
- The parse grammars: gemtext lines, gophermap items, nex listings. These are
  the protocols' document specs, so they follow the wire crates out. The
  earlier plans folded parse into errand for lockstep reasons; lockstep now
  argues the other way, since grammar and wire for one protocol belong to one
  spec crate, and errand composes them.

Stays in genet or cambium (implementation-specific):

- errand itself: scheme routing, the unified `fetch`, Status/MIME
  normalization, trust descriptors. Composition choices, not spec.
- nematic's lowerings, feed sniffing, JSON Feed handling, knot capture.
- cambium-nematic: all rendering, theming, focus/navigation affordances,
  regime-B presentation work (gopher grids, type-7 search inputs).

Open call, recorded rather than decided: RSS/Atom feed parsing is spec-accurate
but not a smolweb protocol, and it carries the quick-xml dep. It stays in
errand until an external audience or a second consumer argues for its own
crate.

## Effect on the existing plans

- [Native smolweb rendering plan](../implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md):
  its §5 crate-home diagram ("parse folds into errand", errand as a sibling
  repo) predates both the genet adoption and this decision. The two-family
  model and the render architecture stand; the crate homes read through this
  note now.
- [Smolweb fidelity plan](../implementation_strategy/2026-07-01_smolweb_fidelity_plan.md):
  WS1's AST enrichment lands wherever the grammar lives at the time; if a
  grammar has moved to a smolweb crate, the enrichment goes there. WS2 (trust)
  and WS3 (regime B) are implementation-side and unaffected.
- Both plans name pelt and meerkat as consumers. Meerkat is deleted; the app
  is Turnstone. Consumer references read as Turnstone/mere hosts.

## Findings from the first extraction (2026-08-03)

Four facts the `gopher-protocol` move established, each of which changes the
plan above.

**1. The publish gate is the real constraint.** errand is itself a published
crate (`publish = true`, 0.1.3), and a published crate may not carry git
dependencies. So errand cannot consume an extracted crate until that crate is
on crates.io. Every extraction is therefore a three-step chain with an
irreversible middle: stage the crate in the smolweb workspace, **publish it**,
then re-point errand and bump it. Staging is free and reversible; the publish
is Mark's call and gates everything downstream of it.

**2. `gemtext` is taken on crates.io** (v0.2.1, "A gemini client and server for
Rust", last published October 2020), so the shared grammar cannot have the
obvious name. It does not need one: gemtext and gemini are defined by the same
spec document, so the grammar bundles into `gemini-protocol` behind a
dependency-free feature, and consumers that only parse bodies (spartan, guppy,
scroll, misfin) take it with `default-features = false`. `gemini-protocol` and
`gopher-protocol` are both available.

**3. The nex grammar is duplicated, not missing.** `nex-protocol` already ships
a listing parser (`listing.rs`, `parse_listing` into `ListingLine`), and errand
independently implements the same grammar in `parse/nex.rs` with directory
detection and base-URL resolution on top. That move is a de-duplication:
upstream errand's extras into `nex-protocol` and delete the copy. Decide which
implementation is the survivor by reading both, rather than assuming errand's
is richer because it is longer.

**4. Feature-gate the transport, not the grammar.** Verified on gopher: with
`default-features = false` the dependency tree is the crate alone, zero
dependencies, while the client pulls tokio and url. This matters because the
consumers of these grammars are *renderers* (cambium-nematic parses gophermaps
to draw them) that have no business pulling an async runtime. Every extracted
crate should follow this shape.

**Blast radius is small and shieldable.** Only genet consumes `errand::parse`:
four nematic engines (`gemtext.rs`, `gopher.rs`, `nex.rs`, `feed.rs`) and
`cambium-nematic/src/views.rs`. mere and turnstone reference it nowhere. errand
can re-export each extracted grammar under its existing `errand::parse::*` path,
so those five files need no change at all and the moves stay invisible
downstream.

## Sequencing

Extraction is deliberate, not urgent, and now has a fixed shape per protocol:
stage in the smolweb workspace (free, reversible), publish (Mark), re-point
errand behind a re-export (invisible downstream), bump errand.

Do it protocol-by-protocol when a lane is next touched, not as a sweep. One
ordering correction: doing a move **before** the fidelity plan's WS1 touches a
grammar is better than after, because WS1's enrichment then lands in the
grammar's final home instead of needing a second move. WS1 has not started.

## Progress

- **2026-08-03**: Decision recorded. `gopher-protocol` staged in the smolweb
  workspace as the first move and the template for the rest: `menu` (the RFC
  1436 parser, dependency-free, always compiled) plus `client` (TCP fetch,
  default feature), with its own `Response`/`ClientError` mapped by errand the
  way `spartan-protocol` already is. 11 tests plus doctests green, 7 of them
  under `--no-default-features` with an empty dependency tree; the smolweb
  workspace is green. Not published, so errand still carries its own copy and
  nothing downstream has changed yet.
- **2026-08-03 (later)**: **The first two moves are complete, end to end.**
  Mark authorised publishing and asked that the successors be supported in the
  same crates, so both went out carrying them:
  - **`gopher-protocol` 0.1.0**, published, now with **Gopher+**: the
    fifth-field marker, the response header (`+<count>`, `+-1`, `+-2`, `--1`),
    attribute blocks, `+VIEWS` alternates, and `+ASK` forms. Gopher+ is a
    superset, so it is modelled as one: a plain RFC 1436 menu is simply a menu
    with no markers. Gopher-II is left unimplemented and said so.
  - **`finger-protocol` 0.1.0**, published, with **WebFinger** (RFC 7033)
    beside RFC 1288. WebFinger ships as request construction plus JRD parsing
    and *no HTTP client*: the GET belongs to the caller, which keeps the crate
    light and lets errand take the classic protocol alone.
  - **errand 0.2.0** consumes both. `gopher.rs`, `finger.rs`,
    `parse/gopher.rs` and `plain.rs` fell from roughly 500 lines to 89 of
    mapping. `parse::gopher` re-exports the grammar, so nematic and
    cambium-nematic needed **no edits**, which is the re-export shield working
    as designed.

  Two things worth keeping. The successor support was landed *before*
  publishing rather than after, because Gopher+ adds a field to `GopherItem`
  and shipping 0.1.0 without it would have meant breaking the API within the
  day. And the ordering rule above held: the grammar reached its final home
  before anything enriched it.

  **A silent bypass surfaced during the re-point.** `cambium-nematic` declared
  `errand = "0.1.3"` as a bare version rather than `workspace = true`, so it
  resolved to the *published* errand instead of this workspace's copy: local
  errand changes never reached those views, and its tests were quietly
  exercising crates.io. Fixed, and the lock now holds one errand instead of
  two. Worth a sweep for other bare-version declarations of workspace members.

  errand is not republished; genet builds it by path, so only an external
  consumer would need that.
- **2026-08-03 (third move)**: **`gemini-protocol` 0.1.0**, published, carrying
  gemini, the gemtext grammar, TOFU pinning, and titan. errand's `gemini.rs`,
  `tls.rs`, `tofu.rs`, `titan.rs` and `parse/gemtext.rs` left; the crate is
  down from 2786 lines to 1612. Downstream needed no edits again: the grammar
  re-exports under `errand::parse::gemtext` and the TOFU types under
  `errand::`, so the hosts that install a trust store still call
  `errand::set_trust_store(errand::InMemoryTofu::new())`.

  Two fidelity gains. Gemini's temporary (`4x`) and permanent (`5x`) failure
  are now **distinct** in the spec crate, since retrying one is reasonable and
  retrying the other is not; errand still flattens them to its cross-protocol
  `Failure`, but that flattening is now an explicit, tested mapping rather than
  a silent one, and the exact code survives on `raw_status`. An undefined
  status class is refused rather than passed along.

## The grouping rule (Mark, 2026-08-03)

Group associated protocols and formats in one crate; keep the architectural
boundary at *intent*, not at *artifact count*. Three shapes have come up, and
they resolve differently:

- **A successor that is a superset** lives in the same crate as its ancestor,
  because splitting them would make every consumer reassemble one protocol
  from two dependencies. Gopher and Gopher+ are one crate; a plain RFC 1436
  menu is simply a Gopher+ menu with no markers.
- **A format its protocol defines** lives with that protocol. Gemtext is not a
  format that happens to travel over gemini; it is the format gemini's spec
  defines, and titan is the write half of the same document space. One spec,
  one crate.
- **A successor that is a different protocol answering the same question**
  gets its own feature in the same crate. WebFinger is HTTP and JSON where
  finger is a line of text over TCP, so it ships beside finger but behind
  `webfinger`, and errand takes the crate without it.

What makes the grouping safe is that **the cost is feature-gated, not
crate-gated**. Every grammar is dependency-free and always compiled; every
transport rides a feature. So `gemini-protocol` with
`default-features = false` is a zero-dependency gemtext parser, which matters
because **five other smolweb formats serve `text/gemini` bodies** (spartan,
guppy, scroll, misfin, titan) and none of them should pull a TLS stack to
read one. Verified per crate, not assumed: the dependency tree in that
configuration is the crate alone.

The corollary for our side: fifteen nematic engines is not fifteen crates.
Most of those fifteen are *lanes over shared grammars*, and the lane is ours
while the grammar is the spec's.

## Expansion: what else is out there (surveyed 2026-08-03)

From [dbohdan's small-internet roundup](http://dbohdan.sdf.org/smolnet/) and
the [ArchiveTeam SmolNet page](https://wiki.archiveteam.org/index.php/SmolNet),
the protocol set is: gemini, gopher, Gopher+, gophers, finger, spartan, text,
SuperText, nex, scorpion, mercury, titan, guppy, scroll, molerat, terse, fsp.

**We have nine**: gemini, gopher, Gopher+, finger, spartan, nex, titan, guppy,
plus misfin for mail.

**Not yet spoken**, with the shape each would take under the grouping rule:

| Protocol | Shape | Note |
|---|---|---|
| `gophers` | a feature on `gopher-protocol` | gopher over TLS; the successor-is-a-superset case again |
| `mercury` | its own crate, or a feature beside spartan | gemini without TLS, so it is close kin to spartan |
| `scorpion` | its own crate, protocol **and** format | richer document format than gemtext, and does not mandate encryption |
| `scroll` | its own crate, protocol **and** format | see the gap below |
| `text`, SuperText | one small crate | simpler than gemini, no interactivity |
| `molerat`, `terse`, `fsp` | unsurveyed | read the specs before committing |

**A fidelity gap this survey exposes.** Scroll and scorpion each define a
document format *richer than gemtext*, and nematic's `scroll` engine currently
delegates to `GemtextEngine`. So scroll content is being flattened into
gemtext's line model, which is exactly the collapse the
[fidelity plan](../implementation_strategy/2026-07-01_smolweb_fidelity_plan.md)
exists to catch, and it is a parse-layer loss of the kind that plan's §1
identifies. Note also that scroll has a nematic engine but **no errand
transport**, so it is currently a format we half-read and cannot fetch.

**Still open**: the nex de-duplication. `nex-protocol` already ships a listing
parser (`listing.rs`, `ListingLine`) and errand independently implements the
same grammar with directory detection and base-URL resolution on top
(`parse/nex.rs`). Unlike the moves so far this is not a lift-and-shift: it
needs a decision about which shape is the survivor on an already-published
crate, so it wants a deliberate read of both rather than a rushed unification.
