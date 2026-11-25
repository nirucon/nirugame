//! Local game discovery. Nothing here downloads metadata or starts a daemon.
//! Steam is read from its appmanifest files, RetroArch from its playlists and
//! the ROM folders, native games from desktop entries. Launch commands are
//! argv arrays, never shell strings.

use crate::config::{self, Config};
use crate::retroarch::{self, Cores, Entry, Install};
use crate::roms::{self, Rom, Source};
use crate::systems::{self, System};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Steam,
    RetroArch,
    Native,
}

/// What NIRUGAME knows about a ROM.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RomInfo {
    pub system: Option<System>,
    pub file: String,
    pub size: u64,
    /// The core or emulator that starts it ("snes9x", "x64sc").
    pub player: String,
    /// Why it cannot start (no core, no RetroArch, file gone); empty if it can.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hint: String,
    /// The RetroArch playlist (thumbnail folder) and label.
    pub playlist: String,
    pub label: String,
    /// The other way to start it: RetroArch when an emulator is configured.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alt_launch: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub alt_label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Game {
    pub key: String,
    pub kind: Kind,
    pub name: String,
    pub detail: String,
    pub launch: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rom: Option<Box<RomInfo>>,
}

impl Game {
    pub fn system(&self) -> Option<System> {
        self.rom.as_ref().and_then(|r| r.system)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Catalog {
    pub steam: Vec<Game>,
    pub retroarch: Vec<Game>,
    pub native: Vec<Game>,
    pub errors: Vec<String>,
    /// The ROM folders that were scanned.
    pub rom_folders: Vec<PathBuf>,
    /// RetroArch thumbnail folders, for box art.
    pub thumbnail_dirs: Vec<PathBuf>,
    /// "cache" or "scan".
    pub index: &'static str,
}

impl Catalog {
    pub fn all(&self) -> Vec<&Game> {
        let mut all = Vec::new();
        all.extend(&self.steam);
        all.extend(&self.retroarch);
        all.extend(&self.native);
        all.sort_by_key(|game| game.name.to_lowercase());
        all
    }
    pub fn count(&self) -> usize {
        self.steam.len() + self.retroarch.len() + self.native.len()
    }

    pub fn discover() -> Self {
        Self::discover_with(&config::load(), false, &mut |_| {})
    }

    /// Everything, with the ROM index from the cache unless `force`. When
    /// the cache is outdated, `stale` first receives a catalog built from it.
    pub fn discover_with(cfg: &Config, force: bool, stale: &mut dyn FnMut(Catalog)) -> Self {
        let t0 = std::time::Instant::now();
        let timing = env::var_os("NIRUGAME_TIMING").is_some();
        let others = thread::spawn(move || {
            let s = discover_steam();
            let t1 = t0.elapsed();
            let n = discover_native();
            if timing {
                eprintln!(
                    "nirugame: steam {:.1} ms, native {:.1} ms",
                    t1.as_secs_f64() * 1000.0,
                    (t0.elapsed() - t1).as_secs_f64() * 1000.0
                );
            }
            (s, n)
        });
        let mut catalog = Self::default();
        let retro = discover_retroarch(cfg, force, &mut |games, ctx| {
            stale(Catalog {
                retroarch: games,
                thumbnail_dirs: ctx.thumbnail_dirs.clone(),
                index: "cache",
                ..Default::default()
            })
        });
        if timing {
            eprintln!(
                "nirugame: retroarch and ROM folders {:.1} ms ({} entries, {:?})",
                t0.elapsed().as_secs_f64() * 1000.0,
                retro.games.len(),
                retro.source
            );
        }
        catalog.retroarch = retro.games;
        catalog.errors = retro.errors;
        catalog.rom_folders = retro.roots;
        catalog.thumbnail_dirs = retro.thumbnail_dirs;
        catalog.index = match retro.source {
            Source::Cache => "cache",
            Source::Scan => "scan",
        };
        let (steam, native) = others
            .join()
            .unwrap_or_else(|_| (Err("discovery failed".into()), Ok(Vec::new())));
        match steam {
            Ok(games) => catalog.steam = games,
            Err(error) => catalog.errors.push(format!("Steam: {error}")),
        }
        match native {
            Ok(games) => catalog.native = games,
            Err(error) => catalog.errors.push(format!("Native: {error}")),
        }
        catalog
    }
}

pub fn launch(game: &Game) -> Result<(), String> {
    launch_argv(&game.launch, &game.name)
}

pub fn launch_argv(argv: &[String], name: &str) -> Result<(), String> {
    let Some(program) = argv.first() else {
        return Err(format!("{name} has no launch command"));
    };
    let mut command = Command::new(program);
    command
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::process::detach_terminal(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {name}: {error}"))?;
    // The launcher may stay open. Reap the child without blocking the UI.
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

fn command_in_path(name: &str) -> Option<String> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(name))
        .find(|candidate| candidate.is_file())
        .map(|path| path.to_string_lossy().to_string())
}

fn command_exists(name: &str) -> bool {
    command_in_path(name).is_some()
        || ["/usr/games", "/usr/bin", "/usr/local/bin"]
            .iter()
            .any(|dir| Path::new(dir).join(name).is_file())
}

fn quoted(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut value = String::new();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => {
                    values.push(value);
                    break;
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        value.push(next);
                    }
                }
                _ => value.push(ch),
            }
        }
    }
    values
}

