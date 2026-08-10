# Wallet Carry Fold-In Plan

**Date:** 2026-08-10
**Status:** W0 (this map) done; W1 executing
**Anchors:** [dramatis tier plan](2026-08-10_dramatis_tier_plan.md) D4,
[credential port + gazette brief](../research/2026-08-10_credential_port_gazette_brief.md),
the 2026-07-08 personae founding ruling ("the carry layer folds into the same
crate later"), the 2026-08-08 family-shared identity plan.

## Why now

Castellan's stored credentials (chatelaine items: passwords, TOTP seeds) are
foreign material that must be stored and synced, not re-derived. The sync
seam is the wallet's sealed-record + epoch machinery, which lives in
`session-runtime`. Until the carry model lives in personae, castellan and any
non-mere consumer would have to dep session-runtime, which is exactly the
wrong direction. This fold-in is therefore the prerequisite of the castellan
runway, not a sibling of it.

## The discovered map (W0, verified 2026-08-10)

The 2026-07-08 "fold it in later" ruling predates two couplings that make a
verbatim move wrong:

- **`eidetic::Hash`** appears as content-ref fields in seven serialized wallet
  types (DeviceGrantRef, IdentityWalletManifest, PrivateRoots, PublicRoots,
  CapabilitySlotRef, PersonaWalletManifest, DeviceRecord). personae must not
  dep eidetic (two-planes doctrine: the planes bond at seams, not crate
  deps; personae is consumed standalone by hocket). **Resolution:** Hash
  serializes as its display string (`blake3:<hex>`; `schema.rs` L229 uses
  `serialize_str(to_string())`). personae defines a repr-identical boundary
  newtype **`CarryRef`**; disk bytes do not change; session-runtime converts
  at the seam via free functions (orphan rule blocks a From impl there). A
  personae test pins the repr against known vectors.
- **`p2panda_core::cbor`** encodes grant envelopes in `wallet_grant.rs`.
  Moving that into personae drags the p2panda fork lineage into a published
  standalone crate. **Resolution deferred to W3:** either the envelope codec
  stays in the adapter, or personae deps the underlying cbor crate directly
  after byte-compat verification against staged grants. Do not guess; staged
  grants exist on Mark's real machine.
- Session-runtime-local couplings, both thin: `device_settings_store` (one
  policy read: `startup_unlock_mode`) and `engine_profile_store::PERSONAS_DIR`
  (path layout). These stay in the adapter; the model takes policy and paths
  as parameters.
- **`engram_seal::WalletEpochSealer` is the seal seam itself** (278 lines,
  implements eidetic's `PayloadSealer`). It stays in session-runtime by
  doctrine; only the model beneath it moves.
- `manifest::PersonaId` is already a re-export of `identity::PersonaId`; no
  type split exists.
- Consumers outside session-runtime: **ports/knot** (`knot_sync_host`,
  `startup.rs`, `tests/revision_bell.rs`). Re-exports must keep these
  compiling unchanged until W4 re-bases them.
- Sizes: `wallet_store.rs` 1653 lines, `wallet_grant.rs` 2524. Both breach
  the 600 ceiling; the fold-in is also the split.

## Shape at the end

- `personae::carry`: the model. Record types, `CarryRef`, schema version,
  derivation (`persona_wallet_salt`, `derive_persona_chain_root`), epoch
  bridge model. No filesystem, no eidetic, no p2panda, no policy reads.
- `session-runtime::wallet_store`: the adapter. Path layout under the data
  root, device-settings policy, sealed-record wiring, load/save, bootstrap
  ladder. Re-exports the carry model for existing consumers.
- `session-runtime::wallet_grant`: the grant flows over the model (W3 rules
  the codec).
- `session-runtime::engram_seal`: unchanged; the PayloadSealer seam.

## Done conditions

### W1: the model moves (this session)

- [ ] `personae::carry` module: all wallet record types (wallet_store L90-417),
      `WALLET_SCHEMA_VERSION`, `CarryRef` with the pinned repr, plus
      `persona_wallet_salt` + `derive_persona_chain_root`
- [ ] Repr test: `CarryRef` round-trips the exact JSON eidetic::Hash produces
- [ ] session-runtime re-exports the moved types from `wallet_store`;
      `wallet_grant`/`engram_seal`/`shared_root`/knot compile unchanged
- [ ] Hash<->CarryRef seam helpers in session-runtime where wallet_grant
      computes content refs
- [ ] Targeted green: `cargo test -p personae -p session-runtime`,
      `cargo check -p knot`
- [ ] Disk-format invariant: zero serialized-byte changes (repr test + the
      existing round-trip tests are the witness)

### W2: the store splits under the ceiling

- [ ] `wallet_store.rs` splits: paths/policy/IO adapter vs re-export facade;
      every resulting file under 600 lines
- [ ] Model-shaped store logic (`ensure_wallet_state` invariants, epoch
      staging rules) moves into carry as pure functions over injected
      state where it cleanly can

### W3: the grant envelope ruling

- [ ] Verify what `p2panda_core::cbor::{encode,decode}_cbor` wrap; test
      byte-compat of a direct cbor dep against a staged grant fixture
- [ ] Rule: envelope codec in personae::carry (if byte-compat holds with a
      non-p2panda dep) or in the adapter (if not)
- [ ] `wallet_grant.rs` splits under the ceiling either way

### W4: consumers re-base + closure

- [ ] knot imports `identity::carry` names directly; re-export facade shrinks
- [ ] The wallet fold-in line in the personae founding doc + trust-plane
      memory updated
- [ ] personae version bump + publish only when a consumer outside mere
      wants carry (republish timing rule)

## Progress

- 2026-08-10: W0 map verified against code (couplings, consumers, sizes,
  Hash repr). W1 begun.
