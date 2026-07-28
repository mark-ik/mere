# H4b SSH key management and isolated wire receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** key-management and nonstandard-endpoint slices passed; H4 remains in progress

## Product cut

Native Graphshell now exposes typed SSH key controls backed by the same
resident Personae vault used for signing:

- generate an Ed25519 key inside the native authority with a selected
  `Session`, bounded `ShortTtl`, or `PerUse` unlock policy;
- accept an imported parsed private key only through a direct native handoff;
- remove a selected key only when the public fingerprint is explicitly
  confirmed;
- return public mutation receipts containing the fingerprint, comment, public
  OpenSSH line, unlock policy, and replacement result.

The portable action model carries only public metadata. The import action has
no field capable of carrying private key bytes and directs the application to
its native picker. The picker and encrypted-key passphrase interaction were
subsequently closed by the
[H4e native import receipt](2026-07-28_h4e_native_key_import_receipt.md).

## Isolated SSH wire proof

On Windows, `PersonaeHost` can bind a uniquely named receipt pipe and records
that nonstandard listener state in its public snapshot. The receipt binder
refuses `\\.\pipe\openssh-ssh-agent`; standard-endpoint takeover remains a
separate cutover operation.

A real `ssh-agent-lib` client connected to the isolated pipe, listed the
vault-held public key, issued a signing request, waited at the `PerUse`
approval broker, and received a 64-byte signature after approval. The focused
test verified that signature against the public key. The signing history gained
the corresponding completed record.

The machine receipt records one listed identity, one wire-protocol signature,
the nonstandard endpoint class, and refusal of the standard endpoint. It does
not retain the random receipt-pipe name.

## Secret boundary

Key generation occurs inside `PersonaeHost`. Import receives an already parsed
`ssh_key::PrivateKey` directly and deliberately cannot be dispatched through a
serialized Graphshell intent. Removal is addressed by public fingerprint.

The generated [machine receipt](receipts/h4_identity_receipt.json) contains
only public key material. Its generator checks that neither the imported
private OpenSSH text nor an OpenSSH private-key header appears in the output.

## Evidence

- Graphshell isolated live-source harness: 10 tests passed.
- The isolated Windows named-pipe test listed the real vault identity and
  verified the approved wire-protocol signature.
- The receipt generator repeated the isolated client/list/sign path and wrote
  schema `graphshell.h4.identity-receipt/v3`.
- `git diff --check` passed.

The harness compiles the live Graphshell source against its real path
dependencies in an isolated manifest. This does not claim a clean,
patch-free workspace build.

## H4 gates still open

- bind the standard endpoint and prove a real SSH login after restart;
- own launch-at-login and crash recovery, then retire the standalone task;
- add device enrollment, grant, delegation, expiry, and revocation intents;
- reopen identity, device, grant, access, and signing projections together in
  one graph scene;
- prove the resident endpoint lifecycle on the other supported desktop
  platforms.

The live `personae-agent` scheduled task and standard Windows SSH-agent pipe
were inspected read-only and left unchanged.

H4f later exercised the standard endpoint reversibly and restored this task.
See the
[resident-device-host receipt](2026-07-28_h4f_resident_device_host_receipt.md).
