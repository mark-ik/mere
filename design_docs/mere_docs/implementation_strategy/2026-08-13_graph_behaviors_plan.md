# Graph Behaviors Plan: watches, cascades, and the reactive denizen

**Date:** 2026-08-13
**Status:** open. Designed with Mark 2026-08-13 (the "neat lil ideas"
conversation: one node triggering others nearby to refresh, summaries from
connected nodes captured into a knot note, and the family of automations
behind them).
**Related:**
[participant gate + packs](2026-07-17_participant_gate_packs_plan.md) (this
plan extends it: a behavior IS a denizen, plus a trigger),
[scriptable field regions](2026-06-13_scriptable_field_regions_plan.md)
(owns the projection tier; not re-planned here),
[runtime mod authoring loop](2026-06-30_runtime_mod_authoring_loop_plan.md)
(the authoring ergonomics behaviors inherit),
[execution-ordering prior art brief](../../2026-08-13_execution_ordering_prior_art_brief.md)
(the wavelet read; its "no site today" for the Trigger/Observe split is the
site this plan builds),
[data-oriented doctrine brief](../../2026-07-02_data_oriented_doctrine_brief.md)
(change is a recorded delta stream, which is what triggers ride).

---

## 1. The ruling this plan rests on

**Two tiers, split by one test: does it write graph truth?**

- **Projection rules** read truth and change only how things look or arrange:
  nearby-refresh, staleness badges, rollup displays, edge visibility. They are
  recomputed, not committed, so they need no gate, no attribution, and no
  cascade discipline. The scriptable field regions plan already owns this
  tier (a placed spatial region carrying rhai rules); "refresh what is near
  this node" is one more rule kind there. This plan does not build any of it.
- **Graph behaviors** write truth. A behavior is a **denizen with a trigger**:
  the identity, grant, petition, and attributed-journal machinery from the
  participant gate plan, plus one new thing: a standing subscription that
  runs the body when something it watches changes, instead of waiting to be
  invoked. The gate answers "may it write"; this plan answers "when does it
  run".

Everything the gate plan ruled carries over unchanged: one authority model,
petitions through `servitor::Gate`, no raw positions (spatial influence goes
through fields, never petitions), auditability plus compensating actions
rather than universal undo.

