// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! W0 receipt: the projection walk's two second datasets, plus the bare
//! headless scenario that drives the installed surface end to end.
//!
//! Nothing here renders a scene, and nothing here reads a projection contract.
//! W0 only has to leave three things on disk and under test:
//!
//! - **Chronicle's second dataset.** Two job boards in the same grammar with
//!   different owners — Distillery's and Djinn's — so the authored Chronicle
//!   definition of W2 has something to be pointed at twice. Both are folded
//!   from an authored `MeshEvent` history, never from a live mesh, so the
//!   fixture a reader diffs is the fixture the fold produces.
//! - **Circuit's second dataset.** Mere's own workspace dependency graph.
//!   This one is *derived*, not authored: the test runs `cargo metadata`
//!   itself, writes the graph under `CARGO_TARGET_TMPDIR`, and reads it back.
//!   A committed snapshot of the member list falls a generation behind every
//!   time a crate is added, so there is no committed snapshot any more.
//! - **The bare scenario.** `distillery.installed.v1` composed against a
//!   scripted DOM and driven by its own runner, with one exact resident
//!   receipt observed. This is the skeleton the headed genet-probe receipts of
//!   W1 and W4 grow from.
//!
//! Every identity here is seeded and every instant is authored, so two runs
//! produce the same bytes. That is the whole point of a fixture: the board is
//! a claim-race, and a claim-race is only reviewable if it lands the same way
//! twice.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use cambium::DomHandle;
use distillery::{
    DistilleryInstalledSnapshotV1, DistilleryInstalledSurfaceState, DistilleryResidentSnapshotV1,
    MaintenanceReport, ResidentReceipt, ResidentSettings, RetentionSettings,
    distillery_installed_descriptor, distillery_installed_surface,
};
use mere_surface_api::SurfaceAvailability;
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;
use mesh::spec::{DeterminismClass, JobOutput, JobSpec, VerificationClass};
use mesh::{
    BlobRef, Digest, ImplementationId, Job, JobBoard, JobBoardSnapshot, JobId, JobState,
    LeaseTerms, MeshEvent, MeshExt, PolicyRevision, ResourceId, RetentionCheckpoint,
};
use p2panda_core::Operation;
use personae::{Ed25519Keypair, IdentityProvider, InMemoryProvider};
use serde::{Deserialize, Serialize};

/// Long enough that no lease in these fixtures expires against an authored
/// instant. Nothing here observes a clock, so this is a span, not a timeout.
const LEASE_MS: u64 = 60_000;
const HEARTBEAT_MS: u64 = 10_000;
/// Salt for the seeded authoring keys. One salt for the whole walk: the seeds
/// are what tell two devices apart, and they are stated at the call site.
const WALK_SALT: &[u8] = b"distillery-walk";
/// The one resource both boards ask for, so a reader comparing them is
/// comparing owners and histories rather than vocabularies.
const RESOURCE: &str = "mesh.blake3/v1";
/// Set to `1` to author a fixture that is not on disk yet. Absent, a missing
/// fixture is a failure rather than a silent write, so a stale checkout cannot
/// quietly regenerate what a reviewer is meant to be reading.
const WRITE_VAR: &str = "WALK_FIXTURES_WRITE";

// ── The receipt stream, made serializable ────────────────────────────────────

/// A resident receipt reduced to what a fixture can carry.
///
/// `ResidentReceipt` deliberately does not derive `Serialize`: it holds the
/// exact `Step` receipts `MeshHost` returned and a whole `MaintenanceReport`
/// with its accepted checkpoint, none of which belongs in a hand-reviewed
/// fixture. `TickRecord` is the projection of that stream Chronicle actually
/// reads — how many steps a turn took, and what maintenance released — and
/// nothing else. It is a stand-in on purpose; W1 reads the live receipts.
///
/// The maintenance counts are `candidates` and `collected`, the two counts
/// `MaintenanceReport` keeps. There is no "retained" count in the works: a
/// candidate is a distinct blob reference that was *safe* to release at the
/// accepted checkpoint, and `collected` is how many custody claims actually
/// went, so the difference between them is what a custodian refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum TickRecord {
    /// One supervisor turn, and how many substrate steps it returned.
    Tick { step_count: usize },
    /// The frontier advanced and maintenance committed.
    MaintenanceCompleted { candidates: u64, collected: u64 },
    /// The cadence fired against an unchanged frontier.
    MaintenanceIdle,
    /// Maintenance was refused; observable and non-fatal.
    MaintenanceFailed { error: String },
    /// The supervisor itself failed, so the resident loop is ending.
    SupervisorFailed { error: String },
    /// The caller's shutdown signal won the lifecycle race.
    StopRequested,
}

