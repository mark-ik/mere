# luggage

Self-update for packaged apps: signed release manifests over pluggable feeds.
The runtime half of the family's update pipeline; packing uses upstream's
[`cargo-packager`](https://github.com/crabnebula-dev/cargo-packager) CLI
unchanged, so the whole pipeline is Rust with no .NET anywhere.

Forked 2026-07-24 from
[`cargo-packager-updater` 0.2.3](https://crates.io/crates/cargo-packager-updater)
(Tauri Programme within The Commons Conservancy / CrabNebula Ltd,
MIT OR Apache-2.0). Upstream copyright headers are preserved on derived
files. Named and homed per Mark's ruling: **luggage**, in the mere repo.

## Divergences from upstream

- **Feeds, not just endpoints.** [`Feed`] is HTTP(S) (upstream semantics,
  `{{target}}`/`{{arch}}`/`{{current_version}}` templating intact), a local
  **directory** holding `luggage.json` (LAN shares, mounted drives,
  acceptance tests — no server), or a **GitHub repo** (`github:owner/repo`,
  resolving to the latest release's `luggage.json` asset).
- **BLAKE3 in the manifest.** Each platform entry may carry a `blake3` hex
  digest of its artifact, verified before the minisign signature. This is
  the content-addressing seam for the planned P2P distribution lane
  (iroh-blobs chunk dedup as implicit delta), carried from day one so
  manifests never need a format break.
- **rustls** instead of system TLS.
- Split into modules per mere's file-size policy.

Everything else — the manifest JSON shape, minisign verification, the
per-platform install mechanics (NSIS/MSI, `.app.tar.gz`, AppImage) — is
upstream's, deliberately. Planned divergences (staged-swap apply mechanics,
the P2P lane) are tracked in
[hocket's auto-update plan](https://github.com/merely-made/hocket/blob/main/design_docs/2026-07-24_auto-update_plan.md)
and mere's auto-update brief.

## Security: two signatures, and why both

Every release carries two minisign signatures, and luggage checks both:

1. **The artifact signature** (in the manifest, per platform) proves the
   downloaded bytes are ours. A BLAKE3 digest is checked first when present,
   so a corrupt download fails with a clearer error than a crypto failure.
2. **The manifest signature** (`luggage.json.sig`, detached) proves the
   *version and URL announced around* those bytes are ours.

The second exists because the first cannot cover it. Demonstrated
2026-07-24 before it was fixed: a manifest claiming version 0.3.0 while
pointing at a genuinely-signed 0.2.0 artifact was accepted, since digest and
signature both checked out against the bytes actually served. Whoever
controls the feed could therefore never ship arbitrary code, but *could*
advertise an old signed build as new and roll a client **backwards** onto a
release with a known flaw.

[`Config::require_signed_manifest`] defaults to `true`, so an unsigned feed
is refused with a message naming the fix. Opting out is possible and has to
be written down in the caller's code, which is the point.

Sign the manifest **last**, after every host has added its platform entry —
any later edit invalidates it:

```sh
cargo packager signer sign <feed>/luggage.json
```

Both `.sig` conventions are accepted: the raw minisign text block that
`minisign -S` writes, and the base64-wrapped form `cargo packager signer
sign` writes.

## Staging

`Update::stage` writes a verified artifact into an app-owned directory, so
"downloaded, ready to restart" survives the app closing rather than being a
claim about bytes held in memory. `StagedUpdate::take_verified` re-hashes
the file at apply time, closing the gap between staging and applying;
staging requires a manifest digest for exactly that reason.

## Manifest

`luggage.json`, same shape upstream documents, plus the optional `blake3`:

```json
{
  "version": "0.2.0",
  "pub_date": "2026-07-24T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "url": "https://github.com/you/app/releases/download/v0.2.0/app-setup.exe",
      "signature": "<contents of the .sig>",
      "blake3": "<hex digest of the artifact>",
      "format": "nsis"
    }
  }
}
```

In a directory feed, artifact `url`s are absolute `file://` URLs (the pack
script generates them on-host); relative names are a noted follow-on.
