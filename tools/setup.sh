#!/bin/sh
# One-time setup after cloning: build the Rust extension and import the game assets.
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
echo "importing assets (first run only, takes a minute)..."
# tell the importer about the extension up front (it otherwise registers it twice)
mkdir -p game/.godot
[ -f game/.godot/extension_list.cfg ] || echo "res://ee.gdextension" > game/.godot/extension_list.cfg
godot --headless --path game --import >/dev/null 2>&1 || true
echo "ready: godot --path game"
