#!/bin/sh
# X11 end-to-end checks in a private Xvfb. HOME and every XDG directory are
# temporary; Steam, RetroArch, VICE, sidplayfp and NIRUSID are stand-in
# scripts that log their arguments, and the ROMs are empty fake files.
# Checked: first frame, memory, idle CPU, live theme reload, search, launch
# (Steam, RetroArch cores, a configured x64sc, the alternative launcher), a
# system without a core, the systems tabs, box art, favourites and history,
# the grid, the Music view (play, Open in NIRUSID, stop), the index cache
# and Esc.
#   tests/xvfb.sh PATH/TO/nirugame [DISPLAY_NUMBER]
set -eu
BIN=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
N=${2:-$((70 + $$ % 20))}
while [ -e "/tmp/.X$N-lock" ]; do N=$((N + 1)); done
D=:$N
SB=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$SB"; }
trap cleanup EXIT INT TERM
export HOME=$SB/home XDG_CONFIG_HOME=$SB/config XDG_DATA_HOME=$SB/data XDG_CACHE_HOME=$SB/cache \
    XDG_STATE_HOME=$SB/state XDG_RUNTIME_DIR=$SB/run NIRU_UI_TRACE=1 NIRUGAME_CORE_DIRS=""
unset XDG_DATA_DIRS XDG_CONFIG_DIRS
mkdir -p "$HOME/.local/share/Steam/steamapps" "$XDG_CONFIG_HOME/niruwm" "$XDG_DATA_HOME/nirugame/music" \
    "$SB/bin" "$SB/sid" "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME/retroarch/cores" "$XDG_CONFIG_HOME/retroarch/playlists" \
    "$XDG_CONFIG_HOME/nirugame"
chmod 700 "$XDG_RUNTIME_DIR"
echo 'theme = "niru-forest-vigil"' > "$XDG_CONFIG_HOME/niruwm/config.toml"
for g in "100 Alpha Quest" "200 Beta Run"; do
    set -- $g
    printf '"AppState"\n{\n\t"appid"\t\t"%s"\n\t"name"\t\t"%s %s"\n\t"installdir"\t\t"%s"\n}\n' "$1" "$2" "$3" "$2" \
        > "$HOME/.local/share/Steam/steamapps/appmanifest_$1.acf"
done
# Stand-ins that log how they were started.
for p in steam retroarch x64sc; do
    printf '#!/bin/sh\necho "$@" >> %s/%s.log\n' "$SB" "$p" > "$SB/bin/$p"
done
printf '#!/bin/sh\necho "$@" >> %s/sid.log\nexec sleep 300\n' "$SB" > "$SB/bin/sidplayfp"
printf '#!/bin/sh\necho "$@" >> %s/nirusid.log\n' "$SB" > "$SB/sid/nirusid"
chmod +x "$SB/bin/"* "$SB/sid/nirusid"
export PATH="$SB/bin:$SB/sid:$PATH"
# Fake ROMs (empty files), cores and a playlist entry that the scan also finds.
R=$HOME/Games/ROMs
mkdir -p "$R/nes" "$R/snes" "$R/c64" "$R/n64" "$R/megadrive" "$R/bios"
: > "$R/nes/Alter Ego (PD).nes"
: > "$R/snes/Zelda (USA).sfc"
: > "$R/c64/Elite.d64"
: > "$R/n64/Kart.z64"
: > "$R/megadrive/Sonic.zip"
: > "$R/bios/scph1001.bin"
for c in mesen snes9x vice_x64sc; do : > "$XDG_CONFIG_HOME/retroarch/cores/${c}_libretro.so"; done
printf '{"version":"1.5","items":[{"path":"%s","label":"Alter Ego","core_path":"DETECT","core_name":"DETECT"}]}\n' \
    "$R/nes/Alter Ego (PD).nes" > "$XDG_CONFIG_HOME/retroarch/playlists/Nintendo - Nintendo Entertainment System.lpl"
printf 'music = ""\n\n[emulators]\nc64 = "x64sc"\n' > "$XDG_CONFIG_HOME/nirugame/config.toml"
# Magenta box art for Zelda, in RetroArch's thumbnail layout.
ART="$XDG_CONFIG_HOME/retroarch/thumbnails/Nintendo - Super Nintendo Entertainment System/Named_Boxarts"
mkdir -p "$ART"
python3 -c '
import struct, sys, zlib
w, h = 200, 150
raw = b"".join(b"\0" + b"\xff\x00\xff" * w for _ in range(h))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))' "$ART/Zelda (USA).png"
# A PSID header with the title "Commando".
python3 -c 'import sys; b=bytearray(0x7c); b[0:4]=b"PSID"; b[0x16:0x1e]=b"Commando"; open(sys.argv[1],"wb").write(b)' \
    "$XDG_DATA_HOME/nirugame/music/commando.sid"

