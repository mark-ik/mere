# Preference adapter experiment plan

**Status (2026-09-03):** in progress. Mark ruled D1–D3 on 2026-09-03: the
local SmolLM2-135M; his prompts as the primary corpus, the workspace voice
as the contrast, his wider writing (poetry, OneDrive or iCloud documents)
held back as a later pointer only if the prompts prove too thin; sixty blind
pairs. Follow-on to the
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
- **The corpora, as extracted 2026-09-03** (local, `mere/models/corpora/`,
  gitignored). *Mark's prompts*: 3,138 user turns in 561 transcripts became
  1,761 pairs after dropping system-injected text (1,621), pasted material
  over 220 words or with code fences (629), exact repeats (659), and anything
  secret-shaped (0); each pair is the last 600 characters of the assistant
  turn it answered and Mark's reply, median 11 words, about 48k words in all.
  That is small for style learning, and it is also mostly imperative
  instruction rather than prose, so what an adapter can learn from it is how
  Mark asks, not how he writes at length. *Workspace voice*: 2,870 body
  paragraphs of 40–220 words from non-archived design docs (headings, lists,
  tables, and code excluded), 2,500 kept, about 187k words, each paired with
  the paragraph before it. The contrast arm is subsampled to the prompt
  corpus's token budget so the comparison is about voice, not volume.
- **Held-out loss on the tiny fixture is a noisy target.** (Phase 1,
  2026-09-03.) The fixture's near-uniform untrained weights make raw
  cross-entropy a far noisier objective than the ranking tally the earlier
  receipts use; a `v_proj`-only adapter at the receipts' learning rate
  plateaued within noise of the baseline, and a reproducible held-out
  decrease needed `q_proj` added and the learning rate an order of magnitude
  lower (0.02), where every combination of 200–400 steps and 0.015–0.025
  passed. The fixture proves the objective's mechanics; it says nothing about
  a real model, which is Phase 2's job.
- **Full-sequence logits set the batch size on a real model.** The objective
  materializes `[batch, seq, vocab]` logits in f32; at SmolLM2's 49,152-token
  vocabulary and sequence 256 that is 50 MB per case, so the harness trains
  in mini-batches and measures held-out loss in chunks rather than one
  full-batch step. Phase 1 left the Adam loop full-batch with the resampling
  point named; Phase 2 adds the loop.
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
  voice for contrast, then narrowed it: no document combing now, the prompts
  are enough to start, and poetry or his OneDrive/iCloud documents are a
  pointer he can give later if the prompts prove too thin. So the experiment
  runs two corpora against one base: **E1, Mark's prompts**, his user turns
  from the local Claude Code transcripts (about 3,100 turns across 561
  transcripts as of 2026-09-03 before filtering; pasted handoffs, tool
  output, and anything secret-shaped filtered out), each paired with the tail
  of the assistant turn it answered so the adapter learns how Mark replies;
  **E2, the workspace voice**, design-doc paragraphs paired with the
  paragraph before them. The house-style rule checks stay as an evaluation
  axis on every arm rather than as a training corpus. Every corpus stays
  local beside the model directory and never enters the repository.
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
  and ruled: SmolLM2-135M first; E1 his prompts, E2 the workspace voice, his
  wider writing deferred; sixty blind pairs. Phase 1 started the same day;
  the prompt corpus is being extracted locally alongside it.
- **2026-09-03:** Phase 1 landed in esp: `SequenceCase` (prompt, response;
  tokenized prompt, response, EOS from `config.json`); right-padding with a
  loss mask under the causal decoder; `train_peft_lora_autodiff_sequences`
  sharing init, the Adam loop, and the serializer with the single-token
  trainer through one `run_adam_loop`; `sequence_loss` for held-out
  measurement on any loaded `DecoderModel` (`DecoderProvider::model()` added
  as the smallest accessor). Tests: padded batch equals the token-weighted
  mean of unpadded cases; a one-token response reproduces the single-token
  objective once the EOS target is accounted for; gradient check on the
  sequence objective; held-out strict decrease on a synthetic rule on CPU
  and on the discrete GPU; the adapter loads through the unchanged loader.
  esp 129 lib tests plus receipts green with `decoder-autodiff,decoder-wgpu`,
  the `decoder-lora` suite unchanged, strict Clippy and fmt clean.
