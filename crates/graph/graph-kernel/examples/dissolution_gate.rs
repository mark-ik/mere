// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lane-D gate measurement: can Container + facets match the current rkyv
//! `Node` on snapshot load and hot graph ops?
//!
//! Run: `cargo run --release -p mere-kernel --example dissolution_gate`
//! Node count overrides with `DISSOLUTION_GATE_NODES` (default 50_000, the
//! scale the node-image plan measured at).
//!
//! # What this isolates, and why the plan's wording needed sharpening
//!
//! The plan asks whether "Container + facet-sidecar equivalents" can match the
//! current rkyv `Node`. Read naively that compares two things at once, because
//! today's `PersistedNode` is **rkyv** while `Container` is serde-only and
//! `NodeFacets` holds `serde_json::Value`. Measured that way, the dissolved
//! shape would lose on codec choice and tell us nothing about dissolution.
//!
//! So the arms hold the codec constant and vary the shape:
//!
//! - **A** — today: fat node, images inline, rkyv.
//! - **A-post-D0** — same shape, images as refs. D0 lands before D1 either way,
//!   so this, not A, is D1's real baseline.
//! - **B** — dissolved: container + facet sidecar, images as refs, rkyv. Same
//!   codec as A-post-D0, so the delta is structural.
//! - **C** — dissolved, but encoded the way the facet types are actually shaped
//!   today (JSON). This is a codec cost, reported separately so it cannot be
//!   mistaken for the cost of dissolving.
//! - **C..G, the codec race** — the *same* open payload (containers +
//!   `serde_json::Value` facet maps) through serde_json, ciborium, cbor4ii,
//!   rmp-serde (named), and minicbor-serde. Identical in-memory target, so the
//!   only variable is codec engineering; "is ciborium letting CBOR down" is
//!   answered by measurement, not intuition.
//!
//! Arms are built, measured, and dropped one at a time; the image-bearing arm
//! alone is a few hundred MB.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rkyv::{Archive, Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Arm A — the current fat node, mirrored. Field-for-field with `PersistedNode`
// as of 2026-07-26 minus the enum-typed columns, which carry no bytes for a
// default session and would only add noise to a size comparison.
// ---------------------------------------------------------------------------

#[derive(Archive, Serialize, Deserialize, Clone)]
struct FatNode {
    node_id: String,
    url: String,
    cached_host: Option<String>,
    title: String,
    tags: Vec<String>,
    is_pinned: bool,
    thumbnail_png: Option<Vec<u8>>,
    thumbnail_width: u32,
    thumbnail_height: u32,
    favicon_rgba: Option<Vec<u8>>,
    favicon_width: u32,
    favicon_height: u32,
    mime_hint: Option<String>,
    body: Option<String>,
    last_session_visited: u64,
    scroll_x: Option<f32>,
    scroll_y: Option<f32>,
    form_draft: Option<String>,
    last_visited_ms: Option<u64>,
    nested: Option<String>,
}

#[derive(Archive, Serialize, Deserialize)]
struct FatSnapshot {
    nodes: Vec<FatNode>,
    timestamp_secs: u64,
}

// ---------------------------------------------------------------------------
// Arm B — the dissolved shape. Container carries what the plan's field map
// sends to Container homes; everything else rides the sidecar, keyed by node.
// Facet values are strings here because that is the shape a binary codec can
// carry; arm C measures what `serde_json::Value` costs instead.
// ---------------------------------------------------------------------------

#[derive(Archive, Serialize, Deserialize, Clone)]
struct ThinContainer {
    id: String,
    addresses: Vec<String>,
    content: Option<String>,
    media_type: Option<String>,
    title: Option<String>,
    tags: Vec<String>,
    nested: Option<String>,
}

#[derive(Archive, Serialize, Deserialize, Clone)]
struct FacetRow {
    node: String,
    facets: Vec<(String, String)>,
}

#[derive(Archive, Serialize, Deserialize)]
struct ThinSnapshot {
    containers: Vec<ThinContainer>,
    facets: Vec<FacetRow>,
    timestamp_secs: u64,
}

// ---------------------------------------------------------------------------
// Synthetic session. Proportions follow the node-image plan's finding that
// inline imagery dominated heap at 50k nodes, so most nodes carry one.
// ---------------------------------------------------------------------------

const THUMB_BYTES: usize = 6 * 1024;
const FAVICON_BYTES: usize = 1200;

fn tag_for(i: usize) -> String {
    // A small vocabulary, so the hot tag-filter op matches a realistic slice
    // rather than one node.
    format!("topic-{}", i % 64)
}