impl From<&ResidentReceipt> for TickRecord {
    fn from(receipt: &ResidentReceipt) -> Self {
        match receipt {
            ResidentReceipt::Tick { steps } => Self::Tick {
                step_count: steps.len(),
            },
            ResidentReceipt::MaintenanceCompleted(report) => Self::MaintenanceCompleted {
                candidates: report.candidates,
                collected: report.collected,
            },
            ResidentReceipt::MaintenanceIdle => Self::MaintenanceIdle,
            ResidentReceipt::MaintenanceFailed { error } => Self::MaintenanceFailed {
                error: error.clone(),
            },
            ResidentReceipt::SupervisorFailed { error } => Self::SupervisorFailed {
                error: error.clone(),
            },
            ResidentReceipt::StopRequested => Self::StopRequested,
        }
    }
}

/// One owner's Chronicle dataset: the mesh it names, whose log it is, the jobs
/// the fold produced, and the receipt stream those jobs hang from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ChronicleFixture {
    mesh_id: String,
    owner: String,
    jobs: Vec<Job>,
    ticks: Vec<TickRecord>,
}

// ── Authoring ────────────────────────────────────────────────────────────────

/// One device's chained log, so a fixture reads as a sequence of events.
///
/// Copied from mesh's own lease receipts, with the mesh id carried per device
/// because this file authors two meshes. A device's `seq`/`backlink` chain runs
/// across every job it touches, which is what a per-author log actually is.
struct Device {
    keypair: Ed25519Keypair,
    mesh: [u8; 32],
    seq: u32,
    backlink: Option<[u8; 32]>,
}

impl Device {
    fn new(mesh: [u8; 32], seed: u8) -> Self {
        Self {
            keypair: InMemoryProvider::from_seed([seed; 32])
                .derive_keypair(WALK_SALT)
                .expect("seeded mesh authoring key"),
            mesh,
            seq: 0,
            backlink: None,
        }
    }

    fn author(&mut self, event: &MeshEvent) -> Operation<MeshExt> {
        let op = mesh::to_operation(&self.keypair, self.mesh, event, self.seq, self.backlink);
        self.seq += 1;
        self.backlink = Some(*op.hash.as_bytes());
        op
    }
}

/// The shape both boards post: one named input, one bounded output slot, an
/// exact determinism ask. `label` and `nonce` are what keep otherwise identical
/// posts distinct operations, and so distinct jobs.
fn spec(label: &str, nonce: u64) -> JobSpec {
    JobSpec::simple(
        ResourceId::parse(RESOURCE).expect("resource id"),
        "payload",
        BlobRef::blake3(format!("{label}/{nonce}").as_bytes()),
        "result",
        64,
        DeterminismClass::Exact,
    )
}

/// An output that honours the grant `spec` signed: same slot, same resource,
/// inside the ceiling, exact enough for an exact ask.
fn output(label: &str) -> JobOutput {
    JobOutput {
        name: "result".to_owned(),
        blob: BlobRef::blake3(format!("{label}/committed").as_bytes()),
        resource: ResourceId::parse(RESOURCE).expect("resource id"),
        implementation: ImplementationId::parse("mesh.blake3.reference/v1")
            .expect("implementation id"),
        verification: VerificationClass::ExactBytes,
    }
}