fn pair_map(text: &str) -> std::collections::HashMap<String, String> {
    // VDF keys and values share a line. Token-pairing the whole file breaks as
    // soon as a nested table starts (AppState, InstalledDepots, ...), so only
    // take lines that contain exactly one key/value pair.
    let mut pairs = std::collections::HashMap::new();
    for line in text.lines() {
        let values = quoted(line);
        if values.len() == 2 {
            pairs.insert(values[0].clone(), values[1].clone());
        }
    }
    pairs
}

fn steam_prefix() -> Option<Vec<String>> {
    if let Some(path) = command_in_path("steam") {
        return Some(vec![path]);
    }
    if let Some(path) = ["/usr/games/steam", "/usr/bin/steam"]
        .iter()
        .find(|path| Path::new(path).is_file())
    {
        return Some(vec![(*path).to_string()]);
    }
    if command_exists("flatpak") {
        return Some(vec![
            "flatpak".into(),
            "run".into(),
            "com.valvesoftware.Steam".into(),
        ]);
    }
    None
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        home().join(".steam/steam"),
        home().join(".steam/root"),
        home().join(".steam/debian-installation"),
        home().join(".local/share/Steam"),
        home().join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    let mut index = 0;
    while index < roots.len() {
        let root = roots[index].clone();
        let folders = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(folders) {
            let values = quoted(&text);
            for window in values.windows(2) {
                if window[0] == "path" && !window[1].is_empty() {
                    let path = PathBuf::from(&window[1]);
                    if !roots.contains(&path) {
                        roots.push(path);
                    }
                }
            }
        }
        index += 1;
    }
    roots.into_iter().filter(|root| root.is_dir()).collect()
}

fn steam_manifest(path: &Path) -> Option<Game> {
    let text = fs::read_to_string(path).ok()?;
    let pairs = pair_map(&text);
    let appid = pairs.get("appid")?.clone();
    let name = pairs.get("name")?.clone();
    if name.trim().is_empty() || !appid.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let prefix = steam_prefix();
    let launch = prefix
        .map(|mut command| {
            command.push("-applaunch".into());
            command.push(appid.clone());
            command
        })
        .unwrap_or_default();
    let installdir = pairs.get("installdir").cloned().unwrap_or_default();
    Some(Game {
        key: format!("steam:{appid}"),
        kind: Kind::Steam,
        name,
        detail: format!("Steam · app {appid} · {installdir}"),
        launch,
        rom: None,
    })
}

fn discover_steam() -> Result<Vec<Game>, String> {
    let roots = steam_roots();
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let mut games = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let apps = root.join("steamapps");
        let Ok(entries) = fs::read_dir(apps) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            if let Some(game) = steam_manifest(&entry.path()) {
                if seen.insert(game.key.clone()) {
                    games.push(game);
                }
            }
        }
    }
    games.sort_by_key(|game| game.name.to_lowercase());
    Ok(games)
}

/// The configured stand-alone emulators that are installed.
fn emulators(cfg: &Config) -> HashMap<System, (String, PathBuf)> {
    systems::ALL
        .iter()
        .filter_map(|s| Some((*s, retroarch::emulator_for(*s, cfg)?)))
        .collect()
}

