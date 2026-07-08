# sibylla

A local-embedding and semantic-retrieval seam. One `EmbeddingProvider` trait
that turns text into fixed-dimension vectors, a `SimilarityMetric` each provider
declares for its output space, and a deterministic `StubEmbeddingProvider` so the
embed-to-index-to-search pipeline tests with no GPU and no model. Retrieval and
model-backed pieces (a pure-Rust vector index, a search facade, a burn-free
lexical embedder, then a burn-wgpu BERT backend) land behind features, ported
from mere's `intel/embed`.

```rust
use sibylla::{EmbeddingProvider, StubEmbeddingProvider};

let p = StubEmbeddingProvider::new(384)?;
let v = p.embed_one("the quick brown fox")?;
assert_eq!(v.len(), 384);
```

Promoted from mere's `intel/embed`. This founding cut is the seam plus the
deterministic stub; the vector index, search facade, lexical embedder, and BERT
backend are the porting roadmap in [`design_docs/`](design_docs/). Sibling to
[vates](https://github.com/mark-ik/vates) (generation): where vates voices and
foretells, sibylla is the consulted corpus — it embeds and returns what is asked
for.

License: MPL-2.0 (see the proposal's licensing note; the final choice is open).
