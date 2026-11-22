use crate::gui::canvas::Rect;
use crate::gui::shell::{App, Waker};
use crate::gui::theme::Palette;
use crate::gui::ui::{keys, Btn, Frame, KeyEv, State, Sz, Ui};
use nirugame::{
    games::{self, Catalog, Game},
    music::{self, Player, Track},
};
use std::sync::mpsc::{self, Receiver};

const SCROLL: &str = "nirugame-list";
const SEARCH: &str = "nirugame-search";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    All,
    Steam,
    RetroArch,
    Native,
    Music,
}

const VIEWS: [View; 5] = [
    View::All,
    View::Steam,
    View::RetroArch,
    View::Native,
    View::Music,
];

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

    fn next(self, back: bool) -> Self {
        let i = VIEWS.iter().position(|v| *v == self).unwrap_or(0);
        let n = VIEWS.len();
        VIEWS[if back { (i + n - 1) % n } else { (i + 1) % n }]
    }
}

/// The libraries and SID folder are read on a thread while the window opens.
type Scan = (Catalog, Vec<Track>);

pub struct NirugameApp {
    catalog: Catalog,
    tracks: Vec<Track>,
    scan: Option<Receiver<Scan>>,
    player: Player,
    music_config: music::Config,
    view: View,
    query: String,
    selected: usize,
    follow: bool,
    message: Option<(String, bool)>,
}

fn frame(fill: u32, stroke: u32) -> Frame {
    Frame {
        fill: Some(fill),
        stroke,
        width: 1,
        pad: 8,
        rad: 4,
    }
}

impl NirugameApp {
    pub fn new(initial: Option<View>, waker: Waker) -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("nirugame-scan".into())
            .spawn(move || {
                let _ = tx.send((Catalog::discover(), music::discover()));
                waker.wake();
            });
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
            catalog: Catalog::default(),
            tracks: Vec::new(),
            scan: Some(rx),
            player,
            music_config,
            view: initial.unwrap_or(View::All),
            query: String::new(),
            selected: 0,
            follow: false,
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

