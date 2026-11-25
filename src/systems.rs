//! Game systems: which files belong to which console or computer, the
//! RetroArch name of each system (playlists and thumbnails) and the libretro
//! cores that play it. Detection is by extension and by folder name, so an
//! archive (.zip, .7z) or a disc image gets its system from the folder it
//! is kept in.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum System {
    Nes,
    Snes,
    N64,
    Gb,
    Gbc,
    Gba,
    Nds,
    MegaDrive,
    MasterSystem,
    GameGear,
    Psx,
    C64,
    Amiga,
    Atari2600,
    PcEngine,
    Arcade,
}

pub struct Info {
    /// Config and command-line name: `[cores] snes = "snes9x"`.
    pub id: &'static str,
    /// Short label for badges and tabs.
    pub badge: &'static str,
    pub name: &'static str,
    /// RetroArch database name: playlist and thumbnail folder.
    pub db: &'static str,
    /// Extensions that belong to this system alone.
    pub exts: &'static [&'static str],
    /// Folder names (normalised: lower case, letters and digits only).
    pub folders: &'static [&'static str],
    /// libretro cores in order of preference (file name without `_libretro.so`).
    pub cores: &'static [&'static str],
    /// The cores to install when none is found.
    pub install: &'static str,
}

pub const ALL: [System; 16] = [
    System::Nes,
    System::Snes,
    System::N64,
    System::Gb,
    System::Gbc,
    System::Gba,
    System::Nds,
    System::MegaDrive,
    System::MasterSystem,
    System::GameGear,
    System::Psx,
    System::C64,
    System::Amiga,
    System::Atari2600,
    System::PcEngine,
    System::Arcade,
];

/// Archive and disc formats that several systems use: the folder decides.
const SHARED: &[&str] = &[
    "zip", "7z", "bin", "iso", "cue", "chd", "img", "m3u", "ccd", "mds", "ecm", "rom",
];

