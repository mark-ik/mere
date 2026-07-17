# personae

The identity and carry layer for the Merely ecosystem (mere, isometry,
strophe, woodshed). A person has *personae*, plural — a work face, a research
face, a burner — and this crate is the register of them plus the root of trust
they derive from: the master Ed25519 keypair, deterministic per-protocol key
derivation (BLAKE3 keyed-hash), a passphrase- and OS-store-unlocked vault, and
sealed-record storage for secrets at rest.

```rust
use personae::{IdentityProvider, InMemoryProvider};

let provider = InMemoryProvider::random();
let cabal = provider.derive_keypair(b"a-32-byte-salt-for-this-cabal...").unwrap();
let sig = cabal.sign(b"hello");
assert!(cabal.public_key().verify(b"hello", &sig));
```

Production hosts can compose their unlock policy with
`SealedIdentityProvider::load_or_create`. The provider owns the versioned master
seed record; each app chooses where it lives and how its `SealedRecordStorage`
is unlocked.

`IdentityProvider::attest_derived_key` lets any app prove that a short-lived or
protocol-scoped derived key was authorized by the durable master identity,
without signing application traffic directly with the master key.

Promoted from mere's `persona/identity`. The carry layer — device roster,
capability grants, private-epoch history, the portable-persona spine that lets a
persona and its data move between your devices — folds in as it lifts out of
mere's `session-runtime`; personae is the whole trust-plane root, not only the
key primitives. (This subsumes the crate that was going to be named `signet`:
one name for the faces and how they carry.)

## License

Dual-licensed under MIT or Apache-2.0, at your option. The name is the plural of
*persona*; unrelated to Mozilla's discontinued Persona / BrowserID.
