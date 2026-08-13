//! P1 of the spatial compute plan (2026-08-13): physics proposes, the
//! record disposes.
//!
//! A card (node body) is dragged with `pin`, released with `unpin`, and
//! at the release the *host* reads seiche's containment proposals and
//! mints a fact. The fact type, the commitment rule, and the log all
//! live here in the test, because they are the host's organs: seiche
//! answers the geometric question and owns no record. The five receipts
//! are the gate's done-conditions, one test each.

use euclid::default::Point2D;
use seiche::{NodeCollider, NodeKey, SceneBodyId, Simulation};

/// The discrete, attributed outcome of a committed release. Positions
/// are absent from this type by construction, which is receipt three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    card: NodeKey,
    into: SceneBodyId,
    at: u64,
}

/// The host's whole doctrine in one function: a release is the only
/// moment a containment proposal may become a fact.
fn commit_release(sim: &Simulation, card: NodeKey, at: u64, log: &mut Vec<Placed>) {
    for into in sim.containments_of(card) {
        log.push(Placed { card, into, at });
    }
}

/// One card, one bin. The card starts well west of the bin; the drag
/// path runs east along y = 0 through the bin's interior.
fn scene() -> (Simulation, NodeKey, SceneBodyId) {
    let mut sim = Simulation::new();
    let card = NodeKey::new(0);
    sim.sync_nodes([(card, Point2D::new(-120.0, 0.0))]);
    // The bin: a scene region the card can pass through (scene bodies
    // and nodes do not collide by default), big enough that "inside"
    // is unambiguous.
    let bin = sim.add_scene_body(
        NodeCollider::Square { half: 40.0 },
        Point2D::new(0.0, 0.0),
        (0.0, 0.0),
    );
    (sim, card, bin)
}

/// Drag the card along +x, ticking as we go, calling `on_tick` after
/// each step. At 2.4 units per tick from x = -120, the card is inside
/// the bin's 40-half square between roughly ticks 34 and 66, and at
/// its centre at tick 50.
fn drag(
    sim: &mut Simulation,
    card: NodeKey,
    ticks: u64,
    mut on_tick: impl FnMut(&Simulation, u64),
) {
    for t in 0..ticks {
        let x = -120.0 + (t as f32) * 2.4;
        sim.pin(card, Point2D::new(x, 0.0));
        sim.tick(1.0 / 60.0);
        on_tick(sim, t);
    }
}

#[test]
fn a_pass_through_the_bin_mints_nothing() {
    // The card is dragged straight through and out the far side, and
    // the hand never releases inside. Proposals existed the whole way
    // across; none became a fact, because nothing committed.
    let (mut sim, card, _bin) = scene();
    let mut log: Vec<Placed> = Vec::new();

    let mut proposals_seen = 0u32;
    drag(&mut sim, card, 110, |sim, _| {
        if !sim.containments_of(card).is_empty() {
            proposals_seen += 1;
        }
    });
    sim.unpin(card);
    // The release happens far east of the bin: commitment reads an
    // empty proposal set and mints nothing.
    commit_release(&sim, card, 110, &mut log);

    assert!(proposals_seen > 10, "the path never crossed the bin");
    assert!(log.is_empty(), "a pass-through minted a fact: {log:?}");
}

#[test]
fn a_release_inside_mints_exactly_one_fact() {
    let (mut sim, card, bin) = scene();
    let mut log: Vec<Placed> = Vec::new();

    // Drag to the bin's centre and let go there.
    drag(&mut sim, card, 50, |_, _| {});
    sim.unpin(card);
    commit_release(&sim, card, 50, &mut log);

    assert_eq!(
        log,
        vec![Placed {
            card,
            into: bin,
            at: 50
        }]
    );
}

#[test]
fn the_record_carries_no_trajectory() {
    // The structural half is by construction: `Placed` has no position
    // field to smuggle a float through. The behavioural half: however
    // long the drag wanders, the record's size is the number of
    // commitments, never the number of ticks.
    let (mut sim, card, _bin) = scene();
    let mut log: Vec<Placed> = Vec::new();

    drag(&mut sim, card, 50, |_, _| {});
    sim.unpin(card);
    commit_release(&sim, card, 50, &mut log);
    assert_eq!(
        log.len(),
        1,
        "one commitment, one record, 50 ticks discarded"
    );
}

#[test]
fn facts_replay_with_no_simulation_present() {
    // The live run concludes a membership. Applying its log to a fresh
    // map reproduces that conclusion with no Simulation constructed at
    // all: the fact plane replicates, the felt simulation stays local.
    let (mut sim, card, bin) = scene();
    let mut log: Vec<Placed> = Vec::new();
    drag(&mut sim, card, 50, |_, _| {});
    sim.unpin(card);
    commit_release(&sim, card, 50, &mut log);

    let mut membership = std::collections::HashMap::new();
    for fact in &log {
        membership.insert(fact.card, fact.into);
    }
    assert_eq!(membership.get(&card), Some(&bin));
}

#[test]
fn two_runs_agree_on_the_facts_and_are_never_asked_about_positions() {
    // The projection ruling's test discipline: assert the facts, not
    // the floats. Both runs commit the same record; where the card's
    // body drifts after release is deliberately not compared.
    let run = || {
        let (mut sim, card, _bin) = scene();
        let mut log: Vec<Placed> = Vec::new();
        drag(&mut sim, card, 50, |_, _| {});
        sim.unpin(card);
        commit_release(&sim, card, 50, &mut log);
        log
    };
    assert_eq!(run(), run());
}
