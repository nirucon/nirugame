# NIRUGAME

Retro-themed launcher for the Steam, RetroArch and native games you already have installed.

**Status: alpha** · Part of the [NIRUX](https://github.com/nirucon/nirux) suite · [MIT License](LICENSE)

## About

NIRUGAME is an optional NIRUX extra: one keyboard-friendly window that lists the
games already on the computer and starts them. It reads local Steam manifests,
RetroArch playlists and desktop entries; it does not download, bundle or
redistribute games, ROMs, artwork or SID music. If you like, it plays your own
SID tunes in the background while you pick a game.

## Features

- **Steam** games from `steamapps/libraryfolders.vdf` and `appmanifest_*.acf`
  (native, Debian and Flatpak Steam locations).
- **RetroArch** entries from `playlists/*.lpl` (native and Flatpak).
- **Native** games from `.desktop` entries with `Categories=Game`, including
  Flatpak exports.
- Tabs **All**, **Steam**, **RetroArch**, **Native** and **Music**, with search.
- Optional **SID background music** from your own `.sid`, `.psid` or `.rsid`
  files, with several tracks and **Off**.
- Live NIRUX theme colours, including custom user themes.
- A reviewable package plan for Steam and RetroArch on Debian, Arch and Void,
  printed only; NIRUGAME never installs anything itself.
- Missing Steam, RetroArch or native libraries give an empty view, not an error.

## Requirements

- Linux x86_64 with X11 and OpenGL (prebuilt binary; Debian, Arch or Void).
- Optional: Steam, RetroArch, `gio` (or `gtk-launch`) for native games, and
  `sidplayfp` for SID music.

## Installation

### With NIRUX

```sh
nirux extras install nirugame
nirux extras update            # later, for the extras you selected
```

NIRUX downloads the pinned NIRUGAME release tag, verifies it and runs the
project's own `install.sh`.

### Standalone

The installer downloads the prebuilt x86_64 binary for its pinned release and
checks it against the release `SHA256SUMS`; nothing is compiled on your machine.

```sh
git clone https://github.com/nirucon/nirugame
cd nirugame
./install.sh                                  # into ~/.local
NIRUGAME_VERSION=0.1.4 ./install.sh           # a specific release
NIRUGAME_PREFIX=/usr/local ./install.sh       # another prefix (needs the rights to write there)
```

## Usage

```sh
nirugame                                    # open the window
nirugame --open steam                       # start on a tab: all, steam, retroarch, native, music
nirugame --list                             # discovered games as JSON
nirugame --music-status                     # music folder, tracks and selection as JSON
nirugame --setup-plan steam                 # print a package plan: steam, retroarch or all
nirugame --version
```

| Key | Action |
|---|---|
| `Up` / `Down` | Select a game |
| `Enter` | Launch the selected game |
| `Tab` | Next tab (when the search field is not focused) |
| `Esc` | Close |

Typing in the search field filters the list; the **Launch** button does the same
as `Enter`.

Games start without a shell: Steam with `steam -applaunch APPID` (or the
Flatpak), RetroArch with `retroarch -L CORE CONTENT`, native games with
`gio launch`. Every external program runs in its own session, so a helper that
touches the terminal can never stop the NIRUWM process group. NIRUGAME does not
scan Windows executables or Wine prefixes.

## Configuration

Put your own SID files in the music folder and choose **Off** or a track in the
**Music** tab. Playback uses `sidplayfp` with the lighter reSID engine; the
player is stopped on track change, **Off** and exit. No SID files are included in
this repository or its releases.

The selected track is the only setting and is saved automatically. Colours come
from the active NIRUX theme in `~/.config/niruwm/`.

## Files and paths

| Path | Purpose |
|---|---|
| `${XDG_DATA_HOME:-~/.local/share}/nirugame/music/` | Your SID files |
| `${XDG_CONFIG_HOME:-~/.config}/nirugame/config.toml` | Selected music track |
| `~/.local/bin/nirugame` | Program (standalone and NIRUX extra) |
| `~/.local/share/applications/nirugame.desktop` | Launcher entry |

## Troubleshooting

- **A tab is empty:** that library was not found. Check `nirugame --list`; it
  also lists discovery errors.
- **No music:** install `sidplayfp` from your distribution and check
  `nirugame --music-status`.
- **The binary does not start after install:** the X11/OpenGL runtime libraries
  are missing; the installer stops with that message.

## Uninstall

```sh
./uninstall.sh            # or: ./install.sh --uninstall
```

Program files, the desktop entry and the icon are removed; music and
configuration are kept. Set `NIRUGAME_PREFIX` if you installed elsewhere.

## Building from source

Release users never need Rust. For development:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

## Status, disclaimer and how it's made

**Status: alpha.** Discovery, launching and SID music work on my machines; the feature set is small and the look will still change.

NIRUX and every niru* project are made by me, Nicklas Rudolfsson
([@nirucon](https://github.com/nirucon)), for me. They are tailored to my own
needs, preferences and workflows, on my own machines. If they fit yours too,
that's a nice bonus.

- **Free and open source.** Anyone is free to use, study, change and share
  it under the [MIT License](LICENSE).
- **No guarantees.** There is no warranty of any kind. Use it at your own
  risk, read what you install and keep backups of anything that matters.
- **How it's made.** Some of these projects I built from scratch by hand. In
  more recent releases I have used Claude, Anthropic's AI, to do much of the
  heavy lifting under my direction: I decide what to build and how it should
  behave, and I test it on my own machines. It saves me a lot of time, and
  the result is quite OK.
- **Alpha, beta, dev, and maybe forever.** Most things here are alpha, beta
  or dev, and may stay that way. Maybe one day there will be a real stable
  release, but given how fast development moves, and how I am as a person,
  maybe never :)

| Label | Meaning |
|---|---|
| dev | Under active construction; may not build, install or work yet. |
| alpha | Works on my machines, but incomplete or rough; expect breaking changes. |
| beta | In daily use on my machines and mostly complete; rough edges remain. |

## License

MIT License, see [LICENSE](LICENSE). Copyright (c) 2026 Nirucon.

NIRUGAME is not affiliated with Valve, Steam, RetroArch, SIDPLAY or any game
publisher.
