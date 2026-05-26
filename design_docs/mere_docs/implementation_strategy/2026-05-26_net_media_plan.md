# net-media Plan — a portable media organ for the Mere ecosystem

**2026-05-26.** Plan for **`net-media`**: the Mere ecosystem's **media organ** —
WebRTC (data channels + media tracks) and audio/video **decode** (everything we
can get *shy of being blessed by the censors* — i.e. no DRM/EME). Sibling to
[`netfetcher`](2026-05-25_netfetcher_plan.md) (network), `netrender`
(paint→GPU), and serval (render). **Mere owns it; serval consumes decoded
frames** through a byte/frame seam, the same way it consumes fetched bytes.

> Status: **plan-only.** No `net-media` crate exists yet. Bucket #3 of the
> networking/web-API/media triage (bucket #1 netfetcher ✓ implemented, bucket #2
> web-API shared-middle planned). This fixes scope, the decode-tier policy (the
> load-bearing build-environment decision), the codec landscape, layering, the
> increment ladder, and open questions.

## 1. Two halves, one organ

net-media is two related-but-distinct capabilities:

1. **WebRTC** — via [`webrtc-rs`](https://github.com/webrtc-rs/webrtc) (pure
   Rust). The **data-channel** half is *core to Mere's p2p / smolweb thesis*
   (peer-to-peer protocol experiences) — arguably more load-bearing than media.
   The **media-track** half (audio/video calls) rides the same stack.
2. **Media decode** — `<video>`/`<audio>` and WebRTC media need codecs. This is
   the "deep but not bottomless" half: a codec-registry + decoders, with a
   deliberate **decode-tier policy** (§3) that keeps the default build clean.

**Out of scope (deliberate):** DRM / EME / Widevine — the "shy of the censors"
line. net-media decodes open and royalty-free formats; protected streams are not
a goal. Also out: the *fetch* of media bytes (that's netfetcher) — net-media
takes bytes/streams and produces frames.

## 2. The gap + the landscape (researched 2026-05; re-verify at build time)

Rust's pure-media-decode story is **uneven but no longer bleak** — the picture
moved a lot in early 2026:

- **AV1 is the royalty-free spine, and it's the mature bet.** `rav1d` (ISRG/
  Memorysafety's Rust port of dav1d — decode, ~5% off dav1d, safety-motivated)
  and `rav1e` (encode; `wav1c` is a newer safe-Rust encoder). These are the
  battle-tested pure-Rust(+asm) codecs.
- **H.264/H.265 pure-Rust is now emerging but young** — `rust_h264` (0.4),
  `rust_h265` (0.1), `hibernia` (clean-room H.264), the sprawling **OxideAV**
  framework (≈95 crates, much still scaffold). **Watch, don't depend** — track
  for when web-compat H.26x matters; use C bindings (`openh264`/ffmpeg) only if
  forced, and prefer not to.
- **AV2** (`rav2d`) is on the horizon — a "watch" line so net-media isn't born a
  generation behind.
- **OxideAV's `core`** (`Packet`/`Frame`/`Decoder`/`Encoder`/`Demuxer` traits +
  `CodecRegistry`) is a ready **reference for our own codec-registry shape** —
  single-vendor and young, so reference-not-dependency.

The structurally-easy wins (genuinely pure-Rust, mature):

- **Demux/containers:** `mp4parse` (Mozilla; ships in Firefox), the
  `matroska`/`mp4` crates, and Symphonia's demuxers. Safe parsing of untrusted
  container bytes — the cheap, high-value first slice.
- **Audio decode:** **Symphonia** (MP3/AAC/FLAC/Vorbis/ALAC/…) — the audio side
  is *solved* in pure Rust. (Note: **`woodshed-audio` is not reusable here** — it's
  a musician's-tool audio-infra crate, not a codec stack; per Mark's own read.
  The relevant crates are Symphonia + `web-audio-api`-rs for the Web Audio graph.)

## 3. Decode-tier policy — the load-bearing decision

The rav1d/rav1e reality (established in the 2026-05 conversation): the fast
codec path is **Rust logic + hand-written NASM/GAS SIMD kernels**, and that asm
is *permanent* (it's a hand-asm-vs-compiler gap, identical in C — `std::simd` is
nightly, intrinsics don't match hand-tuned asm). So pulling in fast software
decode means pulling in **NASM** — the exact build-environment baggage serval
shed (vanilla-Windows builds, no NASM/MOZILLABUILD/clang-cl).

**Resolution — a three-tier decode policy, asm isolated and opt-in:**

1. **Hardware decode first** (`cros-codecs` for VA-API/V4L2; Vulkan Video via
   ash/wgpu interop). The real-time fast path, **zero assembler**, GPU does the
   work. Default fast lane; keeps the build clean.
2. **Pure-Rust no-asm software** (`rav1d` with `--no-default-features`, the
   ported-C-reference functions). **Safe, portable, vanilla-build, slow** — the
   memory-safe fallback for untrusted bitstreams when no GPU decode is available.
   (rav1d frames its no-asm path as a correctness oracle, not a perf path — so
   this tier is "works, not fast.")
3. **asm-accelerated software** — *opt-in only*, behind a feature the media crate
   owns in isolation. The **one** place NASM enters the build, and only if you
   ask for fast CPU decode.

**The rule: the default `cargo build` pulls no assembler (tiers 1+2); asm is a
conscious, isolated opt-in.** This is permanent architecture, not a stopgap —
there's no "wait for pure Rust to catch up" (the asm gap doesn't close). Bake it
in now; it's far cheaper than retrofitting.

## 4. Codec registry (our own, OxideAV-shaped)

