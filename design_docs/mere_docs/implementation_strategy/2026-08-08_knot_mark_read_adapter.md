# Knot Mark read adapter

> **Repository note (2026-09-01):** this is a historical integration record.
> Current Knot source and product authority live in the public
> [`knot-editor`](https://github.com/merely-made/knot-editor) repository; local
> `ports/knot` paths below name the source layout when each receipt landed.

Status: implemented bounded adapter, pending an external Demarkus-client
receipt.

This records the Phase B choice from
`2026-08-07_knot_publishing_protocol_plan.md`: Knot keeps its private native
publish lane and offers a deliberately separate read-only projection for the
Mark Protocol v1.0 working draft. It does not adopt the Mark protocol as
Knot's native contract.

## Compatibility boundary

| Concern | Native Knot publish | Mark projection |
|---|---|---|
| Transport | p2panda/iroh, `mere/knot-publish/v1`, inner Noise and Notochord | direct standard QUIC/TLS, ALPN `mark` |
| Identity | carrier peer plus Personae device and retained delegation | TLS server certificate; optional adapter-local read token |
| Addressing | out-of-band ticket, then mDNS or ticket endpoint | `mark://host[:port]/path`, default UDP 6309 |
| Content | authored `text/vnd.knot` or `text/djot` | canonical CommonMark projection, never a media-type relabel |
| Version identity | causal operation digest | adapter-owned positive integer snapshots |
| History | causal graph, with conflicts refused | linear SHA-256 linked Mark versions only |
| Cache proof | BLAKE3 of exact native body | SHA-256 ETag over stored version and `content-hash` over served body |
| Absence | one non-disclosing `NotAvailable` | `not-found` for absent or denied configured exports |

The Mark working draft requires properties Knot's causal store does not name:
an authored wall-clock time and a linear immutable version sequence. The
adapter therefore takes an explicit owner snapshot. Its `modified` timestamp
is snapshot time, and a native source update changes nothing until the owner
requests another snapshot. An unresolved, deleted, pending, or automatic-merge
source remains unavailable through the existing native source-eligibility
check.

The adapter's `MarkReadAccess::TokenHash` is a separately issued bridge
credential. It stores a SHA-256 digest only. A Personae delegation remains in
the native Notochord hello and is never converted into or delivered as a raw
Mark token.

## Surface

`ports/knot/src/mark.rs` implements:

- explicit `.md` export paths bound once to a native `PublicationId`;
- owner-triggered CommonMark snapshot creation, duplicate-content no-ops,
  stored version frontmatter, numeric history, SHA-256 ETags, conditional
  `FETCH`, version fetches, `VERSIONS`, and content-addressed current reads;
- refusal of writes and catalog discovery; and
- `MarkQuicHost`, a certificate-configured direct QUIC/TLS listener rather
  than a renamed p2panda carrier.

The listener serves complete bidirectional request streams until the client
closes its connection. LIST/LOOKUP remain later independent additions.

## Compatibility and licence gates

The implementation derives only from the public Mark specification
([CC0-1.0](https://github.com/latebit-io/demarkus/blob/main/LICENSE)), not the
Demarkus AGPL implementation. No Demarkus source is copied or linked.

Before claiming application interoperability, retain these receipts:

1. the in-process standard QUIC/TLS `mark` ALPN test in `mark.rs`;
2. a black-box request from a released Demarkus client against a trusted or
   explicitly accepted development certificate; and
3. a native-to-Mark source update that makes a deliberate second snapshot,
   followed by conditional and version-pinned fetches.

The current code proves (1). It does not claim (2) or (3) yet.
