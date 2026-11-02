#!/usr/bin/env bash
# Drive the real UI under Wine: click Add, type a URL, click the dialog's Add
# button, then report the resulting state. Usage: uitest.sh <display> <prefix> [theme]
set -uo pipefail

DISPLAY_NUM="${1:-95}"
PREFIX="${2:-/tmp/wine-a}"
THEME="${3:-dark}"
export DISPLAY=":$DISPLAY_NUM" WINEPREFIX="$PREFIX" TREE_DEBUG=1 WINEDEBUG=-all

pgrep -x "Xvfb" >/dev/null || { Xvfb ":$DISPLAY_NUM" -screen 0 1400x900x24 </dev/null >/tmp/xvfb$DISPLAY_NUM.log 2>&1 & sleep 2; }
pgrep -x openbox >/dev/null || { setsid openbox </dev/null >/tmp/ob$DISPLAY_NUM.log 2>&1 & sleep 1; }

"$WINEPREFIX/drive_c/windows/system32/../../../../../../usr/lib/wine/wine64" --version >/dev/null 2>&1
wineserver -k9 2>/dev/null; sleep 2
# Wine names the process "main", not "tree.exe": match the command line.
for p in $(pgrep -f "tree.exe" 2>/dev/null); do kill -9 "$p" 2>/dev/null; done
sleep 1
for _ in $(seq 1 10); do pgrep -f "tree.exe" >/dev/null || break; sleep 0.5; done
echo "stale instances: $(pgrep -f "tree.exe" | wc -l)"

DATA="$PREFIX/drive_c/users/apps/AppData/Roaming/Tree"
mkdir -p "$DATA" "$PREFIX/drive_c/users/apps/Downloads"
cat > "$DATA/state.json" <<JSON
{"version":2,"settings":{"dir":"C:\\\\users\\\\apps\\\\Downloads","conns":8,"simultaneous":3,
"speed_limit":0,"watch_clipboard":false,"minimize_to_tray":false,"theme":"$THEME",
"sort_col":0,"sort_desc":false,"col_widths":[340,90,80,110,110,80],
"win_w":0,"win_h":0,"win_x":60,"win_y":40},"tasks":[]}
JSON
rm -f "$DATA/startup.log"

cd ~/projects/tree/dist || exit 1
(setsid /usr/lib/wine/wine64 tree.exe </dev/null >/tmp/uitest-run.log 2>&1 &)

WIN=""
for _ in $(seq 1 30); do
  WIN=$(xdotool search --name "^Tree$" 2>/dev/null | tail -1)
  [ -n "$WIN" ] && break
  sleep 0.5
done
echo "window=[$WIN]"
[ -z "$WIN" ] && { tail -5 /tmp/uitest-run.log; exit 1; }

eval "$(xdotool getwindowgeometry --shell "$WIN")"
echo "geometry=$X,$Y ${WIDTH}x${HEIGHT}"
xdotool windowactivate --sync "$WIN"; sleep 1

# Open the dialog with its shortcut; the button itself is exercised below.
xdotool key ctrl+n; sleep 2
DLG=$(xdotool search --name "Add download" 2>/dev/null | tail -1)
echo "dialog=[$DLG]"
[ -z "$DLG" ] && { echo "FAIL: dialog did not open"; cat "$DATA/startup.log"; exit 1; }

eval "$(xdotool getwindowgeometry --shell "$DLG")"
echo "dialog-geometry=$X,$Y ${WIDTH}x${HEIGHT}"
xdotool type --delay 30 "http://127.0.0.1:8099/blob.bin"; sleep 1

# Click the dialog's Add button. The dialog is spawned at a fixed position by
# center_on(), so this offset is stable across runs (verified on screenshots).
xdotool mousemove $((X + 391)) $((Y + 226)) click 1; sleep 4

if xdotool search --name "Add download" >/dev/null 2>&1; then
  echo "RESULT: dialog still open (Add did not commit)"
else
  echo "RESULT: dialog closed"
fi

python3 - "$DATA/state.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print("tasks:", [(t["name"], t["status"], t["done"], t["total"]) for t in d["tasks"]])
PY

echo "--- startup log ---"
cat "$DATA/startup.log"