/// Everything a ROM entry needs to be started.
pub struct RetroContext {
    cfg: Config,
    prefix: Option<Vec<String>>,
    cores: Cores,
    resolved: HashMap<System, Result<(String, PathBuf), String>>,
    emulators: HashMap<System, (String, PathBuf)>,
    pub thumbnail_dirs: Vec<PathBuf>,
}

impl RetroContext {
    pub fn new(cfg: &Config, installs: &[Install]) -> Self {
        let cores = Cores::scan(&retroarch::core_dirs(installs));
        let resolved = systems::ALL
            .iter()
            .map(|s| (*s, retroarch::core_for(*s, &cores, cfg)))
            .collect();
        let mut thumbnail_dirs: Vec<PathBuf> =
            installs.iter().map(Install::thumbnail_dir).collect();
        thumbnail_dirs.dedup();
        Self {
            emulators: emulators(cfg),
            cfg: cfg.clone(),
            prefix: retroarch::prefix(),
            cores,
            resolved,
            thumbnail_dirs,
        }
    }

    pub fn core(&self, system: System) -> Result<(String, PathBuf), String> {
        self.resolved
            .get(&system)
            .cloned()
            .unwrap_or_else(|| Err("unknown system".into()))
    }

    pub fn cores(&self) -> &Cores {
        &self.cores
    }

    pub fn retroarch(&self) -> Option<&[String]> {
        self.prefix.as_deref()
    }

    /// The game for one ROM. `content` is what RetroArch gets (it may name
    /// a member of an archive), `file` the file on disk.
    #[allow(clippy::too_many_arguments)]
    fn game(
        &self,
        content: &str,
        file: &str,
        name: &str,
        system: Option<System>,
        size: u64,
        playlist: &str,
        entry_core: Option<(String, PathBuf)>,
        exists: bool,
    ) -> Game {
        let configured = system.filter(|s| self.cfg.cores.contains_key(s.id()));
        let core: Result<(String, PathBuf), String> = match (configured, entry_core) {
            (Some(s), entry) => match self.core(s) {
                Ok(c) => Ok(c),
                Err(e) => entry.ok_or(e),
            },
            (None, Some(c)) => Ok(c),
            (None, None) => match system {
                Some(s) => self.core(s),
                None => Err("unknown system: set a core for it in its RetroArch playlist".into()),
            },
        };
        let ra_launch = |core: &(String, PathBuf)| -> Option<Vec<String>> {
            let mut argv = self.prefix.clone()?;
            argv.push("-L".into());
            argv.push(core.1.to_string_lossy().to_string());
            argv.push(content.to_string());
            Some(argv)
        };
        let emulator = system.and_then(|s| self.emulators.get(&s).cloned());
        let mut info = RomInfo {
            system,
            file: file.to_string(),
            size,
            playlist: playlist.to_string(),
            label: name.to_string(),
            ..Default::default()
        };
        let mut launch = Vec::new();
        if let Some((emu, path)) = emulator {
            launch = retroarch::emulator_argv(&path, file);
            info.player = emu;
            if let Some(argv) = core.as_ref().ok().and_then(ra_launch) {
                info.alt_launch = argv;
                info.alt_label = format!(
                    "RetroArch ({})",
                    core.as_ref().map(|c| c.0.as_str()).unwrap_or("")
                );
            }
        } else {
            match &core {
                Ok(c) => {
                    info.player = c.0.clone();
                    match ra_launch(c) {
                        Some(argv) => launch = argv,
                        None => {
                            info.hint = "RetroArch is not installed: install the retroarch package or the Flatpak org.libretro.RetroArch".into()
                        }
                    }
                }
                Err(hint) => info.hint = hint.clone(),
            }
        }
        if !exists {
            info.hint = "ROM file not found".into();
            launch.clear();
            info.alt_launch.clear();
        }
        let badge = system.map(|s| s.info().badge).unwrap_or("RetroArch");
        let detail = if !info.hint.is_empty() {
            format!(
                "{badge} · {}",
                if exists { "no core" } else { "file not found" }
            )
        } else {
            format!("{badge} · {}", info.player)
        };
        Game {
            key: format!("rom:{content}"),
            kind: Kind::RetroArch,
            name: name.to_string(),
            detail,
            launch,
            rom: Some(Box::new(info)),
        }
    }
}

