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
         [--device <name>] [--dry-run]"
    );
    std::process::exit(2);
}

struct Args {
    dir: PathBuf,
    store: PathBuf,
    device: String,
    dry_run: bool,
}

fn parse_args() -> Args {
    let mut dir = None;
    let mut store = None;
    let mut device = None;
    let mut dry_run = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = args.next().map(PathBuf::from),
            "--store" => store = args.next().map(PathBuf::from),
            "--device" => device = args.next(),
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

    // The events land beside the receipt rather than being pushed into a
    // running replica: the resident host owns the authoring turn (it holds the
    // signing identity and the log), and a CLI that wrote operations behind
    // its back would be a second writer for the same graph.
    let events_path = args.dir.join("graph-events.json");
    let json = serde_json::to_string_pretty(&ingested.events)
        .expect("personal graph events serialize");
    std::fs::write(&events_path, json)?;
    println!("wrote   {}", events_path.display());
    Ok(())
}
