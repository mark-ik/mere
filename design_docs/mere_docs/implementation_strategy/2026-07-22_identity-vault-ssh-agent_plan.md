# Identity Vault + SSH Agent Plan

**2026-07-22.** Turn personae's existing vault skeleton into a working credential
vault with an SSH-agent front end. This is the "our own 1Password/KeePassXC
alternative" utility, scoped honestly: protocol credentials and SSH keys first,
general password-manager item types later, sync later still.

Continues [`2026-05-05_protocol_architecture_plan.md`](2026-05-05_protocol_architecture_plan.md)
§3 (Identity Vault, Phase 2C), which `personae/src/vault.rs` cites as its
source. That section already resolved the shape (Direct vs Bootstrap slots,
`CredentialLineage`, unlock tiers, storage-backend table, the level-0
single-process threat model). This plan does not re-litigate those decisions;
it wires them.

## Motivating pull

A real credential exists today with no acceptable home: the Claude-session SSH
key for the Linux laptop sits plaintext in `~/.ssh/` on the Windows machine,
and the Windows ssh-agent service is disabled. Constraint from Mark: key
material never enters any GitHub repo, and plaintext at rest is not
acceptable. The first dogfood target is exactly this key: vaulted at rest,
served through our own agent, plaintext file deleted.

This mirrors how the clipboard capability landed (Hocket's hand-off pulled
genet-clipboard into being): a concrete consumer first, generality second.

## What already exists (verified in code, 2026-07-22)

In `repos/personae` (single crate, MIT/Apache):

- `vault` module: `ProfileId`, `ProtocolKey`, `CredentialLineage`,
  `UnlockTier`, `SecretBytes`, `IdentitySlot` (Direct slots explicitly include
  SSH keys in the doc header), `Profile`, `IdentityStorage` trait,
  `IdentityVault<S>`, `InMemoryStorage`. A v0 skeleton per §3; compiles and
  has tests, but no durable backend.
- `passphrase_root` + `passphrase_storage`: `PassphraseEncryptedStorage`,
  Argon2id-derived KEK, multi-slot.
- `seal` + `sealed_record_storage`: OS-keychain-backed sealed records
  (DPAPI on Windows). In production use: hocket-genet's identity rides this.
- `startup_unlock`: OS-protected auto-unlock.
- `delegation`: `DerivedKeyAttestation` (used by Hocket hand-off envelopes).

So encryption at rest, unlock, and the slot model exist. The gaps are wiring
and front ends.

## Landscape (why build, not adopt)

- **Vaultwarden** (Rust): a Bitwarden-compatible *server*; clients are the
  official non-Rust apps. Requires running a server; agent behavior is the
  Bitwarden client's, not ours.
- **ripasso / kbs2 / estash**: thin stores; none has a polished SSH-agent
  story or a sync model that fits this ecosystem.
- No mature pure-Rust 1Password-class application exists.

Differentiators if built here: local-first with P2P sync on the family spine
(iroh / p2panda / retinue, so vault sync can eventually ride the mesh radio),
unification with the identity layer (secrets live where the master key already
lives), and an agent whose approval policy the user controls (automation
friendly instead of prompt-per-use, per
[`feedback_configurability_over_opinionated_defaults`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md)).

## Phases

### V1. Durable storage backend

Done condition: a `Profile` with an `Ssh` slot survives process restart,
encrypted at rest, on Windows and Linux.

- Implement `IdentityStorage` over the existing at-rest layers, per the §3.2
  table: `OsKeychainStorage` (sealed_record_storage; desktop default) and
  `PassphraseEncryptedStorage` (portable file vault).
- Serialization: CBOR via ciborium, matching the rest of the family.
- `SecretBytes` zeroization audit while touching this code.
- No new crate. This is personae-internal wiring
  ([`feedback_check_existing_crates_before_new_module`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_check_existing_crates_before_new_module.md)).

### V2. SSH agent front end

Done condition: `ssh markik@<laptop>` from the Windows machine authenticates
via the agent with the key served from the vault, and the plaintext
`~/.ssh/id_ed25519` is deleted.

