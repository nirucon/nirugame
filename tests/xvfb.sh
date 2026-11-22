#!/bin/sh
# X11 end-to-end checks in a private Xvfb with a fake Steam library, a fake
# `steam` and `sidplayfp` on PATH: first frame, memory, idle CPU, keyboard
# selection and launch, live theme reload, type-to-search, the Music view (play, stop) and Esc.
#   tests/xvfb.sh PATH/TO/nirugame [DISPLAY_NUMBER]
set -eu
BIN=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
D=:${2:-$((70 + $$ % 20))}
SB=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$SB"; }
trap cleanup EXIT INT TERM
export HOME=$SB/home XDG_CONFIG_HOME=$SB/config XDG_DATA_HOME=$SB/data NIRU_UI_TRACE=1
mkdir -p "$HOME/.local/share/Steam/steamapps" "$XDG_CONFIG_HOME/niruwm" "$XDG_DATA_HOME/nirugame/music" "$SB/bin"
echo 'theme = "niru-forest-vigil"' > "$XDG_CONFIG_HOME/niruwm/config.toml"
for g in "100 Alpha Quest" "200 Beta Run"; do
    set -- $g
    printf '"AppState"\n{\n\t"appid"\t\t"%s"\n\t"name"\t\t"%s %s"\n\t"installdir"\t\t"%s"\n}\n' "$1" "$2" "$3" "$2" \
        > "$HOME/.local/share/Steam/steamapps/appmanifest_$1.acf"
done
printf '#!/bin/sh\necho "$@" >> %s/launched\n' "$SB" > "$SB/bin/steam"
printf '#!/bin/sh\necho "$@" >> %s/sid.log\nexec sleep 300\n' "$SB" > "$SB/bin/sidplayfp"
chmod +x "$SB/bin/steam" "$SB/bin/sidplayfp"
export PATH="$SB/bin:$PATH"
# A PSID header with the title "Commando".
python3 -c 'import sys; b=bytearray(0x7c); b[0:4]=b"PSID"; b[0x16:0x1e]=b"Commando"; open(sys.argv[1],"wb").write(b)' \
    "$XDG_DATA_HOME/nirugame/music/commando.sid"
Xvfb "$D" -screen 0 1280x900x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
export DISPLAY=$D
sleep 1
fail() { echo "FAIL: $*" >&2; tail -5 "$SB/err" >&2; exit 1; }
at() {
    awk -v n="$1" -F'|' '/^ui-frame/ {p = ""} $1 == "ui" && $2 == n {p = $3 " " $4} END {print p}' "$SB/err"
}
click() {
    n=0
    while [ -z "$(at "$1")" ]; do n=$((n + 1)); [ $n -lt 40 ] || fail "no control '$1'"; sleep 0.1; done
    set -- $(at "$1"); xdotool mousemove "$1" "$2" click 1; sleep 0.5
}
k() { xdotool key --delay 60 "$@"; sleep 0.5; }

NIRUGAME_TIMING=1 "$BIN" --open steam 2>"$SB/err" &
A=$!
PIDS="$PIDS $A"
sleep 1.5
W=$(xdotool search --pid "$A" | head -1)
xdotool windowmove "$W" 0 0 windowsize "$W" 1000 700; sleep 0.8
grep -q 'first frame after' "$SB/err" || fail "no first frame"
ms=$(sed -n 's/.*first frame after \([0-9]*\)\..*/\1/p' "$SB/err" | head -1)
[ "$ms" -lt 1000 ] || fail "first frame took $ms ms"
rss=$(awk '/VmRSS/ {print $2}' /proc/$A/status)
[ "$rss" -lt 60000 ] || fail "memory $rss kB"
a=$(awk '{print $14+$15}' /proc/$A/stat); sleep 3; b=$(awk '{print $14+$15}' /proc/$A/stat)
[ $((b - a)) -le 1 ] || fail "busy while idle: $((b - a)) ticks in 3 s"
echo "first frame ${ms} ms, ${rss} kB, idle $((b - a)) ticks/3 s"

# Live theme: the background follows a change of the NIRUWM theme.
px() { import -window root -crop 1x1+4+4 -depth 8 txt:- | sed -n 's/.*#\([0-9A-Fa-f]\{6\}\).*/\1/p' | tr A-F a-f; }
[ "$(px)" = 0b100c ] || fail "theme colour $(px), expected 0b100c"
echo 'theme = "niru-nord"' > "$XDG_CONFIG_HOME/niruwm/config.toml"
sleep 0.8
[ "$(px)" = 2e3440 ] || fail "theme did not reload: $(px)"
echo "live theme ok"

# Down selects the second game; Enter launches it through steam.
k Down Return
grep -q -- '-applaunch 200' "$SB/launched" 2>/dev/null || fail "Enter did not launch Beta Run"
# Typing searches; Enter launches the only match.
xdotool type --delay 40 'alp'; sleep 0.4
k Return
grep -q -- '-applaunch 100' "$SB/launched" || fail "search + Enter did not launch Alpha Quest"
# The Launch button of a row.
click 'Launch'
[ "$(grep -c -- '-applaunch 100' "$SB/launched")" = 2 ] || fail "Launch button"
echo "launch ok"

# Music: Tab through the views, play the SID track, stop it.
k ctrl+u Tab Tab Tab
click 'Play'
sleep 0.5
grep -q 'commando.sid' "$SB/sid.log" 2>/dev/null || fail "sidplayfp not started"
grep -q 'commando.sid' "$XDG_CONFIG_HOME/nirugame/config.toml" || fail "track not saved"
pgrep -f "sleep 300" -P "$A" >/dev/null || fail "no player process"
click 'Stop music'
pgrep -P "$A" >/dev/null && fail "player still running"
grep -q 'music = ""' "$XDG_CONFIG_HOME/nirugame/config.toml" || fail "Off not saved"
echo "music ok"

k Escape
sleep 0.5
kill -0 "$A" 2>/dev/null && fail "Esc did not close"
echo PASS
