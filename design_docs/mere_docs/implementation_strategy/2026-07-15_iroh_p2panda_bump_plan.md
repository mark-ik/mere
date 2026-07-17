# Fork iroh + p2panda; move retinue to the final crypto line — plan

**Status:** superseded and executed against upstream releases, 2026-07-17.
The fork strategy below is retained as the historical decision it replaced.
Mere now uses p2panda 0.7.0, iroh 1.0.2, iroh-blobs 0.103, and iroh-tickets 1.0.
The upstream wire is accepted: operation headers omit timestamps and use `u32`
sequence/payload sizes; p2panda-net session frames use postcard while core
operations remain canonical CBOR.

The migration is intentionally identity-breaking across the 0.6/0.7 boundary.
Old signed headers cannot be rewritten without changing operation ids. A 0.6
profile must export semantic application events with the old build and re-author
them into fresh 0.7 logs. The shared store reports that boundary explicitly when
legacy operation bytes fail to decode.

Current completion: the upstream API migration, explicit store boundary,
delegation live-sync proof, aggregate delegation drop carriage, read-only
participant projection, and revocation-derived scope-key epochs are landed.

## Superseded decision

mere's retinue-backed reticulum transport (R5, landed 2026-07-15) forced retinue
onto the **stable dalek-2 / sha2-0.10** line, because the current graph (iroh 0.98
via p2panda-net 0.6.1) hard-pins `ed25519-dalek =3.0.0-pre.6` and
`sha2 =0.11.0-rc.5` — exact prereleases that can neither unify nor split against a
caret. Rather than downgrade retinue or passively adopt upstream's breaking
changes, **we fork iroh and p2panda and control the pins + the wire.**

Two upstream breaking changes in the 0.7 / 1.0 line motivated this: p2panda-net's
wire codec flip **CBOR→postcard**, and p2panda-core's **operation-header
stabilization** (drop `timestamp`, narrow `seq_num` u64→u32). We want neither on
mere's wire. Forking lets us take the one thing worth taking — iroh 1.0's
stabilization + the modern crypto line — and reject the rest.

## The three forks/pins

### 1. iroh fork → final dalek

Fork `n0-computer/iroh`; repin its crypto from the exact rc to **final**:
`ed25519-dalek =3.0.0-rc.0 → 3.0.0`, `curve25519-dalek =5.0.0-rc.0 → 5.0.0`
(x25519 follows). All three finals are published and self-consistent
(`ed25519 3.0.0` and `x25519 3.0.0` both want `curve25519 ^5.0.0`; `curve25519
5.0.0` final is out). This lets retinue run **final** dalek with **no rc
lockstep** — strictly better than mirroring iroh's rc.0.

- **Risk to verify at execution:** iroh's code must compile against dalek 3.0.0
  *final*, not just rc.0. The rc→final delta is usually API-stable; if not, the
  fork carries a small code patch. This is the iroh fork's one real unknown — a
  build settles it.
- iroh is a large multi-crate workspace; the fork is stewardship (rebase on
  upstream). The pin change itself is ~one line.

### 2. p2panda fork → revert postcard + restore the old header

`mark-ik/p2panda` already exists as a clean mirror at upstream `v0.7.0` (no custom
commits). Add two curations:

- **Revert postcard → CBOR.** `git revert` of `26f0b883` (net: framed postcard
  codec; restores the deleted `p2panda-net/src/cbor.rs`, ~350 lines, localized to
  net + discovery/sync actor call sites) and `365208c0` (sync: the 3 serde-attr
  removals that postcard needed).
