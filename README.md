# NIRUGAME

NIRUGAME is an optional retro-themed game launcher for NIRUX. It finds games
you already own and installed; it does not download, bundle or redistribute
games, ROMs, artwork or SID music.

- Steam games from `libraryfolders.vdf` and `appmanifest_*.acf`
- RetroArch entries from `playlists/*.lpl`
- Native games from `.desktop` entries with `Categories=Game`
- Optional SID background music from your own files, with multiple tracks and **Off**
- Live NIRUX theme colours, scrolling lists and keyboard launch

## Install

NIRUGAME is shipped as a **prebuilt x86_64 Linux binary**. The installer
downloads and checksum-verifies the release; it does not compile anything on
the target machine.

From a release checkout or source archive:

```sh
./install.sh
```

Install a specific release or prefix:

```sh
NIRUGAME_VERSION=0.1.3 ./install.sh
NIRUGAME_PREFIX=/usr/local ./install.sh   # run with appropriate privileges
```

Remove program files; music and configuration are kept:

```sh
./uninstall.sh
```

As a NIRUX extra, NIRUX places the binary in its managed prefix. Do not use
this standalone installer over a NIRUX-managed symlink; run `nirux update`.

## Use

```sh
nirugame
nirugame --list
nirugame --music-status
nirugame --setup-plan steam
nirugame --setup-plan retroarch
nirugame --setup-plan all
```

The window lists discovered games. Use the tabs to narrow the list, type in
the search field, use Up/Down and Enter to launch the selected game, or press
the Launch button. Missing Steam, RetroArch or native libraries simply produce
an empty view rather than an error.

`--setup-plan` only prints a reviewable package plan for Debian, Arch or Void.
NIRUGAME never adds repositories, installs packages or changes GPU drivers.

## SID music

Put your own `.sid`, `.psid` or `.rsid` files in:

```text
${XDG_DATA_HOME:-~/.local/share}/nirugame/music
```

Select **Off** or a track in the Music tab. Playback uses the external
`sidplayfp` program; install it from your distribution if you want sound.
NIRUGAME reaps the player process on stop, track change and exit. The selected
track path is stored in `${XDG_CONFIG_HOME:-~/.config}/nirugame/config.toml`.

No SID files are included in this repository or its releases.

## Safe launching

Game launch data is kept as an argument vector and run without a shell. NIRUGAME
does not scan arbitrary Windows executables or Wine prefixes. It starts games
through Steam (`steam -applaunch APPID`), RetroArch (`retroarch -L CORE
CONTENT`) or `gio launch` for native desktop entries.

Every external program is placed in its own session. This matters under NIRUWM,
where session programs share a process group: a helper that tries to read or
change the controlling terminal can receive SIGTTIN/SIGTTOU, which would stop
the whole group including the window manager. The SID player is also started
quietly with the lower-cost reSID engine and is terminated with NIRUGAME.

## Build and test (development only)

Release users never need Rust. Maintainers can run:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Licensed under MIT. NIRUGAME is not affiliated with Valve, Steam, RetroArch,
SIDPLAY or any game publisher.
