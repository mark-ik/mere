use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::Instant;

use esp::infer::decoder::DecoderProvider;
use esp::infer::{GenerationRequest, InferenceProvider};
use serde_json::json;

fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let model_dir = arguments.next().map(PathBuf::from).ok_or_else(usage)?;
    let prompt = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?;
    let max_tokens = arguments
        .next()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| usage())?
                .parse::<usize>()
                .map_err(|_| usage())
        })
        .transpose()?
        .unwrap_or(16);
    if arguments.next().is_some() || max_tokens == 0 {
        return Err(usage());
    }

    let config = std::fs::read(model_dir.join("config.json"))
        .map_err(|error| format!("read config.json: {error}"))?;
    let tokenizer = std::fs::read(model_dir.join("tokenizer.json"))
        .map_err(|error| format!("read tokenizer.json: {error}"))?;
    let weights = std::fs::read(model_dir.join("model.safetensors"))
        .map_err(|error| format!("read model.safetensors: {error}"))?;

    let load_started = Instant::now();
    let provider = DecoderProvider::from_bytes(
        &config,
        &tokenizer,
        &weights,
        "HuggingFaceTB/SmolLM2-135M-Instruct",
        "burn-ndarray",
        &burn::tensor::Device::ndarray(),
    )
    .map_err(|error| format!("load decoder: {error}"))?;
    let load_ms = load_started.elapsed().as_secs_f64() * 1_000.0;

    let request = GenerationRequest {
        prompt: prompt.clone(),
        max_tokens,
        temperature: 0.0,
        top_p: None,
        seed: None,
        stop: Vec::new(),
    };
    let generation_started = Instant::now();
    let mut token_times_ms = Vec::new();
    let mut fragments = Vec::new();
    let generation = provider
        .generate_streaming_observed(
            &request,
            &mut |fragment| {
                fragments.push(fragment.to_string());
                ControlFlow::Continue(())
            },
            &mut |_| token_times_ms.push(generation_started.elapsed().as_secs_f64() * 1_000.0),
        )
        .map_err(|error| format!("generate: {error}"))?;
    let total_ms = generation_started.elapsed().as_secs_f64() * 1_000.0;
    let first_token_ms = token_times_ms.first().copied();
    let steady_tokens_per_second = first_token_ms.and_then(|first| {
        let remaining_ms = total_ms - first;
        (generation.token_ids.len() > 1 && remaining_ms > 0.0)
            .then(|| (generation.token_ids.len() - 1) as f64 * 1_000.0 / remaining_ms)
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema": "distillery.native-decoder-fixture/v1",
            "model_dir": model_dir,
            "model_id": provider.capability().model_id,
            "prompt": prompt,
            "max_tokens": max_tokens,
            "generated_text": generation.text,
            "generated_token_ids": generation.token_ids,
            "emitted_fragments": generation.emitted_fragments,
            "fragments": fragments,
            "stopped_by_callback": generation.stopped_by_callback,
            "timings": {
                "load_ms": load_ms,
                "first_token_ms": first_token_ms,
                "total_generation_ms": total_ms,
                "steady_tokens_per_second": steady_tokens_per_second,
                "token_completion_ms": token_times_ms,
            }
        }))
        .map_err(|error| format!("serialize receipt: {error}"))?
    );
    Ok(())
}

fn usage() -> String {
    "usage: distillery-decoder-native-fixture <model-dir> <prompt> [max-tokens]".into()
}
