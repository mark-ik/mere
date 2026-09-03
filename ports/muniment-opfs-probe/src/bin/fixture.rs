// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Lane 5 portability fixture.
//!
//! `fixture write <dir>` creates `<dir>/portability.redb` through redb 4.2's
//! own native file backend and writes `<dir>/portability.json`, the manifest
//! a browser reopen must reproduce. `fixture verify <file> <manifest>
//! [expected_generation]` opens a file that came back from the browser and
//! checks it. Both print one JSON object and exit non-zero on failure.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    std::process::exit(native::main());
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use muniment_opfs_probe::churn::{self, ChurnShape};
    use muniment_opfs_probe::redb_backend::RedbBackend;
    use muniment_opfs_probe::workload::{self, Workload};
    use redb::Database;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    const GENERATIONS: u64 = 5;

    #[derive(Serialize, Deserialize)]
    struct Manifest {
        schema: String,
        redb: String,
        shape: ChurnShape,
        generation: u64,
        /// Digest of the muniment table after SmallSlots then OrderedLog.
        muniment_digest: String,
        muniment_keys: u64,
        file_len: u64,
    }

    fn clock() -> impl Fn() -> f64 {
        let started = Instant::now();
        move || started.elapsed().as_secs_f64() * 1000.0
    }

    fn write(dir: &Path) -> Result<serde_json::Value, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = dir.join("portability.redb");
        let manifest_path = dir.join("portability.json");
        let _ = std::fs::remove_file(&file);
        let shape = ChurnShape::default();
        let db = Database::builder()
            .set_cache_size(32 << 20)
            .create(&file)
            .map_err(|e| e.to_string())?;
        churn::materialize(&db).map_err(|e| e.to_string())?;
        for generation in 1..=GENERATIONS {
            churn::commit_generation(&db, generation, shape, false).map_err(|e| e.to_string())?;
        }
        let store = RedbBackend::from_database(db).map_err(|e| e.to_string())?;
        let clock = clock();
        for workload in [Workload::SmallSlots, Workload::OrderedLog] {
            let (outcome, _) = pollster::block_on(workload::run(&store, workload, &clock))
                .map_err(|e| e.to_string())?;
            if !outcome.checks_ok {
                return Err(format!(
                    "{workload:?} checks failed while writing the fixture"
                ));
            }
        }
        let (muniment_digest, muniment_keys) =
            pollster::block_on(workload::digest(&store)).map_err(|e| e.to_string())?;
        drop(store);
        let file_len = std::fs::metadata(&file).map_err(|e| e.to_string())?.len();
        let manifest = Manifest {
            schema: "muniment.opfs-probe.fixture/v1".into(),
            redb: "4.2.0".into(),
            shape,
            generation: GENERATIONS,
            muniment_digest,
            muniment_keys,
            file_len,
        };
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": true,
            "file": file,
            "manifest": manifest_path,
            "generation": manifest.generation,
            "muniment_digest": manifest.muniment_digest,
            "file_len": manifest.file_len,
        }))
    }

    fn verify(
        file: &Path,
        manifest: &Path,
        expected_generation: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        let manifest: Manifest = serde_json::from_slice(
            &std::fs::read(manifest).map_err(|e| format!("read manifest: {e}"))?,
        )
        .map_err(|e| format!("parse manifest: {e}"))?;
        let expected_generation = expected_generation.unwrap_or(manifest.generation);
        let mut db = Database::builder()
            .set_cache_size(32 << 20)
            .open(file)
            .map_err(|e| format!("open: {e}"))?;
        let integrity = db
            .check_integrity()
            .map_err(|e| format!("integrity: {e}"))?;
        let check = churn::verify(&db, manifest.shape).map_err(|e| format!("verify: {e}"))?;
        let store = RedbBackend::from_database(db).map_err(|e| e.to_string())?;
        let (digest, keys) =
            pollster::block_on(workload::digest(&store)).map_err(|e| e.to_string())?;
        // The digest is ALWAYS enforced. An earlier version skipped it
        // whenever the caller expected a different generation than the
        // manifest recorded — which is exactly the browser→native route this
        // command exists for, so the content check was disabled on the only
        // path that needed it. It is safe to enforce unconditionally because
        // the digest covers the `muniment` table while the generation churn
        // writes `probe_meta`/`probe_data`: extending generations cannot move
        // it. If it ever does move, that is a real semantic divergence and
        // this should fail.
        let digest_ok = digest == manifest.muniment_digest;
        let ok = integrity
            && check.ok
            && check.generation == expected_generation
            && digest_ok
            && keys == manifest.muniment_keys;
        Ok(json!({
            "ok": ok,
            "integrity": integrity,
            "check": check,
            "expected_generation": expected_generation,
            "muniment_digest": digest,
            "muniment_digest_expected": manifest.muniment_digest,
            "muniment_digest_ok": digest_ok,
            "muniment_keys": keys,
            "file_len": std::fs::metadata(file).map(|m| m.len()).unwrap_or(0),
        }))
    }

    pub fn main() -> i32 {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let outcome = match args.as_slice() {
            [verb, dir] if verb == "write" => write(&PathBuf::from(dir)),
            [verb, file, manifest] if verb == "verify" => {
                verify(&PathBuf::from(file), &PathBuf::from(manifest), None)
            }
            [verb, file, manifest, generation] if verb == "verify" => generation
                .parse()
                .map_err(|e| format!("expected_generation: {e}"))
                .and_then(|g| verify(&PathBuf::from(file), &PathBuf::from(manifest), Some(g))),
            _ => Err("usage: fixture write <dir> | fixture verify <file.redb> <manifest.json> [expected_generation]".into()),
        };
        match outcome {
            Ok(value) => {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
                if value["ok"].as_bool() == Some(true) {
                    0
                } else {
                    1
                }
            }
            Err(error) => {
                println!("{}", json!({ "ok": false, "error": error }));
                2
            }
        }
    }
}
