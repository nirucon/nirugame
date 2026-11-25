//! RetroArch on this computer: its configuration (`retroarch.cfg`), the
//! installed libretro cores, the playlists and how to start it (native or
//! Flatpak). Nothing here writes to RetroArch's files.

use crate::config::{self, Config};
use crate::systems::System;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

const FLATPAK_ID: &str = "org.libretro.RetroArch";

/// `key = "value"` pairs of a retroarch.cfg.
pub fn parse_cfg(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(v);
        map.insert(k.trim().to_string(), v.to_string());
    }
    map
}

/// The directories of a RetroArch installation.
#[derive(Clone, Debug, Default)]
pub struct Install {
    /// The config folder (`~/.config/retroarch`).
    pub base: PathBuf,
    pub cfg: HashMap<String, String>,
}

impl Install {
    pub fn at(base: PathBuf) -> Self {
        let cfg = fs::read_to_string(base.join("retroarch.cfg"))
            .map(|t| parse_cfg(&t))
            .unwrap_or_default();
        Self { base, cfg }
    }

    /// A directory setting: unset or "default" gives None; `~/` and the
    /// RetroArch `:/` (its own folder) are expanded.
    pub fn dir(&self, key: &str) -> Option<PathBuf> {
        let v = self.cfg.get(key)?.trim();
        if v.is_empty() || v == "default" {
            return None;
        }
        if let Some(rest) = v.strip_prefix(":/") {
            return Some(self.base.join(rest));
        }
        Some(config::expand(v))
    }

    pub fn playlist_dir(&self) -> PathBuf {
        self.dir("playlist_directory")
            .unwrap_or_else(|| self.base.join("playlists"))
    }

    pub fn thumbnail_dir(&self) -> PathBuf {
        self.dir("thumbnails_directory")
            .unwrap_or_else(|| self.base.join("thumbnails"))
    }

    pub fn core_dir(&self) -> PathBuf {
        self.dir("libretro_directory")
            .unwrap_or_else(|| self.base.join("cores"))
    }

    /// The "Start directory" of RetroArch's file browser: often the ROMs.
    pub fn rom_dir(&self) -> Option<PathBuf> {
        self.dir("rgui_browser_directory")
            .or_else(|| self.dir("content_directory"))
    }
}

/// Native config first, then the Flatpak one; only those that exist.
pub fn installs() -> Vec<Install> {
    let native = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| config::home().join(".config"))
        .join("retroarch");
    let flatpak = config::home().join(".var/app/org.libretro.RetroArch/config/retroarch");
    [native, flatpak]
        .into_iter()
        .filter(|p| p.is_dir())
        .map(Install::at)
        .collect()
}

pub fn command_in_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = config::expand(name);
        return p.is_file().then_some(p);
    }
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(name))
        .find(|candidate| candidate.is_file())
}

/// How to start RetroArch: the program on PATH, else the Flatpak.
pub fn prefix() -> Option<Vec<String>> {
    if let Some(path) = command_in_path("retroarch") {
        return Some(vec![path.to_string_lossy().to_string()]);
    }
    let flatpak_installed = [
        PathBuf::from("/var/lib/flatpak/app").join(FLATPAK_ID),
        config::home()
            .join(".local/share/flatpak/app")
            .join(FLATPAK_ID),
    ]
    .iter()
    .any(|p| p.is_dir());
    (flatpak_installed && command_in_path("flatpak").is_some())
        .then(|| vec!["flatpak".into(), "run".into(), FLATPAK_ID.into()])
}

/// Directories with libretro cores. `NIRUGAME_CORE_DIRS` (colon separated)
/// replaces the system directories; the RetroArch user folders always count.
pub fn core_dirs(installs: &[Install]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for i in installs {
        dirs.push(i.core_dir());
        dirs.push(i.base.join("cores"));
    }
    match env::var_os("NIRUGAME_CORE_DIRS") {
        Some(list) => dirs.extend(env::split_paths(&list).filter(|p| !p.as_os_str().is_empty())),
        None => dirs.extend(
            [
                "/usr/lib/x86_64-linux-gnu/libretro",
                "/usr/lib/libretro",
                "/usr/lib64/libretro",
                "/usr/local/lib/libretro",
                "/usr/lib/aarch64-linux-gnu/libretro",
            ]
            .map(PathBuf::from),
        ),
    }
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let ok = d.is_dir() && !seen.contains(d);
        seen.push(d.clone());
        ok
    });
    dirs
}

