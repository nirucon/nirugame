//! `~/.config/nirugame/config.toml`: the SID track, extra ROM folders, the
//! preferred core per system and optional stand-alone emulators. NIRUGAME
//! writes only the `music` line and keeps the rest of the file as written.

use serde::Deserialize;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Roms {
    /// Extra folders to scan, besides the defaults.
    pub dirs: Vec<String>,
    /// Scan the default folders (~/Games/roms, ~/ROMs, RetroArch's ...).
    pub defaults: bool,
    /// How deep below a ROM folder to look.
    pub depth: usize,
}

impl Default for Roms {
    fn default() -> Self {
        Self {
            dirs: Vec::new(),
            defaults: true,
            depth: 6,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// The SID background track; empty means Off.
    pub music: String,
    pub roms: Roms,
    /// System id -> core name ("snes9x") or path to a `*_libretro.so`.
    pub cores: BTreeMap<String, String>,
    /// System id -> stand-alone emulator program (`c64 = "x64sc"`).
    pub emulators: BTreeMap<String, String>,
}

pub fn path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
        .join("nirugame/config.toml")
}

pub fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

/// `~/x` and `$HOME/x` to absolute paths.
pub fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    if path == "~" {
        return home();
    }
    if let Some(rest) = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("$HOME/"))
    {
        return home().join(rest);
    }
    PathBuf::from(path)
}

pub fn parse(text: &str) -> Config {
    toml::from_str(text).unwrap_or_default()
}

pub fn load() -> Config {
    fs::read_to_string(path())
        .map(|t| parse(&t))
        .unwrap_or_default()
}

/// The configuration text with its top-level `music` line set to `track`;
/// comments, sections and other keys stay as they are.
pub fn with_music(text: &str, track: &str) -> String {
    let line = format!("music = {}", toml::Value::String(track.to_string()));
    let mut out = Vec::new();
    let mut done = false;
    let mut in_table = false;
    for l in text.lines() {
        let t = l.trim_start();
        if t.starts_with('[') {
            if !done {
                out.push(line.clone());
                done = true;
            }
            in_table = true;
        }
        if !in_table && !done && t.starts_with("music") {
            let rest = t["music".len()..].trim_start();
            if rest.starts_with('=') {
                out.push(line.clone());
                done = true;
                continue;
            }
        }
        out.push(l.to_string());
    }
    if !done {
        out.push(line);
    }
    // A table header right after the music line reads better with a gap.
    let mut text = out.join("\n");
    text.push('\n');
    text
}

pub fn save_music(track: &str) -> Result<(), String> {
    save_music_at(&path(), track)
}

pub fn save_music_at(path: &Path, track: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let old = fs::read_to_string(path).unwrap_or_default();
    let text = with_music(&old, track);
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_folders_cores_and_emulators() {
        let c = parse(
            "music = \"/m/a.sid\"\n[roms]\ndirs = [\"~/More\", \"/mnt/roms\"]\ndepth = 3\n[cores]\nsnes = \"snes9x\"\n[emulators]\nc64 = \"x64sc\"\n",
        );
        assert_eq!(c.music, "/m/a.sid");
        assert_eq!(c.roms.dirs, vec!["~/More", "/mnt/roms"]);
        assert!(c.roms.defaults);
        assert_eq!(c.roms.depth, 3);
        assert_eq!(c.cores.get("snes").map(String::as_str), Some("snes9x"));
        assert_eq!(c.emulators.get("c64").map(String::as_str), Some("x64sc"));
        assert_eq!(parse("not toml ["), Config::default());
    }

    #[test]
    fn saving_music_keeps_the_rest_of_the_file() {
        let text = "# my settings\nmusic = \"old.sid\"\n\n[cores]\nsnes = \"snes9x\" # fast\n";
        let new = with_music(text, "/x/new \"tune\".sid");
        assert!(new.contains("# my settings"));
        assert!(new.contains("snes = \"snes9x\" # fast"));
        assert!(!new.contains("old.sid"));
        let c = parse(&new);
        assert_eq!(c.music, "/x/new \"tune\".sid");
        assert_eq!(c.cores.get("snes").map(String::as_str), Some("snes9x"));
        // No music line yet: it goes before the first table.
        let new = with_music("[cores]\nnes = \"mesen\"\n", "");
        assert!(new.starts_with("music = \"\"\n[cores]"));
        assert_eq!(parse(&new).cores.len(), 1);
        assert_eq!(with_music("", "a.sid"), "music = \"a.sid\"\n");
        // A key that only starts with "music" is not the music line.
        let new = with_music("musicdir = 1\n", "b.sid");
        assert!(new.contains("musicdir = 1") && new.contains("music = \"b.sid\""));
    }

    #[test]
    fn expands_home() {
        let h = home();
        assert_eq!(expand("~/Games"), h.join("Games"));
        assert_eq!(expand("$HOME/x"), h.join("x"));
        assert_eq!(expand("/abs"), PathBuf::from("/abs"));
    }
}
