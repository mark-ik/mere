# personae

The identity and carry layer for the Merely ecosystem (mere, isometry, hocket,
woodshed). A person has *personae*, plural: a work face, a research face, a
burner. This crate is the register of them and the root of trust they derive
from, a master Ed25519 keypair with deterministic per-protocol key derivation,
a passphrase- or OS-store-unlocked vault, sealed-record storage for secrets at
rest, and a signed capability-delegation grammar.

Promoted from mere's `persona/identity`. Edition 2024, pure-Rust crypto
(`ed25519-dalek`, `blake3`, `argon2`, `chacha20poly1305`).

```rust
use personae::{IdentityProvider, InMemoryProvider};

let provider = InMemoryProvider::random();
let cabal = provider.derive_keypair(b"a-32-byte-salt-for-this-cabal...").unwrap();
let sig = cabal.sign(b"hello");
assert!(cabal.public_key().verify(b"hello", &sig));
```

Derivation is `BLAKE3-keyed(master_seed, salt)` to an Ed25519 seed. The master
secret never leaves the `IdentityProvider`; callers get only the derived
keypair. `IdentityProvider::attest_derived_key` returns a
`DerivedKeyAttestation` proving a derived key was authorized by the master
identity, so application traffic is never signed with the master key directly.

## Modules

| Module | Contents |
| --- | --- |
| root | `PersonaId`, `IdentityError`, `Ed25519Keypair`, `Ed25519PublicKey`, `Ed25519Signature`, `VERSION`, `STAGE`; and, from the private `provider` module, `IdentityProvider`, `InMemoryProvider`, `SealedIdentityProvider` (`load_or_create`), `DerivedKeyAttestation` |
| `vault` | `IdentityVault`, `Profile`, `ProfileId`, `ProfileSummary`, `IdentitySlot`, `ProtocolKey`, `SecretBytes`, `UnlockTier`, `CredentialLineage`, `IdentityStorage`, `InMemoryStorage` |
| `bootstrap` | `Unlock`, `PASSPHRASE_ENV`, `load_or_create_profile`; the standard backend-selection and profile-opening ceremony |
| `passphrase_storage` | `PassphraseEncryptedStorage`, an Argon2id + ChaCha20-Poly1305 on-disk vault |
| `passphrase_root` | `PassphraseWrappedRoot`, `wrap_vault_root`, `unwrap_vault_root`, `save_passphrase_root`, `load_passphrase_root`, `passphrase_root_exists`, `change_passphrase` |
| `startup_unlock` | `StartupUnlockMode`, `auto_unlock_backend_available`, `load_or_create_auto_unlock_root`. `AutoOs` is implemented on Windows (DPAPI) only; other platforms return `None` |
| `seal` | `seal_bytes` / `unseal_bytes`, XChaCha20-Poly1305 with a prepended random nonce |
| `sealed_record_storage` | `SealedRecordStorage`, one sealed typed serde value per path |
| `sealed_profile_storage` | `SealedProfileStorage`, the `IdentityStorage` backend over sealed records |
| `delegation` | `DelegationCertificate` / `SignedDelegationCertificate`, `DelegationRevocation` / `SignedDelegationRevocation`, `DelegationId`, `DelegationParent`, `CapabilityScope`, `DelegationError`, `delegation_signing_salt` |
| `signing` | `ApprovalBroker`, `SigningRequest`, `SigningPolicy`, `SigningDecision`, `SigningAuthorization`, `SigningRecord`. Feature `agent` |
| `ssh_slot` | `SshSlot`, `slot_for`, `private_key_from_slot`, `ssh_slots`, `find_by_public`, `protocol_key_for`, `SSH_MOD_ID`. Feature `ssh` |
| `ssh_ca` | `SshCertAuthority`, `UserCertRequest`, `HostCertRequest`, `CertMintError`, `self_grant`, `key_id_for`, `ssh_ca_salt`, `MAX_CERT_TTL_MS`, `SSH_CA_MOD_ID`. The delegation grammar projected into OpenSSH certificates. Feature `ssh` |
| `ssh_face` | `FacePolicy` (`work`/`research`/`burner`), `load_policy`, `store_policy`, `effective_policy`, `policy_key`, `SSH_FACE_MOD_ID`. What one face may do over SSH. Feature `ssh` |
| `enroll` | `user_trust_line`, `user_install_script`, `system_sshd_snippet`, `known_hosts_line`, `device_id_for_host`, `local_device_id`, `local_host_name`, `split_target`, `ENROLLMENT_MARKER`. Feature `ssh` |
| `agent` | `VaultAgent`, an `ssh-agent-lib` session over an `IdentityVault`. Feature `agent` |

## Features and binaries

| Feature | Pulls | Enables |
| --- | --- | --- |
| `ssh` | `ssh-key`, `signature` | `ssh_slot`; the `personae-vault` bin |
| `agent` | `ssh` plus `ssh-agent-lib`, `tokio`, `tracing-subscriber` | `agent`, `signing`; the `personae-agent` bin |

`personae-agent` serves vault `ssh` slots over the OpenSSH agent protocol
(Windows named pipe or Unix socket), each offered twice: once as a
freshly-minted personae certificate and once as the bare key, so a host that
trusts the authority needs no per-key enrollment and a host that only knows
the key still works. `personae-vault` inspects and manages the
vault from a terminal. Both are feature-gated so the default dependency tree
stays lean. `install-agent-windows.ps1`, `install-agent-macos.sh` and
`install-agent-linux.sh` build both bins and register a login job on their
platform (Task Scheduler plus a VBS relaunch loop, launchd, systemd
`--user`). None of them handles the passphrase: each reads it at start from
that platform's own secret store.

Windows additionally pulls `windows-sys` for the DPAPI unlock backend.

## Scope

The carry layer (device roster, capability grants, private-epoch history, the
portable-persona spine that moves a persona and its data between devices) folds
in as it lifts out of mere's `session-runtime`. This crate subsumes what was
going to be named `signet`: one name for the faces and how they carry.

Design notes live in `design_docs/` beside this file.

## License

MPL-2.0 (see LICENSE). The name is the plural of *persona*,
unrelated to Mozilla's discontinued Persona / BrowserID.
