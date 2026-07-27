# Mere

Mere is the library behind a graph-first browser. Your history and content become
nodes in a spatial graph: html pages, gemini capsules, local media, and notes sit
side by side in one canvas, joined by user-made and inferred relationships. Mere
composes that world (graph truth, arrangement, persistence, retrieval, identity,
comms) and hands it to a host; the reference host that renders it is
[turnstone](https://github.com/mark-ik/turnstone). The graph (the *orrery*) is the
root surface; tiles, panes, and content cards are projections of it.

<p align="center">
  <img src="assets/media/mere-demo.gif" alt="The orrery as a live force-directed graph, nodes settling under physics" width="900"><br>
  <sub>The orrery is a live force-directed graph; nodes settle under physics as you browse.</sub>
</p>

<table>
  <tr>
    <td width="50%">
      <img src="assets/media/demo-1.gif" alt="Switching the orrery between layout strategies"><br>
      <sub>Switch the orrery between layout strategies: force-directed, grid, Penrose, L-system, kanban, timeline.</sub>
    </td>
    <td width="50%">
      <img src="assets/media/demo-2.gif" alt="Recoloring the shell with palette and color-harmony controls"><br>
      <sub>Recolor the whole shell with palette and live color-harmony controls.</sub>
    </td>
  </tr>
</table>

This repository is a Cargo workspace of 55 member crates organized by concern.
(That count is the `[workspace] members` list; the `probes` directory in
`[workspace.exclude]` is not a member and is not counted.)
Mere is a library workspace. The reference on-screen host is **turnstone**, a
separate repository (`mark-ik/turnstone`) that composes these crates into a
window; it obviated the former in-repo `meerkat` host, removed 2026-07-18.
Mere keeps a launchable `canvas` bin for developing the graph view on its own.
This is pre-release, AI-assisted development; many crates are partially
implemented and some capabilities exist in code but are not yet wired into a
host.

**Made with AI**

License: MIT OR Apache-2.0 (see `LICENSE-MIT` and `LICENSE-APACHE`).

## What it is

- A spatial browser: the primary surface is a force-directed graph canvas where
  each node is a page or piece of content and edges are relationships.
- Protocol-agnostic: a Gemini node and an HTTP node can live in the same graph
  and be navigated through the same interface. HTML rides a Servo-derived engine
  lane (`genet`); smolweb protocols (gemini, gopher, finger, spartan, nex,
  guppy, titan) ride genet's `nematic` engine and `errand` transport.
- A composable workbench: tabs become tiles in nested split trees, projected
  from the graph.
- Built toward private local memory (`eidetic`), peer-to-peer comms (`murm`),
  and community/federation (`moot` / `moothold`) over an event-DAG substrate
  (p2panda).

The durable architecture is a composition spine: graph truth (`kernel`) is
arranged by `forme`, projected into a presentation plan by `platen`, realized as
surfaces, and backed per-surface by a content engine selected by `inker`. The
host (`turnstone`) renders the result and composites it.

## The stack's technical architecture

The whole stack is arenas for ownership, id-linked trees for structure,
and delta streams for change, repeated at every layer
from the JS heap (forked Nova) up to the orrery graph.
Have a look at Cambium, the Genet-native GUI toolkit hosted in its own
repository, too.
Up and down the stack, this pattern repeats:  
- Identity is an index, not an address.
- Meaning is a kind, not a class.
- Structure is explicit id-valued edges/links, ordered where semantic (in), indexed where derived (out).
- Everything else is kind-dependent data in a table picked by the kind.
- Change is a recorded delta stream against the tables,
- and every downstream layer is an incremental fold over that stream.

## Screenshots

<table>
  <tr>
    <td width="50%">
      <img src="assets/screenshots/graph-and-page.png" alt="The orrery graph on the left, a web page rendered in a tile on the right"><br>
      <sub>Graph and page side by side: the orrery on the left, an HTML node rendered as a tile on the right.</sub>
    </td>
    <td width="50%">
      <img src="assets/screenshots/welcome.png" alt="The welcome card open beside the graph that builds up as you browse"><br>
      <sub>Open Mere on the welcome card; your graph builds up behind it as you browse.</sub>
    </td>
  </tr>
  <tr>
    <td width="50%">
      <img src="assets/screenshots/layouts.png" alt="The orrery settings panel listing deterministic layout strategies"><br>
      <sub>Arrange the orrery with deterministic layouts: force-directed, grid, Penrose, L-system, kanban by site, timeline by order.</sub>
    </td>
    <td width="50%">
      <img src="assets/screenshots/theming.png" alt="Appearance settings with a palette picker and color-harmony controls"><br>
      <sub>Theme the whole shell. Pick a palette or build your own with color-harmony controls (here, Sunset).</sub>
    </td>
  </tr>
</table>

## Toolchain

- Rust edition 2024. The facade crate `mere` declares `rust-version = "1.92.0"`.
- The workspace pulls several sibling repositories as git dependencies on the
  `mark-ik/*` GitHub org (`genet`, `armillary`, `boa`, `misfin`; genet in turn
  carries the render / fetch / smolweb-transport lanes). A plain `cargo build`
  fetches them; no local sibling checkouts are required. A local checkout, if
  present, is picked up via a gitignored `.cargo/config.toml` `[paths]` override.
- The root `[patch.crates-io]` redirects the Stylo/taffy stack onto genet's
  forks (the published `genet-stylo` family, plus the vendored `taffy` /
  `ipc-channel`) so it unifies with genet rather than conflicting.

## Build, run, test

```sh
# Build the whole workspace
cargo build

# Run the standalone graph canvas on its own window (the dev launcher)
cargo run -p canvas

# Test the whole workspace
cargo test

# Test a single crate (faster while iterating)
cargo test -p kernel
```

The on-screen host, `turnstone`, lives in the sibling `mark-ik/turnstone`
repository (it pulls `mere` as a git dependency); run it from there with
`cargo run`.

The workspace's runnable hosts live under `ports/`. Development binaries that
exercise a reusable crate may remain beside that crate:

- `canvas` (`crates/canvas/canvas`): a thin winit shell over the reusable graph
  field-canvas, launchable on its own for development and testing. `turnstone`
  hosts the same canvas as its root surface. Likely also the seed of a thin
  wasm client for browser-extension targets.
- `graphshell` (`ports/graphshell`): Mere's reference remote projection client.
  It composes the reusable Graphshell session stack into acceptance views and,
  as the host grows, the canonical runnable shell.

## Workspace layout

Reusable code is grouped into supercrate directories under `crates/`, each
owning a single concern. Runnable reference applications live under `ports/`.
Ports may depend on crates; crates must never depend on ports. Crate leaf names
dropped their `mere-*` / role prefixes in a 2026-05-19 naming pass, so the
directory path disambiguates (for example the graph kernel's package name is
`kernel`, at `crates/graph/graph-kernel`).

| Directory | Package(s) | Role |
|---|---|---|
| `crates/mere` | `mere` | The facade crate: the curated public surface downstream hosts (`turnstone`) depend on |
| `crates/canvas` | `canvas` (bin + lib), `arrangements`, `cartography` | The spatial graph view (was `orrery`): the graph field-canvas host + dev bin, deterministic layout strategies, and the non-destructive cartography projection with its scene-paint lane. Physics + the field algebra were extracted to the `numen` sibling stack (2026-07-09) |
| `crates/graph` | `kernel`, `glossary`, `graphlets`, `linked-data` | Graph truth: the identity/authority/mutation kernel, the term glossary, graphlet subgraphs, and the RDF/JSON-LD ingest-export + SPARQL bridge |
| `crates/incipit` | `incipit` | Core identity vocabulary: `GraphId`, `SessionId`, and the id/session primitives shared across the stack |
| `crates/domain` | `apparatus`, `gloss`, `roster`, `trail` | The mere-domain UX panels: facet apparatus, gloss navigator, node roster, and navigation trail |
| `crates/shell` | `chrome`, `comms` | Host-neutral domain models: chrome view-models and the comms pane model. The pane/split-tree model (`frisket`) moved to `turnstone` with meerkat's deletion |
| `crates/system` | `session-runtime`, `shell-state`, `content-contract`, `fetch`, `proofs`, `ux-events`, `registry/register-*` | Runtime services: session/manifest persistence, shell state, the content-reference and fetch contracts, typed proofs, the UX event/probe taxonomy, and the capability registries |
| `crates/forme` | `forme`, `uxtree` | Per-graph-view arrangement authority and the UX-tree projection |
| `crates/platen` | `platen`, `domain/*` | Composition surface: compiles arrangements into presentation plans, plus its workbench/accessory domain panels. (Engine selection — `inker`, `document-canvas`, `nematic` — moved to genet 2026-07-10) |
| `crates/eidetic` | `eidetic` (`eidetic-core`), `eidetic-fjall`, `eidetic-https-fetcher`, `eidetic-iroh-fetcher`, `eidetic-search` | Durable private local memory: the typed-payload store vocabulary, the fjall backend, HTTPS and iroh fetchers, and the tantivy/BM25 lexical search index |
| `crates/import` | `import` | Browser-data import: bookmark / history / session models and Chrome-JSON / Netscape-HTML parsers, producing portable page seeds |
| `crates/crawl` | `crawl` | Site crawling into portable page seeds for the graph |
| `crates/intel` | `embed`, `infer`, `signals` | Local intelligence: the embedding-provider trait + vector index, inference glue, and signal extraction over memory |
| `crates/stickleback` | `stickleback` | The shared replicated-space runtime beneath every signed peer domain: joined spaces, policy-before-insert processing, muniment-backed operation storage, checkpoints, retention mechanics, and native drop carriage. Domains supply operation grammar, addressing, authorization, and materialization |
| `crates/murm` | `murm`, `transport` | Peer exchange: direct conversation, the native conversation engine and signed grammar, and Iroh-based transport |
| `crates/moot` | `moothold`, `mooting` | Governed community spaces: Moot event grammar, roster, tessera, constitution primitives, and recognition policy over `stickleback` |
| `crates/mesh` | `mesh` | The personal-space compute mesh: signed job operations over LogSync, a deterministic job board and worker loop, plus policy-bound retention checkpoints and prunable event history |
| `crates/persona` | `identity`, `gazetteer` | Persona identity and handle resolution: master Ed25519 keypair, OS-keychain integration, per-protocol identity derivation, and WebFinger today |
| `crates/script` | `script-rhai` | The Rhai backend for the block-evaluator lane (pure Rust, sandboxed); the privileged omnibar command shell layers verb bindings on top |
| `crates/probes` | (excluded) | Spike/probe crates; referenced in `[workspace.exclude]` and not product crates |

Nine component families joined the workspace in the 2026-07-23 repo
consolidation. They were sibling repositories until then; their crate names,
versions, and licenses are unchanged, and only their repository home moved.

| Directory | Package(s) | Role |
|---|---|---|
| `crates/persona/personae` | `personae` | The trust-plane spine: identity and carry in one crate |
| `crates/armillary` | `armillary` | The host-neutral actor-kernel runtime: the `!Send` host-kernel boundary plus the `Send` actor harness |
| `crates/eidetic` (alongside the adapters above) | `muniment`, `codicil`, `chartulary`, `scholia` | The memory primitives: the storage-backend seam, the append-only log, the container-graph substrate, and its annotations |
| `crates/servitor` | `servitor` | The resident-helper unit and the authority gate over a denizen's nested graph |
| `crates/intel` (alongside `embed`/`infer`/`signals`) | `vates`, `sibylla` | Inference and embedding: the decoder/actor lane and the retrieval core with its GPU index |
| `crates/conatus` | `numen`, `quint`, `seiche` | The portable physics stack: field definitions, evaluation, integration. Kernel-free by default |
| `crates/scenograph` | `sceno`, `scenomise`, `scenotime`, `scenograph` | The projection engine: scene and score contracts, analytic arrangements, and the incremental runtime |
| `crates/graphshell` | `graphshell-protocol`, `-client`, `-endpoint`, `-stdio` | Mere's reusable remote-session stack: its session grammar, client and endpoint state machines, and first local carrier |
| `ports/graphshell` | `graphshell` | Mere's reference remote projection client and executable acceptance views |

## The printing-press metaphor

The data flow runs in two threads:

- Per-node content production: engines (`genet` for HTML, `nematic` for smolweb,
  the scrying engine for system WebViews) produce content; `inker` selects and
  orchestrates the engine and routing for each node.
- Per-graph-view arrangement: graph truth (`kernel`) is locked into an
  arrangement by `forme`; `platen` presses that arrangement into surface/pane
  output, which the host composites.

`eidetic` keeps impressions over time as content-addressed memory; `node-lineage`
records per-owner navigation history. The name *verso* survives only as the
designation for the engine-flip / compatibility-view seam; its crates were
retired in 2026-06.

## In-product vocabulary

The community tiers are *orrery* (a user's root graph view) → *moot* (a themed,
federatable graph-view community) → *moothold* (a holding of moots) → *coalition*
(a sovereign cluster of mootholds). Other terms: *engram* (a portable durable
memory unit), *flora* (a moot's accumulated engrams), *kith / kin* (contact
tiers), *tessera* (a trust/contribution token), *eidetic* (private local memory).

## Key dependency pins

Set once in `[workspace.dependencies]` and consumed via `dep.workspace = true`:

- Linebender visual stack: `vello` 0.9, `wgpu` 29, `kurbo` 0.13, `peniko` 0.6,
  `parley` 0.10, `skrifa` 0.42, `color` 0.3.
- Input / windowing / a11y: `winit` 0.30.13, `accesskit` 0.24,
  `accesskit_windows` 0.32, `accesskit_unix` 0.21, `accesskit_macos` 0.26,
  `ui-events` 0.3, `raw-window-handle` 0.6, `dpi` 0.1.2.
- Cross-cutting: `tracing` 0.1, `serde` 1, `serde_json` 1, `uuid` 1.

`unsafe_code` is a workspace lint set to `warn`, and `clippy::all` is set to
`warn`.

## Relationship to sibling repositories

Mere is a platform, not one product among peers. The 2026-07-23 repo
consolidation set the bar for staying a separate repository: **real coherent
utility and identity apart from mere, genet, and the products.** Everything
below that bar folds into mere or genet, so this list is short by design.

Mere consumes these one-way (it depends on them; they never depend on Mere):

- `genet` (`mark-ik/genet`): the Servo-derived web engine and host layer. It
  carries the engine-management family (`inker`, `nematic`, `document-canvas`,
  2026-07-10) and, since the consolidation, the reactive UI toolkit
  (`cambium`, `sprigging`, `meristem`), the Fetch engine (`netfetcher`), and
  the product-neutral host contract (`genet-host-api`). Pelt is Genet's
  reference application; Mere does not depend on Pelt.
  The `taffy` / `ipc-channel` forks are vendored there and patched in.
- `netrender` (`mark-ik/netrender`): the paint-realization engine. Public, with
  its own upstream lineage.
- `retinue` (`mark-ik/retinue`): the radio family in one workspace — the
  Reticulum implementation plus `tulle`, `sennet`, `tucket`, and the firmware.
  MPL-2.0, unlike the rest of the family.
- `smolweb` (`mark-ik/smolweb`): the small-web wire layer (`misfin` and the
  spartan / nex / guppy protocol crates). Held in stewardship for their
  protocols' communities.
- `boa` (`mark-ik/boa`): the Boa JavaScript document-host lane.

Downstream, four products depend on Mere one-way: **`turnstone`** (the reference
on-screen host, formerly `meerkat`), **`isometry`**, **`woodshed`**, and
**`hocket`**. Each owns its own truth and reaches Mere for the graph,
projection, memory, identity, and session layers.

## Documentation

Authoritative project documentation lives under `design_docs/`. Start with
`design_docs/DOC_README.md` (the index) and follow its required reading order:
`DOC_POLICY.md`, `TERMINOLOGY.md`, the lexicon brief, and the external-deps
topology brief. Recent foundational docs include the composition spine
(`mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md`), the
statement-kernel brief (2026-06-19), and the interaction-model spine
(2026-06-18). Per-crate areas (`eidetic_docs/`, `inker_docs/`, `murm_docs/`,
`nematic_docs/`, `moothold_docs/`, `verso_docs/`) hold their own plans.

## Status

Pre-1.0 development. The graph canvas, chrome shell, smolweb and HTML content
lanes, session persistence, comms pane, and the SPARQL query lane are
implemented to varying degrees; p2p sync, federation, local intelligence, and
the unified-document-host work are in progress or partially wired. Several design
docs note capabilities that exist in code but are not yet on the live host path.
Crate versions are pinned at `0.0.1` and binaries are `publish = false`.

## More screenshots

<table>
  <tr>
    <td width="50%">
      <img src="assets/screenshots/web-page.png" alt="A full HTML page rendered in a tile"><br>
      <sub>The web lane: a full HTML page rendered in a tile by the genet engine.</sub>
    </td>
    <td width="50%">
      <img src="assets/screenshots/graph-favicons.png" alt="A fuller orrery graph with per-site favicons on the nodes"><br>
      <sub>The graph fills out as you browse; nodes carry their site's favicon.</sub>
    </td>
  </tr>
  <tr>
    <td width="50%">
      <img src="assets/screenshots/layout-lsystem.png" alt="The same graph under an L-system layout in the Sunset theme"><br>
      <sub>The same graph under a different layout (L-system) and the Sunset theme.</sub>
    </td>
    <td width="50%">
      <img src="assets/screenshots/history.png" alt="A page rendered as a card, with a recent list and history mini-map in the side panel"><br>
      <sub>A recent-pages list and a history mini-map track your navigation lineage beside the graph.</sub>
    </td>
  </tr>
</table>

<table>
  <tr>
    <td width="50%">
      <img src="assets/media/demo-3.gif" alt="Inspecting a node in the inspector panel"><br>
      <sub>Inspect any node: identity, provenance, and fetch and parse state.</sub>
    </td>
    <td width="50%">
      <img src="assets/media/demo-4.gif" alt="The command menu listing every action"><br>
      <sub>Every action lives in one command menu: relate nodes, export JSON-LD, open a selection in splits, and more.</sub>
    </td>
  </tr>
</table>

## License

Licensed under either of MIT (`LICENSE-MIT`) or Apache-2.0 (`LICENSE-APACHE`)
at your option.
