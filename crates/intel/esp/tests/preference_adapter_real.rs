// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The preference adapter experiment's harness (Phase 2 of
//! `design_docs/mere_docs/implementation_strategy/2026-09-03_preference_adapter_experiment_plan.md`).
//!
//! `#[ignore]`d, env-gated, the `tinyllama_real.rs` pattern: it trains a real
//! PEFT LoRA adapter over `HuggingFaceTB/SmolLM2-135M-Instruct` on a local
//! corpus, measures what changed, and writes a dated sidecar plus a blind
//! scoring sheet for Mark's own judgement (Phase 3). Nothing it reads from a
//! corpus is written anywhere but `ESP_PREFERENCE_OUT`; the corpus itself
//! stays wherever it already lives, outside this repository.
//!
//! ```bash
//! ESP_PREFERENCE_MODEL_DIR=C:/.../smollm2-135m-instruct-contradiction-lora \
//! ESP_PREFERENCE_CORPUS=C:/.../corpora/mark-prompts.jsonl \
//! ESP_PREFERENCE_OUT=C:/.../corpora/runs \
//! ESP_PREFERENCE_DEVICE=cpu \
//! ESP_PREFERENCE_TEMPLATE=reply \
//!     cargo test -p esp --features decoder-autodiff --release \
//!     --test preference_adapter_real -- --ignored --nocapture --test-threads=1
//! ```
//!
//! ## Environment
//!
//! Required: `ESP_PREFERENCE_MODEL_DIR` (the artifact triple's directory),
//! `ESP_PREFERENCE_CORPUS` (a JSONL file of `{"prompt": ..., "response": ...}`
//! pairs), `ESP_PREFERENCE_OUT` (a run directory; this harness creates a
//! timestamped subdirectory under it named after the corpus file's stem),
//! `ESP_PREFERENCE_DEVICE` (`cpu` or `gpu`; `gpu` needs the `decoder-wgpu`
//! feature compiled in), `ESP_PREFERENCE_TEMPLATE` (`reply` or `continue`,
//! see [`Template`]).
//!
//! Optional: `ESP_PREFERENCE_TOKEN_BUDGET` (subsample the training split to
//! at most this many supervised tokens — response tokens plus one EOS per
//! case, the trainer's own accounting — deterministic by the corpus shuffle,
//! so two corpora can be trained at equal volume); `ESP_PREFERENCE_STEPS` /
//! `_BATCH` / `_LR` / `_RANK` / `_MODULES` (comma-separated) override the
//! documented defaults below. Not overridable from the environment: alpha
//! (16), `max_sequence_tokens` (256), the corpus-shuffle seed (7), and Adam's
//! beta/epsilon/weight-decay (0.9 / 0.999 / 1e-8 / 0) — these are the
//! experiment's fixed identity, not knobs for a single run.
//!
//! ## What it does
//!
//! Reads the corpus, shuffles it by seed, and splits 80/10/10 into train,
//! held-out, and blind (the blind split is capped at 60 prompts; a smaller
//! split reports fewer and says so in `results.json`). Renders every case
//! through the chosen [`Template`]. Measures held-out `sequence_loss` on the
//! base model, trains an adapter with
//! [`esp::infer::decoder::train_peft_lora_autodiff_sequences`] on the chosen
//! device (recording wall time, steps, and — on the GPU, when `nvidia-smi` is
//! on `PATH` — peak memory), loads it through the unchanged
//! `PeftLoraAdapterLoader`, and measures held-out loss again. Generates
//! greedy continuations (96 tokens, EOS-stopping) for the 60 blind prompts
//! and for six fixed neutral prompts unrelated to either corpus (recipe,
//! weather, code, and three more — the leakage check), from base and
//! adapted, and scores every one of them with the mechanical rule checks:
//! em/en-dash count, parenthesized-aside count, mean sentence length, a
//! markdown-header count, whether the last sentence opens with a closing
//! offer, and a word count. Writes `results.json` (settings, split sizes and
//! token counts, losses, timings, memory peak, rule aggregates, every
//! generation with its prompt and both continuations), `blind_sheet.md` (the
//! 60 pairs in random order, labelled A/B, with Mark's two questions), and
//! `blind_key.json` (which letter is adapted, per pair) into the run
//! directory.

