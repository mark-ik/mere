# Graphshell H6b physical G5f closure receipt

Date: 2026-07-29

Status: G5 complete; H6 carrier gate closed.

## Physical run

The Windows machine ran the current split `g5_peer` client. Q-PC at
`192.168.4.105` ran the corrected monolithic server from commit `86d77d41`.
Both used distinct Personae seeds, the same owner and network, a hand-carried
p2panda/Iroh endpoint ticket, and a fresh signed `SessionHello` admission for
each carrier connection.

The asymmetric revisions preserve the same protocol and also prove the
refactor did not break wire compatibility.

Session one opened, returned a two-item snapshot, and suspended. Windows then
dropped that carrier connection, redialed, completed a new transport handshake
and admission, and received:

```text
#5 -> resumed by replaying 2 contiguous diff(s), revisions 1->2, 2->3
#6 -> intent Accepted
```

After session two closed, session three admitted the same peer. Q-PC folded
the owner's signed grant revocation before entering the request loop. The
client's first application request was the literal intent:

```text
--- session 3: intent first ---
#8 -> refused: session authority was revoked
```

Q-PC recorded one answered request and
`ended Lapsed(Revoked)`. Both processes exited with status zero.

## Done-condition

The same two physical devices now complete ticket exchange, open a granted
projection, resume after an actual carrier interruption, and reject an actual
post-revocation `IntentInvocation`. This closes G5 and H6's carrier
prerequisite.

Cross-LAN mDNS discovery remains convenience coverage. G5 explicitly permits
discovery or ticket exchange, so it is not part of this closure.
