# register-renderer-types

The wasm-friendly data-types half of the renderer registry contract. Split
from [`register-renderer`](../register-renderer/) so
wasm32-clean consumers (notably [`host-runtime`](../host-runtime/))
can consume the input event vocabulary, scene contract, and renderer-identity
types without pulling vello / wgpu transitively.

## What's here

- **`InputEvent`** + supporting (`PointerEventKind` / `PointerButton` /
  `ModifiersState` / `KeyEventKind` / `KeyCode` / `NamedKey` / `ImeEvent`) —
  the renderer-facing input vocabulary the substrate produces and renderers
  consume. Substrate-side OS-event-source lives in `host-runtime` and
  produces these.
- **`SceneNodeRef`** + `NodeContentKind` / `NodeContentKindSet` /
  `NodeIdentity` / `Placement` / `LodLevel` — the scene contract renderers
  see each frame.
- **`CompositionMode`** — the three composition mode tags every renderer
  declares.
- **`RendererId`**, `ProfileBindingExpectation`, `RendererCapabilities`,
  `ScreenRect` — renderer-identity / capability metadata. Pure data.
- **`ProducerHandle`**, `OverlayHandle` — opaque handle types for
  embedded-frame / overlay renderer lifecycles.

## What's NOT here

The trait surface (`NodeRenderer`, `InScenePaintRenderer`,
`EmbeddedFrameRenderer`, `OverlayRenderer`), the registry (`RendererRegistry`,
`RendererSelector`, `DefaultSelector`), and the paint context (`PaintCtx`)
all live in [`register-renderer`](../register-renderer/) — they
require vello / wgpu and so are not wasm-clean.

## Dep posture

Only `kurbo` (used for `Point`, `Vec2`, `Affine`, `Size` in scene + input
contracts). `kurbo` is wasm-friendly.
