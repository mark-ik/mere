> **Recovered research. Never implemented. Not current direction.**
>
> Recovered 2026-08-16 from the git history of the archived `graphshell`
> repository (`Code/archive/graphshell`), whose entire `design_docs/` tree is
> deleted at HEAD, so this text survives only in history.
>
> - Original path: `design_docs/verse_docs/research/2026-02-27_freenet_takeaways_for_verse.md`
> - Source commit: `04d68365`
>
> None of it was ever built. It is filed here as research, not as a plan and
> not as a statement of where Mere is going. It speaks the retired Verse
> vocabulary (per `TERMINOLOGY.md`, *Verse* is a retired term and the
> network scope is Mere at network scope), and its relative links point at
> donor docs that no longer exist locally. Read the text below for the ideas,
> not the direction.

---

# Freenet (freenet.org) Takeaways for Verse (2026-02-27)

**Status**: Research Notes / External Pattern Review  
**Scope**: Identify reusable architecture patterns from Freenet for Verse without inheriting unrelated complexity.

## Sources Reviewed

- https://freenet.org/
- https://freenet.org/quickstart/
- https://freenet.org/faq/
- https://freenet.org/resources/manual/components/overview/
- https://freenet.org/resources/manual/components/contracts/
- https://freenet.org/resources/manual/components/delegates/
- https://freenet.org/resources/manual/architecture/p2p-network/
- https://freenet.org/resources/manual/architecture/irouting/
- https://freenet.org/resources/manual/architecture/transport/
- https://freenet.org/ghostkey/

## What Is Useful for Verse

### 1. Split shared-state logic from private-identity logic

Freenet separates public/shared contract execution from private delegate execution.  
Verse should keep this boundary explicit:

- Shared sync/state lanes: deterministic, replayable, testable.
- Identity/secret lanes: key material, trust, access control, signing.

Why this matters for Verse:
- Reduces identity seam bleed into compositor/runtime paths.
- Makes access-denied and grant logic easier to audit and test as a separate authority.

### 2. Capability-first runtime contracts

Freenet's model is interface-forward (contracts/delegates/UI each have a clear role).  
Verse should similarly lock down narrow capability surfaces for mods/providers:

- Storage capability
- Sync/messaging capability
- Identity/trust capability
- Diagnostics capability

Why this matters for Verse:
- Prevents implicit cross-module coupling.
- Makes provider swaps and migration slices lower risk.

### 3. Local-node + browser-UI developer loop

Freenet's quickstart story keeps local execution first and visible.  
Verse should preserve the same operational property:

- Deterministic local harness scenarios before distributed complexity.
- One canonical end-to-end scenario per major subsystem (sync, access control, diagnostics).

Why this matters for Verse:
- Keeps Tier 1 quality gates concrete and repeatable.
- Avoids distributed-debug-first development.

### 4. Protocol docs should map to executable checks

Freenet manual is useful structurally, but some pages acknowledge implementation/spec drift.  
Verse should adopt the good part (clear protocol docs) while adding a strict anti-drift guard:

- Each protocol claim links to tests/harness receipts.
- Each critical channel family has schema assertions.
- Doc updates are required when contracts change.

Why this matters for Verse:
- Maintains trust in architecture docs during rapid migration.

## What to Avoid Copying

### 1. Spec/implementation drift

Do not allow architecture docs to become aspirational-only.  
For Verse, any transport/sync claim should be tied to an existing test or marked explicitly as proposed.

### 2. Premature economics coupling

Freenet includes identity/economic mechanisms (Ghost Key, trust signals).  
Verse should avoid coupling core sync correctness to token/economic layers at current maturity.

### 3. Over-centralized "hint" paths

A key current Graphshell/Verse risk is keeping singular hint paths central (focus/render routing).  
Borrow interface clarity, not centralized orchestration that bypasses adapters.

## Recommended Verse Follow-Ons

1. Formalize `shared-state` vs `identity-secrets` authority boundaries in Verse architecture docs.
2. Introduce a capability matrix table for Verse providers/mods (allowed operations by subsystem).
3. Require doc-to-test linkage for every `verse.sync.*` and `verse.identity.*` contract claim.
4. Keep transport and protocol specs explicitly labeled `implemented` vs `proposed`.

## Fit Assessment

Adopt Freenet's **separation discipline** and **interface-first framing**.  
Do not adopt broader network/economic complexity until Verse Tier 1 and Tier 2 done-gates are stable.

