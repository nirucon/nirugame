mod app;
mod gui;

use app::NirugameApp;
use nirugame::{config, games, music, systems};

const HELP: &str = "NIRUGAME: retro game launcher for NIRUX

Usage:
  nirugame [--open all|favourites|recent|steam|retroarch|native|music|SYSTEM]
  nirugame --list [--rescan]
  nirugame --cores
  nirugame --setup-plan steam|retroarch|all
  nirugame --music-status
  nirugame --version

SYSTEM is a system id such as nes, snes, c64, gba, n64, psx or megadrive.

NIRUGAME reads local Steam manifests, RetroArch playlists, your ROM folders
and native game desktop entries. It never bundles ROMs, game files or SID
music.";

fn print_catalog(rescan: bool) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = games::Catalog::discover_with(&config::load(), rescan, &mut |_| {});
    println!("{}", serde_json::to_string_pretty(&catalog)?);
    Ok(())
}

fn print_music() -> Result<(), Box<dyn std::error::Error>> {
    let tracks = music::discover();
    let config = config::load();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "directory": music::music_dir(),
            "selected": config.music,
            "nirusid": music::nirusid(),
            "tracks": tracks.iter().map(|track| serde_json::json!({
                "title": track.title,
                "path": track.path,
            })).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}

fn print_cores() -> Result<(), Box<dyn std::error::Error>> {
    let list: Vec<serde_json::Value> = games::players(&config::load())
        .into_iter()
        .map(|(system, r)| {
            let info = system.info();
            match r {
                Ok(player) => {
                    serde_json::json!({"system": info.id, "name": info.name, "player": player})
                }
                Err(hint) => {
                    serde_json::json!({"system": info.id, "name": info.name, "missing": hint})
                }
            }
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&list)?);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("NIRUGAME {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return;
    }
    if args.iter().any(|arg| arg == "--list") {
        if let Err(error) = print_catalog(args.iter().any(|arg| arg == "--rescan")) {
            eprintln!("nirugame: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|arg| arg == "--cores") {
        if let Err(error) = print_cores() {
            eprintln!("nirugame: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|arg| arg == "--music-status") {
        if let Err(error) = print_music() {
            eprintln!("nirugame: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--setup-plan") {
        let target = args.get(index + 1).map(String::as_str).unwrap_or("all");
        match games::setup_plan(target) {
            Ok(plan) => println!("{plan}"),
            Err(error) => {
                eprintln!("nirugame: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    let initial = args
        .windows(2)
        .find(|window| window[0] == "--open")
        .map(|window| match window[1].as_str() {
            "steam" => app::View::Steam,
            "retroarch" => app::View::RetroArch,
            "native" => app::View::Native,
            "music" => app::View::Music,
            "favourites" | "favorites" => app::View::Favourites,
            "recent" => app::View::Recent,
            other => systems::System::from_id(other)
                .map(app::View::System)
                .unwrap_or(app::View::All),
        });
    let started = std::time::Instant::now();
    let mut app = NirugameApp::new(initial, gui::shell::waker(), started);
    let opts = gui::shell::Opts {
        title: "NIRUGAME",
        class: "nirugame",
        size: (1020, 720),
        min: (360, 360),
        icon: 'G',
        timing_env: "NIRUGAME_TIMING",
    };
    if let Err(error) = gui::shell::run(&mut app, opts, started) {
        eprintln!("nirugame: display/window initialization failed: {error}");
        std::process::exit(1);
    }
}
