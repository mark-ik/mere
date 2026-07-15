# Host P2P Wiring Plan (meerkat S5)

**Date**: 2026-06-03
**Status**: Draft (for review, no code yet). The detailed elaboration of the
[modular integration plan](2026-06-02_modular_integration_plan.md)'s **S5 (comms
surface + cheap p2p win)**: how the proven p2p substrate (transport + murm's
`SyncedCabal` + tessera's `SyncedMoot`) wires into the meerkat host loop.
**Grounded in**: a read of meerkat's actual loop this session (`main.rs`,
`fetch.rs`, `lib.rs`) against the 2026-06-03 tree, plus the just-landed tessera
store/sync productization ([tessera plan](../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md)).

---

## The one idea

**meerkat already has the exact async-host seam p2p needs.** The off-UI-thread
fetcher is the template: a `tokio` runtime runs background work, outcomes return
over an `mpsc` channel and wake the winit loop via an `EventLoopProxy<()>`, and
`user_event` drains the channel into app state and requests a redraw. `fetch.rs`
even reserves the next user of that seam by name:

> "The winit user-event type stays trivial, so persistence (S3) and **sync (S5)**
> can push their own typed channels through the same wake without fighting over
> one event enum."

So host p2p wiring is not new architecture. It is a second background subsystem
shaped exactly like the fetcher: own a runtime, construct the transport + the
synced lanes on it, drain their sync events to a typed channel, wake the loop,
and fold the result into a chrome-visible status. Wiring against a proven
pattern, not building one.

---

## Findings (grounded in the loop)

1. **The async seam is the fetcher.** `fetch::Fetcher` (`fetch.rs:81-130`) owns a
   multi-thread `tokio::runtime::Runtime`, an `EventLoopProxy<()>`, and `mpsc`
   senders; `spawn` runs work and, on completion, sends an outcome then
   `proxy.send_event(())`. `App::user_event` (`main.rs:647-679`) drains the
   receivers with `try_recv` and updates state. P2P mirrors this exactly: a
   `sync` module with a `SyncHost` that owns its runtime, builds the transport +
   lanes, and delivers a `SyncUpdate` over its own channel + the same wake.

2. **Both lanes are host-composed over the same replication shape.** `murm` and
   `gemot::tessera` supply their stores, folds, and verification; the host builds
   `murm_replication::SyncedSpace` pumps over one injected endpoint and projects
   their real `SyncStatus` values (rounds / items / last-activity). **One
   transport, N lanes.** The earlier domain-specific `SyncedCabal` / `SyncedMoot`
   wrappers retired with the sibling-posture purity split.

3. **The chrome is where status shows.** `Chrome` (`lib.rs`) is the host-neutral
   view-model the runner diffs into the toolbar DOM. A sync indicator gets a
   `Chrome.sync` view-model field (peers / items / last-synced / syncing,
   projected from the real `SyncStatus`), rendered by `chrome_view` as a small
   status chip in the toolbar band. The host owns the mutation (the M4 contract):
   `user_event` folds each drained `SyncUpdate` in via `runner.update`.

4. **The discovery / bootstrap gap is the honest limiter.** The live path has
   **no peer discovery wired into the host**. Sync is discovery-driven (gossip
   `NeighbourUp` triggers RBSR; `initiate_session` is test-only), and the proofs
   (the probes + the two library convergence tests) all use explicit `add_peer` +
   `set_topics`. So a single meerkat instance has no peer and nothing converges.
   The status chip must tell that truth ("0 peers"), never a fake spinner (the
   real-sync-feedback rule). Closing it is a deliberate slice (S5.1), not assumed.

