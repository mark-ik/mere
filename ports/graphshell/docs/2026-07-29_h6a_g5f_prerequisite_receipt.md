# Graphshell H6a G5f prerequisite receipt

Date: 2026-07-29

Status: H6 carrier prerequisites implemented and locally exercised. The later
[H6b receipt](2026-07-29_h6b_physical_g5f_closure_receipt.md) closes the
different-device rerun.

## What was already present

The live checkout had advanced past the stale H6 plan text. The existing
`g5_peer` now keeps one resumable endpoint across admissions and runs three
separate carrier sessions:

1. open, snapshot, and suspend;
2. redial, readmit, replay contiguous diffs, invoke a real intent, and close;
3. readmit with a literal `IntentInvocation` as the first request.

The server folds the admitted grant's signed revocation immediately before
session three enters the request loop. The per-request authority gate therefore
refuses the literal intent before endpoint dispatch.

The server also snapshots the revocation ledger before awaiting admission.
It does not hold a synchronous read lock while waiting for a peer. The
subsequent request loop still reads the shared live ledger before every
application request.

## Real-carrier receipt

Two independent `g5_peer` processes used the current p2panda/Iroh QUIC carrier,
distinct Personae seeds, a hand-carried endpoint ticket, a new handshake for
each session, and the ordinary signed `SessionHello` admission.

The client observed:

- session one: `snapshot of 2 item(s)` followed by `suspended`;
- session two: `resumed by replaying 2 contiguous diff(s), revisions 1->2,
  2->3`, then `intent Accepted`;
- session three: `#8 -> refused: session authority was revoked`.

The server observed:

- session one served three requests and ended `Suspended`;
- session two served four requests and ended `Closed`;
- the owner revocation folded before session three's request loop;
- session three served one request and ended `Lapsed(Revoked)`.

Both processes exited successfully. The temporary logs were removed.

## Regression wall

`session_loop::tests::a_revoked_grant_refuses_a_literal_intent_before_dispatch`
now holds a literal `IntentInvocation` at the request-loop gate. Together with
the existing revoked-`Open` test, the focused run passed two tests.

The complete Graphshell all-features suite passed 77 tests. Warning-denying,
all-target Clippy passed with only the existing
`admit_browser_session` argument-count allowance.

## Evidence boundary

The earlier G5f receipt already proved physical Windows-to-Q-PC p2panda
admission and a later run proved resume across that real device interruption.
This receipt proves the corrected intent-first arrangement through the exact
carrier and session code on one machine.

This local proof did not substitute composition for the G5 done-condition.
The later H6b run repeated the arrangement across Windows and Q-PC so
interruption, diff replay, and literal post-revocation intent refusal appeared
in one physical-link receipt.
