# N4 owner-policy receipt

Date: 2026-07-27

## Claim

One persona-scoped owner document retains Notochord admission rules and
verified revocations across restart. Service, discovery, and Retinue transit
remain separately editable axes. Carrier facts, admitted principals, streams,
and live session counts do not enter the document. The action-aware schema is
version 2 and older or future versions fail closed.

## Executed proof

The receipt is generated through the real policy edit and persistence seams:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo run -p graphshell --bin n4_policy_receipt -- ports/graphshell/docs/receipts/n4_owner_policy.html
```

The four stages show:

- Murm and Graphshell initially offered while transit is enabled;
- Murm disabled while Graphshell and transit stay unchanged;
- transit disabled while both service rules stay unchanged;
- the same owner rules restored from
  `personas/<persona>/settings/notochord.json`.

The Graphshell test suite compares fresh output byte-for-byte with
[`receipts/n4_owner_policy.html`](receipts/n4_owner_policy.html). Focused
Notochord tests also prove limits are clamped, verified revocations round-trip,
and unsupported or nondeterministically ordered documents are rejected.
Session-runtime tests prove replacement saves, restart restoration, and the
absence of live-session fields in the JSON.

The 2026-07-27 verification environment refused local `file:` navigation, so
desktop and narrow-width browser inspection remains unclaimed. This is a
harness limit; the committed HTML is still regenerated and compared by the
test suite.

## Boundary

Notochord evaluates service admission. Retinue remains responsible for
enforcing anonymous transit, and the host presents and persists the independent
owner choices. A receipt proves the settings projection and persistence seam;
it is not a claim that the final Merecat settings surface has landed.
