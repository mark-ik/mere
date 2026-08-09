# vates

This package is a compatibility shim. Its implementation moved to `esp::infer`
in `esp` 0.1.0.

Existing imports continue to compile, including the deprecated
`CannedProvider` alias. New code should use:

```rust
use esp::infer::{GenerationRequest, InferenceProvider, StubInferenceProvider};
```
