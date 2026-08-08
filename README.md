# gaz

The contact layer: mere's remote side of identity.

A contact is the local rollup that says "these N addresses are all Bob":
your petname for them, rooted on their stable keys, with attested handles
and current endpoints each carrying its own trust state. Records are
persona-scoped (a throwaway persona must not share the work persona's
contacts), tiered kith / kin, and carry recency (who you reached lately).
Storage rides muniment; records key against personae.

The boundaries are the point:

- **Not the resolver.** The gazetteer turns a name, handle, or key into
  reachable endpoints; gaz is where the ones you keep live. (Gaz is not
  short for gazetteer; they are siblings on the persona tier.)
- **Not identity itself.** personae owns *me*, the key-bag and its carry;
  gaz owns *them*, your records about other people's keys.
- **Not trust arithmetic.** Verification states are stored per endpoint;
  how they are earned belongs to the trust plane.

## Status

**Name reservation.** The slot is real and open: mere's contact identity
model brief (2026-06-15) specifies the record; no implementation exists
yet anywhere in the stack.

## License

MIT OR Apache-2.0.
