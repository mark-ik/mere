# Moot Object M1 — a moot you can declare, join, and share into

**Date**: 2026-06-12
**Status**: Active; implementation starting.
**What this is**: the moot tier's missing product object. The reputation
lane is proven (`moothold::tessera`: signed per-moot ops, LogSync, two-peer
convergence) but nothing yet *is* a moot — no declaration, no visible
membership, no flora. M1 makes the smallest honest moot: **declare** it
(name + charter), **join** it (announce yourself), **share** into its flora
(engram references), all converging deterministically on every member.
**Naming correction recorded**: this was provisionally called "mooting M1"
in conversation, but `mooting`'s charter (its own crate docs) is the
*protocol-adapter selection* layer (Matrix / Nostr / IRC / ATproto /
ActivityPub adapters over a unified social-primitive API) — not the moot
object. The object lane lands in **`moothold::moot`**, beside the tessera
lane it composes with; `mooting` keeps its adapter charter untouched.
**Related**: the [mesh M1 plan](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md) *(archived — M1 done)*
(this is the third lap of the proven wire/state/sync recipe);
the [eidetic browsing derivation plan](../../eidetic_docs/implementation_strategy/2026-06-12_eidetic_browsing_derivation_plan.md)
(the flora is where a shared `SearchIndex` reference would land — the
federation demo seed, and the consume half's eventual trigger);
the communal-compute tiers brief (a moot is ring 2's container).
**Conflict posture**: pure mere lane (moothold + docs); no serval, no
meerkat. Shell adoption is post-reshape, as everywhere today.

---

## Design (the third lap of the proven recipe)

- **Wire** (`moot/wire.rs`): signed `Operation<MootExt>`; `MootExt
  { moot_id: [u8; 32] }` is the signed addressing extension (cross-moot
  replay fails verification). Events, plain words:
  - `Declared { name, charter, at_ms }` — the founding statement.
  - `Joined { name, at_ms }` — a member announcing themselves (their key is
    the operation's author; `name` is just a display label).
  - `Shared { manifest_id: [u8; 32], schema_id, title, at_ms }` — an engram
    **reference** into the flora (the CID + what it claims to be). Blob
    transfer is deliberately out (M2 rides iroh-blobs; eidetic's
    consume-half work picks up from exactly this reference).
- **State** (`moot/roster.rs`): a deterministic, order-independent fold:
  - Competing declarations resolve by **lowest declaring-op hash** (the
    claim-race rule from the mesh board) — every member sees the same
    founding.
  - Membership: first `Joined` per author wins; members are keys, labels
    are decoration.
  - Flora: entries ordered by `(at_ms, op_hash)` — stable everywhere.
- **Sync** (`moot/sync.rs`): `SyncedMootSpace`, the `SyncedMesh` shape
  (which improved on tessera's receive-only session): LogSync catch-up +
  live lane, an `author()` path (sign at next seq/backlink, persist,
  publish), `roster()` folding the store, real `SyncStatus`, settle-watch
  `resync`.
- **Store**: p2panda-store's SQLite backend behind a `MootStore` mirror of
  `MeshStore` (one transactional insert that persists + sync-indexes).
  The tessera lane keeps its proven redb store; convergence of the two is
  the unification step below, not M1 churn.
- **The peer bin** (`examples/moot-peer.rs`): `declare <name> <charter>` /
  `join <name>` / `share <manifest-hex> <schema-id> <title>` / `show`,
  with the mesh-peer transport shape (env-derived identity + space,
  tickets on stdout/stdin, real status). The two-machine run mirrors the
  mesh milestone: declare on one device, join + share from the other,
  both rosters agree.

## The one-endpoint composition finding (recorded, deferred)

p2panda-net 0.6.1 registers LogSync under a **constant**
`LOG_SYNC_PROTOCOL_ID` — two LogSync instances cannot share one endpoint,
and one instance is monomorphic in its extension type. So when meerkat
eventually runs tessera + mesh + moot lanes on one transport, the shape is
**one LogSync, one shared extension type, lanes separated by topic** —
and the lanes are already structurally ready for it (`TesseraExt`,
`MeshExt`, `MootExt` are all `{ 32-byte id }`). The natural end-state for
a moot specifically: **one moot = one topic**, its log carrying tessera
receipts *and* object events as one vocabulary, the folds separating
concerns. That unification (or an upstream patch making the protocol id
configurable) is its own slice with its own plan; M1 builds standalone
exactly as mesh M1 did.

## Tests

1. Wire: round-trip, signature verification, cross-moot replay fails.
2. Roster fold: declaration race identical in both fold orders; duplicate
   joins collapse; flora order stable; foreign-moot ops skipped.
3. Two-peer convergence: A declares + shares, B joins live; both rosters
   agree (declaration, two members, one flora entry); status counters real.
4. The two-*machine* run is Mark's verification, via `moot-peer`.

## Done conditions

- `cargo test -p moothold` green including the new `moot` module's 1-3.
- `moot-peer` round-trips declare/join/share between two in-process peers.
- Workspace untouched beyond moothold (the crate already exists and is a
  member); cross-repo smoke stays green.
- No blob fetching, no tessera coupling, no protocol adapters (that is
  `mooting`'s charter, untouched), no economy.

## Out of scope (named)

M2: flora blob transfer over iroh-blobs + the eidetic consume-half
hand-off; invitation/capability gating (M1 is the trust-ring rule: holding
the moot id is membership eligibility — the kith ring's definition);
moderation/removal events; the one-endpoint unification above; shell
adoption (post-reshape).

## Progress

- **2026-06-12** — Plan written after the survey that redirected it:
  `mooting` is 27 lines because its charter is the adapter layer, not the
  object; tessera is per-moot but receipt-shaped; nothing declares or
  joins a moot today. Recipe and rules lifted from the mesh M1 lap
  (deterministic races, one write path, author+publish, real status).
- **2026-06-12** — **M1 landed: `moothold::moot` with the full suite green
  (moothold 72 tests; the module's 11 across wire/roster/store/sync,
  including both two-peer convergence lanes) and the `moot-peer` rehearsal
  run end to end.** `wire.rs` (signed `Operation<MootExt>`; cross-moot
  replay fails; p2panda validator compatibility), `roster.rs` (the
  order-independent fold: declaration race resolves by lowest op hash in
  both fold orders, duplicate joins collapse to the earliest, flora stable
  by `(at_ms, op_hash)`, foreign-moot ops skipped), `store.rs` (`MootStore`
  over p2panda-store sqlite, one transactional persist+index write path),
  `sync.rs` (`SyncedMootSpace`: catch-up + live lanes, `author()`,
  `roster()`, real `SyncStatus`, settle-watch `resync`). The rehearsal
  (durable sqlite stores so one identity authors across invocations):
  founder declared `printing-circle`; the friend synced the declaration
  (real status: 1 round, 1 op), joined as `alex`, and shared a
  `eidetic.SearchIndexSpec/v1` reference into the flora; the founder's
  final roster converged on all three — declaration, member, flora entry.
  The flora reference is the literal hand-off eidetic's deferred consume
  half picks up from. **Remaining**: Mark's two-machine run (`moot-peer`,
  the mesh-peer recipe: same `MOOT_SPACE`, distinct `MOOT_SEED`s, tickets
  both ways); then M2's named scope.