**The scripting question is not open.** The lanes exist and are placed:
rhai for privileged local automation (the omnibar `>`-shell, knot
note-block eval), **piccolo Lua for sandboxed denizen bodies** (turnstone
`src/denizen.rs`: `.lua` control scripts under a step budget, bindings
derived from the denizen's authority in `src/component.rs`), Wasm components
for portable untrusted mods (the ring-gated `app-core` world, B3). A
behavior body is whatever the denizen already runs; this plan adds no
language. Note the 2026-06 four-lane map predates the piccolo lane; the
memory-level map should read five.

## 2. Findings (code-verified substrate)

Every site below was read this session, not recalled.

- **The delta spine exists and is attributed.**
  [`graph-kernel/src/graph/journal.rs`](../../../crates/graph/graph-kernel/src/graph/journal.rs):
  `GraphJournal` is a `codicil::Codicil<AttributedDelta>` where
  `AttributedDelta { author, delta: CapturedDelta }`; `author` is `user`,
  a denizen subject's hex, or `pre-gate`. It carries `Seq`, `live_cursor()`,
  `replay_from(since, graph)`, and `record_as(author, delta)`. A trigger
  consumer is a cursor-holding reader of this tail. Wasm-clean by its own
  module doc.
- **The commit receipt and the consumer discipline exist.**
  [`chartulary/src/commit.rs`](../../../crates/eidetic/chartulary/src/commit.rs):
  `commit_batch(author, expected, specs) -> Committed { batch: BatchId, .. }`,
  and the test `effects_enqueue_only_after_a_commit_lands` demonstrates the
  post-commit effect pattern (effects carry the batch id, enqueue only after
  a landed commit). The nested-graph (denizen-world) side of triggering
  follows that discipline as designed.
- **The gate is built.**
  [`servitor/src/gate.rs`](../../../crates/servitor/src/gate.rs):
  `Gate::petition(provider, nested, subject, claimed, expected, specs)` runs
  projection-guard, authority (`AuthorityProvider::covers(subject, cap,
  Mode::Write)`), per-node scope containment, then an attributed
  revision-checked commit. `Cap::{Power, Scope}` with segment-prefix scope
  coverage is in `cap.rs`; delegation rides personae signed certs.
- **The runnable body and its budget exist.** turnstone
  `src/denizen.rs`: the node IS the denizen (binding facet + nested world),
  `RunDenizen { member }` runs a piccolo control script under a step budget,
  and `src/ring.rs` checks every emitted Action's ring at emission.
  `src/component.rs` derives `ScriptCapabilities` from the denizen's
  authority. Subjects are content-derived (`blake3(source)`), so an edited
  body is a new subject facing fresh review.
- **The app-tier event stream exists and names this consumer.** turnstone
  [`src/observe.rs`](../../../../turnstone/src/observe.rs): `AppEvent` +
  `Snapshot`, drained each frame, with the module doc stating that later
  automation consumers "subscribe at the same drain".
- **What does not exist:** any way for a denizen to run other than explicit
  invocation. No watch registration, no delta matching, no cascade
  discipline, no time source for behaviors. That is the whole gap, and it is
  narrow.

## 3. Design

### 3.1 A watch is a scope, and the scopes nest

A behavior declares a **watch**: the region of the graph whose changes wake
it. The watch is a `Cap::Scope`, the same vocabulary as its grant, with one
containment law enforced at registration:

```text
watch scope ⊆ read scope ⊆ grant
```

You cannot be woken by what you cannot read. This is where the wavelet
brief's Trigger/Observe split actually lands, not as edge kinds on the data
graph but as the two capability scopes of a behavior: watch = trigger, read
beyond watch = observe. The data graphs stay declarative; agency lives
entirely in the denizen tier.

(Vocabulary: **watch** ruled by Mark 2026-08-13, recorded in
[TERMINOLOGY.md](../../TERMINOLOGY.md).)

### 3.2 Triggers ride the journal, not edges

A watch matches **committed, attributed deltas**, never live mutation. The
matcher holds a per-watch cursor into `GraphJournal` and, at each drain,
tests entries `(cursor, live_cursor]` against the watch scope. Two built-in
refusals:

- **Self-authored entries never match the author's own watch.** The trivial
  self-loop is unrepresentable rather than budgeted.
- Entries authored `pre-gate` match normally (they are ordinary history by
  the time a watch exists).

App-tier watches (on `AppEvent`, e.g. "a session switched") are the same
shape at the observe drain, and land in a later slice; the graph tier comes
first because attribution and scopes already exist there.

**Watches are reviewed at install** (ruled 2026-08-13): they ride the pack
manifest beside the rings, so the review screen shows *when this runs* next
to *what it may touch* before either is granted. A watch added after
install is a widening and re-reviews, the same posture pack upgrades
already have. Registration still enforces the containment law regardless of
how the watch arrived.

### 3.3 The cascade runner

Firing a behavior produces petitions; petitions commit; commits can wake
other watches. That chain is a **cascade**, and it runs at the existing
after-dispatch drain, post-commit, per the chartulary consumer discipline:

1. Collect journal entries since each watch's cursor; compute the wake set.
2. Run woken behaviors **in stable subject order** (sorted subject hex),
   each as the existing `RunDenizen` lane with a trigger context supplied
   (the matched deltas, digested).
3. Their petitions commit through the gate as today, attributed to their
   subjects. Advance cursors.
4. Repeat from 1 with the new entries, up to the **cascade budget** of
   rounds. The budget is a setting (ruled 2026-08-13, per the
   configurability doctrine): default 4, live-switchable, floor 1; there is
   no "unlimited" value, because an unbounded cascade is the failure mode
   the budget exists to name. Exhausting it is loud: an `AppEvent` and a
   visible notice naming the behaviors still waking each other, per the
   no-silent-caps rule.

Termination is structural: finite rounds times a per-run step budget, plus
self-authored entries never re-waking their author. Determinism comes from
stable ordering plus single-threaded execution, which keeps cascades
scenario-replayable. This is the entire "scheduler": a bounded drain loop,
not a framework. Depth queues, backward-scheduling errors, and the rest of
the wavelet apparatus stay unbuilt unless cascades in practice grow real
dependency structure.

### 3.4 Time, later, honestly

"Untouched for a month", "archive orphans weekly" need a clock. Behaviors
get time only as a **watchable source injected at the drain** (the host's
clock today, a test/replay clock in harnesses), never by reading wall time
in a body, so a replayed session wakes the same behaviors at the same
points. This is the wavelet brief's replay-clock idea landing where it
earns its keep. Its own slice, last, because nothing above depends on it.

### 3.5 What a behavior may not do

Carried over or ruled here:

- No raw positions in, no positions out ("nearby" belongs to the projection
  tier or to a host-answered field query; gate-plan ruling, unchanged).
- No watch on another denizen's grant projections (the gate already refuses
  writes there; watches are refused symmetrically, so authority changes
  cannot be used as a signal channel).
- Notifications emitted by behaviors are real or absent, never decorative
  (the no-placebo rule).
- Derived indexes (cited-by counts, backlink maps) are not behaviors. The
  doctrine already classes reverse maps as indexes owned by the kernel;
  writing them as truth via automation would mint divergence.

## 4. Slices