#![cfg(feature = "decoder-autodiff")]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use burn::tensor::Device;
use eidetic::{AdapterRuntimeCompat, Hash, ManifestId, ModelAdapterManifest};
use esp::infer::decoder::{
    AutodiffLoraSettings, PeftLoraAdapterLoader, SequenceCase, SequenceTrainingSettings,
    TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF, sequence_loss, train_peft_lora_autodiff_sequences,
};
use esp::infer::{
    AdapterArtifact, AdapterLoader, AdapterSelection, InferenceProvider, ModelSession,
};
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

const MODEL_ID: &str = "HuggingFaceTB/SmolLM2-135M-Instruct";
const BLIND_CAP: usize = 60;
const GENERATION_MAX_TOKENS: usize = 96;
/// The documented, fixed `max_sequence_tokens` every run trains with (not
/// environment-overridable — see the module doc). A real corpus has outlier
/// responses longer than this on their own (Mark's prompts corpus included
/// one 318-token response, discovered by the trainer's own refusal on the
/// first real run); [`filter_trainable`] drops those before they ever reach
/// the trainer, since the product-code contract is to refuse a case whose
/// response alone cannot fit rather than silently mangle it, and the
/// harness's job is to run a real, messy corpus end to end.
const MAX_SEQUENCE_TOKENS: usize = 256;
/// The mechanical leakage check's fixed prompts: ordinary questions with no
/// connection to either corpus, so any dash/parenthetical/header/closing-offer
/// habit that shows up here came from the adapter, not from the topic.
const UNRELATED_PROMPTS: [&str; 6] = [
    "Can you give me a simple recipe for tomato soup?",
    "What's the weather usually like in early autumn?",
    "How do I reverse a linked list in Python?",
    "What's a good way to organize a bookshelf?",
    "Can you explain how photosynthesis works?",
    "What's a quick way to convert Celsius to Fahrenheit?",
];

// ── splitmix64: the corpus shuffle's own RNG, not shared with esp's ────────
// (this file is a separate compilation unit from the crate it tests, so it
// carries its own copy rather than reaching into esp's private module tree;
// no new crate, same well-understood generator esp's sampler and mini-batch
// scheduler use).

struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut rng = SplitMix64::new(seed);
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        items.swap(i, j);
    }
}

// ── Corpus ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct CorpusPair {
    prompt: String,
    response: String,
}

fn read_corpus(path: &Path) -> Vec<CorpusPair> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read corpus {}: {error}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("corpus line is not {{prompt, response}}: {error}"))
        })
        .collect()
}

/// Which prompt/response pair the harness renders. See the module doc for
/// the exact contract each produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Template {
    /// For Mark's prompts: the tail of the assistant turn he was answering,
    /// wrapped so the model is asked to write the user turn (his reply) and
    /// stop — the trainer's EOS rule supplies the "stop" half.
    Reply,
    /// For the workspace-voice corpus: one design-doc paragraph, asked to be
    /// continued by the next.
    Continue,
}

impl Template {
    fn parse(value: &str) -> Self {
        match value {
            "reply" => Template::Reply,
            "continue" => Template::Continue,
            other => panic!("ESP_PREFERENCE_TEMPLATE must be reply or continue, got {other:?}"),
        }
    }

    fn render_prompt(self, prompt: &str) -> String {
        match self {
            Template::Reply => {
                format!("<|im_start|>assistant\n{prompt}<|im_end|>\n<|im_start|>user\n")
            }
            Template::Continue => {
                format!(
                    "<|im_start|>user\nContinue the design note.<|im_end|>\n\
                     <|im_start|>assistant\n{prompt}\n\n"
                )
            }
        }
    }

