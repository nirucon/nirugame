//! ROM folders: which folders to scan, the scan itself and the index cache.
//!
//! The scan walks each ROM folder to a limited depth, never follows a link
//! twice and skips BIOS, save and media folders. The result is cached in
//! `~/.cache/nirugame/roms.json` together with the modification time of every
//! folder it read: adding, removing or renaming a file changes its folder's
//! time, so a cached index is used only while all those times are the same.

use crate::config::{self, Config};
use crate::systems::{self, System};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// One ROM file found by the scan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rom {
    pub path: String,
    pub system: System,
    pub name: String,
    pub size: u64,
}

const CACHE_VERSION: u32 = 1;
const MAX_ROMS: usize = 100_000;
const MAX_DIRS: usize = 20_000;

/// Folders that hold no games of their own.
const SKIP: &[&str] = &[
    "bios",
    "system",
    "saves",
    "savefiles",
    "savestates",
    "states",
    "screenshots",
    "thumbnails",
    "boxart",
    "boxarts",
    "snap",
    "snaps",
    "titles",
    "media",
    "manuals",
    "cheats",
    "shaders",
    "overlays",
    "config",
    "cores",
    "playlists",
    "images",
    "covers",
    "wheel",
    "video",
    "videos",
    "music",
    "docs",
    "lostfound",
    "downloaded_images",
    "artwork",
];

/// Every folder whose time a cached index depends on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    /// The folders that were scanned and the depth.
    pub key: String,
    pub dirs: Vec<(String, i64)>,
    pub roms: Vec<Rom>,
}

fn mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .map(|m| m.mtime() * 1_000_000_000 + m.mtime_nsec())
        .unwrap_or(0)
}

/// A child of `parent` whose name matches `name` ignoring case.
fn child_ci(parent: &Path, name: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

/// Never scan the home folder or one of its parents as a ROM folder: a
/// RetroArch file browser that starts in `~` is common.
fn too_wide(path: &Path) -> bool {
    let home = config::home();
    path == Path::new("/") || path.as_os_str().is_empty() || home.starts_with(path)
}

/// The ROM folders to scan: the configured ones; unless `defaults = false`
/// also ~/Games/roms (any case), ~/ROMs, ~/roms, system folders directly in
/// ~/Games (PS1, snes, ...), RetroArch's file browser folder and the folders
/// of existing playlist entries. Nested and duplicate folders are dropped.
pub fn roots(cfg: &Config, rom_dirs: &[PathBuf], playlist_files: &[String]) -> Vec<PathBuf> {
    let home = config::home();
    let mut list: Vec<PathBuf> = cfg.roms.dirs.iter().map(|d| config::expand(d)).collect();
    if cfg.roms.defaults {
        let games = home.join("Games");
        list.extend(child_ci(&games, "roms"));
        if let Ok(entries) = fs::read_dir(&games) {
            let mut sys: Vec<PathBuf> = entries
                .flatten()
                .filter(|e| systems::system_for_folder(&e.file_name().to_string_lossy()).is_some())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            sys.sort();
            list.extend(sys);
        }
        list.extend(["ROMs", "roms", "Roms"].map(|n| home.join(n)));
        list.extend(rom_dirs.iter().cloned());
        let mut parents: Vec<PathBuf> = playlist_files
            .iter()
            .filter_map(|f| Path::new(f).parent().map(Path::to_path_buf))
            .collect();
        parents.sort();
        parents.dedup();
        list.extend(parents);
    }
    let mut out: Vec<PathBuf> = Vec::new();
    let mut canon: Vec<PathBuf> = Vec::new();
    for dir in list {
        let Ok(c) = fs::canonicalize(&dir) else {
            continue;
        };
        if !c.is_dir() || too_wide(&c) || too_wide(&dir) || canon.contains(&c) {
            continue;
        }
        canon.push(c.clone());
        out.push(c);
    }
    // A folder inside another one is scanned with it.
    let all = out.clone();
    out.retain(|d| !all.iter().any(|o| o != d && d.starts_with(o)));
    out
}

pub fn index_key(roots: &[PathBuf], depth: usize) -> String {
    let mut key = format!("depth={depth}");
    for r in roots {
        key.push('\n');
        key.push_str(&r.to_string_lossy());
    }
    key
}

/// File names inside a .zip (from its central directory, at most `max`).
pub fn zip_members(path: &Path, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(mut f) = fs::File::open(path) else {
        return out;
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else {
        return out;
    };
    if len < 22 {
        return out;
    }
    let tail = len.min(22 + 65_535);
    let mut buf = vec![0u8; tail as usize];
    if f.seek(SeekFrom::Start(len - tail)).is_err() || f.read_exact(&mut buf).is_err() {
        return out;
    }
    let Some(eocd) = (0..=buf.len() - 22)
        .rev()
        .find(|&i| buf[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
    else {
        return out;
    };
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]) as usize;
    let u32_at =
        |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as u64;
    let entries = u16_at(&buf, eocd + 10);
    let size = u32_at(&buf, eocd + 12).min(1 << 20);
    let offset = u32_at(&buf, eocd + 16);
    if offset + size > len {
        return out;
    }
    let mut dir = vec![0u8; size as usize];
    if f.seek(SeekFrom::Start(offset)).is_err() || f.read_exact(&mut dir).is_err() {
        return out;
    }
    let mut i = 0;
    for _ in 0..entries.min(max) {
        if i + 46 > dir.len() || dir[i..i + 4] != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }
        let n = u16_at(&dir, i + 28);
        let extra = u16_at(&dir, i + 30);
        let comment = u16_at(&dir, i + 32);
        if i + 46 + n > dir.len() {
            break;
        }
        out.push(String::from_utf8_lossy(&dir[i + 46..i + 46 + n]).to_string());
        i += 46 + n + extra + comment;
    }
    out
}

/// The system of an archive from the files it holds.
fn zip_system(path: &Path) -> Option<System> {
    zip_members(path, 16).iter().find_map(|m| {
        let ext = Path::new(m).extension()?.to_str()?;
        systems::system_for_extension(ext)
    })
}

/// Files a disc playlist (.m3u) lists: they are shown once, as the .m3u.
fn m3u_members(path: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            Path::new(l)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
        })
        .collect()
}

