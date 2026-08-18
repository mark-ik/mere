> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verse_docs/research/2026-02-23_modern_yacy_gap_analysis.md`
> - Source commit: `1208e352`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.

---

# The Modern YaCy: Gap Analysis & Search Strategy

**Date**: 2026-02-23
**Status**: Research / Strategy
**Context**: Analysis of how to evolve the Verse storage economy into a functional decentralized search engine ("The Modern YaCy").

---

## 1. The Core Problem: Storage vs. Search

The current Verse architecture (`verse_implementation_strategy.md`) solves **Storage** (hosting encrypted blobs) and **Retrieval** (getting blobs by hash). It does not solve **Discovery** (finding which blob contains the text "Rust Async Tutorial").

**YaCy's Approach**: A global Distributed Hash Table (DHT) where every word is a key, and the value is a list of URLs.
*   *Pros*: Fully decentralized.
*   *Cons*: Extremely chatty, high latency, massive index bloat, poor relevance ranking (spam).

**Verse's Proposed Approach**: **Federated Index Exchange**.
Instead of scattering words across a DHT, peers build and share **Index Artifacts** (complete, searchable indices for specific domains/topics).

---

## 2. Gap 1: The Index Artifact

We need a standard format for a portable search index.

*   **Requirement**: A file format that is:
    1.  **Compact**: Compressed inverted index.
    2.  **Mergeable**: Can be combined with other indices.
    3.  **Queryable**: Can be searched efficiently (ideally mapped into memory).
*   **Solution**: **Tantivy Segments**.
    *   Graphshell already plans to use `tantivy` for local search.
    *   A "Published Index" is just a serialized Tantivy segment containing the indexed content of a Graph or Workspace.
    *   This artifact is stored as a **Verse Blob** (immutable, content-addressed).

## 3. Gap 2: The Query Protocol (Remote vs. Local)

How does a user search?

### Scenario A: Local Search (The "Download" Model)
*   **User Action**: Subscribes to "Rust Community Index".
*   **Mechanism**: Graphshell downloads the Index Blob (Tier 2 transaction).
*   **Execution**: The index is mounted locally. Queries run at native speed.
*   **Pros**: Privacy (queries never leave device), speed.
*   **Cons**: Storage/Bandwidth heavy. Good for curated, high-value indices.

### Scenario B: Remote Search (The "Service" Model)
*   **User Action**: Queries "latest crypto news" (too big to download).
*   **Mechanism**: User sends query to a **Search Provider** node.
*   **Execution**: Provider runs query against their massive hosted index and returns results.
*   **Economics**: User pays micro-transaction (Verse Token) per query.
*   **Pros**: Access to massive datasets (Petabytes).
*   **Cons**: Privacy leakage (provider sees query).

**Conclusion**: Verse must support **both**. The protocol needs a `QueryRequest` message type.

---

## 4. Gap 3: The Crawler Economy

Where does the index data come from?

*   **Passive**: Users publish their own browsing history (anonymized reports).
*   **Active**: **Bounty-Based Crawling**.
    1.  **Bounty**: A Curator creates a Verse for "Scientific Papers". They post a bounty: "100 Tokens for indexing arxiv.org".
    2.  **Work**: Peers (Crawlers) scrape the target, extract text/metadata, and build an Index Artifact.
    3.  **Proof**: Crawlers submit the Index Artifact.
    4.  **Validation**: Validators spot-check the index (does it actually contain arxiv content?).
    5.  **Reward**: Tokens released to Crawler.

This turns "crawling" into a gig-economy job, decoupling it from the "search engine" company.

---

## 5. Implementation Roadmap: Search Layer

### Phase 1: Local Indexing (Graphshell Core)
*   Integrate `tantivy`.
*   Index local nodes (title, URL, tags, cached content).
*   Enable `Ctrl+F` full-text search over local graph.

### Phase 2: Index Export (Publishing)
*   Command: "Publish Workspace Index".
*   Action: Serialize local Tantivy segment for that workspace.
*   Result: A `.index` blob stored in Verse.

### Phase 3: Federated Search (Consumption)
*   UI: "Add Search Source". Input: Verse/Peer ID.
*   Mechanism: Download remote `.index` blob, mount as `MultiSearcher` in Tantivy.
*   Result: Local queries hit both local and remote indices transparently.

### Phase 4: Remote Query Protocol
*   Define `Query` and `ResultSet` structs.
*   Implement RPC over Iroh QUIC streams.
*   Add "Search Provider" role to Peer capabilities.
```

c:\Users\mark_\OneDrive\code\rust\graphshell\design_docs\DOC_README.md
```diff
- verse_docs/technical_architecture/GRAPHSHELL_P2P_COLLABORATION.md - P2P collaboration architecture and integration model.
- verse_docs/research/2026-02-23_storage_economy_and_indices.md - Speculative research on storage economy (Proof of Access) and composable indices.
- verse_docs/implementation_strategy/verse_implementation_strategy.md - Hybrid economic model (Direct vs. Brokered) and technical implementation strategy.
- verse_docs/research/2026-02-23_modern_yacy_gap_analysis.md - Gap analysis and strategy for decentralized search (Index Artifacts, Remote Query).

## Archive Checkpoints
