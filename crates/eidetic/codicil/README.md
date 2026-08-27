# codicil

An append-only, replayable log. A `Codicil<T>` is a linear sequence of immutable
entries you append and replay to rebuild the state they describe. Edits are never
destroyed; a change is a new entry. The event-source and nondestructive-history
primitive, persisted through its sibling [muniment](https://github.com/merely-made/mere).

```rust
use codicil::{Codicil, Seq};

let mut history = Codicil::new();
history.append(5i64);          // each entry is an edit
history.append(-2);
history.append(10);

// Replay folds the entries into the state they describe.
let total = history.replay(0, |sum, delta| sum + delta);
assert_eq!(total, 13);

// A Seq is a durable cursor: catch up on only what is new.
let cursor = Seq(2);
assert_eq!(history.from(cursor), &[10]);
```

Persist it through any muniment slot (it is `Serialize`):

```rust,ignore
history.save(&slots, "history").await?;
let restored: Codicil<i64> = Codicil::load(&slots, "history").await?;
```

## Modules

| Module | Contents |
|---|---|
| `log` | `Codicil<T>`: `new`, `with_id`, `append`, `get`, `entries`, `from`, `len`, `is_empty`, `next_seq`, `replay`, `replay_from`, `fork`, `id`, `provenance` |
| `seq` | `Seq(u64)` with `index` and `next` |
| `causal` | `append_caused_by`, `parents`, `roots`, `causes`, `effects`, `concurrent`, `CausalError` |
| `fork` | `LogId`, `Provenance` |
| `persist` | `Codicil::save` / `Codicil::load` against a muniment `SlotStore` |

Storage is a flat monotonic sequence; causality rides beside it as parent links.
`append` claims no causes and stays linear; `append_caused_by` records them, after
which `causes`, `effects`, and `concurrent` answer over the resulting DAG.
`Codicil::fork` mints a new `LogId` carrying `Provenance` back to the parent log.

Transport-neutral: it produces a replayable sequence, and shipping it to peers is
the consumer's job. The name: a codicil is an amendment appended to a document,
never a rewrite of it.

Dependencies: `muniment` (path), `serde`.

Built alongside muniment from a survey of four consumers (woodshed, hocket,
isometry, mere). See [`design_docs/`](design_docs/).

License: MPL-2.0 (see LICENSE).
