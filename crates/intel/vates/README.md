# vates

A local-model inference seam. One `InferenceProvider` trait (streaming-first),
capability descriptors so a caller matches a model to a runtime, and a
deterministic dependency-light `CannedProvider` so the whole pipeline tests with
no GPU and no model. Model-backed backends (an own burn-wgpu decoder first, then
external llama.cpp/Ollama endpoints, then mistral.rs) land behind features,
selected by capability, never bound as universal.

```rust
use vates::{CannedProvider, GenerationRequest, InferenceProvider};

let p = CannedProvider::new().with_response("ping", "pong");
let out = p.generate(&GenerationRequest { prompt: "ping".into(), ..Default::default() })?;
assert_eq!(out, "pong");
```

Promoted from mere's `intel/infer`. This founding cut is the seam plus the
canned stub; the decoder, endpoint, and actor backends are the porting roadmap
in [`design_docs/`](design_docs/). Latin *vates*, the poet-prophet: it both
voices (speaks as characters) and foretells (inference).

License: dual MIT OR Apache-2.0, at your option.
