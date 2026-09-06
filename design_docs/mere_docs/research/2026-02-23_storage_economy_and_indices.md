> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verse_docs/research/2026-02-23_storage_economy_and_indices.md` *(historical citation)* <!-- doc-audit: historical-path -->
> - Source commit: `1208e352`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.

---

# Verse: Storage Economy & Composable Indices (Speculative)

**Date**: 2026-02-23
**Status**: Speculative Research / RFC
**Context**: Refines the economic model from `VERSE.md` based on "Proof of Access" and defines the "Index" data structure.

---

## 1. The Storage Economy: Proof of Access

The core shift is from **Passive Storage** (getting paid to hold data) to **Active Service** (getting paid to serve data).

### 1.1 The Mechanism: Sharding & Receipts
1.  **Sharding**: Content (Reports, Graphs, Indices) is encrypted and split into fixed-size shards (e.g., 256KB).
2.  **Hosting**: A Peer ("The Cache") stores shards. They cannot read the content (encrypted), but they can verify integrity (hashes).
3.  **Access**: A User requests a shard.
4.  **The Receipt (The "Fractional Coin")**:
    *   The User receives the shard and verifies the hash.
    *   The User signs a cryptographic **Receipt**: `Sign(User_ID + Host_ID + Shard_Hash + Timestamp)`.
    *   This Receipt is sent to the Host.
5.  **Minting**:
    *   The Host collects Receipts.
    *   Receipts are "cashed in" to the network protocol.
    *   **Validation**: The network checks the signatures and ensures the User had the "bandwidth credits" to request data.
    *   **Reward**: The Host receives **Verse Tokens** (fungible).

### 1.2 Token Metadata & Provenance
While the Verse Token is fungible (1 VT = 1 VT), the minting process preserves **Provenance Metadata** in the ledger history.
*   **Serial Number**: We can trace a batch of tokens back to the specific *service event* (serving shards X, Y, Z to users A, B, C).
*   **Reputation**: Tokens minted from serving high-demand, rare indices might carry more "Reputation Weight" for governance, even if they spend the same as other tokens.

### 1.3 The Economic Loop
1.  **Earn**: Host storage -> Serve shards -> Collect Receipts -> Mint Tokens.
2.  **Spend**: Use Tokens to buy **Access Keys** or **Indices**.
3.  **Trade**: Exchange Tokens for **Tokenized Reports** (rare data).

---

## 2. The Index: A Composable Knowledge Graph

An "Index" in Verse is not a database table. It is a **Graphshell Graph**.

### 2.1 Structure
An Index is a portable, content-addressed Graphshell Workspace containing:
1.  **Nodes**: Content IDs (CIDs) pointing to Reports or other Graphs.
2.  **Edges**: Relationships (traversals, citations, "see also").
3.  **Semantics**: UDC tags, user tags, and embeddings.

### 2.2 Composition (The "Merge")
Because Indices are Graphs, they are **mutually composable**.
*   **Scenario**:
    *   Index A: "Rust Async Ecosystem" (Nodes: Tokio, async-std, blogs).
    *   Index B: "WebAssembly Tooling" (Nodes: Yew, Leptos, bindgen).
*   **Composition**: A user loads both. Graphshell merges them.
    *   **Result**: A new Graph containing all nodes.
    *   **Emergent Value**: If both indices reference `wasm-bindgen`, that node becomes a bridge, visually connecting the two clusters.

### 2.3 Navigability
*   **Graphshell**: Browses the Index as a spatial map. You "fly" through the index.
*   **Verso**: Renders the content within the Index nodes.
*   **Verse**: The network that distributes the shards of the Index.

---

## 3. Tokenized Data Types

### 3.1 The Report (The Atom)
*   **Content**: "User X navigated A -> B at Time T".
*   **Value**: Raw behavioral signal.
*   **Token**: NFT (Unique observation).

### 3.2 The Index (The Molecule)
*   **Content**: A curated graph of Reports and Metadata.
*   **Value**: Curation, organization, semantic tagging.
*   **Token**: Access-Gated NFT (The "Book").
    *   Creators sell access to their Index.
    *   Buyers pay in Verse Tokens.
    *   Hosts earn Verse Tokens for serving the Index shards.

---

## 4. Comparison to Existing Models

| Concept | Filecoin / IPFS | The Graph (GRT) | Verse (Proposed) |
| :--- | :--- | :--- | :--- |
| **Unit of Work** | Proof of Spacetime (Storing) | Indexing/Querying | **Proof of Access (Serving)** |
| **Data Structure** | Files / Blobs | Subgraphs (API) | **Spatial Graphs (UI/UX)** |
| **Consumption** | Download | API Call | **Navigation / Browsing** |
| **Incentive** | Persistence | Query Speed | **Availability & Curation** |

## 5. Summary
This model aligns the economic incentive (serving data) with the user need (accessing knowledge). The "Index as Graph" concept ensures that the data structure of the network is native to the Graphshell client, making the "Verse" literally a traversable universe of graphs.
