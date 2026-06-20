# Native Surface Compositing Plan: overlays above embedded WebView surfaces

**Date**: 2026-06-19
**Status**: Planning (direction set with Mark; no code yet). How meerkat's chrome, and its
modal overlays especially (context menu, palette, find, settings), composites above embedded
native surfaces (scrying System WebViews) and the host-composited content layers, so an overlay
is never occluded by content.
**Code**: `crates/meerkat/` (render.rs compositing + present, scrying_host.rs), `wgpu-scry/scrying`
(the WebView2 producer).

Sibling docs:

- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md): the one-shell-document
  chrome; the context menu lives in that document. This plan owns the layer *below* it, how the
  shell document composites over embedded native content. The in-document z-order (menu over the
  orrery node cards) was fixed there by document order; this plan is the native-surface half.
  This plan's **by-nature layering** (the four-way split below) **supersedes** unified-document-host's
  "every host-composited surface → document external-texture element" recipe **for the
  genuinely-external (scry) case**: a system WebView2 visual is a native composition visual under the
  chrome, never a document external-texture element. The orrery gyre *scene* as an `<external-texture>`
  element is retained there (it is a rendered field, not genuinely-external content — a texture is
  correct).
- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md): the
  orrery node/card model. This plan's "snapshot texture vs live visual" split is the compositing
  half of the decision that the orrery card is a snapshot and the live view moves to pelt.
- [scrying_tile_plan](2026-06-10_scrying_tile_plan.md): the WebView2 producer. This plan changes
  how its output is hosted, not the producer itself.
- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md): the
  ownership map. This plan's scrying-surface compositing is the realization of the spine's
  Render-stage **Lane C** (self-rendering external-texture). The spine (line 139) assigns the
  **external-texture-*input* bridge** to window-composition / tearout-composability — *not* to this
  plan; see the tearout bullet below.
- [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md):
  the **owner of the performance strategy** (bake-vs-live, per-lane perf ceilings). This plan's
  static-snapshot decision is a **dormancy/memory** choice it justifies; cross-reference, do not
  restate.
- tearout-composability / window-composition: owns the external-texture-**input** bridge (spine line
  139) — the input-bearing `<external-texture>` element that forwards a serval hit-test hit in
  texture-local coords to the producer. This plan's **API-forwarded input** on the off-window host
  (`SendMouseInput` / CDP keyboard+IME / `MoveFocus`, finding 5) is the **compositing-side
  complement** to that, *not* a duplicate: the live pelt tile uses this plan's API-forwarding route,
  distinct from the serval-hit-relay route tearout-composability owns.

---

## The problem

A context menu (and any modal chrome overlay) is occluded by embedded content in particular,
sticky states: once a scrying System WebView, a pelt tile backed by one, or its content card is
on screen, it draws over the menu and stays that way for the session. The overlay's clicks are
lost in that region.

## Findings (code-verified 2026-06-19)

1. **Two separable layers.** Everything meerkat *rasterizes* (orrery scene, node cards,
   content-card textures, the chrome) is composited in one sequence in `render.rs`, and the chrome
   composites **last and full-window** (render.rs:1724, placement `[0,0,w,h]`), after the orrery
   scene (1244), pelt tiles (1267), content cards (1307), and the scry texture (1398). So for
   rasterized content the order is already correct: the chrome, with the menu, is on top. A plain
   texture content card does not beat the menu.

2. **Scry bypasses that compositor.** The scrying System WebView is not (only) a texture meerkat
   draws. It is a native WebView2 composition visual, HWND-parented, parked on-screen at the card
   rect and re-positioned every frame (scrying_host.rs:437-442). Windows DWM composites that visual
   **above** meerkat's whole swapchain. Since meerkat renders the entire chrome, including the menu,
   *into* that swapchain, the menu is structurally beneath the scry visual regardless of the
   internal compose order. The `texture_view` composite at render.rs:1398 exists, but the live
   visual sits over it.

3. **The on-top visual is a card-format artifact, not a requirement.** The visual is shown directly
   ("the scrying demo's model") for two card reasons: (a) a card floats, pans, and zooms, so the
   visual is chased every frame via `set_offset`; and (b) DWM culls a visual offset *outside the
   visible window's bounds*, which kills the capture, so a card that can leave the viewport must
   keep its visual live and on-screen. A floating, possibly-off-screen card cannot be a texture
   drawn on demand. A **pelt tile** has none of these constraints: a tile is a fixed pane that does
   not pan, zoom, or leave the viewport.

