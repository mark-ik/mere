# Preference adapter experiment plan

**Status (2026-09-03):** in progress. Mark ruled D1–D3 on 2026-09-03: the
local SmolLM2-135M; his own writing corpus as the primary target, his prompts
as a second, and the workspace voice as the contrast; sixty blind pairs. The
corpus location is the one open input. Follow-on to the
[autodiff LoRA trainer plan](2026-09-02_autodiff_lora_trainer_plan.md),
whose trainer this experiment puts to a real question.

## The question

Mark's framing: can a LoRA adapter carry preferences as *statistical desire
paths*, worn into the weights by traffic, so they need not be restated in
context every window? The critical read (2026-09-03) said: partly, and only
for the kind of preference that is a regularity rather than a fact. An
adapter bends the conditional distribution toward tone, format, vocabulary,
and habitual readings; it cannot store a citable fact or a revocable rule.
The prompt does those. So the experiment does not ask "can an adapter replace
context"; it asks the narrower thing the metaphor actually claims:

> Given a few hundred examples of one person's house style, does an adapter
> trained by this stack make a small model follow that style on unseen
> prompts, measurably and by that person's own blind judgement, at a cost
> the stack can pay?

A negative answer is a valid outcome and is written up the same way.

## Findings (verified 2026-09-03)

- **A real llama-family checkpoint is already local.** The full
  `HuggingFaceTB/SmolLM2-135M-Instruct` triple sits under
  `mere/models/smollm2-135m-instruct-contradiction-lora/` (`config.json`,
  `tokenizer.json`, 269,060,552-byte BF16 `model.safetensors`, pinned at
  revision `12fd25f7…` by the
  [PEFT LoRA receipt](../testing/2026-08-24_model_session_peft_lora_receipt.md)),
  beside a third-party PEFT adapter the loader was proven against. The
  directory is gitignored (`/models/`) and machine-local. Nothing larger is on
  disk; TinyLlama-1.1B-Chat is a 2.2 GB download and SmolLM2-360M-Instruct
  724 MB, both bf16, both llama-family configs this decoder already parses.
- **The GPU is an 8 GB RTX 4060 Laptop with about 1 GB in use.** SmolLM2-135M
  in f32 is 540 MB of weights; 30 layers, hidden 576, 9 heads over 3 KV heads,
  vocab 49,152, tied embeddings. Training activations at sequence 256 and
  batch 8 are well inside the budget. This is the model to build the pipeline
  on; scaling is a later download.
- **Right-padding needs no attention change.** The decoder's mask is causal
  (`tril_mask` in `attention.rs`), so pads placed after the real tokens are
  never attended by them; positions stay correct. Variable-length batches
  therefore need only a loss mask, which is exactly the follow-on the
  autodiff plan named. Left-padding or an explicit key mask is not needed.
- **The v1 trainer's objective is one token per case.** It supervises the
  last position only. Style lives in whole responses, so the objective has to
  become per-position cross-entropy over a response span, with the prompt
  span excluded from the loss. That is new product code in esp, not harness
  code.
- **The design-doc corpus is not Mark's prose.** About 1.9 million words sit
  in `design_docs/` across the workspace, but they were written by agents
  under Mark's direction; 126 files record rulings made with him and a few
  thousand words in the `CLAUDE.md`, `PROJECT_DESCRIPTION.md`, and `README.md`
  files are his. A style adapter trained on the docs would learn the
  workspace's voice, which is a legitimate target, but it is not the person.
  The distinction has to be chosen, not blurred.
- **Mark's writing rules are mechanically checkable.** The standing rules
  (no em-dashes, no parentheticals, one idea per sentence near twenty words,
  no headers under five hundred words, no closing offers, plain domain
  vocabulary) are the kind of preference an adapter is good at, and most of
  them can be scored by a program on unseen output. That gives the
  experiment an automatic adherence metric alongside the human one, and a
  way to tell "learned the rule" from "memorized the examples".
- **The receipt schema has one metric.** `EvalMetric::RankingAt` is the only
  variant; held-out loss and rule adherence do not fit it. The experiment
  records its results as a dated testing doc with a JSON sidecar, the way the
  PEFT LoRA receipt does, and productizes a metric only if the answer is
  worth productizing.

## Decisions

