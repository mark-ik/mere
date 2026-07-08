# codicil

An append-only, replayable log. A `Codicil<T>` is a linear sequence of immutable
entries you append and replay to rebuild the state they describe. Edits are never
destroyed; a change is a new entry. The event-source and nondestructive-history
primitive, persisted through its sibling [muniment](https://github.com/mark-ik/muniment).

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

Transport-neutral (it produces a replayable sequence; shipping it to peers is the
consumer's job) and linear (a branching edit-tree is a later shape), by design.
The name: a codicil is an amendment appended to a document, never a rewrite of it.

Built alongside muniment from a survey of four consumers (woodshed, strophe,
isometry, mere). See [`design_docs/`](design_docs/).

License: dual MIT OR Apache-2.0, at your option.
