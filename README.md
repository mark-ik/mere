# Mere

A spatial browser for the web — knowledge as a graph, browsing as etching.

Webpages are nodes in a force-directed graph rendered on a tiling workbench. Paths cut through the substrate by repeated traversal accrue meaning: each user wears their own holloways through the web, each community wears its shared ones.

This is a Cargo workspace organizing Mere into modular crates. Most crates ship as `0.0.x` reservations at this stage; implementation is in progress.

## Crates

| Crate | Role |
|-------|------|
| [`mere`](crates/mere/mere) | The browser product crate — entrypoint and surface composition |
| [`mere-identity`](crates/mere/mere-identity) | Identity management — master Ed25519 keypair, keychain integration, per-protocol derivation |
| [`mere-transport`](crates/mere/mere-transport) | Peer transport layer — iroh-based authenticated streams between known peers |
| [`graphshell`](crates/graphshell) | Portable shell layer — host GUI integration (iced / gpui / html-css / other) and Navigator surface |
| [`graphshell-host`](crates/shell/graphshell-host) | Host-side adapters for Graphshell surface lifecycle seams |
| [`verso-tile`](crates/verso-tile) | Rendering-surface management — receives engine output and places it into GraphTree tiles (Verso brand) |
| [`inker`](crates/inker) | Engine controller — selects and orchestrates content engines |
| [`platen`](crates/platen) | Composition surface — graph-aware layout, the press to verso's page |
| [`nematic`](crates/nematic) | Smolweb engine — Gemini, Gopher, static HTML, Markdown, RSS/Atom |
| [`murm`](crates/murm/murm) | Bilateral peer-to-peer comms supercrate |
| [`murmuring`](crates/murm/murmuring) | Protocol core for bilateral chat-protocol selection — inner layer of `murm` |
| [`moothold`](crates/moot/moothold) | Community / federation supercrate (will switch to `moot` if that name frees up) |
| [`mooting`](crates/moot/mooting) | Protocol core for community social-primitives selection — inner layer of `moothold` |

## The printing-press metaphor

Engines (Wry, Serval, Nematic) produce content. The **inker** pairs each engine to its content. The **platen** composes the layout. The **verso** receives the impression. The user sees the printed result in the surface that `mere` composes atop `graphshell`. **Mnem** keeps the impressions over time. **Murm** carries one-to-one comms; **moothold** carries community/federation.

## In-product lexicon

Vocabulary used in user-facing language:

- **moot** — a single persistent themed federatable graph-view community
- **demesne** — a federation of moots (sovereign cluster)
- **suzerainty** — the relation between a demesne and its constituent moots
- **volvelle** — a moot expanded radially in the Navigator (medieval rotating-disc knowledge instrument)
- **astroid** — internal UX vocab for graphlet hub-collapse
- **tessera** — trust / contribution / reputation token, validated across demesnes
- **engram** — canonical portable contribution payload
- **flora** — the body of engrams composing a moot's geist
- **mnem** — your private local accumulated browsing memory
- **kith / kin** — contact tier distinction; orthogonal to moot membership
- **orrery** — internal term for the root graph view (where the whole knowledge-cosmos spins at once)

## Status

Pre-1.0 development. The 0.0.x releases reserve crate names and document intent; implementation is in progress.

## License

Licensed under the [Mozilla Public License 2.0](LICENSE-MPL).
