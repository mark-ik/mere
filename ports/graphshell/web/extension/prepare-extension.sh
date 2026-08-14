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
web_root=$(CDPATH= cd -- "$script_root/.." && pwd)
mkdir -p "$destination_path"
cp "$script_root/background.js" "$destination_path/background.js"
cp "$script_root/capture-model.js" "$destination_path/capture-model.js"
cp "$script_root/resource-chunk.js" "$destination_path/resource-chunk.js"
cp "$script_root/bridge.html" "$destination_path/bridge.html"
cp "$script_root/bridge.css" "$destination_path/bridge.css"
cp "$script_root/bridge.js" "$destination_path/bridge.js"
cp "$web_root/index.html" "$destination_path/graph.html"
cp "$web_root/styles.css" "$destination_path/styles.css"
cp "$web_root/GraphshellSans.ttf" "$destination_path/GraphshellSans.ttf"
cp "$web_root/loader.js" "$destination_path/loader.js"
cp "$web_root/extension-profile.js" "$destination_path/extension-profile.js"
mkdir -p "$destination_path/pkg"
cp -R "$web_root/pkg/." "$destination_path/pkg/"
cp "$script_root/manifest.$browser_name.json" "$destination_path/manifest.json"
printf '%s\n' "$destination_path"
