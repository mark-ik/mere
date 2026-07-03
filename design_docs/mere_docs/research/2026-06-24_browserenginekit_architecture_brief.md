# BrowserEngineKit architecture: a reference, and what it means for Mere

Inspection of Apple's [BrowserEngineKit](https://developer.apple.com/documentation/browserenginekit)
(BEK), the framework a third-party browser engine must adopt to ship on iOS / iPadOS outside WebKit.
Captures the prescribed architecture + the concrete API surface, then maps it onto Mere's actor
constellation, capability-scoped scripting, and cross-platform plans.

**Scope / gating.** iOS 17.4+, iPadOS 18.0+. Entitlement-gated to the EU (DMA) and Japan
alternative-browser-engine programs; an embedded engine must be owned by the app developer
(`com.apple.developer.embedded-web-browser-engine.engine-association`). macOS needs none of this (any
engine ships freely there). So BEK matters specifically for a **sovereign iOS/iPadOS browser** lane.

## The architecture BEK mandates

A browser is split into the host app plus three OS-sandboxed **extension** processes, each with only
the privileges its role needs, talking over **XPC**. Untrusted remote content lives only in the
content process, which can touch nothing directly; privileged work is brokered to a more-trusted
process.

- **Browser host (the app)** — GUI (SwiftUI/UIKit), input, coordinates extensions, owns prefs.
  Restricted sandbox (no ad ID, no `canOpenURL` app-presence probing).
- **Networking extension** (one) — `URLSession` / sockets, POST, serves fetch requests from content
  processes. Network access isolated from content parsing.
- **Web content extension** (one+ per tab/document) — the engine: parse HTML/CSS, run JS, build the
  DOM. Strictest sandbox: no user data / system resources; only narrow brokered requests. JIT needs
  the W^X dance + entitlement (below).
- **Rendering extension** (one) — Metal/GPU, video + heavy graphics. Hard memory cap; the system may
  kill it on over-allocation, and content processes can attribute memory to it to avoid DoS-by-alloc.

## Concrete API surface (verified 2026-06-24)

**Process types** (Swift struct / ObjC class): `WebContentProcess` / `BEWebContentProcess`,
`NetworkingProcess` / `BENetworkingProcess`, `RenderingProcess` / `BERenderingProcess`.

**Extension protocols** (adopted by each extension target): `WebContentExtension`,
`NetworkingExtension`, `RenderingExtension`, with opaque `*ExtensionConfiguration` structs.

**Capability model**: `BEProcessCapability` / `ProcessCapability` (the capabilities a helper process
may hold) and `BEProcessCapabilityGrant` (a granted capability handed across). `BEWebContentFilter`
(+ `BEWebContentFilter.PermissionDecision`) filters content. `BEMediaEnvironment` / `MediaEnvironment`
identifies a playback environment; `RenderingExtensionFeature` toggles rendering features.

**IPC**: `BEExtensionProcess` — "a common protocol that creates XPC connections for an extension
process." Communication is XPC, host↔extension and extension↔extension. (Macros gate availability:
`BROWSERENGINEKIT_HAS_LIBXPC`, `_HAS_UIKIT`, etc.)

**UI interaction** (the host/content edge): `BEScrollView` + `BEScrollViewScrollUpdate` +
`BEScrollViewDelegate` (DOM-aware nested scrolling), `BEDragInteraction`, `BEContextMenuConfiguration`,
`BEWebAppManifest`, `BEDownloadMonitor`, and text input via `UITextInput` + `BETextInput`.

