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

## Sequencing

Extraction is deliberate, not urgent: each move is a crates.io publish plus a
genet re-point, and gemini's TOFU seam is mid-flight in the fidelity plan's
WS2. Do it protocol-by-protocol when a lane is next touched, not as a sweep.
