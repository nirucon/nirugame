//! Optional user-supplied SID background music.
//!
//! NIRUGAME never bundles SID tunes. It scans one user-owned directory and
//! starts the optional `sidplayfp` player for the selected file. The child is
//! killed and reaped on stop, selection change and application exit.

use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Clone, Debug)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Empty means Off.
    pub music: String,
}

pub fn music_dir() -> PathBuf {
    if let Some(path) = env::var_os("NIRUGAME_MUSIC_DIR") {
        return PathBuf::from(path);
    }
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share")
        })
        .join("nirugame/music")
}

pub fn config_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"))
        .join("nirugame/config.toml")
}

pub fn load_config() -> Config {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = toml::to_string_pretty(config).map_err(|error| error.to_string())?;
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, text).map_err(|error| error.to_string())?;
    fs::rename(temp, path).map_err(|error| error.to_string())
}

fn sid_title(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 0x36 {
        return None;
    }
    if &bytes[..4] != b"PSID" && &bytes[..4] != b"RSID" {
        return None;
    }
    let title = &bytes[0x16..0x36];
    let title = String::from_utf8_lossy(title)
        .trim_matches(|ch: char| ch == '\0' || ch.is_whitespace())
        .to_string();
    (!title.is_empty()).then_some(title)
}

pub fn discover() -> Vec<Track> {
    let dir = music_dir();
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut tracks = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
            if !matches!(extension.as_str(), "sid" | "psid" | "rsid") {
                return None;
            }
            let title = sid_title(&path).unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("SID track")
                    .to_string()
            });
            Some(Track { path, title })
        })
        .collect::<Vec<_>>();
    tracks.sort_by_key(|track| track.title.to_lowercase());
    tracks.truncate(256);
    tracks
}

pub struct Player {
    child: Option<Child>,
    pub selected: PathBuf,
    pub error: String,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            child: None,
            selected: PathBuf::new(),
            error: String::new(),
        }
    }
}

impl Player {
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        self.error.clear();
        self.selected = path.to_path_buf();
        if path.as_os_str().is_empty() {
            return Ok(());
        }
        let mut command = Command::new("sidplayfp");
        command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(target_os = "linux")]
        unsafe {
            command.pre_exec(|| {
                // Do not leave a SID player behind on logout or a forced app exit.
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                if libc::getppid() == 1 {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(|error| {
            self.selected.clear();
            if error.kind() == std::io::ErrorKind::NotFound {
                "sidplayfp is not installed; install it from your distribution".into()
            } else {
                format!("cannot start sidplayfp: {error}")
            }
        })?;
        self.child = Some(child);
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_psid_title_and_rejects_other_files() {
        let dir = std::env::temp_dir().join(format!("nirugame-sid-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("track.sid");
        let mut bytes = vec![0u8; 0x36];
        bytes[..4].copy_from_slice(b"PSID");
        bytes[0x16..0x1e].copy_from_slice(b"Commando");
        fs::write(&path, &bytes).unwrap();
        assert_eq!(sid_title(&path).as_deref(), Some("Commando"));
        fs::write(dir.join("not-sid.sid"), b"text").unwrap();
        assert!(sid_title(&dir.join("not-sid.sid")).is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
