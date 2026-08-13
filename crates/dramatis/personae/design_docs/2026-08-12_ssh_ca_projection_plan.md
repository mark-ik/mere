# SSH CA Projection Plan

**Date**: 2026-08-12
**Status**: Open. Drafted the evening the wgpu-weld parity sweep ran its
Intel-iMac leg over SSH and paid the bilateral toll three ways in one
afternoon.
**Related**:
[personae founding](2026-07-08_personae_founding.md),
[device_grant_delegation_reconciliation](../../../../design_docs/mere_docs/technical_architecture/2026-08-11_device_grant_delegation_reconciliation.md),
[device_grant_certificate_migration_plan](../../../../design_docs/mere_docs/implementation_strategy/2026-08-12_device_grant_certificate_migration_plan.md),
[mesh_host_lanes_plan](../../../../design_docs/mere_docs/implementation_strategy/2026-08-09_mesh_host_lanes_plan.md),
[kith_capability_sharing_plan](../../../../design_docs/mere_docs/implementation_strategy/2026-06-30_kith_capability_sharing_plan.md)

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
