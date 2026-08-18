> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verse_docs/research/2026-02-22_aspirational_protocols_and_tools.md`
> - Source commit: `9a6526a7`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.
>
> The donor file begins at its own section 2, with no title and no section 1;
> it was committed that way and no earlier version exists. Recovered as found.

---


---

## 2. Vision: The "Verse" & User Agency

The "Verse" is the collaborative, networked dimension of Graphshell. It transforms the browser from a *consumer* of the web to a *participant* in a new web.

### 2.1 Philosophy & Ramifications
1.  **Inversion of Control**: Currently, search engines decide what you see. In the Verse, *you* curate your index, and your *trust network* curates your discovery feed.
2.  **The "Tokenized Report"**: Knowledge isn't just a link. It's a subgraph: a collection of nodes, annotations, and edges. The Verse allows packaging this understanding into a portable asset (a file, a hash) that can be shared, signed, and verified.
3.  **Resilience**: If a website goes down, the Verse (via IPFS/P2P storage) remembers. Graphshell becomes a distributed archive.
4.  **Identity as Key**: Using cryptographic keys (Nostr/DID) for identity means you own your social graph. If a platform bans you, your graph (and your connections) moves with you.

### 2.2 The "Web of Trust" vs "Web of Algorithms"
The current web optimizes for engagement (ads). The Verse optimizes for **relevance and trust**.
*   **Scenario**: You search for "best hiking boots."
*   **Google**: Shows SEO-spam and ads.
*   **Verse**: Shows nodes your hiking club pinned, reviews from people you follow, and cached pages from your own history.

### 2.3 The Storage Economy
The real limitation of a decentralized web is storage.
*   **Problem**: Who stores the content when the original host goes offline?
*   **Solution**: A storage economy. Users earn "credits" or reputation by pinning content (IPFS) for their Verse.
*   **Opportunity**: People are rewarded for solving the preservation problem.

---

## 3. Protocols & Transport Ecosystem

Integrating these protocols moves Graphshell towards the "Verse" vision of a resilient, peer-to-peer knowledge network.

### 3.1 Decentralized Storage & Sync
*   **IPFS (InterPlanetary File System)**: Content-addressing. You ask for content by *hash* (CID), not by location (IP address).
    *   *Integration*: Node Identity (CID), Storage (Universal Node Content snapshots), Sharing (publishing workspaces).
*   **Gun (GunDB)**: A decentralized, offline-first, graph database protocol.
    *   *Integration*: Meta-alignment for syncing graph data between peers without a server.
*   **Protocol Coexistence (libp2p vs iroh)**:
    *   *libp2p*: Modular, standard for IPFS/Ethereum. Good for raw flexibility.
    *   *iroh*: Newer, QUIC-native, focused on "syncing bytes". Simpler API surface for Graphshell's specific use case.

### 3.2 Privacy & Anonymity
*   **Tor & I2P**: Onion/Garlic routing to obscure user identity.
    *   *Integration*: **Resolver Layer**. Graphshell can enforce strict isolation per-node. A `.onion` node uses a dedicated circuit; a clearnet node uses standard HTTPS.
*   **Encrypted DNS (DoH)**: DNS over HTTPS prevents ISP spying and censorship.
    *   *Integration*: Configure Servo's network layer to use a DoH resolver (like 1.1.1.1) by default.

### 3.3 Social, Identity, & Federation
*   **ActivityPub (The "Social Graph")**: The standard for decentralized social networking (Mastodon).
    *   *Integration*: Treat a Workspace as an Actor. Adding a node emits a `Create` activity. Pair with IPFS to "repost" timestamped snapshots.
*   **Matrix (The "Collaboration Layer")**: Decentralized real-time communication.
    *   *Integration*: Embed chat rooms inside shared workspace nodes; use as a signaling layer for P2P connections.
*   **Nostr (Identity & Publishing)**: Cryptographic keys and relays.
    *   *Integration*: **Portable Identity**. Use Nostr keys (`npub`/`nsec`) as the user's identity for P2P sync. Publish graph snapshots as events.
*   **AT Protocol (Bluesky)**: Authenticated Transfer.
    *   *Integration*: **Algorithmic Choice**. Users can choose "feed generators" to sort their graph or discovery feed.

### 3.4 Alternative Webs
*   **Gemini & Gopher**: Lightweight protocols focusing on text and structure.
    *   *Integration*: **Native Rendering**. Graphshell can render Gemini content natively (bypassing complex HTML layout) for a distraction-free reading mode.

---

## 4. Semantic Engine & Tooling

Moving from "Scraping" (extracting raw text) to "Parsing" (extracting meaning).

### 4.1 Extraction & Parsing
*   **Schema.org & JSON-LD**: Standardized vocabulary for structured data.
    *   *Integration*: The **"Smart Clipper"** parses JSON-LD to populate node metadata (Ingredients, License, Author) automatically.
*   **Readability**: Strips clutter (ads, nav) to extract core text.
    *   *Integration*: Essential for indexing and "Reader Mode" views of graph nodes.
*   **PDF (lopdf)**: Parsing PDF documents.
    *   *Integration*: Treat PDFs as first-class graph nodes with indexable content.
*   **DataFrames (Polars)**: Fast data manipulation.
    *   *Integration*: If a user crawls a dataset (e.g., a wiki table), Polars allows querying and visualizing that data natively.

### 4.2 Classification
*   **Universal Decimal Classification (UDC)**: A faceted library classification system.
    *   *Integration*: **Ontology Support**. Move beyond flat tags. Use UDC codes to drive graph clustering (physics attraction based on semantic distance) and auto-group tabs.
*   **Web Annotation (W3C)**: Standard data model for annotations.
    *   *Integration*: Store user highlights/notes in a portable format, allowing export to other tools without lock-in.

### 4.3 Crawling & Search
*   **The "Personal Crawler"**:
    *   *Workflow*: User selects a documentation root -> Command "Crawl links to depth 2" -> Graphshell builds a local, offline-searchable map of that domain.
    *   *Tech*: `reqwest-middleware` (retries/caching) + `scraper` (HTML parsing).
*   **Decentralized Search (YaCy Analysis)**:
    *   *Critique*: YaCy is Java-based and resource-heavy.
    *   *Graphshell's Take*: Be the modern, Rust-native evolution. **Local First**: Index what *you* browse (via `tantivy`). **Trusted Federation**: Search only your "Verse" (friends/groups), avoiding global noise.

### 4.4 The "Asset" Pipeline
*   **Philosophy**: "Turn understanding into an asset."
*   **Workflow**: Ingest (Browse/Crawl) -> Enrich (Schema/Readability) -> Curate (UDC/Tags) -> Export (Tokenized Report).

---

## 5. Diagnostics & Observability (The "Inspector")

*   **Tokio Tracing**: Instrumenting Rust programs.
*   **Graphshell Integration**:
    *   **Visualizing Servo**: Hook into `tracing` spans to visualize the engine *as a graph*.
    *   **The "Engine Node"**: A special node showing live topology of Servo's threads (Script, Layout, Webrender) and message channels.
    *   **Performance**: Visualize backpressure as edge thickness.

---

## 6. Architecture: Protocol Registry & Modularity

To support this diverse ecosystem without bloating the core, Graphshell uses a modular **Protocol Registry**.

### 6.1 The Protocol Handler Trait
Instead of hardcoding `http`/`https` logic, we define a trait:

```rust
pub trait ProtocolHandler {
    /// The URL scheme this handler supports (e.g., "ipfs", "gemini").
    fn scheme(&self) -> &str;