fail() { echo "FAIL: $*" >&2; grep -v "^ui" "$SB/err" | tail -8 >&2; exit 1; }

# The command line sees the same library: duplicates merged, no core is a hint.
"$BIN" --list > "$SB/list.json"
python3 - "$SB/list.json" <<'EOF' || fail "--list"
import json, sys
d = json.load(open(sys.argv[1]))
ra = {g["name"]: g for g in d["retroarch"]}
assert sorted(ra) == ["Alter Ego", "Elite", "Kart", "Sonic", "Zelda (USA)"], sorted(ra)
assert ra["Kart"]["launch"] == [] and "N64" in ra["Kart"]["rom"]["hint"]
assert ra["Elite"]["launch"][1:] == ["-autostart", ra["Elite"]["rom"]["file"]]
assert ra["Zelda (USA)"]["rom"]["system"] == "snes" and ra["Zelda (USA)"]["rom"]["player"] == "snes9x"
assert len(d["steam"]) == 2
EOF
"$BIN" --cores | grep -q '"player": "snes9x' || fail "--cores"
[ -s "$XDG_CACHE_HOME/nirugame/roms.json" ] || fail "no index cache"
echo "cli ok"

Xvfb "$D" -screen 0 1280x900x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
export DISPLAY=$D
sleep 1
at() {
    awk -v n="$1" -F'|' '/^ui-frame/ {p = ""} $1 == "ui" && $2 == n {p = $3 " " $4} END {print p}' "$SB/err"
}
click() {
    n=0
    while [ -z "$(at "$1")" ]; do n=$((n + 1)); [ $n -lt 40 ] || fail "no control '$1'"; sleep 0.1; done
    set -- $(at "$1"); xdotool mousemove "$1" "$2" click 1; sleep 0.5
}
k() { xdotool key --delay 60 "$@"; sleep 0.5; }
# A pixel of the screen, or whether a colour is anywhere on it.
shot() {
    if command -v import >/dev/null 2>&1; then
        import -window root -depth 8 "$SB/shot.ppm"
    else
        python3 -c 'import sys; from PIL import ImageGrab; ImageGrab.grab(xdisplay=sys.argv[1]).save(sys.argv[2])' "$D" "$SB/shot.ppm"
    fi
}
px() {
    shot
    python3 -c '
import sys
d = open(sys.argv[1], "rb").read()
parts = d.split(maxsplit=4)
w = int(parts[1]); x, y = int(sys.argv[2]), int(sys.argv[3]); off = len(d) - int(parts[1]) * int(parts[2]) * 3
i = off + (y * w + x) * 3
print("%02x%02x%02x" % tuple(d[i:i + 3]))' "$SB/shot.ppm" "$1" "$2"
}
has_colour() {
    shot
    python3 -c '
import sys
d = open(sys.argv[1], "rb").read()
sys.exit(0 if bytes.fromhex(sys.argv[2]) * 4 in d else 1)' "$SB/shot.ppm" "$1"
}

NIRUGAME_TIMING=1 "$BIN" --open steam 2>"$SB/err" &
A=$!
PIDS="$PIDS $A"
sleep 1.5
W=$(xdotool search --pid "$A" | head -1)
xdotool windowmove "$W" 0 0 windowsize "$W" 1000 700; sleep 0.8
xdotool windowfocus --sync "$W"
grep -q 'first frame after' "$SB/err" || fail "no first frame"
ms=$(sed -n 's/.*first frame after \([0-9]*\)\..*/\1/p' "$SB/err" | head -1)
[ "$ms" -lt 1000 ] || fail "first frame took $ms ms"
rss=$(awk '/VmRSS/ {print $2}' /proc/$A/status)
[ "$rss" -lt 60000 ] || fail "memory $rss kB"
a=$(awk '{print $14+$15}' /proc/$A/stat); sleep 3; b=$(awk '{print $14+$15}' /proc/$A/stat)
[ $((b - a)) -le 1 ] || fail "busy while idle: $((b - a)) ticks in 3 s"
grep -q 'library shown after .*, cache)' "$SB/err" || fail "the index cache was not used"
echo "first frame ${ms} ms, ${rss} kB, idle $((b - a)) ticks/3 s"