/// Fold one owner's authored history into the Chronicle grammar: at least one
/// job posted and never claimed, one claimed under a live lease with a device
/// already proposed for the epoch after it, and one committed.
///
/// `seeds` are the five devices, in order: the asker who posts the open and
/// committed jobs, the steward who posts the lendable one, two racing workers,
/// and the latecomer who proposes itself for the next epoch. `open_jobs` is how
/// many never-claimed jobs the board carries, which is the one lever that makes
/// the two fixtures different sizes.
fn chronicle_board(
    mesh: [u8; 32],
    owner: &str,
    seeds: [u8; 5],
    open_jobs: u64,
) -> ChronicleFixture {
    let mut asker = Device::new(mesh, seeds[0]);
    let mut steward = Device::new(mesh, seeds[1]);
    let mut first = Device::new(mesh, seeds[2]);
    let mut second = Device::new(mesh, seeds[3]);
    let mut latecomer = Device::new(mesh, seeds[4]);
    let mut log = Vec::new();

    // 1. Posted and left alone. No claim, so the fold leaves it `Posted` — the
    //    Chronicle event card with an open span.
    for nonce in 0..open_jobs {
        log.push(asker.author(&MeshEvent::JobPostedV2 {
            spec: Box::new(spec("open", nonce)),
            nonce,
            at_ms: 100 + nonce,
        }));
    }

    // 2. Lendable, and being worked. Two devices race; the deterministic winner
    //    is whichever claim operation hashes lower, which every peer computes
    //    the same way, so the fixture does not have to encode a choice.
    let leased_post = steward.author(&MeshEvent::JobPostedV2 {
        spec: Box::new(spec("leased", 0).leased(LeaseTerms::new(LEASE_MS, HEARTBEAT_MS))),
        nonce: 10,
        at_ms: 1_000,
    });
    let leased = JobId(*leased_post.hash.as_bytes());
    let claim_first = first.author(&MeshEvent::JobClaimed {
        job: leased.0,
        at_ms: 1_200,
    });
    let claim_second = second.author(&MeshEvent::JobClaimed {
        job: leased.0,
        at_ms: 1_200,
    });
    let first_wins = claim_first.hash.as_bytes() < claim_second.hash.as_bytes();
    log.extend([leased_post, claim_first, claim_second]);
    let granted_at_ms = 2_000;
    let expires_at_ms = granted_at_ms + LEASE_MS;
    let holder = if first_wins { &mut first } else { &mut second };
    log.push(holder.author(&MeshEvent::LeaseGranted {
        job: leased.0,
        epoch: 0,
        granted_at_ms,
        expires_at_ms,
    }));
    // A claim signed at the running epoch's boundary is eligible for the *next*
    // epoch and cannot unseat the live one, so it lands in `next_claimants`
    // without touching the state. That is the field W1's score has to disclose.
    log.push(latecomer.author(&MeshEvent::JobClaimed {
        job: leased.0,
        at_ms: expires_at_ms,
    }));

    // 3. Posted, claimed, committed. Unleased, so the claim winner is the sole
    //    claimant and its `JobDoneV2` closes the job at V2's content address.
    let committed_post = asker.author(&MeshEvent::JobPostedV2 {
        spec: Box::new(spec("committed", 0)),
        nonce: 20,
        at_ms: 3_000,
    });
    let committed = JobId(*committed_post.hash.as_bytes());
    log.push(committed_post);
    log.push(second.author(&MeshEvent::JobClaimed {
        job: committed.0,
        at_ms: 3_100,
    }));
    log.push(second.author(&MeshEvent::JobDoneV2 {
        job: committed.0,
        output: Box::new(output(owner)),
        at_ms: 3_500,
    }));

    let board = JobBoard::fold(mesh, log.iter());
    ChronicleFixture {
        mesh_id: hex(mesh),
        owner: owner.to_owned(),
        jobs: board.jobs().cloned().collect(),
        ticks: ticks(mesh, &board),
    }
}

/// A short, plausible turn of the resident cadence over this board: two ticks,
/// a maintenance pass that released what the checkpoint made safe, an idle pass
/// against the unchanged frontier, then the owner's stop.
///
/// These are built as real `ResidentReceipt` values and converted, so the
/// fixture is the mapping's output rather than a parallel hand-written stream.
fn ticks(mesh: [u8; 32], board: &JobBoard) -> Vec<TickRecord> {
    let checkpoint = RetentionCheckpoint::new(
        mesh,
        PolicyRevision([8; 32]),
        Digest::blake3(b"distillery-walk-authority"),
        Vec::new(),
        JobBoardSnapshot::from_board(board),
        4_000,
    );
    let terminal = board.jobs().filter(|job| job.state.is_terminal()).count() as u64;
    let receipts = [
        ResidentReceipt::Tick { steps: Vec::new() },
        ResidentReceipt::Tick { steps: Vec::new() },
        ResidentReceipt::MaintenanceCompleted(Box::new(MaintenanceReport {
            checkpoint,
            candidates: terminal,
            collected: terminal,
            effects: Vec::new(),
        })),
        ResidentReceipt::MaintenanceIdle,
        ResidentReceipt::StopRequested,
    ];
    receipts.iter().map(TickRecord::from).collect()
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ── Fixture files ────────────────────────────────────────────────────────────

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Read a committed fixture with its authored line endings restored.
///
/// The documents are written LF-only. This repository has `core.autocrlf` on
/// and no `text eol=lf` attribute covering these paths, so a Windows checkout
/// hands them back with CRLF. Undoing that here compares the document rather
/// than the checkout filter; it is not slack in the comparison.
fn read_fixture(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.replace("\r\n", "\n"))
}