/// Playlist entries and scanned ROMs as games, each file once. A playlist
/// entry keeps its label, playlist and core; the scan adds the rest.
pub fn retro_games(entries: &[Entry], scanned: &[Rom], ctx: &RetroContext) -> Vec<Game> {
    let by_path: HashMap<&str, usize> = scanned
        .iter()
        .enumerate()
        .map(|(i, r)| (r.path.as_str(), i))
        .collect();
    let mut used = vec![false; scanned.len()];
    let mut keys = HashSet::new();
    // Thousands of entries share a few playlist and database names.
    let mut named: HashMap<String, Option<System>> = HashMap::new();
    let mut by_name = |n: &str| {
        if let Some(s) = named.get(n) {
            return *s;
        }
        let s = systems::system_for_db(n);
        named.insert(n.to_string(), s);
        s
    };
    let mut games = Vec::with_capacity(entries.len() + scanned.len());
    for e in entries {
        let file = retroarch::content_file(&e.path);
        let hit = by_path.get(file).copied().or_else(|| {
            let c = fs::canonicalize(file).ok()?;
            by_path.get(c.to_str()?).copied()
        });
        let rom = hit.map(|i| {
            used[i] = true;
            &scanned[i]
        });
        let system = by_name(&e.db_name)
            .or_else(|| by_name(&e.playlist))
            .or_else(|| rom.map(|r| r.system))
            .or_else(|| {
                let p = Path::new(file);
                let parent = p
                    .parent()
                    .and_then(|d| d.file_name())
                    .map(|n| n.to_string_lossy().to_string());
                systems::detect(p, &[parent.as_deref().unwrap_or("")])
            });
        let (size, exists) = match rom {
            Some(r) => (r.size, true),
            None => match fs::metadata(file) {
                Ok(m) => (m.len(), true),
                Err(_) => (0, false),
            },
        };
        let entry_core = (!e.core_path.is_empty() && e.core_path != "DETECT")
            .then(|| PathBuf::from(&e.core_path))
            .filter(|p| p.is_file())
            .map(|p| {
                let name = p
                    .file_name()
                    .map(|n| {
                        n.to_string_lossy()
                            .trim_end_matches("_libretro.so")
                            .to_string()
                    })
                    .unwrap_or_default();
                (name, p)
            });
        let game = ctx.game(
            &e.path,
            file,
            &e.label,
            system,
            size,
            &e.playlist,
            entry_core,
            exists,
        );
        if keys.insert(game.key.clone()) {
            games.push(game);
        }
    }
    for (i, r) in scanned.iter().enumerate() {
        if used[i] {
            continue;
        }
        let game = ctx.game(
            &r.path,
            &r.path,
            &r.name,
            Some(r.system),
            r.size,
            r.system.info().db,
            None,
            true,
        );
        if keys.insert(game.key.clone()) {
            games.push(game);
        }
    }
    games.sort_by_cached_key(|g| g.name.to_lowercase());
    games
}

struct Retro {
    games: Vec<Game>,
    errors: Vec<String>,
    roots: Vec<PathBuf>,
    thumbnail_dirs: Vec<PathBuf>,
    source: Source,
}

fn discover_retroarch(
    cfg: &Config,
    force: bool,
    stale: &mut dyn FnMut(Vec<Game>, &RetroContext),
) -> Retro {
    let installs = retroarch::installs();
    let mut errors = Vec::new();
    let mut entries = Vec::new();
    for path in retroarch::playlists(&installs) {
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| retroarch::parse_playlist(&t, &name))
        {
            Ok(mut e) => entries.append(&mut e),
            Err(e) => errors.push(format!("RetroArch: playlist {name}: {e}")),
        }
    }
    let files: Vec<String> = entries
        .iter()
        .map(|e| retroarch::content_file(&e.path).to_string())
        .collect();
    let rom_dirs: Vec<PathBuf> = installs.iter().filter_map(Install::rom_dir).collect();
    let roots = roms::roots(cfg, &rom_dirs, &files);
    let ctx = RetroContext::new(cfg, &installs);
    let (index, source) = roms::index(&roots, cfg.roms.depth, force, &mut |old| {
        stale(retro_games(&entries, &old.roms, &ctx), &ctx)
    });
    Retro {
        games: retro_games(&entries, &index.roms, &ctx),
        errors,
        roots,
        thumbnail_dirs: ctx.thumbnail_dirs.clone(),
        source,
    }
}

