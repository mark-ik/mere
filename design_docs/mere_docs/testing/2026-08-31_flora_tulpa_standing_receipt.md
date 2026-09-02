# FLORA, Tulpa, and Standing integration receipt

**Status (2026-09-02):** Passed again on the stack rebased onto `origin/main`
`f2924f08`, together with the upstream Djinn Distillery lane receipts; landed on
`main`. First passed 2026-08-31 on `codex/0831-integration` after merging the
upstream Genet pin correction `77b3c3a2` and Distillery test repair `9a53c77a`.

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
| `cargo check -p mere-canvas -j 1` | passed; 11m51s cold check |

After the Genet pin correction, the integrated receipt command ran again from
the actual integration checkout: 1 passed after 28m23s of cold path-crate
compilation; the test itself completed in 4.77s.

The ordinary strict Distillery Clippy command reached unchanged dependency
warnings in Personae before package linting. The `--no-deps` gate proves the
changed Distillery package itself under `-D warnings` without relabelling those
upstream warnings.

## Rerun after the rebase, 2026-09-02

The stack was replayed onto `origin/main` `4d68c465`, which had gained the
operational Djinn Distillery resident, honest discrete-GPU selection, and the
cross-platform lane receipts since the branch base. All commands ran from the
integration checkout with `-j 2` against the same isolated target directory.

| Command | Result |
|---|---:|
| `cargo test -p distillery --features flora --test flora_social_receipt` | 1 passed |
| `cargo test -p distillery --features flora --lib` | 12 passed |
| `cargo test -p gemot --lib` | 122 passed |
| `cargo test -p muniment` / `-p mere-eidetic` / `-p chartulary` | 37 / 95 / 59 passed |
| `cargo clippy -p distillery --features flora --lib --test flora_social_receipt --no-deps -- -D warnings` | passed |
| `cargo clippy -p distillery --features flora,trainer-gpu --lib --tests --no-deps -- -D warnings` | passed |
| `cargo clippy -p gemot --lib --tests --no-deps -- -D warnings` | passed |
| `cargo test -p djinn --features trainer --test distillery_trainer` | 2 passed |
| `cargo test -p djinn --features trainer --test distillery_lane` | 4 passed |
| `cargo test -p djinn --features trainer-gpu --test distillery_trainer_gpu` | 1 passed, 80 s on the discrete GPU |
| `cargo check -p mere-canvas` | passed |

The Distillery library count rose from 11 to 12 because upstream added a
trainer test alongside the GPU device probe. The three Djinn commands carried
`--config profile.dev.package.mere-canvas.incremental=false`: without it the
test binaries fail to link on windows-msvc with 83 unresolved externals
against `mere-canvas`, the rust-lang/rust#86049 shape recorded in the
projection grammar plan, and cleaning the affected packages does not clear
it. Nothing in the stack touches Canvas; the workaround is the invocation's,
not the tree's.

`main` moved again to `f2924f08` (Canvas derived faces D3) before the push.
Distillery, Gemot, Muniment, Eidetic and Chartulary carry no edge to Canvas
or Graphshell (`cargo tree`), so their results stand; the Canvas check and
the three Djinn receipts were rerun on the tree replayed onto `f2924f08` and
passed again.

## Workspace entry closure

The first receipt found that the root manifest required
`genet-taffy = =0.13.1` while Genet revision `da8762fd` provides the real fork
as `0.14.0`. Upstream commit `77b3c3a2` corrected the exact pin to `=0.14.0`;
that revision's Buckram and Livery manifests also require `0.14.0`. An inverse
dependency query now resolves one chain from `genet-taffy 0.14.0` through
Buckram and `genet-livery` into Mere Canvas, Mere, Graphshell, and Djinn. The
focused Mere Canvas check compiles `genet-taffy 0.14.0`, Buckram,
`genet-livery`, and Canvas from that exact chain. The direct integrated receipt
above also enters Cargo, compiles, and passes without a scratch manifest
change. The detached-checkout exception is retired.

The adjacent upstream commit `9a53c77a` removed duplicate license/header text
that had left Distillery's `authority.rs` and `resident.rs` tests malformed.

## Claim boundary

This proves the current social contract, exact F32 bias-free q/k/v/o adapter
subset, deterministic artifact bytes, and durable replay. It does not measure
production training quality, cross-device floating-point identity, malicious
tensor inspection, differential privacy, secure aggregation, or corpus
confidentiality. Adapter artifacts can leak training information and remain
subject to explicit audience and release policy.
