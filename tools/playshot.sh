#!/bin/bash
# Plays the first map in maps/ with the autopilot in a throwaway folder and saves F12 screenshots.
#   tools/playshot.sh [look index 0-8] [seconds before the first shot, default 8]
# The window opens on the SECOND monitor under a unique title; screenshots land in the printed folder.
# Needs xdotool and a built target/release/rhythm. Never touches your settings.json, scores.json or maps/.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export DISPLAY="${DISPLAY:-:0}"
export XAUTHORITY="${XAUTHORITY:-$(ls /run/user/"$(id -u)"/xauth_* 2>/dev/null | tail -1)}"
DIR="$(mktemp -d /tmp/rhythm-playshot-XXXX)"
mkdir -p "$DIR/maps"
cp "$(ls "$ROOT"/maps/*.sspm "$ROOT"/maps/*.mp3 2>/dev/null | head -1)" "$DIR/maps/"
printf '{"music_volume": 0.1, "sfx_volume": 0.2, "no_fail": true, "look": %s}\n' "${1:-0}" > "$DIR/settings.json"
TITLE="playshot-$(basename "$DIR")"
cd "$DIR"
RHYTHM_TITLE="$TITLE" RHYTHM_BENCH=play:60 "$ROOT/target/release/rhythm" "$(ls "$DIR"/maps/* | head -1)" > "$DIR/out.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do
  WID="$(xdotool search --name "^$TITLE$" | tail -1)"
  [ -n "$WID" ] && break
  sleep 0.25
done
xdotool windowmove "$WID" 1920 58
sleep "${2:-8}"
xdotool key --window "$WID" F12
sleep 2
xdotool key --window "$WID" F12
sleep 1
echo "$DIR/screenshots (game pid $PID, window $WID, title $TITLE)"
echo "stop it with: kill $PID"
