#!/bin/sh
# NIRUGAME standalone installer. It downloads a prebuilt binary; it never
# compiles on the target machine.
#
#   ./install.sh                 install the latest release for this user
#   NIRUGAME_VERSION=0.1.0 ./install.sh
#   ./install.sh --uninstall     remove program files, keep music and config
set -eu

VERSION="0.1.0"
REPO="${NIRUGAME_REPO:-nirucon/nirugame}"
PREFIX="${NIRUGAME_PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
APPDIR="$PREFIX/share/applications"
ICONDIR="$PREFIX/share/icons/hicolor/scalable/apps"

say() { printf '%s\n' "$*"; }
die() { say "nirugame install: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1; }
sha256() {
  if need sha256sum; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1
  fi
}
fetch() {
  if need curl; then curl -fsSL --retry 3 -o "$2" "$1"
  elif need wget; then wget -qO "$2" "$1"
  else die "curl or wget is required"
  fi
}

managed_guard() {
  [ -L "$BINDIR/nirugame" ] || return 0
  case "$(readlink "$BINDIR/nirugame")" in
    */share/nirux/current/bin/nirugame) die 'NIRUGAME is managed by NIRUX; use nirux update.' ;;
  esac
}

install_assets() {
  src_assets="$(dirname "$0")/assets"
  install -d "$APPDIR" "$ICONDIR"
  if [ -f "$src_assets/nirugame.desktop" ] && [ -f "$src_assets/nirugame.svg" ]; then
    install -m 644 "$src_assets/nirugame.desktop" "$APPDIR/nirugame.desktop"
    install -m 644 "$src_assets/nirugame.svg" "$ICONDIR/nirugame.svg"
  else
    tmp_assets=$(mktemp -d)
    fetch "$NIRUGAME_BASE_URL/nirugame.desktop" "$tmp_assets/nirugame.desktop" ||
      die "cannot download the desktop entry"
    fetch "$NIRUGAME_BASE_URL/nirugame.svg" "$tmp_assets/nirugame.svg" ||
      die "cannot download the icon"
    install -m 644 "$tmp_assets/nirugame.desktop" "$APPDIR/nirugame.desktop"
    install -m 644 "$tmp_assets/nirugame.svg" "$ICONDIR/nirugame.svg"
    rm -rf "$tmp_assets"
  fi
}

managed_guard
case "${1:-}" in
  --uninstall)
    rm -f "$BINDIR/nirugame" "$APPDIR/nirugame.desktop" "$ICONDIR/nirugame.svg"
    if need update-desktop-database; then update-desktop-database "$APPDIR" >/dev/null 2>&1 || true; fi
    say "Removed NIRUGAME program files. Music and configuration were kept."
    ;;
  "")
    [ "$(uname -m)" = "x86_64" ] || die "only prebuilt x86_64 Linux binaries are published"
    tag="${NIRUGAME_VERSION:-$VERSION}"
    base="${NIRUGAME_BASE_URL:-https://github.com/$REPO/releases/download/v$tag}"
    NIRUGAME_BASE_URL="$base"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "Downloading NIRUGAME $tag (prebuilt x86_64 binary) ..."
    fetch "$base/nirugame" "$tmp/nirugame" || die "cannot download the binary"
    fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "cannot download checksums"
    expected=$(awk '$2 == "nirugame" {print $1}' "$tmp/SHA256SUMS")
    [ "${#expected}" = 64 ] || die "no checksum for nirugame"
    [ "$expected" = "$(sha256 "$tmp/nirugame")" ] || die "checksum mismatch"
    chmod 755 "$tmp/nirugame"
    got=$("$tmp/nirugame" --version 2>/dev/null | awk '{print $2}') ||
      die "binary cannot run; install missing X11/OpenGL runtime libraries"
    [ "$got" = "$tag" ] || die "expected NIRUGAME $tag, got $got"
    install -d "$BINDIR"
    install -m 755 "$tmp/nirugame" "$BINDIR/nirugame"
    install_assets
    if need update-desktop-database; then update-desktop-database "$APPDIR" >/dev/null 2>&1 || true; fi
    say "Installed NIRUGAME $got in $BINDIR"
    say "Add your own SID files to ${XDG_DATA_HOME:-$HOME/.local/share}/nirugame/music"
    ;;
  *)
    die "unknown option: $1"
    ;;
esac
