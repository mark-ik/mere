# Knot K2 physical two-machine receipt

Date: 2026-08-08

Status: passed.

## Scope

This closes the physical-machine remainder of K2. It proves that a Knot vault
held on one physical machine can be mounted and edited by a Graphshell
projection visitor on another physical machine, with the holder's file as
source truth and the revision bell carrying the change back to the visitor.

This is not the Knot publishing Phase A receipt. K2 deliberately gives both
machines the same fixture owner secret so the visitor can mint its admission
grant. It does not prove an independently addressed reader capability or the
raw publishing protocol.

## Machines and source

- holder: `Q-PC.local`, Darwin 24.6.0, x86_64;
- visitor: `O-PC`, Windows 11 Home Insider Preview 10.0.26220, 64-bit;
- source base: `fd7e0459328a6edab761d3ae2c0a7d8b9067f808` plus the isolated
  K2 working-tree snapshot;
- `Cargo.lock` SHA-256 on both machines:
  `3863ea4ec127b33ef68fe516a5d85ba80f89b155c01daf54aab4ba769d8e7a76`;
- `k2_peer.rs` SHA-256 on both machines:
  `1dc8fa2dcc72a9520f34aecc74f3be8d7e95fc2979cc75731b9f6a8ae81aaae0`.

The live Windows checkout changed concurrently during the first build, so the
receipt used isolated scratch checkouts on both machines. Both runners were
built from the same lockfile with:

```text
cargo build --locked -p knot --example k2_peer
```

Both builds passed. The Windows build retained existing workspace warnings;
the Q-PC build retained the same warnings plus one platform-specific unused
variable warning in `directory.rs`.

The runner initially closed the Graphshell projection session but dropped its
iroh endpoint. K2 exposed that as an ungraceful endpoint-drop diagnostic.
`P2pandaTransport::close` now waits for iroh's endpoint close, and the visitor
calls it after the bounded session thread returns.

## Passing run

The final run used an explicit, redacted endpoint ticket. Both peers shared
`K2_OWNER` and `K2_NETWORK`; Q-PC and Windows used distinct `K2_SEED` values.
The ticket proves the out-of-band ticket path. The runner does not instrument
whether iroh selected a direct address or a relay from that ticket.

Q-PC admitted the Windows subject and opened a live Knot endpoint. Windows
reported:

```text
k2_peer visit
  peer from ticket: [redacted]
  admitted
  endpoint: Knot
  opened 55 bytes of source
  save accepted by the holder
  waiting for the holder's revision bell...
  bell heard, and it carried a revision we had not seen
  session status: Live
  the holder's copy is what we wrote
  closed
```

The visitor exited `0`. Holder and visitor stderr were both empty. The Q-PC
file grew from 55 to 82 bytes and ended with:

```text
Visited at 1786165418684.
```

The file read back from Q-PC had SHA-256
`550cb33561e3c7a727d2b08d5be3fcb18293f786cea0466895f6476658b381e5`,
matching the copy captured on Windows. The holder was stopped only after the
visitor had closed.

Generated logs, binaries, tickets, source snapshots, and file copies remain
outside Git under:

```text
C:\t\mere-k2-physical-20260808-61ea7a173ac0-final4
/tmp/mere-k2-physical-20260808-61ea7a173ac0-final4
```

## Rejected runs and remaining boundary

The first physical direction, Windows holder to Q-PC visitor, parsed the ticket
but timed out before admission. The Windows file remained byte-identical. That
direction remains a reachability defect on the current network.

An earlier reverse-direction pass was discarded after validation found that
the two scratch builds had resolved different lockfiles. The first locked pass
completed the edit and bell path but lost the carrier during final close. The
clean final receipt above uses one lockfile and the explicit transport close.

K2's physical done condition is met by the Q-PC-holder to Windows-visitor run.
Bidirectional reachability, ticketless mDNS, and the independently authorized
Knot publishing protocol remain separate receipts.
