/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Constellation tests.

use uuid::Uuid;

use super::*;

fn m(n: u128) -> GraphMemberId {
    Uuid::from_u128(n)
}

fn graph(n: u128) -> GraphId {
    GraphId(Uuid::from_u128(n))
}

/// A fixed graph id for the reconcile tests — they exercise spawn / warmth /
/// LRU eviction, all graph-agnostic, so one stamp serves.
fn g() -> GraphId {
    graph(0)
}

fn noop_wake() -> Wake {
    std::sync::Arc::new(|| {})
}

fn fake_activation(rx: std::sync::mpsc::Receiver<ContentUpdate>, graph_id: GraphId) -> Activation {
    let (handle, _updates) = armillary::spawn(
        noop_wake(),
        |commands: std::sync::mpsc::Receiver<ContentCommand>,
         _out: armillary::Emitter<ContentUpdate>| { while commands.recv().is_ok() {} },
    );
    Activation {
        handle,
        rx: ContentUpdateStream::Native(rx),
        gens: Generations::default(),
        shown: None,
        engine: String::new(),
        scene: None,
        packet: None,
        fonts: FontTable::default(),
        masks: Vec::new(),
        content_height: 0,
        band: (0, 0),
        requested_band: (0, 0),
        links: Vec::new(),
        find_matches: Vec::new(),
        find_query: String::new(),
        scene_version: 0,
        background: false,
        last_touched: 0,
        respawns: 0,
        graph_id,
    }
}

fn scene_update(content_height: u32) -> ContentUpdate {
    let gens = Generations::default();
    ContentUpdate::Scene {
        nav: gens.nav,
        viewport_gen: gens.viewport,
        scene: Scene::new(1, 1),
        content_height,
        band_y: 0,
        band_h: 1,
        links: Vec::new(),
        masks: Vec::new(),
    }
}

#[test]
fn reconcile_keeps_tabs_warm() {
    let mut c = Constellation::new(noop_wake());
    c.reconcile(&[(m(1), g()), (m(2), g())]);
    assert_eq!(c.active_count(), 2, "two needed nodes spawned");
    c.reconcile(&[(m(2), g())]); // m(1) is no longer needed...
    assert!(
        c.is_active(m(1)),
        "...but stays a warm tab — no reap on blur"
    );
    assert_eq!(c.active_count(), 2);
}

#[test]
fn reconcile_evicts_least_recently_touched_over_cap() {
    let mut c = Constellation::new(noop_wake());
    c.set_cap(2);
    c.reconcile(&[(m(1), g())]); // touch 1
    c.reconcile(&[(m(2), g())]); // touch 2 — m(1) is now the stalest
    c.reconcile(&[(m(3), g())]); // touch 3, over the cap of 2 → evict the stalest evictable
    assert_eq!(c.active_count(), 2, "the cap holds");
    assert!(
        !c.is_active(m(1)),
        "the least-recently-touched, non-needed tab is evicted"
    );
    assert!(c.is_active(m(2)) && c.is_active(m(3)));
}

#[test]
fn a_background_tab_is_exempt_from_eviction() {
    let mut c = Constellation::new(noop_wake());
    c.set_cap(1);
    c.reconcile(&[(m(1), g())]);
    assert!(
        c.set_background(m(1), true),
        "flagging an active node succeeds"
    );
    c.reconcile(&[(m(2), g())]); // over the cap of 1, but m(1) is background → not evictable
    assert!(c.is_active(m(1)), "a background tab survives cap pressure");
    assert!(c.is_active(m(2)), "the needed node is still spawned");
    assert!(
        !c.set_background(m(3), true),
        "flagging a dormant node reports false"
    );
}

#[test]
fn teardown_before_batch_drain_does_not_strand_sibling_update() {
    let mut c = Constellation::new(noop_wake());
    let (removed_tx, removed_rx) = std::sync::mpsc::channel();
    let (survivor_tx, survivor_rx) = std::sync::mpsc::channel();

    c.active.insert(m(1), fake_activation(removed_rx, graph(1)));
    c.active
        .insert(m(2), fake_activation(survivor_rx, graph(2)));

    removed_tx.send(scene_update(10)).unwrap();
    survivor_tx.send(scene_update(20)).unwrap();

    c.reap_graph(graph(1));
    let drained = c.drain();

    assert!(
        drained.any_scene,
        "the surviving owner's scene was accepted"
    );
    assert!(!c.is_active(m(1)), "the torn-down owner stays reaped");
    assert_eq!(
        c.scene_version(m(2)),
        1,
        "sibling update is not stranded behind a cross-owner batch"
    );
    assert_eq!(c.content_height(m(2)), 20);
}

#[test]
fn respawn_replays_the_tab_and_caps_the_storm() {
    let mut c = Constellation::new(noop_wake());
    c.reconcile(&[(m(1), g())]);
    c.drive(
        m(1),
        "mere://welcome",
        None,
        100,
        100,
        DocumentStyleSheet::default(),
        "serval.web",
    ); // gives it a `shown` state
    assert!(c.active.get(&m(1)).unwrap().shown.is_some());
    // A respawn replaces the actor and clears `shown` so the next drive re-Shows.
    assert!(c.respawn(m(1)));
    assert!(
        c.active.get(&m(1)).unwrap().shown.is_none(),
        "shown cleared for replay"
    );
    assert_eq!(c.active.get(&m(1)).unwrap().respawns, 1);
    // The cap stops a storm: MAX_RESPAWNS respawns, then give up.
    assert!(c.respawn(m(1))); // 2
    assert!(c.respawn(m(1))); // 3
    assert!(
        !c.respawn(m(1)),
        "past MAX_RESPAWNS the pool leaves the tab on its last scene"
    );
    // A respawn on a node that is not active is a no-op.
    assert!(!c.respawn(m(99)));
}
