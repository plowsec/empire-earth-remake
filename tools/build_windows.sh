#!/bin/sh
# Cross-build for Windows from macOS or Linux.
#   ./tools/build_windows.sh            build game/bin/ee_godot.dll
#   ./tools/build_windows.sh --export   ...and export a playable build to build/windows/
# Needs: rustup target add x86_64-pc-windows-gnu, and mingw-w64
# (brew install mingw-w64 / apt install mingw-w64). Exporting also needs Godot's
# export templates for your Godot version (Editor > Manage Export Templates).
set -e
cd "$(dirname "$0")/.."
command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || { echo "mingw-w64 is required (brew install mingw-w64)" >&2; exit 1; }
rustup target list --installed 2>/dev/null | grep -q x86_64-pc-windows-gnu || rustup target add x86_64-pc-windows-gnu
cargo build --release -p ee_godot --target x86_64-pc-windows-gnu
mkdir -p game/bin
cp target/x86_64-pc-windows-gnu/release/ee_godot.dll game/bin/.ee_godot.dll.tmp
mv -f game/bin/.ee_godot.dll.tmp game/bin/ee_godot.dll
echo "installed game/bin/ee_godot.dll"
if [ "$1" = "--export" ]; then
  mkdir -p build/windows
  godot --headless --path game --export-release "Windows Desktop" "$(pwd)/build/windows/EmpireEarthAtomic.exe"
  echo "exported: build/windows/ (copy the whole folder to the Windows machine)"
fi