# Live theme: the background follows a change of the NIRUWM theme.
[ "$(px 4 4)" = 0b100c ] || fail "theme colour $(px 4 4), expected 0b100c"
echo 'theme = "niru-nord"' > "$XDG_CONFIG_HOME/niruwm/config.toml"
sleep 0.8
[ "$(px 4 4)" = 2e3440 ] || fail "theme did not reload: $(px 4 4)"
echo "live theme ok"

# Down selects the second game; Enter launches it through steam.
k Down Return
grep -q -- '-applaunch 200' "$SB/steam.log" 2>/dev/null || fail "Enter did not launch Beta Run"
# / searches; Enter launches the only match; Esc leaves and clears the search.
k slash
xdotool type --delay 40 'alp'; sleep 0.4
k Return
grep -q -- '-applaunch 100' "$SB/steam.log" || fail "search + Enter did not launch Alpha Quest"
k Escape Escape
kill -0 "$A" 2>/dev/null || fail "Esc in the search closed the window"
# The Launch button of the details pane.
click 'Launch'
[ "$(grep -c -- '-applaunch 100' "$SB/steam.log")" = 2 ] || fail "Launch button"
echo "steam ok"

# RetroArch: cores by system, x64sc for C64, Shift+Enter for RetroArch, no core.
k Tab Return
grep -q -- "-L $XDG_CONFIG_HOME/retroarch/cores/mesen_libretro.so $R/nes/Alter Ego (PD).nes" "$SB/retroarch.log" ||
    fail "NES game did not start with mesen"
k Down Return
grep -q -- "-autostart $R/c64/Elite.d64" "$SB/x64sc.log" 2>/dev/null || fail "C64 game did not start with x64sc"
k shift+Return
grep -q -- "vice_x64sc_libretro.so $R/c64/Elite.d64" "$SB/retroarch.log" || fail "Shift+Enter did not use RetroArch"
k Down Return
grep -q Kart "$SB/retroarch.log" && fail "a game without a core was started"
echo "retroarch ok"

# Systems tab, box art, favourite, history.
click 'system-snes'
k Return
grep -q -- "snes9x_libretro.so $R/snes/Zelda (USA).sfc" "$SB/retroarch.log" || fail "SNES game did not start with snes9x"
sleep 0.5
has_colour ff00ff || fail "no box art"
k f
grep -q 'Zelda (USA).sfc' "$XDG_DATA_HOME/nirugame/library.json" || fail "favourite not saved"
python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))
assert any("Zelda" in k for k in d["favourites"]), d
assert d["played"]["rom:" + sys.argv[2]]["count"] == 1, d' "$XDG_DATA_HOME/nirugame/library.json" "$R/snes/Zelda (USA).sfc" ||
    fail "history"
click 'Favourites 1'
k g Return
[ "$(grep -c 'Zelda' "$SB/retroarch.log")" = 2 ] || fail "favourite in the grid did not launch"
echo "systems, box art, favourites ok"

# Music: play, hand the track to NIRUSID (background music pauses), stop.
click 'Music'
click 'Play'
sleep 0.5
grep -q 'commando.sid' "$SB/sid.log" 2>/dev/null || fail "sidplayfp not started"
grep -q 'commando.sid' "$XDG_CONFIG_HOME/nirugame/config.toml" || fail "track not saved"
grep -q 'c64 = "x64sc"' "$XDG_CONFIG_HOME/nirugame/config.toml" || fail "saving music lost the rest of the config"
pgrep -f "sleep 300" -P "$A" >/dev/null || fail "no player process"
click 'Open in NIRUSID'
sleep 0.5
grep -q 'commando.sid' "$SB/nirusid.log" 2>/dev/null || fail "NIRUSID not started with the track"
pgrep -f "sleep 300" -P "$A" >/dev/null && fail "background music still playing next to NIRUSID"
k Return
pgrep -f "sleep 300" -P "$A" >/dev/null || fail "Enter did not play the track"
click 'Stop music'
pgrep -f "sleep 300" -P "$A" >/dev/null && fail "player still running"
grep -q 'music = ""' "$XDG_CONFIG_HOME/nirugame/config.toml" || fail "Off not saved"
echo "music ok"

k Escape
sleep 0.5
kill -0 "$A" 2>/dev/null && fail "Esc did not close"

# Without nirusid the Music view stays as it was: no hand-off button.
PATH=$(printf '%s' "$PATH" | sed "s|$SB/sid:||")
"$BIN" --open music 2>"$SB/err" &
A=$!
PIDS="$PIDS $A"
sleep 1.5
[ -n "$(at Play)" ] || fail "no Play without nirusid"
[ -z "$(at 'Open in NIRUSID')" ] || fail "Open in NIRUSID shown without nirusid"
kill "$A"
echo PASS
