#!/usr/bin/env bash
# Build and (re)install the personae SSH agent + vault CLI on Linux.
#
# The third of the three install ceremonies (Windows: Task Scheduler + a VBS
# relaunch loop; macOS: launchd; here: a systemd --user unit). All three end
# in the same place — an agent running at login, serving vault keys *and*
# personae certificates, with the passphrase never written to disk.
#
# Two things differ from the macOS script, both forced by the platform.
#
# 1. systemd --user replaces launchd. Restart=on-failure covers a crashed
#    child the way launchd's KeepAlive does, so no wrapper loop is needed.
#    `loginctl enable-linger` is what makes the unit survive logout, and it
#    is left to you: it is a policy decision about whether your agent should
#    outlive your session, not something an installer should assume.
#
# 2. There is no AutoOs unlock here either (Unlock::AutoOs is DPAPI, Windows
#    only). The macOS script reads the passphrase from the login Keychain;
#    the closest equivalent here is the kernel keyring or a Secret Service
#    agent, and which one exists depends on the desktop. So this installs a
#    wrapper that shells out to `secret-tool` when it is available and falls
#    back to a systemd credential file with 0600 permissions otherwise, and
#    it says which one it chose.
#
# YOU store the passphrase yourself, once, before starting the unit:
#
#   secret-tool store --label='personae vault' service personae-vault account "$USER"
#
# or, without a Secret Service:
#
#   install -m 600 /dev/null ~/.config/personae/passphrase
#   $EDITOR ~/.config/personae/passphrase   # one line, no trailing newline
#
# This script never handles the passphrase and never prints it.
#
# Re-run any time after changing personae; it stops the unit, installs fresh
# release bins, and restarts. Safe to run repeatedly.

set -euo pipefail

MERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BIN="${XDG_DATA_HOME:-$HOME/.local/share}/personae/bin"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="$UNIT_DIR/personae-agent.service"
KEYRING_SERVICE="personae-vault"
PASSPHRASE_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/personae/passphrase"

# Matches bootstrap::default_vault_dir(): XDG_DATA_HOME or ~/.local/share,
# then personae/vault. The agent and the CLI must agree on this path.
VAULT="${XDG_DATA_HOME:-$HOME/.local/share}/personae/vault"
# default_socket() prefers XDG_RUNTIME_DIR, which systemd does set on Linux,
# so the socket lands in the runtime dir and is cleaned up on logout.
SOCKET="${XDG_RUNTIME_DIR:-$VAULT}/personae-agent.sock"

command -v systemctl >/dev/null 2>&1 || {
    echo "systemctl not found: this script installs a systemd --user unit." >&2
    echo "on a non-systemd system, run personae-agent from your session's" >&2
    echo "own startup mechanism with the same arguments the unit below uses." >&2
    exit 1
}

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

mkdir -p "$BIN" "$UNIT_DIR" "$VAULT" "$(dirname "$PASSPHRASE_FILE")"

# A running agent holds the socket; stop it before replacing the binary.
systemctl --user stop personae-agent.service 2>/dev/null || true

install -m 755 "$MERE/target/release/personae-agent" "$BIN/personae-agent"
install -m 755 "$MERE/target/release/personae-vault" "$BIN/personae-vault"
echo "installed to $BIN"

# The wrapper exists only to get the passphrase into the agent's environment
# without it being written anywhere new.
cat > "$BIN/personae-agent-start" <<WRAPPER
#!/usr/bin/env bash
set -euo pipefail
if command -v secret-tool >/dev/null 2>&1 &&
   PERSONAE_PASSPHRASE="\$(secret-tool lookup service $KEYRING_SERVICE account "\$USER" 2>/dev/null)" &&
   [ -n "\$PERSONAE_PASSPHRASE" ]; then
    :
elif [ -r "$PASSPHRASE_FILE" ]; then
    PERSONAE_PASSPHRASE="\$(head -n1 "$PASSPHRASE_FILE")"
else
    echo "no passphrase found." >&2
    echo "store one with:" >&2
    echo "  secret-tool store --label='personae vault' service $KEYRING_SERVICE account \\"\\\$USER\\"" >&2
    echo "or write one line to $PASSPHRASE_FILE (mode 600)." >&2
    exit 1
fi
export PERSONAE_PASSPHRASE
exec "$BIN/personae-agent" --dir "$VAULT" --socket "$SOCKET"
WRAPPER
chmod 755 "$BIN/personae-agent-start"

cat > "$UNIT" <<UNITEOF
[Unit]
Description=personae SSH agent (vault keys and certificates)
Documentation=https://github.com/merely-made/mere
After=graphical-session.target

[Service]
Type=simple
ExecStart=$BIN/personae-agent-start
# systemd restarts a crashed child, which is this platform's answer to the
# Windows installer's VBS relaunch loop.
Restart=on-failure
RestartSec=2
# The agent holds vault secrets in memory: keep them out of a core dump and
# off any new part of the filesystem.
LimitCORE=0
PrivateTmp=true
ProtectHome=read-write
NoNewPrivileges=true

[Install]
WantedBy=default.target
UNITEOF

echo "registering the login unit"
systemctl --user daemon-reload
systemctl --user enable --now personae-agent.service
sleep 2
systemctl --user --no-pager --lines=0 status personae-agent.service || true

echo
echo "agent started. Point your shell at it:"
echo "  export SSH_AUTH_SOCK=\"$SOCKET\""
echo
echo "then 'ssh-add -l' lists vault keys, each once as an ED25519-CERT"
echo "(the personae certificate) and once as the bare key."
echo
echo "add that export to ~/.bashrc to make it stick — it replaces the system"
echo "agent for that shell, so keys held only by the system agent stop"
echo "resolving there."
echo
echo "to keep the agent running after you log out:  loginctl enable-linger \$USER"
echo "logs:  journalctl --user -u personae-agent -f"
