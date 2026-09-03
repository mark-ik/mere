# SSH CA Projection Plan

**Date**: 2026-08-12
**Status**: T1–T5 landed 2026-08-12, the same day this was drafted; see
Progress for what the building of it corrected. Drafted the evening the
wgpu-weld parity sweep ran its Intel-iMac leg over SSH and paid the
bilateral toll three ways in one afternoon.
**Related**:
[personae founding](../technical_architecture/2026-07-08_personae_founding.md),
[device_grant_delegation_reconciliation](../../mere_docs/technical_architecture/2026-08-11_device_grant_delegation_reconciliation.md),
[device_grant_certificate_migration_plan](../../archive_docs/2026-08-18_completed_plans/2026-08-12_device_grant_certificate_migration_plan.md),
[mesh_host_lanes_plan](../../mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md),
[kith_capability_sharing_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-30_kith_capability_sharing_plan.md)

## The ask

Any of Mark's machines invokes any other, gated on nothing but an unlocked
persona. `ssh_slot` + `personae-agent` already serve vault keys over the
agent protocol, and it works — bilaterally. Every machine pair costs an
`authorized_keys` enrollment with a human typing a password, every first
contact costs a TOFU host-key prompt, and none of it knows about faces,
delegation, or revocation. Bilateralism is an accident of implementation
depth, not a design ceiling: the missing piece is one projection, personae's
delegation grammar rendered into OpenSSH's native certificate format. The
master identity is already the root of trust; this plan makes it the fleet's
login authority.

## Findings

1. **The dependency is already here.** `ssh-key` 0.6 — the `ssh` feature's
   existing dependency — carries a certificate module with a builder and
   Ed25519 signing. The projection needs no new crate; T1's first validation
   pins this claim with a test rather than trusting the docs.
2. **The reconciliation ruling constrains the shape.** The 2026-08-11 ruling
   just unwound a second delegation model (wallet device grants) at real
   migration cost; SSH access must not install a third. An OpenSSH cert is
   *minted from* a `SignedDelegationCertificate`, never issued beside one.
   M1's scope mechanics carry over: an SSH action constant in the
   `mere.device` domain (domain compares first, so a device action can never
   read as a narrowing of an unrelated persona capability), and no path
   scopes anywhere near this — `path_covers("/")` is a pinned leaf trap.
3. **Agent ceremonies are two-thirds done.** Windows has the logon scheduled
   task, macOS has `install-agent-macos.sh`; Linux has no unit. The cert work
   rides the same agent, so T4 is convergence rather than invention.
4. **The sweep's receipts, itemized.** Enrolling the M4 against the Intel
   iMac needed Mark at the keyboard (`ssh-copy-id`, a password); first
   contact needed `accept-new` TOFU; and the far end's login keychain is
   locked over SSH, so anything assuming an interactive unlock there fails
   silently. Enrollment must be one ceremony that leaves nothing interactive
   behind on the target.

## Design

- **The `ssh-ca` slot.** A CA keypair derived from master via the standard
  protocol-salt ceremony (`SSH_CA_MOD_ID` beside `SSH_MOD_ID`). The master
  attests the CA key once (`DerivedKeyAttestation` doctrine); the CA key
  signs certificates; neither ever signs application traffic.
- **User certs are projections.** Minting takes a
  `SignedDelegationCertificate` carrying the `mere.device` SSH action and
  renders it: principals from the persona face, validity short-lived,
  re-issued on unlock — deliberately the same posture the migration plan
  chose for device grants (M3), so lock/unlock is the one ceremony that
  refreshes both. Scope attenuations render to critical options:
  `force-command` and `no-agent-forwarding` for constrained faces.
- **Hosts enroll once.** `personae-vault enroll-host <target>` runs over
  bootstrap bilateral SSH and writes three things: `TrustedUserCAKeys`, a
  CA-signed host certificate, and the `AuthorizedPrincipalsFile` mapping.
  Plain SSH keeps exactly one permanent job — bootstrap. Host certs collapse
  `known_hosts` to one `@cert-authority` line, which retires TOFU fleet-wide.
- **The agent serves certs.** The agent protocol carries certificates
  natively; `personae-agent` returns key+cert pairs. Unlock mints, lock
  drops. No target-side state changes per session, ever again.
- **Revocation.** `SignedDelegationRevocation` folds to an OpenSSH KRL
  delivered to the enroll-time path. Short TTLs are the first line,
  the KRL the second — M4's shape, projected.
- **Faces are principals.** The work face, the research face, and the burner
  become distinct SSH principals with distinct constraints; which machines
  accept which face is sshd configuration written at enrollment. Access
  class becomes a property of which persona was unlocked, not which machine
  initiated.
- **Composition, not replacement.** kith/mesh-host consume the same
  `SignedDelegationCertificate`s to authorize supervised jobs: SSH is the
  interactive projection of a grant, the mesh lane the asynchronous
  supervised one, and both answer to one revocation story. Wake stays a
  mesh-host concern (a LAN peer relaying wake-on-demand; note macOS denies
  unsigned binaries local-network egress, per the H10 finding). NAT
  traversal is explicitly out of scope: this plan makes *identity*
  non-bilateral, not the network topology.

## Feature targets

