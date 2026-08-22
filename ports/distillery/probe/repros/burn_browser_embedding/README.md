# Burn browser shared-input binary reproducer

This headed harness extracts Distillery's MiniLM BrowserWebGpu failure from the
artifact, tokenizer, storage, and ESP graph. It retains the eleven embedding
controls that already pass and adds a model-free causal ladder:

- the exact ten-value Burn LayerNorm unit input;
- mean and centering controls;
- one uploaded tensor used by both binary operands;
- two independently uploaded tensors with identical values;
- the resulting variance and LayerNorm;
- an `8 x 384` BERT-width LayerNorm checked against host arithmetic.

On the published Burn/CubeCL row, scalar multiplication and independent tensor
operands pass. `tensor.clone() * tensor` fails, and both exact-unit and
BERT-width LayerNorm return their input unchanged. WebGPU validation scopes stay
empty.

Three patch experiments distinguish the cause:

1. tightening CubeCL's handle mutability count does not restore correctness;
2. allocating a separate binary output does not restore correctness;
3. binding the shared allocation once, aliasing the second logical input to
   input zero, and using a separate output passes every graph and embedding
   case.

The third form is carried by Mere's `support/patches/burn-cubecl` backport.
The before/after headed result is recorded in
[the binary-alias receipt](receipts/2026-08-22_binary_alias_iab.json). A native
WGPU test in this crate checks the shared multiply and exact LayerNorm as a
backend control.

From this directory, with wasm-bindgen CLI 0.2.122 installed:

```powershell
.\run-repro.ps1 -WasmBindgen C:\path\to\wasm-bindgen.exe
```

Open the printed URL in headed Chromium and choose **Run graph cases**. For
automation, call `window.burnEmbeddingRepro.run()` and inspect both `result`
and `gpu_errors`.
