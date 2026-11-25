use crate::gui::canvas::Rect;
use crate::gui::logo;
use crate::gui::shell::{App, Waker};
use crate::gui::theme::{blend, Palette};
use crate::gui::ui::{keys, Btn, Frame, KeyEv, State, Sz, Ui};
use nirugame::{
    config,
    games::{self, Catalog, Game, Kind},
    library::{self, Library},
    music::{self, Player, Track},
    systems::{self, System},
    thumbs::{self, Image, Reply, Request},
};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SCROLL: &str = "nirugame-list";
const MUSIC_SCROLL: &str = "nirugame-music";
const SEARCH: &str = "nirugame-search";
/// Decoded box art kept in memory at most (bytes of pixels).
const THUMB_BYTES: usize = 8 << 20;
const RECENT: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    All,
    Favourites,
    Recent,
    Steam,
    RetroArch,
    Native,
    System(System),
    Music,
}

impl View {
    fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Favourites => "Favourites",
            Self::Recent => "Recent",
            Self::Steam => "Steam",
            Self::RetroArch => "RetroArch",
            Self::Native => "Native",
            Self::System(s) => s.info().badge,
            Self::Music => "Music",
        }
    }

    fn heading(self) -> String {
        match self {
            Self::All => "ALL GAMES".into(),
            Self::System(s) => s.info().name.to_ascii_uppercase(),
            v => v.title().to_ascii_uppercase(),
        }
    }

    /// Slot in the per-view counters.
    fn slot(self) -> usize {
        match self {
            Self::All => 0,
            Self::Favourites => 1,
            Self::Recent => 2,
            Self::Steam => 3,
            Self::RetroArch => 4,
            Self::Native => 5,
            Self::System(s) => 6 + s.index(),
            Self::Music => 6 + systems::ALL.len(),
        }
    }
}

const SLOTS: usize = 7 + systems::ALL.len();
const MAIN: [View; 6] = [
    View::All,
    View::Favourites,
    View::Recent,
    View::Steam,
    View::RetroArch,
    View::Native,
];

enum Msg {
    /// An outdated cached index, shown while the rescan runs.
    Stale(Catalog),
    Done(Catalog, Vec<Track>),
}