/// Installed cores by name ("snes9x" for `snes9x_libretro.so`); the first
/// directory that has a core wins.
#[derive(Clone, Debug, Default)]
pub struct Cores {
    pub by_name: HashMap<String, PathBuf>,
}

impl Cores {
    pub fn scan(dirs: &[PathBuf]) -> Self {
        let mut by_name = HashMap::new();
        for dir in dirs {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if let Some(core) = name.strip_suffix("_libretro.so") {
                    by_name.entry(core.to_string()).or_insert_with(|| e.path());
                }
            }
        }
        Self { by_name }
    }
}

/// The core for `system`: the configured one (a name or a path) when it is
/// installed, else the first installed core of the system's list.
pub fn core_for(system: System, cores: &Cores, cfg: &Config) -> Result<(String, PathBuf), String> {
    let info = system.info();
    let mut wanted_missing = None;
    if let Some(want) = cfg.cores.get(info.id) {
        let want = want.trim();
        let as_path = config::expand(want);
        if want.contains('/') {
            if as_path.is_file() {
                let name = as_path
                    .file_name()
                    .map(|n| {
                        n.to_string_lossy()
                            .trim_end_matches("_libretro.so")
                            .to_string()
                    })
                    .unwrap_or_default();
                return Ok((name, as_path));
            }
        } else {
            let name = want.trim_end_matches("_libretro.so");
            if let Some(p) = cores.by_name.get(name) {
                return Ok((name.to_string(), p.clone()));
            }
        }
        wanted_missing = Some(want.to_string());
    }
    for name in info.cores {
        if let Some(p) = cores.by_name.get(*name) {
            return Ok((name.to_string(), p.clone()));
        }
    }
    Err(match wanted_missing {
        Some(w) => format!(
            "configured core {w} is not installed; install it or {} ({})",
            info.install,
            install_where()
        ),
        None => format!(
            "no {} core: install {} ({})",
            info.badge,
            info.install,
            install_where()
        ),
    })
}

fn install_where() -> &'static str {
    "RetroArch Online Updater, or a libretro package"
}

/// A configured stand-alone emulator that is on PATH.
pub fn emulator_for(system: System, cfg: &Config) -> Option<(String, PathBuf)> {
    let want = cfg.emulators.get(system.id())?.trim();
    let path = command_in_path(want)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Some((name, path))
}

/// The argv of a stand-alone emulator. VICE programs autostart the image.
pub fn emulator_argv(program: &Path, rom: &str) -> Vec<String> {
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut argv = vec![program.to_string_lossy().to_string()];
    let vice = [
        "x64", "x64sc", "x64dtv", "x128", "xvic", "xplus4", "xpet", "xcbm2", "xscpu64",
    ];
    if vice.contains(&name.as_str()) {
        argv.push("-autostart".into());
    }
    argv.push(rom.to_string());
    argv
}

/// One entry of a RetroArch playlist (`*.lpl`, JSON since RetroArch 1.7).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub label: String,
    pub core_path: String,
    pub core_name: String,
    /// The playlist name: the thumbnail folder of the entry.
    pub playlist: String,
    pub db_name: String,
}

