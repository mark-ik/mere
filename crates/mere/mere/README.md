# mere

`mere` is a tiling web browser built on a force-directed graph canvas. Think of Obsidian's graph view, but the nodes are live webpages, files, and notes instead of only static markdown files.

---

A spatial browser. Your tabs live as a map you can arrange, save, and share, instead of a strip at the top of the window.

Tabs are bad at memory. A graph remembers structure: how things connect, what you were doing, where you came from.

Point by point:

Spatial: force-directed graph canvas
Tabs: tiling workbench
Arrange: graphlets (subgraphs) grow as you browse and can be rearranged, tagged, filtered...
Save: versioned, forkable per-node and per-graph histories
Share: optional, P2P permissions-based co-browsing

Long-term, it's a "knowledge user agent" for the information age: the browser crawls and organizes on your behalf instead of passively rendering.

---
AI Disclaimer

To be clear: I've used AI 'cause thinking about this project for a decade got old. Nothing like trying a project to learn about it. And maybe one more turn of the experiment wheel will unlock new perspectives in others on what a browser can be.

The project will stay open source, but I don't intend to bother upstream repos and projects. I am responsible for the code, AI or not. 

---

`mere` is the **product crate**: the eventual entrypoint that composes the
rest of the mere crates into a working browser. The 0.0.1 release reserved
the crate name and maps the ecosystem; implementation is in flight in the
[mere workspace](https://github.com/mark-ik/mere).

## The ecosystem

The architecture follows a printing-press metaphor: protocols encode content,
the inker pairs each engine to its protocol to render, the platen composes the layout,
verso receives the impression on a workbench tile. eidetic keeps the big picture over time.
Peer / social layers ride alongside.

### Shell + presentation

- [`graphshell`](https://crates.io/crates/graphshell) — portable shell layer
  (workbench, Navigator, tile tree) and host-GUI integration contracts.

### Printing-press stack

- [`inker`](https://crates.io/crates/inker) — engine controller; routes URIs
  and content types to the right engine.
- [`platen`](https://crates.io/crates/platen) — graph-aware composition;
  arranges what gets pressed where.
- [`verso-tile`](https://crates.io/crates/verso-tile) — tile rendering
  surfaces; receives the impression and places it into GraphTree slots.

### Engines

- [`nematic`](https://crates.io/crates/nematic) — smolweb engine (Gemini,
  Gopher, Markdown, RSS/Atom, …). Full-web rendering rides on a separate
  Servo/wgpu fork through `inker`.

### Local memory

- [`eidetic`](https://crates.io/crates/eidetic) — owner-scoped private memory
  (graph snapshots, traversal logs, settings, caches). Storage backends are
  pluggable.

### Peer substrate

- [`mere-identity`](https://crates.io/crates/mere-identity) — master Ed25519
  keypair, OS-keychain integration, per-protocol identity derivation.
- [`mere-transport`](https://crates.io/crates/mere-transport) — iroh-based
  authenticated QUIC streams between known peers.

### Bilateral comms

- [`murm`](https://crates.io/crates/murm) — supercrate; one-to-one (and
  small-group) messaging across pluggable protocols.
- [`murmuring`](https://crates.io/crates/murmuring) — protocol core; the
  modular layer that hosts concrete chat protocols (Cable in Phase 2B; MLS,
  Tox, others later).

### Community + federation

The community substrate forms a four-tier scale: orrery (t1, personal) →
moot (t2, themed community) → moothold (t3, federation of moots) →
demesne (t4, sovereign coalition of mootholds). All four tiers are
forkable. Hosting is voluntary — content stays online while pinners care
to keep it online; when it lapses, members' eidetic copies can re-host.
Full design in
[`design_docs/mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md).

- [`moothold`](https://crates.io/crates/moothold) — federation-of-moots
  crate (t3); also currently hosts t1 (orrery) and t2 (moot) lifecycle.
  Owns the persistent shared graph view, member capabilities (meadowcap-
  shaped), pin tracking, tessera, engram flora, and ILL-shaped
  reciprocity at federation tiers.
- [`mooting`](https://crates.io/crates/mooting) — protocol core for
  moot-internal coordination over MereEvents, plus thin client
  orchestration for foreign-protocol resources linked from a moot.
  Foreign protocols stay themselves; the moot links to and stores them
  rather than translating them into a unified internal abstraction.

## Naming

*Mere* draws on three meanings simultaneously: the adverb *merely* (humble,
"merely a browser!"), the noun *mere* (a small lake, a still surface
where things accrue and reflect), and a slant rhyme with *mirror*. Together
they frame the product as a quiet reflective surface for browsing the smolweb and the wider internet.

## Status

Pre-1.0. The 0.0.x crate is a name reservation and ecosystem map — it
currently exposes only `VERSION` and `STAGE` constants. The first composing
release will land once the host adapter and reducer wiring stabilize across
the underlying crates.

## License

MPL-2.0.
