#!/bin/sh
# Setup after cloning or pulling: build the Rust extension and (re)import the game assets.
set -e
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is required: install it from https://rustup.rs and re-run this script." >&2
  exit 1
fi
if ! command -v godot >/dev/null 2>&1; then
  echo "Godot 4.7 is required on your PATH as 'godot' (https://godotengine.org/download)." >&2
  exit 1
fi
./tools/build.sh
# assets deleted upstream leave their (git-ignored) .import files behind, and Godot keeps
# loading the stale cached copy: drop every .import whose source file is gone
find game -name "*.import" -not -path "game/.godot/*" | while read -r imp; do
  [ -e "${imp%.import}" ] || { echo "removing stale import: ${imp%.import}"; rm -f "$imp"; }
done
echo "importing assets (takes a minute the first time)..."
# tell the importer about the extension up front (it otherwise registers it twice)
mkdir -p game/.godot
[ -f game/.godot/extension_list.cfg ] || echo "res://ee.gdextension" > game/.godot/extension_list.cfg
godot --headless --path game --import >/dev/null 2>&1 || true
echo "ready: godot --path game"