- New bin target in the personae repo (name TBD by Mark; working name
  `personae-agent`). Deps: [`ssh-agent-lib`](https://crates.io/crates/ssh-agent-lib)
  (MIT/Apache, has `NamedPipeListener` for Windows plus Unix sockets) and
  RustCrypto [`ssh-key`](https://crates.io/crates/ssh-key) for key
  encode/decode and signatures.
- Windows: listen on the OpenSSH named pipe (`\\.\pipe\openssh-ssh-agent`)
  so stock `ssh.exe` finds it, with the stock ssh-agent service left
  disabled. Unix: socket + `SSH_AUTH_SOCK`.
- Signing requests honor the slot's `UnlockTier` (§3.6): Session tier signs
  silently after vault open; Per-use tier requires confirmation. Approval
  policy is a setting, not hardcoded.
- Import path for existing keys (OpenSSH private key format in, `Ssh` slot
  stored, original shredded on explicit user confirmation only).

### V3. Vault CLI

Done condition: list/add/remove/inspect slots and profiles from a terminal,
including "what does losing this device mean for this slot" per §3.4.

CLI before UI: the agent + CLI pair is the useful utility. A mere pane
(Steward-adjacent, per the pane taxonomy) comes after the surface is proven.

### V4. Broader item types

Passwords, TOTP, secure notes as additional slot kinds (or a parallel
item enum if protocol-slot semantics do not fit). Scope-check against real
need before building; §3's `Custom` slot may already cover it.

### V5. Sync (deferred, gated)

`IdentityStorage` backend replicated over the murm/moot spine. Explicitly
deferred until the moot refactor (and the iroh/p2panda dalek-3 fork work)
lands; secrets sync is the highest-stakes payload and goes last, on a spine
that already syncs lower-stakes data. Not part of the utility's first cut.

### Gate: auto-update before load-bearing

Mark's requirement, 2026-07-22: before this (or any app) becomes a
load-bearing deployment, the family needs a configurable auto-update story.
See [`2026-07-22_auto-update_brief.md`](../../2026-07-22_auto-update_brief.md).
V1-V3 dogfood on this machine pair is fine pre-update-story; wider
installation is not.

## Threat model note

§3.7's level-0 assumption carries over: single process, at-rest encryption
defends passive attacks (disk theft, swap), not in-process compromise. The
agent adds one genuinely new surface: any local process can speak to the
agent socket/pipe. Same exposure as stock ssh-agent; per-use tiers and
(later) client-binding raise the bar. Name this in the CLI docs; do not
oversell.

## Findings

- 2026-07-22: `ssh-agent-lib` confirmed to support Windows named pipes
  (NamedPipeListener) and Unix sockets; MIT-licensed; active.
- 2026-07-22: personae's at-rest layers (sealed records, passphrase storage)
  are already production-exercised by hocket-genet identity, so V1 is backend
  wiring, not new cryptography.
- 2026-07-22 (V1): `PassphraseEncryptedStorage` already implemented
  `IdentityStorage` in full (Argon2id + ChaCha20-Poly1305, atomic writes,
  wrong-passphrase detection, tested end-to-end with `IdentityVault`), so
  the portable-file half of V1 predated this plan. The missing half was the
  OS-auto-unlock composition; the vault trait doc claiming
  "OsKeychainStorage remains the follow-up" was the accurate statement and
  is now updated.
- Profile ids are user-chosen strings, so the sealed backend hashes them
  (blake3 hex) for filenames and stores the id inside the record; listing
  decrypts each record (it needs display_name anyway).

## Progress

- 2026-07-22: plan written. Prior art re-read (§3 of the protocol
  architecture plan); vault.rs verified against it.
- 2026-07-22: **V1 done.** New `personae::SealedProfileStorage`
  (src/sealed_profile_storage.rs): `IdentityStorage` composed over
  `SealedRecordStorage` + the `startup_unlock` AutoOs ladder
  (`open_with_key` for any key ladder, `open_auto_os` returns Ok(None)
  where the ladder is unimplemented, honest degraded state). Shared
  profile wire shape factored into `profile_wire.rs` (used by both on-disk
  backends; passphrase file format unchanged). 52 tests green including
  8 new (round-trip, reopen, wrong-key, hashed-filename listing, awkward
  ids, delete, vault end-to-end, Windows DPAPI auto-os round-trip);
  clippy clean. Serialization note: the wire is serde JSON inside the
  encrypted envelopes (inherited from the existing backends), not CBOR as
  this plan first guessed; staying with the incumbent format, one shape,
  no migration.
- 2026-07-22: **V2 built and live-verified.** New `personae::agent` module
  (feature `agent`) + `personae-agent` bin (`required-features` gated, so
  the published lib's default dep tree is unchanged). `VaultAgent<S>`
  implements ssh-agent-lib 0.6's `Session`: request_identities / sign /
  add_identity / remove_identity / remove_all_identities over the vault's
  `ssh` slots (Direct slots, payload = OpenSSH-encoded private key,
  ProtocolKey instance = SHA256 fingerprint, so re-adds are idempotent).
  Per-use tier refuses to sign (no confirmation UI yet, refusing keeps the
  tier honest); short-TTL treated as Session until relock lands. 58 tests
  green (6 new agent tests incl. sign-verify round trip and per-use
  refusal); clippy clean. Also added the blanket
  `IdentityStorage for Box<T>` impl so the bin picks its backend at
  runtime (auto-OS default, `PERSONAE_PASSPHRASE` env for the portable
  vault).
- 2026-07-22: **live receipt on the real machine pair.** Stock Windows
  `ssh-add` imported the session key into the DPAPI-sealed vault
  (`%LOCALAPPDATA%\personae\vault`, profile auto-created), `ssh-add -l`
  listed it, and `ssh` to the Fedora laptop succeeded **with the plaintext
  key file moved aside**, so the signature provably came from the vault
  through the agent on the stock `\\.\pipe\openssh-ssh-agent` pipe (stock
  service remains disabled). Plaintext file restored afterward; shredding
  it is Mark's call at cutover.
- 2026-07-22 **ruling (Mark): the agent's resident home is
  mere/Graphshell** (the reclaimed family projection host, not the
  archived donor). `personae::agent` stays a library module precisely so
  the resident host process serves the pipe/socket in-process; that host
  is also where the per-use confirmation UI, the vault pane, and
  lock/unlock control live. Launch-at-login mechanics come from the host,
  not from a standalone agent install. A scheduled task (or shell-profile
  lazy start) is interim dogfood scaffolding only, and is removed when the
  host adoption lands.
- 2026-07-22: **interim launch installed (Mark approved the scheduled
  task).** Release exe + `personae-agent.vbs` at
  `%LOCALAPPDATA%\personae\bin`; logon-triggered task `personae-agent`
  for the user (no elevation needed), no execution time limit,
  battery-safe. **Crash recovery lives in the wrapper, not the task**:
  the first cut relied on Task Scheduler's restart-on-failure and a
  kill test proved that policy only covers failure to *launch* (the
  killed agent's 0xFFFFFFFF exit was recorded as a completion, no
  restart). The wrapper now relaunches the agent in a loop with a 5s
  pause; re-test: killed agent back in under 12s and signing logins.
  Stopping for real = stop the task, then stop the process. Agent logs to `%LOCALAPPDATA%\personae\agent.log` via the
  new `--log-file` flag. Also implemented the `session-bind@openssh.com`
  and `query` agent extensions (modern OpenSSH sends session-bind on every
  connection; v1 verifies the binding signature and acknowledges,
  constraint enforcement deferred), which cleared the per-connection
  UnsupportedCommand log error. Live receipts: task-hosted agent answers
  `ssh-add -l` and signs a real laptop login; redeploy procedure = stop
  task + stop process, copy exe, start task (the exe stays locked while
  running). All of this is the interim scaffolding the mere/Graphshell
  host ruling retires.
- 2026-07-22: **V2 done condition met.** Plaintext `~/.ssh/id_ed25519`
  deleted (filesystem delete; a secure wipe would not guarantee physical
  erasure on SSD anyway); `.pub` kept for reference. Post-deletion login
  to the laptop verified: the vault is now the only local holder of the
  key. Recovery story if the vault dir or its DPAPI root is lost:
  register a replacement key on the laptop, per the
  LocallyGeneratedExternallyRegistered lineage.
- 2026-07-22 **intent (Mark): personae folds into the mere repo**
  (family-repo pattern, like eidetic/conatus). Not executed yet;
  repos/personae stays the standalone home and publish source until the
  merge. Shapes V3+: the vault CLI and pane land knowing they end up
  in-tree with mere/Graphshell, and the fold retires the cross-repo
  path-dep dance for mere itself. Consumers (hocket, woodshed) repoint at
  fold time.
- 2026-07-22: **V3 done.** `personae-vault` CLI (bin gated on a new `ssh`
  feature; `agent` now implies it) with profiles / new-profile / list /
  show / add-ssh / pub / remove. Secret bytes are never printed (slots
  report size, kind, category, tier); `show` prints the §3.4 device-loss
  note, now a library method `CredentialLineage::device_loss_note` so the
  future vault pane shows the same words. Slot keys accept a unique
  prefix (`show ssh:SHA256:d3tQ`), with ambiguity listing candidates.
  Three extractions came out of the work rather than being duplicated:
  `bootstrap` (backend selection by unlock ladder + profile
  load-or-create, returning an honest description of what protects the
  secrets, now used by both bins and ready for the mere/Graphshell host),
  `ssh_slot` (slot shape + encode/decode/find, shared by agent and CLI),
  and an `IdentityStorage for &T` blanket impl beside the `Box<T>` one.
  66 lib tests + 5 CLI tests green, clippy clean on both feature sets.
- 2026-07-22 V3 receipts: read-only commands against the **real** vault
  list the live SSH slot; `pub` exported a public key byte-identical to
  the `.pub` file and to the laptop's `authorized_keys` (the recovery
  path now that the private file is gone). Mutating commands were
  exercised against a throwaway vault (add / idempotent re-add with a
  changed tier / remove / error paths), then deleted. The agent was
  rebuilt and redeployed after the refactor and re-proved a real login,
  so the running binary matches the code.
- Findings from V3: the encrypted-key guard works (a passphrase-protected
  key is refused with the `ssh-keygen -p` fix, rather than being stored
  in a form the agent could not sign with); `PassphraseEncryptedStorage`
  writes its file on first save, not at open, so "vault exists" is not a
  post-open invariant.
- Follow-ons noted, not built: interactive passphrase prompt for
  importing encrypted keys (needs a terminal-prompt dep); a `generate`
  command that mints a key straight into the vault so no plaintext file
  ever exists; ShortTtl relock enforcement.
- Next: V4 (broader item types) is optional and demand-driven; the
  higher-value next step is the auto-update pressure test from the
  [auto-update brief](../../2026-07-22_auto-update_brief.md), since it
  gates load-bearing deployment of any of this.
