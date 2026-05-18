# Mere

A prototype spatial browser for the web, where webpages and content are nodes in a force-directed graph.

Mere's primary UI is a force-directed graph canvas,
in which each node is a webpage or piece of content,
and edges are user-created or inferred relationships.
Mere's core is a petgraph-powered graph engine that manages the graph data structure,
and modularized rendering engines that project the graph onto a tiling workbench surface.

This is a Cargo workspace organizing Mere into modular crates. Most crates ship as `0.0.x` reservations at this stage; implementation is in progress.

## Why 'spatial'

Your browser history in every other web browser is a flat list; mere's intent is to add a dimension
by structuring your history on both a per-node (a history-tree inspired branchable node lineage) 
and per-graphlet basis (edges, internodal relationships).

By implementing a spatial browser graph (based on petgraph),
we also create two multiplexer opportunity in the browser space: interface and protocol.
Your classic browser tabs => tiles in componentized, composable tile-trees (`forme`) projected from graphlets.
Want two tiles together in a split? Drag them, in window, and the `frametree` will manage an arbitrary number of nested splits.

But keeping track of all of it, at the base of all the tiles and panels and panes, is the orrery,
the root graph view of the browser's history and relationships.
It's the context in which we support arbitrary protocols and content engines as first-class citizens.

## Protocol...s?

Crucially, mere is protocol agnostic in a way that no other browser is.
mere's graphlet structure allows it to support arbitrary content engines and protocols as first-class citizens.
This means that in mere, you could have a Gemini node and an HTTP node side by side in the same graphlet,
and interact with them using the same spatial interface and tile tree. Exciting, no?

In addition to planning to support the fullweb (HTML, HTTPS, JS, CSS, all the goods in time),
smolweb protocols ought to be a faithfully rendered communication substrate of the web.
smolweb protocols are designed to be simple, human-friendly, and privacy-respecting,
and they're covered by mere's in repo modular content engine, Nematic (pick the protocols you want!),
supporting gemtext and gemini:// alongside Gopher, Scroll, Finger, Misfin, Titan, RSS/Atom, and more. You yourself can serve and manage a Gemini capsule today,
and your browser can and should help you do that. Mere will, by gum.

## P2P and communities?

Speaking of things I'll do, I'll support IPFS with Iroh,
and use its authenticated transport for p2p comms (and co-op browsing!) in `murm`, our chat protocol supercrate.

I'll also support p2p community (moots) formation in `moothold`, our community/federation supercrate.
The goal is to let folks use the same spatial interface and tile tree to interact with communities,
with a core primitive being pinning: like bittorrent seeding, but for community content and activity.
I'm exploring willow and other options, but I'm letting the cart get ahead of the horse here.

The point is, we want to support the **fullest** web, and that includes the smolweb, p2p communities, and bilateral comms protocols. So that's the plan.

## Crates

| Crate | Role |
|-------|------|
| [`mere`](crates/mere/mere) | The browser product crate — entrypoint and surface composition |
| [`mere-identity`](crates/mere/mere-identity) | Identity management — master Ed25519 keypair, keychain integration, per-protocol derivation |
| [`mere-transport`](crates/mere/mere-transport) | Peer transport layer — iroh-based authenticated streams between known peers |
| [`graphshell`](crates/graphshell) | Portable shell layer — host GUI integration (iced / gpui / html-css / other) and Navigator surface |
| [`graphshell-core`](crates/graphshell/core) | Portable Graphshell identity, authority, graph, pane, and shell-state contracts |
| [`graphshell-runtime`](crates/graphshell/runtime) | Portable runtime adapters, frame projections, host ports, and surface schedule application |
| [`graphshell-host`](crates/graphshell/host) | Host-side adapters and toolkit ordering for fresh Graphshell surface hosts |
| [`graphshell-host-iced`](crates/graphshell/host-iced) | Fresh iced host boundary — queues surface lifecycle requests without old GraphBrowserApp mutation assumptions |
| [`verso-tile`](crates/verso-tile) | Rendering-surface management — receives engine output and places it into GraphTree tiles (Verso brand) |
| [`inker`](crates/inker) | Engine controller — selects and orchestrates content engines |
| [`platen`](crates/platen) | Composition surface — graph-aware layout, the press to verso's page |
| [`nematic`](crates/nematic) | Smolweb engine — Gemini, Gopher, static HTML, Markdown, RSS/Atom |
| [`murm`](crates/murm/murm) | Bilateral peer-to-peer comms supercrate |
| [`murmuring`](crates/murm/murmuring) | Protocol core for bilateral chat-protocol selection — inner layer of `murm` |
| [`moothold`](crates/moot/moothold) | Community / federation supercrate (will switch to `moot` if that name frees up) |
| [`mooting`](crates/moot/mooting) | Protocol core for community social-primitives selection — inner layer of `moothold` |

## Workbench: printing-press metaphor

Engines (Wry, Serval, Nematic) produce content. The **inker** pairs each engine to its content. The **platen** composes the layout. The **verso** receives the impression. The user sees the printed result in the surface that `mere` composes atop `graphshell`. **Mnem** keeps the impressions over time. **Murm** carries one-to-one comms; **moothold** carries community/federation.

## Status

Pre-1.0 development. The 0.0.x releases reserve crate names and document intent; implementation is in progress.

## License

Licensed under the [Mozilla Public License 2.0](LICENSE-MPL).