**T1: CA slot and user-cert minting.** `ssh_slot` gains the CA derivation and
`mint_user_cert(&SignedDelegationCertificate) -> ssh_key::Certificate`.
*Validation*: a minted cert verifies against the CA public key in-crate; a
live sshd with `TrustedUserCAKeys` accepts it end to end on one machine.

**T2: Host enrollment.** The `enroll-host` ceremony plus host certificates.
*Validation*: a fresh machine enrolls in one command over bootstrap SSH;
afterwards any vault-holding machine reaches it with zero `authorized_keys`
entries and zero host-key prompts.

**T3: Faces to principals.** Face → principal mapping and constraint
rendering.
*Validation*: the burner's cert is refused where the work face's is accepted
on the same host; a `force-command` constraint is observed in effect.

**T4: Agent parity.** A Linux systemd user unit; the three install
ceremonies converge on serving key+cert on login.
*Validation*: on each OS, a fresh login session serves the cert through the
agent with no manual step.

**T5: Revocation.** Revocation fold → KRL render → deploy to enrolled hosts.
*Validation*: revoking the delegation closes access within one TTL without
touching the target machine by hand.

## Sequencing

T1 then T2, and daily use is real — that is the sweep's toll gone. T3–T5
harden. The kith/mesh consumption note graduates into the mesh host lanes
plan when its remote-adapter gate opens; nothing here blocks on it.

## Progress

**2026-08-12: T1–T5 all landed, same day the plan was written.** New
modules `ssh_ca`, `ssh_face`, `ssh_krl`, `enroll`; `personae-vault` gains
`ca`, `mint`, `enroll-host`, `face`, `revoke`, `krl`; the agent serves
certificates; `install-agent-linux.sh` completes the three ceremonies. 121
unit tests plus four `--ignored` live tests that run a real unprivileged
`sshd` in a temp directory: OpenSSH agreeing with our reading of its own
formats is the one thing no in-crate assertion can establish.

Four things the plan got wrong, each found by building it:

1. **`TrustedUserCAKeys` needs root; `authorized_keys` does not.**
   `sshd(8)` honours a `cert-authority` option on a line in a user's own
   `authorized_keys`, which is per-account, needs no privilege, no daemon
   reload, and nothing outside that home. That is now the default path and
   the system-wide directive is the opt-in — for whole-machine trust and
   for host certificates, which a daemon must offer and so still cost root.
   The consequence for T2's validation is worth stating plainly: the
   no-root path retires per-client `authorized_keys` sprawl but **not**
   host-key TOFU. Only the root path retires both.

2. **A grant is held by the machine carrying the credential, not the one
   being logged into.** T2 first scoped grants to the target, which reads
   well and promises something the format cannot keep: an OpenSSH
   certificate has no destination field, so any host trusting the CA and
   listing the principal accepts it. Scoping to the holder keeps the
   promise revocation actually needs — a stolen laptop's authority dies
   everywhere by revoking one grant. Restricting *which* hosts a face
   reaches is a principals question, settled at enrollment; T3's live test
   is exactly that mechanism (the burner is refused by `sshd` on the
   principal, before its empty extension set is even consulted).

3. **Revocation must key on the certificate serial, not the grant id.** A
   self-grant is re-issued on every mint, so its `DelegationId` is new each
   time: revoking one id closes one certificate while the next mint walks
   around it. The serial is derived from the device instead, so one KRL
   `serial:` line retires every certificate that machine ever carried,
   outstanding and future. The key id keeps carrying the grant id, because
   that is what `sshd` logs and what an auditor reads back. `mint_user_cert`
   now takes the ledger as a *required* field, so no caller can mint
   without considering revocation; the compiler found all five call sites.

4. **The projection is more mechanical than expected, for a reason worth
   keeping.** OpenSSH extensions are positive permissions — absent means
   denied — so an action a grant omits is an extension the certificate
   never carries. `CapabilityScope::attenuates` and OpenSSH's own
   attenuation are the same operation seen twice, which is why a face
   narrowing needs no translation layer at all.

Smaller findings: adding an `ssh-face` slot made `mint ssh` ambiguous,
because the generic slot resolver matches any `mod_id` by prefix and `ssh`
is a prefix of `ssh-face` (minting now resolves among keys only, and the
trap is worth remembering for any future `ssh-*` module); `sshd` re-reads
`RevokedKeys` per authentication, so deploying a KRL is a copy rather than
a maintenance window; and the agent's socket path is bounded by `SUN_LEN`,
which a long scratch directory will exceed with a clear error.

Not done, and honest about it:

- **The three-OS login validation is one-third met.** The certificate-
  serving agent was exercised end to end on this machine (an ED25519-CERT
  and the bare key both listed; a login carrying no `-i` and no
  `CertificateFile` accepted on CA trust alone). Windows and Linux have
  ceremonies written and not run: neither machine was available, and a
  login-session claim is exactly the kind that must not be inferred.
- **No real machine has been enrolled.** Enrollment writes to a target's
  `authorized_keys`, which is a persistent change to a machine, so it waits
  on its owner's say-so rather than riding along with the implementation.
- **The KRL deploy path stops at the file.** `krl --out` compiles a real
  KRL; putting it in a host's `sshd_config` is root-side and deliberately
  left as printed instructions.
