# Dependency + footprint brief — audit baseline

**Date**: 2026-07-04. Cross-repo (mere, serval, netrender, netfetcher, errand,
wgpu-scry, wgpu-graft, wgpu-weld).
**Why**: bloat is a recurring vague fear; this brief converts it into tracked
numbers and recorded gate rationales, on the model that killed held-RDF-truth
(measure, then decide). Re-run the cheap numbers roughly twice a month; drift
becomes a diff instead of a feeling.

## Method

Extracted every crates.io-versioned dependency declaration across the eight
repos (656 declarations, 406 unique crates), queried crates.io for the latest
stable of each, and classified lags: semver-compatible (cargo floats them on
fresh builds; serval + mere gitignore their locks) vs semver-major (a real
migration). Git/path deps (stylo tag, taffy vendor, mark-ik forks) checked by
hand where load-bearing.

## Results (2026-07-03 audit)

- **333 minor/patch lags**: float automatically; not staleness.
- **158 semver-major lags**, but the two biggest are deliberate waits (below),
  a dozen are false positives (multi-line parse artifacts, package renames),
  and ~20 were dead declarations (now deleted, below).
- **Current on the crates that matter most**: vello 0.9, winit 0.30.13,
  accesskit 0.24, p2panda 0.6.1, burn 0.21 were all latest stable at audit time.

## Gate rationales (do not re-derive)

- **iroh 0.98, not 1.0 — gated on p2panda.** p2panda-net 0.6.1 (latest release,
  2026-05-22) requires `iroh ^0.98`; murm shares one endpoint across its
  transports, so a split-version iroh is not viable. p2panda **main** already
  bumped to iroh 1.0 / iroh-gossip 0.101 (commits through 2026-06-30), so the
  unblock is the next p2panda release. Bump iroh + iroh-blobs + iroh-tickets +
  p2panda-* as one move. Do not switch p2panda to a git dep to get 1.0 early.
- **wgpu 29, not 30 — gated on vello + a settling period.** wgpu 30.0.0
  shipped 2026-07-01; vello 0.9 requires `^29.0.3` and vello main has not
  bumped. The expensive part is our own lockstep (scrying/weld sit on wgpu-hal,
  which churns hardest in majors; netrender/serval/graft move in the same
  breath), so waiting for vello + a 30.0.1 is deliberate. Fork trigger: vello
  still on 29 after ~a month AND something in wgpu 30 actually pulls us.
- **taffy — gated on stylo_taffy.** serval vendors
  `taffy 0.11.0-experimental-cache-fix.3` with 3 documented re-applyable
  patches. taffy 0.12.1 stable now ships `float_layout`/`block_layout` (the
  experimental line graduated), but stylo_taffy 0.3.0-alpha.6 still requires
  the exact experimental pre-release. Re-vendor when stylo_taffy releases
  against stable 0.12, or fork stylo_taffy (small adapter crate, cheapest fork
  on the list).

## Actionable queue (ungated, in order)

1. **stylo v0.18 → v0.19** (tag is out; monthly cadence; may pull a fresh
   stylo_taffy that resolves the taffy gate).
2. **Text stack lockstep**: parley 0.11 + fontique 0.11 + **skrifa 0.43** +
   **read-fonts 0.40** (not 0.44/0.41 — parley 0.11 caps them) across
   serval/netrender/mere.
3. **wasmtime 45 → 46** (mere; rebuild wasip2 test guests, read RELEASES.md).
4. **mere RustCrypto generation** (~7 use-sites: hkdf/chacha20poly1305/sha2 in
   murm): 0.10 → 0.11 era as one family (generic-array → hybrid-array trait
   change).
5. **mere singles**: thiserror 1→2, redb 2→4, fjall 2→3, reqwest 0.12→0.13,
   ureq 2→3, toml 0.8→1, rcgen, tokenizers 0.20→0.23, safetensors 0.4→0.8,
   jsonschema 0.18→0.46 (largest lag in the tree). errand: quick-xml 0.39→0.41.

## De-bloat done this pass

Deleted 20 dead crypto workspace declarations from serval's root `Cargo.toml`
(aes, aes-gcm, aes-kw, cbc, chacha20poly1305, cipher, ctr, der, digest, ecdsa,
elliptic-curve, hkdf, ml-dsa, ml-kem, num-bigint-dig, p256, pkcs8, sec1, sha1,
sha3) — servo-heritage entries with zero in-workspace consumers (the WebCrypto
component they served was cut). sha2 kept (serval-scripted + pelt-desktop use
it for subresource integrity). `cargo metadata` validates.

## Migrations done this pass

**icu 1.5 → 2.x, serval.** `icu_locid` renamed to `icu_locale_core` upstream;
bumped `components/fonts` and `components/malloc_size_of` (the two direct
1.5-line consumers) plus the dead `icu_segmenter` workspace pin, all to 2.2.0.
Code changes: `icu_locid::subtags::{Language, language}` →
`icu_locale_core::subtags::{Language, language}` (module path only, same
macro/type); `Language::UND` → `Language::UNKNOWN` and
`.is_empty()` → `.is_unknown()` (icu_locale_core 2.x dropped `Default`/
`is_empty()` from the tinystr-subtag codegen); `icu_properties::maps::
general_category()` → `icu_properties::CodePointMapData::<GeneralCategory>::
new()` (the `maps` module was replaced by `CodePointMapData` in icu_properties
2.x). `cargo check -p servo-fonts -p servo-malloc-size-of --tests` green.
Confirmed in the regenerated `Cargo.lock`: our own crates now resolve
icu_locale_core/icu_properties 2.2.0 unified with nova's temporal_rs-pulled
icu_calendar 2.2.1 — the only `icu_locid 1.5.0` left is stylo's own external
pin (out of our control, semver-compatible coexistence).

## Footprint baseline (2026-07-04)

| Number | Value |
| --- | --- |
| Unique crates.io deps, 8 repos | 406 |
| mere workspace crates | 68 |
| serval workspace crates | 75 |
| netrender / netfetcher / errand | 5 / 1 / 1 |
| wgpu-scry / graft / weld | 7 / 12 / 3 |
| meerkat.exe (debug) | 216 MiB |

Not yet measured (worth adding next pass): release binary size, cold-build
wall time per repo, target-dir footprint. Watch direction, not absolutes; the
debug binary number is only useful as its own trend line.

## Related

Burn-remote-over-iroh prior art recorded in the
[mesh_lease_scheduler_plan](mere_docs/implementation_strategy/2026-06-30_mesh_lease_scheduler_plan.md)
(same audit session). The measure-then-decide model this brief follows:
[petgraph_rdf_plan](mere_docs/implementation_strategy/2026-06-18_petgraph_rdf_plan.md)
(held-RDF-truth rejected on a benchmark, Phase 4 evidence-gated).
