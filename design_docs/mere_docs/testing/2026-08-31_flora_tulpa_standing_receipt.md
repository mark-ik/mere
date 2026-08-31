# FLORA, Tulpa, and Standing integration receipt

**Status (2026-08-31):** Passed in a detached verification checkout of
`codex/0831-integration` through `0738b709`, with the final compile fixes
mirrored before the run.

## Contract under test

Gemot retains signed social facts and exact `proofs::BlobRef` artifact
references. Distillery performs the tensor work. The exact FLoRA construction
follows Wang et al.'s
[*FLoRA: Federated Fine-Tuning Large Language Models with Heterogeneous
Low-Rank Adaptations*](https://arxiv.org/abs/2409.05976) and its
[reference implementation](https://github.com/ziyaow1010/FederatedLLM): A
factors concatenate vertically, B factors concatenate horizontally, and each
participant's exact rational round weight and `alpha / rank` scale are applied
once to B. Heterogeneous ranks sum to the declared global rank. Rank-budget
overflow is an error rather than implicit compression.

## Integrated scenario

`ports/distillery/tests/flora_social_receipt.rs` exercises the whole owned
chain:

1. Three identities sign Standing, FLORA, and Tulpa operations.
2. The operations replicate into peer redb stores and converge.
3. Factor and manifest bytes must match the exact BlobRefs carried by FLORA.
4. Distillery receives contributions in reversed arrival order, sorts by the
   governed contribution id, and writes the same exact aggregate bytes and
   receipt.
5. The output manifest is published as the FLORA candidate and adopted as the
   Tulpa version under the proposal's frozen unanimous electorate.
6. A fulfilled Standing commitment is recorded.
7. All three stores close and reopen. The same FLORA round, effective Tulpa
   version, retained facts, and Standing score project after restart.

The tensor assertions independently reconstruct every weighted participant
delta and compare it with the aggregate A/B product. Ordinary ESP loading also
reads the produced adapter with aggregate alpha equal to aggregate rank, so the
loader applies a scale of one.

## Measured gates

All Cargo commands used an isolated target directory, an isolated Cargo home,
offline resolution, and `-j 1`.

| Command | Result |
|---|---:|
| `cargo test -p distillery --features flora --test flora_social_receipt -j 1` | 1 passed |
| `cargo test -p distillery --features flora --lib -j 1` | 11 passed |
| `cargo test -p gemot --lib -j 1` | 122 passed |
| `cargo clippy -p distillery --features flora --lib --test flora_social_receipt --no-deps -j 1 -- -D warnings` | passed |
| `cargo clippy -p gemot --lib --tests --no-deps -j 1 -- -D warnings` | passed |

The ordinary strict Distillery Clippy command reached unchanged dependency
warnings in Personae before package linting. The `--no-deps` gate proves the
changed Distillery package itself under `-D warnings` without relabelling those
upstream warnings.

## Workspace entry blocker

The integration branch's root manifest pins `genet-taffy = =0.13.1` at Genet
revision `da8762fd`, whose checkout provides `0.0.1` and `0.14.0`. Cargo stops
at patch resolution before compiling Mere. The verification checkout omitted
only that inherited patch line; its generated lockfile and scratch manifest
change did not enter the integration branch. This receipt therefore measures
the changed packages and their integrated scenario, not a green root-workspace
gate.

## Claim boundary

This proves the current social contract, exact F32 bias-free q/k/v/o adapter
subset, deterministic artifact bytes, and durable replay. It does not measure
production training quality, cross-device floating-point identity, malicious
tensor inspection, differential privacy, secure aggregation, or corpus
confidentiality. Adapter artifacts can leak training information and remain
subject to explicit audience and release policy.
