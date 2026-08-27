// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Ingest a scenario-receipt directory into the personal graph's blob store.
//!
//! The CLI half of R2: `remote-receipt.ps1` fetches a receipt from another
//! machine and then calls this, so a run on the ThinkPad or the iMac becomes a
//! replicated fact in the same motion that produced it.
//!
//! ```text
//! receipt_ingest --dir testing/woodshed/thinkpad-2026-08-10_143105 \
//!                --store <data-root>/blobs.redb --device laptop
//! ```
//!
//! `--store` is a redb database file, the same backend the resident host uses
//! for the personal graph on desktop (muniment ships no filesystem backend by
//! design: the host chooses the realization).
//!
//! Prints the receipt's node id, its address, and one line per artifact, then
//! writes the authored events beside the receipt as `graph-events.json` so the
//! resident host can pick them up (and so a dry run is inspectable). Ingest is
//! idempotent, so running it twice on the same directory is safe and says so.

use std::path::PathBuf;

use graphshell::receipts::{self, ReceiptError};
use muniment::{BlobStore, RedbBackend};

fn usage() -> ! {
    eprintln!(
        "usage: receipt_ingest --dir <receipt-dir> --store <blobs.redb> \
         [--device <name>] [--inbox <dir> | --data-root <dir>] [--dry-run]"
    );
    std::process::exit(2);
}

struct Args {
    dir: PathBuf,
    store: PathBuf,
    device: String,
    inbox: Option<PathBuf>,
    dry_run: bool,
}

fn parse_args() -> Args {
    let mut dir = None;
    let mut store = None;
    let mut device = None;
    let mut inbox = None;
    let mut dry_run = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = args.next().map(PathBuf::from),
            "--store" => store = args.next().map(PathBuf::from),
            "--device" => device = args.next(),
            "--inbox" => inbox = args.next().map(PathBuf::from),
            "--data-root" => {
                inbox = args
                    .next()
                    .map(|root| receipts::inbox_dir(std::path::Path::new(&root)));
            }
            "--dry-run" => dry_run = true,
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unexpected argument `{other}`");
                usage();
            }
        }
    }
    let Some(dir) = dir else { usage() };
    let Some(store) = store else { usage() };
    Args {
        dir,
        store,
        // The device name defaults to this machine's hostname: blob
        // availability is per device, and a wrong or missing name would claim
        // the bytes are somewhere they are not.
        device: device.unwrap_or_else(default_device),
        inbox,
        dry_run,
    }
}

fn default_device() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-device".to_string())
}

// `block_on` rather than a spawned task: muniment's `Backend` is `?Send` so a
// browser main thread can await OPFS promises, which means this future never
// crosses threads. Nothing here wants concurrency anyway.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = parse_args();
    if let Err(error) = run(&args).await {
        eprintln!("receipt_ingest: {error}");
        std::process::exit(1);
    }
}

async fn run(args: &Args) -> Result<(), ReceiptError> {
    if let Some(parent) = args.store.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let store = BlobStore::new(RedbBackend::open(&args.store)?);

    let ingested = receipts::ingest_directory(&args.dir, &store, &args.device).await?;

    println!("node    {}", ingested.node);
    println!("address {}", ingested.address);
    println!("device  {}", args.device);
    for (name, hash) in &ingested.blobs {
        println!("blob    {name} {}", hash.to_hex());
    }
    println!("events  {}", ingested.events.len());

    if args.dry_run {
        println!("dry run: nothing written");
        return Ok(());
    }

    // A copy beside the receipt, always: it is what an owner reads to see what
    // is about to be authored on their behalf, and it keeps a receipt
    // self-describing if it is ever moved by hand.
    let events_path = args.dir.join("graph-events.json");
    let json =
        serde_json::to_string_pretty(&ingested.events).expect("personal graph events serialize");
    std::fs::write(&events_path, json)?;
    println!("wrote   {}", events_path.display());

    // The hand-off. Deposited rather than authored here: the resident host
    // owns the authoring turn, because it holds the signing identity and the
    // log, and a CLI writing operations behind its back would be a second
    // writer for one graph. With no inbox given, ingest still did its real
    // work (the blobs are stored) and the events simply wait to be filed.
    match &args.inbox {
        Some(inbox) => {
            let deposited =
                receipts::write_to_inbox(inbox, ingested.node, &args.dir, &ingested.events)?;
            println!("inbox   {}", deposited.display());
            println!("        the resident host authors it within ~10s");
        }
        None => println!("inbox   (none given; pass --inbox or --data-root to file it)"),
    }
    Ok(())
}
