# Family-Shared Identity: One Persona Across Every Merely Application

**2026-08-08.** Turnstone, Woodshed, Knot, Hocket and Cleromancy are separate
products and the same person's. A document sealed in one should open in the
next. They did not agree on where identity lives, so it did not.

Continues [`2026-07-22_identity-vault-ssh-agent_plan.md`](2026-07-22_identity-vault-ssh-agent_plan.md),
which built the vault and the opening ceremony (`personae::bootstrap`). That
plan answered *what protects the secrets*. This one answers *whose secrets* —
which persona an application opens on, and how every application agrees.

## The two roots, and why there were two

Identity was split across two lanes that had grown independently:

1. **The personae vault** — `%LOCALAPPDATA%\personae\vault`, a fixed
   per-machine path holding named profiles, unlocked by DPAPI or a passphrase.
   Turnstone's root identity already rode this, as does Hocket's, and the SSH
   agent serves it.
2. **The session-runtime wallet** — `identity/` and `personas/` under whatever
   data root the application passed. `ensure_local_device_identity` mints a
   device key *there*, so each application minted a different one.

Lane 1 was already shared and lane 2 was not. Worse, lane 1 was shared by
accident of the fixed path rather than by decision: every consumer hardcoded
`ProfileId("default")`, in five places across mere and turnstone. A vault
holding a work persona beside a personal one had no way to say which was in
use, and nothing could switch.

## What landed

### `session_runtime::shared_root` — one wallet root (mere)

`shared_root()` resolves to the platform data dir plus `mere`, overridable with
`MERE_ROOT`. `adopt_legacy_identity(shared, legacy)` **moves** an application's
private `identity/` and `personas/` into it on first access, refusing to
overwrite shared identity that already exists.

Moving rather than copying is deliberate: two copies of an identity is the
worse failure, because they diverge and nothing can say which is authoritative.
Adoption happens on access rather than as a migration step, so an application
that has not run since the split still finds its persona the first time it
looks.

### `personae::roster` — which persona (mere)

The missing half of `bootstrap`. It lists what the vault holds, remembers which
one was picked, and opens that one:

- `read_roster(storage, dir, description) -> Roster` — entries sorted by id,
  the chosen one marked, plus the backend's honest account of what protects it.
- `resolve_profile(storage, dir)` — the ladder: `PERSONAE_PROFILE`, then the
  remembered choice, then **the vault's sole persona when it holds exactly
  one**, then `"default"`.
- `remember_profile(dir, id)` / `chosen_profile(dir)`.
- `create_profile(storage, id, display_name)` — refuses to mint over an
  existing persona, because that would replace its master key and with it every
  certificate rooted on it.
- `open_shared(unlock)` — the one call an application makes.

The sole-persona rung exists for a specific trap: falling straight through to
`"default"` on a vault holding one persona named something else would mint a
second identity beside the only one the user has.

**The choice lives beside the vault, not in an application's data directory.**
That is the whole point — picking a persona in one application picks it in all
of them. It is a profile id in a plain text file; nothing secret, since the
persona names are already visible to anyone who can list the directory.

10 tests. `switching_personas_switches_the_derived_key` is the one that states
the property the lane exists for.

### `mere-persona-picker` — the shared list (mere)

A Cambium view over a `Roster`, built on `command_picker`, which already
supplies the interaction. This crate is the view-model: how a persona reads in
a row, which one is marked in use, what a vault with nothing in it says.

A crate rather than a module in `personae` because the view needs Cambium and
Cambium drags the whole Genet stack; `personae` is consumed by headless bins,
knot endpoints, and the retinue side of the family, none of which should build
a view toolkit to derive a key.

`roster_items(&Roster) -> Vec<CommandItem>` is public separately, so an
application showing personas in a menu or a settings pane gets the same reading
without the picker. Activation resolves by `ProfileId` taken off the drawn rows
rather than by index, so a roster that changes between render and activation
cannot select the wrong person; an activation that resolves to nothing is
dropped as a dismissal, which is the safe direction.

### `impl Backend for Box<B>` (muniment)

Which store a host uses is not always known when its types are named: Woodshed
seals to the chosen persona when the vault opens and writes plain files when it
does not, and both are the same store to everything above. Without this, that
choice is a hand-written enum per host delegating all six methods.

## Per-application state