    fn render_case(self, pair: &CorpusPair) -> SequenceCase {
        SequenceCase {
            prompt: self.render_prompt(&pair.prompt),
            response: pair.response.clone(),
        }
    }
}

// ── Configuration ────────────────────────────────────────────────────────

struct Config {
    model_dir: PathBuf,
    corpus_path: PathBuf,
    out_dir: PathBuf,
    device_spec: String,
    template: Template,
    token_budget: Option<usize>,
    steps: u32,
    batch: u32,
    learning_rate: f64,
    rank: u16,
    modules: Vec<String>,
}

fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn config_from_env() -> Config {
    Config {
        model_dir: PathBuf::from(env_var("ESP_PREFERENCE_MODEL_DIR")),
        corpus_path: PathBuf::from(env_var("ESP_PREFERENCE_CORPUS")),
        out_dir: PathBuf::from(env_var("ESP_PREFERENCE_OUT")),
        device_spec: env_var("ESP_PREFERENCE_DEVICE"),
        template: Template::parse(&env_var("ESP_PREFERENCE_TEMPLATE")),
        token_budget: env_opt("ESP_PREFERENCE_TOKEN_BUDGET")
            .map(|value| value.parse().expect("ESP_PREFERENCE_TOKEN_BUDGET: usize")),
        steps: env_opt("ESP_PREFERENCE_STEPS")
            .map(|value| value.parse().expect("ESP_PREFERENCE_STEPS: u32"))
            .unwrap_or(300),
        batch: env_opt("ESP_PREFERENCE_BATCH")
            .map(|value| value.parse().expect("ESP_PREFERENCE_BATCH: u32"))
            .unwrap_or(8),
        learning_rate: env_opt("ESP_PREFERENCE_LR")
            .map(|value| value.parse().expect("ESP_PREFERENCE_LR: f64"))
            .unwrap_or(2.0e-4),
        rank: env_opt("ESP_PREFERENCE_RANK")
            .map(|value| value.parse().expect("ESP_PREFERENCE_RANK: u16"))
            .unwrap_or(8),
        modules: env_opt("ESP_PREFERENCE_MODULES")
            .map(|value| value.split(',').map(str::trim).map(String::from).collect())
            .unwrap_or_else(|| vec!["q_proj".into(), "v_proj".into()]),
    }
}

/// The device this run trains and evaluates on, and the loader label that
/// travels with it — a local identifier for this harness's own sessions, not
/// a codebase-wide constant, since nothing outside this file needs to agree
/// with it.
fn resolve_device(spec: &str) -> (Device, &'static str) {
    match spec {
        "cpu" => (Device::ndarray(), "burn-ndarray/preference-adapter"),
        "gpu" => gpu_device(),
        other => panic!("ESP_PREFERENCE_DEVICE must be cpu or gpu, got {other:?}"),
    }
}

#[cfg(feature = "decoder-wgpu")]
fn gpu_device() -> (Device, &'static str) {
    (
        Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0)),
        "burn-wgpu/preference-adapter",
    )
}

#[cfg(not(feature = "decoder-wgpu"))]
fn gpu_device() -> (Device, &'static str) {
    panic!("ESP_PREFERENCE_DEVICE=gpu needs the decoder-wgpu feature compiled in")
}

// ── Token-budget subsampling ─────────────────────────────────────────────

/// The trainer's own supervised-token accounting (response tokens plus one
/// EOS), so a token budget here means exactly what it will mean once
/// training actually counts it.
fn supervised_tokens(tokenizer: &Tokenizer, response: &str) -> usize {
    tokenizer
        .encode(response, false)
        .expect("tokenize response for budget accounting")
        .get_ids()
        .len()
        + 1
}

