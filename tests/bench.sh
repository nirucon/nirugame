#!/bin/sh
# Performance check in a private Xvfb with a synthetic library of about 3000
# fake (empty) ROM files and a RetroArch playlist that lists them. Prints the
# first frame, the time until the library is shown (when the program reports
# it), memory and the CPU time of 60 key presses through the list.
#   tests/bench.sh PATH/TO/nirugame [RUNS] [DISPLAY_NUMBER]
# Nothing outside the temporary directory is touched.
set -eu
BIN=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
RUNS=${2:-3}
N=${3:-$((90 + $$ % 9))}
while [ -e "/tmp/.X$N-lock" ]; do N=$((N + 1)); done
D=:$N
SB=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$SB"; }
trap cleanup EXIT INT TERM
export HOME=$SB/home XDG_CONFIG_HOME=$SB/config XDG_DATA_HOME=$SB/data XDG_CACHE_HOME=$SB/cache \
    XDG_STATE_HOME=$SB/state XDG_RUNTIME_DIR=$SB/run NIRUGAME_CORE_DIRS=""
mkdir -p "$HOME" "$XDG_CONFIG_HOME/retroarch/playlists" "$XDG_CONFIG_HOME/retroarch/cores" "$XDG_DATA_HOME" "$SB/bin" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
printf '#!/bin/sh\nexit 0\n' > "$SB/bin/retroarch"
chmod +x "$SB/bin/retroarch"
export PATH="$SB/bin:$PATH"
: > "$XDG_CONFIG_HOME/retroarch/cores/snes9x_libretro.so"
: > "$XDG_CONFIG_HOME/retroarch/cores/mesen_libretro.so"
python3 - "$HOME/Games/roms" "$XDG_CONFIG_HOME/retroarch/playlists/All.lpl" <<'EOF'
import json, os, sys
root, playlist = sys.argv[1], sys.argv[2]
layout = [("nes", ".nes", 950), ("snes", ".sfc", 750), ("snes", ".smc", 220), ("c64", ".d64", 300),
          ("gba", ".gba", 50), ("megadrive", ".zip", 400), ("n64", ".z64", 100), ("psx", ".7z", 230)]
items = []
for system, ext, n in layout:
    d = os.path.join(root, system)
    os.makedirs(d, exist_ok=True)
    for i in range(n):
        p = os.path.join(d, "%s Game %04d (Europe)%s" % (system.upper(), i, ext))
        open(p, "w").close()
        items.append({"path": p, "label": os.path.basename(p).rsplit(".", 1)[0],
                      "core_path": "DETECT", "core_name": "DETECT"})
json.dump(items, open(playlist, "w"))
print(len(items), "fake ROM files", file=sys.stderr)
EOF
Xvfb "$D" -screen 0 1280x900x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
export DISPLAY=$D
sleep 1
ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }
run() {
    NIRUGAME_TIMING=1 "$BIN" --open retroarch 2>"$SB/err" &
    A=$!
    sleep 2.5
    W=$(xdotool search --pid "$A" | head -1)
    xdotool windowfocus --sync "$W"
    ms=$(sed -n 's/.*first frame after \([0-9.]*\) ms.*/\1/p' "$SB/err" | head -1)
    lib=$(sed -n 's/.*library shown after \([0-9.]*\) ms.*/\1/p' "$SB/err" | head -1)
    a=$(ticks "$A")
    xdotool key --delay 15 $(printf 'Down %.0s' $(seq 50)) $(printf 'Next %.0s' $(seq 10))
    sleep 0.5
    b=$(ticks "$A")
    rss=$(awk '/VmRSS/ {print $2}' "/proc/$A/status")
    i0=$(ticks "$A"); sleep 2; i1=$(ticks "$A")
    echo "$1: first frame ${ms} ms, library ${lib:-?} ms, RSS ${rss} kB, 60 keys $(( (b - a) * 10 )) ms CPU, idle $((i1 - i0)) ticks/2 s"
    grep -E "^(nirugame: |T )" "$SB/err"
    kill "$A" 2>/dev/null || true
    wait "$A" 2>/dev/null || true
}
n=1
while [ "$n" -le "$RUNS" ]; do
    rm -rf "$XDG_CACHE_HOME"
    run "cold $n"
    run "warm $n"
    n=$((n + 1))
done
