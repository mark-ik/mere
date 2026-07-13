# murm-replication

Shared replicated-space mechanics for the Murm peer-exchange family.

The crate currently owns four proven pieces:

- `SyncedSpace`, the common p2panda LogSync drain with real status and resync
  reporting.
- `MunimentStore`, the backend-neutral p2panda operation, log, and topic store.
- `OperationProcessor`, the shared policy-before-insert path for structural
  validation, domain admission, prune-aware retained-frontier continuity,
  idempotence, and one atomic indexed write with optional prefix removal or
  authorized body erasure.
- `drop`, the bounded plaintext/public native drop framing: fixed cover,
  semantic manifest identity, self-delimiting records, streaming staged visit,
  integrity checks, and configured allocation limits.

Direct exchange, Moot, mesh, and other domains retain their own operation
grammar, authorization, and deterministic materialization. Mesh and Tessera
use the processor for local authoring and LogSync receipt. Tessera's current
policy is wire-level admission only, not Moot membership or constitutional
authorization. Domains return an `Admission` carrying `HistoryAction::Keep` or
an authorized `PruneBeforeCurrent`, plus any authorized body erasures. A
focused proof covers the upstream `PruneFlag`, prune-aware backlink validator,
and `LogPrune` behavior against `MunimentStore`. Mesh uses that path for
policy-bound checkpoints, terminal-input erasure, and prefix pruning.

The drop codec is a carrier, not an importer. Protected suites, compression,
the staged muniment importer/export selector, gossip receipt, shared commitment
references, and non-mesh checkpoint governance remain later slices.

## License

MPL-2.0.
