#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 /absolute/path/to/graphshell_native_host" >&2
  exit 2
fi

binary_path=$1
case "$binary_path" in
  /*) ;;
  *)
    echo "native host path must be absolute" >&2
    exit 2
    ;;
esac
if [ ! -x "$binary_path" ]; then
  echo "native host is not executable: $binary_path" >&2
  exit 2
fi

script_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
escaped_binary=$(printf '%s' "$binary_path" | sed 's/\\/\\\\/g; s/"/\\"/g')

install_manifest() {
  template=$1
  destination=$2
  mkdir -p "$(dirname -- "$destination")"
  sed "s|__GRAPHSHELL_NATIVE_HOST__|$escaped_binary|g" "$template" > "$destination"
  echo "installed $destination"
}

case "$(uname -s)" in
  Darwin)
    install_manifest \
      "$script_root/native-host.chromium.json.in" \
      "$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts/org.mere.graphshell.json"
    install_manifest \
      "$script_root/native-host.chromium.json.in" \
      "$HOME/Library/Application Support/Chromium/NativeMessagingHosts/org.mere.graphshell.json"
    install_manifest \
      "$script_root/native-host.firefox.json.in" \
      "$HOME/Library/Application Support/Mozilla/NativeMessagingHosts/org.mere.graphshell.json"
    ;;
  Linux)
    config_root=${XDG_CONFIG_HOME:-"$HOME/.config"}
    install_manifest \
      "$script_root/native-host.chromium.json.in" \
      "$config_root/google-chrome/NativeMessagingHosts/org.mere.graphshell.json"
    install_manifest \
      "$script_root/native-host.chromium.json.in" \
      "$config_root/chromium/NativeMessagingHosts/org.mere.graphshell.json"
    install_manifest \
      "$script_root/native-host.firefox.json.in" \
      "$HOME/.mozilla/native-messaging-hosts/org.mere.graphshell.json"
    ;;
  *)
    echo "unsupported platform: $(uname -s)" >&2
    exit 2
    ;;
esac
