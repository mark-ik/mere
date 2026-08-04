# Carrier independence — how many smolweb protocols run over Reticulum, and what all 17 would cost

**Date:** 2026-08-04
**Status:** analysis, answering Mark's question. No work scheduled here.
**Companion to:** the
[smolweb home decision](2026-08-03_smolweb_home_decision.md) (the grouping
rule and the three moves landed so far) and the
[Reticulum browsing plan](../../../../turnstone/design_docs/2026-08-03_reticulum_browsing_plan.md)
(the lane this would feed).

## The short answer

**Thirteen of the seventeen port to Reticulum unchanged**, because the
compatibility test is not about the protocol at all. It is one question:

> Does this protocol need a bidirectional byte stream, or does it need
> something else?

Retinue's `LinkStream` implements `AsyncRead` and `AsyncWrite`
(`crates/retinue/src/endpoint.rs:208,218`). Every smolweb protocol whose whole
shape is *open a connection, write a request line, read a response, close*
therefore runs over an encrypted Reticulum link with **no protocol changes at
all**. That is most of the small web, because that shape is the small web's
defining aesthetic.

This is not speculation. It is already proven for gemini twice over: retinue's
`gemini_over_reticulum` example serves and fetches a capsule over a real link,
and `gemini_protocol::exchange` has a test that runs the exchange over an
in-memory duplex with no TCP and no TLS.

## The categories

| | Protocols | Count | Over Reticulum |
|---|---|---|---|
| **A. Clean stream port** | gemini, gopher, Gopher+, finger, spartan, nex, titan, text, SuperText, mercury, scroll, scorpion, molerat | 13 | Works unchanged. A link is a byte stream. |
| **B. Encryption made redundant** | gophers | 1 | Degenerates: `gophers` is gopher-over-TLS, and a link is already encrypted, so the honest answer is *gopher over Reticulum*. |
| **C. Datagram-shaped** | guppy, fsp | 2 | Possible but architecturally silly: both are UDP with their own chunking, acks, and retransmission, which Reticulum already provides. Carrying them means running two reliability layers. |
| **D. Not a protocol** | terse | 1 | Corrected below: there is no wire protocol to be compatible with. |

Separately, **misfin** (mail, which we also implement) is the one genuinely
interesting case, and it belongs to none of these. See below.