impl System {
    pub fn info(self) -> &'static Info {
        use System::*;
        match self {
            Nes => &Info {
                id: "nes",
                badge: "NES",
                name: "Nintendo Entertainment System",
                db: "Nintendo - Nintendo Entertainment System",
                exts: &["nes", "unf", "unif", "fds"],
                folders: &[
                    "nes",
                    "famicom",
                    "fc",
                    "nintendoentertainmentsystem",
                    "nesroms",
                ],
                cores: &["mesen", "nestopia", "fceumm", "quicknes"],
                install: "Mesen, Nestopia or FCEUmm",
            },
            Snes => &Info {
                id: "snes",
                badge: "SNES",
                name: "Super Nintendo",
                db: "Nintendo - Super Nintendo Entertainment System",
                exts: &["sfc", "smc", "swc", "fig", "bs"],
                folders: &["snes", "sfc", "superfamicom", "supernintendo", "snesroms"],
                cores: &[
                    "snes9x",
                    "bsnes",
                    "bsnes_mercury_accuracy",
                    "bsnes_mercury_balanced",
                    "bsnes_mercury_performance",
                    "bsnes_hd_beta",
                    "mesen-s",
                    "snes9x2010",
                ],
                install: "Snes9x or bsnes",
            },
            N64 => &Info {
                id: "n64",
                badge: "N64",
                name: "Nintendo 64",
                db: "Nintendo - Nintendo 64",
                exts: &["n64", "z64", "v64"],
                folders: &["n64", "nintendo64"],
                cores: &["mupen64plus_next", "parallel_n64", "mupen64plus"],
                install: "Mupen64Plus-Next",
            },
            Gb => &Info {
                id: "gb",
                badge: "GB",
                name: "Game Boy",
                db: "Nintendo - Game Boy",
                exts: &["gb", "dmg"],
                folders: &["gb", "gameboy"],
                cores: &["gambatte", "sameboy", "gearboy", "mgba"],
                install: "Gambatte or SameBoy",
            },
            Gbc => &Info {
                id: "gbc",
                badge: "GBC",
                name: "Game Boy Color",
                db: "Nintendo - Game Boy Color",
                exts: &["gbc", "cgb"],
                folders: &["gbc", "gameboycolor", "gameboycolour"],
                cores: &["gambatte", "sameboy", "gearboy", "mgba"],
                install: "Gambatte or SameBoy",
            },
            Gba => &Info {
                id: "gba",
                badge: "GBA",
                name: "Game Boy Advance",
                db: "Nintendo - Game Boy Advance",
                exts: &["gba", "agb"],
                folders: &["gba", "gameboyadvance"],
                cores: &["mgba", "vbam", "vba_next", "gpsp"],
                install: "mGBA",
            },
            Nds => &Info {
                id: "nds",
                badge: "NDS",
                name: "Nintendo DS",
                db: "Nintendo - Nintendo DS",
                exts: &["nds"],
                folders: &["nds", "ds", "nintendods"],
                cores: &["melonds", "melondsds", "desmume"],
                install: "melonDS or DeSmuME",
            },
            MegaDrive => &Info {
                id: "megadrive",
                badge: "MD",
                name: "Mega Drive / Genesis",
                db: "Sega - Mega Drive - Genesis",
                exts: &["md", "gen", "smd", "68k", "sgd"],
                folders: &[
                    "megadrive",
                    "genesis",
                    "md",
                    "segamegadrive",
                    "segagenesis",
                    "megadrivegenesis",
                ],
                cores: &["genesis_plus_gx", "picodrive", "blastem"],
                install: "Genesis Plus GX or PicoDrive",
            },
            MasterSystem => &Info {
                id: "mastersystem",
                badge: "SMS",
                name: "Master System",
                db: "Sega - Master System - Mark III",
                exts: &["sms"],
                folders: &["mastersystem", "sms", "segamastersystem"],
                cores: &["genesis_plus_gx", "picodrive", "gearsystem", "smsplus"],
                install: "Genesis Plus GX",
            },
            GameGear => &Info {
                id: "gamegear",
                badge: "GG",
                name: "Game Gear",
                db: "Sega - Game Gear",
                exts: &["gg"],
                folders: &["gamegear", "gg", "segagamegear"],
                cores: &["genesis_plus_gx", "gearsystem", "picodrive"],
                install: "Genesis Plus GX",
            },
            Psx => &Info {
                id: "psx",
                badge: "PS1",
                name: "PlayStation",
                db: "Sony - PlayStation",
                exts: &["pbp"],
                folders: &[
                    "psx",
                    "ps1",
                    "playstation",
                    "playstation1",
                    "sonyplaystation",
                    "ps1eboot",
                    "psxeboot",
                ],
                cores: &[
                    "swanstation",
                    "pcsx_rearmed",
                    "mednafen_psx_hw",
                    "mednafen_psx",
                    "duckstation",
                ],
                install: "SwanStation or PCSX ReARMed, plus a PS1 BIOS",
            },
            C64 => &Info {
                id: "c64",
                badge: "C64",
                name: "Commodore 64",
                db: "Commodore - 64",
                exts: &[
                    "d64", "t64", "prg", "crt", "tap", "g64", "d71", "d81", "x64", "p00",
                ],
                folders: &["c64", "commodore64", "commodore", "vice"],
                cores: &["vice_x64sc", "vice_x64", "frodo"],
                install: "VICE x64sc, or VICE itself as [emulators] c64",
            },
            Amiga => &Info {
                id: "amiga",
                badge: "AMIGA",
                name: "Amiga",
                db: "Commodore - Amiga",
                exts: &["adf", "adz", "ipf", "hdf", "lha", "dms"],
                folders: &["amiga", "commodoreamiga", "whdload"],
                cores: &["puae", "puae2021", "uae4arm"],
                install: "PUAE, plus Kickstart ROMs",
            },
            Atari2600 => &Info {
                id: "atari2600",
                badge: "2600",
                name: "Atari 2600",
                db: "Atari - 2600",
                exts: &["a26"],
                folders: &["atari2600", "a2600", "2600", "vcs"],
                cores: &["stella", "stella2014"],
                install: "Stella",
            },
            PcEngine => &Info {
                id: "pcengine",
                badge: "PCE",
                name: "PC Engine / TurboGrafx-16",
                db: "NEC - PC Engine - TurboGrafx 16",
                exts: &["pce", "sgx"],
                folders: &["pcengine", "pce", "tg16", "turbografx16", "turbografx"],
                cores: &["mednafen_pce_fast", "mednafen_pce", "mednafen_supergrafx"],
                install: "Beetle PCE Fast",
            },
            Arcade => &Info {
                id: "arcade",
                badge: "ARC",
                name: "Arcade",
                db: "FBNeo - Arcade Games",
                exts: &[],
                folders: &[
                    "arcade",
                    "mame",
                    "fbneo",
                    "fba",
                    "finalburn",
                    "neogeo",
                    "cps1",
                    "cps2",
                ],
                cores: &["fbneo", "mame2003_plus", "mame", "mame2010"],
                install: "FinalBurn Neo or MAME",
            },
        }
    }

    pub fn id(self) -> &'static str {
        self.info().id
    }

    pub fn from_id(id: &str) -> Option<Self> {
        let id = id.trim().to_ascii_lowercase();
        ALL.iter().copied().find(|s| s.id() == id)
    }

    /// Every system has a fixed place; badges and tabs keep this order.
    pub fn index(self) -> usize {
        ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Lower case, letters and digits only: "Nes-Homebrew" -> "neshomebrew".
pub fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The normalised database names, computed once.
fn db_names() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| ALL.iter().map(|s| normalise(s.info().db)).collect())
}

