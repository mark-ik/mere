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

  **Still open**: the nex de-duplication, and gemini (with gemtext bundled).
  errand is not republished; genet builds it by path, so only an external
  consumer would need that.