struct Walk {
    roms: Vec<Rom>,
    dirs: Vec<(String, i64)>,
    seen: HashSet<(u64, u64)>,
    depth: usize,
}

impl Walk {
    fn dir(&mut self, dir: &Path, level: usize, chain: &[String]) {
        if self.dirs.len() >= MAX_DIRS || self.roms.len() >= MAX_ROMS {
            return;
        }
        let Ok(meta) = fs::metadata(dir) else {
            return;
        };
        if !self.seen.insert((meta.dev(), meta.ino())) {
            return;
        }
        self.dirs.push((
            dir.to_string_lossy().to_string(),
            meta.mtime() * 1_000_000_000 + meta.mtime_nsec(),
        ));
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<(PathBuf, String, u64)> = Vec::new();
        let mut subdirs = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else {
                continue;
            };
            let path = e.path();
            let (is_dir, size) = if ft.is_symlink() {
                match fs::metadata(&path) {
                    Ok(m) => (m.is_dir(), m.len()),
                    Err(_) => continue,
                }
            } else if ft.is_dir() {
                (true, 0)
            } else {
                (false, e.metadata().map(|m| m.len()).unwrap_or(0))
            };
            if is_dir {
                if level < self.depth && !SKIP.contains(&systems::normalise(&name).as_str()) {
                    subdirs.push((path, name));
                }
                continue;
            }
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if systems::is_candidate(&ext) {
                files.push((path, ext, size));
            }
        }
        let has_cue = files.iter().any(|f| f.1 == "cue");
        let mut listed: HashSet<String> = HashSet::new();
        for f in files.iter().filter(|f| f.1 == "m3u") {
            listed.extend(m3u_members(&f.0));
        }
        let folders: Vec<&str> = chain.iter().rev().map(String::as_str).collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, ext, size) in files {
            if (ext == "bin" || ext == "img") && has_cue {
                continue;
            }
            let lower = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if listed.contains(&lower) {
                continue;
            }
            let mut system = systems::detect(&path, &folders);
            if system.is_none() && ext == "zip" {
                system = zip_system(&path);
            }
            let Some(system) = system else {
                continue;
            };
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            self.roms.push(Rom {
                path: path.to_string_lossy().to_string(),
                system,
                name,
                size,
            });
        }
        subdirs.sort();
        for (path, name) in subdirs {
            let mut next = chain.to_vec();
            next.push(name);
            self.dir(&path, level + 1, &next);
        }
    }
}