/// The system a folder name stands for: the whole name ("Game Boy Advance",
/// "Sega - Mega Drive - Genesis") or one of its words ("nes-homebrew",
/// "roms_snes", "PS1-EBOOT").
pub fn system_for_folder(name: &str) -> Option<System> {
    let whole = normalise(name);
    if whole.is_empty() {
        return None;
    }
    for (i, s) in ALL.iter().enumerate() {
        if s.info().folders.contains(&whole.as_str()) || db_names()[i] == whole {
            return Some(*s);
        }
    }
    for word in name.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.len() < 2 {
            continue;
        }
        for s in ALL {
            if s.info().folders.contains(&word.as_str()) {
                return Some(s);
            }
        }
    }
    None
}

/// The system a RetroArch playlist or database name stands for.
pub fn system_for_db(name: &str) -> Option<System> {
    let name = name.trim_end_matches(".lpl").trim_end_matches(".rdb");
    if name.is_empty() {
        return None;
    }
    let n = normalise(name);
    db_names()
        .iter()
        .position(|d| *d == n)
        .map(|i| ALL[i])
        .or_else(|| system_for_folder(name))
}

pub fn system_for_extension(ext: &str) -> Option<System> {
    let ext = ext.to_ascii_lowercase();
    ALL.iter()
        .copied()
        .find(|s| s.info().exts.contains(&ext.as_str()))
}

/// Shared archive or disc format: needs a folder (or a look inside) to know.
pub fn is_shared_extension(ext: &str) -> bool {
    SHARED.contains(&ext.to_ascii_lowercase().as_str())
}

/// Is this a file NIRUGAME can list: a system extension or a shared one.
pub fn is_candidate(ext: &str) -> bool {
    system_for_extension(ext).is_some() || is_shared_extension(ext)
}

/// The system of `path`. `folders` are the names of the folders it is kept
/// in, nearest first. A folder that names a system wins when the extension
/// fits it or is a shared format; otherwise a system extension decides.
pub fn detect(path: &Path, folders: &[&str]) -> Option<System> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let by_ext = system_for_extension(&ext);
    let shared = is_shared_extension(&ext);
    for folder in folders {
        if let Some(s) = system_for_folder(folder) {
            if shared || by_ext == Some(s) {
                return Some(s);
            }
            // "gb" folder with .gbc files and the like: the extension knows better.
            if by_ext.is_some() {
                break;
            }
        }
    }
    by_ext
}

