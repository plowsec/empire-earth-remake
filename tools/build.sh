#!/bin/sh
# Build the Rust GDExtension and install it into the Godot project.
# Installs via a new file + atomic rename: overwriting a dylib in place while a
# running game has it mapped makes macOS SIGKILL the next process that loads it.
set -e
cd "$(dirname "$0")/.."
PROFILE=${1:-release}
if [ "$PROFILE" = "release" ]; then
  cargo build --release -p ee_godot
  SRC=target/release/libee_godot.dylib
else
  cargo build -p ee_godot
  SRC=target/debug/libee_godot.dylib
fi
mkdir -p game/bin
cp "$SRC" game/bin/.libee_godot.dylib.tmp
mv -f game/bin/.libee_godot.dylib.tmp game/bin/libee_godot.dylib
echo "installed libee_godot.dylib ($PROFILE)"