5. **Identity is absent in the host today.** meerkat constructs no keypair. P2P
   needs a master key (the transport's node identity) and persona keys (authoring
   tessera / cabal ops). The `identity` crate has a BLAKE3 vault + passphrase
   backend; the [persona model](../research/2026-05-14_persona_model_brief.md)
   defines `<data_root>/personas/<id>/vault/`. The foundation slice can use an
   ephemeral seeded `InMemoryProvider` (deterministic, distinct seeds per
   instance for a two-instance demo); the persistent vault is a named follow-on
   (S5's "host OS-keychain IdentityStorage" done-condition).

6. **Stores live under the existing session dir.** meerkat already owns
   `<data_dir>/mere` (`session_dir`, holding `graph.json` + `views/`).
   `TesseraStore::open` / `PersistentCabalStore::open` slot under it
   (`moots/<moot_id>.redb`, `cabals/<cabal_id>.redb`). The foundation slice may
   start with `in_memory()` stores (no persistence UX), then move to on-disk.

7. **What is visible is status, not a moot UI.** meerkat has no comms / moot panel
   (modular plan gap #7: "murm/moot unsurfaced"). The foundation slice surfaces a
   sync-status chip, not a posts list or a member / reputation roster. A real
   comms surface is a larger, separate slice (S5.3). The plan must not overclaim a
   "comms surface" from the foundation.

8. **The modular plan's inventory is now stale for moothold.** Its §3 lists
   moothold as "stub + tessera Phase 1" and gap #7 predates the store/sync work.
   Tessera is now Phases 1-5 + a productized store + `SyncedMoot` (60 tests). The
   modular plan's S5 should cite this plan; that reconciliation rides here.

---

## The slices (done-conditions, not dates)

- **S5.0 — P2P foundation in the host.** Add deps (`identity`, `transport`, and
  `moothold` and/or `murm`). Build a `sync` module mirroring `fetch`: a `SyncHost`
  owning a `tokio` runtime that constructs an ephemeral-seeded identity + one
  `P2pandaTransport` (bound on the runtime), holds a synced lane
  (`SyncedMoot` or `SyncedCabal`) over an in-memory store, and drains its
  `SyncStatus` to an `mpsc` channel + `proxy.send_event(())`. `user_event` drains
  it (alongside the fetch channels) and folds it into a `Chrome.sync` view-model;
  `chrome_view` renders a real status chip. **Done**: meerkat boots, the transport
  binds, the lane joins a demo moot/cabal, and the chip shows the real status
  (honestly "0 peers" pre-bootstrap). Headless tests for the `SyncHost`
  construction + the `SyncUpdate -> view-model` fold; the chip is GUI.

- **S5.1 — Bootstrap to real convergence.** Give the host a way to learn a peer.
  Two options (pick one, or do the manual one first):
  - *Manual*: a command-palette verb / CLI flag / config file carrying a peer
    `EndpointAddr` + topic; the host calls `add_peer` + `set_topics`.
  - *Discovery*: wire the transport's existing mDNS / random-walk local discovery
    (per the [p2panda spike](2026-06-01_p2panda_substrate_spike_plan.md)) so
    same-LAN instances auto-find. More magical, slightly bigger.
  **Done**: two meerkat instances converge (the library convergence proof, now in
  the running host); the chip shows items caught up + last-synced.

- **S5.2 — Persistent identity + stores.** Swap the ephemeral provider for the
  `identity` vault (seed-file or passphrase under `<data_root>/personas/...`);
  open `TesseraStore` / `PersistentCabalStore` on disk under the session dir.
  **Done**: identity + the tessera / cabal log survive restart.

- **S5.3 — A real comms / moot surface (the larger arc, S5 proper).** A panel: a
  cabal posts list (authored through the host) and / or a moot member + tessera
  reputation readout (`SyncedMoot::ledger(...).scores(now)`). This is where gap #7
  closes. Touches the chrome's content model / S4 workbench panes; flagged as its
  own milestone, not part of the foundation.

---

## Decisions to surface (Mark's call)

1. **First lane for S5.0: tessera or murm?** Identical host shape; the difference
   is what you see first. Tessera surfaces reputation / sync-status (freshest in
   mind, the chip is the natural readout); murm surfaces chat posts (a more
   familiar "it synced" demo, but wants a posts surface sooner). Recommend tessera
   for S5.0 (the chip is its natural face), murm following on the same foundation.
2. **Bootstrap style for S5.1: manual vs mDNS discovery.** Manual is smaller and
   fully in our control; mDNS is more magical but pulls the discovery wiring
   forward. Recommend manual first (demoable on loopback today), mDNS as a
   follow-on.
3. **Identity for S5.0: ephemeral seed vs persistent vault.** Ephemeral unblocks
   the foundation with no passphrase UX; the vault is S5.2. Recommend ephemeral
   first.

---

## Pitfalls

- **Keep the wake trivial.** The winit user-event stays `()`; p2p adds its own
  typed `sync_rx` channel drained beside the fetch channels (the design `fetch.rs`
  already anticipated). Do not grow a user-event enum.
- **Nothing p2panda touches the UI thread.** All transport / LogSync work runs on
  the `SyncHost` runtime; only the drained view-model fold runs in `user_event`.
- **`fold_moot` is sync redb work.** Cheap for a small moot on the UI thread; for
  a large one, fold on the sync runtime and deliver the scores in the
  `SyncUpdate`, or `spawn_blocking`. A perf follow-on, not a foundation blocker.
- **Honest status only.** The chip shows real peers / items / last-synced; "0
  peers / not synced" is the truth before bootstrap, never a placebo spinner.

---

## Progress

- **2026-06-03** — Plan drafted (no code). Mapped meerkat's loop (`main.rs` /
  `fetch.rs` / `lib.rs`) and found the fetcher's async seam is the exact template
  p2p needs (and `fetch.rs` already reserves "sync (S5)" through the same wake).
  Confirmed both lanes (`SyncedCabal`, `SyncedMoot`) are host-ready and identical
  in shape over one transport's `sync_parts`. Surfaced the honest discovery /
  bootstrap gap (a single instance has no peer; the chip must say so), the
  identity-absence (ephemeral seed first, vault later), and the store placement
  (under the existing session dir). Sliced S5 into S5.0 foundation -> S5.1
  bootstrap-to-convergence -> S5.2 persistence -> S5.3 the real comms surface, and
  posed three decisions (first lane, bootstrap style, identity). Next: Mark's
  steer on the three decisions, then S5.0.
- **2026-06-03 — S5.0 landed: the p2p foundation stands up in the host (tessera
  lane, Mark's pick).** Mirrors the fetcher seam exactly. New
  [`sync`](../../../crates/meerkat/src/sync.rs) bin module: a `SyncHost` owns a
  tokio runtime, binds one `P2pandaTransport` (ephemeral seeded identity), joins
  a tessera `SyncedMoot` over an in-memory `TesseraStore`, and a 1s status poller
  pushes each `SyncStatus` change over an `mpsc` channel + an `EventLoopProxy`
  wake. `user_event` drains it (beside the fetch channels) and folds it into
  `Chrome.sync`, a new host-neutral
  [`SyncIndicator`](../../../crates/meerkat/src/sync_indicator.rs) view-model
  rendered as an honest status chip in the toolbar band (`p2p off` → `tessera:
  idle` → `tessera: syncing` → `tessera: N ops`). Setup failure disables p2p,
  never the shell. **Runtime-verified**: the built shell boots and logs `p2p sync
  up: joined tessera demo moot` (~1.3s to bind + join), so the transport binds a
  real endpoint and the lane joins its LogSync session in the live process; the
  chip reads the true `tessera: idle` (a lone instance has no peer yet, S5.1).
  meerkat 56 tests (39 lib + 17 bin) pass, no new clippy warnings. The substance
  went into new modules (`sync`, `sync_indicator`); `main.rs` / `lib.rs` gained
  only minimal wiring. *Flagged* (DOC_POLICY §9): meerkat's `main.rs` and `lib.rs`
  were already over the 600-LOC ceiling before this work — a split is a worthwhile
  cleanup follow-on, kept out of S5.0's scope. *Next*: S5.1 (manual peer-add → two
  instances converge) makes the `N ops` state real.
- **2026-06-03 — S5.1 core landed + proven (the ticket connect path); meerkat verb
  UI held for coordination.** Bootstrap mechanism chosen with Mark: a
  command-palette **verb** (over CLI flag / config file). Built + verified the
  hard, host-neutral core in `transport` + `moothold` (crates not under concurrent
  edit). `transport::P2pandaTransport::ticket()` serializes this node's dialable
  `EndpointAddr` to a shareable base32 string (iroh `EndpointTicket`), and
  `add_peer_ticket(&str)` parses a peer's ticket, registers its transport info,
  and returns its `PeerID` to tag on the overlay — the string-friendly bootstrap a
  "connect to peer" verb needs (28 transport tests, clippy clean). A moothold test
  `two_moots_converge_bootstrapped_by_ticket` proves the **full connect path end to
  end, headless**: two moots bootstrap purely by exchanging ticket strings
  (`ticket` → `add_peer_ticket` → `set_topics(overlay)`) and converge on the same
  tessera score in ~3s (61 moothold tests, clippy clean). **The meerkat verb UI is
  held**: meerkat's `main.rs` is under concurrent edit (the S3.2c eidetic content
  cache landed deps + fields this session), so wiring the verb (a
  `Command::ConnectPeer`; the chrome-records-intent / host-executes-effect routing;
  distinct per-instance node seeds; startup tessera authoring so there is data to
  sync; `SyncHost::connect` / `my_ticket`) is deferred to avoid clobbering that
  work — to be done once S3.2c settles or coordinated with it. The connect path the
  verb will call is already proven, so the remaining work is host plumbing, not
  protocol. Note: the final two-instance demo is interactive (drive two shells'
  palettes), so it is Mark's to run; the headless test is the convergence proof.