    fn refresh_music(&mut self) {
        self.tracks = music::discover();
        if !self.music_config.music.is_empty()
            && !std::path::Path::new(&self.music_config.music).is_file()
        {
            self.player.stop();
            self.music_config.music.clear();
            if let Err(error) = music::save_config(&self.music_config) {
                self.message = Some((error, true));
                return;
            }
        }
        self.message = Some((
            format!("Rescanned SID music folder ({} tracks)", self.tracks.len()),
            false,
        ));
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

    fn set_view(&mut self, st: &mut State, view: View) {
        self.view = view;
        self.selected = 0;
        st.reset_scroll(SCROLL);
        if view == View::Music {
            self.refresh_music();
        }
    }

    fn nav(&mut self, ui: &mut Ui) {
        let mut go = None;
        ui.flow(|ui| {
            for view in VIEWS {
                if ui.button_ex(view.title(), Btn::default().selected(self.view == view)) {
                    go = Some(view);
                }
            }
        });
        if let Some(view) = go {
            self.message = None;
            self.set_view(ui.st, view);
        }
    }

    fn current_track(&self) -> Option<&Track> {
        self.tracks.iter().find(|track| {
            !self.music_config.music.is_empty()
                && self.music_config.music == track.path.to_string_lossy()
        })
    }

    fn music_view(&mut self, ui: &mut Ui, pal: &Palette) {
        ui.heading("Background music");
        ui.label(
            "NIRUGAME does not bundle SID music. Add your own PSID/RSID files to ~/.local/share/nirugame/music and install sidplayfp.",
            Sz::Small,
            pal.muted,
        );
        let mut refresh = false;
        ui.flow(|ui| {
            refresh = ui.button("Refresh tracks");
            ui.label(
                &format!("{} files found", self.tracks.len()),
                Sz::Small,
                pal.muted,
            );
        });
        if refresh {
            self.refresh_music();
        }
        ui.space(6);
        let off = self.music_config.music.is_empty();
        if ui.button_ex(
            if off { "Off · active" } else { "Off" },
            Btn::default().selected(off),
        ) {
            self.select_track(None);
        }
        if self.tracks.is_empty() {
            ui.label("No SID files found.", Sz::Body, pal.warning);
            return;
        }
        let mut play = None;
        let playing = self.player.is_playing();
        for track in &self.tracks {
            let active = self.music_config.music == track.path.to_string_lossy();
            let fill = if active { pal.surface } else { pal.background };
            let stroke = if active { pal.accent } else { pal.border };
            ui.frame(frame(fill, stroke), |ui| {
                ui.flow(|ui| {
                    ui.strong(&track.title, Sz::Body, pal.foreground);
                    if active && playing {
                        ui.label("playing", Sz::Body, pal.success);
                    }
                    if ui.button_ex("Play", Btn::default().small()) {
                        play = Some(track.path.clone());
                    }
                });
                ui.label(&track.path.display().to_string(), Sz::Tiny, pal.muted);
            });
        }
        if let Some(path) = play {
            self.select_track(Some(&path));
        }
        if self.player.is_playing() && ui.button("Stop music") {
            self.select_track(None);
        }
        if let Some(track) = self.current_track() {
            let t = format!("Selected: {}", track.title);
            ui.label(&t, Sz::Small, pal.accent);
        }
    }

    fn games_view(&mut self, ui: &mut Ui, pal: &Palette, view: Rect) {
        let games: Vec<Game> = self.games().into_iter().cloned().collect();
        if games.is_empty() {
            let text = if self.scan.is_some() {
                "Reading the local game libraries…"
            } else {
                "No local games found in this view."
            };
            ui.label(text, Sz::Body, pal.muted);
        }
        let mut launch = None;
        let mut pick = None;
        let mut sel_rect = None;
        let launch_w = ui.text_width("Launch", Sz::Small) + 14;
        for (index, game) in games.iter().enumerate() {
            let selected = index == self.selected;
            let fill = if selected {
                pal.surface
            } else {
                pal.background
            };
            let stroke = if selected { pal.accent } else { pal.border };
            let (_, r) = ui.frame(frame(fill, stroke), |ui| {
                let (x, y, w) = (ui.left(), ui.cursor_y(), ui.width());
                ui.set_area(Rect::new(x, y, (w - launch_w - 8).max(40), 0));
                let name = ui.ellipsize(&game.name, Sz::Body, ui.width());
                ui.strong(&name, Sz::Body, pal.foreground);
                ui.label(&game.detail, Sz::Tiny, pal.muted);
                let end = ui.cursor_y();
                let bh = ui.line_h(Sz::Small) + 6;
                let by = y + ((end - ui.spacing - y) - bh).max(0) / 2;
                ui.set_area(Rect::new(x + w - launch_w, by, launch_w, 0));
                if ui.button_ex(
                    "Launch",
                    Btn::default().small().enabled(!game.launch.is_empty()),
                ) {
                    launch = Some(game.key.clone());
                }
                let end = ui.cursor_y().max(end);
                ui.set_area(Rect::new(x, end, w, 0));
            });
            if ui.click_area(&game.key, r) {
                pick = Some(index);
            }
            if selected {
                sel_rect = Some(r);
            }
        }
        if let Some(index) = pick {
            self.selected = index;
        }
        if let Some(key) = launch {
            if let Some(game) = games.iter().find(|game| game.key == key) {
                self.launch_selected(game);
            }
        }
        // Keep the keyboard selection on screen.
        if self.follow {
            self.follow = false;
            if let Some(r) = sel_rect {
                let delta = if r.y < view.y {
                    r.y - view.y - 4
                } else if r.bottom() > view.bottom() {
                    r.bottom() - view.bottom() + 4
                } else {
                    0
                };
                if delta != 0 && ui.st.scroll_by(SCROLL, delta) {
                    ui.st.again = true;
                }
            }
        }
    }

    fn footer(&self, ui: &mut Ui, pal: &Palette) {
        ui.flow(|ui| {
            ui.label(
                "Up/Down select · Enter launch · Tab switch · Esc close",
                Sz::Tiny,
                pal.disabled,
            );
            match self.current_track() {
                Some(track) => {
                    let t = format!("♪ {}", track.title);
                    ui.label(&t, Sz::Tiny, pal.accent)
                }
                None => ui.label("♪ Off", Sz::Tiny, pal.disabled),
            };
        });
    }

    fn launch_current(&mut self) {
        if let Some(game) = self.games().get(self.selected).copied().cloned() {
            if !game.launch.is_empty() {
                self.launch_selected(&game);
            }
        }
    }

    fn move_selection(&mut self, down: bool) -> bool {
        let count = self.games().len();
        if count == 0 {
            self.selected = 0;
            return false;
        }
        self.selected = if down {
            (self.selected + 1).min(count - 1)
        } else {
            self.selected.min(count - 1).saturating_sub(1)
        };
        self.follow = true;
        true
    }
}

impl App for NirugameApp {
    fn build(&mut self, ui: &mut Ui) {
        let pal = ui.pal.clone();
        let screen = ui.screen;
        let m = if screen.w < 480 { 10 } else { 18 };
        let area = Rect::new(m, m, (screen.w - 2 * m).max(40), (screen.h - 2 * m).max(40));
        ui.set_area(area);
        let (x, y) = (ui.left(), ui.cursor_y());
        let logo = crate::gui::logo::title(ui, "NIRUGAME", 4);
        let sub_w = ui.text_width("// retro game launcher", Sz::Small);
        if logo.w + 14 + sub_w <= area.w {
            ui.paint_text(
                x + logo.w + 14,
                y + logo.h - ui.line_h(Sz::Small) - 4,
                "// retro game launcher",
                Sz::Small,
                pal.muted,
                false,
            );
        } else {
            ui.label("// retro game launcher", Sz::Small, pal.muted);
        }
        let count = if self.scan.is_some() {
            "reading libraries".to_string()
        } else {
            format!("{} games", self.catalog.count())
        };
        let info = format!("NIRUGAME · {count} · theme {}", ui.theme.name);
        ui.label(&info, Sz::Small, pal.muted);
        ui.space(4);
        self.nav(ui);
        ui.separator();
        let mut refresh = false;
        ui.flow(|ui| {
            if ui.text_edit(SEARCH, &mut self.query, "Search games…", 260) {
                self.selected = 0;
            }
            refresh = ui.button("Refresh libraries");
        });
        if refresh {
            self.refresh_catalog();
        }
        if let Some((message, error)) = &self.message {
            let color = if *error { pal.error } else { pal.success };
            ui.label(message, Sz::Body, color);
        }
        ui.space(2);
        let footer_h = ui.line_h(Sz::Tiny) + 8;
        let top = ui.cursor_y();
        let view = Rect::new(
            area.x,
            top,
            area.w,
            (area.bottom() - footer_h - top).max(40),
        );
        ui.scroll(SCROLL, view, |ui| {
            if self.view == View::Music {
                self.music_view(ui, &pal);
            } else {
                self.games_view(ui, &pal, view);
            }
        });
        ui.set_area(Rect::new(area.x, area.bottom() - footer_h + 4, area.w, 0));
        self.footer(ui, &pal);
    }