| Application | State | What was done |
|---|---|---|
| **Turnstone** | wired | Knot's persona vault and device key come from `shared_root()`, on both the hosted and the spawned path. `adopt_legacy_identity` runs at boot. `HostedKnot::PersonaVault` lost its `data_root` field: a persona vault is never under turnstone's own root, and a field nothing reads is a lie about where things live. |
| **Woodshed** | wired | `open_store()` seals the practice session to the chosen persona, or writes plain files when no vault opens. |
| **Knot's binaries** | correct as-is | `knot_endpoint` and `knot_sync_host` take the root as an argument by design, so a test can point them at a scratch profile. The caller decides, and turnstone's caller now says the shared root. |
| **Cleromancy** | nothing to wire | It has no identity. `CLEROMANCY_ROOT` holds product state (the redb store, sync settings); `admitted.rs` says outright that identity and transport stay outside. |
| **Isometry** | nothing to wire | `isometry-net` takes an `Ed25519Keypair` as a parameter. No persistent identity exists yet; when one does, `roster::open_shared` is the call. |
| **Hocket** | **blocked, deliberately** | See below. |

### Woodshed's sealing is not a gate

Woodshed practiced without an identity before sealing existed, and a machine
with no vault backend — no DPAPI, no `PERSONAE_PASSPHRASE` — still has to be
able to open a tuner. So a vault that will not open is said out loud and
stepped over, never raised. `SealedBackend::adopting_plaintext()` carries the
migration: a session written before sealing was switched on is read once as it
stands, and the next save seals it. Nothing to run, and nothing to run in the
right order.

This closes the "deferred: host wiring" item on woodshed's
[`2026-07-08_personae_sealed_session.md`](../../../../../woodshed/design_docs/2026-07-08_personae_sealed_session.md).

### Hocket is a key rotation, not a wiring change

`hocket-genet/src/identity.rs` holds a live app-private identity: a
`SealedIdentityProvider` under Hocket's own data root, with a **contact token**
(the full public key, 64 hex characters) that musicians paste to each other to
address a hand-off back.

Pointing it at the shared vault changes its master public key. Every contact
token already shared stops resolving, and every hand-off envelope already
signed stops tracing back to its signer. Hocket's own source says this, about
the Strophe rename that forced the same question: a musician who already has an
identity would silently become a new person with a new fingerprint.

So Hocket needs the migration Strophe got — unseal under the old root, re-seal
under the new — plus an answer for tokens already in the wild, which is a
product decision, not a mechanical one. Left undone on purpose.

## Where the picker gets drawn

**Choosing is restart-shaped everywhere, not just in Woodshed.** Turnstone
loads `RootIdentity` once at boot and every denizen grant descends from it;
Knot opens `StartupUnlockedPersonalVault` when the endpoint opens; Woodshed
derives its sealing key at `open_store()`. So the honest v1 in every
application is the same: the choice is remembered and takes effect next launch.
`genet_host_api::settings` already has the word for that,
`SettingMutability::RestartRequired`, which makes Woodshed's settings row the
*easy* case rather than the awkward one — a `SettingControl::Choice` over the
roster whose `apply` calls `remember_profile`.

**Graphshell is not just a consumer; it is a prior implementation.**
`ports/graphshell/src/native/personae_host.rs::snapshot` already builds
`ProfileView { selected, id, display_name, slot_count,
master_public_fingerprint }` — the same list-and-mark-and-sort-by-id that
`read_roster` does, projected over the protocol as a secret-free read model.
The two should be reconciled rather than left to drift: `ProfileView` becomes
the wire form of a `RosterEntry` plus the fingerprint.

Graphshell also has **no switch intent** — `SSH_GENERATE`, `SSH_IMPORT_NATIVE`,
`SSH_REMOVE`, `DEVICE_REVOKE`, three signing-approval intents, and nothing for
the profile. It can show which persona you are on and cannot change it. That is
precisely the gap the roster closes, which makes graphshell the first surface
to draw rather than the last.

Suggested order:

1. **Graphshell** — reconcile `ProfileView` with `RosterEntry`, add the switch
   intent over `remember_profile`. The read model is already there.
2. **Turnstone** — `identity.rs` still hardcodes `ProfileId("default")`; it
   moves to `roster::open_chosen`. This is the only remaining *production*
   hardcode. (The `"default"` occurrences in `browser_host.rs` and `pairing.rs`
   are test fixtures constructing a `Profile` directly, which is correct.)
3. **Knot** — startup-bound the same way.
4. **Woodshed** — the settings row above.
5. **Hocket** — gated. Its picker *is* the rotation surface, so it cannot be
   drawn before the migration is decided.

## Open

- **Creating a persona.** `PickerEvent::CreateRequested` reports the intent;
  naming it is the application's flow. No application has one.
