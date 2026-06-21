# Native Surface Compositing Plan: overlays above embedded WebView surfaces

**Date**: 2026-06-19
**Status**: **Complete (2026-06-21)** — P1/P2/P3 landed and verified in-app, including multi-tile IME
(multiple live pelt tiles + CJK composing + dropdown over content). The only open item is one flagged
wgpu-scry follow-up (non-blocking capture settle; see the finish-pass Progress entry), an
optimization, not a gap. P2 **verified** (off-window
composition host; capture liveness confirmed in-app
2026-06-21 — a scry tile renders **and scrolls** live off-window on DX12, and the speculative
`CalculateNativeWinOcclusion`-disable proved unnecessary and was dropped). P1 implemented with a
**corrected model** (the snapshot orrery card is a chrome-DOM data-URI `<img>`, **not** an
under-chrome texture — a texture cannot paint over the chrome's own node cards; see finding 6,
Shell z-stack). P3 (pelt live tile) is **done**: renders + scrolls + takes mouse / keyboard input;
its menu-over-tile done-condition holds by the z-stack, and keyboard / IME routing is audit-verified
single-focus-clean. How meerkat's chrome, and its modal overlays especially
(context menu, palette, find, settings, omnibar dropdown), composites above embedded native
surfaces (scrying System WebViews) and the host-composited content layers, so an overlay is never
occluded by content.
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
   visual sits over it. **(P2 has since removed this on-screen parking: the `set_offset` block is
   gone (scrying_host.rs:439 now records its removal) and the composition lives on an off-window
   host, so only the render.rs:1398 texture remains, under the chrome.)**

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
   (scrying_host.rs:288-355: `forward_mouse` -> `send_mouse_input`, `forward_key` ->
   `send_keyboard_input`, `focus_tile` -> `move_focus`). CDP input is a protocol call into the WebContent process;
   it works regardless of HWND visibility or focus (`--cdp-input-test` PASSes "under no-overlay
   composition"). The `--composition-focus-hwnd-test` keyboard timeout was specific to the
   `SendInput` / posted-`WM_*` path (OS focus chain), which scrying documents as a dead route and
   does not use. **Therefore an off-window-hosted live tile keeps full keyboard / IME**: the host
   captures its own winit keys and forwards them by API to whichever tile its focus model selects.
   IME placement is host-owned (`set_ime_cursor_area` from the WebView's reported caret geometry).

6. **The shell z-stack was implicit (a flow-vs-positioned accident), now explicit (2026-06-21).**
   Finding 1's "the chrome is on top" holds for the chrome *texture* vs the content *textures*, but
   it did **not** hold *inside* the one shell document. The orrery node/content cards are
   `position:absolute` + `transform` (each its own stacking context — CSS paint step "positioned"),
   while normal-flow chrome content — the omnibar **suggestions dropdown**, and latently the
   palette / find / settings flex overlays — paints at the earlier "in-flow block" step, so it lands
   **under** the cards regardless of the chrome being document-last. The context menu escaped this
   only because the host sets `position:absolute` on it inline. Serval implements real CSS stacking
   (`serval-layout` `paint_stacking` tests: `negative_z_index_paints_behind_in_flow`,
   `z_index_is_scoped_to_its_stacking_context`), so z-index is the clean lever. Fix:
   `.orrery { z-index:0 }` (the base layer — a stacking context that *contains* its node/focus cards
   so they cannot hoist) and `.chrome { position:relative; z-index:10 }` (the top layer) — so
   **every** chrome surface paints over the orrery cards and wins their hit-test, in normal flow or
   positioned, with no per-element patching. The canonical shell z-stack, bottom→top: orrery scene
   (external-texture underlay) → node cards → focused content card (snapshot/unvisited) → side panes
   (roster, lists, comms) → chrome (toolbar, omnibar dropdown, context menu, palette, find, settings,
   shellbar). Verified: the omnibar dropdown and the context menu both paint over a node card
   (scry-shots/zs-03, zs-04). This is the comprehensive resolution of the in-document z-order that
   unified-document-host started piecemeal (document-order, chrome-last) — the z-stack makes it a
   model, not a per-element accident.

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

- **Static snapshot (the orrery card):** **[model corrected 2026-06-21.]** Originally specified as a
  captured **texture** composited under the chrome at render.rs:1398. That is **wrong for the orrery
  card specifically**: the orrery renders its nodes as chrome **DOM** cards, and a texture under the
  chrome can never paint over them (finding 6). The snapshot card is instead a chrome-DOM **data-URI
  `<img>`** of the page's top peek (the favicon trick), placed after the node cards in the orrery
  element so document order + the z-stack put it over the nodes and under the chrome overlays. Built
  host-side once per url by a blocking GPU readback of the rendered scene → PNG/base64, cached.
  Still captured once and frozen (no ongoing pump); still the compositing half of the node-rep
  decision (orrery card = snapshot of the last visit; the live view moves to pelt). The snapshot is
  a **DORMANCY / MEMORY** decision (you cannot hold N live layout sessions in
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
present side, and is **implemented (P2)**: scrying gained `CompositionRoot::new_offscreen`
(webview2_composition_producer/setup.rs), which creates a private off-screen top-level host window
and binds the `DesktopWindowTarget` there instead of to a consumer HWND. meerkat's
`build_composition_root` (scrying_host.rs:524) now calls it instead of binding to the visible HWND,
and the per-frame `set_offset` chase is gone. DWM composites that host's tree onto the (off-screen)
host, never over meerkat's window, while WGC keeps capturing the visual. Meerkat composites only the
imported textures, all under the chrome, and forwards input by API (finding 5). There is no native
input-sink window and no OS-focus dependency.

## Phases

Done-conditions, not dates.

- **P1, snapshot orrery card. [IMPLEMENTED 2026-06-21.]** The orrery card is a static snapshot of the
  last visit (no live visual in the orrery), retiring the per-frame visual hosting for an
  orrery-pegged scry. **Model corrected** from "captured texture under the chrome" to a chrome-DOM
  data-URI `<img>` (finding 6 + the Static-snapshot bullet): a texture under the chrome cannot paint
  over the orrery's own chrome-DOM node cards. The done-condition (a menu over an orrery card is over
  it) holds by the z-stack — the card sits in the orrery (z-index:0), the menu in the chrome
  (z-index:10).
- **P2, off-window composition host. [VERIFIED 2026-06-21.]** scrying gained
  `CompositionRoot::new_offscreen` (a private off-screen host window owns the `DesktopWindowTarget`);
  meerkat's `build_composition_root` calls it, the per-frame `set_offset` is removed, the threaded
  `origin` dropped, pane offset is (0,0). Everything composites under the chrome via the
  render.rs:1398 texture path. **In-app verified**: a scry tile (`>compat_view` on a focused node)
  renders and **scrolls** live from the off-window host, on DX12, with no on-window visual. The
  speculative `CalculateNativeWinOcclusion`-disable was tested and proved **unnecessary** (the
  off-screen page paints + WGC captures it live without it) and was dropped — the load-bearing pieces
  are the DX12 backend (meerkat main.rs forces `WGPU_BACKEND=dx12`) and the off-window host.
- **P3, pelt live-WebView hosting. [DONE 2026-06-21.]** Host the live System WebView as a pelt
  tile captured from the off-window host; per-tile, no offset chase; input forwarded by API
  (finding 5). Verified: a System WebView lives in a pelt tile, renders, **scrolls**, and takes mouse
  + wheel input. The menu-over-tile half of the done-condition holds by the z-stack (the tile is a
  content texture under the chrome; the menu is in the chrome at z-index:10). Keyboard / IME routing
  is audit-verified clean (a single `scrying_input_focus` slot, no cross-talk); the only residual is
  the manual multi-tile IME exercise in Validation below (Mark-runs-it, not a code gap).

The earlier "P3 keyboard spike" is dropped. It rested on the false premise that keyboard must flow
through the OS focus chain (`SendInput`); finding 5 shows scrying forwards keyboard / IME by CDP,
host-driven, regardless of HWND focus, and meerkat already does this. There is no keyboard gap to
spike.

## Validation (not unsolved design)

What remains for pelt is confirming two things scrying already built the mechanism for, not closing
a design hole:

- **Off-screen capture liveness. [RESOLVED 2026-06-21.]** Confirmed in-app: WGC keeps producing live
  frames from the off-screen host's visual — a scry tile rendered and **scrolled** live (DuckDuckGo,
  responding to wheel input). The host is a present top-level window merely positioned off-screen
  (`SW_SHOWNOACTIVATE`, not minimized / `SW_HIDE`), and WebView2 page-visibility throttling did not
  bite even **without** the `CalculateNativeWinOcclusion`-disable (which was therefore dropped).
- **Multi-tile IME. [RESOLVED 2026-06-21.]** scrying proved the host-owned IME bridge single-pane
  (CJK Pinyin + emoji, 2026-05-12); the meerkat routing is audit-verified clean (a single
  `scrying_input_focus` slot holds the keyboard, so keys / IME go to exactly one tile with no
  cross-talk). Confirmed in-app: multiple live pelt tiles (example.com + iana.org) compositing under
  the chrome at once, with CJK IME (`年后`) composing in the shell and the omnibar dropdown painting
  over content.

## Open questions (resolved by design)

Both are answered, kept for the routing record, not open work:

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
- 2026-06-19 (P2 implemented): off-window composition host built. **scrying:**
  `CompositionRoot::new_offscreen(size)` (webview2_composition_producer/setup.rs) creates a private
  top-level tool window parked off-screen (`SW_SHOWNOACTIVATE`, `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`)
  and binds the `DesktopWindowTarget` there; `new` / `new_offscreen` refactored over a shared `build`;
  `OwnedHostWindow` destroys the window on drop. **meerkat** (scrying_host.rs): `build_composition_root`
  now calls `new_offscreen` (was binding to the visible HWND), the per-frame `set_offset` chase is
  removed, the threaded `origin` dropped, pane offset is (0,0); input wiring (`forward_*` / `focus_tile`)
  unchanged. `cargo check -p scrying` and `-p meerkat` both green (the meerkat build needed a fresh
  `Cargo.lock` regen to clear an unrelated `windows` 0.61/0.62 wgpu-hal skew in the shared lock; not a
  source issue). Remaining: in-app verification (capture liveness + occlusion + input), then P1 and P3.
  Doc synced this pass: Status, finding 2 (parking removed), present-path, P2 phase, finding-5 line
  refs.
- 2026-06-21 (P1+P2 verified, P3 largely done, shell z-stack — the comprehensive layering pass Mark
  asked for). Three threads landed this session:
  (1) **P2 verified in-app.** A scry tile (`>compat_view` on a focused node) renders **and scrolls**
  live from the off-window host, on DX12. The blocker that had made it blank earlier was a backend
  mismatch (meerkat ran Vulkan; the WebView2 shared-texture import is DX12-only) — fixed by forcing
  `WGPU_BACKEND=dx12` in meerkat's `main()`. The speculative `CalculateNativeWinOcclusion`-disable in
  wgpu-scry was tested by reverting it: the page still painted + captured live, so it is **unnecessary
  and was dropped**. Off-screen capture-liveness validation item resolved.
  (2) **P1 model corrected + implemented.** The plan specified the orrery snapshot card as a captured
  texture under the chrome. Implementing it surfaced a hard wall (finding 6): the orrery renders its
  nodes as chrome **DOM** cards, and a texture composited under the chrome can never paint over them
  (a transparent external-texture hole does not erase the opaque node cards behind it). The snapshot
  card is now a chrome-DOM **data-URI `<img>`** of the page's top peek (favicon trick), built once per
  url by a blocking GPU readback → PNG/base64, placed after the node cards in the orrery element.
  Headed-verified over the node cards (scry-shots/du-iana). Committed in meerkat (`a9c66a4`).
  (3) **Shell z-stack made explicit (finding 6).** Tracing the "omnibar dropdown renders under the
  node" bug showed the in-document z-order was an implicit flow-vs-positioned accident: normal-flow
  chrome content painted under the `transform`-stacked node cards. Fixed with two CSS rules —
  `.orrery { z-index:0 }` (base layer) and `.chrome { position:relative; z-index:10 }` (top layer) —
  so every chrome surface (dropdown, menu, palette, find, settings, shellbar) paints over the orrery
  cards by one model, not per-element. Headed-verified the omnibar dropdown and the context menu both
  over a node card (scry-shots/zs-03, zs-04). This is the canonical resolution of the in-document
  z-order unified-document-host started piecemeal. meerkat changes (main.rs `.chrome` rule, window_view
  `.orrery` z-index) pending commit. **wgpu-scry's off-window-host `setup.rs` (load-bearing for the
  committed meerkat scry path) + its `Cargo.toml` remain uncommitted in that repo — Mark to commit +
  push.**
