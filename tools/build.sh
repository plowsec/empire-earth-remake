#!/bin/sh
# Build the Rust GDExtension and install it into the Godot project.
#   ./tools/build.sh [release|debug]
# Installs via a new file + atomic rename: overwriting a dylib in place while a
# running game has it mapped makes macOS SIGKILL the next process that loads it.
set -e
cd "$(dirname "$0")/.."
PROFILE=${1:-release}
case "$(uname -s)" in
  Darwin) LIB=libee_godot.dylib ;;
  Linux) LIB=libee_godot.so ;;
  MINGW*|MSYS*|CYGWIN*) LIB=ee_godot.dll ;;
  *) echo "unknown platform $(uname -s)" >&2; exit 1 ;;
esac
if [ "$PROFILE" = "release" ]; then
  cargo build --release -p ee_godot
else
  cargo build -p ee_godot
fi
mkdir -p game/bin
cp "target/$PROFILE/$LIB" "game/bin/.$LIB.tmp"
mv -f "game/bin/.$LIB.tmp" "game/bin/$LIB"
echo "installed $LIB ($PROFILE)"
