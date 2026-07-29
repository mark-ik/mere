# Graphshell H4i remote SSH login receipt

Date: 2026-07-29

Status: H4 remote-login done-condition complete.

## Live authority

The installed `graphshell-device-host` task was enabled and running.
`personae-agent` remained retained but disabled, and the Windows stock
`ssh-agent` service was stopped and disabled.

The standard OpenSSH agent endpoint listed the existing vault-held Ed25519 key:

`SHA256:d3tQOqvSRA4QE1D7R1j2SJh31wLCXTTZofJxvQLfd0o`

Q-PC had moved from its historical `192.168.4.38` lease to
`192.168.4.105`; `Q-PC.local` resolved the current address.

## Remote login

Windows OpenSSH connected to `markik@192.168.4.105` in batch mode. Q-PC
accepted the public-key signature supplied through Graphshell's standard agent
endpoint and returned:

```text
GRAPHSHELL_REMOTE_LOGIN_OK
Darwin 24.6.0 x86_64
```

This is a real remote authentication result, not an isolated signing probe.
The remaining lifecycle wall is a real sign-out or reboot followed by
Graphshell startup, fingerprint continuity, crash recovery, and final removal
of the retained disabled Personae task.