Two footnotes on category A. **Mercury and scorpion are the best fits in the
set**: mercury is gemini without TLS, and scorpion makes TLS optional, so
neither has an encryption layer to make redundant. And **molerat mandates TLS**
and uses request *and response* headers with a markdown-like markup
([spec](https://molerat.trinket.icu/)), so it ports as a stream but is the most
HTTP-shaped of the group.

## Why it is nearly free, and what "nearly" costs

The mechanism is the three-layer split already shipped in `gemini-protocol`
and `gopher-protocol`:

```
<format>   always compiled, zero dependencies    the grammar
client     feature: tokio                        the exchange over ANY stream
tls / tcp  feature: rustls, ring                 the ordinary internet carrier
```

If that shape is house style, **Reticulum support is not per-protocol work**.
Thirteen protocols do not need thirteen ports; they need their `client` layer,
which they need anyway, plus one shared piece of glue.

That shared piece is the real cost, and it is **addressing**, not transport.
Every one of these protocols puts a hostname in its request line, and Reticulum
has no DNS and no host:port. A destination is a hash, optionally reached by an
announced name. So there is exactly one decision to make, once, for all of
them: how a smolweb URL's authority names a Reticulum destination. The retinue
example already demonstrates the named form, resolving `gemini://capsule/` by
recompute-and-match against announces, which is how Nomad Network addresses
nodes. That decision is [N0 in the Reticulum browsing
plan](../../../../turnstone/design_docs/2026-08-03_reticulum_browsing_plan.md)
and it is a prerequisite for all thirteen, not a per-protocol tax.

The second shared piece is **trust vocabulary**. Over TLS the identity is a
pinned certificate fingerprint; over Reticulum it is a destination hash the
link handshake proves. Both are "trust the key you actually talked to", so the
ladder maps, but the posture needs its own name rather than borrowing TOFU's,
and the fidelity plan's trust descriptor is where it lands.

## The distinction that matters: two different bridges

Conflating these is the trap, and keeping them apart is what stops broad
protocol support from turning into mush.

**1. The carrier bridge — cheap, already proven.** A smolweb protocol running
over a Reticulum link. Nothing about the protocol changes; only what carries
the bytes. Gemini over Reticulum is still gemini, and it is still gemtext at
the other end.

**2. The format bridge — expensive, and mostly a bad idea.** Translating
between a smolweb format and a Reticulum-native one. This is *not* a carrier
question, and the protocol-faithfulness doctrine says not to do it: render
micron as micron, not as lowered gemtext.

The useful observation is that the Reticulum-native formats are **analogues,
not gaps**:

| Smolweb | Reticulum-native | Relationship |
|---|---|---|
| gemtext | micron | Both are the line-oriented markup their side reads. |
| misfin | LXMF | Both are the peer-to-peer mail their side sends. |

So "misfin over Reticulum" is a question that mostly answers itself: Reticulum
already has LXMF. Porting misfin there would mean rebinding its identity model,
because misfin's sender identity *is* an X.509 client certificate, while a
Reticulum sender's identity is its destination key. That is not a carrier swap;
it is a new protocol wearing misfin's name. The same reasoning applies to
gemini's `6x` client-certificate flow, which is why category A's "unchanged"
claim covers gemini's ordinary path and not its client-cert path.

The right move where the two worlds meet is a bridge at the *message* level
(misfin ↔ LXMF), deliberately, with provenance preserved, and not a silent
transcoding.

## Correction: terse is a design sketch, not a protocol (2026-08-04)

Listing `terse` as "unsurveyed, probably category A" was a guess, and it was
wrong. Reading [TerseNet's README](https://github.com/runvnc/tersenet)
directly: it specifies **no transport, no port, no request or response
format**. The author says plainly that after two days the prototype is "a
janky RST viewer that only knows about headings and paragraphs. Not useable
for anything," and that everything beyond that "is just an idea -- not
actually coded yet."

So terse is not a protocol we could implement faithfully even if we wanted to.
It is a document-format sketch (a restricted reStructuredText subset), a URI
scheme, and a set of design intentions. It should come out of any protocol
count and be read as design kin instead.

## Design kin: what TerseNet independently arrived at

Worth recording because it is a convergence signal rather than a borrowing
opportunity. TerseNet's sketch reaches, from a standing start, most of the
architecture this workspace has been building:

| TerseNet | Ours |
|---|---|
| "Unlike Chrome or Firefox, which incorporate a full bloated operating system", a relatively simple system | The W3C knockout strategy and genet's decomposition |
| Multiple implementations by individuals or small teams, so no corporation controls the platform | Merely's posture, and why the spec crates are published separately |
| Three browser tiers: Info, Media Application (wasm), Extended Application (wasm plus device-driver-like extensions) | The engine picker's rung ladder ([engine adoption plan](../../../../turnstone/design_docs/2026-08-03_turnstone_engine_adoption_plan.md)) |
| Attachments and applications never load without explicit user selection; separate Media and Applications tabs | The participant gate, and the UA taxonomy plan's downloads and permissions rows |
| Peer-to-peer distribution with full-text search across the peer network | Turnstone's places, plus the Knot search fusion that is the intel family's first real consumer |
| Bounded pages: 5KiB text, 1KiB attachment listings, 10KB default image cap | Bounded documents suit a graph canvas, where every node is a document |
| Links only to other terse pages | A closed link graph, which is a fully navigable bounded graph for a graph browser |

None of this is adoptable, because none of it is built. It is corroboration
that the shape is a natural attractor, which is a different and cheaper kind
of useful.

## The one actionable thing: IconVG

TerseNet embeds [IconVG](https://github.com/google/iconvg) between paragraphs,
and IconVG is the part that is real, specified, and independent of TerseNet
entirely: a compact binary format for icons, logos, glyphs and emoji by Nigel
Tao, deliberately far simpler than SVG (no text, multimedia, interactivity,
scripting, animation, DOM).

**It maps onto our painting stack exactly, which is verified rather than
hoped.** IconVG's model is "define geometry from linear, quadratic and cubic
Bézier segments; define paint as flat colors or gradients; fill the geometry
with the paint." Our `paint_list_api` already carries a `Path` item built from
a Bézier command sequence plus `LinearGradient`, `RadialGradient` and
`ConicGradient` payloads, and sprigging's custom-paint `Path` leaf already has
`move_to` / `line_to` / `quad_to` / `cubic_to` / `close` / `arc`. So an IconVG
decoder needs **no new rendering capability at all**; it decodes into paint
items we already emit.

`iconvg` is available on crates.io and there appears to be no Rust
implementation (the known ones are Go).

Where it would actually earn its place is **node faces in the graph canvas**.
Nodes are content-type-coded shapes carrying favicons, and IconVG is purpose-built
for exactly that payload: vector, tiny, resolution-independent, no scripting
surface, and a fraction of the attack surface of SVG. That is a better fit for
our canvas than for any smolweb protocol, terse included.

Not scheduled. Recorded because it is the rare case where an outside format
lands precisely on capability we already have.

## Homes

The rule stays the one already recorded, and carrier independence does not
disturb it:

> **A crate's home follows who defines the spec, not what carries the bytes.**

- **smolweb workspace**: the internet-native small protocols and the formats
  their specs define. A gemini crate that can run over a Reticulum link is
  still a gemini crate.
- **retinue**: the Reticulum-native protocols and formats — micron, LXMF, node
  page and file serving. These are Reticulum facts and belong with the trunk.
- **errand / turnstone**: the composition. "Fetch this URL, over that carrier"
  is a client-integration decision, which is ours, and it is where the
  addressing adapter and the carrier choice live.

One open question worth naming rather than deciding here: errand is a
published, deliberately light crate, and retinue is a large dependency. So the
Reticulum lane is either a feature on errand or a separate bridge that
turnstone composes. The second is likelier right, but it is a real decision and
it is not urgent.

## Is broad, esoteric protocol support possible?

Yes, and the honest limiting factor is not code.

**Code volume is small and measured, not guessed.** From the three crates
landed so far: `gopher-protocol`'s client is about 130 lines and its menu
parser 245; `finger-protocol`'s client is about 180. A simple stream protocol
costs roughly 100 to 200 lines of client plus its format, with tests. Formats
are consistently more expensive than transports, which is why scroll,
scorpion, and molerat (each with a richer document format than gemtext) are the
expensive three and the remaining transports are nearly free.

**The real constraints are two.**

*Specs.* Several of these have thin, single-author, or moving specifications,
and one (terse) I could not characterise at all from public sources. A crate
published as spec-accurate cannot be written from a summary; the gopher work
already made that concrete, where an introductory page had to be abandoned for
the actual 1993 document before the wire format could be implemented.

*Testability.* There are few live servers for the obscure end of this list, so
"it compiles and round-trips against my own fixture" is most of what is
available. That is worth stating in each crate rather than implying field
verification we do not have.

**Sequencing that follows from all this**, if it is ever picked up:

1. `gophers` and `mercury` — the cheapest real wins, both nearly free beside
   crates that exist.
2. `scroll` — not for expansion but because it is a live fidelity bug: its
   nematic engine delegates to the gemtext engine although scroll defines a
   richer format, and it has no errand transport at all. It is the one item on
   this page that fixes something broken rather than adding surface.
3. `text` / SuperText — one small crate, trivially A.
4. `scorpion`, `molerat` — real work, because each brings a document format.
5. `fsp` — read the spec first; it is category C and may simply not be worth
   carrying. `terse` is off the list entirely, per the correction above.

Reticulum support, on this analysis, is not step six. It is a property the
`client` layer already gives every one of them, plus the addressing decision,
which is worth making once and early.
