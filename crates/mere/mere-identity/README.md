# mere-identity

`mere-identity` is the identity foundation for the
[mere](https://crates.io/crates/mere) browser. It owns the user's master
Ed25519 keypair, the per-protocol derivation algorithm, and the storage
abstractions for keeping the master secret at rest. Every other
identity-bearing crate in the workspace (`mere-transport`, `murm`, eventually
`moothold` and `eidetic`) builds on this trust root.

When a user runs mere, they have one identity. But that identity has to play multiple roles:

- a network identity (their iroh `NodeId` in mere-transport)
- a per-conversation identity (Ed25519 keys for each cabal in murm)
- a per-community identity across the tier framework (future moothold for t1–t3, demesne for t4)
- at-rest encryption keys (future eidetic)

You don't want to use one keypair for all of those: if one has a vulnerability, you don't want it to leak your master identity. You also don't want to store N separate keys forever; that's an attack surface and a backup nightmare.

mere-identity solves this by storing one master secret and deriving every other key from it on demand. Lose the master, lose everything; protect the master, protect everything.

## Design rules

- **The master secret never leaves the `IdentityProvider`.** All derivation
  happens inside the provider; consumers receive only the derived keypair
  for their salt.
- **Pure-Rust crypto** — `ed25519-dalek` + `blake3`, no `sodiumoxide` /
  libsodium dependency.

## What's in the crate

- **`provider`** — the trust root.
  - `IdentityProvider` (trait): `master_public_key()`, `derive_keypair(salt)`.
  - `InMemoryProvider` — reference impl for tests and ephemeral sessions.
- **`keypair`** — Ed25519 primitives. `Ed25519Keypair`, `Ed25519PublicKey`,
  `Ed25519Signature`. Sign / verify / round-trip through bytes.
- **`vault`** — multi-profile credential storage.
  - `IdentityVault` + `IdentityStorage` (trait) — profile-aware vault and its
    storage abstraction.
  - `Profile`, `ProfileId`, `ProfileSummary`, `ProtocolKey`, `IdentitySlot`,
    `CredentialLineage`, `UnlockTier`, `SecretBytes`.
  - `InMemoryStorage` — reference impl.
- **`passphrase_storage`** — `PassphraseEncryptedStorage`: Argon2 KDF +
  ChaCha20-Poly1305 AEAD for at-rest encryption when an OS keychain isn't
  available.
- **`error`** — `IdentityError`.

## Quick start

```rust
use mere_identity::{IdentityProvider, InMemoryProvider};

let provider = InMemoryProvider::random();
let _master_pubkey = provider.master_public_key();

// Derive a per-protocol keypair from a salt (e.g. a cabal id).
let cabal_salt = b"my-cabal-id-32-byte-salt-here..";
let cabal_keypair = provider.derive_keypair(cabal_salt).unwrap();

// Sign and verify.
let msg = b"hello, cabal";
let sig = cabal_keypair.sign(msg);
assert!(cabal_keypair.public_key().verify(msg, &sig));
```

## How it relates to other workspace crates

mere-identity is a leaf crate: it depends on no other mere workspace crate,
only third-party crypto. Consumers reach for it as the identity trust root.

```text
    murm   mere-transport   moothold (planned)   eidetic (planned)
      ▲          ▲                 ▲                   ▲
      │          │                 │                   │
      └──────────┴─────────────────┴───────────────────┘
                       │ master_public_key()
                       │ derive_keypair(salt)
                       ▼
                  mere-identity
                       │
                       ▼  IdentityStorage impl
              OS keychain | passphrase-encrypted | in-memory
```

- [`mere-transport`](https://crates.io/crates/mere-transport) — derives the
  iroh `NodeId` from the master public key, so the user's network identity
  is keyed to their identity root. The transport never sees the master
  secret; it consumes the public key for addressing.
- [`murm`](https://crates.io/crates/murm) — calls `derive_keypair(cabal_id)`
  for a per-cabal Ed25519 keypair. Each bilateral conversation gets its own
  derived identity; a vulnerability in the conversation protocol does not
  leak the master.
- [`moothold`](https://crates.io/crates/moothold) (planned) —
  moot / moothold / demesne credentials across the tier framework
  (t1 orrery → t2 moot → t3 moothold → t4 demesne), including
  capability-based delegation per the meadowcap pattern.
- [`eidetic`](https://crates.io/crates/eidetic) (planned) — at-rest
  encryption keys for owner-private memory.
- **OS keychains / hardware keys** (host-side) — implement `IdentityStorage`
  to keep the master seed off-disk in plaintext. `PassphraseEncryptedStorage`
  is the fallback for environments without keychain access.

## Status

Pre-1.0. The `IdentityProvider` trait surface is intended to stabilize
before 0.1.0. Today's storage backends are `InMemoryStorage` and
`PassphraseEncryptedStorage`; OS-keychain and hardware-key backends land
behind the same `IdentityStorage` trait.

**In flight per the [event-DAG substrate brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md):**

- BLAKE3 unification — derivation moves to BLAKE3 keyed-hash mode (master
  seed as key, salt as input), replacing the prior BLAKE2b-based pattern.
  Aligns with iroh's BLAKE3-native content addressing and the workspace's
  one-hash-function direction.
- Identity *system* on top of the cryptographic base — device enrollment,
  device revocation, profile recovery, contact discovery, key rotation with
  a signed key-history chain, multi-device sync, pseudonymous personas with
  reputation accumulating against the chain root. First-pass intuitions
  captured in the brief; concrete designs land iteratively.

## License

MPL-2.0.