/// Drop cases the trainer would refuse outright: [`MAX_SEQUENCE_TOKENS`]
/// leaves no prompt budget once a response (plus EOS) alone already exceeds
/// it, and `encode_sequences_for_training` refuses those by name rather than
/// truncating the response. Filtering here, before the trainer ever sees
/// them, is the harness's own responsibility for running a real corpus —
/// the product-code contract is deliberately not to silently mangle a
/// response, only to refuse one that cannot fit.
fn filter_trainable(tokenizer: &Tokenizer, cases: &[CorpusPair]) -> (Vec<CorpusPair>, usize) {
    let mut kept = Vec::with_capacity(cases.len());
    let mut dropped = 0usize;
    for pair in cases {
        if supervised_tokens(tokenizer, &pair.response) > MAX_SEQUENCE_TOKENS {
            dropped += 1;
        } else {
            kept.push(pair.clone());
        }
    }
    (kept, dropped)
}

/// Keep cases from the front of an already-shuffled slice until the next one
/// would push the running supervised-token count past `budget` — deterministic
/// given the corpus shuffle, no extra randomness spent.
fn cap_to_token_budget(
    tokenizer: &Tokenizer,
    cases: &[CorpusPair],
    budget: usize,
) -> (Vec<CorpusPair>, usize) {
    let mut kept = Vec::new();
    let mut total = 0usize;
    for pair in cases {
        let tokens = supervised_tokens(tokenizer, &pair.response);
        if total + tokens > budget {
            break;
        }
        total += tokens;
        kept.push(pair.clone());
    }
    (kept, total)
}

// ── Mechanical rule checks ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct RuleScores {
    em_en_dashes: usize,
    parenthetical_asides: usize,
    mean_sentence_words: f64,
    markdown_headers: usize,
    closing_offer: bool,
    word_count: usize,
}

