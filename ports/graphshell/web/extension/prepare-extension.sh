#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: $0 chromium|firefox /path/to/output" >&2
  exit 2
fi

browser_name=$1
destination_path=$2
case "$browser_name" in
  chromium|firefox) ;;
  *)
    echo "browser must be chromium or firefox" >&2
    exit 2
    ;;
esac

script_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$destination_path"
cp "$script_root/background.js" "$destination_path/background.js"
cp "$script_root/bridge.html" "$destination_path/bridge.html"
cp "$script_root/bridge.css" "$destination_path/bridge.css"
cp "$script_root/bridge.js" "$destination_path/bridge.js"
cp "$script_root/manifest.$browser_name.json" "$destination_path/manifest.json"
printf '%s\n' "$destination_path"
