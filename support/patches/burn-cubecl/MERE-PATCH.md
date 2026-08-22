# Mere patch provenance

This directory is the published `burn-cubecl 0.22.0-pre.2` crate from Burn
commit `89bcc85f75c55e3451442f5371de45b243865340`.

Mere adds one BrowserWebGpu correctness guard to the numeric, integer, and
float binary launchers. When both logical operands reference the same CubeCL
allocation and view, the published launcher binds that allocation twice as two
kernel inputs. Headed Chromium executes the kernel without a validation error
but returns stale or unmodified storage. Burn LayerNorm reaches this path at
`centered.clone() * centered` and therefore returns its input unchanged.

The patch exposes logical-allocation identity through Mere's existing
`cubecl-runtime 0.11.0-pre.2` patch. A same-allocation binary launch now binds
the allocation once, aliases the second tensor argument to input zero, and
writes to a distinct output. Independent allocations retain Burn's existing
in-place selection.

The headed reproducer is
`ports/distillery/probe/repros/burn_browser_embedding`. Its exact Burn unit
LayerNorm and `8 x 384` BERT-width cases fail on the published row and pass with
this patch, with an empty WebGPU error list. Remove both identity and launcher
changes when a released Burn/CubeCL row passes the same cases unpatched.
