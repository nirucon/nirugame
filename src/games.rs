//! Local game discovery. Nothing here downloads metadata or starts a daemon.
//! Steam is read from its appmanifest files, RetroArch from its playlists and
//! native games from desktop entries. Launch commands are argv arrays, never
//! shell strings.

use serde::Serialize;
use std::{
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

#[derive(Clone, Debug, Serialize)]
pub struct Game {
    pub key: String,
    pub kind: Kind,
    pub name: String,
    pub detail: String,
    pub launch: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Catalog {
    pub steam: Vec<Game>,
    pub retroarch: Vec<Game>,
    pub native: Vec<Game>,
    pub errors: Vec<String>,
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
        let mut catalog = Self::default();
        match discover_steam() {
            Ok(games) => catalog.steam = games,
            Err(error) => catalog.errors.push(format!("Steam: {error}")),
        }
        match discover_retroarch() {
            Ok(games) => catalog.retroarch = games,
            Err(error) => catalog.errors.push(format!("RetroArch: {error}")),
        }
        match discover_native() {
            Ok(games) => catalog.native = games,
            Err(error) => catalog.errors.push(format!("Native: {error}")),
        }
        catalog
    }
}

pub fn launch(game: &Game) -> Result<(), String> {
    let Some(program) = game.launch.first() else {
        return Err(format!("{} has no launch command", game.name));
    };
    let mut command = Command::new(program);
    command
        .args(&game.launch[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::process::detach_terminal(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", game.name))?;
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

fn retroarch_prefix() -> Option<Vec<String>> {
    if let Some(path) = command_in_path("retroarch") {
        return Some(vec![path]);
    }
    command_exists("flatpak").then(|| {
        vec![
            "flatpak".into(),
            "run".into(),
            "org.libretro.RetroArch".into(),
        ]
    })
}

fn retroarch_playlists() -> Vec<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    let mut paths = Vec::new();
    for dir in [
        base.join("retroarch/playlists"),
        home().join(".var/app/org.libretro.RetroArch/config/retroarch/playlists"),
    ] {
        if let Ok(entries) = fs::read_dir(dir) {
            paths.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext == "lpl")),
            );
        }
    }
    paths.sort();
    paths
}

fn retroarch_playlist(path: &Path) -> Result<Vec<Game>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    let playlist = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("RetroArch")
        .to_string();
    let prefix = retroarch_prefix();
    let mut games = Vec::new();
    for item in items {
        let Some(content) = item.get("path").and_then(|value| value.as_str()) else {
            continue;
        };
        if content.trim().is_empty() {
            continue;
        }
        let label = item
            .get("label")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                Path::new(content)
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Unknown game".into());
        let core = item
            .get("core_path")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let mut launch = prefix.clone().unwrap_or_default();
        if !core.is_empty() && core != "DETECT" {
            launch.push("-L".into());
            launch.push(core.into());
        }
        launch.push(content.into());
        let core_name = item
            .get("core_name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        games.push(Game {
            key: format!("retro:{}:{}", playlist, content),
            kind: Kind::RetroArch,
            name: label,
            detail: format!(
                "RetroArch · {} · {}",
                playlist,
                if core_name.is_empty() {
                    "detected core"
                } else {
                    core_name
                }
            ),
            launch,
        });
    }
    Ok(games)
}

fn discover_retroarch() -> Result<Vec<Game>, String> {
    let mut games = Vec::new();
    for path in retroarch_playlists() {
        if let Ok(mut discovered) = retroarch_playlist(&path) {
            games.append(&mut discovered);
        }
    }
    games.sort_by_key(|game| game.name.to_lowercase());
    Ok(games)
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
    fn retroarch_playlist_parser_reads_label_and_core() {
        let dir = std::env::temp_dir().join(format!("nirugame-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("Arcade.lpl");
        fs::write(&path, r#"[{"path":"/games/a.zip","label":"Alpha","core_path":"/cores/mame_libretro.so","core_name":"MAME"}]"#).unwrap();
        let games = retroarch_playlist(&path).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Alpha");
        assert!(games[0].detail.contains("MAME"));
        let _ = fs::remove_dir_all(dir);
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
