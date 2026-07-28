# H4a Personae authority and approval receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** first authority slice passed; H4 remains in progress

## Product cut

Native Graphshell now composes one resident Personae authority in-process:

- one vault shared by the host and per-connection SSH adapter sessions;
- a public identity read model for vault posture, profiles, SSH public keys,
  recovery lineage, carry personas, devices, grant summaries, pending signing,
  and signing history;
- portable identity cards and typed approve-once, approve-until-idle, and deny
  intents;
- an approval broker used by the real `ssh-agent-lib` signing path.

The carry adapter calls only the identity-wallet, device-roster, and signed
grant APIs. It does not call the seed, local-device-secret, or private-epoch
bridge loaders. Missing carrier context is rendered as `unknown`.

## Approval behavior

`PerUse` now blocks at the adapter until the broker receives a visible
decision. Approval resumes signing and appends one signed result. Denial returns
an SSH adapter error and appends one denied result.

`ShortTtl` may cache only the key's configured idle window. A focused test
approved a one-second window, reused it, waited past real idle expiry, and
observed a new pending request. A per-use request cannot be widened to that
cached scope. Session policy keeps the prior unlocked-session behavior.

The native Graphshell proof projected a real pending request, serialized its
typed approve-once action, applied that intent to `PersonaeHost`, received a
64-byte signature from the waiting SSH adapter, cleared the pending queue, and
observed one signed history record.

## Secret boundary

Signing requests contain a BLAKE3 payload digest, never the cleartext payload.
The projected SSH slot contains its public OpenSSH line, fingerprint, comment,
lineage, and unlock policy. The private OpenSSH encoding and profile seed
sentinels were absent from the serialized snapshot.

The generated machine receipt confirms:

- cleartext payload absent;
- private material absent;
- one completed signing record;
- standalone agent retained;
- standard endpoint unchanged.

The generated surface is
[h4_identity_surface.html](receipts/h4_identity_surface.html), paired with the
[machine receipt](receipts/h4_identity_receipt.json).

## Evidence

- Personae approval broker: 4 focused tests passed.
- Personae SSH adapter: 8 focused tests passed, including approve, deny, and
  signature verification.
- Graphshell identity authority: 6 focused tests passed, including the live
  projected-intent-to-SSH-adapter path and public carry projection.
- The receipt generator ran the live pending/approve/sign/history scenario and
  wrote both committed artifacts.
- `rustfmt` and `git diff --check` passed for the H4a files.

Concurrent workspace Cargo jobs held shared package and build locks during this
pass. The tests therefore ran in isolated manifests that compiled the live
Personae and Graphshell source files against their real path dependencies. This
receipt does not claim a clean patch-free workspace test run.

The HTML renderer and control model are test-covered, but the in-app browser
could not attach a webview during the visual check. This receipt does not claim
a headed browser pass.

## H4 gates still open

- bind native Graphshell to the real standard SSH-agent endpoint;
- reach the local identity authority from browser Graphshell through an
  admitted session;
- prove a real SSH login after Graphshell restart;
- prove launch-at-login and crash recovery, then retire the standalone task;
- add public-key import, generation, removal, device enrollment, grant,
  delegation, expiry, and revocation intents;
- reopen identity, device, grant, access, and signing projections together in
  one graph scene.

The standalone agent remains installed and authoritative until those receipts
pass.
