# Changelog

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
