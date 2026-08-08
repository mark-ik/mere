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

**Ruled 2026-08-08 (Mark): switching is live. A forced restart is the
exception, reserved for necessities, never the design.** An earlier draft of
this section called restart-shaped switching "the honest v1"; it is retracted.
What live means per application:

- **Graphshell** — free. The resident host holds `Arc<Mutex<IdentityVault>>`
  and the SSH agent shares the same handle, so `IdentityVault::switch_profile`
  (added for this) takes effect on the next operation of everything reading
  the vault. Nothing anywhere is told to die.
- **Woodshed** — reopen the store and reload: save the old persona's session
  sealed under its key, `open_store()` again (it reads the new choice), load
  whatever the new persona had. "Switching personas switches practice
  sessions" is the tested property; live switch is just doing it without the
  relaunch in the middle.
- **Turnstone** — the heavy one, and the machinery exists: `denizen::rebuild`'s
  re-root heal already handles "the root changed" by re-issuing grants from
  the reviewed projections. A live switch is an invocation of the heal, not
  new infrastructure.
- **Knot** — close the endpoint, open the new persona's. K2 built exactly the
  contract this needs: sessions on a vanished endpoint are told
  (`Disconnected`), never shown a stale copy.

`SettingMutability::Live` is the marker for Woodshed's settings row, with the
row's `apply` doing the actual swap, not just remembering it.

**Graphshell is not just a consumer; it is a prior implementation.**
`ports/graphshell/src/native/personae_host.rs::snapshot` already builds
`ProfileView { selected, id, display_name, slot_count,
master_public_fingerprint }` — the same list-and-mark-and-sort-by-id that
`read_roster` does, projected over the protocol as a secret-free read model.
One caution from reconciling them: their `selected` fields answer different
questions. `read_roster`'s chosen is the *resolved choice* (env, remembered
file, sole persona); `snapshot`'s selected is the *loaded current profile*.
After a switch they coincide, but under a `GRAPHSHELL_PROFILE` override they
must not — the UI shows what is, not what would be resolved. So the two stay
separate constructions, on purpose.

Graphshell also had **no switch intent** — `SSH_GENERATE`, `SSH_IMPORT_NATIVE`,
`SSH_REMOVE`, `DEVICE_REVOKE`, three signing-approval intents, and nothing for
the profile. It could show which persona you are on and could not change it.

### Landed 2026-08-08 (graphshell)

- `IdentityVault::switch_profile` (personae) — the vault-level switching the
  `ProfileId` docs had promised. Loads before replacing, so a failed switch
  changes nothing.
- `graphshell::profile` joins the family ladder: explicit `--profile`, then
  `GRAPHSHELL_PROFILE` (the application's own override, above the family one),
  then `roster::resolve_profile`. The device host resolves after opening
  storage — resolution without looking at the vault is how five hardcoded
  `"default"`s happened. Pair/unpair subcommands now resolve too, which fixes
  a real bug: they operated on `default`'s pairing settings regardless of the
  family choice.
- `PROFILE_SWITCH_INTENT` + `SwitchProfileIntentV1`: every non-selected
  profile card carries "Speak as this persona", payload bound to the profile
  id off the drawn card (identity, not row position). The host applies it
  live and then remembers the choice beside the vault for the rest of the
  family; the receipt says whether remembering happened, because "everyone
  follows" and "just here" are different promises.

Remaining order: **Turnstone** (`identity.rs` still hardcodes
`ProfileId("default")` — the only remaining production hardcode; the
`"default"`s in `browser_host.rs` and `pairing.rs` are test fixtures and
correct), then **Knot**, then **Woodshed** (the settings row), then **Hocket**
— gated: its picker *is* the rotation surface, so it cannot be drawn before
the contact-token migration is decided. Its persona-faceted timeline concept
is sketched in
[hocket's design docs](../../../../hocket/design_docs/2026-08-08_persona_timeline_design.md).

## Open

- **Creating a persona.** `PickerEvent::CreateRequested` reports the intent;
  naming it is the application's flow. No application has one.
