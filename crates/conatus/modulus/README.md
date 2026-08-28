# modulus

Renamed from `conatus-brick` and claimed on crates.io 2026-08-28: the
classical architect's *modulus* is the base unit of measure from which a
building's proportions derive, and this crate's slot and pointer layout
is literally modular arithmetic.

The shared sparse-brick presentation ABI: a 3D pointer volume whose zero
value means air and whose other values select dense 8-cubed material slots
in an R8 atlas, plus the camera-neutral WGSL that traverses it.

What this crate owns:

- **`BrickMap`** — deterministic pointer/atlas layout over product-selected
  brick keys. `from_keys` builds a bounded map; `refresh` copies changed
  source bricks into their stable slots; `with_capacity` + `retarget`
  (2026-08-26) fix the extents for the map's whole life so a consumer keying
  texture identity to them never reallocates — retained bricks keep their
  atlas slots, evicted slots recycle deterministically, and a retarget
  names exactly the loaded slots a publisher must move.
- **`BrickTraceSpace`** — the exact uniform fields the shader consumes.
- **`BRICK_DDA_WGSL`** — pointer lookup, ray-box clipping, and voxel DDA
  from a caller-supplied ray. No camera, lighting, material, body, or
  composition policy; the shader stops where product policy begins.
- **`BrickProjectionRevision`** — disposable presentation identity the
  working-set owner advances when selection or slot assignment changes.

What stays with products: working-set selection, source authority and
revision, camera construction, material appearance, lighting, and final
composition. Mesocosm and Paredros each bind their own `Ground` bytes into
this layout and pin this crate by rev; their tracers and receipts live in
their own repos.

Lifted out of `mesocosm-lens` on 2026-08-26 after the second-consumer
proof (engine review R1a in the mesocosm repo); the capacity-fixed
retargeting mode landed the same day for the V1b stable-residency gate.