/// Per system: the core or emulator that starts it, or what to install.
pub fn players(cfg: &Config) -> Vec<(System, Result<String, String>)> {
    let installs = retroarch::installs();
    let ctx = RetroContext::new(cfg, &installs);
    systems::ALL
        .iter()
        .map(|s| {
            let r = match retroarch::emulator_for(*s, cfg) {
                Some((name, path)) => Ok(format!("{name} ({})", path.display())),
                None => ctx.core(*s).map(|(n, p)| format!("{n} ({})", p.display())),
            };
            (*s, r)
        })
        .collect()
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        home().join(".local/share/applications"),
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        home().join(".local/share/flatpak/exports/share/applications"),
    ];
    dirs.retain(|dir| dir.is_dir());
    dirs
}

fn desktop_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().to_string())
    })
}

fn desktop_game(path: &Path) -> Option<Game> {
    let text = fs::read_to_string(path).ok()?;
    // The launcher's own desktop entry is a Game entry too. Never list
    // NIRUGAME as a game, and tolerate a user copy with the same Exec.
    if path.file_name().and_then(|name| name.to_str()) == Some("nirugame.desktop") {
        return None;
    }
    let exec = desktop_value(&text, "Exec").unwrap_or_default();
    if exec.split_whitespace().next() == Some("nirugame") {
        return None;
    }
    if desktop_value(&text, "Type").as_deref() != Some("Application")
        || desktop_value(&text, "NoDisplay").as_deref() == Some("true")
    {
        return None;
    }
    let categories = desktop_value(&text, "Categories").unwrap_or_default();
    if !categories.split(';').any(|category| category == "Game") {
        return None;
    }
    let name = desktop_value(&text, "Name")?;
    let comment = desktop_value(&text, "Comment").unwrap_or_default();
    let launch = if command_exists("gio") {
        vec![
            "gio".into(),
            "launch".into(),
            path.to_string_lossy().to_string(),
        ]
    } else if command_exists("gtk-launch") {
        vec![
            "gtk-launch".into(),
            path.file_name()?.to_string_lossy().to_string(),
        ]
    } else {
        Vec::new()
    };
    Some(Game {
        key: format!("native:{}", path.display()),
        kind: Kind::Native,
        name,
        detail: if comment.is_empty() {
            format!("Native · {categories}")
        } else {
            format!("Native · {comment}")
        },
        launch,
        rom: None,
    })
}

fn discover_native() -> Result<Vec<Game>, String> {
    let mut games = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in desktop_dirs() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            if let Some(game) = desktop_game(&path) {
                if seen.insert(game.key.clone()) {
                    games.push(game);
                }
            }
        }
    }
    games.sort_by_key(|game| game.name.to_lowercase());
    Ok(games)
}

