# Eidetic Deferred Phases — Implementation Plan (2026-06-09)

**Status**: Active (open tail spun out of the completed layered-stack plan).
**Spun out of**: `archive_docs/.../2026-05-09_eidetic_layered_stack_plan.md` (Phases 1-6
plus all nine sidequests shipped; that plan is archived as complete).
**Source design**: [`../research/2026-05-09_eidetic_design_pass.md`](../research/2026-05-09_eidetic_design_pass.md)
**Crate family**: `repos/mere/crates/eidetic/` (`eidetic-core`, `eidetic-fjall`,
`eidetic-https-fetcher`, `eidetic-iroh-fetcher` shipped).

The four-layer stack (blob → manifest → typed-payload → memory-domain), the
schemas-as-engrams recursion, three-axis classification, BLAKE3/CIDv1 addressing,
async Store trait, and the model library are all built. What remains are the
three trigger-gated phases the layered-stack plan deferred, plus the open
questions that outlive it. Each is consumer-gated: build when a real consumer
pulls.

---

## Phases

### Phase 7 — `eidetic-opfs` (browser Store backend)

Browser-side OPFS-backed `Store`. **Trigger**: a browser-side eidetic consumer
pulls (likely `intel/embed` running embeddings in-browser, or browser-side
persistence of a vector index).

**Scope**:
- New crate `crates/eidetic/eidetic-opfs/`.
- Hand-rolled `Store` over `FileSystemSyncAccessHandle` in a dedicated worker.
- One file per blob keyed by hash; manifest store as a directory under a known prefix.
- wasm-bindgen + `web-sys` for OPFS access.
- `fjall-on-OPFS` feasibility (wasi-fs shim) tracked separately, not gating.

**Done conditions**: browser round-trip through Layer 1/2/3 in a wasm harness;
quota-exceeded returns a clean error; persistence survives reload after
`navigator.storage.persist()`; under the 600-LOC ceiling.

**Dependencies**: Phases 1-2 shipped (the OPFS impl is at Layer 1).

### Phase 8 — `eidetic::browsing` (Layer-4 browsing memory)

Browsing-memory accumulation composing `BrowsingTrace` and related typed payloads
into a user-facing `BrowsingMemory` API. **Trigger**: a UI surface pulls, e.g.
meerkat's omnibar/command surface persisting query history, or the gloss
Navigator recalling a recent corridor.

**Scope (detail when triggered)**: `BrowsingTrace`, `ClipNode`, `SettingBundle`
schema engrams; `BrowsingMemory` API (`record_traversal`, `recent_corridor`,
`nodes_visited_in_window`, `co_occurrence`); quota policy per design pass §8.

**Dependencies**: Phase 3 shipped. Note the node-navigation lineage work already
gives nodes a durable nav history; browsing memory should read from that rather
than duplicate a visited-set.

### Phase 9 — `SearchIndex` schema + tantivy

First-class lexical search; the federated-contribution counterpart to the vector
index. **Trigger**: a moot-side consumer pulls lexical search, or graph/semantic
search demands exact-phrase recall.

**Scope (detail when triggered)**: `SearchIndex` schema engram (corpus shape,
tokenizer, ranking config, field set); `tantivy::Directory` over OPFS (browser)
and native fs; index-merge plumbing for moot composability (design pass §7.5);
segment-separable composition default.

**Dependencies**: Phase 3 (typed payloads), Phase 7 (OPFS for the browser case).

---

## Open questions (carried forward)

- **Federated identity for engram signatures** — waits on the persona/identity
  vault (`crates/persona/identity`, now built); wire signature verification once
  signed engrams cross a peer boundary.
- **Schema-engram GC semantics** — surfaces when GC is implemented; no current
  phase. The engram lifecycle policy (durable by default, no implicit deletion)
  stands.
- **Moot accepted-schema-set discovery API** — Phase 9 territory; how a moot
  advertises which schema classes it ingests (ties to the moothold federation
  filtering in the local-intelligence research §5.6).
- **fjall-on-OPFS feasibility** — Phase 7 tracks; not gating.

---

## Progress

- 2026-06-09 — Spun out of the completed layered-stack plan. Phases 1-6 +
  sidequests 1-9 shipped (eidetic 67 tests + the four companion crates green per
  the archived plan's progress log). No new code; this plan holds the deferred
  tail until a consumer triggers each phase.
