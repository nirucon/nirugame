# NIRUGAME

Retro-themed launcher for the Steam games, ROMs and native games you already have installed.

**Status: alpha** · Part of the [NIRUX](https://github.com/nirucon/nirux) suite · [MIT License](LICENSE)

## About

NIRUGAME is an optional NIRUX extra: one keyboard-friendly window that lists the
games already on the computer and starts them. It reads local Steam manifests,
RetroArch playlists, your ROM folders and desktop entries, and starts each ROM
with an installed libretro core. It does not download, bundle or redistribute
games, ROMs, cores, artwork or SID music. If you like, it plays your own SID
tunes in the background while you pick a game; NIRUSID, the suite's full SID
player, takes over a track when it is installed.

## Features

- **Steam** games from `steamapps/libraryfolders.vdf` and `appmanifest_*.acf`
  (native, Debian and Flatpak Steam locations).
- **ROMs from your folders**, not only playlist entries: `~/Games/roms`,
  `~/Games/ROMs`, `~/ROMs`, `~/roms`, system folders directly in `~/Games`
  (`PS1`, `snes`, ...), RetroArch's file browser folder, the folders of your
  RetroArch playlists and any folder you add. The scan is recursive (6 levels
  by default), ignores case and skips BIOS, save and media folders.
- **System detection** by extension and by folder name, so `.zip`, `.7z` and
  disc images get their system from the folder they are in (a `.zip` outside a
  system folder is recognised by the files inside it). NES, SNES, N64, Game
  Boy, Game Boy Color, GBA, DS, Mega Drive, Master System, Game Gear,
  PlayStation, C64, Amiga, Atari 2600, PC Engine and arcade.
- **RetroArch playlists** (`*.lpl`, native and Flatpak) are merged with the
  scan: each file is listed once, with the playlist's name and core.
- **A core per system**: the installed libretro cores (RetroArch's cores
  folder, the system `libretro` folders, the Flatpak) are matched to each
  system, with your preferred core first. A game is started as
  `retroarch -L CORE ROM` (or with the Flatpak).
- **No core, no error**: such games are greyed out with the core to install.
- **C64 with VICE**: `x64sc` (or another stand-alone emulator) can start a
  system's games directly; Shift+Enter still uses RetroArch.
- **Tabs** All, Favourites, Recent, Steam, RetroArch, Native, one per system
  with its count, and Music. Search filters the current tab, and while you
  search every tab shows how many games match there.
- **Box art** from RetroArch's thumbnails, loaded in the background only for
  the games on screen; a pixel-style system badge otherwise.
- **Details pane** with system, file, folder, size, core, play count and the
  last time played; **favourites** and **recently played**; list or grid.
- **Native** games from `.desktop` entries with `Categories=Game`, including
  Flatpak exports.
- Optional **SID background music** from your own `.sid`, `.psid` or `.rsid`
  files, with several tracks and **Off**. With NIRUSID installed, **Open in
  NIRUSID** hands a track to the full player.
- Live NIRUX theme colours, including custom user themes.
- A reviewable package plan for Steam and RetroArch on Debian, Arch and Void,
  printed only; NIRUGAME never installs anything itself.
- Missing Steam, RetroArch, cores or native libraries give an empty or greyed
  view, not an error.

## Requirements

- Linux x86_64 with an X11 session (Debian, Arch or Void). The prebuilt binary
  is static (musl) and draws its own window: no OpenGL or toolkit libraries.
- Optional: Steam, RetroArch with the libretro cores you need, VICE for C64,
  `gio` (or `gtk-launch`) for native games, `sidplayfp` for SID background
  music and [NIRUSID](https://github.com/nirucon/nirusid) as the full SID
  player.

## Installation

### With NIRUX

```sh
nirux extras install nirugame
nirux extras update            # later, for the extras you selected
```

NIRUX downloads the pinned NIRUGAME release tag, verifies it and runs the
project's own `install.sh`. In NIRUX, `nirux manual nirugame` opens the short
tour in the NIRUX manual.

### Standalone

The installer downloads the prebuilt x86_64 binary for its pinned release and
checks it against the release `SHA256SUMS`; nothing is compiled on your machine.

```sh
git clone https://github.com/nirucon/nirugame
cd nirugame
./install.sh                                  # into ~/.local
NIRUGAME_VERSION=0.3.0 ./install.sh           # a specific release
NIRUGAME_PREFIX=/usr/local ./install.sh       # another prefix (needs the rights to write there)
```

## Usage

```sh
nirugame                                    # open the window
nirugame --open snes                        # start on a tab: all, favourites, recent, steam,
                                            # retroarch, native, music or a system (nes, c64, ...)
nirugame --list                             # discovered games as JSON (--rescan ignores the cache)
nirugame --cores                            # the core or emulator per system, or what to install
nirugame --music-status                     # music folder, tracks, selection and NIRUSID as JSON
nirugame --setup-plan retroarch             # print a package plan: steam, retroarch or all
nirugame --version
```

| Key | Action |
|---|---|
| `Up` / `Down` | Select a game (a row of the grid) |
| `Left` / `Right` | Select in the grid |
| `Page Up` / `Page Down`, `Home` / `End` | Jump a page, to the first or the last game |
| `Enter` | Launch the selected game |
| `Shift+Enter` | Launch with the other launcher (RetroArch when an emulator is set) |
| `/` or `Ctrl+F` | Search; `Esc` leaves the search field, a second `Esc` clears it |
| `Tab` / `Shift+Tab` | Next or previous tab, systems included |
| `F` | Favourite on or off |
| `G` | Grid or list |
| `F5` | Rescan the libraries and ROM folders (in Music: the SID folder) |
| `Esc` | Close |

In the Music tab: `Up` / `Down` choose a track, `Enter` plays it as
background music, `N` opens it in NIRUSID and `S` stops the music.

The mouse works too: click a tab, a system badge or a game, double-click a
game to launch it, and use the buttons of the details pane.

Games start without a shell: Steam with `steam -applaunch APPID` (or the
Flatpak), ROMs with `retroarch -L CORE ROM` (or `flatpak run
org.libretro.RetroArch -L CORE ROM`), a configured emulator with its own
command (`x64sc -autostart ROM`), native games with `gio launch`. Every
external program runs in its own session, so a helper that touches the
terminal can never stop the NIRUWM process group. NIRUGAME does not scan
Windows executables or Wine prefixes.

## Configuration

Everything is optional; without a file NIRUGAME scans the default folders and
picks the first installed core for each system. See
[`config.example.toml`](config.example.toml):

```toml
[roms]
dirs = ["~/Emulation/roms"]    # extra folders; the defaults are scanned too
defaults = true                # false: only the folders in dirs
depth = 6                      # folder levels below each ROM folder

[cores]
snes = "snes9x"                # core name or path to a *_libretro.so
nes = "mesen"

[emulators]
c64 = "x64sc"                  # VICE directly; Shift+Enter still uses RetroArch
```

System ids: `nes`, `snes`, `n64`, `gb`, `gbc`, `gba`, `nds`, `megadrive`,
`mastersystem`, `gamegear`, `psx`, `c64`, `amiga`, `atari2600`, `pcengine`,
`arcade`. Core names are the file names without `_libretro.so`, for example
`bsnes_mercury_accuracy`, `gambatte`, `vice_x64sc` or `swanstation`.
`nirugame --cores` shows what each system uses.

Box art is read from RetroArch's thumbnail folder
(`thumbnails/<playlist>/Named_Boxarts/<name>.png`, with RetroArch's rule that
`&*/:` `` ` `` `<>?\|"` become `_`). Download the thumbnails in RetroArch (Online
Updater, playlist thumbnails); NIRUGAME looks in the game's playlist folder
and in the folder of its system, by label and by file name.

Put your own SID files in the music folder and choose **Off** or a track in the
**Music** tab. Playback uses `sidplayfp` with the lighter reSID engine; the
player is stopped on track change, **Off** and exit. NIRUSID is the full SID
player of the suite: when `nirusid` is on `PATH`, **Open in NIRUSID** (or `N`)
starts `nirusid FILE` and pauses the background music. No SID files are
included in this repository or its releases.

The selected track is saved automatically in the `music` line of the
configuration; the rest of the file is left as you wrote it. Favourites and
the play history are saved as you go. Colours come from the active NIRUX theme
in `~/.config/niruwm/`.

## Files and paths

| Path | Purpose |
|---|---|
| `${XDG_CONFIG_HOME:-~/.config}/nirugame/config.toml` | ROM folders, cores, emulators, music track |
| `${XDG_DATA_HOME:-~/.local/share}/nirugame/music/` | Your SID files |
| `${XDG_DATA_HOME:-~/.local/share}/nirugame/library.json` | Favourites and play history |
| `${XDG_CACHE_HOME:-~/.cache}/nirugame/roms.json` | ROM index (safe to delete) |
| `~/.config/retroarch/` (or the Flatpak's) | Read only: `retroarch.cfg`, playlists, cores, thumbnails |
| `~/.local/bin/nirugame` | Program (standalone and NIRUX extra) |
| `~/.local/share/applications/nirugame.desktop` | Launcher entry |

## Troubleshooting

- **A tab is empty:** that library was not found. Check `nirugame --list`; it
  also lists the scanned ROM folders and discovery errors.
- **ROMs are missing:** add their folder under `[roms] dirs`. A `.7z` or disc
  image needs a system folder name (`psx`, `PS1`, `megadrive`, ...) in its
  path. Press `F5` after adding files elsewhere than a scanned folder.
- **A game is greyed out:** its hint says what is missing: a core for the
  system (`nirugame --cores`), RetroArch itself, or the ROM file.
- **No music:** install `sidplayfp` from your distribution and check
  `nirugame --music-status`.
- **The binary does not start after install:** the installer stops with that
  message. The binary is static; check that it is the x86_64 release and that
  `DISPLAY` points at an X11 session.

## Uninstall

With NIRUX: `nirux extras uninstall nirugame` runs the uninstaller below from the pinned release.

```sh
./uninstall.sh            # or: ./install.sh --uninstall
```

Program files, the desktop entry and the icon are removed; music,
configuration, favourites and the ROM index cache are kept. Set `NIRUGAME_PREFIX` if you installed elsewhere.

## Building from source

Release users never need Rust. For development:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
tests/xvfb.sh target/release/nirugame          # end-to-end GUI test in Xvfb
tests/bench.sh target/release/nirugame         # timing and memory with 3000 fake ROMs
```

### Performance

Since 0.2.0 the window is drawn by a small software canvas on x11rb (the
NIRUFM toolkit) instead of egui and OpenGL. The libraries are read on a
thread while the window opens; while idle the program sleeps in `poll()`.

0.3.0 lists thousands of ROMs, so the list only builds the rows on screen,
search and tabs work on a prepared index, and the ROM index is cached:
`~/.cache/nirugame/roms.json` records the modification time of every folder it
read, and a start only checks those times (a new, removed or renamed file
changes its folder's time). Box art is decoded and scaled on its own thread,
only for visible games, and at most 8 MB of it stays in memory.

Measured with `tests/bench.sh` on yggdrasil, static (musl) release builds in
the same Xvfb: 3000 empty fake ROM files in 8 system folders plus a playlist
that lists them (0.2.0 only reads the playlist), two cold and two warm starts
per run, two runs each:

| | 0.2.0 | 0.3.0 |
|---|---|---|
| First frame | 11 to 35 ms | 13 to 42 ms |
| Library on screen | not reported | 25 to 38 ms (scan), 16 to 46 ms (cache) |
| Reading folders and playlist (thread) | | 22 to 34 ms, from the cache 11 to 23 ms |
| Memory (RSS) with 3000 games | 9.8 to 9.9 MB | 11.3 to 11.5 MB |
| CPU for 60 key presses through the list | 940 to 980 ms | 160 to 180 ms |
| CPU while idle | 0 | 0 |
| Binary | 1.4 MB, static | 1.9 MB, static (adds PNG decoding) |

The first frame varies with the machine's load in both versions; it does not
wait for the libraries, and the library appears right after it. In 0.2.0 every key press laid out all 3000 rows (about
16 ms); 0.3.0 needs under 3 ms, about what it costs with 30 games.

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
