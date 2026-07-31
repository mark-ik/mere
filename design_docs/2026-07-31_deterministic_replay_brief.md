# Wholesale Snapshots: A Follow-on to the Data-Oriented Doctrine

**Status: brief, 2026-07-31. No work proposed.** Cross-cutting: mere, Hocket,
Turnstone, Isometry, and the games wing.

Written after [Tangle](https://github.com/kettle11/tangle) came up while
designing co-op for Mesocosm, and the question arose whether its lesson reaches
the rest of the family.

**It mostly does not, because the stack got there first.** This brief exists to
record *what is actually new* in that lesson, which is narrower than it first
appears, and to mark the one place it beats what we already do.

Read [`2026-07-02_data_oriented_doctrine_brief.md`](2026-07-02_data_oriented_doctrine_brief.md)
first. This is a follow-on to its §"capture/replay as the universal debugging
idiom", not an independent idea.

---

## 1. The correction that motivated this brief

The initial framing was that the stack owns "state as a fold over ordered
inputs" but lacks "cheap wholesale snapshots." **The second half of that is
false**, and the doctrine brief says so:

- Doctrine principle 5: *change is a recorded delta stream against the tables;
  the delta log is the only write path worth trusting, and* **replay from empty
  must reproduce the tables**.
- Doctrine principle 6: every downstream layer is an incremental fold over the
  stream above it.
- Shipped today: netrender has `snapshot_postcard` / `replay_postcard`;
  graph-kernel has history plus two-phase apply.
- The stated per-layer done condition is already exactly the thing:
  *a failing frame can be reduced to a serialized delta log plus a table
  snapshot that replays the failure headlessly.*
- Two unrecorded streams already have plans: `GraphDelta` (mere) and
  `DomMutation` (genet).

So both halves are doctrine here, and have been since 2026-07-02. Anyone
arriving at "we should make state a fold and capture it" is rediscovering the
house style.

---

## 2. What Tangle actually adds

One property, and it is a real one: **totality without authorship.**

The stack's capture is **per-layer and hand-written**. Each delta stream is
captured at its boundary, by code someone wrote for that boundary, and a
snapshot is a table dump someone maintains. Tangle's capture is **whole-heap
and automatic**: because a WebAssembly module's linear memory *is* its entire
world, "capture everything" is a `memcpy` and there are no fields to forget.

That difference matters in exactly one way. Hand-written capture has a failure
mode — a field added later and not added to the snapshot — that produces
divergence you debug for a week. Whole-heap capture cannot have that failure
mode, structurally.

The trade runs the other way too, and the stack's choice is the right one for
the stack:

| | Per-layer (ours) | Whole-heap (Tangle) |
| --- | --- | --- |
| Precision | Capture exactly one boundary; reduce a bug to one stream | All or nothing |
| Cost | Proportional to the stream | Proportional to total heap |
| Applicability | Works in a browser host with GPU resources, sockets, engine state | Needs bounded, owned, self-contained state |
| Failure mode | A forgotten field | None structurally |

**Conclusion: per-layer is correct for mere, Turnstone, and genet.** Their
state includes GPU resources, live network handles, engine internals, and large
blobs — a host, not a heap. Whole-heap capture is not available to them at any
price.

---

## 3. Where whole-heap wins

**Game cores, and essentially only game cores**, because a game core is the
rare thing whose state is bounded, owned, and self-contained. `mesocosm-core`
has already adopted it as a constraint with a deadline (see the games wing's
body pipeline plan §R0), and the payoff there is that co-op, replay, save/load,
time-travel debugging, and host comparison become one mechanism.

Two boundaries worth stating once, because they generalise:

- **Documents can be deterministic; renders and DSP cannot.** Float paths,
  sample rates, device buffers, and GPU drivers differ across machines. Claim
  determinism for the document or simulation layer, and disclaim it explicitly
  for pixels and samples. Nobody needs sample-identical audio, only an
  identical composition.
- **A browser cannot replay the web.** Network responses, timers, page script,
  and external content are not reproducible. Turnstone's *shell* state (graph,
  panes, session, navigation) is fold-shaped and replayable; rendered page
  content never is. Any design here must draw that line or it will promise a
  guarantee it cannot keep.

---

## 4. Hocket is the family's live experiment

Worth watching, because it reached these questions independently and from the
provenance side.

Its collaboration model is *passing the mic around a circle — people add parts
asynchronously rather than jamming across a network*, so real-time sync was
sidestepped by design. Recording **appends a layer**. A `.hock` file is a zip
of `manifest.cbor` plus content-addressed `media/<hash>.wav`. And the engine
already carries a **signed, recipient-addressed hand-off envelope for a
complete project snapshot and its media, with a transactional same-root
branch-acceptance rule** — transport-neutral groundwork; peer hand-off and
synchronisation are the explicitly unbuilt piece.

The instructive part is that Hocket chose **snapshot transfer** over **input
replay**, and was right to. For asynchronous turn-taking you hand someone the
whole state, not a log to re-execute. These solve different problems, and
conflating them is the easiest available mistake:

- **Input replay** — many peers, one shared timeline, everyone recomputes.
  Suits shared-session co-op.
- **Snapshot transfer** — one author at a time, state handed on with
  provenance. Suits turn-taking, visiting, and grafting.

The games wing has both shapes for the same reason: shared-session co-op wants
replay, and Paredros' visiting model wants transfer.

---

## 5. Recommendation

**No work proposed, no shared crate, no migration.**

- **Keep the doctrine as written.** Per-layer capture/replay is correct for the
  host applications, and the two planned streams (`GraphDelta`, `DomMutation`)
  remain the right next steps for it.
- **Use whole-heap capture only where state is bounded and owned**, which today
  means game cores.
- **When any new deterministic core is written**, keep it a pure function of
  seed and ordered inputs: no ambient clock reads, no unordered iteration
  affecting results, all randomness from a seeded stream. Nearly free to design
  in, expensive to retrofit.
- **Never claim determinism across a render or DSP boundary.**
- **Watch Hocket's hand-off work.** Whatever it settles for snapshot-transfer
  plus branch acceptance is the natural thing for other apps to copy, and it is
  the same shape the games wing needs for visiting.

Tangle itself is MIT, web-only, TypeScript-hosted, and last pushed July 2024.
The technique transfers; the library is not a dependency candidate.