    /// Resolve the content (fetch, stream, or proxy).
    fn resolve(&self, uri: &str) -> ProtocolResult;

    /// Return capabilities (e.g., supports_search, supports_caching).
    fn capabilities(&self) -> ProtocolCapabilities;
}
```

### 6.2 The Registry & Opt-In Model
*   **Default Set**: `http`, `https`, `file`, `about`.
*   **Opt-In Set**: `ipfs`, `gemini`, `gopher`, `magnet`.
*   **Discovery**: When a user encounters a new scheme, prompt to enable the handler.
*   **Bridge Layer**: Custom protocols can be bridged via internal loopback (e.g., `http://localhost/bridge/gemini/...`) for Servo rendering, or rendered natively.

---

## 7. Extensions & Future Capabilities

### 7.1 RSS/Atom (Syndication)
*   **Concept**: The original decentralized subscription protocol.
*   **Graphshell Integration**:
    *   **Feed Nodes**: A node representing an RSS feed. Edges connect to article nodes.
    *   **Auto-Update**: The "Personal Crawler" can poll feeds and spawn new nodes for new entries.
*   **Crate**: `feed-rs`.

### 7.2 WebAssembly (Wasm) for Applets & Mods
*   **Concept**: Safe, portable binary format for executing code.
*   **Graphshell Integration**:
    *   **Applet Nodes**: Nodes that run a small Wasm binary (calculator, visualizer, game) instead of a web page.
    *   **Mods**: User-defined physics forces or renderers compiled to Wasm.
