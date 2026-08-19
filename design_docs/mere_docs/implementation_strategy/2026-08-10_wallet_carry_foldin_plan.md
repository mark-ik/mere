# Wallet Carry Fold-In Plan

**Date:** 2026-08-10
**Status:** COMPLETE 2026-08-10 (W0-W4). One question spun out; see W3.
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

- [x] `personae::carry` module (split as `carry/mod.rs` + `carry/refs.rs`,
      both under the ceiling): all wallet record types, `WALLET_SCHEMA_VERSION`,
      `CarryRef`, `persona_wallet_salt` + `derive_persona_chain_root`
- [x] Repr test: `carry_ref_repr_matches_eidetic_hash_repr` in wallet_store
      compares string and JSON forms across three inputs
- [x] session-runtime re-exports the moved types from `wallet_store`;
      `wallet_grant`/`engram_seal`/`shared_root`/knot compile unchanged
- [x] No seam helpers needed at all: both ref producers were plain BLAKE3
      over canonical bytes, so they now call `CarryRef::of` directly and
      eidetic::Hash simply left the wallet
- [x] Targeted green: personae 74, session-runtime 245 (including all wallet
      round-trips and grant flows), knot check
- [x] Disk-format invariant held by the repr test + the untouched round-trip
      suite

### W2: the store splits under the ceiling

- [x] `wallet_store.rs` (1544) splits into a module tree, largest file 322:
      `paths` (composition + the persona membership test), `secrets` (the
      unlock ladder, sealed-record stores, identity seed), `manifests` (the
      three plain-JSON manifests), `devices` (grants, local delegated
      identity, wrapping keys), `epochs` (the private-epoch bridge),
      `bootstrap` (the first-launch ladder), `io` (atomic writes),
      `test_support` (shared fixtures). Tests moved with their subject.
- [x] Model-shaped logic reviewed and deliberately NOT moved: `ensure_wallet_state`
      and the epoch rules are *sequenced filesystem effects*, not pure functions
      over injected state. Faking a filesystem to relocate them would buy
      nothing the carry model wants. The seam stayed where W0 drew it.

### W3: the grant envelope ruling

- [x] Verified: `p2panda_core::cbor` is a thin wrapper over `ciborium`
      (`encode_cbor` is `ciborium::ser::into_writer` into a `Vec`, `decode_cbor`
      is `ciborium::from_reader`) with no framing of its own. Byte-compat with a
      direct ciborium dependency therefore holds by construction.
- [x] Fixture pinned: `signed_device_grant_wire_format_is_pinned` encodes a
      fully deterministic signed grant and asserts its 612 bytes exactly, so any
      future codec swap has to come argue for itself. (Observation recorded, not
      acted on: `[u8; 32]` keys serialize as CBOR arrays of integers rather than
      byte strings, which costs about 2x on every key. That is the format in the
      wild; changing it is a migration, not a cleanup.)
- [x] **RULED: the envelope codec STAYS in the adapter** — and the reason is not
      byte-compat, which holds. It is that `personae::delegation` already exists
      and is the same concept: an issuer delegating a scoped, time-bounded
      capability to a subject, with attenuation and revocation. The two models
      disagree on everything underneath:

      | | `DelegationCertificate` (personae) | `DeviceGrantPayload` (here) |
      |---|---|---|
      | signing | hand-rolled domain-separated bytes | canonical CBOR envelope |
      | scope | typed `CapabilityScope` with `attenuates()` | `Vec<String>` atoms |
      | attenuation | `remaining_delegation_depth: u16` | `Vec<String>` atoms |
      | carriage | none | wrapped private-epoch material |

      Moving the envelope in as-is would install a second delegation model beside
      the first, inside the one crate that owns delegation. That is the
      duplication the generalize-don't-duplicate rule exists to stop.
- [x] **Spun out**: whether a device grant should BECOME a
      `SignedDelegationCertificate` (device-scoped `CapabilityScope`, depth
      instead of `no-subdelegation`, wrapped epochs as a side-carriage) is a
      design question, not a mechanical move. It needs its own brief before
      anything relocates. Castellan reaches grant verification through that
      reconciliation, not through a copy.
      **Answered 2026-08-11** in
      [device grants and delegation certificates](../technical_architecture/2026-08-11_device_grant_delegation_reconciliation.md):
      yes, but as a split rather than a move. The capability statement becomes
      a certificate, the wrapped epoch material leaves the signed envelope for
      its own record, and revocation gains a portable signed statement while
      the roster demotes to a local fold. Execution is gated on the migration
      posture, which is Mark's call.
      **Settled 2026-08-12**: re-issue now, no legacy decoder. Executed the
      same day; the
      [migration plan](../../archive_docs/2026-08-18_completed_plans/2026-08-12_device_grant_certificate_migration_plan.md)
      was archived 2026-08-18, complete, nothing carried forward.
- [x] `wallet_grant.rs` (2803 lines) split in place into fourteen modules,
      largest 432: `errors` + `types` (pure shapes), `envelope` (the CBOR grant
      and its on-disk read/write), `pairing`, `epochs`, `records`, `wrapping`,
      `validate` (one concern each), and `issue` / `enroll` / `revoke` /
      `refresh` (the four flows that write wallet state).

### W4: consumers re-base + closure

- [x] **knot re-base: ruled unnecessary, facade stays.** knot turned out to
      consume the *adapter*, not the model: `list_personas`,
      `load_current_private_epoch`, `ensure_local_device_identity`, and
      `ensure_wallet_state`, plus three carry types in one test module. Pointing
      those three at `identity::carry` would split one import into two and add a
      personae dependency to knot for nothing. The facade is doing exactly its
      job, which is letting a store consumer not care where the model lives.
- [x] The personae founding doc's roadmap item 1 now records what actually
      moved and what deliberately did not; trust-plane memory updated.
- [x] personae publish deferred per the republish-timing rule: no consumer
      outside mere wants carry yet. Castellan will be the first, and its own
      plan carries the bump.

## Noted in passing, not fixed

`session-runtime` has three other files over the 600-line ceiling that have
nothing to do with the wallet: `athanor.rs` (840), `graph_engram.rs` (817),
`manifest_store.rs` (655). Left alone deliberately; they are their own task.

## Progress

- 2026-08-10 (W2): the split landed with all 245 session-runtime tests green
  and zero warnings; knot still compiles untouched. Two mechanical lessons:
  PowerShell `.Replace()` with LF-joined literals silently no-ops against
  CRLF files (use the Edit tool for multi-line patches), and giving each test
  module `use super::super::*;` resolves the re-export surface in one line
  instead of hand-threading dozens of imports.
- 2026-08-10: W0 map verified against code (couplings, consumers, sizes,
  Hash repr). W1 executed and green the same session. Two surprises, both
  benign: `RecoveryPolicy` was missed from the first re-export list (caught by
  the compiler), and the wallet tests construct refs directly so they needed
  the same `CarryRef::of` spelling (digests unchanged). wallet_grant's whole
  diff was 9 insertions / 10 deletions, which is the payoff of converting at
  the producers instead of the touchpoints.
