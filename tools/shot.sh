#!/bin/sh
# Render a screenshot of the running game: tools/shot.sh out.png [frames] [extra user args...]
cd "$(dirname "$0")/.."
OUT=$(cd "$(dirname "$1")" && pwd)/$(basename "$1"); shift
FRAMES=${1:-90}; shift 2>/dev/null
godot --path game --resolution 1600x900 -- --shot="$OUT" --shot-frames="$FRAMES" "$@" 2>&1 | grep -vE "^\s*$" | tail -25