4. **Capture does not need the visual on the visible window.** WebView2 capture is
   `GraphicsCaptureItem::CreateFromVisual` off the composition *visual*
   (webview2_composition_producer/capture.rs:376), not an on-screen window region. wgpu-scry's own
   `--composition-focus-hwnd-test` proves panes parented to **hidden or 1x1 child HWNDs
   capture/import independently** (demo-win/README.md:111). So the visual can live entirely off
   meerkat's visible window and still feed the render.rs:1398 texture composite. The culling in
   finding 3 is specific to a visual offset out of a *visible* window's bounds, not to an off-window
   host with its own target.

5. **Input is API-forwarded by the host, not routed through OS window focus.** This is the finding
   that retires the "keyboard gap". scrying's WebView2 producer delivers all input through the
   composition controller's own programmatic APIs: mouse via `SendMouseInput`, focus via
   `MoveFocus`, and **keyboard / text / IME via CDP** (`Input.dispatchKeyEvent` / `insertText` /
   `imeSetComposition`); the capability string is literally "keyboard/text uses WebView2 CDP Input on
   the pure visual-hosted path" (input.rs:205-231). meerkat already drives all three
   (scrying_host.rs:319-355: `forward_key` -> `send_keyboard_input`, `focus_tile` -> `move_focus`,
   `forward_mouse` -> `send_mouse_input`). CDP input is a protocol call into the WebContent process;
   it works regardless of HWND visibility or focus (`--cdp-input-test` PASSes "under no-overlay
   composition"). The `--composition-focus-hwnd-test` keyboard timeout was specific to the
   `SendInput` / posted-`WM_*` path (OS focus chain), which scrying documents as a dead route and
   does not use. **Therefore an off-window-hosted live tile keeps full keyboard / IME**: the host
   captures its own winit keys and forwards them by API to whichever tile its focus model selects.
   IME placement is host-owned (`set_ime_cursor_area` from the WebView's reported caret geometry).

## Scope / layering

Host surfaces are a **layered mix split BY NATURE**, not "every surface becomes a document
external-texture element." The four-way split (this plan is the canonical owner of it):

- **(a) serval-rendered content** → a **real DOM subtree** in the shell document (rides a11y,
  find-in-page, selection, true scroll). Owned by
  [unified_document_host_plan](2026-06-17_unified_document_host_plan.md).
- **(b) genuinely-external content** (scrying = a system WebView2 visual) → a **native composition
  visual** on the dedicated off-window host HWND, composited as a texture **z-ordered below the
  chrome**, **NOT** a document external-texture element. **Owned here.**
- **(c) dormant surfaces** → a **snapshot texture** (the suspended-tab model). **The snapshot half is
  owned here.**
- **(d) the orrery gyre scene** (a rendered *field*, not a document) → a **texture is correct**.
  Owned by node-representation / orrery.

Chrome composites **above all**. This plan owns only kind **(b)** plus the **snapshot half of (c)**.
The "both surfaces become texture consumers of render.rs:1398" claim below is scoped **explicitly to
genuinely-external WebView2 content** — it is correct there and is *not* a blanket claim over
serval-rendered DOM (a), which never becomes a host texture.

## The direction

The native-visual-on-meerkat's-window model is the bug. Retire it. Both **genuinely-external
WebView2** surfaces (the snapshot card and the live pelt tile — kinds (b)/(c) above) become **texture
consumers** of the existing render.rs:1398 path, which already composites under the chrome. The
WebView2 composition lives on a **dedicated off-window host HWND** (hidden / clipped), never on
meerkat's visible window, so its visual can never composite over the chrome. This needs no DWM
z-ordering, no transparency contract, no composition-tree present, and no per-frame `set_offset`
chase. The chrome and its overlays win structurally: the swapchain is the only thing on meerkat's
visible window.

- **Static snapshot (the orrery card):** a captured **texture**, composited under the chrome at the
  existing render.rs:1398 path, captured once and frozen (no ongoing pump). This is the compositing
  half of the node-rep decision (orrery card = snapshot of the last visit; the live view moves to
  pelt). The snapshot is a **DORMANCY / MEMORY** decision (you cannot hold N live layout sessions in
  RAM for N previews — the suspended-tab model), **NOT** a per-frame-perf win; the focused/live
  surface stays live the engine way. Owner of that reasoning:
  [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  §4(b) (cross-reference, not restated).
- **Live view (a pelt tile):** the WebView2 visual lives on the off-window host HWND and is captured
  continuously; meerkat composites the live texture at the tile rect under the chrome. Input keeps
  flowing through the already-wired API forwarding (finding 5), so the live tile is fully
  interactive (mouse, keyboard, IME) with its visual off the visible window. A tile resizes like any
  pane (`producer.resize()` + recapture); "off the visible window" is about hosting, not size.

This retires the floating-live-WebView entirely. The only surface that needed the on-top hack was a
live WebView in a floating orrery card, and the model already moves live content to pelt, so the
orrery never hosts a live visual again. It is also cheaper: no per-frame reposition.

## The present-path change

Meerkat keeps presenting a bare wgpu swapchain. The change is on the *producer* side, not the
present side: scrying's `CompositionRoot` stops binding its `DesktopWindowTarget` to meerkat's
visible HWND (scrying_host.rs:524-533) and binds instead to a dedicated off-window host HWND. DWM
composites that host's tree onto the (off-screen) host, never over meerkat's window, while WGC keeps
capturing the visual. Meerkat composites only the imported textures, all under the chrome, and
forwards input by API (finding 5). There is no native input-sink window and no OS-focus dependency.

## Phases

Done-conditions, not dates.

- **P1, snapshot-texture orrery card.** The orrery card composites only the captured texture (no
  live visual in the orrery); capture once and freeze; retire the per-frame visual hosting for an
  orrery-pegged scry. Done when a menu over an orrery card is over it.
- **P2, off-window composition host.** Move scrying's `CompositionRoot` off meerkat's visible HWND
  onto a dedicated off-screen host HWND; everything composites under the chrome via the
  render.rs:1398 texture path. Done when the chrome and its overlays sit above all embedded native
  content with the visual no longer parented to the visible window.
- **P3, pelt live-WebView hosting.** Host the live System WebView as a pelt tile captured from the
  off-window host; per-tile, no offset chase; input forwarded by API (finding 5). Done when a System
  WebView lives in a tile, takes mouse + keyboard + IME, and a menu draws over it.

The earlier "P3 keyboard spike" is dropped. It rested on the false premise that keyboard must flow
through the OS focus chain (`SendInput`); finding 5 shows scrying forwards keyboard / IME by CDP,
host-driven, regardless of HWND focus, and meerkat already does this. There is no keyboard gap to
spike.

## Validation (not unsolved design)

What remains for pelt is confirming two things scrying already built the mechanism for, not closing
a design hole:

- **Off-screen capture liveness.** Confirm WGC keeps producing frames from a visual on an off-screen
  host window continuously. DWM can throttle a minimized / fully hidden window, so the host top-level
  window must be present and merely positioned off-screen (DWM still composites non-minimized windows
  at any position), not minimized or `SW_HIDE`. Watch WebView2 page-visibility throttling too.
- **Multi-tile IME.** scrying proved the host-owned IME bridge single-pane (CJK Pinyin + emoji,
  2026-05-12) but lists multi-pane IME as wired-but-unexercised. With several pelt tiles, validate
  IME routes to the focus-model-selected tile only.

## Open questions

- **Which interactive-node case routes here.** node-representation's P2-interactive names two kinds
  of interactive node body. The **compat-WebView-node** (a live System WebView as a node) is
  genuinely-external content that rides **this plan's API-forwarding path** (finding 5), distinct
  from a serval-rendered textured DOM body, which needs tearout-composability's serval-hit-relay
  external-texture-*input* bridge (spine line 139). See
  [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md) P2.
- macOS / Linux: this is Windows/DWM-specific (WebView2). The other targets' embedded-surface story
  is out of scope here. (scrying already maps these: macOS = WKWebView + ScreenCaptureKit ->
  IOSurface -> Metal, input via NSResponder / NSTextInputClient; Linux = WPE pre-composition DMABUF +
  forwarded `wpe_view_event`. Same shape: capture to texture, forward input by API.)

## Progress

- 2026-06-19: Plan created from a debugging session. The symptom (a context menu occluded by scry
  System WebViews, pelt tiles, and content cards in sticky states) traced to scry's native-visual
  hosting (scrying_host.rs:437-442) sitting above meerkat's swapchain via DWM, distinct from the
  rasterized compose order (render.rs:1724, chrome-last), which is already correct for textures.
  The in-document menu-over-node-cards z-order was fixed the same session in the shell document
  (document order, chrome last). Direction set with Mark: snapshot-texture orrery + composition-tree
  pelt hosting, retiring the floating live WebView. No code yet.
- 2026-06-19 (revision): Direction refined with Mark. Cards-in-graph constraints are not pelt's, so
  the DWM composition-tree present is unnecessary. Verified in code that capture is `CreateFromVisual`
  off the visual (not the visible window) and that wgpu-scry's `--composition-focus-hwnd-test`
  already proves hidden / 1x1-child-HWND capture works. New direction: move scrying's composition to
  a dedicated off-window host HWND so both surfaces are texture consumers under the chrome; no DWM
  z-ordering.
- 2026-06-19 (revision 2): Read wgpu-scry's embedding experimentation (platform_ceilings,
  windows_webview2_target, the phase-4c.4 input MVP + SPI eval, the demo smokes, parity matrix) plus
  the producer source, with an adversarial verify pass. This retires the "keyboard gap" entirely (it
  was a wrong premise of revision 1's P3 spike): scrying delivers keyboard / text / IME by **CDP**
  (`Input.dispatchKeyEvent` / `insertText` / `imeSetComposition`), host-driven, independent of HWND
  focus, and meerkat already forwards it (scrying_host.rs:319-355). The `--composition-focus-hwnd`
  keyboard timeout was specific to the `SendInput` / posted-`WM_*` route, which scrying documents as
  a dead route and does not use. The general answer wgpu-scry already reached for "embed a system
  WebView in a wgpu context without native chrome": capture the composition visual to a shared GPU
  texture, composite it as a layer in the host scene under the chrome, and forward all input by the
  WebView's own APIs (SendMouseInput / CDP keyboard+IME / MoveFocus) from the host's focus model.
  Added finding 5, dropped the P3 spike, reframed the residual as two validation items (off-screen
  capture liveness, multi-tile IME).
- 2026-06-19 (cross-plan consolidation): reconciliation pass across the orrery/host plan cluster
  (this plan = the canonical owner of the **compositing-layering** concept). Edits this pass:
  (1) the **DOC_README index entry (line 56) was found stale** — it described the superseded
  revision-1 direction (a live WebView2 visual in meerkat's own composition tree below the chrome) —
  and was corrected to the revision-2 **off-window-host-HWND + texture-consumer** direction, adding
  finding 5 (CDP keyboard/IME retires the keyboard gap; the P3 spike is dropped). This is the
  same-session DOC_README update required by DOC_POLICY rule 7 (a correction of the existing entry,
  no doc added/moved). (2) Added an explicit **Scope / layering** section stating the four-way
  by-nature split (serval DOM subtree / genuinely-external WebView2 native visual / dormant snapshot /
  orrery gyre texture) and scoping the "texture consumers of render.rs:1398" claim to
  genuinely-external WebView2 content only — never serval-rendered DOM. (3) Sharpened the
  static-snapshot bullet to a **dormancy/memory** justification (not per-frame perf), cross-referenced
  to the parallelism research doc §4(b) as the owner (bake-vs-live, per-lane ceilings); did not
  restate it. (4) Extended the unified-document-host sibling bullet to flag that this layering
  **supersedes** its blanket external-texture-migration recipe for the genuinely-external scry case
  (the orrery gyre *scene* as `<external-texture>` is retained — a texture is correct there).
  (5) Added sibling cross-refs to the interaction-model spine (this plan realizes Render-stage Lane C)
  and to tearout-composability / window-composition: the spine (line 139) owns the
  external-texture-**input** bridge; this plan's API-forwarded input on the off-window host is the
  **compositing-side complement**, not a duplicate. (6) Added an Open-questions entry routing
  node-representation's compat-WebView-node (P2-interactive) to this plan's API-forwarding path,
  distinct from the serval-hit-relay textured-DOM-body case. No prior progress entry edited; no doc
  added or moved.
