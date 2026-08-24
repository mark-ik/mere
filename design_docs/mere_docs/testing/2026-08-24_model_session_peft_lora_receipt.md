# ModelSession PEFT LoRA Receipt

**Date**: 2026-08-24

**Source**: clean detached Mere `aa121f03`.

ESP loaded the Apache-2.0
`spellicer/SmolLM2-135M-Instruct-contradiction` PEFT 0.19.1 adapter at pinned
revision `919fa899e7635df123b68eb9c266c0d98757d954` over the exact
`HuggingFaceTB/SmolLM2-135M-Instruct` base at revision
`12fd25f77366fa6b3b4b768ec3050bf629380bac`.

The base config, BF16 weights, tokenizer, prompt template, adapter config,
adapter weights, and typed `ModelAdapterManifest` all survived an Eidetic
save/resolve round trip byte-for-byte. The immutable `ModelSession` bound the
base manifest, tokenizer blob, prompt-template hash, unquantized assumption,
exact loader, and ordered adapter manifest. A session carrying the wrong
template hash was rejected before tensor application.

The native NdArray PEFT loader applied rank-8 LoRA A/B tensors to all 120
q/k/v/o attention projections. An independent CPU safetensors merger built the
same full checkpoint without calling ESP's loader. All 49,152 next-token logits
matched exactly (`0.0` maximum absolute error, tolerance `0.002`), and both
paths emitted the same 12 token ids and text. The adapted logits differed from
the base by `4.1851387`, so this is not a no-op match.

The machine-readable receipt is
[`ports/distillery/probe/receipts/2026-08-24_model_session_peft_lora.json`](../../../ports/distillery/probe/receipts/2026-08-24_model_session_peft_lora.json).

## Source checkpoint finding

The adapter repository also publishes `merged_model.safetensors`, but that file
is not a valid numerical oracle at the pinned revision. Across all 120 target
tensors it differs from the base by at most `0.00025337934`, while the adapter's
expected maximum delta is `0.012088048`. The fixture records
`contains_adapter: false` and uses the independent merge as its oracle.

## Claim boundary

This closes the native `ModelSession` and ordinary PEFT LoRA loader gate for
unquantized llama-family q/k/v/o adapters. Ordered multi-adapter identity is
unit-tested, but a real stacked-adapter numerical row is still consumer-gated.
The receipt does not claim WGPU adapter execution, browser adapters, training,
portable remote checkpoints, or endpoint-provider support.
