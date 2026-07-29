# Graphshell H7b personal-sync closure receipt

Date: 2026-07-29

Status: H7 complete.

## Resident authority

`PersonalSyncHost` now keeps one Personae-bound personal-graph replica, Redb
store, p2panda transport, and Stickleback `JoinedSpace` alive in the resident
Graphshell device host. The device host enables it with `--sync-graph` and
accepts explicit store, roster-root, peer-ticket, facet, access-record, saved
scene, handler-preference, and blob-availability selections.

The structural graph lane is always present once personal sync is enabled.
Every optional history or preference lane remains user selected. The local
Personae public root joins the roster automatically. The host prints its own
endpoint ticket, and each paired device is configured with the other device's
ticket.

Redb reopen tests reproduce the same graph and access projection, then resume
the existing writer head with the next sequence and backlink. The resident host
also has an explicit asynchronous close: it leaves LogSync and waits until the
database lock is released before declaring the store restartable.

## Browser and blob boundaries

The browser broker reads the resident sync projection into action-free
PortableCards. Identity cards retain their existing `personae:card:*`
presentation keys; sync cards use `device:card:*`. Supplemental data is added
only after the browser challenge and the existing `SessionHello` admission
produce a `SessionAuthority`.

The admitted-browser test runs the actual native-messaging framing, browser
challenge, `SessionHello`, session open, snapshot, resource request, and close.
The browser receives the personal-sync card only inside that admitted device
scene. This is composition through the existing authority, not another browser
admission path.

Blob availability is a separate immutable metadata event. Its fold retains
which device last reported each addressed blob available or unavailable.
Neither blob bytes nor local filesystem locations enter personal graph sync.

## Physical two-device proof

`h7_sync_peer` authors into independent Redb stores before either transport is
bound. Windows and Q-PC each create the same two base nodes, then make distinct
offline edits:

- concurrent titles for the same node;
- one device-attributed tag and access record each;
- one availability observation each for the same blob;
- the Windows side's semantic relation.

Each process closes and reopens its Redb store before connecting. The two
physical machines then exchange endpoint tickets in both directions, join the
same LogSync topic, and produce byte-identical projection receipts:

```json
{
  "graph": "46e4f9b71d6d06abc211eb5dc1d5d0e69780b5e23def2aa375c639dcd6e142d1",
  "nodes": 2,
  "relations": 1,
  "tags": ["qpc", "windows"],
  "accesses": [["qpc", 100], ["windows-laptop", 200]],
  "conflict_targets": [
    "node/00000000-0000-0000-0000-000000007001/title"
  ],
  "blob_devices": ["qpc", "windows-laptop"],
  "writers": 4,
  "pending": 0
}
```

A preceding one-way-ticket attempt produced zero sync rounds on both peers.
That failure establishes an operational boundary: direct request carriers can
bootstrap from one server ticket, while gossip peer sampling needs both
address books populated. The successful receipt therefore uses bilateral
ticket exchange, as two configured resident hosts do.

The Windows logs and Redb store remain outside Git under
`C:\t\mere-h7-physical-bidir-cc8f290a043a40468d3c9e3387f84989`.
Q-PC used the isolated source tree under
`/tmp/mere-h7-physical-bd541032cc8e45bfaf7203509af45124`; its clean main
checkout was untouched.

## Verification

```text
cargo test -p graphshell --lib --features personal-sync
89 passed; 0 failed

cargo check -p graphshell --bin graphshell_device_host \
  --features personal-sync
passed

cargo check -p graphshell --bin h7_sync_peer --features personal-sync
passed on Windows

cargo build -p graphshell --bin h7_sync_peer --features personal-sync
passed on Q-PC

cargo check -p graphshell --target wasm32-unknown-unknown \
  --no-default-features --features web
passed

cargo clippy -p graphshell --lib --features personal-sync --no-deps -- \
  -D warnings -A clippy::too_many_arguments
passed

cargo fmt -p graphshell -- --check
passed
```

## Remaining boundary

H7 synchronizes selected public graph metadata. Personae carry, enrollment,
secret wrapping, revocation, and recovery remain a separate high-assurance
lane. Browser sync cards are read-only in this slice; typed graph editing
through application and agent adapters belongs to H9.
