# Changelog

## 0.3.0 (2026-11-25)

- ROMs are found in your folders, not only in RetroArch playlists:
  `~/Games/roms`, `~/Games/ROMs`, `~/ROMs`, `~/roms`, system folders in
  `~/Games`, RetroArch's file browser folder, the playlist folders and
  `[roms] dirs`. Recursive (6 levels), case-insensitive, BIOS and save
  folders skipped. Playlist entries and scanned files are merged.
- System detection by extension and folder name (archives and disc images
  take the folder's system; a loose .zip is read for the files inside), for
  16 systems from NES to PlayStation, C64 and Amiga.
- Each system is matched to an installed libretro core (user, system and
  Flatpak folders), with `[cores]` preferences; `retroarch -L CORE ROM` or the
  Flatpak. Games without a core are greyed out with what to install.
  `[emulators] c64 = "x64sc"` starts C64 games in VICE; Shift+Enter uses
  RetroArch. `nirugame --cores` prints the mapping.
- Tabs for Favourites, Recent and every system with its count; search counts
  per tab. RetroArch box art (Named_Boxarts, RetroArch's file name rules),
  loaded lazily on a thread and cached; pixel-style system badges otherwise.
- New look: pixel headings and badges, list or grid (`G`), a details pane
  (system, file, size, core, play count, last played), favourites (`F`),
  `/` to search, Home/End, double-click to launch.
- The ROM index is cached in `~/.cache/nirugame/roms.json` and checked by
  folder times, so a start shows the library at once; only visible rows are
  built, so 3000 games scroll at under 3 ms per key press (16 ms in 0.2.0).
- Music: **Open in NIRUSID** (`N`) hands a track to NIRUSID, the suite's full
  SID player, when `nirusid` is installed; otherwise as before. Up/Down and
  Enter choose and play a track.
- Saving the music track keeps the rest of `config.toml`.
- Typing no longer starts a search by itself: letters are commands now
  (`F`, `G`); use `/` or Ctrl+F.
- `tests/xvfb.sh` covers ROMs, cores, VICE, box art, favourites and NIRUSID
  with stand-in programs; `tests/bench.sh` measures 3000 fake ROMs.

## 0.2.0 (2026-11-22)

- The window is drawn with x11rb and a small software canvas (the NIRUFM
  toolkit) instead of egui and OpenGL: first frame in about 20 ms instead of
  about 80 ms, 8 MB of memory instead of about 115 MB, no CPU while idle.
- The release binary is static (musl); no OpenGL or GUI libraries needed.
- Same views, search, launch, SID music and keys. New: Shift+Tab, Page
  Up/Down, Ctrl+F, F5 rescan, type anywhere to search, click a row to
  select it. The libraries are read on a thread while the window opens.
- The theme follows `_NIRUWM_THEME` on the root window and reloads through
  inotify instead of polling the configuration every 2 seconds.
- `tests/xvfb.sh`: end-to-end test with a fake Steam library, `steam` and
  `sidplayfp`.

## 0.1.4

- Do not list the launcher itself as a game.
