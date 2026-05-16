# OS Plumbing Reuse Audit — research brief

**Date**: 2026-05-15
**Status**: Survey / research probe (per-subsystem extraction posture; defers concrete commitments to follow-up plans)
**Scope**: For each OS-plumbing subsystem the [spatial chrome IR brief §8.2](2026-05-15_spatial_chrome_ir_brief.md) names as the actually-expensive part of substrate-as-host: characterise what gpui provides, what the Rust ecosystem provides, what the cross-platform variation is, and what extraction posture (lift-as-crate / fork-and-trim / reimplement-against-trait / use-existing-ecosystem-crate / accept-rough-edges) is plausible. Ground for §8.2's precondition before substrate-as-host can be reopened. Useful even under the current gpui host because it inventories the dependencies our host actually leans on, regardless of where it's hosted.

**Related**:

- [`2026-05-15_spatial_chrome_ir_brief.md`](2026-05-15_spatial_chrome_ir_brief.md) — parent. §8.2 names this audit as a precondition for reopening the host pivot.
- [`2026-05-15_renderer_registry_contract_brief.md`](2026-05-15_renderer_registry_contract_brief.md) — sibling brief (same day). The renderer registry is host-agnostic; this audit is what tells us whether a non-gpui host is realistic.
- Memory: `project_host_framework_glass_gpui` (gpui via PlatformSurface is canonical until preconditions clear), `feedback_dont_dismiss_borrowable_components` (sketch the layered architecture before cutting borrowed pieces; Mark's borrow-intuitions are reliable), `user_linux_hardware` (only Linux box is an old Mint Acer with X11 default; Wayland-specific extraction can't be validated locally — default Linux receipts to X11+master-capture), `feedback_consumer_pull_gates_check_first` (read upstream API before deferring wraps; we're the only consumer in our codebase).

---

## Thesis

> **The renderer is the cheap part. OS plumbing is where years of host-framework hardening live, and where rebuild risk is real. For each subsystem, three things matter: how much Rust-ecosystem coverage already exists, how tight the gpui implementation is to gpui's internals, and how much of it Mere actually needs. Map all three before any substrate-as-host commitment.**

This is a survey, not a decision. Its job is to make the substrate-as-host conversation honest about what would have to be rebuilt or borrowed, so the conversation isn't "well, surely we could…" but "here's exactly what each subsystem costs at each adoption mode."

---

## 1. Audit method

For each subsystem:

- **What it is** — the user-visible behaviour the OS layer is responsible for.
- **Cross-platform shape** — how Windows / macOS / Linux-X11 / Linux-Wayland each express it.
- **gpui's posture today** — how gpui handles it (architecturally; not source-line-grade — that's the next-level audit). Includes whether the relevant gpui module is tightly woven with gpui's view tree or naturally extractable.
- **Rust ecosystem alternatives** — non-gpui crates that implement this. Includes winit (Mere uses winit indirectly via gpui), AccessKit, arboard, smithay-clipboard, ime-rs, etc.
- **What Mere actually needs** — the minimum spec that satisfies Mere's use cases. Often less than what gpui ships (which targets Zed's IDE workload).
- **Extraction posture** — one of:
  - *use-ecosystem*: a non-gpui crate already covers the need; no extraction work.
  - *lift-as-crate*: gpui's implementation could be extracted to a standalone crate with bounded surgery (license is Apache 2; extractable in principle).
  - *fork-and-trim*: gpui's implementation is too tangled to lift; clean-room rewrite informed by gpui's structure.
  - *reimplement-against-trait*: subsystem is small enough that own-implementation against an explicit trait is plausible.
  - *accept-rough-edges*: ship without the polish; backfill later.
- **Cross-platform validation gap** — what we can verify locally (Windows ✅; Linux X11 on the Mint Acer per `user_linux_hardware`) vs what needs external verification (macOS, Linux Wayland).

The audit's recommendations are *postures*, not commitments. None of them schedule work.

## 2. Subsystem audits

### 2.1 IME (input method editors)

**What it is.** Composition of multi-keystroke characters: CJK input methods, dead keys, accents, emoji pickers, dictation. Surfaces for composition strings (the in-progress text), commit events, candidate windows, cursor/caret positioning hints.

