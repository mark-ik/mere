# gaz

The contact layer: your records about other people.

A contact is the local rollup that says these addresses are all the same
person: your petname for them, rooted on their stable keys, with handles and
endpoints that each carry their own trust state. Records are persona-scoped
(a throwaway persona must not share the work persona's contacts), tiered
kith / kin, and carry recency.

```rust
let mut book = ContactBook::new(PersonaScope::new("work"));

book.insert(
    Contact::new("Alice", alice_key)
        .with_handle(Handle::acct("acct:Alice@example.org"))
        .with_endpoint(Endpoint::new(EndpointKind::Misfin, "alice@example.org")),
);

book.mark_contacted(&alice_key, now_ms);
assert_eq!(book.recent(5)[0].petname, "Alice");
```

## What roots a record

Keys, not names. Handles change and hosts move; a key does not. A record is
filed under its **anchor**, the first key you ever knew someone by, so
rotating a key never moves the record and a message signed with a retired key
still finds its owner.

## The boundaries are the point

- **Not the resolver.** A gazetteer turns a name, handle, or key into
  reachable endpoints. gaz is where the ones you keep live. The two are
  siblings on the persona tier, and gaz is not short for gazetteer.
- **Not identity.** `personae` owns *me*, the key-bag and its carry. gaz owns
  *them*, your own records about other people's keys.
- **Not trust arithmetic.** Trust state is stored per endpoint; how it is
  earned belongs to the trust plane. gaz depends on no cryptography, holding
  keys as bytes it compares but never verifies.

## Two habits

gaz never reads a clock: every timestamp is a `now_ms` you pass in, which
keeps it deterministic under test and usable on wasm. And every recency
update is monotonic, so a replayed or late event can never rewind a record.

## Status

Pre-1.0. The data model exists and is tested; persistence over `muniment` and
the adapters that turn resolver output into records are the next lifts. See
`design_docs/` for the founding plan.

## License

MIT OR Apache-2.0.
