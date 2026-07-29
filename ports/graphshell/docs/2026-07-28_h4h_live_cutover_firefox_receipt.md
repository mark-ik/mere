# H4h live cutover and Firefox carrier receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** installed-task cutover, rollback, crash replacement, and headed Firefox admission passed; H4 remains in progress

## Live Windows cutover

The Windows installer now accepts an optional carry data root, builds the
resident-host command with correct `wscript.exe` quoting, disables the retained
Personae task during a successful cutover, and restores a working authority
when an install or update fails.

The first real install exposed an invalid quote sequence in the generated VBS
launcher. The cutover failed before Graphshell could claim the standard agent,
and the existing `personae-agent` task resumed with the same key. The installer
was then hardened so a failed first install unregisters its new task, while a
failed update disables the broken Graphshell task and restores the retained
Personae task when it is the available fallback.

With corrected quoting, the real cutover:

1. staged `graphshell-device-host.exe` and `graphshell-native-host.exe` under
   `%LOCALAPPDATA%\Graphshell\bin`;
2. registered and started the `graphshell-device-host` logon task;
3. disabled and stopped `personae-agent`;
4. listed
   `SHA256:d3tQOqvSRA4QE1D7R1j2SJh31wLCXTTZofJxvQLfd0o` through the standard
   OpenSSH endpoint;
5. killed the first resident-host process;
6. observed the recovery loop launch a replacement that reclaimed the standard
   endpoint and listed the same fingerprint.

An intentional update with the wrong expected fingerprint exercised the
failure path. Graphshell stopped and became disabled, `personae-agent` was
re-enabled and started, and the same fingerprint remained available. A final
correct update returned the system to:

- `graphshell-device-host`: `Running`;
- `personae-agent`: `Disabled`, retained for rollback;
- stock OpenSSH endpoint: served by Graphshell;
- listed key: the expected fingerprint above.

The known laptop at `<private-address>:22` remained unreachable. The installer did
not retire `personae-agent`; retirement still requires a real sign-out or
reboot receipt and a real remote public-key login.

No live `session-runtime` carry root was found for this profile, so the task was
installed without `--data-root`. The resident host reports that boundary
through the public Device access card instead of presenting carry mutations
that cannot reach an authority.

## Headed Firefox receipt

Firefox 153 loaded the prepared temporary Graphshell extension from
`manifest.json`. The extension opened its real `moz-extension:` bridge and
displayed:

- `Graphshell · native Personae authority`;
- `Admitted · 10 public cards`;
- an OS-protected, unlocked Identity vault;
- a Device access card with recovery unconfigured, no Personae carry entries,
  and the missing carry-data-root reason;
- the default public profile.

The page contained the live **Close device session** control. After Firefox
closed, the resident log recorded:

```text
browser device session ended answered=13 end=Closed
```

This proves the Firefox launcher identity, native-messaging relay, nonce
exchange, transcript-bound admission, public projection, and clean carrier
close against the installed resident authority. The receipt did not invoke
key removal, device revocation, signing, or import against the user's vault.

The browser bridge now renders the existing SSH removal and delegated-device
revocation intents. It reconstructs each request from public card fields,
requires a browser confirmation, and sends `confirmed: true` to the native
authority. Native tests remain the mutation proof; the live Firefox receipt
deliberately leaves the user's key untouched.

## Verification

The final checkout passed:

```powershell
cargo fmt -p graphshell -- --check
cargo test -p graphshell --all-features --offline --lib -- --test-threads=1
cargo check -p graphshell --all-targets --all-features --offline
cargo check -p graphshell --target wasm32-unknown-unknown --no-default-features --features web --offline
python scripts/check_port_boundaries.py
cargo clippy -p graphshell --lib --all-features --offline --no-deps -- -D warnings -A clippy::too-many-arguments
node --check ports/graphshell/web/extension/bridge.js
node --check ports/graphshell/web/extension/smoke-native-host.mjs
```

The PowerShell installer also passed an AST parse. The Firefox form of the
native-host smoke script completed through the installed relay before the
headed run.

## Remaining H4 boundary

- Sign out or reboot, prove launch-at-login and crash recovery in that new
  session, then retire the retained Personae task.
- Bring the known laptop online and prove a real SSH public-key login through
  Graphshell.
- Prove native key import in Firefox and the macOS and Linux dialog providers.
- Configure a real carry authority before adding enrollment, grant issuance,
  expiry, delegation, and recovery management. Enrollment should be a native
  pairing flow with secret-bearing ticket or short authentication string
  handling, not a static portable-card intent.