/// A string field, or empty when it is missing or not a string.
fn lax<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::Deserialize;
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s.trim().to_string(),
        _ => String::new(),
    })
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Item {
    #[serde(deserialize_with = "lax")]
    path: String,
    #[serde(deserialize_with = "lax")]
    label: String,
    #[serde(deserialize_with = "lax")]
    core_path: String,
    #[serde(deserialize_with = "lax")]
    core_name: String,
    #[serde(deserialize_with = "lax")]
    db_name: String,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Doc {
    items: Vec<Item>,
}

pub fn parse_playlist(text: &str, playlist: &str) -> Result<Vec<Entry>, String> {
    // New format: {"version": "1.5", "items": [...]}; old: a plain array.
    let items: Vec<Item> = if text.trim_start().starts_with('[') {
        serde_json::from_str(text).map_err(|e| e.to_string())?
    } else {
        serde_json::from_str::<Doc>(text)
            .map_err(|e| e.to_string())?
            .items
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if item.path.is_empty() {
            continue;
        }
        let mut label = item.label;
        if label.is_empty() {
            label = Path::new(content_file(&item.path))
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown game".into());
        }
        out.push(Entry {
            core_path: item.core_path,
            core_name: item.core_name,
            db_name: item.db_name,
            playlist: playlist.to_string(),
            path: item.path,
            label,
        });
    }
    Ok(out)
}

/// The file of a playlist path: `/roms/a.zip#a.nes` is `/roms/a.zip`.
pub fn content_file(path: &str) -> &str {
    match path.rfind('#') {
        Some(i) if path[..i].contains('.') => &path[..i],
        _ => path,
    }
}

/// All playlists of all installations (not the history and favourites).
pub fn playlists(installs: &[Install]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for i in installs {
        if let Ok(entries) = fs::read_dir(i.playlist_dir()) {
            paths.extend(entries.flatten().map(|e| e.path()).filter(|p| {
                p.extension().is_some_and(|e| e == "lpl")
                    && !p.file_name().is_some_and(|n| {
                        let n = n.to_string_lossy();
                        n.starts_with("content_") || n == "builtin_imageviewer.lpl"
                    })
            }));
        }
    }
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_retroarch_cfg_values() {
        let cfg = parse_cfg(
            "# comment\nrgui_browser_directory = \"~/Games/roms\"\nlibretro_directory = \":/cores\"\nthumbnails_directory = \"default\"\nvideo_fullscreen = \"true\"\nbroken line\n",
        );
        assert_eq!(
            cfg.get("video_fullscreen").map(String::as_str),
            Some("true")
        );
        let install = Install {
            base: PathBuf::from("/cfg/retroarch"),
            cfg,
        };
        assert_eq!(install.rom_dir(), Some(config::home().join("Games/roms")));
        assert_eq!(install.core_dir(), PathBuf::from("/cfg/retroarch/cores"));
        assert_eq!(
            install.thumbnail_dir(),
            PathBuf::from("/cfg/retroarch/thumbnails")
        );
        assert_eq!(
            install.playlist_dir(),
            PathBuf::from("/cfg/retroarch/playlists")
        );
    }

    #[test]
    fn reads_new_and_old_playlists() {
        let new = r#"{"version":"1.5","items":[
            {"path":"/r/snes/Zelda (USA).sfc","label":"Zelda (USA)","core_path":"DETECT","core_name":"DETECT","db_name":"Nintendo - Super Nintendo Entertainment System.lpl"},
            {"path":"/r/a.zip#Mario.nes","label":"","core_path":"/c/mesen_libretro.so","core_name":"Mesen"},
            {"path":"","label":"empty"}]}"#;
        let e = parse_playlist(new, "SNES").unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].label, "Zelda (USA)");
        assert_eq!(e[0].playlist, "SNES");
        assert_eq!(e[1].label, "a");
        assert_eq!(e[1].core_name, "Mesen");
        let old = r#"[{"path":"/g/a.zip","label":"Alpha","core_path":"/c/mame_libretro.so","core_name":"MAME"}]"#;
        assert_eq!(parse_playlist(old, "Arcade").unwrap()[0].label, "Alpha");
        assert!(parse_playlist("{", "x").is_err());
        assert!(parse_playlist("{}", "x").unwrap().is_empty());
        // Fields of another type are ignored, not fatal.
        let odd = r#"{"items":[{"path":"/r/a.nes","label":5,"crc32":"00|crc","core_path":null}]}"#;
        assert_eq!(parse_playlist(odd, "x").unwrap()[0].label, "a");
    }

    #[test]
    fn archive_members_point_at_the_archive() {
        assert_eq!(content_file("/r/a.zip#Mario.nes"), "/r/a.zip");
        assert_eq!(content_file("/r/Mario #1.nes"), "/r/Mario #1.nes");
        assert_eq!(content_file("/r/b.nes"), "/r/b.nes");
    }

    fn cfg(cores: &[(&str, &str)], emus: &[(&str, &str)]) -> Config {
        let mut c = Config::default();
        for (k, v) in cores {
            c.cores.insert(k.to_string(), v.to_string());
        }
        for (k, v) in emus {
            c.emulators.insert(k.to_string(), v.to_string());
        }
        c
    }

    #[test]
    fn maps_systems_to_installed_cores() {
        let dir = std::env::temp_dir().join(format!("nirugame-cores-{}", std::process::id()));
        let user = dir.join("user");
        let system = dir.join("system");
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&system).unwrap();
        for f in [
            "mesen_libretro.so",
            "snes9x_libretro.so",
            "vice_x64sc_libretro.so",
            "notes.txt",
        ] {
            fs::write(user.join(f), b"").unwrap();
        }
        for f in [
            "bsnes_mercury_accuracy_libretro.so",
            "gambatte_libretro.so",
            "snes9x_libretro.so",
        ] {
            fs::write(system.join(f), b"").unwrap();
        }
        let cores = Cores::scan(&[user.clone(), system.clone()]);
        assert_eq!(cores.by_name.len(), 5);
        // The user folder wins over the system one.
        assert_eq!(cores.by_name["snes9x"], user.join("snes9x_libretro.so"));
        let none = Config::default();
        assert_eq!(core_for(System::Snes, &cores, &none).unwrap().0, "snes9x");
        assert_eq!(core_for(System::Nes, &cores, &none).unwrap().0, "mesen");
        assert_eq!(
            core_for(System::C64, &cores, &none).unwrap().0,
            "vice_x64sc"
        );
        assert_eq!(core_for(System::Gb, &cores, &none).unwrap().0, "gambatte");
        assert_eq!(core_for(System::Gbc, &cores, &none).unwrap().0, "gambatte");
        // A preferred core per system.
        let pref = cfg(&[("snes", "bsnes_mercury_accuracy")], &[]);
        let (name, path) = core_for(System::Snes, &cores, &pref).unwrap();
        assert_eq!(name, "bsnes_mercury_accuracy");
        assert_eq!(path, system.join("bsnes_mercury_accuracy_libretro.so"));
        let by_path = cfg(
            &[("nes", &user.join("mesen_libretro.so").to_string_lossy())],
            &[],
        );
        assert_eq!(core_for(System::Nes, &cores, &by_path).unwrap().0, "mesen");
        // A configured core that is missing falls back and is never an error...
        let gone = cfg(&[("snes", "bsnes")], &[]);
        assert_eq!(core_for(System::Snes, &cores, &gone).unwrap().0, "snes9x");
        // ... and no core at all is a hint, not a failure.
        let hint = core_for(System::N64, &cores, &none).unwrap_err();
        assert!(
            hint.contains("N64") && hint.contains("Mupen64Plus"),
            "{hint}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn vice_autostarts_and_other_emulators_get_the_file() {
        assert_eq!(
            emulator_argv(Path::new("/usr/bin/x64sc"), "/r/Elite.d64"),
            vec!["/usr/bin/x64sc", "-autostart", "/r/Elite.d64"]
        );
        assert_eq!(
            emulator_argv(Path::new("/usr/bin/fs-uae"), "/r/a.adf"),
            vec!["/usr/bin/fs-uae", "/r/a.adf"]
        );
        let c = cfg(&[], &[("c64", "/bin/sh")]);
        assert_eq!(emulator_for(System::C64, &c).unwrap().0, "sh");
        assert!(emulator_for(System::Nes, &c).is_none());
        let c = cfg(&[], &[("c64", "no-such-emulator-here")]);
        assert!(emulator_for(System::C64, &c).is_none());
    }
}