/// Scan `roots` to `depth` levels below each.
pub fn scan(roots: &[PathBuf], depth: usize) -> Index {
    let mut w = Walk {
        roms: Vec::new(),
        dirs: Vec::new(),
        seen: HashSet::new(),
        depth,
    };
    for root in roots {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        w.dir(root, 0, &[name]);
    }
    Index {
        version: CACHE_VERSION,
        key: index_key(roots, depth),
        dirs: w.dirs,
        roms: w.roms,
    }
}

pub fn cache_path() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| config::home().join(".cache"))
        .join("nirugame/roms.json")
}

pub fn load_cache(path: &Path) -> Option<Index> {
    let text = fs::read(path).ok()?;
    let index: Index = serde_json::from_slice(&text).ok()?;
    (index.version == CACHE_VERSION).then_some(index)
}

pub fn save_cache(path: &Path, index: &Index) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_vec(index).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

/// A cached index is good while the folder list and every folder time match.
pub fn is_fresh(index: &Index, key: &str) -> bool {
    index.version == CACHE_VERSION
        && index.key == key
        && index.dirs.iter().all(|(d, t)| mtime(Path::new(d)) == *t)
}

/// How the index was obtained, for the status line and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Cache,
    Scan,
}

/// The index for `roots`: the cache when fresh, else a new scan (saved).
/// `stale` receives an outdated cached index before a rescan starts, so a
/// caller can show it meanwhile.
pub fn index(
    roots: &[PathBuf],
    depth: usize,
    force: bool,
    stale: &mut dyn FnMut(&Index),
) -> (Index, Source) {
    let key = index_key(roots, depth);
    let path = cache_path();
    if !force {
        if let Some(cached) = load_cache(&path) {
            if is_fresh(&cached, &key) {
                return (cached, Source::Cache);
            }
            stale(&cached);
        }
    }
    let index = scan(roots, depth);
    let _ = save_cache(&path, &index);
    (index, Source::Scan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nirugame-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"").unwrap();
    }

    /// A .zip with only a central directory: enough for `zip_members`.
    fn fake_zip(path: &Path, names: &[&str]) {
        let mut cd = Vec::new();
        for n in names {
            let mut h = vec![0u8; 46];
            h[..4].copy_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
            h[28..30].copy_from_slice(&(n.len() as u16).to_le_bytes());
            cd.extend(h);
            cd.extend(n.as_bytes());
        }
        let mut eocd = vec![0u8; 22];
        eocd[..4].copy_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        eocd[10..12].copy_from_slice(&(names.len() as u16).to_le_bytes());
        eocd[12..16].copy_from_slice(&(cd.len() as u32).to_le_bytes());
        let mut bytes = cd;
        bytes.extend(eocd);
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn scans_recursively_by_extension_and_folder() {
        let d = tmp("scan");
        let r = d.join("roms");
        touch(&r.join("nes/Mario (USA).nes"));
        touch(&r.join("nes-homebrew/Alter Ego.NES"));
        touch(&r.join("SNES/Zelda.sfc"));
        touch(&r.join("SNES/Mario World.SMC"));
        touch(&r.join("megadrive/Sonic.zip"));
        touch(&r.join("loose/Unknown.zip"));
        touch(&r.join("c64/games/a/Elite.d64"));
        touch(&r.join("c64/notes.txt"));
        touch(&r.join("bios/scph1001.bin"));
        touch(&r.join(".hidden/Secret.nes"));
        touch(&r.join("PS1/Crash/Crash.cue"));
        touch(&r.join("PS1/Crash/Crash (Track 1).bin"));
        touch(&r.join("PS1/FF7/FF7.m3u"));
        fs::write(
            r.join("PS1/FF7/FF7.m3u"),
            "FF7 (Disc 1).chd\nFF7 (Disc 2).chd\n",
        )
        .unwrap();
        touch(&r.join("PS1/FF7/FF7 (Disc 1).chd"));
        touch(&r.join("PS1/FF7/FF7 (Disc 2).chd"));
        fake_zip(&r.join("loose/Tetris.zip"), &["readme.txt", "Tetris.gb"]);
        touch(&r.join("deep/1/2/3/4/5/6/7/Too Deep.nes"));
        let idx = scan(std::slice::from_ref(&r), 6);
        let mut got: Vec<(String, &str)> = idx
            .roms
            .iter()
            .map(|x| (x.name.clone(), x.system.id()))
            .collect();
        got.sort();
        let want = [
            ("Alter Ego", "nes"),
            ("Crash", "psx"),
            ("Elite", "c64"),
            ("FF7", "psx"),
            ("Mario (USA)", "nes"),
            ("Mario World", "snes"),
            ("Sonic", "megadrive"),
            ("Tetris", "gb"),
            ("Zelda", "snes"),
        ];
        assert_eq!(
            got,
            want.iter()
                .map(|(a, b)| (a.to_string(), *b))
                .collect::<Vec<_>>()
        );
        assert!(idx.dirs.iter().any(|(p, _)| p.ends_with("c64/games/a")));
        assert!(!idx.dirs.iter().any(|(p, _)| p.contains("bios")));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn reads_zip_central_directory() {
        let d = tmp("zip");
        let z = d.join("a.zip");
        fake_zip(&z, &["Super Mario Bros.nes", "b.txt"]);
        assert_eq!(zip_members(&z, 8), vec!["Super Mario Bros.nes", "b.txt"]);
        assert_eq!(zip_system(&z), Some(System::Nes));
        fs::write(&z, b"not a zip at all, but long enough to look").unwrap();
        assert!(zip_members(&z, 8).is_empty());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn link_loops_are_scanned_once() {
        let d = tmp("loop");
        let r = d.join("roms");
        touch(&r.join("nes/A.nes"));
        std::os::unix::fs::symlink(&r, r.join("nes/again")).unwrap();
        let idx = scan(std::slice::from_ref(&r), 6);
        assert_eq!(idx.roms.len(), 1);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn cache_is_used_until_a_folder_changes() {
        let d = tmp("cache");
        let r = d.join("roms");
        touch(&r.join("snes/A.sfc"));
        let idx = scan(std::slice::from_ref(&r), 6);
        let key = index_key(std::slice::from_ref(&r), 6);
        let file = d.join("cache/roms.json");
        save_cache(&file, &idx).unwrap();
        let back = load_cache(&file).unwrap();
        assert_eq!(back, idx);
        assert!(is_fresh(&back, &key));
        assert!(!is_fresh(&back, &index_key(std::slice::from_ref(&r), 3)));
        // A new file changes the folder time: the cache is stale.
        std::thread::sleep(std::time::Duration::from_millis(20));
        touch(&r.join("snes/B.sfc"));
        assert!(!is_fresh(&back, &key));
        assert_eq!(scan(std::slice::from_ref(&r), 6).roms.len(), 2);
        // A damaged or old cache is ignored.
        fs::write(&file, b"{broken").unwrap();
        assert!(load_cache(&file).is_none());
        fs::write(&file, br#"{"version":0,"key":"","dirs":[],"roms":[]}"#).unwrap();
        assert!(load_cache(&file).is_none());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn roots_drop_nested_missing_and_home() {
        let d = tmp("roots");
        let a = d.join("a");
        fs::create_dir_all(a.join("snes")).unwrap();
        let mut cfg = Config::default();
        cfg.roms.defaults = false;
        cfg.roms.dirs = vec![
            a.to_string_lossy().to_string(),
            a.join("snes").to_string_lossy().to_string(),
            d.join("missing").to_string_lossy().to_string(),
            a.to_string_lossy().to_string(),
            "/".into(),
        ];
        let r = roots(&cfg, &[], &[]);
        assert_eq!(r, vec![fs::canonicalize(&a).unwrap()]);
        let _ = fs::remove_dir_all(d);
    }
}