enum Thumb {
    Pending,
    Missing,
    Ready(Arc<Image>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArtSize {
    Row,
    Tile,
    Pane,
}

fn art_dims(s: ArtSize) -> (i32, i32) {
    match s {
        ArtSize::Row => (40, 40),
        ArtSize::Tile => (128, 120),
        ArtSize::Pane => (280, 210),
    }
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub struct NirugameApp {
    games: Vec<Game>,
    /// Lower-case search text of each game.
    search: Vec<String>,
    by_key: HashMap<String, u32>,
    fav: Vec<bool>,
    /// Unfiltered number of games per view, and matches while searching.
    totals: [usize; SLOTS],
    counts: [usize; SLOTS],
    filtered: Vec<u32>,
    dirty: bool,
    errors: Vec<String>,
    thumb_dirs: Vec<PathBuf>,
    index_source: &'static str,
    scan: Option<Receiver<Msg>>,
    waker: Waker,
    library: Library,
    tracks: Vec<Track>,
    player: Player,
    music: String,
    music_sel: usize,
    nirusid: Option<PathBuf>,
    view: View,
    query: String,
    selected: usize,
    follow: bool,
    grid: bool,
    cols: usize,
    page: usize,
    /// Scroll offset, row height and view height of the last list frame.
    geo: Option<(i32, i32, i32)>,
    message: Option<(String, bool)>,
    last_click: Option<(usize, Instant)>,
    thumb_tx: Sender<Request>,
    thumb_rx: Receiver<Reply>,
    thumbs: HashMap<u64, Thumb>,
    thumb_lru: VecDeque<u64>,
    thumb_bytes: usize,
    started: Instant,
    timing: bool,
}

fn frame(fill: u32, stroke: u32) -> Frame {
    Frame {
        fill: Some(fill),
        stroke,
        width: 1,
        pad: 8,
        rad: 2,
    }
}

/// A colour of the theme for each system, so badges tell systems apart and
/// still follow the theme.
fn tint_for(p: &Palette, game: Option<&Game>) -> u32 {
    let choices = [
        p.accent,
        p.success,
        p.warning,
        p.error,
        blend(p.accent, p.foreground, 0.45),
        blend(p.success, p.accent, 0.5),
    ];
    match game.map(|g| (g.kind, g.system())) {
        Some((_, Some(s))) => choices[s.index() % choices.len()],
        Some((Kind::Steam, _)) => p.accent,
        Some((Kind::Native, _)) => p.success,
        _ => p.muted,
    }
}

fn badge_text(game: &Game) -> &'static str {
    match (game.kind, game.system()) {
        (_, Some(s)) => s.info().badge,
        (Kind::Steam, _) => "STEAM",
        (Kind::Native, _) => "PC",
        (Kind::RetroArch, None) => "RA",
    }
}

/// A retro placeholder: the system name in pixel letters on a tile.
fn badge(ui: &mut Ui, r: Rect, text: &str, color: u32) {
    let p = ui.pal.clone();
    ui.paint_box(
        r,
        2,
        Some(blend(p.panel, color, 0.14)),
        blend(p.border, color, 0.6),
        1,
    );
    let unit = logo::word_width(text, 1).max(1);
    let px = ((r.w - 8) / unit).min((r.h - 6) / 8).clamp(1, 8);
    let (w, h) = (logo::word_width(text, px), 7 * px);
    let (x, y) = (r.x + (r.w - w) / 2, r.y + (r.h - h) / 2);
    logo::pixel_text(
        ui,
        x + (px / 2).max(1),
        y + (px / 2).max(1),
        text,
        px,
        blend(p.panel, color, 0.35),
    );
    logo::pixel_text(ui, x, y, text, px, color);
    let mut sy = r.y + 1;
    while sy < r.bottom() - 1 {
        ui.paint_tint(Rect::new(r.x + 1, sy, r.w - 2, 1), p.background, 40);
        sy += 2;
    }
}

impl NirugameApp {
    pub fn new(initial: Option<View>, waker: Waker, started: Instant) -> Self {
        let (thumb_tx, thumb_rx) = thumbs::spawn(move || waker.wake());
        let cfg = config::load();
        let mut player = Player::default();
        let mut message = None;
        if !cfg.music.is_empty() {
            let path = std::path::Path::new(&cfg.music);
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
        let mut app = Self {
            games: Vec::new(),
            search: Vec::new(),
            by_key: HashMap::new(),
            fav: Vec::new(),
            totals: [0; SLOTS],
            counts: [0; SLOTS],
            filtered: Vec::new(),
            dirty: true,
            errors: Vec::new(),
            thumb_dirs: Vec::new(),
            index_source: "",
            scan: None,
            waker,
            library: Library::load_from(&library::path()),
            tracks: Vec::new(),
            player,
            music: cfg.music,
            music_sel: 0,
            nirusid: music::nirusid(),
            view: initial.unwrap_or(View::All),
            query: String::new(),
            selected: 0,
            follow: false,
            grid: false,
            cols: 1,
            page: 8,
            geo: None,
            message,
            last_click: None,
            thumb_tx,
            thumb_rx,
            thumbs: HashMap::new(),
            thumb_lru: VecDeque::new(),
            thumb_bytes: 0,
            started,
            timing: std::env::var_os("NIRUGAME_TIMING").is_some(),
        };
        app.start_scan(false);
        app
    }

    /// Read the libraries on a thread; the window does not wait.
    fn start_scan(&mut self, force: bool) {
        let (tx, rx) = mpsc::channel();
        let waker = self.waker;
        let _ = std::thread::Builder::new()
            .name("nirugame-scan".into())
            .spawn(move || {
                let cfg = config::load();
                let catalog = Catalog::discover_with(&cfg, force, &mut |stale| {
                    let _ = tx.send(Msg::Stale(stale));
                    waker.wake();
                });
                let _ = tx.send(Msg::Done(catalog, music::discover()));
                waker.wake();
            });
        self.scan = Some(rx);
    }

    fn apply(&mut self, catalog: Catalog) {
        let keep = self.current().map(|g| g.key.clone());
        let mut games: Vec<Game> = Vec::with_capacity(catalog.count());
        let Catalog {
            steam,
            retroarch,
            native,
            errors,
            thumbnail_dirs,
            index,
            ..
        } = catalog;
        games.extend(steam);
        games.extend(retroarch);
        games.extend(native);
        games.sort_by_cached_key(|g| g.name.to_lowercase());
        self.search = games
            .iter()
            .map(|g| {
                let sys = g.system().map(|s| s.info().id).unwrap_or("");
                let kind = match g.kind {
                    Kind::Steam => "steam",
                    Kind::RetroArch => "retroarch",
                    Kind::Native => "native",
                };
                format!("{} {} {}", g.name, sys, kind).to_lowercase()
            })
            .collect();
        self.by_key = games
            .iter()
            .enumerate()
            .map(|(i, g)| (g.key.clone(), i as u32))
            .collect();
        self.games = games;
        self.errors = errors;
        self.thumb_dirs = thumbnail_dirs.into_iter().filter(|d| d.is_dir()).collect();
        self.index_source = index;
        self.thumbs.clear();
        self.thumb_lru.clear();
        self.thumb_bytes = 0;
        self.refresh_favourites();
        self.refilter();
        if let Some(i) = keep
            .and_then(|k| self.by_key.get(&k).copied())
            .and_then(|i| self.filtered.iter().position(|f| *f == i))
        {
            self.selected = i;
        }
    }

    fn refresh_favourites(&mut self) {
        self.fav = self
            .games
            .iter()
            .map(|g| self.library.favourites.contains(&g.key))
            .collect();
        self.dirty = true;
    }

    fn in_view(&self, i: usize, view: View) -> bool {
        let g = &self.games[i];
        match view {
            View::All => true,
            View::Favourites => self.fav[i],
            View::Recent => self.library.played.contains_key(&g.key),
            View::Steam => g.kind == Kind::Steam,
            View::RetroArch => g.kind == Kind::RetroArch,
            View::Native => g.kind == Kind::Native,
            View::System(s) => g.system() == Some(s),
            View::Music => false,
        }
    }

    /// Recompute the visible list and the per-view counters.
    fn refilter(&mut self) {
        self.dirty = false;
        let q = self.query.trim().to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        let mut totals = [0; SLOTS];
        let mut counts = [0; SLOTS];
        let mut list = Vec::new();
        for i in 0..self.games.len() {
            let g = &self.games[i];
            let mut slots = [usize::MAX; 4];
            slots[0] = View::All.slot();
            slots[1] = match g.kind {
                Kind::Steam => View::Steam.slot(),
                Kind::RetroArch => View::RetroArch.slot(),
                Kind::Native => View::Native.slot(),
            };
            if let Some(s) = g.system() {
                slots[2] = View::System(s).slot();
            }
            if self.fav[i] {
                slots[3] = View::Favourites.slot();
            }
            let hit = words.iter().all(|w| self.search[i].contains(w));
            for s in slots.into_iter().filter(|s| *s != usize::MAX) {
                totals[s] += 1;
                if hit {
                    counts[s] += 1;
                }
            }
            if hit && self.view != View::Recent && self.in_view(i, self.view) {
                list.push(i as u32);
            }
        }
        for key in self.library.recent(RECENT) {
            if let Some(&i) = self.by_key.get(key) {
                totals[View::Recent.slot()] += 1;
                if words.iter().all(|w| self.search[i as usize].contains(w)) {
                    counts[View::Recent.slot()] += 1;
                    if self.view == View::Recent {
                        list.push(i);
                    }
                }
            }
        }
        self.totals = totals;
        self.counts = counts;
        self.filtered = list;
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    fn current(&self) -> Option<&Game> {
        self.filtered
            .get(self.selected)
            .map(|&i| &self.games[i as usize])
    }

    /// The tabs in Tab order: the main ones, the systems that have games
    /// (and the current one), then Music.
    fn views(&self) -> Vec<View> {
        let mut v: Vec<View> = MAIN.to_vec();
        for s in systems::ALL {
            let view = View::System(s);
            if self.totals[view.slot()] > 0 || self.view == view {
                v.push(view);
            }
        }
        v.push(View::Music);
        v
    }

    fn next_view(&self, back: bool) -> View {
        let views = self.views();
        let n = views.len();
        let i = views.iter().position(|v| *v == self.view).unwrap_or(0);
        views[if back { (i + n - 1) % n } else { (i + 1) % n }]
    }

    fn set_view(&mut self, st: &mut State, view: View) {
        self.view = view;
        self.selected = 0;
        self.last_click = None;
        self.geo = None;
        st.reset_scroll(SCROLL);
        st.reset_scroll(MUSIC_SCROLL);
        self.refilter();
        if view == View::Music {
            self.refresh_music();
        }
    }

    fn refresh_music(&mut self) {
        self.tracks = music::discover();
        self.nirusid = music::nirusid();
        self.music_sel = self.music_sel.min(self.tracks.len().saturating_sub(1));
        if !self.music.is_empty() && !std::path::Path::new(&self.music).is_file() {
            self.player.stop();
            self.music.clear();
            if let Err(error) = config::save_music("") {
                self.message = Some((error, true));
                return;
            }
        }
        self.message = Some((
            format!("Rescanned SID music folder ({} tracks)", self.tracks.len()),
            false,
        ));
    }

    fn rescan(&mut self) {
        self.message = Some((
            "Rescanning the game libraries and ROM folders…".into(),
            false,
        ));
        self.start_scan(true);
    }

    fn launch_argv(&mut self, idx: usize, argv: Vec<String>) {
        let name = self.games[idx].name.clone();
        match games::launch_argv(&argv, &name) {
            Ok(()) => {
                self.message = Some((format!("Launching {name}"), false));
                let key = self.games[idx].key.clone();
                self.library.record_play(&key, library::now());
                if let Err(e) = self.library.save_to(&library::path()) {
                    self.message = Some((
                        format!("Launched, but the history was not saved: {e}"),
                        true,
                    ));
                }
                self.dirty = true;
            }
            Err(error) => self.message = Some((error, true)),
        }
    }

    fn launch_current(&mut self, alt: bool) {
        let Some(&i) = self.filtered.get(self.selected) else {
            return;
        };
        let g = &self.games[i as usize];
        let argv = if alt {
            g.rom
                .as_ref()
                .map(|r| r.alt_launch.clone())
                .unwrap_or_default()
        } else {
            g.launch.clone()
        };
        if argv.is_empty() {
            let hint = g.rom.as_ref().map(|r| r.hint.clone()).unwrap_or_default();
            if !hint.is_empty() {
                self.message = Some((hint, true));
            }
            return;
        }
        self.launch_argv(i as usize, argv);
    }

    fn toggle_favourite(&mut self) {
        let Some(&i) = self.filtered.get(self.selected) else {
            return;
        };
        let key = self.games[i as usize].key.clone();
        let on = self.library.toggle_favourite(&key);
        self.message = Some((
            format!(
                "{} {}",
                self.games[i as usize].name,
                if on {
                    "is a favourite"
                } else {
                    "is no longer a favourite"
                }
            ),
            false,
        ));
        if let Err(e) = self.library.save_to(&library::path()) {
            self.message = Some((format!("Favourites not saved: {e}"), true));
        }
        self.fav[i as usize] = on;
        self.refilter();
    }

    fn select_track(&mut self, path: Option<&std::path::Path>) {
        self.music = path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Err(error) = config::save_music(&self.music) {
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

    fn open_nirusid(&mut self) {
        let Some(program) = self.nirusid.clone() else {
            return;
        };
        let Some(track) = self.tracks.get(self.music_sel).map(|t| t.path.clone()) else {
            return;
        };
        match music::open_in_nirusid(&program, &track) {
            Ok(()) => {
                // One SID player at a time; the saved choice stays.
                self.player.stop();
                self.message = Some(("Opened in NIRUSID; background music paused".into(), false));
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    fn current_track(&self) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|track| !self.music.is_empty() && self.music == track.path.to_string_lossy())
    }

    // ------------------------------------------------------------ box art

    fn thumb(&mut self, idx: usize, size: ArtSize) -> Option<Arc<Image>> {
        if self.thumb_dirs.is_empty() {
            return None;
        }
        let g = &self.games[idx];
        let rom = g.rom.as_ref()?;
        let id = fnv(&g.key).wrapping_mul(4).wrapping_add(size as u64);
        match self.thumbs.get(&id) {
            Some(Thumb::Ready(img)) => return Some(img.clone()),
            Some(_) => return None,
            None => {}
        }
        let stem = std::path::Path::new(&rom.file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let db = rom.system.map(|s| s.info().db).unwrap_or("");
        let files = thumbs::candidates(
            &self.thumb_dirs,
            &[rom.playlist.as_str(), db],
            &[rom.label.as_str(), stem.as_str()],
        );
        let (w, h) = art_dims(size);
        if self.thumb_tx.send(Request { id, files, w, h }).is_ok() {
            self.thumbs.insert(id, Thumb::Pending);
        }
        None
    }

    fn take_thumbs(&mut self) -> bool {
        let mut any = false;
        while let Ok(reply) = self.thumb_rx.try_recv() {
            match reply {
                Reply::Ready(id, img) => {
                    self.thumb_bytes += img.px.len() * 4;
                    self.thumbs.insert(id, Thumb::Ready(img));
                    self.thumb_lru.push_back(id);
                    any = true;
                }
                Reply::Missing(id) => {
                    self.thumbs.insert(id, Thumb::Missing);
                }
                Reply::Dropped(id) => {
                    self.thumbs.remove(&id);
                    any = true;
                }
            }
        }
        while self.thumb_bytes > THUMB_BYTES {
            let Some(old) = self.thumb_lru.pop_front() else {
                break;
            };
            if let Some(Thumb::Ready(img)) = self.thumbs.remove(&old) {
                self.thumb_bytes -= img.px.len() * 4;
            }
        }
        any
    }

    /// Box art centred in `r`, or the system badge.
    fn art(&mut self, ui: &mut Ui, idx: usize, r: Rect, size: ArtSize) {
        if let Some(img) = self.thumb(idx, size) {
            let p = ui.pal.clone();
            ui.paint_box(r, 2, Some(p.panel), p.border, 1);
            ui.paint_image(r.x + (r.w - img.w) / 2, r.y + (r.h - img.h) / 2, img);
            return;
        }
        let g = &self.games[idx];
        let color = tint_for(ui.pal, Some(g));
        badge(ui, r, badge_text(g), color);
    }

    // ------------------------------------------------------------ views

    fn tabs(&mut self, ui: &mut Ui) {
        let mut go = None;
        let searching = !self.query.trim().is_empty();
        let n = |s: &Self, v: View| {
            if searching {
                s.counts[v.slot()]
            } else {
                s.totals[v.slot()]
            }
        };
        ui.flow(|ui| {
            for view in MAIN {
                let label = format!("{} {}", view.title(), n(self, view));
                if ui.button_ex(&label, Btn::default().small().selected(self.view == view)) {
                    go = Some(view);
                }
            }
            if ui.button_ex(
                "Music",
                Btn::default().small().selected(self.view == View::Music),
            ) {
                go = Some(View::Music);
            }
        });
        // Systems: pixel badges with their counts.
        let sys: Vec<View> = self
            .views()
            .into_iter()
            .filter(|v| matches!(v, View::System(_)))
            .collect();
        if !sys.is_empty() {
            let p = ui.pal.clone();
            ui.flow(|ui| {
                for view in sys {
                    let View::System(s) = view else { continue };
                    let count = n(self, view).to_string();
                    let bw = logo::word_width(s.info().badge, 2) + 12;
                    let cw = ui.text_width(&count, Sz::Small);
                    let h = 24;
                    let r = ui.alloc(bw + cw + 10, h);
                    let on = self.view == view;
                    let color = p_color(&p, s);
                    let fill = if on {
                        blend(p.panel, color, 0.22)
                    } else {
                        p.panel
                    };
                    ui.paint_box(r, 2, Some(fill), if on { color } else { p.border }, 1);
                    logo::pixel_text(ui, r.x + 6, r.y + 5, s.info().badge, 2, color);
                    let fg = if n(self, view) == 0 {
                        p.disabled
                    } else {
                        p.foreground
                    };
                    ui.paint_text(
                        r.x + bw,
                        r.y + (h - ui.line_h(Sz::Small)) / 2,
                        &count,
                        Sz::Small,
                        fg,
                        false,
                    );
                    if ui.click_area(&format!("system-{}", s.id()), r) {
                        go = Some(view);
                    }
                }
            });
        }
        if let Some(view) = go {
            self.message = None;
            self.set_view(ui.st, view);
        }
    }

    fn row(&mut self, ui: &mut Ui, pos: usize, r: Rect, narrow: bool) -> Option<bool> {
        let idx = self.filtered[pos] as usize;
        let p = ui.pal.clone();
        let selected = pos == self.selected;
        let inner = Rect::new(r.x, r.y + 2, r.w, r.h - 4);
        let (fill, stroke) = if selected {
            (p.surface, p.accent)
        } else {
            (p.background, blend(p.background, p.border, 0.6))
        };
        ui.paint_box(inner, 2, Some(fill), stroke, 1);
        if selected {
            ui.paint_fill(Rect::new(inner.x, inner.y, 3, inner.h), p.accent);
        }
        let art = Rect::new(inner.x + 8, inner.y + (inner.h - 40) / 2, 40, 40);
        self.art(ui, idx, art, ArtSize::Row);
        let g = &self.games[idx];
        let can = !g.launch.is_empty();
        let tx = art.right() + 10;
        let mut tw = inner.right() - tx - 10;
        let launch_w = ui.text_width("Launch", Sz::Small) + 16;
        let mut launch = None;
        if narrow && selected {
            tw -= launch_w + 8;
        }
        let star = self.fav[idx];
        if star {
            tw -= 16;
            logo::pixel_text(
                ui,
                inner.right() - 18 - if narrow && selected { launch_w + 8 } else { 0 },
                inner.y + 8,
                "*",
                1,
                p.warning,
            );
        }
        let name = ui.ellipsize(&g.name, Sz::Body, tw.max(20));
        let body_h = ui.line_h(Sz::Body);
        let tiny_h = ui.line_h(Sz::Tiny);
        let ty = inner.y + (inner.h - body_h - tiny_h - 2) / 2;
        ui.paint_text(
            tx,
            ty,
            &name,
            Sz::Body,
            if can { p.foreground } else { p.disabled },
            can,
        );
        let sub = match g.rom.as_ref() {
            Some(rom) if !rom.hint.is_empty() => rom.hint.clone(),
            _ => g.detail.clone(),
        };
        let sub = ui.ellipsize(&sub, Sz::Tiny, tw.max(20));
        let sub_color = if can { p.muted } else { p.warning };
        ui.paint_text(tx, ty + body_h + 2, &sub, Sz::Tiny, sub_color, false);
        if narrow && selected {
            let bh = ui.line_h(Sz::Small) + 6;
            ui.set_area(Rect::new(
                inner.right() - launch_w - 8,
                inner.y + (inner.h - bh) / 2,
                launch_w,
                0,
            ));
            if ui.button_ex("Launch", Btn::default().small().enabled(can)) {
                launch = Some(true);
            }
            // Back to the row layout of the list.
            ui.set_area(Rect::new(r.x, r.bottom(), r.w, 0));
        }
        let key = self.games[idx].key.clone();
        if ui.click_area(&key, inner) && launch.is_none() {
            launch = Some(false);
        }
        launch
    }

    fn tile(&mut self, ui: &mut Ui, pos: usize, r: Rect) -> bool {
        let idx = self.filtered[pos] as usize;
        let p = ui.pal.clone();
        let selected = pos == self.selected;
        let inner = Rect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6);
        let (fill, stroke) = if selected {
            (p.surface, p.accent)
        } else {
            (p.background, blend(p.background, p.border, 0.6))
        };
        ui.paint_box(inner, 2, Some(fill), stroke, if selected { 2 } else { 1 });
        let (aw, ah) = art_dims(ArtSize::Tile);
        let art = Rect::new(inner.x + (inner.w - aw) / 2, inner.y + 6, aw, ah);
        self.art(ui, idx, art, ArtSize::Tile);
        let g = &self.games[idx];
        let can = !g.launch.is_empty();
        let lines = ui.wrap(&g.name, Sz::Small, inner.w - 12);
        let lh = ui.line_h(Sz::Small);
        for (i, l) in lines.iter().take(2).enumerate() {
            let l = if i == 1 && lines.len() > 2 {
                ui.ellipsize(&format!("{l}…"), Sz::Small, inner.w - 12)
            } else {
                l.clone()
            };
            let w = ui.text_width(&l, Sz::Small);
            ui.paint_text(
                inner.x + (inner.w - w) / 2,
                art.bottom() + 6 + i as i32 * (lh + 1),
                &l,
                Sz::Small,
                if can { p.foreground } else { p.disabled },
                false,
            );
        }
        if self.fav[idx] {
            logo::pixel_text(ui, inner.right() - 12, inner.y + 4, "*", 1, p.warning);
        }
        let key = self.games[idx].key.clone();
        ui.click_area(&key, inner)
    }

    fn games_view(&mut self, ui: &mut Ui, view: Rect, narrow: bool) {
        let p = ui.pal.clone();
        let n = self.filtered.len();
        if n == 0 {
            let text = if self.scan.is_some() && self.games.is_empty() {
                "Reading the game libraries and ROM folders…".to_string()
            } else if !self.query.trim().is_empty() {
                let all = self.counts[View::All.slot()];
                if all > 0 && self.view != View::All {
                    format!("No match here; {all} in All (Tab to it, or Esc to clear the search).")
                } else {
                    "Nothing matches the search.".into()
                }
            } else {
                match self.view {
                    View::Favourites => "No favourites yet: select a game and press F.".into(),
                    View::Recent => "Nothing played yet.".into(),
                    View::System(s) => {
                        format!("No {} games found in the ROM folders.", s.info().badge)
                    }
                    _ => "No local games found in this view.".into(),
                }
            };
            ui.label(&text, Sz::Body, p.muted);
            return;
        }
        let saved_spacing = ui.spacing;
        ui.spacing = 0;
        let width = ui.width();
        let off = view.y - ui.cursor_y();
        let (rh, cols) = if self.grid {
            let tw = 150;
            (200, ((width / tw).max(1)) as usize)
        } else {
            ((ui.line_h(Sz::Body) + ui.line_h(Sz::Tiny) + 22).max(54), 1)
        };
        self.cols = cols;
        let rows = n.div_ceil(cols);
        self.page = ((view.h / rh).max(1) as usize) * cols;
        self.geo = Some((off, rh, view.h));
        let first = (off.max(0) / rh) as usize;
        let last = (((off + view.h) / rh) as usize + 1).min(rows);
        ui.space(first as i32 * rh);
        let mut pick = None;
        let mut launch = None;
        for row in first..last.max(first) {
            let r = ui.alloc(width, rh);
            if self.grid {
                let tw = width / cols as i32;
                for c in 0..cols {
                    let pos = row * cols + c;
                    if pos >= n {
                        break;
                    }
                    let tr = Rect::new(r.x + c as i32 * tw, r.y, tw, rh);
                    if self.tile(ui, pos, tr) {
                        pick = Some(pos);
                    }
                }
            } else {
                match self.row(ui, row, r, narrow) {
                    Some(true) => launch = Some(row),
                    Some(false) => pick = Some(row),
                    None => {}
                }
            }
        }
        ui.space((rows - last.max(first)) as i32 * rh);
        ui.spacing = saved_spacing;
        ui.space(saved_spacing);
        if let Some(pos) = launch {
            self.selected = pos;
            self.launch_current(false);
        }
        if let Some(pos) = pick {
            // A second click on the same game within half a second starts it.
            let now = Instant::now();
            let double = self.last_click.is_some_and(|(p, t)| {
                p == pos && now.duration_since(t) < Duration::from_millis(450)
            });
            self.selected = pos;
            if double {
                self.last_click = None;
                self.launch_current(false);
            } else {
                self.last_click = Some((pos, now));
            }
        }
        // Keep the keyboard selection on screen.
        if self.follow {
            let top = (self.selected / cols) as i32 * rh;
            let delta = if top < off {
                top - off
            } else if top + rh > off + view.h {
                top + rh - off - view.h
            } else {
                0
            };
            if delta != 0 && ui.st.scroll_by(SCROLL, delta) {
                ui.st.again = true;
            } else {
                self.follow = false;
            }
        }
    }

    fn details(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.pal.clone();
        ui.paint_box(r, 2, Some(p.panel), p.border, 1);
        let x = r.x + 12;
        let w = r.w - 24;
        let head = logo::pixel_heading(ui, x, r.y + 10, "DETAILS", 2);
        let Some(&i) = self.filtered.get(self.selected) else {
            ui.set_area(Rect::new(x, head.bottom() + 12, w, 0));
            ui.label("Select a game to see its details.", Sz::Small, p.muted);
            return;
        };
        let i = i as usize;
        let (aw, ah) = art_dims(ArtSize::Pane);
        let art = Rect::new(
            x,
            head.bottom() + 10,
            w.min(aw),
            ((w.min(aw)) * ah / aw).min(ah),
        );
        self.art(ui, i, art, ArtSize::Pane);
        ui.set_area(Rect::new(x, art.bottom() + 10, w, 0));
        let g = self.games[i].clone();
        let lines = ui.wrap(&g.name, Sz::Big, w);
        for l in lines.iter().take(3) {
            ui.strong(l, Sz::Big, p.foreground);
        }
        let mut kv: Vec<(&str, String)> = Vec::new();
        match (&g.rom, g.kind) {
            (Some(rom), _) => {
                kv.push((
                    "System",
                    rom.system
                        .map(|s| s.info().name.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                ));
                let file = std::path::Path::new(&rom.file);
                kv.push((
                    "File",
                    file.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                ));
                kv.push((
                    "Folder",
                    file.parent()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                ));
                kv.push(("Size", library::format_size(rom.size)));
                if !rom.player.is_empty() {
                    let label = if rom.alt_launch.is_empty() && rom.alt_label.is_empty() {
                        "Core"
                    } else {
                        "Emulator"
                    };
                    kv.push((label, rom.player.clone()));
                }
            }
            (None, Kind::Steam) => {
                kv.push(("Steam", g.detail.trim_start_matches("Steam · ").to_string()))
            }
            (None, _) => kv.push((
                "About",
                g.detail.trim_start_matches("Native · ").to_string(),
            )),
        }
        let played = self.library.played.get(&g.key).copied();
        kv.push((
            "Played",
            played
                .map(|p| format!("{} times", p.count))
                .unwrap_or_else(|| "never".into()),
        ));
        if let Some(pl) = played {
            kv.push(("Last", library::format_time(pl.last)));
        }
        let label_w = ui.text_width("Emulator ", Sz::Small);
        for (k, v) in kv {
            let y = ui.cursor_y();
            ui.paint_text(x, y, k, Sz::Small, p.muted, false);
            let v = ui.ellipsize(&v, Sz::Small, w - label_w);
            ui.paint_text(x + label_w, y, &v, Sz::Small, p.foreground, false);
            ui.space(ui.line_h(Sz::Small) + 3);
        }
        ui.space(4);
        if let Some(hint) = g
            .rom
            .as_ref()
            .map(|r| r.hint.clone())
            .filter(|h| !h.is_empty())
        {
            ui.label(&hint, Sz::Small, p.warning);
        }
        let can = !g.launch.is_empty();
        let mut act = None;
        ui.flow(|ui| {
            if ui.button_ex("Launch", Btn::default().strong().enabled(can)) {
                act = Some(0);
            }
            if let Some(rom) = g.rom.as_ref().filter(|r| !r.alt_launch.is_empty()) {
                if ui.button_ex(&rom.alt_label, Btn::default().small()) {
                    act = Some(1);
                }
            }
            let fav = if self.fav[i] {
                "Unfavourite"
            } else {
                "Favourite"
            };
            if ui.button_ex(fav, Btn::default().small()) {
                act = Some(2);
            }
        });
        match act {
            Some(0) => self.launch_current(false),
            Some(1) => self.launch_current(true),
            Some(2) => self.toggle_favourite(),
            _ => {}
        }
    }

    fn music_view(&mut self, ui: &mut Ui, area: Rect) {
        let p = ui.pal.clone();
        ui.set_area(area);
        if self.nirusid.is_some() {
            ui.label(
                "NIRUSID is the full SID player of the suite: open a track in it for playlists, songs and the details of each tune. Here you choose the background music.",
                Sz::Small,
                p.muted,
            );
        } else {
            ui.label(
                "NIRUGAME does not bundle SID music. Add your own PSID/RSID files to ~/.local/share/nirugame/music and install sidplayfp. NIRUSID, the full SID player of the suite, opens them too when it is installed.",
                Sz::Small,
                p.muted,
            );
        }
        let mut act = None;
        let playing = self.player.is_playing();
        let off = self.music.is_empty();
        ui.flow(|ui| {
            let has = !self.tracks.is_empty();
            if ui.button_ex("Play", Btn::default().strong().enabled(has)) {
                act = Some(0);
            }
            if self.nirusid.is_some()
                && ui.button_ex("Open in NIRUSID", Btn::default().enabled(has))
            {
                act = Some(1);
            }
            if playing && ui.button("Stop music") {
                act = Some(2);
            }
            if ui.button_ex(
                if off { "Off · active" } else { "Off" },
                Btn::default().selected(off),
            ) {
                act = Some(2);
            }
            if ui.button("Refresh tracks") {
                act = Some(3);
            }
            ui.label(
                &format!("{} files found", self.tracks.len()),
                Sz::Small,
                p.muted,
            );
        });
        match act {
            Some(0) => {
                if let Some(t) = self.tracks.get(self.music_sel).map(|t| t.path.clone()) {
                    self.select_track(Some(&t));
                }
            }
            Some(1) => self.open_nirusid(),
            Some(2) => self.select_track(None),
            Some(3) => self.refresh_music(),
            _ => {}
        }
        if self.tracks.is_empty() {
            ui.label("No SID files found.", Sz::Body, p.warning);
            return;
        }
        let top = ui.cursor_y();
        let view = Rect::new(area.x, top, area.w, (area.bottom() - top).max(40));
        let mut pick = None;
        let playing = self.player.is_playing();
        ui.scroll(MUSIC_SCROLL, view, |ui| {
            for (i, track) in self.tracks.iter().enumerate() {
                let active = self.music == track.path.to_string_lossy();
                let sel = i == self.music_sel;
                let fill = if sel { p.surface } else { p.background };
                let stroke = if sel { p.accent } else { p.border };
                let (_, r) = ui.frame(frame(fill, stroke), |ui| {
                    ui.flow(|ui| {
                        ui.strong(&track.title, Sz::Body, p.foreground);
                        if active && playing {
                            ui.label("playing", Sz::Small, p.success);
                        } else if active {
                            ui.label("selected", Sz::Small, p.accent);
                        }
                    });
                    ui.label(&track.path.display().to_string(), Sz::Tiny, p.muted);
                });
                if ui.click_area(&format!("track-{i}"), r) {
                    pick = Some(i);
                }
            }
        });
        if let Some(i) = pick {
            self.music_sel = i;
        }
    }

    fn footer(&self, ui: &mut Ui, pal: &Palette) {
        let hints = if self.view == View::Music {
            if self.nirusid.is_some() {
                "Up/Down track · Enter play · N open in NIRUSID · S stop · Tab view · Esc close"
            } else {
                "Up/Down track · Enter play · S stop · Tab view · Esc close"
            }
        } else {
            "Arrows move · Enter launch · / search · Tab view · F favourite · G grid · F5 rescan · Esc close"
        };
        ui.flow(|ui| {
            let w = ui.width() - 200;
            let h = ui.ellipsize(hints, Sz::Tiny, w.max(80));
            ui.label(&h, Sz::Tiny, pal.disabled);
            match self.current_track() {
                Some(track) => {
                    let t = format!("♪ {}", track.title);
                    ui.label(&t, Sz::Tiny, pal.accent)
                }
                None => ui.label("♪ Off", Sz::Tiny, pal.disabled),
            };
        });
    }

    fn move_by(&mut self, delta: isize, st: &mut State) -> bool {
        let count = self.filtered.len();
        if count == 0 {
            self.selected = 0;
            return false;
        }
        let new = (self.selected as isize + delta).clamp(0, count as isize - 1) as usize;
        self.selected = new;
        self.follow = true;
        // Scroll now, with the geometry of the last frame, so the next frame
        // is drawn once and already in the right place.
        if let Some((off, rh, h)) = self.geo {
            let top = (new / self.cols.max(1)) as i32 * rh;
            let d = if top < off {
                top - off
            } else if top + rh > off + h {
                top + rh - off - h
            } else {
                0
            };
            if d != 0 && st.scroll_by(SCROLL, d) {
                self.geo = Some((off + d, rh, h));
            }
        }
        true
    }
}

fn p_color(p: &Palette, s: System) -> u32 {
    let choices = [
        p.accent,
        p.success,
        p.warning,
        p.error,
        blend(p.accent, p.foreground, 0.45),
        blend(p.success, p.accent, 0.5),
    ];
    choices[s.index() % choices.len()]
}

impl App for NirugameApp {
    fn build(&mut self, ui: &mut Ui) {
        if self.dirty {
            self.refilter();
        }
        let pal = ui.pal.clone();
        let screen = ui.screen;
        let m = if screen.w < 480 { 10 } else { 16 };
        let area = Rect::new(m, m, (screen.w - 2 * m).max(40), (screen.h - 2 * m).max(40));
        ui.set_area(area);
        let (x, y) = (ui.left(), ui.cursor_y());
        let logo = logo::title(ui, "NIRUGAME", 4);
        let sub = "// retro game launcher";
        let sub_w = ui.text_width(sub, Sz::Small);
        let count = if self.scan.is_some() && self.games.is_empty() {
            "reading libraries".to_string()
        } else if self.scan.is_some() {
            format!("{} games · rescanning", self.games.len())
        } else {
            format!("{} games", self.games.len())
        };
        let info = format!("{count} · theme {}", ui.theme.name);
        if logo.w + 14 + sub_w <= area.w {
            ui.paint_text(
                x + logo.w + 14,
                y + logo.h - ui.line_h(Sz::Small) - 4,
                sub,
                Sz::Small,
                pal.muted,
                false,
            );
            let iw = ui.text_width(&info, Sz::Small);
            if logo.w + 14 + sub_w + 24 + iw <= area.w {
                ui.paint_text(
                    area.right() - iw,
                    y + logo.h - ui.line_h(Sz::Small) - 4,
                    &info,
                    Sz::Small,
                    pal.muted,
                    false,
                );
            } else {
                ui.label(&info, Sz::Small, pal.muted);
            }
        } else {
            ui.label(sub, Sz::Small, pal.muted);
            ui.label(&info, Sz::Small, pal.muted);
        }
        ui.space(4);
        self.tabs(ui);
        ui.separator();
        let mut act = None;
        if self.view != View::Music {
            ui.flow(|ui| {
                if ui.text_edit(SEARCH, &mut self.query, "Search all games  ( / )", 300) {
                    self.selected = 0;
                    self.dirty = true;
                }
                if ui.button_ex("Rescan", Btn::default().small()) {
                    act = Some(0);
                }
                if ui.button_ex(
                    if self.grid { "List" } else { "Grid" },
                    Btn::default().small(),
                ) {
                    act = Some(1);
                }
            });
        }
        match act {
            Some(0) => self.rescan(),
            Some(1) => {
                self.grid = !self.grid;
                self.geo = None;
                self.follow = true;
            }
            _ => {}
        }
        if self.dirty {
            self.refilter();
        }
        if let Some((message, error)) = &self.message {
            let color = if *error { pal.error } else { pal.success };
            let msg = ui.ellipsize(message, Sz::Small, area.w);
            ui.label(&msg, Sz::Small, color);
        }
        let footer_h = ui.line_h(Sz::Tiny) + 8;
        let top = ui.cursor_y();
        let body = Rect::new(
            area.x,
            top,
            area.w,
            (area.bottom() - footer_h - top).max(40),
        );
        if self.view == View::Music {
            let head = logo::pixel_heading(ui, body.x, body.y, "MUSIC", 2);
            let rest = Rect::new(
                body.x,
                head.bottom() + 8,
                body.w,
                (body.bottom() - head.bottom() - 8).max(20),
            );
            self.music_view(ui, rest);
        } else {
            let pane_w = if body.w >= 820 { 320 } else { 0 };
            let list_w = if pane_w > 0 {
                body.w - pane_w - 14
            } else {
                body.w
            };
            let n = self.filtered.len();
            let title = format!("{} · {}", self.view.heading(), n);
            let head = logo::pixel_heading(ui, body.x, body.y, &title, 2);
            let lv = Rect::new(
                body.x,
                head.bottom() + 6,
                list_w,
                (body.bottom() - head.bottom() - 6).max(20),
            );
            let narrow = pane_w == 0;
            ui.scroll(SCROLL, lv, |ui| self.games_view(ui, lv, narrow));
            if pane_w > 0 {
                self.details(ui, Rect::new(body.right() - pane_w, body.y, pane_w, body.h));
            }
        }
        ui.set_area(Rect::new(area.x, area.bottom() - footer_h + 4, area.w, 0));
        self.footer(ui, &pal);
        if self.timing && !self.games.is_empty() && self.scan.is_none() {
            self.timing = false;
            eprintln!(
                "nirugame: library shown after {:.1} ms ({} games, {})",
                self.started.elapsed().as_secs_f64() * 1000.0,
                self.games.len(),
                self.index_source
            );
        }
    }

    fn key(&mut self, k: &KeyEv, st: &mut State) -> bool {
        if k.is(keys::ESCAPE) {
            if st.typing() {
                st.blur();
            } else if !self.query.is_empty() && self.view != View::Music {
                self.query.clear();
                self.dirty = true;
            } else {
                st.quit = true;
            }
            return true;
        }
        if (k.ks == keys::TAB || k.ks == keys::ISO_LEFT_TAB) && !k.ctrl && !k.alt {
            st.blur();
            let next = self.next_view(k.shift || k.ks == keys::ISO_LEFT_TAB);
            self.set_view(st, next);
            self.message = None;
            return true;
        }
        if k.is(keys::F5) {
            if self.view == View::Music {
                self.refresh_music();
            } else {
                self.rescan();
            }
            return true;
        }
        if self.view != View::Music
            && ((k.ctrl && matches!(k.ch, Some('f') | Some('F')))
                || (!st.typing() && !k.ctrl && !k.alt && k.ch == Some('/')))
        {
            st.focus_field(SEARCH);
            return true;
        }
        if self.view == View::Music {
            let n = self.tracks.len();
            return match k.ks {
                keys::DOWN if n > 0 => {
                    self.music_sel = (self.music_sel + 1).min(n - 1);
                    true
                }
                keys::UP => {
                    self.music_sel = self.music_sel.saturating_sub(1);
                    true
                }
                _ if k.enter() => {
                    if let Some(t) = self.tracks.get(self.music_sel).map(|t| t.path.clone()) {
                        self.select_track(Some(&t));
                    }
                    true
                }
                _ if !k.ctrl && !k.alt && matches!(k.ch, Some('n') | Some('N')) => {
                    self.open_nirusid();
                    true
                }
                _ if !k.ctrl && !k.alt && matches!(k.ch, Some('s') | Some('S')) => {
                    self.select_track(None);
                    true
                }
                _ => false,
            };
        }
        let typing = st.typing();
        let cols = if self.grid {
            self.cols.max(1) as isize
        } else {
            1
        };
        match k.ks {
            keys::DOWN if !k.ctrl && !k.alt => self.move_by(cols, st),
            keys::UP if !k.ctrl && !k.alt => self.move_by(-cols, st),
            keys::RIGHT if self.grid && !typing && !k.ctrl => self.move_by(1, st),
            keys::LEFT if self.grid && !typing && !k.ctrl => self.move_by(-1, st),
            keys::PAGE_DOWN => self.move_by(self.page.max(1) as isize, st),
            keys::PAGE_UP => self.move_by(-(self.page.max(1) as isize), st),
            keys::HOME if !typing => self.move_by(-(self.filtered.len() as isize), st),
            keys::END if !typing => self.move_by(self.filtered.len() as isize, st),
            _ if k.enter() => {
                self.launch_current(k.shift);
                true
            }
            _ if !typing && !k.ctrl && !k.alt && matches!(k.ch, Some('f') | Some('F')) => {
                self.toggle_favourite();
                true
            }
            _ if !typing && !k.ctrl && !k.alt && matches!(k.ch, Some('g') | Some('G')) => {
                self.grid = !self.grid;
                self.geo = None;
                self.follow = true;
                true
            }
            _ => false,
        }
    }

    fn poll(&mut self) -> bool {
        let mut redraw = self.take_thumbs();
        let Some(rx) = &self.scan else {
            return redraw;
        };
        let mut msgs = Vec::new();
        let mut gone = false;
        loop {
            match rx.try_recv() {
                Ok(m) => msgs.push(m),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    gone = true;
                    break;
                }
            }
        }
        for m in msgs {
            match m {
                Msg::Stale(catalog) => {
                    if self.games.is_empty() {
                        self.apply(catalog);
                    }
                    redraw = true;
                }
                Msg::Done(catalog, tracks) => {
                    let rescan = !self.games.is_empty();
                    self.apply(catalog);
                    self.tracks = tracks;
                    self.music_sel = self.music_sel.min(self.tracks.len().saturating_sub(1));
                    self.scan = None;
                    if rescan
                        && self
                            .message
                            .as_ref()
                            .is_some_and(|m| m.0.starts_with("Rescanning"))
                    {
                        self.message =
                            Some((format!("Rescanned: {} games", self.games.len()), false));
                    }
                    return true;
                }
            }
        }
        if gone {
            self.scan = None;
            redraw = true;
        }
        redraw
    }

    fn quit(&mut self) {
        self.player.stop();
    }
}