/// RetroArch's thumbnail file name for a playlist label: the characters
/// `&*/:\`<>?\|"` are replaced with `_`.
pub fn thumbnail_name(label: &str) -> String {
    label
        .chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(path: &str, folders: &[&str]) -> Option<&'static str> {
        detect(Path::new(path), folders).map(System::id)
    }

    #[test]
    fn extensions_decide_case_insensitively() {
        assert_eq!(det("Super Mario World (USA).SFC", &[]), Some("snes"));
        assert_eq!(det("Zelda.smc", &["roms"]), Some("snes"));
        assert_eq!(det("Mario.nes", &["roms", "Games"]), Some("nes"));
        assert_eq!(det("Boulder Dash.d64", &[]), Some("c64"));
        assert_eq!(det("Elite.T64", &[]), Some("c64"));
        assert_eq!(det("Mario Kart.z64", &[]), Some("n64"));
        assert_eq!(det("Metroid.gba", &[]), Some("gba"));
        assert_eq!(det("Crash.pbp", &["PS1-EBOOT"]), Some("psx"));
        assert_eq!(det("readme.txt", &["snes"]), None);
        assert_eq!(det("noext", &["snes"]), None);
    }

    #[test]
    fn archives_and_discs_take_the_folder() {
        assert_eq!(det("Sonic.zip", &["megadrive"]), Some("megadrive"));
        assert_eq!(det("Sonic.zip", &["Genesis"]), Some("megadrive"));
        assert_eq!(det("Crash.7z", &["PS1"]), Some("psx"));
        assert_eq!(det("Disc.cue", &["Sony - PlayStation"]), Some("psx"));
        assert_eq!(
            det("Game.zip", &["usa", "nes-homebrew", "roms"]),
            Some("nes")
        );
        assert_eq!(det("Game.ZIP", &["roms"]), None);
        assert_eq!(det("Game.bin", &[]), None);
        assert_eq!(det("Turrican.adf", &["whatever"]), Some("amiga"));
    }

    #[test]
    fn a_folder_never_overrides_a_different_system_extension() {
        assert_eq!(det("Pokemon.gbc", &["gb"]), Some("gbc"));
        assert_eq!(det("Mario.nes", &["snes"]), Some("nes"));
    }

    #[test]
    fn folder_names_map_to_systems() {
        let f = |n| system_for_folder(n).map(System::id);
        assert_eq!(f("NES"), Some("nes"));
        assert_eq!(f("nes-homebrew"), Some("nes"));
        assert_eq!(f("SNES"), Some("snes"));
        assert_eq!(f("Game Boy Advance"), Some("gba"));
        assert_eq!(
            f("Nintendo - Super Nintendo Entertainment System"),
            Some("snes")
        );
        assert_eq!(f("Sega - Mega Drive - Genesis"), Some("megadrive"));
        assert_eq!(f("psx"), Some("psx"));
        assert_eq!(f("PS1-EBOOT"), Some("psx"));
        assert_eq!(f("c64"), Some("c64"));
        assert_eq!(f("Amiga"), Some("amiga"));
        assert_eq!(f("roms"), None);
        assert_eq!(f("Games"), None);
        assert_eq!(f("Nintendo"), None);
        assert_eq!(f("bios"), None);
    }

    #[test]
    fn playlist_names_map_to_systems() {
        let f = |n| system_for_db(n).map(System::id);
        assert_eq!(
            f("Nintendo - Nintendo Entertainment System.lpl"),
            Some("nes")
        );
        assert_eq!(f("Commodore - 64"), Some("c64"));
        assert_eq!(f("C64"), Some("c64"));
        assert_eq!(f("SNES"), Some("snes"));
        assert_eq!(f("Favourites"), None);
    }

    #[test]
    fn ids_round_trip() {
        for s in ALL {
            assert_eq!(System::from_id(s.id()), Some(s));
            assert!(!s.info().cores.is_empty());
        }
        assert_eq!(System::from_id("SNES"), Some(System::Snes));
    }

    #[test]
    fn thumbnail_names_follow_retroarch() {
        assert_eq!(
            thumbnail_name("Legend of Zelda, The - A Link to the Past (USA)"),
            "Legend of Zelda, The - A Link to the Past (USA)"
        );
        assert_eq!(thumbnail_name("Tom & Jerry"), "Tom _ Jerry");
        assert_eq!(thumbnail_name("Q*bert"), "Q_bert");
        assert_eq!(
            thumbnail_name("AC/DC: Live? <1> |x| `y` \\z\""),
            "AC_DC_ Live_ _1_ _x_ _y_ _z_"
        );
    }
}
