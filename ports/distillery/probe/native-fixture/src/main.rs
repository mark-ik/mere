use std::path::PathBuf;
use std::time::Instant;

use esp::embed::bert::load_cpu;
use serde_json::json;

fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let model_dir = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: distillery-native-fixture <model-dir> <input>".to_string())?;
    let input = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "usage: distillery-native-fixture <model-dir> <input>".to_string())?;
    if arguments.next().is_some() {
        return Err("usage: distillery-native-fixture <model-dir> <input>".into());
    }

    let load_started = Instant::now();
    let provider = load_cpu(&model_dir).map_err(|error| format!("load provider: {error:?}"))?;
    let load_ms = load_started.elapsed().as_secs_f64() * 1_000.0;
    let execution_started = Instant::now();
    let mut outputs = provider
        .embed(&[input.as_str()])
        .map_err(|error| format!("embed: {error:?}"))?;
    let execution_ms = execution_started.elapsed().as_secs_f64() * 1_000.0;
    let output = outputs
        .pop()
        .ok_or_else(|| "provider returned no output".to_string())?;
    let l2_norm = output.iter().map(|value| value * value).sum::<f32>().sqrt();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema": "distillery.native-bert-fixture/v1",
            "model_dir": model_dir,
            "input": input,
            "dimensions": output.len(),
            "all_finite": output.iter().all(|value| value.is_finite()),
            "l2_norm": l2_norm,
            "first_8": output.iter().take(8).copied().collect::<Vec<_>>(),
            "timings": {
                "load_ms": load_ms,
                "execution_ms": execution_ms
            }
        }))
        .map_err(|error| format!("serialize receipt: {error}"))?
    );
    Ok(())
}
