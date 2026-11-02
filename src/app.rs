use crate::theme::{self, Palette, Theme, Watcher};
use egui::{Align, Button, Context, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit, Ui};
use nirugame::{
    games::{self, Catalog, Game},
    music::{self, Player, Track},
};
use std::time::Duration;

const LOGO: &str = "\
 _  _ ___ ___ _   _ ___   ___   __  __ ___
| \\| |_ _| _ \\ | | / __| / __| /  \\|  \\/  | __|
| .` || ||   / |_| \\__ \\| (_ | | () | |\\/| | _|
|_|\\_|___|_|_\\\\___/|___/ \\___| \\__/|_|  |_|___|";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    All,
    Steam,
    RetroArch,
    Native,
    Music,
}

impl View {
    fn title(self) -> &'static str {
        match self {
            Self::All => "All games",
            Self::Steam => "Steam",
            Self::RetroArch => "RetroArch",
            Self::Native => "Native",
            Self::Music => "Music",
        }
    }
}

pub struct NirugameApp {
    catalog: Catalog,
    tracks: Vec<Track>,
    player: Player,
    music_config: music::Config,
    view: View,
    theme: Theme,
    theme_watcher: Option<Watcher>,
    query: String,
    selected: usize,
    message: Option<(String, bool)>,
}

impl NirugameApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<View>) -> Self {
        let theme = theme::load();
        theme::apply(&cc.egui_ctx, &theme);
        let catalog = Catalog::discover();
        let tracks = music::discover();
        let music_config = music::load_config();
        let mut player = Player::default();
        let mut message = None;
        if !music_config.music.is_empty() {
            let path = std::path::Path::new(&music_config.music);
            if path.is_file() {
                if let Err(error) = player.play(path) {
                    message = Some((error, true));
                }
            } else {
                message = Some((
                    "Saved SID track is no longer present; music is Off".into(),
                    true,
                ));
            }
        }
        Self {
            catalog,
            tracks,
            player,
            music_config,
            view: initial.unwrap_or(View::All),
            theme,
            theme_watcher: Some(Watcher::start(cc.egui_ctx.clone())),
            query: String::new(),
            selected: 0,
            message,
        }
    }

    fn games(&self) -> Vec<&Game> {
        let mut games = match self.view {
            View::All => self.catalog.all(),
            View::Steam => self.catalog.steam.iter().collect(),
            View::RetroArch => self.catalog.retroarch.iter().collect(),
            View::Native => self.catalog.native.iter().collect(),
            View::Music => Vec::new(),
        };
        if !self.query.trim().is_empty() {
            let needle = self.query.trim().to_lowercase();
            games.retain(|game| {
                game.name.to_lowercase().contains(&needle)
                    || game.detail.to_lowercase().contains(&needle)
            });
        }
        games
    }

    fn refresh_catalog(&mut self) {
        self.catalog = Catalog::discover();
        self.message = Some(("Rescanned local game libraries".into(), false));
        self.selected = 0;
    }

    fn launch_selected(&mut self, game: &Game) {
        match games::launch(game) {
            Ok(()) => self.message = Some((format!("Launching {}", game.name), false)),
            Err(error) => self.message = Some((error, true)),
        }
    }

    fn select_track(&mut self, path: Option<&std::path::Path>) {
        self.music_config.music = path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Err(error) = music::save_config(&self.music_config) {
            self.message = Some((error, true));
            return;
        }
        match path {
            Some(path) => match self.player.play(path) {
                Ok(()) => self.message = Some(("Playing SID background music".into(), false)),
                Err(error) => self.message = Some((error, true)),
            },
            None => {
                self.player.stop();
                self.player.selected.clear();
                self.message = Some(("Background music is Off".into(), false));
            }
        }
    }

    fn nav(&mut self, ui: &mut Ui, _pal: &Palette) {
        ui.horizontal_wrapped(|ui| {
            for view in [
                View::All,
                View::Steam,
                View::RetroArch,
                View::Native,
                View::Music,
            ] {
                let selected = self.view == view;
                if ui
                    .add(Button::new(view.title()).selected(selected))
                    .clicked()
                {
                    self.view = view;
                    self.selected = 0;
                    self.message = None;
                }
            }
        });
    }

    fn music_view(&mut self, ui: &mut Ui, pal: &Palette) {
        ui.heading("Background music");
        ui.label(
            RichText::new(
                "NIRUGAME does not bundle SID music. Add your own PSID/RSID files to ~/.local/share/nirugame/music and install sidplayfp.",
            )
            .color(pal.muted)
            .size(11.0),
        );
        ui.add_space(6.0);
        let off = self.music_config.music.is_empty();
        if ui
            .add(Button::new(if off { "Off · active" } else { "Off" }).selected(off))
            .clicked()
        {
            self.select_track(None);
        }
        if self.tracks.is_empty() {
            ui.label(RichText::new("No SID files found.").color(pal.warning));
            return;
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for track in self.tracks.clone() {
                    let active = self.music_config.music == track.path.to_string_lossy();
                    let response = Frame::none()
                        .fill(if active { pal.surface } else { pal.background })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if active { pal.accent } else { pal.border },
                        ))
                        .rounding(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(&track.title).strong().color(pal.foreground),
                                );
                                if active && self.player.is_playing() {
                                    ui.label(RichText::new("playing").color(pal.success));
                                }
                                if ui.button("Play").clicked() {
                                    self.select_track(Some(&track.path));
                                }
                            });
                            ui.label(
                                RichText::new(track.path.display().to_string())
                                    .monospace()
                                    .size(10.0)
                                    .color(pal.muted),
                            );
                        });
                    let _ = response;
                    ui.add_space(3.0);
                }
            });
        if self.player.is_playing() && ui.button("Stop music").clicked() {
            self.select_track(None);
        }
        if let Some(track) = self.tracks.iter().find(|track| {
            !self.music_config.music.is_empty()
                && self.music_config.music == track.path.to_string_lossy()
        }) {
            ui.label(
                RichText::new(format!("Selected: {}", track.title))
                    .color(pal.accent)
                    .size(11.0),
            );
        }
    }

    fn games_view(&mut self, ui: &mut Ui, pal: &Palette) {
        let games = self.games();
        if games.is_empty() {
            ui.label(RichText::new("No local games found in this view.").color(pal.muted));
        }
        let mut launch = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, game) in games.iter().enumerate() {
                    let selected = index == self.selected;
                    Frame::none()
                        .fill(if selected {
                            pal.surface
                        } else {
                            pal.background
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if selected { pal.accent } else { pal.border },
                        ))
                        .rounding(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&game.name).strong().color(pal.foreground),
                                    );
                                    ui.label(
                                        RichText::new(&game.detail).color(pal.muted).size(10.0),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .add_enabled(
                                            !game.launch.is_empty(),
                                            Button::new("Launch").small(),
                                        )
                                        .clicked()
                                    {
                                        launch = Some(game.key.clone());
                                    }
                                });
                            });
                        });
                    ui.add_space(3.0);
                }
            });
        if let Some(key) = launch {
            if let Some(game) = games.iter().find(|game| game.key == key) {
                let game = (*game).clone();
                self.launch_selected(&game);
            }
        }
    }

    fn handle_keys(&mut self, ctx: &Context) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let typing = ctx.memory(|memory| memory.focused().is_some());
        if typing {
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Tab)) {
            self.view = match self.view {
                View::All => View::Steam,
                View::Steam => View::RetroArch,
                View::RetroArch => View::Native,
                View::Native => View::Music,
                View::Music => View::All,
            };
            self.selected = 0;
            return;
        }
        if self.view == View::Music {
            return;
        }
        let count = self.games().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
            self.selected = (self.selected + 1).min(count - 1);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
            self.selected = self.selected.saturating_sub(1);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
            if let Some(game) = self.games().get(self.selected).copied().cloned() {
                if !game.launch.is_empty() {
                    self.launch_selected(&game);
                }
            }
        }
    }

    fn footer(&self, ui: &mut Ui, pal: &Palette) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("↑↓ select · Enter launch · Tab switch · Esc close")
                    .color(pal.disabled)
                    .size(10.0),
            );
            if let Some(track) = self.tracks.iter().find(|track| {
                !self.music_config.music.is_empty()
                    && self.music_config.music == track.path.to_string_lossy()
            }) {
                ui.label(
                    RichText::new(format!("♪ {}", track.title))
                        .color(pal.accent)
                        .size(10.0),
                );
            } else {
                ui.label(RichText::new("♪ Off").color(pal.disabled).size(10.0));
            }
        });
    }
}

impl eframe::App for NirugameApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_keys(ctx);
        if let Some(theme) = self.theme_watcher.as_ref().and_then(Watcher::poll) {
            self.theme = theme;
            theme::apply(ctx, &self.theme);
        }
        let pal = self.theme.palette.clone();
        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal.background).inner_margin(18.0))
            .show(ctx, |ui| {
                ui.label(RichText::new(LOGO).monospace().color(pal.accent).size(13.0));
                ui.label(
                    RichText::new(format!(
                        "NIRUGAME · {} games · theme {}",
                        self.catalog.count(),
                        self.theme.name
                    ))
                    .color(pal.muted)
                    .size(11.0),
                );
                ui.add_space(8.0);
                self.nav(ui, &pal);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.query)
                            .hint_text("Search games…")
                            .desired_width(260.0),
                    );
                    if ui.button("Refresh libraries").clicked() {
                        self.refresh_catalog();
                    }
                });
                if let Some((message, error)) = &self.message {
                    ui.label(RichText::new(message).color(if *error {
                        pal.error
                    } else {
                        pal.success
                    }));
                }
                ui.add_space(4.0);
                if self.view == View::Music {
                    self.music_view(ui, &pal);
                } else {
                    self.games_view(ui, &pal);
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    self.footer(ui, &pal);
                });
            });
        if self.player.is_playing() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player.stop();
    }
}