- 2026-06-21 (finish pass: audit + close-out). Commit status from the prior entry is now resolved:
  wgpu-scry's off-window-host (`new_offscreen` + `OwnedHostWindow`) is committed (`efa86ef`), and the
  meerkat z-stack + snapshot + scry-in-pelt are committed (`967a39c`, `a9c66a4`, `d4375d0`). A 6-agent
  read-only audit (parallel sweep + adversarial verify) confirmed P1/P2/P3 done-conditions in code and
  surfaced the residuals; the verified ones were fixed this pass:
  - **Stall-restart UI-freeze (was a blocker).** The capture stall-restart (`scrying_host.rs` drive
    loop) fired after 120 empty polls (~2s at 60Hz); a *legitimately static* off-window page produces
    no WGC frames, so it restarted periodically, and each restart pays `start_capture`'s 500ms settle
    **on the UI thread** (wgpu-scry `capture.rs`). Mitigated meerkat-side: the restart threshold now
    starts at 600 (~10s) and **backs off** (doubling, capped at 4800) after each unproductive restart,
    resetting to 600 on any acquired frame — so a genuine stall still recovers, while a static tile
    quiesces to rare restarts. Deferred (needs demo runtime verification, not blind-edited): the root
    fix is making `start_capture`'s settle non-blocking in wgpu-scry, which removes the residual hitch
    on the now-rare restart.
  - **Snapshot readback panic (was a blocker).** `read_texture_rgba` (`render.rs`) `.expect()`-ed on
    GPU poll/map failure (crash on hang / lost device / OOM); now logs and returns empty, so the
    snapshot silently skips to the stand-in card (the caller already treats `None` as skip).
  - **Orphaned `toggle_live_preview()` (broke test / agent-harness builds).** `agent_harness.rs` still
    called the method d4375d0 removed; rewrote the action as a not-supported stub (live preview is
    retired — content shows as a snapshot card automatically).
  - **Unbounded snapshot cache.** `snapshot_data_uris` grew per-url forever; added a 256-entry cap.
  - Deferred cleanups (flagged, not blocking): cache-flush per-tile submit batching; the silent
    implicit-sync fallback when the explicit D3D12 fence fails (should warn + fail the tile under
    D3D12); the `favicon_data_uri` misnomer (it encodes the snapshot peek too).
  - **Multi-tile IME** routing was code-verified clean: a single `scrying_input_focus` slot holds the
    keyboard, so keys/IME route to one tile with no cross-talk. The remaining validation is the manual
    2-3-tile CJK exercise (Mark-runs-it), not a code gap. Both Open-questions entries are
    answered-by-design. The plan is functionally complete.
- 2026-06-21 (multi-tile IME verified in-app — last residual closed). Headed run showed two live pelt
  tiles (example.com + iana.org) compositing side-by-side under the chrome, the omnibar suggestions
  dropdown painting over the content/orrery (z-stack), and CJK IME (`年后`) composing in the shell.
  Multi-tile IME validation resolved; the plan is **complete**. The only remaining item is the
  optional wgpu-scry non-blocking-capture-settle follow-up (removes the residual rare hitch the
  meerkat stall-restart backoff already made infrequent).