fn sentences(text: &str) -> Vec<&str> {
    text.split(['.', '!', '?'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

fn score_text(text: &str) -> RuleScores {
    let em_en_dashes = text
        .chars()
        .filter(|&c| c == '\u{2014}' || c == '\u{2013}')
        .count();
    let parenthetical_asides = text.chars().filter(|&c| c == '(').count();
    let sentence_list = sentences(text);
    let mean_sentence_words = if sentence_list.is_empty() {
        0.0
    } else {
        sentence_list
            .iter()
            .map(|s| s.split_whitespace().count())
            .sum::<usize>() as f64
            / sentence_list.len() as f64
    };
    let markdown_headers = text
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();
    const CLOSING_OFFERS: [&str; 4] = ["let me know", "feel free", "i hope", "if you"];
    let closing_offer = sentence_list
        .last()
        .map(|s| {
            let lower = s.to_lowercase();
            CLOSING_OFFERS
                .iter()
                .any(|phrase| lower.starts_with(phrase))
        })
        .unwrap_or(false);
    let word_count = text.split_whitespace().count();
    RuleScores {
        em_en_dashes,
        parenthetical_asides,
        mean_sentence_words,
        markdown_headers,
        closing_offer,
        word_count,
    }
}

#[derive(Debug, Clone, Serialize)]
struct RuleAggregate {
    mean_em_en_dashes: f64,
    mean_parenthetical_asides: f64,
    mean_sentence_words: f64,
    mean_markdown_headers: f64,
    closing_offer_rate: f64,
    mean_word_count: f64,
}

fn aggregate(scores: &[RuleScores]) -> RuleAggregate {
    let n = scores.len().max(1) as f64;
    RuleAggregate {
        mean_em_en_dashes: scores.iter().map(|s| s.em_en_dashes as f64).sum::<f64>() / n,
        mean_parenthetical_asides: scores
            .iter()
            .map(|s| s.parenthetical_asides as f64)
            .sum::<f64>()
            / n,
        mean_sentence_words: scores.iter().map(|s| s.mean_sentence_words).sum::<f64>() / n,
        mean_markdown_headers: scores
            .iter()
            .map(|s| s.markdown_headers as f64)
            .sum::<f64>()
            / n,
        closing_offer_rate: scores.iter().filter(|s| s.closing_offer).count() as f64 / n,
        mean_word_count: scores.iter().map(|s| s.word_count as f64).sum::<f64>() / n,
    }
}

// ── GPU memory sampling ──────────────────────────────────────────────────

/// Peak `nvidia-smi`-reported memory during a closure's execution, sampled
/// every 500 ms on a background thread; `None` when `nvidia-smi` is not on
/// `PATH` (the caller reports "unavailable" in that case, not a zero).
fn with_gpu_memory_peak<T>(run: impl FnOnce() -> T) -> (T, Option<u64>) {
    let probe = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
        .output();
    let available = probe.is_ok();
    if !available {
        return (run(), None);
    }

    let peak: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let peak_writer = peak.clone();
    let stop_reader = stop.clone();
    let sampler = std::thread::spawn(move || {
        while !stop_reader.load(Ordering::Relaxed) {
            if let Ok(output) = Command::new("nvidia-smi")
                .args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
                .output()
                && let Ok(text) = String::from_utf8(output.stdout)
                && let Ok(value) = text.lines().next().unwrap_or("").trim().parse::<u64>()
            {
                let mut guard = peak_writer.lock().expect("peak mutex");
                *guard = Some(guard.map_or(value, |previous| previous.max(value)));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });

    let result = run();
    stop.store(true, Ordering::Relaxed);
    sampler.join().expect("gpu memory sampler thread");
    let peak_value = *peak.lock().expect("peak mutex");
    (result, peak_value)
}

// ── Blind sheet ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct BlindPair {
    prompt: String,
    base: String,
    adapted: String,
    /// `true` when the sheet lists the adapted continuation first (as "A").
    adapted_is_a: bool,
}

fn write_blind_sheet(path: &Path, pairs: &[BlindPair]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "# Preference adapter blind sheet\n")?;
    writeln!(
        file,
        "For each pair: which reads more like your rules, and which reads more like you?\n"
    )?;
    for (index, pair) in pairs.iter().enumerate() {
        let (a, b) = if pair.adapted_is_a {
            (&pair.adapted, &pair.base)
        } else {
            (&pair.base, &pair.adapted)
        };
        writeln!(file, "## Pair {}\n", index + 1)?;
        writeln!(file, "Prompt: {}\n", pair.prompt)?;
        writeln!(file, "A: {a}\n")?;
        writeln!(file, "B: {b}\n")?;
        writeln!(file, "Which reads more like your rules? _____\n")?;
        writeln!(file, "Which reads more like you? _____\n")?;
    }
    Ok(())
}

// ── Results sidecar ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct Generation {
    prompt: String,
    base: String,
    adapted: String,
}

#[derive(Debug, Serialize)]
struct Results {
    model_id: String,
    corpus: String,
    template: String,
    device: String,
    settings: serde_json::Value,
    corpus_size: usize,
    train_size: usize,
    train_dropped_too_long: usize,
    held_out_size: usize,
    blind_size: usize,
    blind_note: Option<String>,
    training_supervised_tokens: usize,
    token_budget: Option<usize>,
    base_held_out_loss: f64,
    adapted_held_out_loss: f64,
    trainer_initial_loss: f64,
    trainer_trained_loss: f64,
    train_wall_time_ms: u128,
    gpu_memory_peak_mib: Option<u64>,
    base_blind_rules: RuleAggregate,
    adapted_blind_rules: RuleAggregate,
    base_unrelated_rules: RuleAggregate,
    adapted_unrelated_rules: RuleAggregate,
    blind_generations: Vec<Generation>,
    unrelated_generations: Vec<Generation>,
}

// ── The harness itself ───────────────────────────────────────────────────

fn generate(provider: &esp::infer::decoder::DecoderProvider, rendered_prompt: &str) -> String {
    let request = esp::infer::GenerationRequest {
        prompt: rendered_prompt.to_string(),
        max_tokens: GENERATION_MAX_TOKENS,
        temperature: 0.0,
        ..Default::default()
    };
    provider.generate(&request).expect("greedy generation")
}

#[test]
#[ignore = "requires ESP_PREFERENCE_MODEL_DIR / _CORPUS / _OUT / _DEVICE / _TEMPLATE (real checkpoint + corpus)"]
fn run_preference_adapter_experiment() {
    let cfg = config_from_env();
    let (device, loader_name) = resolve_device(&cfg.device_spec);

    let config_bytes = std::fs::read(cfg.model_dir.join("config.json")).expect("read config.json");
    let tokenizer_bytes =
        std::fs::read(cfg.model_dir.join("tokenizer.json")).expect("read tokenizer.json");
    let weights_bytes =
        std::fs::read(cfg.model_dir.join("model.safetensors")).expect("read model.safetensors");
    let tokenizer =
        Tokenizer::from_bytes(&tokenizer_bytes).expect("parse tokenizer.json for budget counting");

    // ── Split ──
    let mut corpus = read_corpus(&cfg.corpus_path);
    const SPLIT_SEED: u64 = 7;
    shuffle(&mut corpus, SPLIT_SEED);
    let train_n = corpus.len() * 80 / 100;
    let remaining = corpus.len() - train_n;
    let held_n = remaining / 2;
    let (train_pool, rest) = corpus.split_at(train_n);
    let (held_out_pool, blind_pool) = rest.split_at(held_n);
    let blind_n = blind_pool.len().min(BLIND_CAP);
    let blind_pool = &blind_pool[..blind_n];
    let blind_note = if blind_n < BLIND_CAP {
        Some(format!(
            "blind split has only {blind_n} prompts; the {BLIND_CAP}-prompt cap was not reached"
        ))
    } else {
        None
    };
    println!(
        "split: {} total -> {} train, {} held-out, {} blind{}",
        corpus.len(),
        train_pool.len(),
        held_out_pool.len(),
        blind_n,
        blind_note
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default(),
    );

    let (trainable_pool, train_dropped_too_long) = filter_trainable(&tokenizer, train_pool);
    if train_dropped_too_long > 0 {
        println!(
            "dropped {train_dropped_too_long} training case(s) whose response alone exceeds \
             max_sequence_tokens ({MAX_SEQUENCE_TOKENS}); the trainer refuses these by name \
             rather than truncate the response"
        );
    }
    let (train_pairs, training_supervised_tokens) = match cfg.token_budget {
        Some(budget) => cap_to_token_budget(&tokenizer, &trainable_pool, budget),
        None => {
            let total = trainable_pool
                .iter()
                .map(|pair| supervised_tokens(&tokenizer, &pair.response))
                .sum();
            (trainable_pool.clone(), total)
        }
    };
    println!(
        "training on {} cases, {training_supervised_tokens} supervised tokens{}",
        train_pairs.len(),
        cfg.token_budget
            .map(|b| format!(" (budget {b})"))
            .unwrap_or_default(),
    );

    let train_cases: Vec<SequenceCase> = train_pairs
        .iter()
        .map(|pair| cfg.template.render_case(pair))
        .collect();
    let held_out_cases: Vec<SequenceCase> = held_out_pool
        .iter()
        .map(|pair| cfg.template.render_case(pair))
        .collect();

    // ── Sessions: a loader shared by the base and adapted evaluations ──
    let base_model_ref = ManifestId::of_blob(MODEL_ID.as_bytes());
    let tokenizer_ref = ManifestId::of_blob(&tokenizer_bytes);
    let template_name = match cfg.template {
        Template::Reply => "reply",
        Template::Continue => "continue",
    };
    let prompt_template_hash = Hash::of(format!("preference-adapter/{template_name}").as_bytes());

    let loader = PeftLoraAdapterLoader::new(
        base_model_ref,
        tokenizer_ref,
        &config_bytes,
        &tokenizer_bytes,
        &weights_bytes,
        MODEL_ID,
        loader_name,
        &device,
    );
    let session_for = |adapters: Vec<AdapterSelection>| ModelSession {
        base_model_ref,
        model_id: MODEL_ID.into(),
        tokenizer_ref,
        prompt_template_hash,
        quantization: None,
        loader: loader_name.into(),
        adapters,
    };

    let base_session = loader
        .load_session(session_for(vec![]), &[])
        .expect("base session (no adapters)");

    let base_held_out_loss = sequence_loss(
        base_session.provider().model(),
        &tokenizer_bytes,
        &held_out_cases,
    )
    .expect("base held-out sequence_loss");
    println!("base held-out sequence_loss: {base_held_out_loss:.4}");

    // ── Train ──
    let train_settings = AutodiffLoraSettings {
        rank: cfg.rank,
        alpha: 16.0,
        target_modules: cfg.modules.clone(),
        steps: cfg.steps,
        learning_rate: cfg.learning_rate,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1.0e-8,
        weight_decay: 0.0,
    };
    let mini_batch = SequenceTrainingSettings {
        batch_size: cfg.batch,
        seed: SPLIT_SEED,
        max_sequence_tokens: MAX_SEQUENCE_TOKENS as u32,
    };

    let train_started = Instant::now();
    let (trained, gpu_memory_peak_mib) = with_gpu_memory_peak(|| {
        train_peft_lora_autodiff_sequences(
            &config_bytes,
            &tokenizer_bytes,
            &weights_bytes,
            MODEL_ID,
            &train_cases,
            &train_settings,
            &mini_batch,
            &device,
        )
        .expect("train preference adapter")
    });
    let train_wall_time_ms = train_started.elapsed().as_millis();
    println!(
        "trained {} steps in {train_wall_time_ms} ms: loss {:.4} -> {:.4}; gpu peak {}",
        cfg.steps,
        trained.initial_loss,
        trained.trained_loss,
        gpu_memory_peak_mib
            .map(|v| format!("{v} MiB"))
            .unwrap_or_else(|| "unavailable".into()),
    );

    let adapter_ref = ManifestId::of_blob(&trained.adapter_safetensors);
    let manifest = ModelAdapterManifest {
        name: "preference-adapter".into(),
        base_model_ref,
        adapter_blob: ManifestId::of_blob(&trained.adapter_safetensors),
        adapter_config_blob: ManifestId::of_blob(&trained.adapter_config_json),
        adapter_format: "peft-lora".into(),
        adapter_format_version: TRAINED_ADAPTER_FORMAT_VERSION_AUTODIFF.into(),
        runtime_compat: AdapterRuntimeCompat {
            minimum_capabilities: vec!["peft-lora".into()],
            known_loaders: vec![loader_name.into()],
            converter_lineage: vec![],
        },
        rank: train_settings.rank,
        alpha: train_settings.alpha,
        target_modules: train_settings.target_modules.clone(),
        tokenizer_ref,
        prompt_template_hash,
        quantization_assumption: None,
        training_corpus_root: None,
        training_method: serde_json::json!({
            "trainer": "esp-trainer-v1",
            "objective": "sequence",
            "settings": serde_json::to_value(&train_settings).unwrap(),
            "mini_batch": serde_json::to_value(&mini_batch).unwrap(),
        }),
        eval_results: None,
    };
    let adapted_session = loader
        .load_session(
            session_for(vec![AdapterSelection {
                manifest_ref: adapter_ref,
                scale: 1.0,
            }]),
            &[AdapterArtifact {
                manifest_ref: adapter_ref,
                manifest: &manifest,
                config_bytes: &trained.adapter_config_json,
                weight_bytes: &trained.adapter_safetensors,
            }],
        )
        .expect("adapted session must pass the unchanged loader's checks");

    let adapted_held_out_loss = sequence_loss(
        adapted_session.provider().model(),
        &tokenizer_bytes,
        &held_out_cases,
    )
    .expect("adapted held-out sequence_loss");
    println!("adapted held-out sequence_loss: {adapted_held_out_loss:.4}");

    // ── Blind generation and scoring ──
    let mut blind_pairs = Vec::with_capacity(blind_n);
    let mut sheet_seed = SplitMix64::new(SPLIT_SEED ^ 0xA5A5_A5A5_A5A5_A5A5);
    let mut base_blind_scores = Vec::with_capacity(blind_n);
    let mut adapted_blind_scores = Vec::with_capacity(blind_n);
    let mut blind_generations = Vec::with_capacity(blind_n);
    for pair in blind_pool {
        let rendered = cfg.template.render_prompt(&pair.prompt);
        let base_text = generate(base_session.provider(), &rendered);
        let adapted_text = generate(adapted_session.provider(), &rendered);
        base_blind_scores.push(score_text(&base_text));
        adapted_blind_scores.push(score_text(&adapted_text));
        blind_generations.push(Generation {
            prompt: pair.prompt.clone(),
            base: base_text.clone(),
            adapted: adapted_text.clone(),
        });
        blind_pairs.push(BlindPair {
            prompt: pair.prompt.clone(),
            base: base_text,
            adapted: adapted_text,
            adapted_is_a: sheet_seed.next_u64().is_multiple_of(2),
        });
    }

    // ── Unrelated-prompt leakage check ──
    let mut base_unrelated_scores = Vec::with_capacity(UNRELATED_PROMPTS.len());
    let mut adapted_unrelated_scores = Vec::with_capacity(UNRELATED_PROMPTS.len());
    let mut unrelated_generations = Vec::with_capacity(UNRELATED_PROMPTS.len());
    for prompt in UNRELATED_PROMPTS {
        let rendered = cfg.template.render_prompt(prompt);
        let base_text = generate(base_session.provider(), &rendered);
        let adapted_text = generate(adapted_session.provider(), &rendered);
        base_unrelated_scores.push(score_text(&base_text));
        adapted_unrelated_scores.push(score_text(&adapted_text));
        unrelated_generations.push(Generation {
            prompt: prompt.to_string(),
            base: base_text,
            adapted: adapted_text,
        });
    }

    // ── Write the sidecar, the blind sheet, and the key ──
    let corpus_stem = cfg
        .corpus_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("corpus");
    let epoch_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs();
    let run_dir = cfg.out_dir.join(format!("{corpus_stem}_{epoch_seconds}"));
    std::fs::create_dir_all(&run_dir).expect("create run directory");

    let results = Results {
        model_id: MODEL_ID.into(),
        corpus: cfg.corpus_path.display().to_string(),
        template: template_name.into(),
        device: cfg.device_spec.clone(),
        settings: serde_json::json!({
            "trainer": train_settings,
            "mini_batch": mini_batch,
        }),
        corpus_size: corpus.len(),
        train_size: train_pairs.len(),
        train_dropped_too_long,
        held_out_size: held_out_pool.len(),
        blind_size: blind_n,
        blind_note,
        training_supervised_tokens,
        token_budget: cfg.token_budget,
        base_held_out_loss,
        adapted_held_out_loss,
        trainer_initial_loss: trained.initial_loss,
        trainer_trained_loss: trained.trained_loss,
        train_wall_time_ms,
        gpu_memory_peak_mib,
        base_blind_rules: aggregate(&base_blind_scores),
        adapted_blind_rules: aggregate(&adapted_blind_scores),
        base_unrelated_rules: aggregate(&base_unrelated_scores),
        adapted_unrelated_rules: aggregate(&adapted_unrelated_scores),
        blind_generations,
        unrelated_generations,
    };
    std::fs::write(
        run_dir.join("results.json"),
        serde_json::to_vec_pretty(&results).expect("serialize results.json"),
    )
    .expect("write results.json");

    write_blind_sheet(&run_dir.join("blind_sheet.md"), &blind_pairs).expect("write blind_sheet.md");

    let blind_key: Vec<serde_json::Value> = blind_pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            serde_json::json!({
                "pair": index + 1,
                "adapted_is": if pair.adapted_is_a { "A" } else { "B" },
            })
        })
        .collect();
    std::fs::write(
        run_dir.join("blind_key.json"),
        serde_json::to_vec_pretty(&blind_key).expect("serialize blind_key.json"),
    )
    .expect("write blind_key.json");

    println!("wrote results to {}", run_dir.display());
    println!("held-out loss: base {base_held_out_loss:.4} -> adapted {adapted_held_out_loss:.4}");
}
