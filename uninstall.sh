#!/bin/sh
# Remove NIRUGAME program files. Music and configuration are always kept.
set -eu
PREFIX="${NIRUGAME_PREFIX:-$HOME/.local}"
rm -f "$PREFIX/bin/nirugame"
rm -f "$PREFIX/share/applications/nirugame.desktop"
rm -f "$PREFIX/share/icons/hicolor/scalable/apps/nirugame.svg"
printf '%s\n' "Removed NIRUGAME. Music in \${XDG_DATA_HOME:-\$HOME/.local/share}/nirugame is untouched."