/// Compare a regenerated document against its committed file, or author the
/// file when `WALK_FIXTURES_WRITE=1` and none exists yet.
fn compare_or_author(path: &Path, document: &str) {
    match read_fixture(path) {
        Some(committed) => assert_eq!(
            committed,
            document,
            "{} no longer matches what the fold produces; regenerate it with \
             {WRITE_VAR}=1 after deleting it, and review the diff",
            path.display()
        ),
        None if std::env::var(WRITE_VAR).as_deref() == Ok("1") => {
            fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
            fs::write(path, document).expect("author fixture");
        }
        None => panic!(
            "{} is absent; re-run with {WRITE_VAR}=1 to author it",
            path.display()
        ),
    }
}

// ── Circuit's derived dataset: the workspace graph ───────────────────────────

/// Where the generated workspace graph lands. `CARGO_TARGET_TMPDIR` is a
/// per-test-target directory under `target/`, so the derived document never
/// goes back into `tests/fixtures/` and is never a thing to commit.
fn generated_workspace_graph_path() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("circuit/workspace_graph.json")
}

/// The cargo that is running this test, so a toolchain override is honoured.
fn cargo_binary() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

/// Run a tool in the port directory and hand back its stdout, or say plainly
/// what failed. Nothing here falls back to a cached answer: if the workspace
/// cannot be read, the test that reads the graph has to fail, not pass with an
/// empty one.
fn tool_output(program: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|error| panic!("`{program} {}` did not run: {error}", args.join(" ")));
    assert!(
        output.status.success(),
        "`{program} {}` failed ({}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("`{program}` printed non-UTF-8: {error}"))
}

/// Derive the Circuit graph from `cargo metadata` and write it out.
///
/// Only workspace members appear. Edges are the normal and build dependencies
/// between members; dev-dependencies are deliberately excluded, because a
/// dev-dependency may legitimately point back at a crate that depends on it
/// and the graph is asserted to be acyclic.
///
/// Keys are sorted and the document ends in a newline, so two runs against one
/// commit produce byte-identical files.
fn generate_workspace_graph() -> PathBuf {
    let metadata: serde_json::Value =
        serde_json::from_str(&tool_output(&cargo_binary(), &[
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
        ]))
        .expect("cargo metadata parses");
    let packages = metadata["packages"]
        .as_array()
        .expect("cargo metadata names its packages");
    assert!(
        !packages.is_empty(),
        "cargo metadata returned no workspace members"
    );

    let members: std::collections::BTreeSet<String> = packages
        .iter()
        .map(|package| package["name"].as_str().expect("package name").to_owned())
        .collect();
    let mut edges: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for package in packages {
        let source = package["name"].as_str().expect("package name");
        for dependency in package["dependencies"]
            .as_array()
            .expect("package dependencies")
        {
            // `kind` is null for a normal dependency, "build" for a
            // build-script one, "dev" for a test-only one.
            match dependency["kind"].as_str() {
                None | Some("build") => {}
                Some(_) => continue,
            }
            let target = dependency["name"].as_str().expect("dependency name");
            if members.contains(target) {
                edges.insert((source.to_owned(), target.to_owned()));
            }
        }
    }

    // The commit the graph was read from, so a reader can tell two generations
    // apart. A dirty tree still names its HEAD; that is honest enough here.
    let generated_from = tool_output("git", &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_owned();
    assert!(
        !generated_from.is_empty(),
        "git named no HEAD for the workspace graph"
    );

    let graph = serde_json::json!({
        "generated_from": generated_from,
        "packages": members.iter().collect::<Vec<_>>(),
        "edges": edges
            .iter()
            .map(|(from, to)| vec![from, to])
            .collect::<Vec<_>>(),
    });

    let path = generated_workspace_graph_path();
    fs::create_dir_all(path.parent().expect("graph parent")).expect("graph directory");
    fs::write(&path, document(&graph)).expect("write generated workspace graph");
    path
}

fn document<T: Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("fixture serializes");
    text.push('\n');
    text
}

