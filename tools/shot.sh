#!/bin/sh
# Render a screenshot of the running game: [SCENE=menu] tools/shot.sh out.png [frames] [extra user args...]
cd "$(dirname "$0")/.."
OUT=$(cd "$(dirname "$1")" && pwd)/$(basename "$1"); shift
FRAMES=${1:-90}; shift 2>/dev/null
( sleep ${SHOT_TIMEOUT:-120}; pkill -f "shot=$OUT" ) >/dev/null 2>&1 &
WATCH=$!
godot --path game --resolution 1600x900 -- --shot="$OUT" --shot-frames="$FRAMES" --scene=${SCENE:-main} "$@" 2>&1 | grep -vE "^\s*$" | grep -iE "error|saved|warn" | head -20
kill $WATCH 2>/dev/null
