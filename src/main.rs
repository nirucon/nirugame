mod app;
mod theme;

use app::NirugameApp;
use nirugame::{games, music};

const HELP: &str = "NIRUGAME — retro game launcher for NIRUX

Usage:
  nirugame [--open all|steam|retroarch|native|music]
  nirugame --list
  nirugame --setup-plan steam|retroarch|all
  nirugame --music-status
  nirugame --version

NIRUGAME reads local Steam manifests, RetroArch playlists and native game
desktop entries. It never bundles ROMs, game files or SID music.";

fn print_catalog() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = games::Catalog::discover();
    println!("{}", serde_json::to_string_pretty(&catalog)?);
    Ok(())
}

fn print_music() -> Result<(), Box<dyn std::error::Error>> {
    let tracks = music::discover();
    let config = music::load_config();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "directory": music::music_dir(),
            "selected": config.music,
            "tracks": tracks.iter().map(|track| serde_json::json!({
                "title": track.title,
                "path": track.path,
            })).collect::<Vec<_>>(),
        }))?
    );
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
        if let Err(error) = print_catalog() {
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
            _ => app::View::All,
        });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NIRUGAME")
            .with_app_id("nirugame")
            .with_inner_size(egui::vec2(1020.0, 720.0))
            .with_min_inner_size(egui::vec2(360.0, 360.0))
            .with_resizable(true),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "NIRUGAME",
        options,
        Box::new(move |cc| Ok(Box::new(NirugameApp::new(cc, initial)))),
    ) {
        eprintln!("nirugame: display/window initialization failed: {error}");
        std::process::exit(1);
    }
}