// ── W0's three receipts ──────────────────────────────────────────────────────

/// Chronicle's founding and second datasets: one grammar, two owners.
#[test]
fn chronicle_fixtures_are_deterministic_and_round_trip() {
    let boards: [(&str, fn() -> ChronicleFixture); 2] = [
        ("distillery", || {
            chronicle_board([0xd1; 32], "distillery", [0xa1, 0xa2, 0xb1, 0xb2, 0xb3], 1)
        }),
        ("djinn", || {
            chronicle_board([0x1d; 32], "djinn", [0xc1, 0xc2, 0xe1, 0xe2, 0xe3], 2)
        }),
    ];
    let mut restored = Vec::new();

    for (owner, build) in boards {
        let rendered = document(&build());
        assert_eq!(
            rendered,
            document(&build()),
            "{owner}: two folds of one authored history disagree, so the fixture is not a fixture"
        );
        let path = fixtures().join("chronicle").join(format!("{owner}_board.json"));
        compare_or_author(&path, &rendered);

        let fixture: ChronicleFixture =
            serde_json::from_str(&read_fixture(&path).expect("committed fixture"))
                .expect("committed fixture parses");
        assert_eq!(fixture.owner, owner);
        let jobs: &Vec<Job> = &fixture.jobs;
        assert!(
            jobs.iter().any(|job| matches!(job.state, JobState::Posted)),
            "{owner}: a job posted and never claimed"
        );
        assert!(
            jobs.iter()
                .any(|job| matches!(job.state, JobState::Claimed { .. })),
            "{owner}: a job claimed under a lease"
        );
        assert!(
            jobs.iter()
                .any(|job| matches!(job.state, JobState::Committed { .. })),
            "{owner}: a job committed at a content address"
        );
        assert!(
            jobs.iter().any(|job| !job.next_claimants.is_empty()),
            "{owner}: a device proposed for the next lease epoch"
        );
        assert_eq!(
            fixture.ticks.last(),
            Some(&TickRecord::StopRequested),
            "{owner}: the receipt stream ends where the owner stopped it"
        );
        restored.push(fixture);
    }

    let [distillery, djinn] = <[ChronicleFixture; 2]>::try_from(restored).expect("two fixtures");
    assert_ne!(
        distillery.mesh_id, djinn.mesh_id,
        "the two datasets are two meshes, not one mesh read twice"
    );
    assert_ne!(
        distillery.jobs.len(),
        djinn.jobs.len(),
        "the boards differ in size, so a reader can tell which one is on screen"
    );
    assert!(
        distillery
            .jobs
            .iter()
            .all(|job| djinn.jobs.iter().all(|other| other.id != job.id)),
        "no job id is shared between the two owners"
    );
}