pub fn setup_plan(target: &str) -> Result<String, String> {
    let platform = if Path::new("/etc/arch-release").exists() {
        "arch"
    } else if Path::new("/etc/void-release").exists() || Path::new("/etc/xbps.d").exists() {
        "void"
    } else if Path::new("/etc/debian_version").exists() {
        "debian"
    } else {
        "unknown"
    };
    if !matches!(target, "steam" | "retroarch" | "all") {
        return Err("setup-plan takes steam, retroarch or all".into());
    }
    let mut text = format!("NIRUGAME setup plan · {platform}\n\n");
    if matches!(target, "steam" | "all") {
        text.push_str("Steam:\n");
        match platform {
            "debian" => text.push_str("  sudo dpkg --add-architecture i386\n  sudo apt update\n  sudo apt install steam-installer gamemode mangohud vulkan-tools\n  # This requires the Debian non-free repository and a supported GPU driver.\n"),
            "arch" => text.push_str("  sudo pacman -S --needed steam steam-devices gamemode lib32-gamemode mangohud lib32-mangohud gamescope vulkan-tools\n  # This requires the multilib repository and matching 32-bit GPU drivers.\n"),
            "void" => text.push_str("  sudo xbps-install -S steam gamemode mangohud vulkan-tools\n  # Verify the nonfree repository and 32-bit packages with xbps-query -Rs.\n"),
            _ => text.push_str("  Unsupported distribution; review packages manually.\n"),
        }
        text.push_str(
            "  Review the package list before running it; nothing is installed by this plan.\n\n",
        );
    }
    if matches!(target, "retroarch" | "all") {
        text.push_str("RetroArch:\n");
        match platform {
            "debian" => text.push_str("  sudo apt install retroarch retroarch-assets libretro-core-info\n  # Add only the libretro-* cores you need.\n"),
            "arch" => text.push_str("  sudo pacman -S --needed retroarch retroarch-assets libretro-core-info\n  # Add only the libretro-* cores you need.\n"),
            "void" => text.push_str("  sudo xbps-install -S retroarch retroarch-assets\n  # Verify available libretro-* cores in the configured repositories.\n"),
            _ => text.push_str("  Unsupported distribution; review packages manually.\n"),
        }
        text.push_str(
            "  ROMs and BIOS files are not installed or bundled. Use only content you own.\n\n",
        );
    }
    text.push_str("NIRUGAME never runs this plan automatically.\n");
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_pairs_handle_escaped_characters() {
        let pairs = pair_map("\"name\" \"A \\\"Game\\\"\"\n\"appid\" \"42\"");
        assert_eq!(pairs.get("appid").map(String::as_str), Some("42"));
        assert_eq!(pairs.get("name").map(String::as_str), Some("A \"Game\""));
    }

    #[test]
    fn steam_manifest_parser_rejects_non_appid() {
        assert!(steam_manifest(Path::new("/dev/null")).is_none());
    }

    #[test]
    fn steam_manifest_reads_nested_vdf_values() {
        let dir = std::env::temp_dir().join(format!("nirugame-acf-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("appmanifest_730.acf");
        fs::write(
            &path,
            "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"installdir\"\t\t\"Counter-Strike Global Offensive\"\n\t\"InstalledDepots\"\n\t{\n\t\t\"731\"\t\t{ \"manifest\" \"1\" }\n\t}\n}\n",
        )
        .unwrap();
        let game = steam_manifest(&path).unwrap();
        assert_eq!(game.name, "Counter-Strike 2");
        assert_eq!(game.key, "steam:730");
        let _ = fs::remove_dir_all(dir);
    }

    fn ctx(cores: &[(&str, &str)], cfg: Config, retroarch: bool) -> RetroContext {
        let cores = Cores {
            by_name: cores
                .iter()
                .map(|(n, p)| (n.to_string(), PathBuf::from(p)))
                .collect(),
        };
        let resolved = systems::ALL
            .iter()
            .map(|s| (*s, retroarch::core_for(*s, &cores, &cfg)))
            .collect();
        RetroContext {
            emulators: emulators(&cfg),
            cfg,
            prefix: retroarch.then(|| vec!["retroarch".to_string()]),
            cores,
            resolved,
            thumbnail_dirs: Vec::new(),
        }
    }

    fn rom(path: &str, system: System) -> Rom {
        Rom {
            path: path.into(),
            system,
            name: Path::new(path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            size: 1024,
        }
    }

    #[test]
    fn playlists_and_scan_merge_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("nirugame-merge-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let zelda = dir.join("Zelda.sfc");
        fs::write(&zelda, b"").unwrap();
        let zelda = zelda.to_string_lossy().to_string();
        let entries = vec![
            Entry {
                path: zelda.clone(),
                label: "The Legend of Zelda (USA)".into(),
                core_path: "DETECT".into(),
                core_name: "DETECT".into(),
                playlist: "Nintendo - Super Nintendo Entertainment System".into(),
                db_name: String::new(),
            },
            Entry {
                path: "/gone/Old.nes".into(),
                label: "Old".into(),
                core_path: String::new(),
                core_name: String::new(),
                playlist: "NES".into(),
                db_name: String::new(),
            },
        ];
        let scanned = vec![
            rom(&zelda, System::Snes),
            rom("/r/nes/Mario.nes", System::Nes),
            rom("/r/n64/Kart.z64", System::N64),
        ];
        let c = ctx(
            &[
                ("snes9x", "/c/snes9x_libretro.so"),
                ("mesen", "/c/mesen_libretro.so"),
            ],
            Config::default(),
            true,
        );
        let games = retro_games(&entries, &scanned, &c);
        assert_eq!(games.len(), 4, "{games:#?}");
        let by = |name: &str| games.iter().find(|g| g.name == name).unwrap();
        // The playlist label wins and the file is listed once.
        let z = by("The Legend of Zelda (USA)");
        assert_eq!(
            z.launch,
            vec!["retroarch", "-L", "/c/snes9x_libretro.so", zelda.as_str()]
        );
        assert_eq!(
            z.rom.as_ref().unwrap().playlist,
            "Nintendo - Super Nintendo Entertainment System"
        );
        assert!(games.iter().all(|g| g.name != "Zelda"));
        let m = by("Mario");
        assert_eq!(m.launch[2], "/c/mesen_libretro.so");
        assert_eq!(m.rom.as_ref().unwrap().playlist, System::Nes.info().db);
        // No core: listed, cannot start, says what to install.
        let k = by("Kart");
        assert!(k.launch.is_empty());
        assert!(k.rom.as_ref().unwrap().hint.contains("Mupen64Plus"));
        assert_eq!(k.detail, "N64 · no core");
        // A playlist entry whose file is gone.
        let o = by("Old");
        assert!(o.launch.is_empty());
        assert_eq!(o.rom.as_ref().unwrap().hint, "ROM file not found");
        // No RetroArch at all: a hint as well.
        let none = ctx(
            &[("mesen", "/c/mesen_libretro.so")],
            Config::default(),
            false,
        );
        let g = retro_games(&[], &scanned[1..2], &none);
        assert!(
            g[0].launch.is_empty()
                && g[0]
                    .rom
                    .as_ref()
                    .unwrap()
                    .hint
                    .contains("RetroArch is not installed")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn configured_core_and_emulator_win() {
        let mut cfg = Config::default();
        cfg.cores.insert("snes".into(), "bsnes".into());
        cfg.emulators.insert("c64".into(), "/bin/sh".into());
        let c = ctx(
            &[
                ("snes9x", "/c/snes9x_libretro.so"),
                ("bsnes", "/c/bsnes_libretro.so"),
                ("vice_x64sc", "/c/vice_x64sc_libretro.so"),
            ],
            cfg,
            true,
        );
        let entries = vec![Entry {
            path: "/r/snes/A.sfc".into(),
            label: "A".into(),
            core_path: String::new(),
            core_name: String::new(),
            playlist: "SNES".into(),
            db_name: String::new(),
        }];
        let scanned = vec![
            rom("/r/snes/A.sfc", System::Snes),
            rom("/r/c64/Elite.d64", System::C64),
        ];
        let games = retro_games(&entries, &scanned, &c);
        let a = games.iter().find(|g| g.name == "A").unwrap();
        assert_eq!(a.launch[2], "/c/bsnes_libretro.so");
        let e = games.iter().find(|g| g.name == "Elite").unwrap();
        assert_eq!(e.launch, vec!["/bin/sh", "/r/c64/Elite.d64"]);
        let rom = e.rom.as_ref().unwrap();
        assert_eq!(rom.player, "sh");
        assert_eq!(
            rom.alt_launch,
            vec![
                "retroarch",
                "-L",
                "/c/vice_x64sc_libretro.so",
                "/r/c64/Elite.d64"
            ]
        );
        assert_eq!(rom.alt_label, "RetroArch (vice_x64sc)");
    }

    #[test]
    fn desktop_launcher_is_not_a_game() {
        let dir = std::env::temp_dir().join(format!("nirugame-self-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("nirugame.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=NIRUGAME\nCategories=Game;\nExec=nirugame\n",
        )
        .unwrap();
        assert!(desktop_game(&path).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn desktop_game_requires_game_category() {
        let dir = std::env::temp_dir().join(format!("nirugame-desktop-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("game.desktop");
        fs::write(&path, "[Desktop Entry]\nType=Application\nName=Test Game\nCategories=Game;ActionGame;\nExec=test-game\n").unwrap();
        let game = desktop_game(&path).unwrap();
        assert_eq!(game.name, "Test Game");
        let path = dir.join("notgame.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Editor\nCategories=Utility;\nExec=editor\n",
        )
        .unwrap();
        assert!(desktop_game(&path).is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
