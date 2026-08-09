# sibylla

This package is a compatibility shim. Its implementation moved to
`esp::embed` in `esp` 0.1.0.

Existing imports continue to compile:

```rust
use sibylla::{LexicalEmbeddingProvider, SemanticSearch};
```

New code should use:

```rust
use esp::embed::{LexicalEmbeddingProvider, SemanticSearch};
```