/// Circuit's second dataset, per the walk plan's §3.2: the workspace graph is
/// Circuit's own founding dataset, and it must read as a DAG over the packages
/// it names. The test derives the graph from `cargo metadata` itself and then
/// reads what it wrote, so the dataset cannot fall behind the member list.
#[test]
fn workspace_graph_fixture_is_a_dag_over_named_packages() {
    #[derive(Deserialize)]
    struct WorkspaceGraph {
        generated_from: String,
        packages: Vec<String>,
        edges: Vec<(String, String)>,
    }

    let path = generate_workspace_graph();
    let graph: WorkspaceGraph =
        serde_json::from_str(&read_fixture(&path).expect("generated workspace graph"))
            .expect("workspace graph parses");
    assert!(
        !graph.generated_from.is_empty(),
        "the graph names the commit it was read from"
    );
    assert!(graph.packages.len() > 1, "a workspace of one is not a graph");

    // A stale or empty graph is caught here rather than by the DAG walk, which
    // is happy to succeed over nothing. These four are load-bearing members of
    // the boundary work the Circuit recipe renders.
    for member in ["pelt", "knot-editor-host", "tabard", "mere-document-lanes"] {
        assert!(
            graph.packages.iter().any(|name| name == member),
            "the generated graph does not name `{member}`, so it is stale or empty"
        );
    }
    assert!(
        !graph.edges.is_empty(),
        "a workspace graph with no edges between members is not a graph"
    );
    assert!(
        graph.packages.windows(2).all(|pair| pair[0] < pair[1]),
        "packages are sorted and distinct, so two generations diff cleanly"
    );

    let index: Vec<&str> = graph.packages.iter().map(String::as_str).collect();
    let position = |name: &str| index.binary_search(&name).ok();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); index.len()];
    for (from, to) in &graph.edges {
        let from = position(from).unwrap_or_else(|| panic!("edge source {from} is not a package"));
        let to = position(to).unwrap_or_else(|| panic!("edge target {to} is not a package"));
        adjacency[from].push(to);
    }

    // Iterative DFS with the usual three colours: 0 unvisited, 1 on the stack,
    // 2 finished. Meeting a node that is still on the stack is a cycle.
    let mut colour = vec![0u8; index.len()];
    for root in 0..index.len() {
        if colour[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        colour[root] = 1;
        while let Some((node, cursor)) = stack.pop() {
            match adjacency[node].get(cursor) {
                Some(&next) => {
                    stack.push((node, cursor + 1));
                    assert_ne!(
                        colour[next], 1,
                        "{} depends back on {}, so the graph is not a DAG",
                        index[node], index[next]
                    );
                    if colour[next] == 0 {
                        colour[next] = 1;
                        stack.push((next, 0));
                    }
                }
                None => colour[node] = 2,
            }
        }
    }
}

/// The bare scenario: the installed surface composed against a scripted DOM and
/// driven by its own runner, with one exact receipt observed. No vault, no
/// resident, no window — this is the skeleton the headed receipts grow from.
#[test]
fn installed_surface_drives_headless_through_its_runner() {
    let snapshot = DistilleryInstalledSnapshotV1 {
        profile: "research".to_owned(),
        protection: "passphrase-wrapped vault root".to_owned(),
        mesh_id: [0xd1; 32],
        mesh_root: PathBuf::from("/distillery/walk/mesh"),
        mesh_store_path: PathBuf::from("/distillery/walk/mesh/mesh.redb"),
        blob_store_root: PathBuf::from("/distillery/walk/blobs"),
        resident: Some(DistilleryResidentSnapshotV1 {
            settings: ResidentSettings {
                tick_every: Duration::from_millis(250),
                maintenance_every: Some(Duration::from_secs(30)),
                blob_gc_every: Duration::from_secs(5),
                retention: RetentionSettings {
                    collect_after_checkpoint: true,
                },
            },
        }),
    };
    let mut state = DistilleryInstalledSurfaceState::new(snapshot);
    state.observe_receipt(ResidentReceipt::Tick { steps: Vec::new() });

    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    let session = distillery_installed_surface(dom.clone(), state);

    assert_eq!(session.descriptor(), &distillery_installed_descriptor());
    assert_eq!(
        session.descriptor().surface_id.as_str(),
        "distillery.installed.v1"
    );
    assert_eq!(session.availability(), SurfaceAvailability::Available);
    assert!(
        text_present(&dom.borrow(), "Resident receipt: tick (0 steps)"),
        "the runner rendered the observed receipt rather than a placeholder"
    );
    assert!(
        text_present(&dom.borrow(), "Profile: research"),
        "the runner rendered the installed facts it was handed"
    );
}

/// Whether any text node under the document contains `needle`. The same walk
/// the surface's own unit tests use, so a scenario and a unit test agree on
/// what "rendered" means.
fn text_present(dom: &ScriptedDom, needle: &str) -> bool {
    fn contains(dom: &ScriptedDom, node: genet_scripted_dom::NodeId, needle: &str) -> bool {
        dom.text(node).is_some_and(|text| text.contains(needle))
            || dom
                .dom_children(node)
                .any(|child| contains(dom, child, needle))
    }
    contains(dom, dom.document(), needle)
}
