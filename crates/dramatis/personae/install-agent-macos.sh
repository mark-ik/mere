#!/usr/bin/env bash
# Build and (re)install the personae SSH agent + vault CLI on macOS.
#
# The macOS counterpart to install-agent-windows.ps1. Same shape: build the
# release bins from the mere workspace, install them, and register a
# launch-at-login job. Two things differ, and both are forced by the platform.
#
# 1. launchd replaces the VBS relaunch loop. Task Scheduler's restart policy
#    only covers failure to launch, not a crashed child, which is why the
#    Windows installer wraps the agent in a WScript loop. launchd's KeepAlive
#    genuinely restarts a crashed child, so the wrapper is unnecessary here.
#
# 2. There is no AutoOs unlock on macOS. `Unlock::AutoOs` is DPAPI, Windows
#    only; on every other platform bootstrap returns
#      "no OS auto-unlock backend on this platform yet; set PERSONAE_PASSPHRASE"
#    So the agent needs a passphrase at start. Writing it into the plist would
#    put the vault's KEK material in plaintext on disk and undo the point of a
#    sealed vault, so this installs a wrapper that reads it from the login
#    Keychain instead — the closest macOS equivalent of what DPAPI does for the
#    Windows path.
#
# YOU must store the passphrase yourself, once, before loading the job:
#
#   security add-generic-password -a "$USER" -s personae-vault -w
#
# (omit -w's value and it prompts, so the passphrase never lands in your shell
# history). This script never handles the passphrase and never prints it.
#
# Re-run any time after changing personae; it stops the job, copies fresh
# release bins, and reloads. Safe to run repeatedly.

set -euo pipefail

MERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BIN="$HOME/Library/Application Support/personae/bin"
LOGDIR="$HOME/Library/Logs/personae"
PLIST="$HOME/Library/LaunchAgents/org.merely.personae-agent.plist"
LABEL="org.merely.personae-agent"
KEYCHAIN_SERVICE="personae-vault"

# Matches bootstrap::default_vault_dir() — XDG_DATA_HOME or ~/.local/share,
# then personae/vault. Deliberately NOT ~/Library/Application Support: the
# crate uses the XDG path on every non-Windows platform, and the agent and the
# vault CLI must agree on where the vault is.
VAULT="${XDG_DATA_HOME:-$HOME/.local/share}/personae/vault"
# default_socket() prefers XDG_RUNTIME_DIR, which macOS does not set, so it
# falls through to <vault-dir>/agent.sock.
SOCKET="$VAULT/agent.sock"

# launchd jobs and non-login shells do not read ~/.zprofile, so never assume
# cargo is already on PATH here even once a profile exists.
if ! command -v cargo >/dev/null 2>&1; then
    # shellcheck source=/dev/null
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi
command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found, and no $HOME/.cargo/env to source." >&2
    echo "install rust first: https://rustup.rs" >&2
    exit 1
}

echo "building personae bins (release, agent feature) from $MERE"
( cd "$MERE" && cargo build -p personae --features agent --release )

mkdir -p "$BIN" "$LOGDIR" "$VAULT" "$(dirname "$PLIST")"

# A running agent holds the socket; stop it before replacing the binary.
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
sleep 1

install -m 755 "$MERE/target/release/personae-agent" "$BIN/personae-agent"
install -m 755 "$MERE/target/release/personae-vault" "$BIN/personae-vault"
echo "installed to $BIN"

# The wrapper exists only to get the passphrase out of the Keychain and into
# the agent's environment without it ever being written to disk.
cat > "$BIN/personae-agent-start" <<WRAPPER
#!/bin/bash
set -euo pipefail
PERSONAE_PASSPHRASE="\$(security find-generic-password -a "\$USER" -s $KEYCHAIN_SERVICE -w)" || {
    echo "no passphrase in the Keychain under service '$KEYCHAIN_SERVICE'." >&2
    echo "add it with: security add-generic-password -a \\"\\\$USER\\" -s $KEYCHAIN_SERVICE -w" >&2
    exit 1
}
export PERSONAE_PASSPHRASE
exec "$BIN/personae-agent" --dir "$VAULT" --socket "$SOCKET" --log-file "$LOGDIR/agent.log"
WRAPPER
chmod 755 "$BIN/personae-agent-start"

cat > "$PLIST" <<PLISTEOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>$LABEL</string>
    <key>ProgramArguments</key>
    <array><string>$BIN/personae-agent-start</string></array>
    <key>RunAtLoad</key><true/>
    <!-- launchd restarts a crashed child, unlike Task Scheduler. This is the
         VBS relaunch loop's replacement. -->
    <key>KeepAlive</key>
    <dict><key>SuccessfulExit</key><false/></dict>
    <key>StandardErrorPath</key><string>$LOGDIR/agent.stderr.log</string>
    <key>ProcessType</key><string>Background</string>
</dict>
</plist>
PLISTEOF

echo "registering login agent"
launchctl bootstrap "gui/$(id -u)" "$PLIST"
sleep 2

echo
echo "agent started. Point your shell at it:"
echo "  export SSH_AUTH_SOCK=\"$SOCKET\""
echo
echo "then 'ssh-add -l' should list vault keys."
echo "Add that export to ~/.zshrc to make it stick — but note it replaces the"
echo "system agent for that shell, so keys held only by the system agent stop"
echo "resolving there."
echo
echo "log: $LOGDIR/agent.log"
