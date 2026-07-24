# servitor

Capability-scoped resident helpers for graph applications: a **denizen** holds
a scoped structural capability and proposes changes through a validating gate,
attributed and revision-checked.

A denizen is anything admitted to act on a graph: a resident helper (a
servitor), a script, a scenario runner, a remote peer, an agent. It holds a
keyholder identity and a scoped capability, and proposes **petitions** that the
gate validates and commits, attributing every change to the denizen.

## The core

- `Subject` — a keyholder identity (a 32-byte public key), the shape the moot
  authorization seam already speaks.
- `Grant` / `AuthorityProvider` — a scoped structural capability and the
  replaceable read boundary that answers "does this subject's capability cover
  this path?", mirroring `gemot::MootAuthorizationProvider`. `PrefixAuthority`
  is the minimal stand-in until the meadowcap-shaped structural-cap layer over
  graph-cluster-derived namespaces is built.
- `Gate` — one authority pipeline: refuse petitions that touch a grant
  projection, check authority, check scope, then commit attributed through
  chartulary's revision-checked batch.

A denizen's inner world is an ordinary `chartulary::GraphLog` (the nested graph
a graph-bearing node points at). Wiring a host-graph node to bear it is the
host's job.

The capability model is deliberately minimal: an opaque `capability_path` with
prefix-shaped coverage. The `AuthorityProvider` seam lets the richer provider
(meadowcap structural caps + tessera policy facts + group-key state) drop in
without the gate changing.

## License

MIT OR Apache-2.0.