*   **Crate**: `wasmer` or `wasmtime`.

### 7.3 Vector Search (Semantic Embeddings)
*   **Concept**: Searching by *meaning* rather than keyword matching.
*   **Graphshell Integration**:
    *   **Semantic Association**: "Find nodes related to 'climate change'" (even if they don't use that phrase).
    *   **Auto-Linking**: Suggest edges between nodes with high semantic similarity.
*   **Crate**: `lance` (vector DB) or `candle` (local inference).

### 7.4 Local LLM Inference (The "Synthesizer")
*   **Concept**: Running efficient language models locally to summarize and extract insights without cloud dependencies.
*   **Graphshell Integration**:
    *   **Summarization**: Auto-generate summaries for "Active" nodes to populate tooltips.
    *   **Extraction**: Turn unstructured page text into structured node metadata (e.g., extracting event dates).
    *   **Chat with Graph**: RAG (Retrieval-Augmented Generation) combining Vector Search with an LLM to answer questions based on the user's browsing history.
*   **Crate**: `candle` (Hugging Face's Rust ML framework) or `burn`.

### 7.5 CRDTs (Real-Time Collaboration)
*   **Concept**: Conflict-free Replicated Data Types allow concurrent edits from multiple peers to merge automatically.
*   **Graphshell Integration**:
    *   **Shared Notes**: Enabling real-time co-editing of text notes attached to nodes.
    *   **Live Lists**: Managing shared "To Read" queues in a P2P workspace where order matters.
*   **Crate**: `automerge` or `yrs` (Rust port of Yjs).

### 7.6 Web Archiving (WARC)
*   **Concept**: The ISO standard file format for web archives, preserving headers and content fidelity.
*   **Graphshell Integration**:
    *   **Forensic Clipping**: Saving the exact network response (headers + body) for a node, not just the DOM.
    *   **Portability**: Exporting clips that can be viewed in standard tools like ReplayWeb.page or uploaded to the Internet Archive.
*   **Crate**: `warc`.

---

## 8. Master Crate Index

Summary of external crates mapped to Graphshell capabilities.

| Domain | Crate | Purpose |
| :--- | :--- | :--- |
| **Search** | `tantivy` | Local, high-performance full-text indexing of graph content. |
| **P2P/Sync** | `iroh` | Efficient syncing of graph state and blobs; IPFS alternative. |
| **Networking** | `libp2p` | Modular network stack (DHT, GossipSub) if raw IPFS compat is needed. |
| **Tor** | `arti-client` | Embedding Tor connectivity directly into the resolver layer. |
| **DNS** | `hickory-dns` | DoH support for privacy and censorship resistance. |
| **Parsing** | `scraper` | Lightweight HTML parsing (CSS selectors) for the crawler. |
| **Content** | `readability` | Extracting main article content (stripping ads/nav). |
| **Semantic** | `json-ld` | Extracting structured data from web pages. |
| **Data** | `polars` | Querying and visualizing tabular data found during browsing. |
| **Diagnostics**| `tracing` | Instrumenting the engine to visualize internal topology. |
| **Social** | `activitystreams` | ActivityPub federation for graph sharing. |
| **Chat** | `matrix-sdk` | Real-time collaboration and signaling. |
| **Identity** | `nostr-sdk` | Portable identity and censorship-resistant publishing. |
| **Protocol** | `gemini` | Native rendering of lightweight Gemini content. |
| **Federation** | `atrium-api` | AT Protocol for portable identity and algorithmic choice. |
| **HTTP** | `reqwest-middleware`| Robust HTTP client with retries/caching for the crawler. |
| **Documents** | `lopdf` | Parsing and indexing PDF content. |
| **Syndication**| `feed-rs` | Parsing RSS/Atom feeds for subscription nodes. |
| **Runtime** | `wasmer` | Running Wasm applets and mods safely. |
| **AI/Vector** | `lance` | Vector database for semantic search and auto-linking. |
| **AI/ML** | `candle` | Local LLM inference for summarization and RAG. |
| **Collab** | `automerge` | CRDTs for conflict-free real-time data merging. |
| **Archiving** | `warc` | Standardized web archive format for high-fidelity clips. |

---

## 9. The Registry Ecosystem

To manage the complexity of a "Knowledge User Agent," Graphshell employs a system of modular registries. These allow features to be composed, swapped, and extended by users or mods.

### 9.1 Core Registries

1.  **Protocol Registry** (Transport):
    *   **Role**: Maps URL schemes (`ipfs://`, `gemini://`) to handlers.
    *   **Pattern**: Opt-in. Users enable protocols as needed.

2.  **Viewer Registry** (Rendering):
    *   **Role**: Maps MIME types or file extensions to renderers (PDF, Markdown, CSV, 3D Models).
    *   **Concept**: Decouples content from the browser engine. Not everything needs a webview.

3.  **Command Registry** (Action & Automation):
    *   **Scope**: Unifies user actions, keybindings, and autonomous agents.
    *   **Categories**:
        *   *User Commands*: Manual actions (palette, context menu).
        *   *Keybinds*: Input mapping.
        *   *Autonomous Agents*: Background scripts (crawlers, auto-taggers) that emit intents.
    *   **Pattern**: Piecewise combination. Users can compose commands from defaults or import mod dependencies.

4.  **Lens Registry** (Presentation):
    *   **Role**: Composable view configurations.
    *   **Definition**: A "Lens" is a composition of **Theme** + **Layout** + **Physics**.
    *   **Usage**: Users can layer lenses (e.g., "Dark Mode" + "Tree Layout" + "Low Gravity") or switch contexts entirely.

5.  **Identity Registry** (Auth & Persona):
    *   **Role**: Manages cryptographic keys and profiles (Work, Personal, Public/Nostr).
    *   **Function**: Signs reports and sync payloads without leaking keys to every module.

6.  **Ontology Registry** (Meaning):
    *   **Role**: Manages structured data definitions (Schema.org, UDC).
    *   **Function**: Provides UI for editing metadata and defines how nodes relate semantically.

7.  **Index Registry** (Recall & Federation):
    *   **Role**: Manages search backends (Local Tantivy, Peer Indexes).
    *   **Verse Integration**: The primary mechanism for sharing knowledge. You publish your index; you subscribe to others'.

### 9.2 Universal Registry Patterns
*   **Modularity**: All registries support "Mods" — external definitions that can be loaded/unloaded.
*   **Composition**: Defaults can be mixed with user overrides and mod extensions.

<!--
  Original sections merged:
  - Introduction -> §1
  - Decentralized & Privacy Protocols -> §3
  - Semantic Understanding -> §4
  - Diagnostics -> §5
  - Social, Identity, Alternative -> §3
  - Tooling Concepts -> §4
  - Summary of Promising Crates -> §7
  - Architecture -> §6
  - Additional Interesting Crates -> §7
  - Analysis: YaCy -> §4.3
  - The "Verse" -> §2
-->