fn fat_nodes(count: usize, inline_images: bool) -> Vec<FatNode> {
    (0..count)
        .map(|i| {
            let has_thumb = inline_images && i % 5 != 0;
            let has_favicon = inline_images && i % 10 != 0;
            FatNode {
                node_id: format!("00000000-0000-4000-8000-{i:012}"),
                url: format!("https://example-{}.test/path/segment/{i}", i % 997),
                cached_host: Some(format!("example-{}.test", i % 997)),
                title: format!("A representative page title for node {i}"),
                tags: vec![tag_for(i), tag_for(i + 7)],
                is_pinned: i % 50 == 0,
                thumbnail_png: has_thumb.then(|| vec![0xA5; THUMB_BYTES]),
                thumbnail_width: if has_thumb { 320 } else { 0 },
                thumbnail_height: if has_thumb { 200 } else { 0 },
                favicon_rgba: has_favicon.then(|| vec![0x5A; FAVICON_BYTES]),
                favicon_width: if has_favicon { 16 } else { 0 },
                favicon_height: if has_favicon { 16 } else { 0 },
                mime_hint: Some("text/html".into()),
                body: (i % 8 == 0).then(|| "# A note body\n\nWith a little prose.".into()),
                last_session_visited: i as u64,
                scroll_x: (i % 3 == 0).then_some(0.0),
                scroll_y: (i % 3 == 0).then_some(1280.5),
                form_draft: None,
                last_visited_ms: Some(1_750_000_000_000 + i as u64),
                nested: None,
            }
        })
        .collect()
}

/// The same session, dissolved. Image bytes become refs (D0), the Container
/// homes take their fields, and the rest becomes facet rows.
fn thin_session(count: usize) -> (Vec<ThinContainer>, Vec<FacetRow>) {
    let mut containers = Vec::with_capacity(count);
    let mut facets = Vec::with_capacity(count);
    for i in 0..count {
        let id = format!("00000000-0000-4000-8000-{i:012}");
        containers.push(ThinContainer {
            id: id.clone(),
            addresses: vec![format!("https://example-{}.test/path/segment/{i}", i % 997)],
            content: (i % 8 == 0).then(|| format!("blake3:{:064x}", i)),
            media_type: Some("text/html".into()),
            title: Some(format!("A representative page title for node {i}")),
            tags: vec![tag_for(i), tag_for(i + 7)],
            nested: None,
        });

        let mut rows: Vec<(String, String)> = Vec::new();
        rows.push(("web.compat".into(), format!("example-{}.test", i % 997)));
        if i % 5 != 0 {
            rows.push((
                "image.thumbnail".into(),
                format!("blake3:{:064x}/320x200", i),
            ));
        }
        if i % 10 != 0 {
            rows.push(("image.favicon".into(), format!("blake3:{:064x}/16x16", i)));
        }
        if i % 50 == 0 {
            rows.push(("arrangement.pinned".into(), "true".into()));
        }
        if i % 3 == 0 {
            rows.push(("web.scroll".into(), "0,1280.5".into()));
        }
        rows.push((
            "visit.history".into(),
            format!("{},{}", 1_750_000_000_000u64 + i as u64, i),
        ));
        facets.push(FacetRow {
            node: id,
            facets: rows,
        });
    }
    (containers, facets)
}

/// Arm C: the dissolved shape encoded the way the live facet types are shaped
/// today, `serde_json::Value` in a `BTreeMap` per node.
fn json_facets(count: usize) -> Vec<(String, BTreeMap<String, serde_json::Value>)> {
    let (_, rows) = thin_session(count);
    rows.into_iter()
        .map(|row| {
            let map = row
                .facets
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect();
            (row.node, map)
        })
        .collect()
}

/// The open-map payload every self-describing codec races on: identical
/// in-memory types, so decode cost differences are pure codec engineering.
type OpenPayload = (
    Vec<ThinContainerJson>,
    Vec<(String, BTreeMap<String, serde_json::Value>)>,
);

fn open_payload(count: usize) -> OpenPayload {
    let (containers, _) = thin_session(count);
    let containers = containers
        .into_iter()
        .map(|c| ThinContainerJson {
            id: c.id,
            addresses: c.addresses,
            content: c.content,
            media_type: c.media_type,
            title: c.title,
            tags: c.tags,
            nested: c.nested,
        })
        .collect();
    (containers, json_facets(count))
}

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

fn time_it(label: &str, mut f: impl FnMut()) -> Duration {
    // One warm pass so allocator and cache state are not charged to the first
    // measured run, then take the best of three.
    f();
    let mut best = Duration::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        f();
        best = best.min(start.elapsed());
    }
    println!("    {label:<28} {:>10.1} ms", best.as_secs_f64() * 1000.0);
    best
}