    fn key(&mut self, k: &KeyEv, st: &mut State) -> bool {
        if k.is(keys::ESCAPE) {
            st.quit = true;
            return true;
        }
        if (k.ks == keys::TAB || k.ks == keys::ISO_LEFT_TAB) && !k.ctrl && !k.alt {
            st.blur();
            let next = self.view.next(k.shift || k.ks == keys::ISO_LEFT_TAB);
            self.set_view(st, next);
            self.message = None;
            return true;
        }
        if k.is(keys::F5) {
            if self.view == View::Music {
                self.refresh_music();
            } else {
                self.refresh_catalog();
            }
            return true;
        }
        if k.ctrl && matches!(k.ch, Some('f') | Some('F')) {
            st.focus_field(SEARCH);
            return true;
        }
        if self.view == View::Music {
            return false;
        }
        match k.ks {
            keys::DOWN if !k.ctrl && !k.alt => self.move_selection(true),
            keys::UP if !k.ctrl && !k.alt => self.move_selection(false),
            _ if k.enter() => {
                self.launch_current();
                true
            }
            _ => false,
        }
    }

    fn key_late(&mut self, k: &KeyEv, st: &mut State) -> bool {
        // Typing anywhere searches.
        if let Some(c) = k.ch {
            if !k.ctrl && !k.alt && !c.is_control() && c != ' ' && !st.typing() {
                st.focus_field(SEARCH);
                st.paste(&c.to_string());
                self.selected = 0;
                return true;
            }
        }
        let page = (st.scroll_view_height(SCROLL) - 40).max(40);
        match k.ks {
            keys::PAGE_DOWN => st.scroll_by(SCROLL, page),
            keys::PAGE_UP => st.scroll_by(SCROLL, -page),
            _ => false,
        }
    }

    fn poll(&mut self) -> bool {
        let Some(rx) = &self.scan else {
            return false;
        };
        match rx.try_recv() {
            Ok((catalog, tracks)) => {
                self.catalog = catalog;
                self.tracks = tracks;
                self.scan = None;
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.scan = None;
                true
            }
        }
    }

    fn quit(&mut self) {
        self.player.stop();
    }
}
