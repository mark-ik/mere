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

**Which graphs a scope can name (found building W0).** Segment-prefix
matching fits a denizen's nested world, whose node ids *are* scope paths:
the same strings `Gate::petition` already scope-checks. Mere's main graph
keys nodes by `Uuid`, and a UUID is one opaque segment, so against that
journal a `ScopePath` can only ever name one exact node or (via the root)
everything. Watching a *region* of the main graph therefore needs a region
vocabulary that does not exist yet. This does not block the matcher, which
takes events rather than a journal, but it does gate W2: see W0.5.

### 3.2 Triggers ride the journal, not edges

A watch matches **committed, attributed deltas**, never live mutation. The
matcher holds a per-watch cursor into `GraphJournal` and, at each drain,
tests entries `(cursor, live_cursor]` against the watch scope. Two built-in
refusals:

- **Self-authored entries never match the author's own watch.** The trivial
  self-loop is unrepresentable rather than budgeted. One trap here, found
  building W0: the two journals label the same subject differently.
  chartulary carries `denizen:abcd1234` (`Subject::to_author`), while mere's
  `GraphJournal` carries the full 64-char hex (turnstone
  `remote_projection.rs`). A derived label would be right on one tier and
  quietly wrong on the other, and being wrong means this refusal stops
  working while the budget silently absorbs the spin. So the label is a
  registration argument: a caller states which convention its journal uses.
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

- **W0: watch table and matcher (headless, kernel-side). LANDED
  2026-08-13.** Built in `servitor` (`src/watch.rs`), which already owns
  `Cap`, `Grant`, and the `AuthorityProvider` seam a watch is contained by.
  `Watch { subject, scope, self_author, cursor }`, a `WatchTable` whose
  `register` enforces the containment law against
  `AuthorityProvider::covers(.., Mode::Read)`, and a `wake(events)` matcher
  returning wakes in stable subject order. Persistence is servitor's own
  `to_wire`/`parse` idiom rather than a serde dependency (space-separated,
  scope last, because a scope segment may contain a colon and `Cap`'s wire
  form could not carry one unambiguously). The matcher takes `WatchEvent`s
  rather than a journal type, so it serves both tiers.
- **W0.5: the main graph's region vocabulary. RULED: container membership
  (Mark, 2026-08-13).** Containment is already real in the kernel:
  `EdgeFamily::Containment` with seven sub-kinds (`UrlPath`, `Domain`,
  `FileSystem`, `UserFolder`, `ClipSource`, `NotebookSection`,
  `CollectionMember`), so this is a vocabulary the graph has rather than one
  to invent. The design that follows: **a node's watch scope is its
  containment ancestry**, written as a `ScopePath` of UUIDs
  (`containerUuid/memberUuid/...`). Segment-prefix coverage then means what
  it should with no change to `Cap`: a watch naming a container covers
  everything under it, transitively, and a watch naming the full path covers
  exactly one node. Two details the implementation settles, neither a
  blocker: a node reachable by several containment sub-kinds has *several*
  ancestry paths, which `WatchEvent.scopes` already accommodates by being a
  slice; and a node removed by the delta being matched has no ancestry left
  to compute, so it falls back to its own id as a single segment (matching an
  exact-node watch or the root, and nothing else). The adapter lives host-side
  in turnstone, which has both the graph and the journal; putting it in
  graph-kernel would drag personae's crypto into a wasm-clean crate through
  servitor. Unblocks W2.
- **W1a: the cascade runner itself (headless). LANDED 2026-08-13.** The
  bounded rounds loop of 3.3, in `servitor::cascade`: `CascadeBudget` (a
  setting, floored at 1, no unlimited value), `run_cascade` taking a
  host-supplied runner closure so no body-running lives here, and a
  `Cascade { rounds, outcome }` whose `BudgetExhausted` names the subjects
  still waking each other. Exhaustion **defers** rather than consumes: the
  naming peek (`WatchTable::would_wake`) advances no cursor, so work a
  cascade ran out of budget for is still there on the next drain.
