# murm-replication

Shared replicated-space mechanics for the Murm peer-exchange family.

The crate currently owns five proven pieces:

- `SyncedSpace`, the common p2panda LogSync drain with real status and resync
  reporting.
- `MunimentStore`, the backend-neutral p2panda operation, log, and topic store.
- `OperationProcessor`, the shared policy-before-insert path for structural
  validation, domain admission, prune-aware retained-frontier continuity,
  idempotence, and one atomic indexed write with optional prefix removal or
  authorized body erasure.
- `drop`, the bounded native drop framing: fixed cover,
  semantic manifest identity, self-delimiting records, streaming staged visit,
  integrity checks, configured allocation limits, and an injected authenticated
  protection-suite seam.
- `drop_io`, the first export/import path: topic operation selection, canonical
  operation records, full-corpus structural/policy preflight, and ordinary
  processor admission with idempotent reports. Domains inject omit/header/full
  privacy decisions and settings-derived priority; replication applies a
  deterministic semantic-byte budget and reports policy versus budget
  omissions. Explicit plaintext/public and injected protected file helpers
  carry the same bytes between profiles. Private file export requires the
  selector.

Direct exchange, Moot, mesh, and other domains retain their own operation
grammar, authorization, and deterministic materialization. Mesh and Tessera
use the processor for local authoring and LogSync receipt. Tessera's current
policy is wire-level admission only, not Moot membership or constitutional
authorization. Domains return an `Admission` carrying `HistoryAction::Keep` or
an authorized `PruneBeforeCurrent`, plus any authorized body erasures. A
focused proof covers the upstream `PruneFlag`, prune-aware backlink validator,
and `LogPrune` behavior against `MunimentStore`. Mesh uses that path for
policy-bound checkpoints, terminal-input erasure, and prefix pruning.

The codec deliberately chooses no domain key authority. Murm injects an
epoch-aware p2panda XChaCha protector; a future group-state adapter still owns
authorization, key distribution, and persisted epoch history. Suite-owned
decompression must honor the codec's plaintext bound. Verified drops now stage
durably by semantic `DropId`;
operation corpora, authorized prefix pruning and payload erasure, stage cleanup,
and a receipt commit in one backend batch. Payload and blob chunks assemble by
digest; header-only and payload-only drops can meet later through a one-shot
pending slot, while erasure removes the reference that could restore an old
body. A later full operation also hydrates an already-retained header through
the same reference, and reports that separately from insertion or a contentless
duplicate. Callers can list, retry, or discard verified stages according to
their own settings. Portable receipt coordination is now available at the library
boundary: the local atomic marker becomes a bounded, integrity-framed
statement, and received statements live in a separate peer-scoped advisory
namespace with caller-directed cleanup. The peer service must derive that scope
from its authenticated carrier identity. A remote claim cannot become a local
import marker or bypass domain admission. Live peer command wiring, the Moot
domain mapping, and a live Moot constitution fold remain. Mesh and direct
conversation now supply concrete drop selectors. Murm's live
`ConversationEngine` now uses the muniment-backed `ConversationStore` for local
authoring, gossip receipt, LogSync, and export selection. Durable redb selection,
reopen reconstruction, and native-drop materialization refresh are landed.

## License

MPL-2.0.
