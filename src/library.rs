//! Favourites and play history in `~/.local/share/nirugame/library.json`.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Played {
    pub count: u32,
    /// Unix time of the last start.
    pub last: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub favourites: BTreeSet<String>,
    pub played: BTreeMap<String, Played>,
}

pub fn path() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::config::home().join(".local/share"))
        .join("nirugame/library.json")
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Library {
    pub fn load_from(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text).map_err(|e| e.to_string())?;
        fs::rename(temp, path).map_err(|e| e.to_string())
    }

    /// Toggle; returns whether `key` is a favourite now.
    pub fn toggle_favourite(&mut self, key: &str) -> bool {
        if self.favourites.remove(key) {
            false
        } else {
            self.favourites.insert(key.to_string());
            true
        }
    }

    pub fn record_play(&mut self, key: &str, at: i64) {
        let p = self.played.entry(key.to_string()).or_default();
        p.count = p.count.saturating_add(1);
        p.last = at;
    }

    /// Played keys, most recent first.
    pub fn recent(&self, max: usize) -> Vec<&str> {
        let mut v: Vec<(&String, &Played)> = self.played.iter().collect();
        v.sort_by(|a, b| b.1.last.cmp(&a.1.last).then(a.0.cmp(b.0)));
        v.into_iter().take(max).map(|(k, _)| k.as_str()).collect()
    }
}

/// Local date and time as "YYYY-MM-DD HH:MM".
pub fn format_time(t: i64) -> String {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // `as _`: time_t without naming the alias, which musl deprecates.
    let tt = t as _;
    if unsafe { libc::localtime_r(&tt, &mut tm) }.is_null() {
        return String::new();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

/// "512 KB", "3.2 MB".
pub fn format_size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b < K {
        format!("{bytes} B")
    } else if b < K * K {
        format!("{:.0} KB", b / K)
    } else if b < K * K * K {
        format!("{:.1} MB", b / K / K)
    } else {
        format!("{:.2} GB", b / K / K / K)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favourites_and_history_round_trip() {
        let dir = std::env::temp_dir().join(format!("nirugame-lib-{}", std::process::id()));
        let file = dir.join("library.json");
        let mut lib = Library::default();
        assert!(lib.toggle_favourite("rom:/a.sfc"));
        assert!(lib.toggle_favourite("steam:1"));
        assert!(!lib.toggle_favourite("steam:1"));
        lib.record_play("rom:/a.sfc", 100);
        lib.record_play("rom:/a.sfc", 300);
        lib.record_play("steam:2", 200);
        assert_eq!(
            lib.played["rom:/a.sfc"],
            Played {
                count: 2,
                last: 300
            }
        );
        assert_eq!(lib.recent(10), vec!["rom:/a.sfc", "steam:2"]);
        assert_eq!(lib.recent(1).len(), 1);
        lib.save_to(&file).unwrap();
        assert_eq!(Library::load_from(&file), lib);
        fs::write(&file, b"garbage").unwrap();
        assert_eq!(Library::load_from(&file), Library::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn formats_sizes_and_times() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(512 * 1024), "512 KB");
        assert_eq!(format_size(3 * 1024 * 1024 + 300 * 1024), "3.3 MB");
        assert_eq!(format_time(0).len(), 16);
    }
}
