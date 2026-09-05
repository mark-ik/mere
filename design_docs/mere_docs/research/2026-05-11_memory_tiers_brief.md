# Memory tiers — short-term vs. long-term — design brief

**Date**: 2026-05-11
**Status**: Design brief — establishes the partition story Mere's persistence layer needs once branches, forks, view-intent state, and diff machinery start producing data that may or may not deserve durable storage. **Refined 2026-06-09** by [`../technical_architecture/2026-06-09_alembic_memory_and_engrams.md`](../technical_architecture/2026-06-09_alembic_memory_and_engrams.md) §2: the two tiers become three levels (short-term / long-term / engram). That doc promotes this brief's *medium-term* footnote (§3.3) to a real **long-term** level (durable but not necessarily addressable) and makes the **engram** a distinct distillation on top, rather than equating long-term with engram. The consolidation gesture (§4) is that distillation.
**Scope**: Names the two tiers (short-term, long-term), describes what lives where, defines the consolidation gesture that promotes short-term state to long-term, and clarifies how this layers on top of eidetic's existing immutable-engram model. Does **not** specify the short-term substrate's implementation in detail; that lands in its own follow-up plan when consumers exist.

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) — §5.3 (`ViewIntent` persistence), §11.3 (referenced this partition as undecided). This brief resolves the partition.
- [`2026-05-11_tearout_operations_brief.md`](2026-05-11_tearout_operations_brief.md) — branch and fork state live in short-term by default; consolidation is an affirmative gesture. The fork-model brief's substrate footnote depends on this brief's framing.
- [`../implementation_strategy/2026-05-11_graph_session_manifest_plan.md`](../implementation_strategy/2026-05-11_graph_session_manifest_plan.md) — long-term session state. The manifest itself is long-term; short-term state lives alongside it but isn't engram-shaped.
- Eidetic's engram model: [`crates/eidetic/src/engram.rs`](../../../crates/eidetic/src/engram.rs).

---

## 1. The principle

> **Mere shouldn't pay the cost of durable, content-hashed, schema-versioned storage for state the user hasn't affirmatively chosen to keep.**

Eidetic's engrams are powerful: immutable, content-addressed, time-bounded, federation-ready. They're also expensive — every engram is a snapshot with integrity metadata, cross-tier shipping concerns, and consolidation overhead. Engrams are the *right* substrate for memories the user wants to keep. They're the *wrong* substrate for working state that may or may not survive the next half hour.

A short-term tier exists so working state doesn't have to be either ephemeral-in-RAM or fully engram-grade. Short-term state is durable enough to survive an app restart, cheap enough that creating lots of it doesn't matter, and discardable without ceremony.

## 2. The two tiers, side by side

| Tier              | Substrate (sketch)                        | Cost      | Lifetime                                  | Examples                                                                     |
| ----------------- | ----------------------------------------- | --------- | ----------------------------------------- | ---------------------------------------------------------------------------- |
| **Short-term**    | TBD (likely fjall or simple JSON sidecar) | Cheap     | Until garbage-collected or consolidated   | Live branch state, fork before consolidation, view intent, transient diffs, in-progress edits, ephemeral lineage extensions |
| **Long-term**     | Eidetic engrams                           | Expensive | Forever (content-hashed; supersedable)    | Saved sessions, named branches, published forks, consolidated lineage records, durable diagnostics |

The boundary between them is a **user gesture**, not a heuristic. Heuristics can apply on top (auto-consolidate idle branches after N days) but every default has to be a user-overridable policy, not a silent rule.

## 3. What lives where

### 3.1 Short-term

Reasonably scoped list, growing as new consumers arrive:

- **Branch state** (per [tearout-operations-brief](2026-05-11_tearout_operations_brief.md) §5.3) — new graphlets in a donor's graph-tree, with their accumulated lineage edges, anchors, and members. Lives short-term until the user consolidates.
- **Fork state** — a fork's session graph, before consolidation. The fork is functionally complete (the user can use it like a session) but isn't engram-backed yet.
- **`ViewIntent`** (per framing brief §5.3) — per-pane `(form_factor, scale, focus, filter, strategy)`. Cheap to write often; consolidation is rare unless the user pins a specific view.
- **Transient diffs** — diff records between a branch and its donor, between two engrams, between past and present states. Useful for the UI to show what's changed without writing a new engram per diff.
- **In-progress edits** — local node renames, position drags, edge additions before they're flushed. Edits become engram-eligible only when the user consolidates or when the engram interval naturally rolls over.
- **Ephemeral diagnostics** — apparatus event buffer (already exists; this brief frames it as the short-term form of `permission.denied` / `action.dispatched` / etc.). Engram form is the consolidated incident receipt.