struct ArmResult {
    name: &'static str,
    bytes: usize,
    load_ms: f64,
    hot_ms: f64,
}

fn main() {
    let count: usize = std::env::var("DISSOLUTION_GATE_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000);

    println!("Lane-D gate measurement");
    println!("nodes: {count}");
    println!(
        "build: {}\n",
        if cfg!(debug_assertions) {
            "debug (NOT a valid measurement, re-run with --release)"
        } else {
            "release"
        }
    );

    let mut results: Vec<ArmResult> = Vec::new();

    // --- Arm A: today, images inline -------------------------------------
    {
        println!("A  fat node, images inline, rkyv");
        let snapshot = FatSnapshot {
            nodes: fat_nodes(count, true),
            timestamp_secs: 1,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot).unwrap();
        let encoded = bytes.len();
        println!(
            "    {:<28} {:>10.1} MiB",
            "encoded",
            encoded as f64 / 1048576.0
        );
        let load = time_it("load (deserialize)", || {
            let s = rkyv::from_bytes::<FatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
            std::hint::black_box(&s);
        });
        let loaded = rkyv::from_bytes::<FatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        let hot = time_it("hot: tag filter + by-id", || {
            let wanted = tag_for(11);
            let n = loaded
                .nodes
                .iter()
                .filter(|n| n.tags.contains(&wanted))
                .count();
            let found = loaded
                .nodes
                .iter()
                .find(|n| n.node_id.ends_with("000000042"));
            std::hint::black_box((n, found));
        });
        results.push(ArmResult {
            name: "A   fat + inline images (rkyv)",
            bytes: encoded,
            load_ms: load.as_secs_f64() * 1000.0,
            hot_ms: hot.as_secs_f64() * 1000.0,
        });
    }

    // --- Arm A-post-D0: today's shape, images as refs ---------------------
    {
        println!("\nA' fat node, images as refs (post-D0), rkyv");
        let snapshot = FatSnapshot {
            nodes: fat_nodes(count, false),
            timestamp_secs: 1,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot).unwrap();
        let encoded = bytes.len();
        println!(
            "    {:<28} {:>10.1} MiB",
            "encoded",
            encoded as f64 / 1048576.0
        );
        let load = time_it("load (deserialize)", || {
            let s = rkyv::from_bytes::<FatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
            std::hint::black_box(&s);
        });
        let loaded = rkyv::from_bytes::<FatSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        let hot = time_it("hot: tag filter + by-id", || {
            let wanted = tag_for(11);
            let n = loaded
                .nodes
                .iter()
                .filter(|n| n.tags.contains(&wanted))
                .count();
            let found = loaded
                .nodes
                .iter()
                .find(|n| n.node_id.ends_with("000000042"));
            std::hint::black_box((n, found));
        });
        results.push(ArmResult {
            name: "A'  fat, image refs (rkyv)  <- D1 baseline",
            bytes: encoded,
            load_ms: load.as_secs_f64() * 1000.0,
            hot_ms: hot.as_secs_f64() * 1000.0,
        });
    }

    // --- Arm B: dissolved, same codec ------------------------------------
    {
        println!("\nB  container + facet sidecar, image refs, rkyv");
        let (containers, facets) = thin_session(count);
        let snapshot = ThinSnapshot {
            containers,
            facets,
            timestamp_secs: 1,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&snapshot).unwrap();
        let encoded = bytes.len();
        println!(
            "    {:<28} {:>10.1} MiB",
            "encoded",
            encoded as f64 / 1048576.0
        );
        let load = time_it("load (deserialize)", || {
            let s = rkyv::from_bytes::<ThinSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
            std::hint::black_box(&s);
        });
        let loaded = rkyv::from_bytes::<ThinSnapshot, rkyv::rancor::Error>(&bytes).unwrap();
        let hot = time_it("hot: tag filter + by-id", || {
            let wanted = tag_for(11);
            let n = loaded
                .containers
                .iter()
                .filter(|c| c.tags.contains(&wanted))
                .count();
            let found = loaded
                .containers
                .iter()
                .find(|c| c.id.ends_with("000000042"));
            std::hint::black_box((n, found));
        });
        results.push(ArmResult {
            name: "B   container + facets (rkyv)",
            bytes: encoded,
            load_ms: load.as_secs_f64() * 1000.0,
            hot_ms: hot.as_secs_f64() * 1000.0,
        });
    }

    // --- The open-map codec race: C..G, identical in-memory target --------
    let payload = open_payload(count);

    let mut race = |name: &'static str,
                    banner: &str,
                    encode: &dyn Fn(&OpenPayload) -> Vec<u8>,
                    decode: &dyn Fn(&[u8]) -> OpenPayload| {
        println!("\n{banner}");
        let bytes = encode(&payload);
        let encoded = bytes.len();
        println!(
            "    {:<28} {:>10.1} MiB",
            "encoded",
            encoded as f64 / 1048576.0
        );
        let load = time_it("load (deserialize)", || {
            let s = decode(&bytes);
            std::hint::black_box(&s);
        });
        let loaded = decode(&bytes);
        let hot = time_it("hot: tag filter + by-id", || {
            let wanted = tag_for(11);
            let n = loaded.0.iter().filter(|c| c.tags.contains(&wanted)).count();
            let found = loaded.0.iter().find(|c| c.id.ends_with("000000042"));
            std::hint::black_box((n, found));
        });
        results.push(ArmResult {
            name,
            bytes: encoded,
            load_ms: load.as_secs_f64() * 1000.0,
            hot_ms: hot.as_secs_f64() * 1000.0,
        });
    };

    race(
        "C   open map, serde_json",
        "C  open map, serde_json (today's facet types)",
        &|p| serde_json::to_vec(p).unwrap(),
        &|b| serde_json::from_slice(b).unwrap(),
    );
    race(
        "D   open map, ciborium (CBOR)",
        "D  open map, ciborium (CBOR)",
        &|p| {
            let mut out = Vec::new();
            ciborium::into_writer(p, &mut out).unwrap();
            out
        },
        &|b| ciborium::from_reader(b).unwrap(),
    );
    race(
        "E   open map, cbor4ii (CBOR)",
        "E  open map, cbor4ii (CBOR)",
        &|p| cbor4ii::serde::to_vec(Vec::new(), p).unwrap(),
        &|b| cbor4ii::serde::from_slice(b).unwrap(),
    );
    race(
        "F   open map, rmp-serde (MessagePack)",
        "F  open map, rmp-serde (MessagePack, named structs)",
        &|p| rmp_serde::to_vec_named(p).unwrap(),
        &|b| rmp_serde::from_slice(b).unwrap(),
    );
    race(
        "G   open map, minicbor-serde (CBOR)",
        "G  open map, minicbor-serde (CBOR)",
        &|p| minicbor_serde::to_vec(p).unwrap(),
        &|b| minicbor_serde::from_slice(b).unwrap(),
    );

    // --- Verdict table ----------------------------------------------------
    println!("\n{:-<78}", "");
    println!(
        "{:<42} {:>10} {:>10} {:>10}",
        "arm", "MiB", "load ms", "hot ms"
    );
    println!("{:-<78}", "");
    for r in &results {
        println!(
            "{:<42} {:>10.1} {:>10.1} {:>10.2}",
            r.name,
            r.bytes as f64 / 1048576.0,
            r.load_ms,
            r.hot_ms
        );
    }
    println!("{:-<78}", "");

    // The gate compares B against A', not against A: D0 lands first either way.
    let baseline = &results[1];
    let dissolved = &results[2];
    let json = &results[3];
    println!(
        "\nB vs A' (the structural question, codec held constant):\n  \
         size {:.2}x   load {:.2}x   hot {:.2}x",
        dissolved.bytes as f64 / baseline.bytes as f64,
        dissolved.load_ms / baseline.load_ms,
        dissolved.hot_ms / baseline.hot_ms,
    );
    println!(
        "\nC vs B (the codec cost, shape held constant):\n  \
         size {:.2}x   load {:.2}x   hot {:.2}x",
        json.bytes as f64 / dissolved.bytes as f64,
        json.load_ms / dissolved.load_ms,
        json.hot_ms / dissolved.hot_ms,
    );
    println!("\nCodec race vs C (same in-memory payload; <1.00 load = faster than JSON):");
    for r in &results[4..] {
        println!(
            "  {:<40} size {:.2}x   load {:.2}x",
            r.name,
            r.bytes as f64 / json.bytes as f64,
            r.load_ms / json.load_ms,
        );
    }
    let best = results[3..]
        .iter()
        .min_by(|a, b| a.load_ms.total_cmp(&b.load_ms))
        .unwrap();
    println!(
        "\nBest open-map codec: {} at {:.1} ms — vs closed-typed rkyv (B) {:.2}x load",
        best.name.trim(),
        best.load_ms,
        best.load_ms / dissolved.load_ms,
    );
}

/// Container mirror with owned `String` fields for the JSON arm (rkyv's derive
/// is not needed here and its archived types do not deserialize from JSON).
#[derive(serde::Serialize, serde::Deserialize)]
struct ThinContainerJson {
    id: String,
    addresses: Vec<String>,
    content: Option<String>,
    media_type: Option<String>,
    title: Option<String>,
    tags: Vec<String>,
    nested: Option<String>,
}