```text
net-media
├── codec-api        # neutral Packet / Frame / Decoder / Encoder / Demuxer traits
│                    #   + CodecRegistry (indexed by codec id); NO codec deps
├── codec-av1        # rav1d (decode) + rav1e/wav1c (encode), tier-gated (§3)
├── codec-audio      # Symphonia adapters (mp3/aac/flac/…)
├── demux            # mp4parse / matroska / ogg adapters
├── hwdecode         # cros-codecs / Vulkan-Video adapter (tier 1)
└── webrtc           # webrtc-rs: data channels (core) + media tracks
```

The registry mirrors the **engine-neutral organ pattern** the ecosystem already
uses (netfetcher's seams, netrender's `PaintList`, serval's `LayoutDom`): neutral
trait contract + per-impl crates + feature/tier gating. A codec is selected by
content kind + a tier policy (hw → pure-rust → asm), exactly parallel to
netfetcher's transport selection.

## 5. Layering — who consumes it

- **Mere owns net-media**; it runs decode (likely in a service worker akin to
  netfetcher's `FetcherPool`, off the UI thread).
- **serval consumes decoded frames** through a seam, the same shape as its
  `ImageLoader` (host fetches+decodes; serval lays out + composites). A
  `VideoFrameSink`/`MediaSource`-style seam: serval requests a media resource;
  the host decodes via net-media and hands back frames (as GPU textures where
  hardware decode produced them — composited via netrender, zero-copy where
  possible, reusing the External-layer interop from the serval viewer work).
- **JS `<video>`/`<audio>`/`MediaSource`/WebRTC APIs** (serval's scripting tier)
  bind to net-media through the host — never linking it directly (same discipline
  as netfetcher).
- **WebRTC data channels** serve the **p2p/smolweb** lane directly (Mere's
  protocol experiences), distinct from the render path. (Mere's *transport*
  identity is iroh; webrtc-rs is for *browser-interop* p2p where a peer speaks
  WebRTC. The two coexist — flag for the murm/transport owners.)

## 6. Increment ladder

1. **Demux-safe + audio.** `mp4parse`/`matroska` container parsing + Symphonia
   audio decode + the `codec-api` registry skeleton. Fully pure-Rust, no asm, no
   GPU — the cheap, safe, high-value first slice. Oracle: parse + decode known
   fixtures; fuzz the demuxers (untrusted-bytes attack surface).
2. **AV1 software decode (no-asm tier) + registry.** `rav1d` `--no-default-
   features` behind `codec-av1`; wire into the registry; the frame seam to a
   consumer. Proves end-to-end decode on the safe tier.
3. **Hardware decode seam.** `cros-codecs`/Vulkan-Video adapter (tier 1); the
   GPU-texture frame path + netrender composite.
4. **WebRTC data channels.** `webrtc-rs` data channels — the p2p-thesis core.
5. **WebRTC media tracks + opt-in asm decode.** Media-track plumbing; the
   isolated asm-accelerated decode feature (tier 3).

## 7. Open questions

1. **rav1d build-env isolation** — confirm `--no-default-features` truly drops
   NASM (it should); design the asm feature so it never leaks into the default
   workspace build (a separate crate + explicit feature, like the boa fork's
   pattern).
2. **`webrtc-rs` maturity/version** — verify current release + how heavy its
   tree is; whether data-channels-only can be feature-gated from media tracks.
3. **Frame seam shape** — what serval's media seam looks like (CPU `Frame` vs
   GPU texture handle); reuse the External-layer / `wgpu` interop from the serval
   viewer + `wgpu-scry`/`grafting` work rather than inventing.
4. **Repo placement** — own repo `Code/repos/net-media/` (sibling to netfetcher),
   or a crate under an existing repo? Likely own repo (it's a substantial,
   independently-useful organ). `net*` naming sits alongside netfetcher/netrender.
5. **Shared vocabulary with netfetcher** — net-media takes *streams of bytes*
   (which netfetcher can supply); is there a shared `Stream`/`Bytes` seam, or are
   they just composed at the call site? Likely composed; don't over-unify.
6. **Native-only, like netfetcher** — codecs + hardware decode + webrtc-rs are
   native; the wasm/PWA path uses the browser's `<video>`/WebRTC. Same platform
   split as netfetcher (§3 there) and scripting.

## 8. Relationship to other plans

- **netfetcher** supplies the *bytes* (HTTP(S) media fetch, range requests for
  seeking); net-media decodes them. Composed, not coupled.
- **netrender** composites decoded frames (GPU textures) into the scene.
- **serval** consumes frames via a media seam (parallel to `ImageLoader`).
- **murm/transport** — WebRTC data channels are a *browser-interop* p2p path
  beside Mere's iroh transport; coordinate ownership.
- **Conformance** — WPT `media-source/`/`webcodecs/` deferred; manual playback +
  demuxer fuzzing first.

## Findings

- AV1 (rav1d/rav1e) is the only ship-today pure-Rust codec bet; H.26x pure-Rust
  is real but young (watch). The asm-vs-compiler gap is permanent → the decode-
  tier policy is steady-state architecture, not a stopgap.
- Audio + demux are genuinely solved in pure Rust (Symphonia, mp4parse) — the
  cheap first slice.
- The organ + registry shape mirrors the ecosystem's existing engine-neutral
  pattern; OxideAV-core is a reference for the trait surface.

## Progress

- **2026-05-26** — plan created. Scope (WebRTC + decode, no DRM), the three-tier
  asm-isolated decode policy, codec-registry shape, layering (Mere owns; serval
  consumes frames), increment ladder, and open questions fixed. No code yet.
