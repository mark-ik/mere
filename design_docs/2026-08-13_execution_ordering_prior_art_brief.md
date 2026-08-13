# Execution-Ordering Prior Art Brief: wavelet

**Status:** research brief. Reads one external crate closely and extracts what
transfers. No code change proposed; two candidate shapes are named for
whenever the kernel's internal ordering becomes a real question.
**Date:** 2026-08-13.
**Scope:** cross-cutting. Companion to the
[data-oriented doctrine brief](2026-07-02_data_oriented_doctrine_brief.md),
which owns *representation* (arenas, ids, kinds, delta streams). This one is
about *execution*: in what order dependent work runs inside one thread, and
how staleness is detected. The two meet only at invalidation.

---

## 1. What the crate actually is

[`wavelet`](https://crates.io/crates/wavelet), from
`github.com/Abso1ut3Zer0/wavelet-rs`, described by its author as a
"High-performance graph-based stream processing runtime".

There are no wavelets in it. The name is discussed in section 5.

It is a single-threaded, deterministic, push-based computation-graph runtime,
positioned by its README explicitly against async runtimes and actor systems.
Nodes hold state and run a closure; edges declare dependency. The worked
examples throughout are a trading system: currency-pair feeds, a risk engine,
an order gateway.

Ecosystem facts as of today: version 0.6.1, twelve-plus releases between
2025-09-16 and 2025-12-05 (one yanked), repository last pushed 2026-03-06,
MIT/Apache, `rust-version = 1.89.0`. **20,008 downloads, 11 stars, and zero
reverse dependencies on crates.io.** It leans on `petgraph` for the graph,
`mio` for I/O readiness, and `crossbeam`'s `ArrayQueue` for the cross-thread
channel feature.

## 2. The five ideas worth taking

### 2.1 Two edge kinds, named separately

`Trigger` propagates: when an upstream node broadcasts, its trigger-children
are scheduled and run. `Observe` does not propagate: it establishes the
dependency ordering and read access, and the observer reads upstream state
when it happens to run for some other reason.

This is the sharpest thing in the crate. Where it applies, making it two edge
constructors forces the question to be answered per edge, at the point where
the author knows the answer, rather than inferred later by whoever is
debugging a missing repaint or an over-eager one.

**It does not apply anywhere in the stack today.** The first version of this
section claimed the set graph, the container graph, and cambium's view
rebuilds each conflate reachability with invalidation. That was written from
memory of those graphs rather than from reading them, and it is wrong on all
three. None of them carries propagation semantics for the split to divide:

- **The set graph** (`woodshedding::rehearsal`) is a display projection of
  practice order. `SetGraphEdgeKind` has exactly one variant, `Next`, derived
  from `nodes.windows(2)`, and every consumer either draws it, hides it (the
  relation-visibility toggle), or hashes it into a rebuild signature. Nothing
  runs downstream of a card.
- **Chartulary's relations** are semantic: RDF-projecting predicates or
  app-private families. A knowledge graph, not an execution graph, and
  nothing in the crate invalidates along one.
- **Cambium's view rebuilds** have no dependency edges at all, being
  xilem-shaped structural diffing.

Woodshed's invalidation is signature-based rather than edge-based:
`leaves.rs` hashes the whole swatch and repaints when the hash moves. That is
the opposite of a dependency walk, which is why there is no conflation to
fix rather than a hidden one.

So the idea is worth holding and has nothing to attach to. It becomes real at
the same moment section 3 does: when something inside a kernel has to decide
what re-runs because something else changed. Until then, adding
`Trigger`/`Observe` to a graph that only draws would be a distinction no code
could branch on.

### 2.2 Depth-ordered scheduling, with backward scheduling a hard error

Read from `runtime/scheduler.rs`, which is worth more than the README says.
The scheduler is a multi-queue: `Vec<VecDeque<NodeIndex>>` indexed by the
node's depth in the graph, plus a monotonically advancing `curr_depth`.
Scheduling a node whose depth is *above* the current processing depth returns
`SchedulerError` rather than being accepted, with the reasoning stated in a
comment: accepting it would let some execution paths never run, silently
dropping work.

That is the whole loop-prevention story, and it is structural. Depth ordering
guarantees parents before children; the refusal to schedule backward
guarantees a cycle terminates. Note the README describes this as "epoch-based
deduplication", which the source does not support: epochs do something else
(2.3). Doc-to-doc reading would have got this wrong.

### 2.3 A per-node mutation epoch, for staleness rather than ordering

Every node carries `mut_epoch()`, the epoch at which it last mutated. An
observer compares it against what it last saw to decide whether upstream
actually changed. The counter answers "is what I read still what I read
before", not "have I already run".

**This is armillary's `Generations` at a different scale**, and the parallel
is worth holding. Armillary stamps work with a monotonic generation so a
result computed against superseded state (a scene for a page the surface has
left, input from before a resize) is dropped rather than applied. Wavelet
stamps state with a monotonic epoch so a reader can tell whether it has moved.
Same instrument, one applied across a thread boundary and one within a graph.

### 2.4 The clock is a component, and one implementation is replay

Three clocks behind one trait: realtime, test, and `HistoricalClock`, which
walks a `[start, end]` interval in fixed steps, advancing one step per
`cycle_time()` call, with the instant baseline rebased so duration arithmetic
still holds against a wall-clock-shaped API.

We already have the input half of this discipline. Woodshed takes `now_ms`
from the host and refuses to measure at all when the host supplies none,
rather than dating practice from epoch zero, and genet's `Harness` is the
same instinct applied to input events. The piece we do not have is the replay
implementation: the thing that turns "the app does not read a clock of its
own" into "re-run yesterday's session and get the same answer". That is the
shape scenotime and the physics crates want, and it costs one more
implementation of a trait that would already exist.

### 2.5 Factories: one topology, swapped leaves, memoized

Build-time dependency injection. The same graph-construction code runs
against live, mock, or historical sources depending on what the factory
attaches, and a keyed factory memoizes so the same key returns the same node
instead of building a duplicate subgraph.

We arrive at this by hand each time. The persona gate's tests drive the real
product root through `Harness` with a fabricated roster; woodshed's scenario
lane swaps the store via `WOODSHED_STATE`. The generalizable statement is that
the topology is the invariant and the leaves are the variable, which is also
why an A/B against an unpatched control is worth the trouble: it holds the
topology still.

## 3. Where it sits relative to armillary

**Correction to a claim made in conversation before reading either closely.**
Wavelet is not a counter-position to [armillary](../crates/armillary/src/lib.rs).
They answer different questions and compose.

Armillary is a thread-boundary discipline: a single-threaded host kernel owns
canonical state, `!Send` by construction so the compiler refuses to move
kernel authority onto an actor thread; actors run off-thread with `Send`
handles; generations drop superseded results. It says which thread owns what.
It says nothing about the order in which work runs *inside* the kernel.

Wavelet is that missing half, and only that: within one thread, what runs
before what, and what re-runs when. Its rejection of "actor systems" is a
rejection of actors as a *scheduling* model, which is not what armillary uses
them for.

So the honest reading is that wavelet's shape is a candidate for a layer
inside armillary's kernel, not a replacement for it, and it becomes relevant
exactly when the kernel's internal work grows a real dependency structure.
Nothing today demands it.

## 4. What not to take

- **Single-threaded by design.** Its determinism claim depends on it. Any
  parallel execution reopens the ordering question it closes.
- **Shaped by one domain.** Trading is a low-latency pipeline with a natural
  DAG and a natural replay story. Our load is a document model, layout, paint,
  and probes, which is a DAG in a different sense.
- **`UnsafeCell` in the garbage collector**, justified by a temporal-
  separation argument in a comment: writes during cycle execution, reads after
  cycle completion. Documented, per our standard, but the argument is by
  convention rather than by types, and that is the weaker kind.
- **Zero reverse dependencies.** Nobody has integrated it publicly. The ideas
  are worth reading; the crate carries no external validation.

## 5. Two lessons that are not about dataflow

**Naming.** The repository still contains `.idea/reflex-rs.iml`, and
`.idea/modules.xml` still points at it: the project was **reflex-rs** and was
renamed to wavelet. "Reflex" described the mechanism, push-propagation and
reaction. "Wavelet" is a precise term in signal processing for something this
crate does not do, so it will be found forever by people wanting the
transform and missed by people wanting a dataflow graph. This is the cost our
naming discipline already prices in, made concrete: evocative is worth paying
for on a product name and expensive on a library whose word is already spoken
for in an adjacent field. It also sits one letter-cluster from `wavicle`, so
an audio-crate search surfaces both.

**Download counts are not adoption.** 20,008 downloads against zero reverse
dependencies and 11 stars is CI and mirror traffic. Worth remembering if
download figures are ever tempting as evidence in census reasoning, where the
[leverage census](2026-08-10_leverage_census_brief.md) counts real consumer
edges instead, which is the right unit.

## 6. Verification notes

Read from source rather than from the README: `runtime/scheduler.rs` (depth
multi-queue, the backward-scheduling error), `runtime/node.rs` (`mut_epoch`),
`runtime/clock/historical.rs` (interval walk and instant rebasing),
`runtime/garbage_collector.rs` (deferred removal, the `UnsafeCell` argument).
Ecosystem figures from the crates.io API, including the reverse-dependency
count. The rename is inferred from the two `.idea` files, which is strong but
circumstantial. Armillary's shape is from its own `lib.rs`, not from memory of
it, which is what caught the error corrected in section 3.

**Corrected 2026-08-13, after publication.** Section 2.1 originally named
three of our graphs as sites for the edge split. Reading them settled it the
other way: `rehearsal.rs` (one edge kind, derived from window pairs, no
propagating consumer), `chartulary/src/taxonomy.rs` (semantic predicates, no
invalidation anywhere in the crate), and `leaves.rs` (whole-swatch hashing,
not a dependency walk). The claim came from recollection rather than reading,
which is the same failure this section exists to catch, and it survived one
round of review because a brief about verification is not itself verified.