- **2026-06-03 — S5.1 complete: the "connect to peer" verb is wired in the host**
  (Mark gave the go-ahead once S3.2c had landed). The verb is `Command::ConnectPeer`
  ("Connect to peer (ticket in address bar)") in meerkat's palette. Routing follows
  the chrome-records-intent / host-executes pattern — and the actor-constellation
  framing, where it is an outbound command to the sync I/O actor:
  `Chrome::run_command(ConnectPeer)` captures the address-bar text as the ticket
  into `Chrome.pending_connect`; the host drains it after either palette-run path
  (Enter or click) via `App::drain_pending_connect` and calls
  `SyncHost::connect(ticket)`, which parses the ticket, registers the peer, and
  tags it on the moot's overlay topic (the proven path). The `SyncHost` now uses a
  **random per-launch identity** (`InMemoryProvider::random`, so two instances are
  distinct peers), **authors a starter `commit -> fulfil -> govern` tessera log**
  on launch (so a connecting peer has something to catch up), and **logs its
  dialable ticket** at startup to hand out. **Runtime-verified**: the host boots
  and logs `p2p sync up ... ticket=endpoint...` (~1.3s); the ticket is a real iroh
  `EndpointTicket`. meerkat 56 tests pass (the palette-row + step-wrap tests
  updated for the 4th command), no new clippy warnings. **The two-instance demo**
  (Mark's to run): launch two shells, copy instance A's ticket from its startup
  log, paste it into instance B's address bar, Ctrl+K → "Connect to peer" → both
  chips move `tessera: idle` → `tessera: 3 ops` as the logs converge. S5.1 done;
  S5.2 (persistent identity + on-disk stores) and S5.3 (a real moot / comms
  surface) remain.
- **2026-06-04 — S5 re-scoped against the real tree; ThinkPad L14 (Fedora 44)
  designated the second peer.** Re-confirmed in code that S5.0 + S5.1 are built and
  wired (`sync.rs` `spawn_sync` / `SyncCommand::Connect`; `command.rs`
  `Command::ConnectPeer`; `lib.rs` `pending_connect`; `main.rs`
  `drain_pending_connect`), with random per-launch identity, in-memory
  `TesseraStore`, and a starter log. (The DOC_README summary calling S5 "no code
  yet" was stale.) **Immediate gate, Mark's to run: cross-machine validation** of
  the built ConnectPeer path across the Windows laptop and the Fedora L14 over the
  LAN — the first cross-machine, cross-OS convergence (prior proofs were headless /
  loopback), which also validates the full meerkat GUI build on Fedora 44. Steps:
  `cargo run -p meerkat` on both; copy A's startup `ticket=...`; paste into B's
  address bar; Ctrl+K → "Connect to peer"; both chips move `tessera: idle →
  tessera: 3 ops`. Gotchas: same-LAN direct dial from the ticket's `EndpointAddr`
  (firewall may need the UDP port opened on both OSes; cross-network would need
  relay/hole-punch, not needed on one LAN). **Next build, S5.2 (persistent identity +
  on-disk stores):** in `sync.rs`, swap `InMemoryProvider::random()` for a
  seed-file-backed identity under the data dir (stable peer identity across
  restarts) and `TesseraStore::in_memory()` for
  `TesseraStore::open(<session_dir>/moots/<moot>.redb)` (log survives restart).
  Done-when: the L14 restarts as the same peer with its log intact and reconnects
  without re-authoring. Small; stays in `sync.rs`, under the ceiling. S5.3 (real
  moot surface) after.
- **2026-06-05 — S5.2 landed: persistent identity + on-disk tessera store
  (`sync.rs` only).** Identity is now a 32-byte seed file at
  `<data>/mere/node_identity.seed`, minted on first launch and reused after (a
  stable iroh node id across restarts) via
  `InMemoryProvider::from_seed(load_or_create_seed(..))`; a missing / wrong-size /
  unwritable seed falls back to ephemeral for that launch and warns, never fatal.
  The tessera log moved on disk to
  `TesseraStore::open(<data>/mere/moots/<moot>.redb)` (one redb file per moot), and
  the starter `commit -> fulfil -> govern` log is authored only when the store is
  empty (first launch), so a restart reuses the persisted log instead of
  re-authoring. Fully contained to `sync.rs` (new helpers `data_dir` /
  `load_or_create_seed` / `hex32`; the `spawn_sync` signature is unchanged, so
  `main.rs` / `lib.rs` were untouched, honoring the collision-wariness ask while
  S3.2c / lib edits are in flight). No new dependency: the seed is minted from a
  throwaway `InMemoryProvider::random()`'s `to_seed()`, and `dirs` was already a
  dep. **Verified:** `cargo check -p meerkat` clean (no new warnings in meerkat
  src); meerkat bin tests 26/26 pass, including two new sync tests
  (`hex32_is_64_lowercase_hex`, `the_node_seed_is_stable_across_calls`). Loopback
  convergence was proven live earlier this session (the chip moved to
  `tessera: 3 ops`). **Still Mark's to run:** the cross-restart + cross-machine
  demo on the L14 (restart as the same peer, log intact, reconnect without
  re-authoring), which also exercises the Fedora GUI build + LAN path. *Note (not
  S5.2):* the meerkat *lib* tests `back_click_navigates_history` and
  `palette_filters_and_runs_command` fail against the uncommitted `lib.rs` edits
  (toolbar `can_go_back`; a palette run landing on `mere://welcome` not
  `https://example.com`); bin-only change, unrelated to `sync.rs`, flagged for the
  in-flight lib work. S5.3 (real moot surface) after.

The minimal seed file is a known interim: a plaintext secret on disk. The
passphrase-encrypted persona vault (`<data_root>/personas/<id>/vault/`) is the
follow-on that supersedes it (the `identity` crate already has the vault +
passphrase backend).