- **Restore the 0.6.1 operation header.** Re-add `Header.timestamp` and flip the
  `SeqNum` type alias (`p2panda-core/src/logs.rs`) back to **u64**. Verified
  decoupled: net/store/sync reference only the address-book `HybridTimestamp`, not
  the operation-header field, and seq flows through the single `SeqNum` alias.
  - **Wire-critical:** the restored header must serialize/sign/hash **identically
    to 0.6.1** for mere's stores to stay valid. Do this by reverting the specific
    header commits (#1194 seq, #1195 timestamp, #1196 encoding) rather than
    hand-editing, and verify against a 0.6.1 operation-hash fixture.

### 3. retinue → final crypto line (atomic with the mere pin-flip)

```toml
ed25519-dalek = "3.0.0"                                        # final, caret
x25519-dalek  = { version = "3.0.0", features = ["static_secrets"] }
sha2 = "0.11"
hkdf = "0.13"
hmac = "0.13"
aes  = "0.9.1"   # unchanged
cbc  = "0.2.1"   # unchanged
```

This reverts the dalek-2 / digest-0.10 pins committed on 2026-07-15. It **cannot
land before** mere points at the forked iroh: current iroh 0.98 pins
`ed25519-dalek =3.0.0-pre.6`, which conflicts with `3.0.0` just as hard. The
retinue pin-flip and the mere fork-repoint are **one atomic change**. retinue's
source needs no edits (the public wrapper types are unchanged; dalek 2→3 final is
API-stable for the surface retinue uses).

## mere side: manifest-only

Because the forked p2panda-core keeps the 0.6.1 header and CBOR wire, **mere needs
no source migration** — the ~10-crate M+9S surgery from the upstream-bump plan
evaporates. mere only:

- Repoints `iroh` / `iroh-blobs` / `iroh-tickets` / `iroh-gossip` at the iroh fork
  (git dep, `mark-ik/iroh`), and `p2panda-*` at the p2panda fork (git dep,
  `mark-ik/p2panda`), via `[patch]` in the committed manifest + the local
  `.cargo/config.toml` redirect (same pattern as genet/cambium/armillary).
- Bumps root `hkdf` 0.12→0.13, `sha2` 0.10→0.11 to share retinue's digest copy.
- Closes the latent `meerkat` gap: it references `p2panda-sync` in `sync.rs` but
  doesn't declare the dep.

**No wire break, no store invalidation, no dev-data wipe** — the whole point of
the header/codec revert.

## Two corrected non-issues

- **retinue has exactly one consumer: mere-transport.** (An earlier synthesis pass
  wrongly named Beechat as a consumer; Beechat *is* the old `reticulum` 0.1 crate
  retinue replaced — no repo, no consumer.)
- The p2panda fork is genuinely needed now (for the reverts), unlike the plain
  bump where crates.io 0.7.0 would have sufficed.

## Sequencing

- **Fork prep is independent of mere's tree** — the forks are separate repos; build
  and verify them while the gemot→moot refactor is still in flight.
- The **final mere repoint + retinue pin-flip** is the only tree-touching step, and
  it is now **manifest-only** (Cargo.toml `[patch]` + version pins), so it barely
  collides with the refactor. Land it once the refactor settles, as one atomic
  commit-pair (mere `[patch]` + retinue crypto block).

## Risks

- **iroh-on-final-dalek** (highest) — verify iroh compiles against dalek 3.0.0
  final; carry a code patch in the fork if the rc→final delta bites.
- **Header-restore wire fidelity** (sharp) — the forked header must hash identically
  to 0.6.1; verify against a fixture, don't eyeball it.
- **Fork maintenance** — two more owned forks to rebase on upstream (iroh is large).
  Accepted: it's the same steward-the-stack posture as genet/stylo/boa/cambium.
- **Single-copy unification** — after repointing, `cargo tree -d` must show one
  iroh/iroh-base/curve25519 copy and one shared dalek-3-final between retinue and
  the iroh fork.

## Execution recipe (when greenlit)

1. **p2panda fork:** revert the 2 postcard commits + the 3 header commits on
   `mark-ik/p2panda`; `cargo test` the p2panda workspace; verify a 0.6.1
   operation-hash fixture round-trips. Push.
2. **iroh fork:** fork `n0-computer/iroh`, repin dalek/curve/x25519 → final;
   `cargo build` iroh + iroh-blobs + iroh-gossip + iroh-tickets; patch code only if
   final-dalek needs it. Push.
3. **retinue:** flip the crypto block to final (above); `cargo test` from retinue's
   own cwd. Do not commit-build against current mere (atomicity).
4. **mere (after refactor settles):** `[patch]` iroh + p2panda at the forks, bump
   root digest pins, add meerkat's `p2panda-sync`; `cargo tree -d` unification
   gate; `cargo check --workspace`; `cargo test -p mere-transport --features
   reticulum`. Commit the mere `[patch]` + retinue crypto flip together.

## Counter-option (rejected)

Keep retinue on dalek-2 (current committed state) and never bump. Zero fork
maintenance, but retinue stays on the older crypto line indefinitely and the diff
only grows. Rejected: the fork posture is already how the rest of the stack is
stewarded, and it buys the modern crypto line *and* wire stability at once.
