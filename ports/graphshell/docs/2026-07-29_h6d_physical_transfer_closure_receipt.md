# Graphshell H6d physical transfer closure receipt

Date: 2026-07-29

Status: H6 complete.

## Implemented carrier seam

`TransferSourceEndpoint` exposes one prepared transfer through Graphshell's
existing admitted vocabulary:

- the projection snapshot advertises `graphshell.transfer.begin`;
- an accepted typed intent discloses the transfer;
- a PortableCard addresses the manifest first and each blob independently;
- resource responses are checked against their BLAKE3 addresses;
- resume retains the disclosed projection across a fresh carrier admission.

`h6_transfer_peer` is the bounded two-machine receipt executable. Its source
constructs one URL Container and one real-file Container, tags both, asserts a
`Cites` relation, prepares a replicate manifest, and serves it across three
admitted sessions. Its destination uses independent staging, Mere, Muniment,
and Eidetic authority stores.

The transfer is interrupted after the destination has accepted disclosure and
cached the manifest but before it requests the file blob. The first connection
ends with `Suspend`. A new p2panda/QUIC connection performs a fresh Notochord
admission, resumes the existing projection at epoch 1/revision 1 without a
replacement snapshot, and fetches the remaining blob. The destination then
applies the original manifest rather than preparing or requesting it again.

## Executable proof

The granted and revoked variants first passed between two Windows processes.
The same executable was then compiled on <remote-host> with Rust 1.97.1 in a detached
temporary worktree. <remote-host>'s clean `main` checkout remained untouched.

The physical source ran on Windows and the destination ran on <remote-host> at
`<private-address>`. The transferred file was the 3,389-byte H6c receipt.

Granted run:

```text
session 1: admitted; transfer disclosure accepted
cached manifest 0e1a1889-a928-4820-81e8-4ec97b811f52
connection suspended before 1 blob

session 2: admitted on a new connection
resumed current projection without a fresh snapshot
fetched blob blake3:fac9a09ee0677f76ebb6a8595a6633260edebb278707245c22e6495b1ad6733d
applied: 2 objects, 1 relation, 2 destination AccessRecords

session 3: admitted; transfer intent first
granted transfer intent accepted
```

The typed receipt records:

```text
operation: replicate
manifest: blake3:454b024d01ee0ce99b19b1bf91fff4dd883d5be72609dcff31c42421d207ea19
selection: blake3:1b908e18c97cde9082bbfd2159c9b5c96eadaeb4a969daa3664b2ccfdc6b9555
nodes: 2
relations: 1
id map: 2 identity mappings
destination AccessRecords: 2
```

The destination executable also asserts that both imported Containers retain
the `h6` and `physical` tags, the graph contains exactly one relation, every
replicated id is unchanged, and the destination Muniment store holds the
addressed blob.

Revoked physical run:

```text
session 1: manifest cached; suspended before the blob
session 2: resumed; blob verified; destination applied
session 3: admitted; transfer intent first
server: grant revoked before the transfer intent
server: served 1; ended Lapsed(Revoked)
client: revoked transfer intent refused before endpoint dispatch
```

The existing transfer-core test separately proves that a revoked
`TransferAuthorization` returns before destination blob reads or graph
mutation. Together, the physical request-loop result and the two-store
mutation guard cover both sides of the revocation boundary.

Verification:

```text
cargo test -p graphshell --all-features transfer --no-fail-fast
6 passed; 0 failed

cargo test -p graphshell --all-features
83 passed; 0 failed

cargo check -p graphshell --bin h6_transfer_peer --all-features
passed on Windows and <remote-host>

cargo build -p graphshell --bin h6_transfer_peer --all-features
passed on Windows and <remote-host>

cargo fmt -p graphshell -- --check
passed
```

Generated source logs and typed receipt JSON remain outside Git under
`C:\t\mere-h6-physical-windows-qpc*` on Windows and the isolated H6 worktree
under `/tmp/mere-h6-physical-20260729` on <remote-host>.

## Boundary

H6 proves replicate over the physical admitted carrier. Copy is covered by the
two-store executable tests because it changes destination identity and
provenance, not carrier framing. Source retirement remains a separate,
explicitly authorized operation.
