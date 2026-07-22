# Auto-Update Brief

**2026-07-22.** Cross-cutting research: a configurable auto-update capability
for every deployment surface in the family (desktop apps, radio firmware,
web/wasm, eventually mobile). Requirement from Mark: this is figured out
before any load-bearing deployment; it is a utility in its own right, wanted
alongside the identity vault
([plan](mere_docs/implementation_strategy/2026-07-22_identity-vault-ssh-agent_plan.md)).

## Requirements

1. **Configurable, never a checkbox default.** Update policy is a real
   setting per
   [`feedback_configurability_over_opinionated_defaults`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md):
   off / check-and-notify / download-then-ask / fully automatic; channel
   selection (stable/beta/dev); check cadence; metered-connection behavior.
2. **Honest status.** Real states surfaced to the user (idle, checking,
   available, downloading, staged, restart-pending, applied, failed with
   reason), never a placebo spinner
   ([`feedback_real_sync_feedback`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_real_sync_feedback.md)).
3. **Signed.** Every artifact verified against a release key before apply.
   Release signing keys are company/offline keys, deliberately separate from
   personae user identity.
4. **Safe apply.** Power-fail-safe where the platform allows it; rollback or
   at minimum a working previous version on failure.
5. **Respect the install channel.** A distro-packaged or store-installed
   build must detect that and disable self-update (the package manager owns
   it).

## Prior art by surface

### Desktop (hocket, merecat, isometry, woodshed, mere, strophe)

- **Velopack** (MIT, active as of July 2026): installer + auto-update
  framework, Win/macOS/Linux, delta packages, background apply. Core is
  Rust with a first-class [`velopack` crate](https://crates.io/crates/velopack).
  Successor lineage of Squirrel.Windows/Clowd.Squirrel. Opinionated: its
  installer owns the install layout (per-user dir on Windows, no UAC).
- **axoupdater** (MIT/Apache): standalone updater or library, consumes
  GitHub Releases (cargo-dist receipt format). Still maintained (0.10.x,
  2026); axo the company wound down and astral maintains a cargo-dist fork
  for uv/ruff, so treat the pipeline as community-carried.
- **self_update** crate: simple in-process GitHub-Releases binary
  replacement. Smallest possible mechanism, no deltas, no installer story.
- **tauri-plugin-updater**: minisign signatures + update manifest JSON;
  Tauri-coupled, but its manifest + minisign shape is worth borrowing.
- **Sparkle** (macOS, ObjC): the mac gold standard; appcast feed, EdDSA
  signatures, delta updates. Design reference, not a dependency.
- **Omaha / Keystone** (Chrome): background service, silent updates, staged
  percentage rollouts, differential compression (courgette/zucchini).
  Design reference for channels + rollout, far beyond our v1 needs.
- **TUF** (The Update Framework): the threat-model framework (rollback,
  freeze, mix-and-match, key-compromise recovery).
  [`tough`](https://crates.io/crates/tough) (AWS, powers Bottlerocket) is the
  maintained Rust client. Adopt the threat model vocabulary now, the full
  framework only if/when we self-host a repository.

Platform mechanics to design around: Windows cannot replace a running exe
(rename dance, apply-on-restart); macOS is an .app bundle swap with
codesign/notarization and translocation to respect; Linux raw-binary swap is
easy but coexists with Flatpak/AppImage/distro channels that own their own
update stories (AppImage has AppImageUpdate/zsync).

### Embedded / radio firmware (Merely hardware: T114 nRF52840, V4 boards; Tulle/Tucket/Sennet)

- Meshtastic practice today: nRF52 updates via UF2 drag-and-drop onto the
  Adafruit bootloader's USB drive (double-tap reset), or BLE DFU from the
  phone app (documented as riskier); ESP32 via WebSerial web flasher or
  esptool, with no app OTA path.
- **embassy-boot**: Rust bootloader, A/B active/DFU partitions,
  power-fail-safe swap, trial boot + rollback, optional ed25519 signature
  verification (dalek or salty), nRF flavor exists. The pick if/when we own
  firmware images. **MCUboot** is the C incumbent (Zephyr/nRF Connect).
- FCC/region-lock constraint from the firmware plan: shipped devices default
  to signed, region-locked images, and the update channel must be
  region-aware; the GPLv3 firmware + bootstrap-console posture means the
  user-flashable path must remain open. Auto-update here means "the app
  offers and stages firmware for a connected radio," never silent flash.

### Web / wasm (genet browser targets, isometry-web)

Deploy is the update. The work is PWA service-worker lifecycle (update
detection, skipWaiting/clients.claim) plus hashed-asset cache busting.
Configurability surface: prompt-to-reload vs auto-reload.

### Mobile (future, note only)

Store-mediated. Play has an in-app updates API; iOS has none. Nothing to
build until a mobile target exists.

## Recommendation

Two layers, so "configurable" is shared and mechanisms are per-surface:

1. **A small shared policy layer** (family crate, name TBD): the policy
   enum, channel, cadence, metered handling, and the honest status state
   machine from Requirements 1-2, with a `Transport` seam per surface.
   This is the part every app and the firmware-offering UI share.
2. **Per-surface transports behind it.** Desktop v1 candidates, in
   preference order to probe:
   - **Velopack via its Rust crate** (full story: installer + deltas +
     staged apply on all three hosts), accepting its install-layout
     opinions; or
   - **GitHub Releases + minisign-style ed25519 + our own staging/swap**
     (self_update-shaped, smallest surface, no installer), if Velopack's
     opinions fight the family's needs.
   Firmware transport is embassy-boot-shaped and lives with the firmware
   plan, behind the same policy layer.

**Concrete next step (the pressure test):** package hocket with Velopack on
all three hosts (all three are available, including SSH to the Linux
laptop) and run a real update cycle v0.1 to v0.2. Done condition: an
installed hocket updates itself through a signed release with the policy
setting honored and honest status shown. What that probe teaches decides
Velopack-vs-composed before anything load-bearing ships.

Signing for v1 is detached ed25519 (minisign format) over release artifacts,
key held offline, verify before apply. TUF/tough enters when releases move
off GitHub onto Merely-hosted (or P2P) distribution. P2P distribution of
update artifacts over iroh/retinue (content-addressed, blake3, fits eidetic)
is noted as a fit and deferred.

## Non-goals now

Omaha-class update servers, staged percentage rollouts, mobile, and P2P
distribution. Recorded so they are chosen later, not drifted into.

## Sources

- [Velopack repo](https://github.com/velopack/velopack), [velopack crate](https://crates.io/crates/velopack), [Rust getting started](https://docs.velopack.io/getting-started/rust)
- [axoupdater](https://github.com/axodotdev/axoupdater), [astral cargo-dist fork discussion](https://github.com/posit-dev/air/issues/297), [uv self update](https://github.com/astral-sh/uv/pull/2228)
- [tough (TUF client)](https://crates.io/crates/tough), [rust-tuf](https://github.com/theupdateframework/rust-tuf)
- [embassy-boot](https://docs.embassy.dev/embassy-boot), [embassy-boot-nrf](https://crates.io/crates/embassy-boot-nrf), [Drogue firmware-updates writeup](https://blog.drogue.io/firmware-updates-part-1/)
- [Meshtastic nRF52 OTA](https://meshtastic.org/docs/getting-started/flashing-firmware/nrf52/ota/), [UF2 drag-and-drop](https://meshtastic.org/docs/getting-started/flashing-firmware/nrf52/drag-n-drop/)
