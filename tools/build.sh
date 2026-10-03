#!/bin/sh
# Build the Rust GDExtension and install it into the Godot project.
set -e
cd "$(dirname "$0")/.."
PROFILE=${1:-release}
if [ "$PROFILE" = "release" ]; then
  cargo build --release -p ee_godot
  cp target/release/libee_godot.dylib game/bin/
else
  cargo build -p ee_godot
  cp target/debug/libee_godot.dylib game/bin/
fi
echo "installed libee_godot.dylib ($PROFILE)"