- **W0: watch table and matcher (headless, kernel-side).** A watch registry
  keyed by subject (`Cap::Scope` + cursor), persisted beside the denizen
  binding; a matcher over `GraphJournal` entries. Done when: registration
  enforces watch ⊆ grant read scope; scope-prefix matching proven by test;
  self-authored entries proven non-waking; cursors survive reload.
- **W1: cascade runner at the drain (turnstone).** The bounded rounds loop
  of 3.3 wired to the existing after-dispatch drain and `RunDenizen` lane.
  Done when: a two-behavior mutual-wake fixture terminates at the budget
  with the loud event on screen; a linear A-wakes-B chain settles in one
  cascade; every behavior-authored entry in the journal reads back with the
  right subject; the whole cascade replays deterministically in a scenario
  run; lowering the budget setting from 4 to 1 takes effect on the next
  cascade without a restart.
- **W2: trigger context into the body.** The matched-delta digest handed to
  the piccolo body (and the wasm envelope, same shape as an Action payload);
  bindings stay capability-derived. First product behavior: an **inbox
  rule** (a node appearing under a watched scope is filed/tagged by
  petition). Done when: the install review screen shows the watch beside
  the rings before anything is granted; the inbox rule runs headed; its
  edit is attributed in the inspector; and uninstalling the denizen removes
  the watch with it.
- **W3: app-tier watches.** `AppEvent` watches at the observe drain, ring
  vocabulary unchanged. Done when: a behavior wakes on `SessionSwitched`
  without polling, and the scenario log shows the wake attributed.
- **W4: time watches.** The injected time source of 3.4 plus a test clock.
  Done when: a "stale scope" behavior fires under a test clock stepped past
  the threshold, fires identically on replay, and never fires from a body
  reading wall time (no such binding exists).
- **W5: the flagship: neighborhood summary into a knot note.** Read scope
  over a container's members, write scope over one note node, waking on
  member changes; summarization via a host-provided capability (the intel
  seam: esp is the stated successor to vates/sibylla, so the capability
  fronts esp; a plain-text digest body is the fallback if the intel seam is
  not ready, and the behavior shape is identical). Done when: editing a
  watched node updates the summary note within one cascade, the note's
  history shows every update attributed to the summarizer, and revoking the
  grant stops updates with the standing note intact.

## 5. The candidate catalog

The "ten neat lil ideas", each placed. Tier: **P** = projection rule (field
regions plan), **B** = graph behavior (this plan), **I** = index (kernel,
not automation).

| idea | tier | trigger | needs |
|---|---|---|---|
| nearby nodes refresh | P | spatial region | field regions plan |
| neighborhood summary into a knot note | B | delta (W5) | intel capability |
| inbox rule: new node under scope → file/tag | B | delta (W2) | nothing new |
| staleness badge on old nodes | P | time | projection + clock |
| gardening: petition to archive orphans | B | time (W4) | compensating action |
| auto-tag on content | B | delta | intel capability |
| watch-and-notify | B | delta | honest notify capability |
| rollup counts displayed on a container | P | delta | projection only |
| rollup written back as node content | B | delta | scope discipline |
| template expansion on drop | B | delta | nothing new |
| cited-by / backlink counts | I | not triggered | kernel index, never a behavior |
| cross-app: woodshed practice event → knot journal line | B (far) | app event | murm lane; out of scope here |

## 6. Non-goals

- Operative edges or edge-kind variants in any data or display graph. The
  2026-08-13 brief's retraction stands; agency lives in denizens.
- A general scheduler. The cascade runner is a bounded drain loop; wavelet's
  depth machinery waits for evidence of need.
- New scripting languages or lanes. rhai / piccolo / wasm as placed.
- Spatial influence through petitions (numen fields remain the path).
- Universal undo (gate-plan posture: attribution + compensating actions).
- Cross-app and peer-carried behaviors (the pack/moot distribution story
  already covers how a behavior travels; wiring its *triggers* across murm
  or retinue is its own future plan).

## 7. Rulings (all three open questions closed 2026-08-13)

1. **The noun is watch.** Recorded in TERMINOLOGY.md beside denizen and
   petition.
2. **Watches are reviewed at install**, in the pack manifest beside the
   rings; post-install additions are widenings and re-review (folded into
   3.2).
3. **The cascade budget is a setting**: default 4 rounds, live-switchable,
   floor 1, no unlimited value (folded into 3.3). W1's done conditions gain
   one clause: changing the setting takes effect on the next cascade
   without a restart.

## Progress

- 2026-08-13: plan written. Substrate grounded against journal.rs,
  commit.rs, gate.rs/cap.rs, denizen.rs/component.rs/ring.rs, observe.rs,
  all read this session. No code yet.
- 2026-08-13 (later): Mark ruled the three open questions (watch / install
  review / budget-as-setting); rulings folded into 3.1, 3.2, 3.3 and
  TERMINOLOGY.md. W1 and W2 done conditions extended accordingly.