**Cross-process accessibility**: `BEAccessibilityRemoteElement` / `BEAccessibilityRemoteHostElement`
share an a11y tree across the process boundary (the content process's a11y surfaced in the host),
plus `BEAccessibilityTextMarkerSupport` and a set of traits/notifications.

**JIT protection** (web content process only): hardware-enforced **W^X** (a page is writable XOR
executable, never both). Two inlined toggles bracket the JIT critical section:
`be_memory_inline_jit_restrict_rwx_to_rw_with_witness()` (make writable; on arm64e inserts a signed
PAC) and `be_memory_inline_jit_restrict_rwx_to_rx_with_witness()` (make executable; authenticates the
PAC, kills the process if invalid). `BE_JIT_WRITE_PROTECT_TAG` is the PAC discriminator. The toggles
**must be force-inlined** with no stack spilling (registers / PAC-protected heap only), or an attacker
who controls the stack subverts the compiler. Entitlements:
`com.apple.security.cs.allow-jit` + `com.apple.developer.kernel.extended-virtual-addressing` (and
`com.apple.security.cs.jit-write-allowlist` if using `pthread_jit_write_with_callback_np`). Japan also
wants `com.apple.security.hardened-process.checked-allocations`.

## What it means for Mere

**Mere's actor constellation already is this decomposition.** BEK's process roles map almost
one-to-one onto Mere's actors (see [actor_constellation_plan](../implementation_strategy/2026-06-03_actor_constellation_plan.md)):

| BEK process | Mere actor | Mere type(s) |
|---|---|---|
| Browser host | meerkat host | the winit/`xilem_serval` host loop |
| Networking extension | I/O / fetch actor | `errand` / netfetcher (the fetch async seam) |
| Web content extension | content actor | serval gnode pool / the constellation content actors |
| Rendering extension | render / compositor | netrender / platen / the scrying GPU import |

The constellation deliberately runs Servo's split **in-process** ("scenes travel as messages"), and
[document_script_substrate_plan](../../archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md)
chose **in-process capability confinement** (WASM components) as "the third option between
semi-trusted-in-process and OS-subprocess." BEK is that OS-subprocess model. Because Mere drew the
boundaries the same way, the model is **process-portable**: on a BEK target each actor promotes to an
extension process and the actor message bus becomes XPC, with little conceptual change. The
parallelism brief's "transferable flat Scene for the one expensive worker→main hop"
([substrate_parallelism_composition_brief](../../2026-06-21_substrate_parallelism_composition_brief.md))
is exactly BEK's content→render (or content→host) frame hop over XPC.

Four sharper correspondences:

1. **Capability grants ≈ DocumentScript capabilities.** `BEProcessCapability` / `…Grant` is the same
   shape as Mere's capability-scoped script/extension contract, but at the *process* boundary. On
   BEK, Mere would get **both layers**: BEK processes for coarse OS isolation, the WASM-component
   capabilities for fine confinement *inside* the content process. Complementary, not redundant.
2. **JIT flips per target.** On the web Mere settled "no JIT" (jco AOT, Nova/Boa interpreters). BEK
   *grants* JIT to the content process (W^X + PAC + entitlement), so the iOS lane could run a JIT JS
   engine, unlike web. A per-target scripting decision, not a global one. (The W^X-with-witness +
   force-inline + no-stack-spill discipline is a real engineering cost, but it is a known, bounded
   one.)
3. **Rendering memory cap ≈ the actor budget.** BEK's hard cap + memory attribution on the rendering
   extension mirrors the physics/render actor budgeting Mere already reasons about; the
   content→render attribution maps onto who owns a Scene's GPU memory in the
   [scrying](../implementation_strategy/2026-06-10_scrying_tile_plan.md) / netrender path.
4. **Cross-process a11y is solved-shape.** `BEAccessibilityRemoteElement` is the pattern for surfacing
   the content process's a11y tree in the host - the same problem Mere's DOM-sourced a11y faces once
   content is a separate actor/process.

**Strategic fork.** BEK = "ship serval as a real iOS/iPadOS browser engine," a *more sovereign* and
heavier commitment than the
[browser_extension_companion_plan](../implementation_strategy/2026-06-23_browser_extension_companion_plan.md)'s
"ride inside someone else's browser as an extension / PWA." They are two different cross-platform
strategies; BEK is the one where Mere *is* the engine. It is also entitlement-gated (EU/Japan,
owned-engine), so it is a deliberate, applied-for path, not a default.

## Takeaways

- Strong validation that Mere's actor split is the right shape: Apple arrived at the same
  security-motivated host / net / content / render decomposition.
- A concrete iOS deployment target whose process model Mere already matches; promoting actors to BEK
  extensions is a transport change (XPC) over boundaries that exist, plus the per-process entitlement
  + sandbox work.
- The per-target JIT divergence (allowed on BEK, not on web) is now documented with the exact
  mechanism + entitlements, for whenever the iOS lane is scoped.
- Not on the near-term path unless the sovereign-iOS-engine lane is chosen over the extension/PWA
  companion; captured here as the reference for that decision.

## Source pages

- [Designing your browser architecture](https://developer.apple.com/documentation/browserenginekit/designing-your-browser-architecture)
- [Extension lifecycle](https://developer.apple.com/documentation/browserenginekit/extension-lifecycle)
- [Protecting code compiled just in time](https://developer.apple.com/documentation/browserenginekit/protecting-code-compiled-just-in-time)
- [Creating browser extensions in Xcode](https://developer.apple.com/documentation/browserenginekit/creating-browser-extensions-in-xcode)