- **D1, the model.** (recommended) SmolLM2-135M-Instruct, local now, for the
  whole pipeline and the first verdict; then, if the verdict warrants, scale
  to SmolLM2-360M or TinyLlama-1.1B as a separate download with its own
  permission. Alternative: download TinyLlama first and build on it. The
  pipeline's bugs are cheaper to find at 135M, and a 1B result without a 135M
  baseline cannot say whether size mattered.
- **D2, the corpus.** (a, recommended) *House-style pairs*: a few hundred
  prompt/response pairs whose responses obey Mark's writing rules, drafted by
  Claude from the rules and from Mark's own short texts, with Mark reading a
  sample before training; evaluated by the mechanical rule checks and by
  Mark's blind judgement. (b) *Workspace voice*: the design-doc corpus as
  next-token language modelling, evaluated by held-out loss and blind
  judgement; learns the agents' voice. (c) Both, as E1 and E2. The
  recommendation is (a) because it is the only variant where the target is a
  preference rather than a corpus, and the only one with an automatic metric.
- **D2 as ruled.** Mark has a large corpus of his own writing (he is an
  English major) and offered it, together with his prompts and the workspace
  voice for contrast. So the experiment runs three corpora against one base:
  **E1, Mark's prose**, the real desire path; **E2, Mark's prompts**, his
  user turns from the local Claude Code transcripts (about 3,100 turns across
  561 transcripts as of 2026-09-03, pasted material filtered out); **E3, the
  workspace voice**, the design docs. The house-style rule checks stay as an
  evaluation axis on every arm rather than as a training corpus. Every corpus
  stays local beside the model directory and never enters the repository.
- **D3, the human budget.** How many blind pairs Mark will score: twenty,
  forty, or sixty. Each pair is one unseen prompt with the base and adapted
  continuations in random order, asked "which reads more like your rules,
  and which reads more like you". Forty gives a sign test some teeth; twenty
  is a smell test.

## Phase 1: the sequence objective in esp

`train_peft_lora_autodiff` gains a case shape with a prompt span and a
response span, right-padded to the batch's longest, with per-position
cross-entropy over the response tokens only (targets shifted by one, pads and
prompt masked out). The single-token case becomes the degenerate instance
(response of length one), so the existing receipts keep passing.

Done conditions: the padded-batch loss equals the mean of the unpadded
per-case losses on the fixture; the gradient check still holds at the v0
point; the existing v1 receipts are unchanged; strict Clippy and fmt clean.

## Phase 2: corpus and harness

An env-gated, ignored integration test in esp (the `tinyllama_real.rs`
pattern, keyed on a model directory variable) that: loads the SmolLM2 triple;
reads a corpus file of prompt/response pairs split by a fixed seed into
train, held-out, and blind sets; trains with the autodiff arm on CPU and on
the GPU, reporting wall time, steps, and peak GPU memory from the adapter
probe; measures held-out loss base versus adapted; generates continuations
for the blind prompts from both; runs the mechanical rule checks on both;
and writes a JSON sidecar plus a randomized, unlabeled scoring sheet for D3.
The corpus itself lives beside the model directory, not in the repository.

Done conditions: the harness runs end to end on the 135M model on this
machine, on both devices, with the numbers written to the sidecar.

## Phase 3: the verdict

Mark scores the blind sheet. The write-up records held-out loss, the
mechanical adherence rates for base and adapted, Mark's blind preference
with a sign test, wall time and memory on both devices, and a plain
statement of which rules moved, which did not, and what that says about the
desire-path thesis. It also records what the same adapter did to a set of
unrelated prompts, because a style that leaks into everything is a cost, not
a win.

Done conditions: a dated testing doc with the JSON sidecar, indexed, and one
of three verdicts stated: the path forms, the path does not form at this
size, or the experiment could not distinguish the two and why.

## Non-goals

Continual training from live behaviour, any federated round, persona
scoping, a product surface, and any claim about 1B-class models before one
has been run.

## Progress

- **2026-09-03:** assessment complete; findings verified; D1–D3 put to Mark
  and ruled: SmolLM2-135M first; E1 Mark's prose, E2 his prompts, E3 the
  workspace voice; sixty blind pairs. Waiting on the corpus location before
  Phase 2; Phase 1 (the sequence objective) does not depend on it.