**Cross-platform shape.**
- Windows: TSF (Text Services Framework) — modern; ITextStoreACP / ITfContextOwner. IMM32 is the legacy path some apps still use.
- macOS: NSTextInputClient protocol; the OS injects composition strings via the responder chain.
- Linux/X11: XIM (or fcitx5/ibus IM modules). Notoriously fragile.
- Linux/Wayland: zwp_text_input_v3 protocol; desktop environment provides the IME server.

**gpui's posture today.** gpui has its own IME plumbing tied to its window/view system; macOS path is well-hardened (Zed-driven); Windows TSF is implemented; Linux IME is the historically-weakest leg across all desktop frameworks (gpui included). Tight coupling to gpui's input dispatch; not naturally a leaf module.

**Rust ecosystem alternatives.** winit has basic IME support (preedit, commit) on all platforms but is acknowledged-shallow on the candidate-window / cursor-feedback side. `tao` (wry's window crate) is similar. No standalone "IME for Rust" crate exists at the polish gpui has.

**What Mere actually needs.** Composition + commit for text editing in: omnibar, knot editing, text inputs in serval/wry pages (handled by the engine), graph node label edits. Candidate-window positioning is desirable but not load-bearing for v1.

**Extraction posture.** *fork-and-trim* on macOS (gpui's implementation is the reference; extraction would essentially be a clean-room rewrite informed by it). *lift-as-crate* possible on Windows TSF if the implementation isn't too entangled. *use-ecosystem* (winit baseline) on Linux as a pragmatic floor; gpui's Linux IME is no better than winit's anyway.

**Cross-platform validation gap.** macOS: can't validate locally. Linux Wayland: can't validate locally per `user_linux_hardware`. Windows ✅; Linux X11 ✅ on Mint Acer.

**Posture summary.** The expensive subsystem. macOS hardening is the gpui asset hardest to replace and the one that justifies "stay on gpui until we prove otherwise."

---

### 2.2 Accessibility

**What it is.** Screen-reader integration: exposing the UI as a tree of typed nodes (button, text, list, etc.) with names, roles, states, and actions; routing OS-mediated user input from assistive tech back into the application as semantic actions.

**Cross-platform shape.**
- Windows: UIA (UI Automation).
- macOS: AX (NSAccessibility protocol).
- Linux: AT-SPI (D-Bus-based).

**gpui's posture today.** gpui uses **AccessKit**, an MIT/Apache cross-platform accessibility crate that abstracts UIA / AX / AT-SPI behind a single tree-of-nodes API. AccessKit is independent of gpui — Mere's mere-domain `uxtree` already plans to be AccessKit-native (per [terminology #engines](../../TERMINOLOGY.md) and adjacent docs).

**Rust ecosystem alternatives.** AccessKit *is* the ecosystem. There's no other game in town.

**What Mere actually needs.** AccessKit nodes for every chrome element (panels, panes, toolbars, the orrery, edges, nodes). Engines (serval / scrying / wry) maintain their own accessibility trees for their content; the host stitches at the boundary.

**Extraction posture.** *use-ecosystem* — already done structurally. The substrate-as-host story for accessibility is essentially "drop AccessKit in the same place gpui does," and our `uxtree` work is already moving toward this.

**Cross-platform validation gap.** Same as IME — macOS and Wayland accessibility require remote validation. AccessKit's own test infrastructure covers most of this independently.

**Posture summary.** The cheap subsystem. Accessibility is largely a solved Rust problem because of AccessKit. No gpui dependency to extract.

---

### 2.3 Focus + keyboard navigation

**What it is.** Tracking which UI element receives keyboard input; visually distinguishing the focused element; tab-order traversal; Esc / Enter / arrow / shortcut routing.

**Cross-platform shape.** Largely OS-agnostic; some interaction with OS conventions (macOS uses different focus traversal than Windows; Linux varies).

**gpui's posture today.** gpui has a focus model tied to its view tree (focusable elements register with the window's focus manager; focus moves via tab traversal or programmatic `focus()`). Tightly coupled to the view tree's structure.

**Rust ecosystem alternatives.** No standalone focus-management crate. Most frameworks (iced, egui, winit consumers) implement their own.

**What Mere actually needs.** Focus tracking per pane (which pane is keyboard-active); within-pane focus delegation to the renderer (engines manage their own focus internally; cartography/graph-canvas needs node focus; mere-domain panels need element focus). Action-bus-routed focus moves (Tab / Shift-Tab dispatch through the bus per [multiplexer brief §5.8](2026-05-11_browser_multiplexer_framing.md)).

**Extraction posture.** *reimplement-against-trait*. Focus management is not very large; coupling it to a substrate-specific spatial scene graph (focus following the cursor's last spatial position; per-renderer focus delegation via the renderer registry) is *more natural than* extracting gpui's tree-shaped focus model. Probably not worth lifting.

**Cross-platform validation gap.** Minimal; focus is largely an in-process concern.

**Posture summary.** Reimplement, don't extract. Substrate-shaped focus is structurally different from tree-shaped focus.

---

### 2.4 Drag-and-drop

**What it is.** Two flavours: (a) intra-app drags (drag a tile to a new pane, drag a tab out of a window); (b) OS-mediated drags (drag a file from Finder/Explorer into Mere; drag text out of a knot to another app).

**Cross-platform shape.**
- Windows: COM drag-drop (IDropTarget / IDropSource); modern UWP has its own model.
- macOS: NSDraggingDestination / NSDraggingSource.
- Linux/X11: XDND protocol.
- Linux/Wayland: zwp_data_device protocol family.

**gpui's posture today.** gpui has intra-app drag (Zed uses it heavily for tab manipulation, tree drags, etc.) and OS-mediated drag for files. Macos polish is high; Windows and Linux are functional.

**Rust ecosystem alternatives.** winit has *very basic* drag (file-drop events; not full drag-source/destination). No standalone full-drag crate. arboard handles clipboard, not drag.

**What Mere actually needs.** Intra-app drag is critical (tear-out gestures per [tearout-operations brief](2026-05-11_tearout_operations_brief.md), tab rearrange, panel resize). OS-mediated file-drop is critical (drag a knot file in; drag a URL in). OS-mediated drag-out (Mere → other app) is nice-to-have but not v1-critical.

**Extraction posture.** *lift-as-crate* for the OS-mediated drag-source/destination wrappers (these are largely cross-platform abstraction over native COM/ObjC/XDND/Wayland surfaces — a candidate for a `mere-os-drag` crate or contribution back to winit). *reimplement-against-trait* for intra-app drag (the substrate's spatial scene graph already has hit-testing and identity, so intra-app drag is mostly "track the dragged identity and emit `drag.start` / `drag.move` / `drag.end` actions on the bus").

**Cross-platform validation gap.** macOS drag-out: can't validate locally. Linux Wayland: can't. Windows ✅; X11 ✅.

**Posture summary.** Intra-app reimplements naturally on the substrate. OS-mediated wraps existing native protocols — viable as a small standalone crate (whether forked from gpui or written from scratch is a judgment call at the time).

---

### 2.5 Window decoration + state

**What it is.** Title bars, system menus (close/minimise/maximise on Windows; traffic lights on macOS; window-decoration headers on Linux). Window state management (maximised, minimised, fullscreen, restored, snapped). Custom title-bar drag regions for client-side-decorated windows.

**Cross-platform shape.**
- Windows: native title bar by default; custom title bars require WM_NCCALCSIZE handling. Server-side decorations standard.
- macOS: native title bar with traffic lights; custom title bars require `titlebarAppearsTransparent` + content-view manipulation.
- Linux/X11: window manager decides (server-side via the WM, or client-side decorations / CSD).
- Linux/Wayland: client-side decorations are the modern norm; libdecor for fallback.

**gpui's posture today.** gpui implements custom title-bar drag regions (Zed uses them for the project-name + tab strip in the title area). Cross-platform but with the usual Wayland CSD nuances.

**Rust ecosystem alternatives.** winit handles window state (maximise / minimise / fullscreen) cleanly; CSD support varies by platform; custom title bars are app-specific code in winit consumers.

**What Mere actually needs.** Decision-deferred. The current gpui host probably uses gpui's title bar; Mere's chrome can either (a) use native title bars and place chrome below them, or (b) use a custom title bar that absorbs the title-bar real estate into the chrome (workbench shellbar, etc.). The shellbar-rotation per [multiplexer brief §5.2](2026-05-11_browser_multiplexer_framing.md) hints at (b) but doesn't commit.

**Extraction posture.** *use-ecosystem* (winit) for window state. *reimplement-against-trait* for custom title bar drag regions if Mere wants them — the implementation is small per platform and gpui's variant is tied to its event system. Custom title bars are a *if you want them, you pay for them* feature.

**Cross-platform validation gap.** macOS title-bar manipulation: can't validate locally. Linux Wayland CSD: can't.

**Posture summary.** winit covers most of what's needed; custom title bars are a small per-platform implementation at adoption time. Not a major extraction concern.

---

### 2.6 Clipboard

**What it is.** Cut/copy/paste with multiple data types (plain text, rich text, images, custom MIME types for app-specific data). Cross-platform and OS-mediated.

**Cross-platform shape.**
- Windows: GetClipboardData / SetClipboardData with platform format atoms.
- macOS: NSPasteboard with UTI-typed data.
- Linux/X11: PRIMARY + CLIPBOARD selections (two clipboards!), MIME types via INCR protocol for large data.
- Linux/Wayland: zwp_data_device protocol.

**gpui's posture today.** gpui has its own clipboard layer; supports text + custom types; cross-platform.

**Rust ecosystem alternatives.** Multiple options:
- **arboard** — most popular standalone clipboard crate; text + image; Apache/MIT.
- **clipboard-rs** — typed clipboard with MIME type support; broader than arboard.
- **smithay-clipboard** — Wayland-specific.

**What Mere actually needs.** Text clipboard (knot edits, omnibar, address copies). Custom MIME types for cross-Mere drag-as-clipboard scenarios (drag a node into the clipboard, paste it elsewhere). Image clipboard for screenshot-like flows (e.g., paste-into-knot of an image clipboard payload).

**Extraction posture.** *use-ecosystem* — arboard or clipboard-rs are the right baseline. gpui's clipboard isn't significantly better than arboard for what Mere needs.

**Cross-platform validation gap.** Wayland: can't validate locally; arboard's Wayland support is via smithay-clipboard internally.

**Posture summary.** Cheap subsystem. Use the ecosystem; no gpui extraction needed.

---

### 2.7 Color management

**What it is.** Display color profile awareness (sRGB vs Display P3 vs Rec.2020); HDR rendering on capable displays; color-space conversion for the GPU output.

**Cross-platform shape.**
- Windows: WCS (Windows Color System); HDR via DirectX swap chain configuration.
- macOS: ColorSync; HDR via Metal layer pixel format.
- Linux: nascent (KMS color management); not generally enabled.

**gpui's posture today.** gpui handles color profile awareness for its own renderer (blade pipeline). HDR support is platform-specific.

**Rust ecosystem alternatives.** wgpu exposes display color space configuration via `wgpu::Surface::configure` with the appropriate `TextureFormat`. vello/netrender consume wgpu surfaces; they inherit whatever color space the surface is configured with. Beyond surface configuration, color management in Rust is an ad-hoc per-app concern.

**What Mere actually needs.** Correct sRGB rendering on commodity displays (the floor). Display P3 awareness on capable displays (macOS, modern Windows laptops). HDR is not a v1 concern.

**Extraction posture.** *use-ecosystem* — wgpu's surface configuration + correct color-space handling in netrender + vello covers the floor. The substrate-as-host story for color management is essentially "configure your wgpu surface correctly and trust the renderer." gpui-extraction not worthwhile.

**Cross-platform validation gap.** macOS Display P3 + HDR: can't validate locally.

**Posture summary.** Cheap subsystem at the floor; HDR is a real subsystem if/when needed. Floor is wgpu-native.

---

### 2.8 HiDPI / DPR scaling

**What it is.** Logical-vs-physical pixel handling: a 200×100 logical button at 2x DPR is 400×200 physical pixels, but should *look* the same size on screen. Cross-monitor DPR (a window dragged from a 1x display to a 2x display has to re-render everything at the new scale).

**Cross-platform shape.**
- Windows: per-monitor DPI awareness v2; WM_DPICHANGED on monitor changes.
- macOS: backing-scale-factor on NSWindow; AppKit handles most of this transparently.
- Linux/X11: `Xft.dpi` and per-monitor DPI hints; framework-by-framework.
- Linux/Wayland: `wl_surface::set_buffer_scale`; protocol-driven.

**gpui's posture today.** gpui handles per-monitor DPR with full re-layout on monitor changes. Production-grade; Zed runs on every common DPR scenario.

**Rust ecosystem alternatives.** winit handles DPR cleanly (per-monitor v2 on Windows; native on macOS; protocol-correct on Wayland; X11 best-effort). Most winit consumers handle re-layout themselves.

**What Mere actually needs.** Per-monitor DPR; correct re-layout on monitor changes; correct framebuffer sizing for the wgpu surface. Cross-monitor drag (one window spanning two monitors of different DPR — Windows handles this by picking the dominant monitor's DPR; macOS does it natively).

**Extraction posture.** *use-ecosystem* (winit) for OS event handling; *reimplement-against-trait* for the substrate-side re-layout cascade (the substrate's spatial scene graph naturally handles this — every transform is logical, the scene's "scale to physical" is one matrix at the root). Probably the easiest subsystem of the whole audit on the substrate side.

**Cross-platform validation gap.** macOS multi-monitor: can't validate locally. Wayland: can't.

**Posture summary.** winit + substrate-side matrix scaling. Not a meaningful extraction concern.

---

### 2.9 Cursor management

**What it is.** Setting the OS cursor (arrow, I-beam, hand, resize handles, custom cursors). Hide-cursor for fullscreen video / drawing modes. Cursor confinement (lock cursor for camera-style controls).

**Cross-platform shape.** Each OS has its own cursor API; cross-platform Rust crates wrap them.

**gpui's posture today.** gpui sets cursors via its window abstraction; standard set + custom cursor images.

**Rust ecosystem alternatives.** winit covers all common cursor needs (standard cursors, custom images, hide, confine via CursorGrabMode).

**What Mere actually needs.** Standard cursors (arrow, I-beam for text, resize for splitter handles, drag for tab grabs, hand for clickable graph nodes). Custom cursors not v1-critical.

**Extraction posture.** *use-ecosystem* (winit). No gpui extraction.

**Cross-platform validation gap.** Minimal.

**Posture summary.** Trivial subsystem. winit covers it.

---

### 2.10 Display / monitor enumeration + change events

**What it is.** Listing connected monitors with their geometry, DPR, and refresh rate. Notifications when monitors are added/removed/reconfigured.

**Cross-platform shape.** All OSes provide enumeration; Wayland's is protocol-driven (wl_output).

**gpui's posture today.** gpui enumerates monitors for window placement. Standard.

**Rust ecosystem alternatives.** winit exposes monitor enumeration with change events.

**What Mere actually needs.** Initial monitor enumeration for window placement (restore window on the same monitor it was on at last shutdown). Change events for re-layout when a monitor is unplugged.

**Extraction posture.** *use-ecosystem* (winit).

**Posture summary.** Trivial. winit covers it.

---

### 2.11 System theme integration

**What it is.** Detect OS dark/light mode preference; respond to runtime changes; pull system accent color; respond to system high-contrast / reduced-motion / reduced-transparency settings (accessibility metadata).

**Cross-platform shape.**
- Windows: registry queries + WM_SETTINGCHANGE; UISettings on UWP.
- macOS: NSAppearance + KVO observation.
- Linux/Freedesktop: org.freedesktop.appearance D-Bus interface (modern); per-DE settings (older).
- Linux/Wayland: same D-Bus path as X11.

**gpui's posture today.** gpui detects light/dark mode and exposes it; high-contrast / reduced-motion not as polished.

**Rust ecosystem alternatives.** **dark-light** crate (small, MIT) covers light/dark detection cross-platform. Reduced-motion / high-contrast detection is mostly per-platform code; no popular crate.

**What Mere actually needs.** Light/dark mode detection (drives the chrome color scheme). Accent color is nice but not load-bearing. Reduced-motion is an accessibility floor — affects `feedback_graph_canvas_navigation_defaults` (inertia is a default; respect reduced-motion preference).

**Extraction posture.** *use-ecosystem* (dark-light) for dark/light. *reimplement-against-trait* for reduced-motion detection (small, per-platform code).

**Cross-platform validation gap.** macOS reduced-motion: can't validate locally.

**Posture summary.** Cheap subsystem. dark-light covers the main concern.

---

### 2.12 System notifications

**What it is.** Native OS toast notifications (Windows Action Center, macOS Notification Center, Linux org.freedesktop.Notifications).

**Cross-platform shape.** Standard per-OS APIs.

**gpui's posture today.** gpui has notification support for Zed's "build complete" / "AI agent done" notifications.

**Rust ecosystem alternatives.** **notify-rust** (Linux-focused but cross-platform-shimmed). **winrt-notification** (Windows). **mac-notification-sys** (macOS). No single dominant cross-platform crate at this writing.

**What Mere actually needs.** Probably not v1-critical. A "moot received contribution," "session restore failed," "background fetcher complete" notification surface is plausible later. The kernel doesn't need it; the host might.

**Extraction posture.** *use-ecosystem* with multi-crate orchestration if needed. *accept-rough-edges* — Mere can ship without native notifications and use in-chrome toasts.

**Posture summary.** Defer. Not a substrate-as-host blocker.

---

### 2.13 File dialogs

**What it is.** Native open / save / directory picker dialogs. Mediated by the OS for security (sandbox-permitted file access).

**Cross-platform shape.** Standard per-OS APIs; Wayland uses xdg-desktop-portal.

**gpui's posture today.** gpui exposes file dialogs for Zed's file-open/save flows.

**Rust ecosystem alternatives.** **rfd** (Rust File Dialogs) — popular; Apache/MIT; cross-platform; native dialogs. **tinyfiledialogs** (older C-binding alternative).

**What Mere actually needs.** Open: import a knot, import a graph snapshot, attach a file to a knot. Save: export a knot, export a graph snapshot. Directory pick: rare but plausible for project-shaped graphs.

**Extraction posture.** *use-ecosystem* (rfd). No gpui extraction.

**Posture summary.** Cheap subsystem. rfd covers it.

---

### 2.14 Pointer / touch / pen / multi-touch

**What it is.** Beyond basic mouse: pen events (Wacom-like; pressure, tilt), touch events (multi-finger), gestures (pinch, rotate). Critical for a spatial canvas (graph canvas pinch-zoom, two-finger pan).

**Cross-platform shape.**
- Windows: WM_POINTER (modern, unified); WM_TOUCH (legacy).
- macOS: NSEvent gesture recognisers.
- Linux/X11: XInput2 with valuators.
- Linux/Wayland: wl_pointer + wl_touch.

**gpui's posture today.** gpui has mouse + scroll wheel; pen / touch / gesture support varies by platform. Zed is keyboard-first; this isn't gpui's strongest area.

**Rust ecosystem alternatives.** winit has pointer + touch events; gesture-recognition (pinch, rotate) is generally app-side. No dominant gesture-recognition crate.

**What Mere actually needs.** Mouse: critical (and gpui-via-winit covers it). Touch + pinch-zoom: critical for the graph canvas (pinch-zoom is a natural infinite-canvas gesture per `feedback_graph_canvas_navigation_defaults`). Pen: nice-to-have for knot annotation in the future.

**Extraction posture.** *use-ecosystem* (winit) for raw events. *reimplement-against-trait* for gesture recognition (pinch, rotate, two-finger-pan) — not a large amount of code; no dominant ecosystem crate to use.

**Cross-platform validation gap.** macOS gestures: can't validate locally. Wayland touch: can't.

**Posture summary.** winit floor + Mere-side gesture recognition. Substrate-shaped naturally — gestures are spatial, scene-graph friendly.

---

## 3. Summary table

| Subsystem                       | Recommended posture          | Cross-platform risk           | Substrate-as-host blocker? |
| ------------------------------- | ---------------------------- | ----------------------------- | -------------------------- |
| 2.1  IME                        | fork-and-trim (macOS); lift-as-crate (Win); use-ecosystem (Linux) | High (macOS most painful)     | **Yes** — biggest single risk |
| 2.2  Accessibility              | use-ecosystem (AccessKit)    | Low                           | No                         |
| 2.3  Focus + keyboard           | reimplement-against-trait    | Low                           | No                         |
| 2.4  Drag-and-drop              | reimplement intra-app; lift-or-write OS-mediated | Medium                  | Partial (intra-app no; OS-mediated yes) |
| 2.5  Window decoration + state  | use-ecosystem (winit) + reimplement custom title bars | Medium       | No                         |
| 2.6  Clipboard                  | use-ecosystem (arboard)      | Low                           | No                         |
| 2.7  Color management           | use-ecosystem (wgpu surfaces) | Low (floor); high (HDR)      | No (floor); deferred (HDR) |
| 2.8  HiDPI / DPR scaling        | use-ecosystem + substrate-native re-layout | Medium          | No                         |
| 2.9  Cursor management          | use-ecosystem (winit)        | Low                           | No                         |
| 2.10 Display enumeration        | use-ecosystem (winit)        | Low                           | No                         |
| 2.11 System theme integration   | use-ecosystem (dark-light)   | Low                           | No                         |
| 2.12 System notifications       | accept-rough-edges initially | Low (can defer)               | No                         |
| 2.13 File dialogs               | use-ecosystem (rfd)          | Low                           | No                         |
| 2.14 Pointer / touch / pen      | use-ecosystem + Mere-side gestures | Medium (touch on Wayland)| No                         |

**One real blocker (IME), several medium-effort items (intra-app drag, custom title bars, gestures), most subsystems handled by ecosystem.**

This is more reassuring than the spatial-chrome-IR brief §8.2 framing suggested. The conventional wisdom that "OS plumbing is years of work" was largely true *before* AccessKit, winit's modern maturity, arboard, rfd, dark-light, and wgpu's surface model. Most subsystems have ecosystem coverage now. The substrate-as-host conversation reduces to "do we want to take on the IME hardening commitment, especially on macOS, that gpui has already done and we'd be re-doing?"

## 4. The `mere-os-plumbing` umbrella sketch

If/when adoption advances, a sensible decomposition:

```
mere-os-plumbing/                       (umbrella; minimal API surface)
├── mere-os-window/                     (winit wrapper + custom title bar)
├── mere-os-input/                      (raw input + gesture recognition)
├── mere-os-ime/                        (the hard one; per-platform implementations)
├── mere-os-clipboard/                  (arboard re-export with MIME-typed convenience)
├── mere-os-drag/                       (intra-app + OS-mediated drag wrappers)
├── mere-os-theme/                      (dark-light re-export + reduced-motion)
└── mere-os-dialogs/                    (rfd re-export with Mere-shaped types)
```

`mere-os-a11y` doesn't appear because AccessKit is the public API — uxtree consumes it directly with no Mere wrapper crate.

`mere-os-notifications` and `mere-os-color-management` are deferred until needed.

This decomposition is **subsystem-per-crate** because each subsystem has independent extraction maturity, independent platform validation needs, and independent ecosystem alternatives. Bundling them risks tangling the cheap subsystems with the expensive (IME) one.

## 5. Cross-platform validation gaps

Per `user_linux_hardware`, local Linux validation is X11-only on the Mint Acer; Wayland-specific behaviours can't be validated locally. macOS can't be validated locally either.

What this means for adoption planning:

- **Windows ✅** — primary local validation environment for substrate-as-host work (matches current dev setup).
- **Linux X11 ✅** — secondary local validation; Mint Acer.
- **Linux Wayland ❌ locally; needs remote/CI/community validation.** Acutely true for IME (zwp_text_input_v3) and drag-and-drop (zwp_data_device).
- **macOS ❌ locally; needs remote/CI/community validation.** Acutely true for IME (NSTextInputClient — the most-polished gpui surface), gestures (NSEvent), and color management (Display P3).

Any substrate-as-host adoption plan must include a **non-local-validation strategy** for Wayland and macOS — CI runners, community testing, or a deliberate "Windows + X11 first; macOS / Wayland gated behind external help" sequencing.

This is itself an argument for staying on gpui until we have a clearer remote-validation pipeline: gpui has community validation across all four targets that we can't reproduce alone.

## 6. Reuse-mode glossary

Repeating §1's posture vocabulary for reference, with concrete examples from the audit:

- **use-ecosystem** — A non-gpui crate already covers the need. Examples: AccessKit (a11y), arboard (clipboard), rfd (dialogs), dark-light (theme), winit (window/input/cursor/HiDPI/displays).
- **lift-as-crate** — gpui's implementation could be extracted to a standalone crate with bounded surgery. License is Apache 2 — extractable in principle. Example candidates: Windows TSF IME, OS-mediated drag-source/destination wrappers.
- **fork-and-trim** — gpui's implementation is too tangled with gpui's view tree / dispatch system to lift cleanly; clean-room rewrite informed by gpui's structure. Example: macOS NSTextInputClient IME (the implementation is gpui-shaped enough that a clean rewrite reading gpui as reference is more honest than a "lift").
- **reimplement-against-trait** — Subsystem is small enough that own-implementation against an explicit trait is plausible and probably cleaner than borrowing. Examples: focus management (substrate-shaped, not tree-shaped), intra-app drag (substrate's spatial model gives this for free), gesture recognition (pinch / rotate / pan), reduced-motion detection.
- **accept-rough-edges** — Ship without the polish; backfill later. Example: system notifications (can use in-chrome toasts initially).

## 7. What this audit does and does not decide

**Decides:**

- A per-subsystem extraction posture (not commitment).
- That **IME on macOS is the single subsystem that justifies "stay on gpui" the most**.
- That **AccessKit + winit + ecosystem crates cover most subsystems**, reducing the substrate-as-host plumbing risk significantly relative to the spatial-chrome-IR brief §8.2 framing.
- A `mere-os-plumbing` umbrella decomposition (subsystem-per-crate).
- That cross-platform validation gaps (Wayland, macOS) require a non-local-validation strategy independent of substrate adoption.

**Does not decide:**

- Whether to adopt substrate-as-host. Spatial-chrome-IR brief §8.2 still gates that on three preconditions; this audit only addresses one of the three.
- Specific crate choices within posture categories. (arboard vs clipboard-rs is a pick-when-adopting concern.)
- IME-specific adoption strategy. The audit notes the cost; deciding what to do about it is a follow-up if/when substrate-as-host is on the table.
- Source-line-grade gpui audit per subsystem. This is architectural; the next-level audit (open the gpui source for a given subsystem and trace the API surface in detail) happens at adoption-decision time, per subsystem in priority order.

## 8. Open questions

### 8.1 Is winit the right baseline?

This audit assumes winit (or a winit-shaped crate) is the floor for window/input/cursor/displays. That's defensible — winit is the most-used Rust window crate and most other host frameworks (iced, egui consumers, gpui itself indirectly) have lived with or contributed to it. But the substrate-as-host could in principle pick a different baseline (raw-window-handle over a thinner wrapper; a Mere-specific window layer for tighter integration). Defer; flag.

### 8.2 IME-specifically: what minimum is acceptable for v1?

The spatial-chrome-IR brief §8.2 frames IME as expensive without specifying what "polished IME" means in Mere's context. v1 is probably "composition + commit + reasonable candidate-window positioning on Windows / macOS / X11; Wayland on best-effort." Worth a follow-up brief that defines the acceptance criteria so the IME work isn't open-ended.

### 8.3 Should `mere-os-plumbing` umbrella exist before substrate-as-host?

The umbrella organises subsystems independently of substrate adoption. Two of its candidates (clipboard, dialogs, notifications) could land *under the current gpui host* by simply bypassing gpui's versions and using ecosystem crates directly. This would prove the extraction posture and reduce gpui leakage even before substrate-as-host is on the table. Worth considering as an incremental hardening step.

### 8.4 Engine-side OS plumbing inheritance

Engines (serval, scrying, wry) bring their own OS plumbing inside their content surfaces — wry inherits the system WebView's IME/clipboard/etc.; serval implements its own; scrying inherits the system WebView's. Mere's chrome OS plumbing only covers the *chrome*, not the engine surfaces. This is correct (engine surfaces are opaque to Mere's input router, per the [renderer-registry brief §10.4](2026-05-15_renderer_registry_contract_brief.md)) but worth being explicit about: the audit's scope ends at the chrome / engine boundary.

### 8.5 The "honest broker" question

If Mere's OS plumbing is mostly ecosystem crates plus thin wrappers, is there genuine differentiation in shipping our own host vs. a community-maintained host like gpui or a reimagined-on-winit host? The differentiation is the *substrate*, not the plumbing — but the plumbing is what makes the substrate accessible. Worth an honest-broker check: would Mere-on-winit + AccessKit + arboard + rfd + dark-light + Mere-side substrate be *materially better* than gpui-as-host + spatial-substrate-as-overlay? Probably yes (one GPU stack; substrate is native, not retrofit) but the case wants explicit articulation. Defer; flag for the substrate-prototype-plan if/when it lands.

## 9. Decisions and non-decisions

**Decides:** the per-subsystem posture vocabulary, which subsystems are ecosystem-covered today, which subsystems would require real engineering for substrate-as-host, the umbrella crate decomposition shape, the cross-platform validation gap as a substrate-adoption concern.

**Does not decide:** substrate-as-host adoption (spatial-chrome-IR brief §8 still gates it on three preconditions), specific subsystem extraction work, IME acceptance criteria (§8.2), umbrella-crate-before-substrate (§8.3).

**Implies follow-ups:**

- *IME acceptance-criteria brief* — addresses §8.2.
- *Substrate prototype plan* — folded into Phase 8 of [`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md). The OS plumbing audit feeds into the prototype's "what's the floor we need" inventory.
- *Source-grade gpui IME audit* — addresses the next-level question only IME warrants raising before substrate-as-host commits. Open the source, characterise the macOS and Windows TSF surfaces in detail, identify lift-vs-rewrite per platform.
- *Honest-broker review* — addresses §8.5; companion to the substrate prototype plan and explicitly called out in the modular adoption plan.