- **W1b: wire it to the drain (turnstone). MOSTLY LANDED 2026-08-13.**
  `src/behaviors.rs` is the adapter and the drain: `touched_ids` (exhaustive
  over all 44 `CapturedDelta` variants, so a new one fails to compile until
  classified), `ancestry_scopes` (W0.5's containment walk), `entries_since`,
  and `drain`. `App::update` now splits into `dispatch` plus the drain, so a
  woken body sees the world the action left. Woken subjects run through
  `run_denizen_for_cascade`, which is the ordinary `RunDenizen` lane by
  another name: a behavior is a denizen whose run was triggered, and a second
  path would mean a second set of rules. Exhaustion reports as
  `AppEvent::CascadeExhausted`, naming the residents by label.
  **Remaining:** the budget is a live `App` field read per cascade but is not
  yet persisted in `ApplicationSettings` or exposed as a settings row (that
  reaches the app only through the settings pane's provider, so it is real
  threading, and adding the field before the wiring would be dead config);
  and the headed scenario receipt is not captured.
  Done when: a two-behavior mutual-wake fixture terminates at the budget
  with the loud event on screen; a linear A-wakes-B chain settles in one
  cascade; every behavior-authored entry in the journal reads back with the
  right subject; the whole cascade replays deterministically in a scenario
  run; lowering the budget setting from 4 to 1 takes effect on the next
  cascade without a restart. (The headless halves of the first two and of the
  budget clause are covered by W1a; what remains is the wiring and the headed
  receipt.)
- **W2: trigger context into the body. MOSTLY LANDED 2026-08-13.** A woken
  body reads `mere.trigger()`, beside `mere.snapshot()` and gated on the same
  `app.read` capability because it describes the graph. It returns a
  `TriggerContext`: per matched entry, the journal seq, the author, and the
  node ids that moved. A **digest, not the deltas**, so a body is not coupled
  to the kernel's 44 variants or their evolution; and the nodes are each
  scope's *last* segment, since ancestry is written outermost-first and the
  tail is what actually changed. A hand-invoked run gets an empty context
  rather than a missing one, so a script can always ask. Uninstall now calls
  `WatchTable::remove_subject`: residency, authority, and standing
  subscriptions end together.
  The **install review now names the watch** (the ruled condition): a pack
  declares `-- @watch <url>` in its source, the review row reads "wakes on:
  <url>" beside the rings, and confirming registers it. Three properties fall
  out rather than being built: the declaration is an *address* because an
  author cannot know a UUID (install resolves it, minting the target with
  `visit` if absent); it lives *in the source*, and the subject is
  `blake3(source)`, so changing what a pack wakes on changes its identity and
  re-reviews; and `install_caps` grants a READ scope over the watched region,
  so the containment law holds by construction rather than by hope.
  **Remaining:** the inbox rule is not built, so there is no headed receipt.
  The original text follows. The matched-delta digest handed to
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
- 2026-08-13 (W0): landed in servitor. **43 tests pass** (32 existing plus
  11 new), clippy clean in servitor's own files. All four done conditions
  covered by name: containment enforced at registration (including the root
  scope refused as a loophole, and a `Write` grant accepted as covering the
  `Read` a watch needs), segment matching proven by `trail` not covering
  `trailer`, self-waking refused under both author conventions, and cursors
  proven to survive the wire round trip. Four beyond them: stable subject
  ordering (what cascade determinism rests on), uninstall taking watches with
  it, a malformed record failing the load rather than vanishing, and a
  colon-bearing scope surviving the wire form. Two findings folded into 3.1
  and 3.2, and W0.5 added: the main graph has no region vocabulary, so W2 is
  blocked until Mark rules one.
  Note for whoever runs the workspace clippy gate: `-D warnings` is red on
  personae's deprecated `chacha20poly1305` `from_slice` calls, which is
  pre-existing crypto-row residue and unrelated to this slice.
- 2026-08-13 (W0.5 ruled, W1a): Mark ruled container membership; the ancestry
  design above is written against `edge_taxonomy.rs`, which already carries
  `EdgeFamily::Containment`. `servitor::cascade` landed with it. **51 tests
  pass** (8 cascade, 11 watch, 32 existing). The cascade tests cover a chain
  settling, a two-scope relay settling, mutual waking stopping at the budget
  with both subjects named, deferred work surviving exhaustion, a budget of
  one, the floor, an empty wake set never calling the runner, and two
  identical runs producing identical rounds.
  One fixture was wrong before it was right, and the correction is the
  interesting part: the first "relay settles" test had both subjects watching
  the same scope with a runner that committed unconditionally, which is not a
  relay but two behaviors answering each other, and it correctly exhausted
  the budget. A real relay needs distinct scopes and a last link that writes
  where nobody is watching. The test now says so in its name.
- 2026-08-13 (W1b): the drain landed in turnstone. **236 tests pass**, 5 of
  them new, clippy clean in the touched files. Two findings from reading
  rather than assuming. **Containment points from the member to the
  container**: the kernel asserts `assert_relation(child, parent,
  Containment)`, so ancestry follows *outgoing* edges, and walking incoming
  ones would have built every watch tree upside down. And the delta grouping
  has five shapes, not four: `ReplayBranchHistoryByIds` carries
  `child_id`/`parent_id`, which a field-name sweep misses, so a history
  branch would have woken nobody.
- 2026-08-13 (W2): the trigger context landed in turnstone. **250 tests pass
  with `--features piccolo`** (up from 236 default), including a body that
  dispatches only when something woke it and refuses the digest without
  `app.read`. Two incidental finds. The **piccolo test lane did not compile at
  HEAD**: three `App.canvas` references outlived a rename to `graph_runtimes`,
  so `cargo test --features piccolo` had not been run in a while; fixed here
  because testing `mere.trigger()` required it. And the **sandbox carries no
  `string` library**, so the fixture compares against the empty wire form
  instead of searching it, with the exact form pinned by a unit test so the
  two fail together. The default lane's one red is a `place::lanes` timeout
  that passes alone in 8s, a flake under concurrent builds, and untouched by
  this work.
- 2026-08-13 (W2, review half): the watch declaration and install registration
  landed (turnstone `59573a6`). **Not verified**: the source compiles clean,
  but its test has not run, because mere's tree has been mid-migration in two
  sibling sessions for ~20 minutes (the wallet_grant device-grant split, and a
  `p2panda_auth` import in gemot), so turnstone's dependency chain does not
  build. Committed rather than left dirty, since in-flight work in these trees
  was swept into sibling commits three times today and an accurate message
  beats an inaccurate one. **Re-run `cargo test --lib --features piccolo` once
  mere is green**, and check that the longer review row still passes the
  <96-char guard in `denizen.rs`.
