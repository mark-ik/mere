# Commons Profile v1

**Date:** 2026-07-27
**Status:** executable software profile. The graph, Knot-document, and chat
receipts live in `commons-spine`, Knot, and Stickleback. This document states
the user-visible contract rather than inventing another runtime.

## Shared substrate

A Commons space retains signed p2panda operations in Stickleback. Operations
carry exact observed-frontier parents. Materialization uses causal order and a
stable author/log/sequence tiebreak only for concurrent facts. LogSync,
protected native drop, Reticulum, and other carriers move the same signed
operation bytes.

Knot remains the owner of document events and file semantics. It consumes the
shared causal projection functions directly. It does not translate documents
through chartulary or hide a whole-document conflict behind last-writer-wins.

## Identity and authority

Every communal operation binds its signer to a stable Personae root. A master
key binds directly. A per-space derived key carries a
`DerivedKeyAttestation` verified with the space-specific derivation salt.

Graph mutations require the typed Servitor write scope
`commons/container/<space-id>`. Converged Gemot/Personae authority classifies a
retained operation:

- **effective:** included in the current projection;
- **pending:** structurally valid and retained, awaiting sufficient authority;
- **revoked:** retained for audit and withdrawn from the current projection.

Authority is re-evaluated during every fold. A certificate arriving later can
make an existing fact effective. Revocation can withdraw it without rewriting
the log. Relay identity and session admission have no effect on operation
authority.

## Encryption profiles

Encryption is a required profile choice:

| Profile | Mode | History rule |
|---|---|---|
| `commons.knowledge.v1` | p2panda Data Encryption | New members may receive retained epochs and read retained history. |
| `commons.chat.v1` | p2panda Data Encryption, eight retained epochs | The first chat slice favors disconnected recovery and multi-carrier history. A future forward-secure chat profile must use a distinct Message Encryption profile. |

The host persists p2panda group-secret state. Membership removal rotates the
data epoch before later application writes. Removed members retain plaintext
they already had and any retained old epoch, but cannot open operations sealed
under the rotated epoch. Epoch pruning needs a domain checkpoint and explicit
retention authority.

Application bytes are encrypted first. The p2panda header then signs the
ciphertext, its space address, content class, causal parents, author sequence,
and backlink.

## Content classes

The first chat vocabulary is:

- `commons.channel`: channel identity, title, and membership-facing settings;
- `commons.message`: immutable body, channel, authored time, and optional reply
  operation.

Message edit and delete are absent in v1. Murm `Post` remains the bilateral
direct-conversation grammar. Compatible posts may be read into one view, but
they are not rewritten or declared wire-equivalent.

Knot contributes `knot.file` and `knot.note`. Concurrent writers touching one
document produce a visible `KnotDocumentConflict` with one current version per
writer. Unrelated documents remain available. Resolution and text merge stay
Knot-owned.

## Merge and incomplete history

- Missing causal history blocks only its operation and descendants. The
  projection returns unrelated causally closed state plus exact pending roots.
- Whole-node concurrent edits use the stable operation tiebreak. This remains a
  known coarse merge until facets become independent edits.
- Remove wins against a truly concurrent insert of the same node.
- An insert causally after a removal deliberately recreates that node.
- Remove wins against a concurrent connect, and removes incident edges.

## Limits and durability

Desktop graph and chat profiles admit at most 64 causal parents and a 1 MiB
signed payload. One graph batch admits at most 1024 edits. Knot admits at most
64 parents and a 16 MiB sealed document event. Carrier profiles may tighten
these ceilings.

Knot checkpoints persist document digests, conflicts, pending parents, and
every author/log head. A tail receipt names operations newer than the durable
checkpoint. Pruning may proceed only when the domain also preserves authority,
decryption, and author-continuation proof.

Ingress is serialized per author/log across clones of one Stickleback store.
Redb reopen restores author sequence, backlinks, causal frontier, document
checkpoint, and tail receipt.

## Deferred

Facet-grained text merge, message edit/delete, calls, LXMF codecs, RF hardware
receipts, compressed frontiers, richer content schemas, and automated epoch
retention policy remain separate work. The carrier-neutral Reticulum packet
seam is software-proven; a direct-PHY radio result requires real hardware.