### 3.2 Long-term

- **Session manifests** — `GraphSessionManifest` per session is long-term (it's the durable session identity). Stored as JSON per the [manifest plan](../implementation_strategy/2026-05-11_graph_session_manifest_plan.md), not as an engram, because manifests are mutable identity records and engrams are immutable.
- **Consolidated graph state** — the full `Graph` content of a session, at the moment of consolidation. One engram per consolidation event.
- **Consolidated branches** — a branch's graphlet + its accumulated members, frozen into an engram. Reference-able by future graphlets, sessions, or moot tier work.
- **Published forks** — forks the user has explicitly chosen to durably retain (vs. throwaway exploration forks).
- **Diagnostic receipts** — engram-form of session events; the consolidated, federation-shippable record. Triggered on significant transitions (`session.killed`, `session.forked`, `permission.denied` with severity).

### 3.3 Boundary cases

- **The graph itself** — when a session is just running, the in-memory `Entity<Graph>` is short-term. The on-disk graph store under `sessions/<session_id>/graph/` is **medium-term**: it survives restarts but isn't an engram. Engrams come from explicit consolidation of the in-memory or on-disk graph state.
- **Saved frame layouts** — the existing `frames/<frame_id>.json` is medium-term. Survives restarts; doesn't need engram durability.

"Medium-term" isn't a separate tier in this brief's vocabulary — it's "short-term with disk persistence." The two real tiers are short-term (no engram cost) and long-term (engram). Whether short-term lives in RAM or on disk is an implementation choice per consumer.

## 4. Consolidation gesture

The defining act of long-term commitment. Affirmative, user-driven, never automatic without explicit policy opt-in.

### 4.1 Surfaces

- **Palette entries:**
  - "Consolidate this branch as engram."
  - "Consolidate this fork as engram."
  - "Snapshot this session" (consolidates the whole session's current graph as one engram).
- **Toast prompts** — at significant moments (closing a session with unconsolidated branches; killing a session with active forks), prompt the user: "you have N unconsolidated branches; consolidate, keep ephemeral, or discard?"
- **Auto-consolidation policies** (opt-in, configurable):
  - "Consolidate branches inactive for >N days."
  - "Consolidate the current session graph on app exit."
  - "Auto-snapshot every N hours of active editing."

### 4.2 Mechanics

Consolidation produces an engram via the existing eidetic pipeline:

1. Snapshot the short-term state (branch, fork, session graph, diff record, ...) into a schema-conformant payload.
2. Compute the content hash, attach time bounds, attach provenance (session id, persona id, parent references).
3. Write the engram via `eidetic::engram::Engram::new(...)`-style construction (specifics defer to eidetic's API).
4. Update the relevant manifest's references (`consolidated_engrams: Vec<EngramId>` on the session manifest, for instance).
5. The short-term state itself **stays** by default — consolidation produces an engram alongside, doesn't necessarily replace the short-term form. A separate "discard short-term, engram is canonical" gesture exists for users who want a clean cut.

### 4.3 What consolidation does not do

- It does **not** delete short-term state. Engrams are immutable snapshots taken at a moment; the working state continues past that moment.
- It does **not** automatically trigger federation shipping (moothold/murm tier traffic). That's a separate "publish this engram" gesture.
- It does **not** commit users to keeping the engram. Engrams are immutable but can be marked superseded or held private.

## 5. Garbage collection

Short-term state grows. Without bounds it eats disk. The brief doesn't pin a specific GC strategy but states the principles:

- **GC happens, eventually.** Even short-term state isn't infinite — branches the user abandoned, view intents for closed panes, diffs the user looked at once.
- **GC is observable.** Diagnostics fire (`short_term.gc { kind, count }`) so the user can see what's being collected. Per the [browser multiplexer framing brief](2026-05-11_browser_multiplexer_framing.md) §8, diagnostics are how the multiplexer becomes legible.
- **GC respects the consolidation boundary.** Anything that's been consolidated is exempt (the engram is the durable record; the short-term form can be discarded). Anything unconsolidated needs an explicit "I don't care about this" gesture, or an inactivity threshold the user has opted into.
- **GC is user-overridable.** "Keep this branch alive" / "pin this view intent" gestures exempt specific items from GC.

Concrete defaults (configurable):

- Branches: GC after 30 days of inactivity *if* unconsolidated *and* not pinned.
- View intents: GC when the parent pane closes, unless pinned.
- Transient diffs: GC after the user navigates away from the diff view.
- Ephemeral diagnostics: ring-buffer with a configurable cap (already how apparatus's event buffer works).

## 6. How this interacts with existing work

### 6.1 The manifest plan ([implementation strategy](../implementation_strategy/2026-05-11_graph_session_manifest_plan.md))

Unchanged in its v0 scope. The manifest itself is long-term (JSON record of durable identity). The session's graph data on disk is **medium-term** as defined in §3.3 — survives restarts but isn't an engram until consolidated.

Two small additions to the manifest schema this brief implies (none of which break the manifest plan's v0):

- `consolidated_engrams: Vec<EngramId>` — engrams produced from this session, reference-able for retrieval.
- `last_consolidated_at: Option<SystemTime>` — informational; helps "consolidate on idle" policies.

Both are `#[serde(default)]` additions; pre-existing manifests load cleanly.

### 6.2 The fork-model brief ([tearout-operations-brief](2026-05-11_tearout_operations_brief.md))

This brief is its substrate brief. Branch state and fork state live short-term per §3.1. The fork-model brief's "consolidation gesture" reference resolves to §4 of this brief.

### 6.3 `ViewIntent` sidecar (framing brief §5.3)

`ViewIntent` is short-term per §3.1. The sidecar storage shape (proposed in the framing brief as `<data_dir>/sessions/<session_id>/views/<frame_id>/<pane_id>.json`) is the **medium-term** form — disk-persisted, not engram-shaped. Consolidating a view intent to long-term is the gesture for "this view of this graph is canonical; I want it preserved across all future restores and federations."

### 6.4 Apparatus diagnostics

The existing in-process `EventBuffer` is the short-term form. Consolidation of a diagnostic burst into an engram is the long-term form, useful for incident-after-the-fact analysis or federation-shipped failure receipts. Out of scope for v0; mentioned so the diagnostics brief eventually pins the boundary.

## 7. Open questions

### 7.1 Short-term substrate — which technology?

Three reasonable answers:

- **Per-consumer JSON sidecar** — `<session_dir>/views/<pane_id>.json`, `<session_dir>/branches/<graphlet_id>.json`, etc. Hand-inspectable; cheap; no shared engine.
- **Per-session fjall key-value store** — one fjall keyspace per session, holding `views/`, `branches/`, `diffs/`. Better for many small writes; less inspectable.
- **App-scope fjall store** — one big fjall keyspace for all short-term state across sessions. Single index; potentially faster queries; centralised GC.

Recommendation deferred. Pick when the first short-term consumer (probably `ViewIntent` sidecar) lands. JSON is the lean default for inspection-friendly v0; fjall is the path if write volume becomes a bottleneck.

### 7.2 Schema evolution

Engrams have `schema_version`. Short-term state probably should too — branches and diffs need to evolve. **Recommendation:** every short-term struct that hits disk carries a `schema_version: u32` field, defaulting to 1. Cheap insurance.

### 7.3 What does "consolidating a diff" produce?

A diff between two engrams is itself a structural record. Is the consolidated form:

- A new engram containing the diff data?
- A reference structure pointing at the two source engrams + the diff record?
- A patch that, applied to one source, reconstructs the other?

Probably the second (reference + diff record); engrams are immutable so a diff doesn't replace its sources. Defer specifics to the eidetic team / the diff UX work.

### 7.4 Cross-tier federation

Engrams ship across moothold/murm tiers. Short-term state does **not** (it's local). The consolidation gesture is therefore also the **"makes shipable" gesture** — until something is an engram, it can't leave the local persona's scope. This is a desirable property (privacy by default; explicit consent to ship) and should be called out as a security principle that capability-gate work picks up.

## 8. What this brief locks in / doesn't

**Locks in:**

- Two tiers: short-term and long-term, with eidetic engrams as the long-term substrate.
- Consolidation is an affirmative user gesture, never silent.
- Short-term state can persist to disk (medium-term form) without being engram-shaped.
- GC is observable and user-overridable.
- Local-only-until-consolidated is a federation/privacy property.
- Auto-consolidation policies exist only opt-in.

**Doesn't preclude:**

- Federation-time auto-consolidation (an explicit "publish this session" gesture could consolidate-then-ship in one step).
- Cross-engram diff records as their own engram type, if that simplifies federation.
- Multi-tier storage (a future "warm" tier between short-term and engram, if a use case warrants).

**Defers to follow-up:**

- Short-term substrate technology (JSON / fjall / hybrid) — pick at first consumer.
- GC algorithm specifics — implementation time.
- Consolidation UI surfaces beyond palette entries (toasts, gloss-strip prompts, settings panel).
- Diff record schema — eidetic's domain.
