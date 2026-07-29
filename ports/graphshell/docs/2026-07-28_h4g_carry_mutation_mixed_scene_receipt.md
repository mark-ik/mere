# H4g carry mutation and mixed-scene receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** first carry mutation and the mixed-scene reopen condition passed; H4 remains in progress

## Product cut

Graphshell now exposes delegated-device revocation as a typed native intent.
An active device card carries the device UUID and an initially unconfirmed
request. The native authority requires explicit confirmation, calls
`session-runtime::revoke_remote_auth_device`, and returns only public outcome
facts. The live carry roster, grants, and epoch rotation remain authoritative.
A revoked device loses the action, and its device and grant cards carry a
`revoked` badge.

The identity endpoint marks its already secret-free public read model as
exportable. `GraphshellApp::pin_portable_card` accepts only an exportable
mounted projection and copies one user-selected portable card into Mere. The
pinned facet records:

- the source reference;
- the observed session, epoch, and revision;
- the fixed authority state `source-owned`;
- the public portable card.

Advertised actions and intent payloads are absent. They remain valid only in
the live admitted session. The source reference hash supplies a stable local
projection address.

## Mixed-scene proof

The H4g test built an isolated live carry authority, issued a signed remote
device grant, and used a real Personae SSH-agent request to append signing
history. It then:

1. rejected the projected device's unconfirmed revocation intent;
2. accepted the confirmed intent through `IdentityEndpoint`;
3. read the live `session-runtime` roster and found the device revoked;
4. mounted the refreshed exportable identity projection;
5. pinned the profile, device, grant, and signing-history cards;
6. selected those cards with an existing access-history node;
7. saved, persisted, and reopened the Mere scene.

The reopened scene retained the complete selection and access history. Every
identity facet remained `source-owned`, used the `personae.public` adapter, and
contained neither advertised actions nor a private-key marker.

## Verification

These commands passed against the live checkout:

```powershell
cargo test -p graphshell --all-features --offline h4_exportable_identity_cards_and_access_survive_scene_reopen_as_projections -- --nocapture
cargo test -p graphshell --all-features --offline --lib -- --test-threads=1
cargo check -p graphshell --all-targets --all-features --offline
cargo check -p graphshell --target wasm32-unknown-unknown --no-default-features --features web --offline
python scripts/check_port_boundaries.py
cargo clippy -p graphshell --lib --all-features --offline --no-deps -- -D warnings -A clippy::too-many-arguments
```

The full library result is 72 passed, 0 failed. Native all-target and
browser-safe Wasm checks passed, as did the port-boundary script. Focused
library Clippy is clean while allowing the already present
`admit_browser_session` argument-count lint. Warning-denying all-target Clippy
still reaches the earlier `g5_peer` lock-across-await lint.

The checks are offline and use the live checkout's patch configuration. This
receipt does not claim a tracked clean-checkout lockfile.

## Remaining H4 boundary

- Repeat the standard-pipe restart with `<private-address>:22` reachable and prove a
  real public-key login.
- Install the Graphshell logon task, sign out or reboot, prove startup and crash
  recovery in that session, then retire the interim Personae task.
- Complete headed Firefox and other desktop-dialog receipts.
- Add enrollment, grant issuance, expiry, and delegation management as typed
  carry-authority intents where the product requires them.
