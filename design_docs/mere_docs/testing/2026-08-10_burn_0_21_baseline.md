# Burn 0.21 Baseline

**Date**: 2026-08-10

**Status**: Captured. This is the "before" the stable 0.22 migration gets diffed
against — B0 items 4 and 5 of the
[Burn 0.22 migration plan](../implementation_strategy/2026-08-09_burn_0_22_migration_plan.md),
taken **now** rather than after the bump, so nothing has to be reconstructed
from memory once APIs have moved.

Everything below was run on 2026-08-10 unless a line says otherwise. Where a
receipt could not be run, it says so and why; an unrun receipt is not a passing
one.

---

## 1. Environment

| | |
|---|---|
| `rustc` | 1.97.1 (8bab26f4f 2026-07-14) |
| Burn | 0.21.0 — **one generation across the whole graph** |
| Host | Windows 11, AMD Radeon 780M (integrated) + NVIDIA RTX 4060 Laptop |

Burn 0.22.0-pre.1 declares Rust 1.95, so the toolchain is not the gate. Release
stability is.

## 2. Dependency boundary

After B1 (below), `cargo metadata` shows **two** direct Burn dependents, both
production, both optional:

| crate | kind | optional | features |
|---|---|---|---|
| `esp` | normal | yes | `ndarray`, `std` |
| `quint` | normal | yes | `ndarray`, `std` |

```
burn v0.21.0
├── esp   → mere-embed
└── quint → mere-embed
```

**The default `esp` tree is `serde` alone** — no Burn, no tokenizers. That is
the floor the migration must not disturb, and it is the cheapest regression to
check after the bump:

```
esp v0.1.0
└── serde v1.0.228
    ├── serde_core v1.0.228
    └── serde_derive v1.0.228 (proc-macro)
```

Feature-graph shape under `esp/bert-wgpu`, for diffing against 0.22's:
`burn/burn-ndarray ← burn/ndarray ← esp/{bert,bert-wgpu,burn,default}` and
`burn/burn-wgpu ← burn/wgpu ← esp/bert-wgpu`.

## 3. Feature/target matrix

Every feature checked individually, as the plan requires — a combined check
hides a feature that only compiles because another one turned something on.

| feature | native | wasm32-unknown-unknown |
|---|---|---|
| `esp` (default) | ok | ok |
| `esp/actor` | ok | not run |
| `esp/index-burn` | ok | ok |
| `esp/index-burn-wgpu` | ok | not run |
| `esp/bert` | ok | ok |
| `esp/bert-wgpu` | ok | not run |
| `esp/bert-validation` | ok | not run |
| `esp/decoder` | ok | not run |
| `esp/decoder-wgpu` | ok | not run |
| `quint` (default) | ok | not run |
| `quint/field-burn` | ok | not run |
| `quint/field-burn-wgpu` | ok | not run |
| `quint/field-rhai` | ok | not run |

The wasm32 column covers the three portable configurations that matter for the
browser lane. The `-wgpu` and `decoder` rows were not run for wasm this pass;
re-running them belongs with the browser-ceiling probe rather than here.

## 4. Test suites

| suite | result |
|---|---|
| `cargo test -p esp` (default) | 62 passed |
| `cargo test -p esp --features bert` | 126 passed, 1 ignored |
| `cargo test -p esp --features index-burn-wgpu` | 69 passed, 1 ignored |
| `cargo test -p quint --features field-burn` | 61 passed |
| `cargo test -p quint --features field-burn-wgpu` | 64 passed, 2 ignored |
| `cargo test -p mere-embed --features bert` | 23 + 5 + 1 passed, 5 ignored |

**Real-hardware WGPU parity: passes.**
`esp::embed::bert::wgpu_parity::bert_sentence_parity_ndarray_wgpu` runs
NdArray against WGPU on the machine above and agrees. This is the receipt most
likely to move under 0.22 and the one worth re-running first.

## 5. What was *not* run, and why

Recorded so the 0.22 comparison does not quietly treat these as regressions
when they were never green here either.

- **Real MiniLM checkpoint tests.** `MERE_MINILM_DIR` is unset on this machine,
  so the five `mere-embed` integration tests and the ESP validation tests that
  want a real all-MiniLM-L6-v2 directory stayed ignored. They are the receipts
  that would catch a numerical change in model output, so **the migration needs
  the model present**; a green run without it proves loading and shape, not
  arithmetic.
- **TinyLlama decoder output and throughput.** Same reason: no checkpoint.
- **Timing sweeps.** `index_burn`'s sweep and the BERT timing test are
  `#[ignore]`d release-mode benchmarks; no numbers are claimed here, so none can
  be claimed to have regressed.
- **wasm32 for the `-wgpu` and `decoder` features.**

## 6. B1: accidental fan-out, removed

The plan listed two test/example-only Burn dependencies. There were **three**
consumers naming a Burn type — the third was `mere-embed`'s own lib doctest,
which compiles under `cargo test` and so held the dependency open just as
firmly as the integration test did.

Replaced with ESP-owned constructors returning the provider seam:

- `esp::embed::bert::load_cpu(dir) -> Box<dyn EmbeddingProvider>`
- `esp::embed::bert::load_wgpu(dir)` (feature `bert-wgpu`)
- `esp::embed::bert::from_bytes_cpu(config, tokenizer, weights)`

plus `impl EmbeddingProvider for Box<dyn EmbeddingProvider>`, without which a
boxed provider cannot satisfy a generic bound like `SemanticSearch::new` — and a
consumer that picks its backend at runtime would still have to name a Burn type.

`from_bytes_cpu` exists because the eidetic round-trip resolves a model from
stored *bytes* rather than a directory. There is deliberately no `_wgpu` twin:
no consumer needs one, and a constructor with no caller is a guess.

`BertEmbeddingProvider<B>` stays public. The constructors take the backend's
**default** device, and a host that must register an *existing* device — one
that already owns a `wgpu` queue and does not want Burn opening a second — needs
the generic API. That is the recorded justification for the remaining generic
surface, not an oversight.

One thing the cleanup removed was live fragility: `mere-eidetic-search`'s
example named `burn::backend::Wgpu` while depending on Burn with only the
`ndarray` feature. It compiled solely because `embed/bert-wgpu` unified
`burn/wgpu` into the graph. That is the exact failure mode B1 exists to prevent,
and it would have been a confusing first error under 0.22.

## 7. Re-run list for stable 0.22

In order, cheapest and most diagnostic first:

1. default `esp` tree is still `serde` alone;
2. the feature/target matrix in §3, individually;
3. the CPU suites in §4;
4. real-hardware WGPU parity;
5. the model-backed receipts in §5 — **with `MERE_MINILM_DIR` set this time**;
6. `cargo package -p esp` from a clean commit.
